//! The gate `loop.base-respected`: a change is judged against the tree it starts from.
//!
//! A record that is `reviewed` or `published` on the base may only:
//! - stay as it is;
//! - move forward in status, with reviews appended and nothing else touched;
//! - be reopened: its status returns to `draft` or `proposed`, and a skill, lesson or path also raises its counter
//!   (`version` or `revision`). A reopened record is visibly unreviewed again.
//!
//! A reviewed item cannot be edited in place at all (its id is the hash of its content): it is revised by a new item that
//! names it in `supersedes`, and only then may it leave the file. Everything unreviewed is free to change.

use crate::canon::canonical;
use crate::scan::Finding;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const RULE: &str = "loop.base-respected";
/// The folders whose records carry a status.
const GOVERNED: &[&str] = &["lessons", "skills", "alignments", "paths", "assets"];

/// Where the base tree comes from.
pub trait Base {
    /// Governed files that differ between the base and the working tree (relative, forward slashes), deleted ones included.
    fn changed(&self) -> Result<Vec<String>>;
    /// A file as it is on the base; None when the base does not have it.
    fn read(&self, path: &str) -> Result<Option<String>>;
}

/// A git revision of the repository that holds the tree (`origin/main`, a commit id).
pub struct GitBase {
    pub root: PathBuf,
    pub rev: String,
}

impl GitBase {
    fn git(&self, args: &[&str]) -> Result<std::process::Output> {
        Command::new("git").arg("-C").arg(&self.root).args(args).output().context("running git (needed for --base)")
    }
    pub fn new(root: &Path, rev: &str) -> Result<GitBase> {
        // a revision is a name, never an option
        if rev.starts_with('-') || rev.is_empty() {
            bail!("--base takes a git revision");
        }
        let b = GitBase { root: root.to_path_buf(), rev: rev.to_string() };
        let ok = b.git(&["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")])?;
        if !ok.status.success() {
            bail!("the base revision {rev} is not a commit in this repository");
        }
        Ok(b)
    }
}

impl Base for GitBase {
    fn changed(&self) -> Result<Vec<String>> {
        let mut args = vec!["diff", "--name-only", "--no-renames", "--relative", "-z", self.rev.as_str(), "--"];
        args.extend(GOVERNED);
        let out = self.git(&args)?;
        if !out.status.success() {
            bail!("git diff failed: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).split('\0').filter(|s| !s.is_empty()).map(str::to_string).collect())
    }
    fn read(&self, path: &str) -> Result<Option<String>> {
        let out = self.git(&["show", &format!("{}:./{path}", self.rev)])?;
        Ok(out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string()))
    }
}

/// Another folder holding the base tree (used by the self-test and the tests).
pub struct DirBase {
    pub base: PathBuf,
    pub head: PathBuf,
}

fn files(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            files(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            out.insert(rel, fs::read_to_string(&p).unwrap_or_default().replace("\r\n", "\n"));
        }
    }
}

impl Base for DirBase {
    fn changed(&self) -> Result<Vec<String>> {
        let (mut a, mut b) = (BTreeMap::new(), BTreeMap::new());
        for d in GOVERNED {
            files(&self.base, &self.base.join(d), &mut a);
            files(&self.head, &self.head.join(d), &mut b);
        }
        let mut out: Vec<String> = a.iter().filter(|(k, v)| b.get(*k) != Some(v)).map(|(k, _)| k.clone()).collect();
        out.extend(b.keys().filter(|k| !a.contains_key(*k)).cloned());
        Ok(out)
    }
    fn read(&self, path: &str) -> Result<Option<String>> {
        let p = self.base.join(path);
        Ok(p.is_file().then(|| fs::read_to_string(&p).unwrap_or_default()))
    }
}

