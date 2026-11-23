//! openlessons: validate, index and search a curriculum commons kept as files.
//!
//! - [`scan`]: one pass over a data tree that runs every gate and builds the index
//! - [`selftest`]: proves each gate fails its red fixture and the green fixture passes all of them
//! - [`find`]: answers a query from the generated index
//! - [`base`]: judges a change against the tree it starts from
//! - [`export`] / [`import`]: bundles, learner bundles and app packs out; packs and proposal files in
//! - [`mcp`]: a Model Context Protocol server over the tree (read-only unless told otherwise)
//! - [`observe`]: gate results and standards coverage as observation/1 records
//! - [`gaps`]: the gap report over objective x age band x language
//! - [`looprun`]: the content loop: work orders, landing drafts, the run report
//! - [`canon`]: canonical JSON and the ids derived from content

pub mod base;
pub mod canon;
pub mod export;
pub mod gaps;
pub mod import;
pub mod item;
pub mod looprun;
pub mod mcp;
pub mod observe;
pub mod scan;
pub mod text;

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub use scan::{scan, Finding, Scan, RULES};

/// The scan, plus the comparison with a base git revision when one is given.
pub fn judge(root: &Path, data: &Path, base_rev: Option<&str>) -> Result<Scan> {
    let mut s = scan(root, data)?;
    if let Some(rev) = base_rev {
        s.findings.extend(base::against_base(root, &base::GitBase::new(root, rev)?)?);
    }
    Ok(s)
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &dest)?;
        } else {
            fs::copy(e.path(), &dest)?;
        }
    }
    Ok(())
}

pub struct SelfTest {
    pub ok: bool,
    pub lines: Vec<String>,
}

/// Every gate must stay green on gates/fixtures/green and go red on its own overlay under gates/fixtures/red/<rule>/.
/// A gate that was never seen to fail is not known to work, so `validate` runs this first.
pub fn selftest(root: &Path) -> Result<SelfTest> {
    let gates = root.join("gates");
    let data = gates.join("data");
    let green = gates.join("fixtures").join("green");
    let mut lines = Vec::new();
    let mut ok = true;

    let listed: Value = serde_json::from_str(&fs::read_to_string(gates.join("rules.json")).context("gates/rules.json")?).context("gates/rules.json")?;
    let listed: BTreeSet<String> = listed["rules"].as_array().into_iter().flatten().filter_map(|r| r["id"].as_str()).map(str::to_string).collect();
    let coded: BTreeSet<String> = RULES.iter().map(|s| s.to_string()).collect();
    if listed != coded {
        ok = false;
        lines.push(format!("FAIL gates/rules.json lists {:?}; the program implements {:?}", listed.difference(&coded).collect::<Vec<_>>(), coded.difference(&listed).collect::<Vec<_>>()));
    }

    let mut g = scan(&green, &data)?;
    g.findings.extend(base::against_base(&green, &base::DirBase { base: green.clone(), head: green.clone() })?);
    if g.findings.is_empty() && g.counts.get("items").copied().unwrap_or(0) > 0 {
        lines.push(format!("ok   green fixture: {} gates, 0 findings", RULES.len()));
    } else {
        ok = false;
        lines.push(format!("FAIL green fixture: {} finding(s), {} item(s) read", g.findings.len(), g.counts.get("items").copied().unwrap_or(0)));
        lines.extend(g.findings.iter().take(10).map(|f| format!("       {} {} {}", f.rule, f.path, f.message)));
    }

    for rule in RULES {
        // a gate that compares two trees keeps the tree it starts from in fixtures/<rule>/base; the head is the green tree
        let paired = gates.join("fixtures").join(rule).join("base");
        let red = if paired.is_dir() { gates.join("fixtures").join("empty-overlay") } else { gates.join("fixtures").join("red").join(rule) };
        if !paired.is_dir() && !red.is_dir() {
            ok = false;
            lines.push(format!("FAIL {rule}: no red fixture at gates/fixtures/red/{rule}/"));
            continue;
        }
        let tmp = std::env::temp_dir().join(format!("openlessons-selftest-{}-{rule}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        // the green tree is the base every red overlay is a change to
        let base_tree = std::env::temp_dir().join(format!("openlessons-selftest-{}-{rule}-base", std::process::id()));
        let _ = fs::remove_dir_all(&base_tree);
        let result = copy_tree(&green, &tmp).and_then(|_| if red.is_dir() { copy_tree(&red, &tmp) } else { Ok(()) }).and_then(|_| scan(&tmp, &data)).and_then(|mut s| {
            let mut from = green.clone();
            if paired.is_dir() {
                copy_tree(&green, &base_tree)?;
                copy_tree(&paired, &base_tree)?;
                from = base_tree.clone();
            }
            s.findings.extend(base::against_base(&tmp, &base::DirBase { base: from, head: tmp.clone() })?);
            Ok(s)
        });
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_dir_all(&base_tree);
        let hits = result?.findings.iter().filter(|f| f.rule == *rule).count();
        if hits > 0 {
            lines.push(format!("ok   {rule}: red on its red fixture ({hits} finding(s))"));
        } else {
            ok = false;
            lines.push(format!("FAIL {rule}: its red fixture passes, so the gate does not work"));
        }
    }
    Ok(SelfTest { ok, lines })
}

/// Write the index a clean scan produced; files under index/ that the scan did not produce are removed.
pub fn write_index(root: &Path, scan: &Scan) -> Result<usize> {
    let dir = root.join("index");
    fn walk(d: &Path, out: &mut Vec<PathBuf>) {
        for e in fs::read_dir(d).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                walk(&e.path(), out);
            } else {
                out.push(e.path());
            }
        }
    }
    let mut existing = Vec::new();
    walk(&dir, &mut existing);
    for p in existing {
        let rel = p.strip_prefix(&dir).unwrap_or(&p).to_string_lossy().replace('\\', "/");
        if !scan.index.contains_key(&rel) {
            fs::remove_file(&p)?;
        }
    }
    for (rel, body) in &scan.index {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&p, body)?;
    }
    Ok(scan.index.len())
}

