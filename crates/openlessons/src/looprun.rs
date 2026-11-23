//! The content loop: gap report -> pick -> draft -> gates -> land as `draft` -> a run report on a branch or a pull request.
//!
//! The loop is agent-agnostic. `orders` turns the top gaps into work orders (`order/1`): everything a harness needs to
//! draft one lesson, and nothing it may not copy. Any harness answers an order with a proposal file (`proposal/1`).
//! `land` judges every proposal with every gate and places the ones that pass at status `draft`. Nothing here can set
//! `reviewed`: that takes a decision of the jury (`openlessons jury`), and a product for children reads `reviewed` content only.
//!
//! Drafting needs a model, so it runs where a person or an approved runner holds a key. Everything else in this file,
//! and the gap report, needs neither a model nor a secret.

use crate::canon::sha256_hex;
use crate::export::{arr, st};
use crate::gaps::{self, Cell};
use crate::import::{from_proposal_as, write_records};
use crate::scan::{scan, Scan, RULES};
use crate::write_index;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Today's date (UTC) as YYYY-MM-DD, from the system clock.
pub fn today() -> String {
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0) as i64;
    // civil-from-days (H. Hinnant): days since 1970-01-01 -> year, month, day
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    format!("{:04}-{m:02}-{d:02}", yoe + era * 400 + i64::from(m <= 2))
}

const RULES_FOR_A_DRAFT: &[&str] = &[
    "Answer with one proposal/1 object (schemas/proposal.schema.json): `provenance`, then `lessons` with one lesson and its items. No `skills`, no `alignments`: the loop drafts lessons for objectives that exist.",
    "Say what you are: provenance.origin `ai`, provenance.maker (your agent name), provenance.model, provenance.created. Never write a status or a review: every record lands as `draft`.",
    "Use `lesson.slug`, `locale`, `subject`, `ages` and `skills` exactly as this order gives them.",
    "Write in the order's language, for a learner of the order's ages, in your own words. Do not translate another lesson word for word.",
    "Never copy text from a textbook, a paid course, an exam paper, a word list or a standard. A standard's wording appears in this order only where its terms allow it; paraphrase everything else.",
    "No names of real people, no contact details, no links, no brand names. Nothing that frightens, shames or sells.",
    "Every graded item has one defensible answer, and its prompt does not give that answer away. Hints give a step, never the answer.",
    "Set key.verification.method: `computed` when a program worked the answer out (never guess arithmetic), `constructed` when the item was built from its answer.",
    "A `check` item has at least three choices, in two forms (`a` and `b`). Start `easy`; include `guided`, `practice` and `check` roles.",
    "Address the objective's misconceptions: a wrong choice a learner with that misconception would pick goes in key.distractors with its `misconception` number and kind feedback.",
];