fn st<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn rank(status: &str) -> u8 {
    match status {
        "draft" => 0,
        "proposed" => 1,
        "reviewed" => 2,
        "published" => 3,
        _ => 0,
    }
}
fn settled(v: &Value) -> bool {
    rank(st(v, "status")) >= 2
}
fn reviews(v: &Value) -> Vec<Value> {
    v["provenance"]["reviews"].as_array().cloned().unwrap_or_default()
}
/// The record without the two things a forward move may change.
fn frozen(v: &Value) -> String {
    let mut c = v.clone();
    if let Some(o) = c.as_object_mut() {
        o.remove("status");
        if let Some(p) = o.get_mut("provenance").and_then(Value::as_object_mut) {
            p.remove("reviews");
        }
    }
    canonical(&c).unwrap_or_else(|_| c.to_string())
}
/// The counter a record must raise when it is reopened (0 = this kind has none).
fn counter(v: &Value) -> u64 {
    match st(v, "schema") {
        "skill/1" => v["version"].as_u64().unwrap_or(1),
        "lesson/1" | "path/1" => v["revision"].as_u64().unwrap_or(1),
        _ => 0,
    }
}

/// Why `head` is not a legal successor of the settled record `base`; None when it is.
fn illegal(base: &Value, head: &Value) -> Option<String> {
    let (was, now) = (st(base, "status"), st(head, "status"));
    if rank(now) < 2 {
        if counter(base) > 0 && counter(head) <= counter(base) {
            let name = if st(base, "schema") == "skill/1" { "version" } else { "revision" };
            return Some(format!("was `{was}` on the base and is now `{now}`: reopening it needs a higher `{name}`"));
        }
        return None;
    }
    if rank(now) < rank(was) {
        return Some(format!("moves back from `{was}` to `{now}`"));
    }
    if frozen(base) != frozen(head) {
        return Some(format!("was `{was}` on the base and its content changed: reopen it (status `proposed`, a higher version) or add a new record"));
    }
    let (old, new) = (reviews(base), reviews(head));
    if new.len() < old.len() || new[..old.len()] != old[..] {
        return Some("reviews recorded on the base were removed or rewritten: reviews are only appended".to_string());
    }
    None
}

/// The records of a file by key, with the line each sits on (0 for a .json file).
fn records(path: &str, raw: &str) -> BTreeMap<String, (usize, Value)> {
    let mut out = BTreeMap::new();
    if path.ends_with(".jsonl") {
        for (i, line) in raw.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                let key = if st(&v, "schema") == "alignment/1" { format!("{} -> {}", st(&v, "skill"), st(&v, "standard")) } else { st(&v, "id").to_string() };
                out.insert(key, (i + 1, v));
            }
        }
    } else if path.ends_with(".json") {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            out.insert(String::new(), (0, v));
        }
    }
    out
}

/// Judge the working tree at `root` against a base.
pub fn against_base(root: &Path, base: &dyn Base) -> Result<Vec<Finding>> {
    let mut out = Vec::new();
    for path in base.changed()? {
        let Some(before) = base.read(&path)? else { continue };
        let before = records(&path, &before);
        if !before.values().any(|(_, v)| settled(v)) {
            continue;
        }
        let after = records(&path, &fs::read_to_string(root.join(&path)).unwrap_or_default());
        for (key, (_, old)) in before.iter().filter(|(_, (_, v))| settled(v)) {
            let what = if key.is_empty() { "this record".to_string() } else { key.clone() };
            let was = st(old, "status");
            match after.get(key) {
                Some((line, new)) => {
                    if let Some(why) = illegal(old, new) {
                        out.push(Finding { rule: RULE.to_string(), path: path.clone(), line: *line, message: format!("{what} {why}") });
                    }
                }
                None => {
                    let superseded = st(old, "schema") == "item/1" && after.values().any(|(_, v)| st(v, "supersedes") == key);
                    if !superseded {
                        let how = if st(old, "schema") == "item/1" { "revise it with a new item that names it in `supersedes`" } else { "reopen it first (status `proposed`)" };
                        out.push(Finding { rule: RULE.to_string(), path: path.clone(), line: 0, message: format!("{what} was `{was}` on the base and is gone: {how}") });
                    }
                }
            }
        }
    }
    out.extend(ids_stable(root, base)?);
    Ok(out)
}

pub const IDS_RULE: &str = "graph.ids-stable";