#[derive(Debug, Default, Clone)]
pub struct Query {
    pub locale: Option<String>,
    pub subject: Option<String>,
    pub skill: Option<String>,
    pub standard: Option<String>,
    pub framework: Option<String>,
    pub age: Option<i64>,
    pub status: Option<String>,
    pub text: Option<String>,
}

fn matches(q: &Query, l: &Value) -> bool {
    let s = |k: &str| l[k].as_str().unwrap_or("");
    let list = |k: &str| l[k].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>();
    let eq = |want: &Option<String>, got: &str| want.as_deref().is_none_or(|w| w == got);
    let prefix = |want: &Option<String>, k: &str| want.as_deref().is_none_or(|w| list(k).iter().any(|x| *x == w || x.starts_with(&format!("{w}.")) || x.starts_with(&format!("{w}:"))));
    eq(&q.locale, s("locale"))
        && eq(&q.subject, s("subject"))
        && eq(&q.status, s("status"))
        && prefix(&q.skill, "skills")
        && prefix(&q.standard, "standards")
        && q.framework.as_deref().is_none_or(|w| list("frameworks").contains(&w))
        && q.age.is_none_or(|a| l["ages"][0].as_i64().unwrap_or(0) <= a && a <= l["ages"][1].as_i64().unwrap_or(0))
        && q.text.as_deref().is_none_or(|w| {
            let hay = text::tokens(&format!("{} {} {}", s("title"), s("slug"), list("skills").join(" ")));
            text::tokens(w).iter().all(|t| hay.contains(t))
        })
}

/// Lessons matching the query, read from the generated index (the data files are not opened).
pub fn find(root: &Path, q: &Query) -> Result<Vec<Value>> {
    let dir = root.join("index");
    if !dir.join("catalog.json").exists() {
        bail!("no index at {}: run `openlessons index` in a clone of the repository", dir.display());
    }
    let mut out = Vec::new();
    let mut locales: Vec<PathBuf> = fs::read_dir(&dir)?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir()).collect();
    locales.sort();
    for ldir in locales {
        if q.locale.as_deref().is_some_and(|l| ldir.file_name().is_some_and(|n| n.to_string_lossy() != l)) {
            continue;
        }
        let mut files: Vec<PathBuf> = fs::read_dir(&ldir)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
        files.sort();
        for f in files {
            if q.subject.as_deref().is_some_and(|s| f.file_stem().is_some_and(|n| n.to_string_lossy() != s)) {
                continue;
            }
            for line in fs::read_to_string(&f)?.lines().filter(|l| !l.trim().is_empty()) {
                let v: Value = serde_json::from_str(line).with_context(|| format!("{}", f.display()))?;
                if matches(q, &v) {
                    out.push(v);
                }
            }
        }
    }
    Ok(out)
}

/// Give every item in an items.jsonl file the id of its content and sort the file by id. Returns (items, ids changed).
pub fn stamp_file(p: &Path) -> Result<(usize, usize)> {
    let raw = fs::read_to_string(p).with_context(|| format!("{}", p.display()))?;
    let mut items: Vec<(String, Value)> = Vec::new();
    let mut changed = 0;
    for (i, line) in raw.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let mut v: Value = serde_json::from_str(line).with_context(|| format!("{}:{}", p.display(), i + 1))?;
        let id = canon::item_id(&v).with_context(|| format!("{}:{}", p.display(), i + 1))?;
        let Some(o) = v.as_object_mut() else { bail!("{}:{}: not an object", p.display(), i + 1) };
        if o.get("id").and_then(Value::as_str) != Some(&id) {
            changed += 1;
            o.insert("id".to_string(), Value::String(id.clone()));
        }
        items.push((id, v));
    }
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let body: String = items.iter().map(|(_, v)| format!("{v}\n")).collect();
    if body != raw {
        fs::write(p, body)?;
    }
    Ok((items.len(), changed))
}

/// Stamp one items.jsonl, or every items.jsonl under a folder.
pub fn stamp(p: &Path) -> Result<(usize, usize, usize)> {
    let (mut files, mut items, mut changed) = (0, 0, 0);
    if p.is_dir() {
        let mut entries: Vec<PathBuf> = fs::read_dir(p)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for e in entries {
            if e.is_dir() || e.file_name().is_some_and(|n| n == "items.jsonl") {
                let (f, i, c) = stamp(&e)?;
                files += f;
                items += i;
                changed += c;
            }
        }
    } else {
        let (i, c) = stamp_file(p)?;
        return Ok((1, i, c));
    }
    Ok((files, items, changed))
}
