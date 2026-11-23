//! The sourced AI jury (review/jury-policy.md): work orders per role, the decision record, and the checks the gate
//! `review.jury-decision` and the runner share.
//!
//! Three roles (subject, safety, language) are filled apart by any harness. A work order holds the content and one
//! role's checklist, never another role's verdict. `run` turns the three verdicts into one decision record per lesson
//! and item under `review/decisions/`, and sets a lesson and its items `reviewed` only when every record of the lesson
//! passes unanimously with every correctness claim sourced. The program never calls a model itself.

use crate::canon::{canonical, sha256_hex};
use crate::scan::{self, Node};
use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const RULE: &str = "review.jury-decision";
pub const ROLES: [&str; 3] = ["subject", "safety", "language"];
pub const DECISIONS: &str = "review/decisions";

/// Each role's checklist, as review/jury-policy.md states it (a test keeps the two in step).
pub const CHECKS: &[(&str, &[(&str, &str)])] = &[
    (
        "subject",
        &[
            ("objective-taught", "the lesson teaches the objective it names, at the level of its age band"),
            ("answer-correct", "every graded item has exactly one correct answer, the key names it, and no distractor is also right"),
            ("explanation-correct", "the teaching text, examples, hints and feedback state nothing false or misleading"),
            ("misconceptions-sound", "each distractor and its feedback answer a real misconception, and say so correctly"),
            ("hints-fair", "a hint helps thinking without giving the answer away (no position, colour, or elimination give-aways)"),
            ("claims-sourced", "every correctness claim cites a registry entry and a real reference"),
        ],
    ),
    (
        "safety",
        &[
            ("no-harm", "nothing violent, sexual, frightening, hateful, or dangerous to imitate"),
            ("no-personal-data", "never asks a child for personal information, and never encourages contact with strangers"),
            ("safe-advice", "any safety guidance (online or physical) is accurate and points the child to a trusted adult"),
            ("age-fit", "ideas, words and sentence length fit the youngest age the lesson names"),
            ("no-manipulation", "no pressure, shame, fear, rewards for secrecy, persuasive design, brands or advertising"),
            ("inclusive", "no stereotype or exclusion by sex, ethnicity, religion, disability, family form or wealth"),
        ],
    ),
    (
        "language",
        &[
            ("natural", "reads as a native speaker of the locale would write for a child; not a literal translation"),
            ("correct", "grammar, spelling, script and punctuation are right for the locale"),
            ("register", "words and tone are right for the age"),
            ("culture", "names, foods, places, measures and customs fit the locale or are neutral; nothing offends there"),
            ("consistent", "the prompt, the options, the answer and the teaching text use the same terms"),
        ],
    ),
];

pub fn checks(role: &str) -> &'static [(&'static str, &'static str)] {
    CHECKS.iter().find(|(r, _)| *r == role).map(|(_, c)| *c).unwrap_or(&[])
}

fn st<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

/// The content a decision is about: the record without `status` and `provenance`.
pub fn content_hash(record: &Value) -> Result<String> {
    let mut v = record.clone();
    if let Some(o) = v.as_object_mut() {
        o.remove("status");
        o.remove("provenance");
    }
    Ok(sha256_hex(format!("jury-content/1\n{}", canonical(&v)?).as_bytes()))
}

pub fn decision_path(id: &str) -> String {
    format!("{DECISIONS}/{id}.json")
}

/// The jury entries of REVIEWERS.json.
pub fn juries(reviewers: &[Value]) -> Vec<&Value> {
    reviewers.iter().filter(|e| st(e, "kind") == "jury").collect()
}

/// What the checks need from the tree: the standards registry and the skill -> standard alignments.
pub struct Registry<'a> {
    pub standards: &'a BTreeMap<String, Node>,
    pub aligned: &'a BTreeMap<String, BTreeSet<String>>,
}