/// The gate `graph.ids-stable`: an objective (skill) id present on the base never vanishes. It may be marked
/// `deprecated: {"replacedBy": "<id>"}`, and the replacement must exist; the record itself stays.
fn ids_stable(root: &Path, base: &dyn Base) -> Result<Vec<Finding>> {
    let mut out = Vec::new();
    let mut head = BTreeMap::new();
    files(root, &root.join("skills"), &mut head);
    let mut live: BTreeMap<String, (String, Value)> = BTreeMap::new();
    for (path, raw) in &head {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            live.insert(st(&v, "id").to_string(), (path.clone(), v));
        }
    }
    for path in base.changed()?.into_iter().filter(|p| p.starts_with("skills/")) {
        let Some(before) = base.read(&path)? else { continue };
        let Ok(old) = serde_json::from_str::<Value>(&before) else { continue };
        let id = st(&old, "id");
        if !id.is_empty() && !live.contains_key(id) {
            out.push(Finding { rule: IDS_RULE.to_string(), path, line: 0, message: format!("objective id {id} was on the base and is gone: ids are a public contract; keep the record and mark it `deprecated` with `replacedBy`") });
        }
    }
    for (id, (path, v)) in &live {
        if let Some(d) = v.get("deprecated") {
            let to = d.get("replacedBy").and_then(Value::as_str).unwrap_or("");
            if to.is_empty() || to == id || !live.contains_key(to) {
                out.push(Finding { rule: IDS_RULE.to_string(), path: path.clone(), line: 0, message: format!("{id} is deprecated but `replacedBy` {to:?} is not another objective that exists") });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn lesson(status: &str, title: &str, revision: u64, reviews: Value) -> Value {
        json!({"schema": "lesson/1", "id": "les_x", "title": title, "revision": revision, "status": status,
               "provenance": {"origin": "ai", "maker": "a", "created": "2026-01-01", "reviews": reviews}})
    }
    fn pass(by: &str) -> Value {
        json!({"by": by, "kind": "human", "role": "subject", "verdict": "pass", "date": "2026-01-02"})
    }

    #[test]
    fn a_reviewed_record_may_stay_or_move_forward() {
        let base = lesson("reviewed", "Count", 1, json!([pass("t1")]));
        assert!(illegal(&base, &base).is_none());
        assert!(illegal(&base, &lesson("published", "Count", 1, json!([pass("t1"), pass("t2")]))).is_none());
    }

    #[test]
    fn a_silent_change_is_refused() {
        let base = lesson("reviewed", "Count", 1, json!([pass("t1")]));
        assert!(illegal(&base, &lesson("reviewed", "Count!", 1, json!([pass("t1")]))).is_some());
        assert!(illegal(&base, &lesson("reviewed", "Count", 1, json!([]))).is_some());
        assert!(illegal(&lesson("published", "Count", 1, json!([pass("t1")])), &base).is_some());
    }

    #[test]
    fn a_deprecated_id_stays_and_points_at_a_live_one() {
        let d = std::env::temp_dir().join(format!("ol-ids-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let (b, h) = (d.join("b"), d.join("h"));
        for (root, files) in [(&b, vec![("a", ""), ("b", "")]), (&h, vec![("a", ",\"deprecated\":{\"replacedBy\":\"b\"}"), ("b", "")])] {
            fs::create_dir_all(root.join("skills")).unwrap();
            for (n, extra) in files {
                fs::write(root.join(format!("skills/{n}.json")), format!("{{\"id\":\"{n}\"{extra}}}")).unwrap();
            }
        }
        let base = DirBase { base: b.clone(), head: h.clone() };
        assert!(ids_stable(&h, &base).unwrap().is_empty());
        fs::remove_file(h.join("skills/a.json")).unwrap();
        assert_eq!(ids_stable(&h, &base).unwrap().len(), 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_revision_reopens_the_record() {
        let base = lesson("reviewed", "Count", 1, json!([pass("t1")]));
        assert!(illegal(&base, &lesson("proposed", "Count on", 1, json!([]))).is_some(), "the counter did not move");
        assert!(illegal(&base, &lesson("proposed", "Count on", 2, json!([]))).is_none());
    }
}