/// One work order per cell: what to draft, for whom, against which standards, inside which bounds.
pub fn orders(data: &Path, scan: &Scan, cells: &[Cell], min_items: u64) -> Result<Vec<Value>> {
    let reading: Value = serde_json::from_str(&fs::read_to_string(data.join("reading-levels.json")).context("gates/data/reading-levels.json")?)?;
    let mut out = Vec::new();
    for c in cells {
        let obj = scan.objectives.get(&c.objective).ok_or_else(|| anyhow!("no objective {}", c.objective))?;
        let band = scan.bands.iter().find(|b| b.id == c.band).ok_or_else(|| anyhow!("no band {}", c.band))?;
        // the adult band has no upper school age: a lesson record still needs one
        let ages = json!({"min": band.min, "max": band.max.min(99)});
        let limits = arr(&reading, "bands").iter().find(|b| band.min <= b["maxAge"].as_i64().unwrap_or(i64::MAX)).cloned().unwrap_or(Value::Null);
        let mut standards = Vec::new();
        for a in scan.alignments.iter().filter(|a| a.skill == c.objective) {
            let Some(node) = scan.nodes.get(&a.standard) else { continue };
            let framework = st(node, "framework");
            let mut s = json!({"id": a.standard, "strength": a.strength, "code": node["code"], "label": node["label"], "sourceUrl": node["sourceUrl"]});
            if let Some(own) = node.get("statementOwn") {
                s["statementOwn"] = own.clone();
            }
            // the publisher's wording travels only where the framework's terms allow copying it
            if scan.frameworks.get(framework).map(String::as_str) == Some("official_text") {
                if let Some(official) = node.get("statementOfficial") {
                    s["statementOfficial"] = official.clone();
                }
            }
            standards.push(s);
        }
        let mut o = obj.record.as_object().cloned().unwrap_or_default();
        for k in ["schema", "provenance", "status"] {
            o.remove(k);
        }
        let others: Vec<Value> = scan
            .lessons
            .iter()
            .filter(|l| arr(l, "skills").iter().any(|s| s.as_str() == Some(c.objective.as_str())))
            .map(|l| json!({"id": l["id"], "locale": l["locale"], "ages": l["ages"], "items": l["items"], "status": l["status"], "path": l["path"]}))
            .collect();
        let slug = format!("{}-{}", st(&obj.record, "slug"), c.band);
        out.push(json!({
            "schema": "order/1", "cell": c.id(), "gap": c.kind, "demand": c.demand,
            "objective": o,
            "band": {"id": band.id, "ages": ages},
            "locale": scan.locales.get(&c.locale).cloned().unwrap_or(Value::Null),
            "lesson": {"slug": slug, "locale": c.locale, "subject": c.subject, "ages": ages, "skills": [c.objective], "minItems": min_items},
            "standards": standards,
            "reading": limits,
            "existing": others,
            "rules": RULES_FOR_A_DRAFT,
            "shapes": "Copy the shape of an existing lesson and its items (see `existing`, or any folder under lessons/): schemas/lesson.schema.json and schemas/item.schema.json are the contract.",
        }));
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    /// write the files and the report into the working tree; touch no branch
    Files,
    /// commit the run on a new local branch `loop/<run id>`; push nothing
    DryRun,
    /// commit, push the branch and open a pull request
    PullRequest,
}

pub struct Land<'a> {
    pub root: &'a Path,
    pub data: &'a Path,
    /// a folder of proposal files (*.json); `orders.json` beside them, when present, is what they must answer
    pub from: &'a Path,
    pub mode: Mode,
    pub date: String,
    pub gaps: gaps::Options,
    /// drafts that never became a proposal file (a drafter that failed), to be reported as refused
    pub failed: Vec<(String, String)>,
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").arg("-C").arg(root).args(args).output().context("running git")?;
    if !out.status.success() {
        bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn blocking(s: &Scan) -> Vec<Value> {
    s.findings.iter().filter(|f| f.rule != "index.fresh").map(|f| json!({"rule": f.rule, "path": f.path, "line": f.line, "message": f.message})).collect()
}

/// A lesson folder as it was, so a refused draft leaves no trace.
struct Backup {
    dir: PathBuf,
    files: Option<Vec<(String, Vec<u8>)>>,
}
impl Backup {
    fn take(dir: PathBuf) -> Backup {
        let files = dir.is_dir().then(|| fs::read_dir(&dir).into_iter().flatten().flatten().filter_map(|e| Some((e.file_name().to_string_lossy().to_string(), fs::read(e.path()).ok()?))).collect());
        Backup { dir, files }
    }
    fn restore(&self) {
        let _ = fs::remove_dir_all(&self.dir);
        if let Some(files) = &self.files {
            let _ = fs::create_dir_all(&self.dir);
            for (name, bytes) in files {
                let _ = fs::write(self.dir.join(name), bytes);
            }
        } else if let Some(shard) = self.dir.parent() {
            // an empty shard folder is left behind by nothing
            let _ = fs::remove_dir(shard);
        }
    }
}

/// Why a proposal may not come in through the loop, before any gate reads it.
fn loop_problem(records: &crate::import::Records, scan: &Scan, wanted: &BTreeMap<String, Value>) -> Option<String> {
    if !records.skills.is_empty() || !records.alignments.is_empty() || !records.paths.is_empty() {
        return Some("the loop drafts lessons and items for objectives that exist: a new skill or alignment changes the topic graph and comes as its own pull request".to_string());
    }
    for (l, items) in &records.lessons {
        if st(&l["provenance"], "origin") == "ai" && st(&l["provenance"], "model").is_empty() {
            return Some("an AI draft names its model (provenance.model)".to_string());
        }
        if items.is_empty() {
            return Some(format!("the lesson {} has no items", st(l, "slug")));
        }
        for s in arr(l, "skills").iter().filter_map(Value::as_str) {
            if !scan.objectives.contains_key(s) {
                return Some(format!("the objective {s} is not in the topic graph"));
            }
        }
        if !wanted.is_empty() {
            let answers = wanted.values().any(|o| {
                let w = &o["lesson"];
                st(w, "locale") == st(l, "locale") && st(w, "subject") == st(l, "subject") && st(w, "slug") == st(l, "slug") && w["skills"] == l["skills"] && w["ages"] == l["ages"]
            });
            if !answers {
                return Some(format!("the lesson {} ({}) answers no work order of this run: slug, locale, subject, ages and skills come from the order", st(l, "slug"), st(l, "locale")));
            }
        }
    }
    None
}

/// Judge every proposal in `from` and place the ones that pass, as drafts. Returns the run report (`run/1`).
pub fn land(o: &Land) -> Result<Value> {
    let (root, data) = (o.root, o.data);
    let in_git = o.mode != Mode::Files;
    let mut branch = String::new();
    if in_git {
        branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
        if !git(root, &["status", "--porcelain"])?.is_empty() {
            bail!("the working tree has uncommitted changes: a loop run starts from a clean tree (keep work orders and drafts outside it, or under .loop/)");
        }
    }
    let before = scan(root, data)?;
    if !blocking(&before).is_empty() {
        bail!("the tree is red before the run ({} finding(s)): run `openlessons validate` and fix it first", blocking(&before).len());
    }
    let gaps_before = gaps::report(&before, &o.gaps);
    let count = |s: &Scan, k: &str| s.counts.get(k).copied().unwrap_or(0);
    let (lessons_before, items_before) = (count(&before, "lessons"), count(&before, "items"));

    let mut wanted: BTreeMap<String, Value> = BTreeMap::new();
    if let Ok(raw) = fs::read_to_string(o.from.join("orders.json")) {
        for order in serde_json::from_str::<Vec<Value>>(&raw).context("orders.json")? {
            wanted.insert(st(&order, "cell").to_string(), order);
        }
    }
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in [o.from.to_path_buf(), o.from.join("drafts")] {
        files.extend(fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json") && p.file_name().is_some_and(|n| n != "orders.json")));
    }
    files.sort();
    if files.is_empty() && o.failed.is_empty() {
        bail!("no proposal files (*.json) in {} or its drafts/ folder", o.from.display());
    }

    let (mut landed, mut refused, mut makers) = (Vec::new(), Vec::new(), BTreeSet::new());
    for (name, why) in &o.failed {
        refused.push(json!({"draft": name, "why": [{"rule": "drafter", "message": why}]}));
    }
    let mut current = before;
    for f in &files {
        let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let mut refuse = |why: Vec<Value>| refused.push(json!({"draft": name, "why": why}));
        let say = |m: String| vec![json!({"rule": "loop", "message": m})];
        let p: Value = match fs::read_to_string(f).map_err(anyhow::Error::from).and_then(|raw| Ok(serde_json::from_str(&raw)?)) {
            Ok(p) => p,
            Err(e) => {
                refuse(say(format!("not a JSON file: {e}")));
                continue;
            }
        };
        let records = match from_proposal_as(&p, "draft") {
            Ok(r) => r,
            Err(e) => {
                refuse(say(format!("{e:#}")));
                continue;
            }
        };
        if let Some(m) = loop_problem(&records, &current, &wanted) {
            refuse(say(m));
            continue;
        }
        let backups: Vec<Backup> = records.lessons.iter().map(|(l, _)| Backup::take(root.join(format!("lessons/{}/{}/{}/{}", st(l, "locale"), st(l, "subject"), st(l, "id").get(4..6).unwrap_or(""), st(l, "id"))))).collect();
        let undo = || backups.iter().for_each(Backup::restore);
        if let Err(e) = write_records(root, &records) {
            undo();
            refuse(say(format!("{e:#}")));
            continue;
        }
        let after = scan(root, data)?;
        let findings = blocking(&after);
        if !findings.is_empty() {
            undo();
            let n = findings.len();
            let mut why: Vec<Value> = findings.into_iter().take(12).collect();
            if n > 12 {
                why.push(json!({"rule": "loop", "message": format!("... and {} more finding(s)", n - 12)}));
            }
            refuse(why);
            continue;
        }
        current = after;
        makers.insert(format!("{} ({})", st(&p["provenance"], "maker"), if st(&p["provenance"], "model").is_empty() { st(&p["provenance"], "origin") } else { st(&p["provenance"], "model") }));
        for (l, items) in &records.lessons {
            landed.push(json!({
                "draft": name, "lesson": l["id"], "slug": l["slug"], "locale": l["locale"], "subject": l["subject"], "objectives": l["skills"], "ages": l["ages"], "items": items.len(),
                "path": format!("lessons/{}/{}/{}/{}", st(l, "locale"), st(l, "subject"), st(l, "id").get(4..6).unwrap_or(""), st(l, "id")),
            }));
        }
    }

    write_index(root, &current)?;
    let mut after = scan(root, data)?;
    let gaps_after = gaps::report(&after, &o.gaps);
    let open: BTreeSet<String> = gaps_after.gaps.iter().map(Cell::id).collect();
    let closed: Vec<String> = gaps_before.gaps.iter().map(Cell::id).filter(|c| !open.contains(c)).collect();
    let seed: String = landed.iter().map(|l| st(l, "lesson")).chain(refused.iter().map(|r| st(r, "draft"))).collect::<Vec<_>>().join(",");
    let id = format!("{}-{}", o.date, &sha256_hex(seed.as_bytes())[..8]);
    let subjects: BTreeSet<&str> = landed.iter().map(|l| st(l, "subject")).collect();
    let locales: BTreeSet<&str> = landed.iter().map(|l| st(l, "locale")).collect();
    let objectives: BTreeSet<&str> = landed.iter().flat_map(|l| arr(l, "objectives")).filter_map(Value::as_str).collect();
    let mut report = json!({
        "schema": "run/1", "id": id, "date": o.date,
        "mode": match o.mode { Mode::Files => "files", Mode::DryRun => "dry-run", Mode::PullRequest => "pull-request" },
        "makers": makers, "orders": wanted.len(), "drafts": files.len() + o.failed.len(),
        "status": "draft",
        "note": "Every record of this run is `draft`: drafted by a model, passed by the gates, reviewed by nobody. Only a unanimous, sourced decision of the jury in REVIEWERS.json can make it `reviewed`, and a product for children reads `reviewed` content only.",
        "landed": landed, "refused": refused,
        "spread": {"objectives": objectives.len(), "subjects": subjects, "locales": locales},
        "gaps": {"cells": gaps_after.cells, "before": gaps_before.gaps.len(), "after": gaps_after.gaps.len(), "closed": closed},
        "lessons": {"before": lessons_before, "after": count(&after, "lessons"), "added": count(&after, "lessons") - lessons_before},
        "items": {"before": items_before, "after": count(&after, "items"), "added": count(&after, "items") - items_before},
        "gates": {},
    });

    // the report is part of the tree, so it is written before the last judgement and judged with everything else
    let runs = root.join("runs");
    fs::create_dir_all(&runs)?;
    let write_report = |report: &Value| -> Result<()> {
        fs::write(runs.join(format!("{id}.json")), format!("{}\n", serde_json::to_string_pretty(report)?))?;
        fs::write(runs.join(format!("{id}.md")), markdown(report))?;
        Ok(())
    };
    write_report(&report)?;
    after = scan(root, data)?;
    write_index(root, &after)?;
    after = scan(root, data)?;
    if in_git {
        after.findings.extend(crate::base::against_base(root, &crate::base::GitBase::new(root, "HEAD")?)?);
    }
    let rules: serde_json::Map<String, Value> = RULES.iter().map(|r| (r.to_string(), json!(after.findings.iter().filter(|f| f.rule == *r).count()))).collect();
    let red = rules.values().filter(|n| n.as_u64() != Some(0)).count();
    report["gates"] = json!({
        "count": RULES.len(), "green": RULES.len() - red, "red": red, "findings": after.findings.len(), "rules": rules,
        "base": if in_git { "judged against HEAD: nothing reviewed changed" } else { "loop.base-respected not judged (no git in this mode)" },
    });
    write_report(&report)?;
    if red > 0 {
        bail!("the tree is red after the run ({} finding(s)): the files are left in place for you to read; nothing was committed", after.findings.len());
    }

    if in_git {
        let name = format!("loop/{id}");
        let n_landed = arr(&report, "landed").len();
        let n_closed = arr(&report["gaps"], "closed").len();
        git(root, &["switch", "-c", &name])?;
        let done = (|| -> Result<Option<String>> {
            git(root, &["add", "--", "lessons", "index", "runs"])?;
            let message = format!(
                "feat(loop): run {id} drafts {n_landed} lesson(s), {} item(s), closes {n_closed} gap(s)\n\nEvery record is `draft`: no educator has read it. See runs/{id}.md.",
                report["items"]["added"]
            );
            git(root, &["commit", "-q", "-m", &message])?;
            if o.mode != Mode::PullRequest {
                return Ok(None);
            }
            git(root, &["push", "-u", "origin", &name])?;
            let title = format!("loop {id}: {n_landed} draft lesson(s), {n_closed} gap(s) closed");
            let pr = Command::new("gh").current_dir(root).args(["pr", "create", "--base", &branch, "--head", &name, "--title", &title, "--body-file", &format!("runs/{id}.md")]).output();
            Ok(Some(match pr {
                Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
                Ok(out) => format!("not opened: {}", String::from_utf8_lossy(&out.stderr).trim()),
                Err(_) => "not opened: the `gh` command is not installed; the branch is pushed, open the pull request by hand".to_string(),
            }))
        })();
        // whatever happened, the caller gets its branch back
        git(root, &["switch", &branch])?;
        if let Some(pr) = done? {
            report["pullRequest"] = json!(pr);
        }
        report["branch"] = json!(name);
    }
    Ok(report)
}

/// The run report as Markdown: the body of the pull request.
pub fn markdown(r: &Value) -> String {
    let n = |v: &Value| v.as_u64().unwrap_or(0);
    let list = |v: &Value| v.as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(", ");
    let mut s = format!("# Loop run {}\n\n", st(r, "id"));
    s.push_str(&format!(
        "- **Landed:** {} lesson(s) with {} item(s), for {} objective(s) in {} subject(s) ({}) and {} language(s) ({}).\n",
        arr(r, "landed").len(),
        n(&r["items"]["added"]),
        n(&r["spread"]["objectives"]),
        arr(&r["spread"], "subjects").len(),
        list(&r["spread"]["subjects"]),
        arr(&r["spread"], "locales").len(),
        list(&r["spread"]["locales"])
    ));
    s.push_str(&format!("- **Gaps:** {} before, {} after: {} closed (of {} cells).\n", n(&r["gaps"]["before"]), n(&r["gaps"]["after"]), arr(&r["gaps"], "closed").len(), n(&r["gaps"]["cells"])));
    s.push_str(&format!("- **Gates:** {} of {} green over the whole tree ({} finding(s)); {}.\n", n(&r["gates"]["green"]), n(&r["gates"]["count"]), n(&r["gates"]["findings"]), st(&r["gates"], "base")));
    s.push_str(&format!("- **Refused:** {} of {} draft(s).\n", arr(r, "refused").len(), n(&r["drafts"])));
    s.push_str(&format!("- **Made by:** {}. Mode: {}.\n\n", list(&r["makers"]), st(r, "mode")));
    s.push_str(&format!("> {}\n\n", st(r, "note")));
    s.push_str("## Landed\n\n| lesson | language | objective | ages | items |\n|---|---|---|---|---:|\n");
    for l in arr(r, "landed") {
        s.push_str(&format!("| `{}` | {} | {} | {}-{} | {} |\n", st(l, "slug"), st(l, "locale"), list(&l["objectives"]), l["ages"]["min"], l["ages"]["max"], l["items"]));
    }
    if !arr(r, "refused").is_empty() {
        s.push_str("\n## Refused\n\n");
        for d in arr(r, "refused") {
            s.push_str(&format!("- `{}`\n", st(d, "draft")));
            for w in arr(d, "why") {
                s.push_str(&format!("  - {}: {}\n", st(w, "rule"), st(w, "message")));
            }
        }
    }
    s.push_str("\n## Gaps closed\n\n");
    for c in arr(&r["gaps"], "closed") {
        s.push_str(&format!("- `{}`\n", c.as_str().unwrap_or("")));
    }
    s
}

/// Hand each order to a drafter command (the order on standard input, a proposal on standard output) and save what
/// comes back under `<dir>/drafts/`. Returns the orders the drafter failed on, with the reason.
pub fn draft_with(dir: &Path, orders: &[Value], drafter: &str) -> Result<Vec<(String, String)>> {
    let mut parts = drafter.split_whitespace();
    let program = parts.next().ok_or_else(|| anyhow!("--drafter names a command"))?;
    let args: Vec<&str> = parts.collect();
    let drafts = dir.join("drafts");
    fs::create_dir_all(&drafts)?;
    let mut failed = Vec::new();
    for (i, order) in orders.iter().enumerate() {
        let name = format!("{:03}-{}.json", i + 1, st(order, "cell").replace(['@', '.'], "_"));
        let result = (|| -> Result<Value> {
            let mut child = Command::new(program).args(&args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().with_context(|| format!("starting the drafter `{program}`"))?;
            // a drafter that never reads its input must not hang the run
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(format!("{order}\n").as_bytes());
            }
            let out = child.wait_with_output()?;
            if !out.status.success() {
                bail!("the drafter exited with {}", out.status);
            }
            serde_json::from_slice(&out.stdout).context("the drafter's output is not one JSON object")
        })();
        match result {
            Ok(p) => fs::write(drafts.join(&name), format!("{}\n", serde_json::to_string_pretty(&p)?))?,
            Err(e) => failed.push((name, format!("{e:#}"))),
        }
    }
    Ok(failed)
}