impl Registry<'_> {
    /// A registry entry counts for a record when it exists and it, or one of its ancestors, is aligned to a skill of it.
    fn entry_problem(&self, id: &str, skills: &[String]) -> Option<String> {
        if !self.standards.contains_key(id) {
            return Some(format!("the registry entry `{id}` is not a standard in standards/"));
        }
        let wanted: BTreeSet<&String> = skills.iter().flat_map(|s| self.aligned.get(s).into_iter().flatten()).collect();
        let mut at = id.to_string();
        for _ in 0..16 {
            if wanted.contains(&at) {
                return None;
            }
            match self.standards.get(&at) {
                Some(n) if !n.parent.is_empty() => at = n.parent.clone(),
                _ => break,
            }
        }
        Some(format!("the registry entry `{id}` is not aligned to a skill of this record ({}), itself or through an ancestor", skills.join(", ")))
    }
}

fn date_ok(d: &str) -> bool {
    d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-' && d.chars().filter(|c| c.is_ascii_digit()).count() == 8
}

/// Why one source does not resolve, if it does not.
pub fn source_problem(s: &Value, decided: &str) -> Option<String> {
    let url = st(s, "url");
    let accessed = st(s, "accessed");
    let licence = st(s, "licence");
    let by_licence = matches!(licence, "public-domain" | "CC0-1.0") || ["CC-BY-", "CC-BY-SA-"].iter().any(|p| licence.strip_prefix(p).is_some_and(|v| v.starts_with(|c: char| c.is_ascii_digit())));
    if !url.is_empty() {
        if !url.starts_with("https://") || url.len() < 12 || url.contains(char::is_whitespace) {
            return Some(format!("the source `{url}` is not an https web address"));
        }
        if accessed.is_empty() && !by_licence {
            return Some(format!("the source {url} has no access date"));
        }
        if !accessed.is_empty() && (!date_ok(accessed) || accessed > decided || accessed < "2000-01-01") {
            return Some(format!("the source {url} was accessed on `{accessed}`, which is not a date on or before the decision ({decided})"));
        }
        return None;
    }
    if by_licence && !st(s, "title").is_empty() {
        return None;
    }
    Some(format!(
        "the source `{}` has neither a web address with an access date nor a public-domain, CC0 or CC BY licence",
        st(s, "title")
    ))
}

/// Why one role's verdict on one record is not a pass, if it is not. `record` is the lesson or item, `locale` its
/// lesson's, `decided` the decision date.
pub fn role_problems(reg: &Registry, role: &str, v: &Value, record: &Value, locale: &str, decided: &str) -> Vec<String> {
    let mut out = Vec::new();
    if v.is_null() {
        return vec![format!("{role}: no verdict was given")];
    }
    if st(v, "verdict") != "pass" {
        let notes: Vec<&str> = arr(v, "notes").iter().filter_map(Value::as_str).collect();
        out.push(format!("{role}: objects{}", if notes.is_empty() { String::new() } else { format!(": {}", notes.join("; ")) }));
    }
    if st(v, "model").trim().is_empty() {
        out.push(format!("{role}: the model is not recorded"));
    }
    for (check, _) in checks(role) {
        match v["checks"].get(*check).and_then(Value::as_bool) {
            Some(true) => {}
            Some(false) => out.push(format!("{role}: check `{check}` is false")),
            None => out.push(format!("{role}: check `{check}` was not answered")),
        }
    }
    if role == "language" && st(v, "locale") != locale {
        out.push(format!("language: judged the locale `{}`, but the record is in `{locale}`", st(v, "locale")));
    }
    if role == "subject" {
        let skills: Vec<String> = arr(record, "skills").iter().filter_map(Value::as_str).map(str::to_string).collect();
        let claims = arr(v, "claims");
        if claims.is_empty() {
            out.push("subject: no correctness claim is made, so nothing is sourced: an unsourced pass is not a pass".to_string());
        }
        for (i, c) in claims.iter().enumerate() {
            let n = i + 1;
            if st(c, "registry").is_empty() {
                out.push(format!("subject: claim {n} cites no registry entry"));
            } else if let Some(m) = reg.entry_problem(st(c, "registry"), &skills) {
                out.push(format!("subject: claim {n}: {m}"));
            }
            let sources = arr(c, "sources");
            if sources.is_empty() {
                out.push(format!("subject: claim {n} has no source: an unsourced claim fails"));
            }
            for s in sources {
                if let Some(m) = source_problem(s, decided) {
                    out.push(format!("subject: claim {n}: {m}"));
                }
            }
        }
    }
    out
}

/// Every reason a decision record does not settle `record` (lesson `lesson`, locale `locale`); empty when it does.
pub fn decision_problems(reg: &Registry, reviewers: &[Value], d: &Value, record: &Value, lesson: &str, locale: &str) -> Vec<String> {
    let mut out = Vec::new();
    let id = st(record, "id");
    if st(d, "record") != id {
        out.push(format!("the decision is about `{}`, not `{id}`", st(d, "record")));
    }
    if st(d, "lesson") != lesson {
        out.push(format!("the decision names the lesson `{}`, but the record belongs to `{lesson}`", st(d, "lesson")));
    }
    match content_hash(record) {
        Ok(h) if h == st(d, "contentHash") => {}
        Ok(_) => out.push("the content changed after the jury decided it (contentHash differs): the decision lapsed".to_string()),
        Err(e) => out.push(format!("the record cannot be hashed: {e}")),
    }
    let date = st(d, "date");
    match juries(reviewers).into_iter().find(|j| st(j, "id") == st(d, "jury")) {
        None => out.push(format!("the jury `{}` is not listed in REVIEWERS.json", st(d, "jury"))),
        Some(j) => {
            if st(j, "policyVersion") != st(d, "policyVersion") {
                out.push(format!("decided under policy version {}, but the jury is at version {}", st(d, "policyVersion"), st(j, "policyVersion")));
            }
            if st(j, "policy") != st(d, "policy") {
                out.push(format!("decided under the policy `{}`, but the jury's policy is `{}`", st(d, "policy"), st(j, "policy")));
            }
            if date < st(j, "since") || (!st(j, "until").is_empty() && date > st(j, "until")) {
                out.push(format!("decided on {date}, outside the jury's term"));
            }
        }
    }
    let mut unanimous = true;
    for role in ROLES {
        let p = role_problems(reg, role, &d["roles"][role], record, locale, date);
        unanimous &= p.is_empty();
        out.extend(p);
    }
    if !unanimous && st(d, "outcome") == "pass" {
        out.push("the outcome says pass, but the roles are not unanimous or a claim is unsourced".to_string());
    }
    if st(d, "outcome") != "pass" {
        out.push(format!("the jury's outcome is `{}`: only a unanimous, sourced pass settles a record", st(d, "outcome")));
    }
    out
}

/// The skill -> aligned standards map, from a scan.
pub fn aligned_map(s: &scan::Scan) -> BTreeMap<String, BTreeSet<String>> {
    let mut m: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for a in &s.alignments {
        m.entry(a.skill.clone()).or_default().insert(a.standard.clone());
    }
    m
}

/// One lesson folder: the lesson record and its items, in file order.
pub struct LessonFiles {
    pub dir: PathBuf,
    pub lesson: Value,
    pub items: Vec<Value>,
}

pub fn read_lesson(root: &Path, s: &scan::Scan, id: &str) -> Result<LessonFiles> {
    let line = s.lessons.iter().find(|l| st(l, "id") == id).with_context(|| format!("no lesson {id} in the tree"))?;
    let dir = root.join(st(line, "path"));
    let lesson: Value = serde_json::from_str(&fs::read_to_string(dir.join("lesson.json"))?).with_context(|| format!("{}", dir.display()))?;
    let mut items = Vec::new();
    if let Ok(raw) = fs::read_to_string(dir.join("items.jsonl")) {
        for l in raw.lines().filter(|l| !l.trim().is_empty()) {
            items.push(serde_json::from_str(l)?);
        }
    }
    Ok(LessonFiles { dir, lesson, items })
}

fn bare(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        o.remove("status");
        o.remove("provenance");
    }
    v
}

/// The work order for one role on one lesson: the content, its objective, its aligned standards and the checklist.
/// Never another role's verdict.
pub fn order(root: &Path, s: &scan::Scan, jury: &Value, role: &str, lesson: &str) -> Result<Value> {
    if !ROLES.contains(&role) {
        bail!("the role is subject, safety or language");
    }
    let f = read_lesson(root, s, lesson)?;
    let skills: Vec<String> = arr(&f.lesson, "skills").iter().filter_map(Value::as_str).map(str::to_string).collect();
    let aligned = aligned_map(s);
    let mut standards = Vec::new();
    for sk in &skills {
        for std_id in aligned.get(sk).into_iter().flatten() {
            let n = &s.nodes[std_id];
            standards.push(json!({"id": std_id, "label": n.get("label"), "statementOwn": n.get("statementOwn"), "sourceUrl": n.get("sourceUrl")}));
        }
    }
    let objectives: Vec<Value> = skills.iter().filter_map(|k| s.objectives.get(k)).map(|o| bare(&o.record)).collect();
    let mut hashes = Map::new();
    hashes.insert(st(&f.lesson, "id").to_string(), json!(content_hash(&f.lesson)?));
    for it in &f.items {
        hashes.insert(st(it, "id").to_string(), json!(content_hash(it)?));
    }
    let checklist: Vec<Value> = checks(role).iter().map(|(k, t)| json!({"id": k, "check": t})).collect();
    let example_claim = json!([{"claim": "what is true, in a sentence", "registry": "<a standard id from `standards` above, or a child of one>", "sources": [{"title": "what you read", "url": "https://...", "accessed": "YYYY-MM-DD"}]}]);
    let mut record_answer = json!({"contentHash": "<from contentHashes>", "verdict": "pass or object", "checks": checklist.iter().map(|c| (c["id"].as_str().unwrap_or("").to_string(), json!(true))).collect::<Map<_, _>>(), "notes": []});
    if role == "subject" {
        record_answer["claims"] = example_claim;
    }
    if role == "language" {
        record_answer["locale"] = json!(st(&f.lesson, "locale"));
    }
    Ok(json!({
        "schema": "jury-order/1",
        "jury": st(jury, "id"),
        "policy": st(jury, "policy"),
        "policyVersion": st(jury, "policyVersion"),
        "role": role,
        "lesson": {"record": bare(&f.lesson), "items": f.items.iter().map(bare).collect::<Vec<_>>()},
        "contentHashes": hashes,
        "objectives": objectives,
        "standards": standards,
        "checklist": checklist,
        "rules": [
            format!("You are the {role} reviewer of a jury of three. Decide alone: you are not shown, and must not seek, any other reviewer's verdict."),
            "Give one verdict per record: the lesson and every item, keyed by id. Answer every check true or false; a false check is an objection and needs a note.",
            "Any doubt is an objection: an objection keeps the record a draft, which costs little; a wrong pass reaches a child.",
            "Subject: every correctness claim cites a registry entry (a standard id above, or a child of one) and a real reference you opened: an https address with the date you read it, or a public-domain, CC0 or CC BY source. An unsourced claim fails.",
            "Record the model you are in `model`. Write nothing into the tree: hand the verdict to `openlessons jury run` (or the MCP tool `jury_submit`).",
        ],
        "answer": {"schema": "jury-verdict/1", "role": role, "lesson": lesson, "model": "<the model you are>", "reviewer": "<your harness session>", "date": "YYYY-MM-DD", "records": {"<record id>": record_answer}},
    }))
}

/// A verdict file's shape problems (a malformed verdict decides nothing).
pub fn verdict_shape(v: &Value, role: &str, lesson: &str) -> Vec<String> {
    let mut out = Vec::new();
    if st(v, "schema") != "jury-verdict/1" {
        out.push("`schema` must be \"jury-verdict/1\"".to_string());
    }
    if st(v, "role") != role {
        out.push(format!("filed as the {role} verdict, but says role `{}`", st(v, "role")));
    }
    if st(v, "lesson") != lesson {
        out.push(format!("filed for {lesson}, but says lesson `{}`", st(v, "lesson")));
    }
    if !date_ok(st(v, "date")) {
        out.push("`date` must be YYYY-MM-DD".to_string());
    }
    if !v["records"].is_object() {
        out.push("`records` must be an object keyed by record id".to_string());
    }
    out
}

pub struct RunOptions<'a> {
    pub root: &'a Path,
    /// the gate data folder (usually <root>/gates/data)
    pub data: &'a Path,
    pub verdicts: &'a Path,
    pub date: String,
    pub harness: String,
    pub independence: String,
    pub dry_run: bool,
    pub lessons: Vec<String>,
}

/// One lesson's outcome in a run.
#[derive(Debug, Clone)]
pub struct LessonOutcome {
    pub lesson: String,
    pub passed: bool,
    pub records: usize,
    pub passed_records: usize,
    pub notes: Vec<String>,
}

#[derive(Debug, Default)]
pub struct RunReport {
    pub lessons: Vec<LessonOutcome>,
    pub spot_checks: Vec<String>,
    pub written: Vec<String>,
}

/// The spot-check draw: the `ceil(rate * n)` passes with the lowest sha256 of "<policy version>:<date>:<id>".
pub fn spot_sample(passes: &[String], policy_version: &str, date: &str, rate: f64) -> Vec<String> {
    let n = ((passes.len() as f64) * rate).ceil() as usize;
    let mut ranked: Vec<(String, String)> = passes.iter().map(|id| (sha256_hex(format!("{policy_version}:{date}:{id}").as_bytes()), id.clone())).collect();
    ranked.sort();
    let mut out: Vec<String> = ranked.into_iter().take(n).map(|(_, id)| id).collect();
    out.sort();
    out
}

type Decided = (LessonFiles, Vec<(String, Value)>, bool);

/// Decide every lesson that has verdicts from all three roles; write the decisions and settle the lessons that pass.
pub fn run(o: &RunOptions) -> Result<RunReport> {
    let s = scan::scan(o.root, o.data)?;
    let reviewers: Value = serde_json::from_str(&fs::read_to_string(o.root.join("REVIEWERS.json")).context("REVIEWERS.json")?)?;
    let reviewers = arr(&reviewers, "reviewers").to_vec();
    let jlist = juries(&reviewers);
    let [jury] = jlist.as_slice() else { bail!("REVIEWERS.json must list exactly one jury entry (it lists {})", jlist.len()) };
    let jury = (*jury).clone();
    let rate = jury.get("spotCheckRate").and_then(Value::as_f64).unwrap_or(0.05);
    let aligned = aligned_map(&s);
    let reg = Registry { standards: &s.standards, aligned: &aligned };

    // the lessons to decide: those with a verdict from any role (or the ones asked for)
    let mut lessons: BTreeSet<String> = o.lessons.iter().cloned().collect();
    if lessons.is_empty() {
        for role in ROLES {
            for e in fs::read_dir(o.verdicts.join(role)).into_iter().flatten().flatten() {
                if let Some(id) = e.file_name().to_string_lossy().strip_suffix(".json") {
                    lessons.insert(id.to_string());
                }
            }
        }
    }

    let mut report = RunReport::default();
    // per lesson: its files, (record id, decision) for each record, and whether the whole lesson passed
    let mut decided: Vec<Decided> = Vec::new();
    for lesson in &lessons {
        let f = read_lesson(o.root, &s, lesson)?;
        let locale = st(&f.lesson, "locale").to_string();
        let mut verdicts: BTreeMap<&str, Value> = BTreeMap::new();
        let mut missing = Vec::new();
        for role in ROLES {
            let p = o.verdicts.join(role).join(format!("{lesson}.json"));
            match fs::read_to_string(&p) {
                Ok(raw) => {
                    let v: Value = serde_json::from_str(&raw).with_context(|| format!("{}", p.display()))?;
                    let shape = verdict_shape(&v, role, lesson);
                    if !shape.is_empty() {
                        bail!("{}: {}", p.display(), shape.join("; "));
                    }
                    verdicts.insert(role, v);
                }
                Err(_) => missing.push(role),
            }
        }
        if !missing.is_empty() {
            report.lessons.push(LessonOutcome { lesson: lesson.clone(), passed: false, records: 0, passed_records: 0, notes: vec![format!("not decided: no verdict yet from {}", missing.join(", "))] });
            continue;
        }
        let mut records: Vec<Value> = vec![f.lesson.clone()];
        records.extend(f.items.iter().cloned());
        let mut out = Vec::new();
        let mut all = true;
        let mut notes = Vec::new();
        let mut passed_records = 0;
        for rec in &records {
            let id = st(rec, "id").to_string();
            let hash = content_hash(rec)?;
            let mut roles = Map::new();
            let mut problems = Vec::new();
            for role in ROLES {
                let v = &verdicts[role];
                let mut rv = v["records"].get(&id).cloned().unwrap_or(Value::Null);
                if !rv.is_null() {
                    if st(&rv, "contentHash") != hash {
                        problems.push(format!("{role}: judged other content than this record holds (contentHash differs)"));
                    }
                    let Some(obj) = rv.as_object_mut() else { bail!("{role} verdict on {id} is not an object") };
                    obj.remove("contentHash");
                    obj.insert("model".into(), json!(st(v, "model")));
                    obj.insert("reviewer".into(), json!(st(v, "reviewer")));
                    obj.entry("notes").or_insert(json!([]));
                    obj.entry("checks").or_insert(json!({}));
                }
                problems.extend(role_problems(&reg, role, &rv, rec, &locale, &o.date));
                if rv.is_null() {
                    rv = json!({"verdict": "object", "model": st(v, "model"), "reviewer": st(v, "reviewer"), "checks": {}, "notes": ["no verdict was given for this record"]});
                }
                roles.insert(role.to_string(), rv);
            }
            let pass = problems.is_empty();
            all &= pass;
            if pass {
                passed_records += 1;
            }
            notes.extend(problems.iter().map(|p| format!("{id}: {p}")));
            out.push((
                id.clone(),
                json!({
                    "schema": "jury-decision/1",
                    "record": id,
                    "lesson": lesson,
                    "contentHash": hash,
                    "jury": st(&jury, "id"),
                    "policy": st(&jury, "policy"),
                    "policyVersion": st(&jury, "policyVersion"),
                    "date": o.date,
                    "harness": o.harness,
                    "independence": o.independence,
                    "roles": roles,
                    "outcome": if pass { "pass" } else { "draft" },
                    "notes": problems,
                    "spotCheck": "none",
                }),
            ));
        }
        report.lessons.push(LessonOutcome { lesson: lesson.clone(), passed: all, records: records.len(), passed_records, notes });
        decided.push((f, out, all));
    }

    // the spot-check draw, over the records this run settles
    let settled: Vec<String> = decided.iter().filter(|d| d.2).flat_map(|d| d.1.iter().map(|(id, _)| id.clone())).collect();
    report.spot_checks = spot_sample(&settled, st(&jury, "policyVersion"), &o.date, rate);

    if o.dry_run {
        return Ok(report);
    }
    if !decided.is_empty() {
        fs::create_dir_all(o.root.join(DECISIONS))?;
    }
    for (f, out, all) in decided {
        for (id, mut d) in out {
            if report.spot_checks.contains(&id) {
                d["spotCheck"] = json!("pending");
            }
            if !all && d["outcome"] == "pass" {
                d["notes"] = json!(["held: another record of this lesson did not pass, and a lesson leaves draft whole"]);
            }
            let rel = decision_path(&id);
            fs::write(o.root.join(&rel), format!("{}\n", serde_json::to_string_pretty(&d)?))?;
            report.written.push(rel);
        }
        if all {
            settle(&f, st(&jury, "id"), &o.date)?;
            report.written.push(f.dir.strip_prefix(o.root).unwrap_or(&f.dir).to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(report)
}

fn jury_review(jury: &str, id: &str, date: &str) -> Value {
    json!({"by": jury, "kind": "jury", "role": "jury", "verdict": "pass", "date": date, "decision": decision_path(id)})
}

fn settled(mut v: Value, jury: &str, date: &str) -> Value {
    let id = st(&v, "id").to_string();
    v["status"] = json!("reviewed");
    let p = v["provenance"].as_object_mut();
    if let Some(p) = p {
        let reviews = p.entry("reviews").or_insert(json!([]));
        if let Some(a) = reviews.as_array_mut() {
            a.push(jury_review(jury, &id, date));
        }
    }
    v
}

/// Set a lesson and its items `reviewed`, with the jury's review entry appended to each.
fn settle(f: &LessonFiles, jury: &str, date: &str) -> Result<()> {
    let lesson = settled(f.lesson.clone(), jury, date);
    fs::write(f.dir.join("lesson.json"), format!("{}\n", serde_json::to_string_pretty(&lesson)?))?;
    let body: String = f.items.iter().map(|it| format!("{}\n", settled(it.clone(), jury, date))).collect();
    if !f.items.is_empty() {
        fs::write(f.dir.join("items.jsonl"), body)?;
    }
    Ok(())
}

/// The run as Markdown, for a pull request.
pub fn markdown(r: &RunReport) -> String {
    let pass = r.lessons.iter().filter(|l| l.passed).count();
    let records: usize = r.lessons.iter().map(|l| l.records).sum();
    let rpass: usize = r.lessons.iter().filter(|l| l.passed).map(|l| l.records).sum();
    let mut s = format!(
        "## Jury run\n\n{pass} of {} lesson(s) passed ({rpass} of {records} record(s) settled `reviewed`); spot-check drawn: {}.\n\n| lesson | outcome | records passed |\n|---|---|---:|\n",
        r.lessons.len(),
        if r.spot_checks.is_empty() { "none".to_string() } else { r.spot_checks.join(", ") }
    );
    for l in &r.lessons {
        s.push_str(&format!("| `{}` | {} | {}/{} |\n", l.lesson, if l.passed { "reviewed" } else { "draft" }, l.passed_records, l.records));
    }
    for l in r.lessons.iter().filter(|l| !l.passed) {
        s.push_str(&format!("\n### `{}` stays a draft\n\n", l.lesson));
        for n in &l.notes {
            s.push_str(&format!("- {n}\n"));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_resolves_by_an_accessed_address_or_an_open_licence() {
        let d = "2026-11-23";
        assert_eq!(source_problem(&json!({"title": "t", "url": "https://example.org/a", "accessed": "2026-11-20"}), d), None);
        assert_eq!(source_problem(&json!({"title": "t", "licence": "CC-BY-4.0"}), d), None);
        assert_eq!(source_problem(&json!({"title": "t", "licence": "public-domain"}), d), None);
        assert!(source_problem(&json!({"title": "t", "url": "https://example.org/a"}), d).is_some(), "no access date");
        assert!(source_problem(&json!({"title": "t", "url": "http://example.org/a", "accessed": d}), d).is_some(), "not https");
        assert!(source_problem(&json!({"title": "t", "url": "https://example.org/a", "accessed": "2026-12-01"}), d).is_some(), "read after the decision");
        assert_eq!(source_problem(&json!({"title": "t", "licence": "CC-BY-SA-4.0"}), d), None);
        assert!(source_problem(&json!({"title": "t", "licence": "CC-BY-NC-4.0"}), d).is_some(), "non-commercial is not CC BY");
        assert!(source_problem(&json!({"title": "t", "licence": "all rights reserved"}), d).is_some());
        assert!(source_problem(&json!({"title": "t"}), d).is_some());
    }

    #[test]
    fn the_draw_is_five_percent_rounded_up_and_repeatable() {
        let ids: Vec<String> = (0..41).map(|i| format!("itm_{i:024}")).collect();
        let a = spot_sample(&ids, "1", "2026-11-23", 0.05);
        assert_eq!(a.len(), 3);
        assert_eq!(a, spot_sample(&ids, "1", "2026-11-23", 0.05));
        assert!(spot_sample(&[], "1", "2026-11-23", 0.05).is_empty());
    }

    #[test]
    fn the_content_hash_ignores_status_and_provenance_only() {
        let a = json!({"id": "les_x", "title": "Count", "status": "draft", "provenance": {"maker": "a"}});
        let mut b = a.clone();
        b["status"] = json!("reviewed");
        b["provenance"]["reviews"] = json!([1]);
        assert_eq!(content_hash(&a).unwrap(), content_hash(&b).unwrap());
        b["title"] = json!("Count!");
        assert_ne!(content_hash(&a).unwrap(), content_hash(&b).unwrap());
    }

    #[test]
    fn the_policy_states_every_check() {
        let policy = include_str!("../../../review/jury-policy.md");
        for (role, list) in CHECKS {
            assert!(policy.contains(&format!("### {role}")), "{role}");
            for (k, _) in *list {
                assert!(policy.contains(&format!("`{k}`")), "{role}: {k}");
            }
        }
    }
}
