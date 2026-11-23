//! One pass over a data tree: every gate, the counts, and the index the tree should have.
//!
//! Reference data (locales, standards, skills, alignments, assets) is small and read whole. Lessons are read one folder at
//! a time; across lessons only item ids and content signatures are kept, for the duplicate gate.

use crate::canon::{asset_id, item_id, lesson_id, sha256_hex, shard};
use crate::item;
use crate::text::{self, Lexicon};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// The gates, in the order they are reported. Each has a row in gates/rules.json and a red fixture.
pub const RULES: &[&str] = &[
    "schema.valid",
    "layout.content-addressed",
    "refs.resolve",
    "standards.terms-respected",
    "lesson.aligned",
    "graph.placed",
    "alignment.band-matches",
    "lesson.reading-level",
    "item.answer-verified",
    "item.no-answer-leak",
    "item.heard-prompt-recorded",
    "asset.licensed",
    "content.no-pii",
    "content.no-copied-text",
    "content.child-safe",
    "content.no-duplicates",
    "loop.review-before-reviewed",
    "loop.reviewer-listed",
    "loop.base-respected",
    "index.fresh",
];

const SCHEMAS: &[(&str, &str)] = &[
    ("framework/1", include_str!("../../../schemas/framework.schema.json")),
    ("standard/1", include_str!("../../../schemas/standard.schema.json")),
    ("skill/1", include_str!("../../../schemas/skill.schema.json")),
    ("alignment/1", include_str!("../../../schemas/alignment.schema.json")),
    ("lesson/1", include_str!("../../../schemas/lesson.schema.json")),
    ("item/1", include_str!("../../../schemas/item.schema.json")),
    ("asset/1", include_str!("../../../schemas/asset.schema.json")),
    ("locale/1", include_str!("../../../schemas/locale.schema.json")),
    ("path/1", include_str!("../../../schemas/path.schema.json")),
    ("proposal/1", include_str!("../../../schemas/proposal.schema.json")),
    ("reviewers/1", include_str!("../../../schemas/reviewers.schema.json")),
    ("age-bands/1", include_str!("../../../schemas/age-bands.schema.json")),
    ("subject/1", include_str!("../../../schemas/subject.schema.json")),
    ("signal/1", include_str!("../../../schemas/signal.schema.json")),
    ("run/1", include_str!("../../../schemas/run.schema.json")),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub rule: String,
    pub path: String,
    /// 1-based line in a .jsonl file; 0 when the finding is about the whole file
    pub line: usize,
    pub message: String,
}

/// One alignment row, as the coverage figures need it.
#[derive(Debug, Clone)]
pub struct Aligned {
    pub skill: String,
    pub standard: String,
    pub strength: String,
    pub status: String,
}

/// One standard node, as the coverage figures need it.
#[derive(Debug, Clone)]
pub struct Node {
    pub framework: String,
    pub kind: String,
    pub parent: String,
}

/// One age band of bands/age-bands.json.
#[derive(Debug, Clone)]
pub struct Band {
    pub id: String,
    pub min: i64,
    pub max: i64,
}

/// One objective of the topic graph (a skill/1 record), as the gap report and the loop need it.
#[derive(Debug, Clone, Default)]
pub struct Objective {
    pub subject: String,
    pub strand: String,
    pub topic: String,
    pub bands: Vec<String>,
    pub prerequisites: Vec<String>,
    /// the languages of instruction the objective applies to (its own list, or its subject's)
    pub locales: Vec<String>,
    /// the whole record
    pub record: Value,
}

/// One usage count of signals/: (objective, band, locale) -> count.
#[derive(Debug, Clone)]
pub struct Signal {
    pub objective: String,
    pub band: String,
    pub locale: String,
    pub count: u64,
}

#[derive(Debug, Default)]
pub struct Scan {
    /// the age bands, youngest first
    pub bands: Vec<Band>,
    /// locale id -> the locale/1 record
    pub locales: BTreeMap<String, Value>,
    /// skill id -> objective
    pub objectives: BTreeMap<String, Objective>,
    /// standard id -> the standard/1 record
    pub nodes: BTreeMap<String, Value>,
    /// framework id -> terms.quote
    pub frameworks: BTreeMap<String, String>,
    /// the standards registry: every framework/1 record
    pub registry: Vec<Value>,
    /// subject id -> its area (maths, literacy, science, ...)
    pub areas: BTreeMap<String, String>,
    pub signals: Vec<Signal>,
    pub findings: Vec<Finding>,
    pub counts: BTreeMap<String, u64>,
    /// the generated index: relative path -> file content
    pub index: BTreeMap<String, String>,
    /// standard id -> node
    pub standards: BTreeMap<String, Node>,
    pub alignments: Vec<Aligned>,
    /// one index line per lesson (id, locale, subject, skills, status, items, ...)
    pub lessons: Vec<Value>,
}

struct Ctx {
    root: PathBuf,
    validators: HashMap<&'static str, jsonschema::Validator>,
    findings: Vec<Finding>,
    counts: BTreeMap<String, u64>,
}

impl Ctx {
    fn add(&mut self, rule: &str, path: &str, line: usize, message: impl Into<String>) {
        debug_assert!(RULES.contains(&rule));
        self.findings.push(Finding { rule: rule.to_string(), path: path.to_string(), line, message: message.into() });
    }
    fn count(&mut self, what: &str, n: u64) {
        *self.counts.entry(what.to_string()).or_default() += n;
    }
    fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root).unwrap_or(p).to_string_lossy().replace('\\', "/")
    }
    /// Schema-check one record; false means later gates skip it.
    fn valid(&mut self, schema: &'static str, v: &Value, path: &str, line: usize) -> bool {
        if st(v, "schema") != schema {
            self.add("schema.valid", path, line, format!("`schema` must be \"{schema}\""));
            return false;
        }
        let errors: Vec<String> = self.validators[schema].iter_errors(v).take(3).map(|e| format!("{} at {}", e, pointer(&e))).collect();
        for e in &errors {
            self.add("schema.valid", path, line, e.clone());
        }
        errors.is_empty()
    }
    fn read_json(&mut self, p: &Path) -> Option<Value> {
        let rel = self.rel(p);
        let raw = match fs::read_to_string(p) {
            Ok(s) => s,
            Err(e) => {
                self.add("schema.valid", &rel, 0, format!("cannot be read: {e}"));
                return None;
            }
        };
        match serde_json::from_str(&raw) {
            Ok(v) => Some(v),
            Err(e) => {
                self.add("schema.valid", &rel, 0, format!("not JSON: {e}"));
                None
            }
        }
    }
    /// The records of a .jsonl file with their line numbers.
    fn read_jsonl(&mut self, p: &Path) -> Vec<(usize, Value)> {
        let rel = self.rel(p);
        let raw = match fs::read_to_string(p) {
            Ok(s) => s,
            Err(e) => {
                self.add("schema.valid", &rel, 0, format!("cannot be read: {e}"));
                return vec![];
            }
        };
        let mut out = Vec::new();
        for (i, line) in raw.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str(line) {
                Ok(v) => out.push((i + 1, v)),
                Err(e) => self.add("schema.valid", &rel, i + 1, format!("not JSON: {e}")),
            }
        }
        out
    }
}

fn pointer(e: &jsonschema::ValidationError) -> String {
    let p = e.instance_path().to_string();
    if p.is_empty() {
        "the record".to_string()
    } else {
        p
    }
}

fn st<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}
fn strs(v: &Value, k: &str) -> Vec<String> {
    arr(v, k).iter().filter_map(Value::as_str).map(str::to_string).collect()
}

/// Sorted entries of a folder; empty when it does not exist.
fn ls(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir).map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
    out.sort();
    out
}
fn name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}
fn stem(p: &Path, ext: &str) -> Option<String> {
    name(p).strip_suffix(ext).map(str::to_string)
}

/// A record that is `reviewed` or `published` has a passing review by a person who did not make it.
fn review_problem(v: &Value) -> Option<String> {
    let status = st(v, "status");
    let p = &v["provenance"];
    let reviewed = matches!(status, "reviewed" | "published");
    let human_pass = arr(p, "reviews").iter().any(|r| st(r, "kind") == "human" && st(r, "verdict") == "pass" && st(r, "by") != st(p, "maker"));
    if reviewed && !human_pass {
        return Some(format!(
            "status `{status}` (origin: {}) needs a passing review by a person other than the maker",
            st(p, "origin")
        ));
    }
    // an exact alignment says "this standard is covered": only a subject reviewer can settle that
    let subject_pass = arr(p, "reviews").iter().any(|r| st(r, "kind") == "human" && st(r, "verdict") == "pass" && st(r, "role") == "subject" && st(r, "by") != st(p, "maker"));
    if st(v, "strength") == "exact" && reviewed && !subject_pass {
        return Some(format!("an `exact` alignment is a coverage claim: `{status}` needs a passing review by a person in the role `subject`"));
    }
    None
}

fn covers(entry: &Value, field: &str, want: &str) -> bool {
    want.is_empty() || arr(entry, field).iter().filter_map(Value::as_str).any(|x| x == want || x == "*")
}

/// A settled record has a passing review by someone REVIEWERS.json lists for that role and that corner of the tree.
/// `lesson` is (locale, subject) for an item, which does not carry them itself.
fn reviewer_problem(r: &Refs, v: &Value, lesson: Option<(&str, &str)>) -> Option<String> {
    let status = st(v, "status");
    if !matches!(status, "reviewed" | "published") {
        return None;
    }
    let (locale, subject, framework) = match st(v, "schema") {
        "skill/1" => ("", st(v, "subject"), ""),
        "alignment/1" => ("", st(v, "skill").split('.').next().unwrap_or(""), st(v, "standard").split(':').next().unwrap_or("")),
        "lesson/1" | "path/1" => (st(v, "locale"), st(v, "subject"), ""),
        "item/1" => (lesson.map(|l| l.0).unwrap_or(""), lesson.map(|l| l.1).unwrap_or(""), ""),
        _ => ("", "", ""),
    };
    let p = &v["provenance"];
    let exact = st(v, "strength") == "exact";
    let listed = arr(p, "reviews").iter().any(|rv| {
        st(rv, "kind") == "human"
            && st(rv, "verdict") == "pass"
            && st(rv, "by") != st(p, "maker")
            && (!exact || st(rv, "role") == "subject")
            && r.reviewers.iter().any(|e| {
                st(e, "handle") == st(rv, "by")
                    && arr(e, "roles").iter().any(|x| x.as_str() == Some(st(rv, "role")))
                    && covers(e, "locales", locale)
                    && covers(e, "subjects", subject)
                    && covers(e, "frameworks", framework)
                    && st(e, "since") <= st(rv, "date")
                    && (st(e, "until").is_empty() || st(rv, "date") <= st(e, "until"))
            })
    });
    if listed {
        return None;
    }
    let mut place: Vec<String> = Vec::new();
    for (name, value) in [("locale", locale), ("subject", subject), ("framework", framework)] {
        if !value.is_empty() {
            place.push(format!("{name} {value}"));
        }
    }
    Some(format!(
        "status `{status}` needs a passing review by a person REVIEWERS.json lists for that role{}{}, on a date inside their term",
        if place.is_empty() { String::new() } else { format!(" and for {}", place.join(", ")) },
        if exact { " (role `subject` for an exact alignment)" } else { "" }
    ))
}

/// Both review gates on one record.
fn reviewed_ok(c: &mut Ctx, r: &Refs, v: &Value, path: &str, line: usize, lesson: Option<(&str, &str)>) {
    if let Some(m) = review_problem(v) {
        c.add("loop.review-before-reviewed", path, line, m);
    }
    if let Some(m) = reviewer_problem(r, v, lesson) {
        c.add("loop.reviewer-listed", path, line, m);
    }
}

/// Every string in a record, except web addresses the schema asks for.
fn all_strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| all_strings(x, out)),
        Value::Object(o) => o.iter().filter(|(k, _)| !matches!(k.as_str(), "sourceUrl" | "termsUrl" | "source")).for_each(|(_, x)| all_strings(x, out)),
        _ => {}
    }
}
fn pii(c: &mut Ctx, v: &Value, path: &str, line: usize) {
    let mut all = Vec::new();
    all_strings(v, &mut all);
    if let Some(what) = all.iter().find_map(|s| text::pii_issue(s)) {
        c.add("content.no-pii", path, line, format!("holds {what}"));
    }
}

/// A subject of the topic graph: (the languages it applies to, its (strand, topic) pairs).
type SubjectInfo = (Vec<String>, BTreeSet<(String, String)>);

struct Refs {
    locales: BTreeMap<String, String>,
    /// framework id -> terms.quote
    frameworks: BTreeMap<String, String>,
    standards: BTreeMap<String, Node>,
    /// skill id -> (subject, misconception numbers)
    skills: BTreeMap<String, (String, BTreeSet<u64>)>,
    bands: Vec<Band>,
    /// subject id -> (its locales, its (strand, topic) pairs)
    subjects: BTreeMap<String, SubjectInfo>,
    objectives: BTreeMap<String, Objective>,
    /// standard id -> the bands it names itself
    node_bands: BTreeMap<String, Vec<String>>,
    nodes: BTreeMap<String, Value>,
    locale_records: BTreeMap<String, Value>,
    signals: Vec<Signal>,
    registry: Vec<Value>,
    areas: BTreeMap<String, String>,
    protected: text::Protected,
    /// skill id -> aligned standard ids
    aligned: BTreeMap<String, BTreeSet<String>>,
    alignments: Vec<Aligned>,
    /// asset id -> (kind, has an attribution)
    assets: BTreeMap<String, (String, bool)>,
    lessons: BTreeMap<String, (String, String)>,
    /// the entries of REVIEWERS.json
    reviewers: Vec<Value>,
}

/// Scan the tree at `root` with the gate data (reading levels, licences, safety lexicons) in `data`.
pub fn scan(root: &Path, data: &Path) -> Result<Scan> {
    let mut validators = HashMap::new();
    for (id, raw) in SCHEMAS {
        let schema: Value = serde_json::from_str(raw).with_context(|| format!("schema {id}"))?;
        validators.insert(*id, jsonschema::validator_for(&schema).map_err(|e| anyhow::anyhow!("schema {id}: {e}"))?);
    }
    let load = |f: &str| -> Result<Value> {
        let p = data.join(f);
        serde_json::from_str(&fs::read_to_string(&p).with_context(|| format!("gate data {}", p.display()))?).with_context(|| format!("gate data {}", p.display()))
    };
    let bands = text::reading_bands(&load("reading-levels.json")?);
    let licences = load("licences.json")?;
    let mut lexicons: BTreeMap<String, Lexicon> = BTreeMap::new();
    for p in ls(&data.join("safety")) {
        if let Some(lang) = stem(&p, ".json") {
            let v: Value = serde_json::from_str(&fs::read_to_string(&p)?).with_context(|| format!("gate data {}", p.display()))?;
            lexicons.insert(lang, Lexicon::from_json(&v));
        }
    }

    let mut c = Ctx { root: root.to_path_buf(), validators, findings: vec![], counts: BTreeMap::new() };
    let mut r = Refs {
        locales: BTreeMap::new(),
        frameworks: BTreeMap::new(),
        standards: BTreeMap::new(),
        skills: BTreeMap::new(),
        bands: Vec::new(),
        subjects: BTreeMap::new(),
        objectives: BTreeMap::new(),
        node_bands: BTreeMap::new(),
        nodes: BTreeMap::new(),
        locale_records: BTreeMap::new(),
        signals: Vec::new(),
        registry: Vec::new(),
        areas: BTreeMap::new(),
        protected: text::Protected::from_json(&load("protected.json")?),
        aligned: BTreeMap::new(),
        alignments: Vec::new(),
        assets: BTreeMap::new(),
        lessons: BTreeMap::new(),
        reviewers: Vec::new(),
    };

    // who may review what: no file, or an empty list, means nothing can be `reviewed`
    let reviewers = root.join("REVIEWERS.json");
    if reviewers.exists() {
        if let Some(v) = c.read_json(&reviewers) {
            if c.valid("reviewers/1", &v, "REVIEWERS.json", 0) {
                r.reviewers = arr(&v, "reviewers").to_vec();
            }
        }
    }
    proposals(&mut c);
    locales(&mut c, &mut r);
    age_bands(&mut c, &mut r);
    subjects(&mut c, &mut r);
    standards(&mut c, &mut r);
    skills(&mut c, &mut r, &lexicons);
    alignments(&mut c, &mut r);
    assets(&mut c, &mut r, &licences);
    let entries = lessons(&mut c, &mut r, &bands, &lexicons);
    paths(&mut c, &r);
    signals(&mut c, &mut r);
    runs(&mut c);

    let index = build_index(&entries, &c.counts, &r);
    let mut on_disk = BTreeMap::new();
    collect_files(&root.join("index"), &root.join("index"), &mut on_disk);
    if on_disk != index {
        let stale: Vec<&String> = index.keys().filter(|k| on_disk.get(*k) != index.get(*k)).chain(on_disk.keys().filter(|k| !index.contains_key(*k))).collect();
        let first = stale.first().map(|s| s.as_str()).unwrap_or("index");
        c.add("index.fresh", &format!("index/{first}"), 0, format!("the index does not match the data ({} file(s) differ): run `openlessons index`, never edit it by hand", stale.len()));
    }
    let lessons = entries.into_iter().map(|e| e.line).collect();
    Ok(Scan {
        bands: r.bands,
        locales: r.locale_records,
        objectives: r.objectives,
        nodes: r.nodes,
        frameworks: r.frameworks,
        signals: r.signals,
        registry: r.registry,
        areas: r.areas,
        findings: c.findings,
        counts: c.counts,
        index,
        standards: r.standards,
        alignments: r.alignments,
        lessons,
    })
}

fn collect_files(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    for p in ls(dir) {
        if p.is_dir() {
            collect_files(base, &p, out);
        } else {
            let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            out.insert(rel, fs::read_to_string(&p).unwrap_or_default().replace("\r\n", "\n"));
        }
    }
}

fn stray(c: &mut Ctx, p: &Path, expected: &str) {
    if name(p) == "README.md" {
        return;
    }
    let rel = c.rel(p);
    c.add("layout.content-addressed", &rel, 0, format!("unexpected here: this folder holds {expected}"));
}

/// proposals/ is an inbox of proposal files waiting to be applied; each must at least be a well-formed proposal.
fn proposals(c: &mut Ctx) {
    for p in ls(&c.root.join("proposals")) {
        if stem(&p, ".json").is_none() {
            stray(c, &p, "proposal files (*.json)");
            continue;
        }
        let rel = c.rel(&p);
        let Some(v) = c.read_json(&p) else { continue };
        if c.valid("proposal/1", &v, &rel, 0) {
            pii(c, &v, &rel, 0);
            c.count("proposals", 1);
        }
    }
}

fn locales(c: &mut Ctx, r: &mut Refs) {
    for p in ls(&c.root.join("locales")) {
        let Some(id) = stem(&p, ".json") else {
            stray(c, &p, "<locale>.json");
            continue;
        };
        let rel = c.rel(&p);
        let Some(v) = c.read_json(&p) else { continue };
        if !c.valid("locale/1", &v, &rel, 0) {
            continue;
        }
        if st(&v, "id") != id {
            c.add("layout.content-addressed", &rel, 0, format!("the file is named {id} but its id is {}", st(&v, "id")));
        }
        r.locales.insert(st(&v, "id").to_string(), st(&v, "segmentation").to_string());
        r.locale_records.insert(st(&v, "id").to_string(), v.clone());
        c.count("locales", 1);
    }
}

/// bands/age-bands.json: the one age-band model. Bands run youngest first and never overlap.
fn age_bands(c: &mut Ctx, r: &mut Refs) {
    let p = c.root.join("bands").join("age-bands.json");
    for other in ls(&c.root.join("bands")) {
        if name(&other) != "age-bands.json" {
            stray(c, &other, "age-bands.json");
        }
    }
    if !p.exists() {
        return;
    }
    let rel = c.rel(&p);
    let Some(v) = c.read_json(&p) else { return };
    if !c.valid("age-bands/1", &v, &rel, 0) {
        return;
    }
    let mut last = -1;
    for b in arr(&v, "bands") {
        let (id, min, max) = (st(b, "id").to_string(), b["ages"]["min"].as_i64().unwrap_or(0), b["ages"]["max"].as_i64().unwrap_or(0));
        if min > max || min <= last {
            c.add("schema.valid", &rel, 0, format!("the band {id} ({min}-{max}) overlaps the one before it or runs backwards"));
        }
        let want = if id == "adult" { "adult".to_string() } else { format!("b{min:02}-{max:02}") };
        if id != want {
            c.add("layout.content-addressed", &rel, 0, format!("a band of ages {min}-{max} has the id {want}, not {id}"));
        }
        for system in b["systems"].as_object().into_iter().flatten().map(|(k, _)| k) {
            if v["systems"].get(system).is_none() {
                c.add("refs.resolve", &rel, 0, format!("the band {id} names the system {system}, which `systems` does not describe"));
            }
        }
        last = max;
        r.bands.push(Band { id, min, max });
    }
    c.count("bands", r.bands.len() as u64);
}

/// topics/<subject>.json: the strands and topics of each subject.
fn subjects(c: &mut Ctx, r: &mut Refs) {
    for p in ls(&c.root.join("topics")) {
        let Some(id) = stem(&p, ".json") else {
            stray(c, &p, "<subject>.json");
            continue;
        };
        let rel = c.rel(&p);
        let Some(v) = c.read_json(&p) else { continue };
        if !c.valid("subject/1", &v, &rel, 0) {
            continue;
        }
        if st(&v, "id") != id {
            c.add("layout.content-addressed", &rel, 0, format!("the file is named {id} but its id is {}", st(&v, "id")));
        }
        let mut locales = Vec::new();
        for l in strs(&v, "locales") {
            if l == "*" {
                locales.extend(r.locales.keys().cloned());
            } else if r.locales.contains_key(&l) {
                locales.push(l);
            } else {
                c.add("refs.resolve", &rel, 0, format!("the locale {l} is not in locales/"));
            }
        }
        locales.sort();
        locales.dedup();
        let mut pairs = BTreeSet::new();
        for s in arr(&v, "strands") {
            for t in arr(s, "topics") {
                if !pairs.insert((st(s, "id").to_string(), st(t, "id").to_string())) {
                    c.add("content.no-duplicates", &rel, 0, format!("the topic {}.{} is listed twice", st(s, "id"), st(t, "id")));
                }
            }
        }
        c.count("topics", pairs.len() as u64);
        c.count("subjects", 1);
        r.areas.insert(id.clone(), st(&v, "area").to_string());
        r.subjects.insert(id, (locales, pairs));
    }
}

/// signals/*.jsonl: aggregated usage counts. The schema allows counts only; each row must name a real cell.
fn signals(c: &mut Ctx, r: &mut Refs) {
    for p in ls(&c.root.join("signals")) {
        if stem(&p, ".jsonl").is_none() {
            stray(c, &p, "*.jsonl files of signal/1 records");
            continue;
        }
        let rel = c.rel(&p);
        for (line, v) in c.read_jsonl(&p) {
            if !c.valid("signal/1", &v, &rel, line) {
                continue;
            }
            let (objective, band, locale) = (st(&v, "objective"), st(&v, "band"), st(&v, "locale"));
            match r.objectives.get(objective) {
                None => c.add("refs.resolve", &rel, line, format!("the objective {objective} does not exist")),
                Some(o) if !o.bands.iter().any(|b| b == band) || !o.locales.iter().any(|l| l == locale) => {
                    c.add("refs.resolve", &rel, line, format!("{objective} has no cell for the band {band} and the locale {locale}"))
                }
                _ => r.signals.push(Signal { objective: objective.to_string(), band: band.to_string(), locale: locale.to_string(), count: v["count"].as_u64().unwrap_or(0) }),
            }
            c.count("signals", 1);
        }
    }
}

/// runs/: the reports of the loop's runs (<id>.json and its <id>.md).
fn runs(c: &mut Ctx) {
    for p in ls(&c.root.join("runs")) {
        if stem(&p, ".md").is_some() {
            continue;
        }
        let Some(id) = stem(&p, ".json") else {
            stray(c, &p, "run reports (<id>.json, <id>.md)");
            continue;
        };
        let rel = c.rel(&p);
        let Some(v) = c.read_json(&p) else { continue };
        if c.valid("run/1", &v, &rel, 0) {
            if st(&v, "id") != id {
                c.add("layout.content-addressed", &rel, 0, format!("the file is named {id} but the run is {}", st(&v, "id")));
            }
            pii(c, &v, &rel, 0);
            c.count("runs", 1);
        }
    }
}

/// The bands a standard is written for: its own, or those of the nearest ancestor that names any.
fn node_bands(r: &Refs, id: &str) -> Vec<String> {
    let mut at = id.to_string();
    for _ in 0..16 {
        if let Some(b) = r.node_bands.get(&at) {
            return b.clone();
        }
        match r.standards.get(&at) {
            Some(n) if !n.parent.is_empty() => at = n.parent.clone(),
            _ => break,
        }
    }
    Vec::new()
}

fn span_of(r: &Refs, bands: &[String]) -> Option<(i64, i64)> {
    let found: Vec<&Band> = r.bands.iter().filter(|b| bands.contains(&b.id)).collect();
    Some((found.iter().map(|b| b.min).min()?, found.iter().map(|b| b.max).max()?))
}

/// The gate `content.no-copied-text` on one piece of our own text.
fn copied(c: &mut Ctx, r: &Refs, t: &str, path: &str, line: usize, place: &str) {
    if let Some(m) = r.protected.issue(t) {
        c.add("content.no-copied-text", path, line, format!("{place}: {m}"));
    }
}

fn standards(c: &mut Ctx, r: &mut Refs) {
    let mut parents: Vec<(String, usize, String)> = Vec::new();
    for dir in ls(&c.root.join("standards")) {
        if !dir.is_dir() {
            stray(c, &dir, "one folder per framework");
            continue;
        }
        let fw = name(&dir);
        let fpath = dir.join("framework.json");
        let frel = c.rel(&fpath);
        let Some(f) = c.read_json(&fpath) else { continue };
        if !c.valid("framework/1", &f, &frel, 0) {
            continue;
        }
        if st(&f, "id") != fw {
            c.add("layout.content-addressed", &frel, 0, format!("the folder is {fw} but the framework id is {}", st(&f, "id")));
        }
        let terms = &f["terms"];
        let (class, quote) = (st(terms, "class"), st(terms, "quote"));
        let mut term_problem = |m: &str| c.add("standards.terms-respected", &frel, 0, m);
        if terms["reviewed"].is_null() && (class != "reference_only" || quote != "ids_only") {
            term_problem("terms nobody has read (`reviewed` is null) allow identifiers only: class reference_only, quote ids_only");
        }
        if class == "reference_only" && quote != "ids_only" {
            term_problem("a reference_only framework may be quoted by identifier only");
        }
        if class != "reference_only" && (st(terms, "notice").is_empty() || st(terms, "licence").is_empty() && class == "open") {
            term_problem("an open or attribution framework states its licence and the notice to show");
        }
        if f["span"]["ages"]["min"].as_i64() > f["span"]["ages"]["max"].as_i64() {
            c.add("schema.valid", &frel, 0, "span.ages.min is above span.ages.max");
        }
        r.registry.push(f.clone());
        r.frameworks.insert(fw.clone(), quote.to_string());
        c.count("frameworks", 1);
        for p in ls(&dir) {
            if name(&p) != "framework.json" && name(&p) != "nodes" {
                stray(c, &p, "framework.json and nodes/*.jsonl");
            }
        }
        for p in ls(&dir.join("nodes")) {
            if stem(&p, ".jsonl").is_none() {
                stray(c, &p, "*.jsonl files of standard/1 records");
                continue;
            }
            let rel = c.rel(&p);
            for (line, v) in c.read_jsonl(&p) {
                if !c.valid("standard/1", &v, &rel, line) {
                    continue;
                }
                let id = st(&v, "id").to_string();
                if st(&v, "framework") != fw || !id.starts_with(&format!("{fw}:")) {
                    c.add("layout.content-addressed", &rel, line, format!("{id} does not belong to the framework {fw}"));
                }
                if v.get("statementOfficial").is_some() && quote != "official_text" {
                    c.add("standards.terms-respected", &rel, line, format!("{id} copies the publisher's wording, which the terms of {fw} do not allow (quote: {quote})"));
                }
                if v.get("titleOfficial").is_some() && quote == "ids_only" {
                    c.add("standards.terms-respected", &rel, line, format!("{id} copies the publisher's title, which the terms of {fw} do not allow (quote: {quote})"));
                }
                if v.get("bands").is_some() {
                    for b in strs(&v, "bands") {
                        if !r.bands.iter().any(|x| x.id == b) {
                            c.add("alignment.band-matches", &rel, line, format!("{id} names the age band {b}, which bands/age-bands.json does not hold"));
                        }
                    }
                    r.node_bands.insert(id.clone(), strs(&v, "bands"));
                }
                // our own wording must be ours: the publisher's text is allowed only in the fields that say so
                for field in ["label", "statementOwn"] {
                    copied(c, r, st(&v, field), &rel, line, field);
                }
                r.nodes.insert(id.clone(), v.clone());
                let node = Node { framework: fw.clone(), kind: st(&v, "kind").to_string(), parent: st(&v, "parent").to_string() };
                if r.standards.insert(id.clone(), node).is_some() {
                    c.add("content.no-duplicates", &rel, line, format!("the standard {id} is recorded twice"));
                }
                if !st(&v, "parent").is_empty() {
                    parents.push((rel.clone(), line, st(&v, "parent").to_string()));
                }
                c.count("standards", 1);
            }
        }
    }
    for (rel, line, parent) in parents {
        if !r.standards.contains_key(&parent) {
            c.add("refs.resolve", &rel, line, format!("the parent {parent} does not exist"));
        }
    }
}

fn skills(c: &mut Ctx, r: &mut Refs, lexicons: &BTreeMap<String, Lexicon>) {
    let mut prereq: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for subject in ls(&c.root.join("skills")) {
        if !subject.is_dir() {
            stray(c, &subject, "<subject>/<strand>/<slug>.json");
            continue;
        }
        for strand in ls(&subject) {
            if !strand.is_dir() {
                stray(c, &strand, "<subject>/<strand>/<slug>.json");
                continue;
            }
            for p in ls(&strand) {
                let Some(slug) = stem(&p, ".json") else {
                    stray(c, &p, "<slug>.json");
                    continue;
                };
                let rel = c.rel(&p);
                let Some(v) = c.read_json(&p) else { continue };
                if !c.valid("skill/1", &v, &rel, 0) {
                    continue;
                }
                let id = st(&v, "id").to_string();
                let want = format!("{}.{}.{slug}", name(&subject), name(&strand));
                if id != want || format!("{}.{}.{}", st(&v, "subject"), st(&v, "strand"), st(&v, "slug")) != id {
                    c.add("layout.content-addressed", &rel, 0, format!("this file is the skill {want}, but the record says {id}"));
                }
                let nos: BTreeSet<u64> = arr(&v, "misconceptions").iter().filter_map(|m| m["no"].as_u64()).collect();
                if nos.len() != arr(&v, "misconceptions").len() {
                    c.add("content.no-duplicates", &rel, 0, "two misconceptions share a number");
                }
                reviewed_ok(c, r, &v, &rel, 0, None);
                pii(c, &v, &rel, 0);
                let mut texts: Vec<(&String, &str)> = v["statement"].as_object().into_iter().flatten().filter_map(|(l, t)| t.as_str().map(|t| (l, t))).collect();
                for m in arr(&v, "misconceptions") {
                    texts.extend(m["feedback"].as_object().into_iter().flatten().filter_map(|(l, t)| t.as_str().map(|t| (l, t))));
                }
                for (locale, t) in texts {
                    if !r.locales.contains_key(locale) {
                        c.add("refs.resolve", &rel, 0, format!("text in the locale {locale}, which locales/ does not hold"));
                    }
                    if let Some((term, cat)) = lexicons.get(text::language(locale)).and_then(|l| l.hit(t)) {
                        c.add("content.child-safe", &rel, 0, format!("'{term}' ({cat}) is on the safety lexicon"));
                    }
                }
                // placed in the topic graph: a topic that exists, bands that exist, languages that exist
                let (subject, strand, topic) = (st(&v, "subject").to_string(), st(&v, "strand").to_string(), st(&v, "topic").to_string());
                let mut applies = Vec::new();
                match r.subjects.get(&subject) {
                    None => c.add("graph.placed", &rel, 0, format!("the subject {subject} has no file under topics/: an objective hangs from a subject, a strand and a topic")),
                    Some((locales, pairs)) => {
                        if !pairs.contains(&(strand.clone(), topic.clone())) {
                            c.add("graph.placed", &rel, 0, format!("topics/{subject}.json has no topic `{topic}` in the strand `{strand}`"));
                        }
                        applies = locales.clone();
                    }
                }
                if v.get("locales").is_some() {
                    applies = strs(&v, "locales");
                    for l in &applies {
                        if !r.locales.contains_key(l) {
                            c.add("refs.resolve", &rel, 0, format!("the locale {l} is not in locales/"));
                        }
                    }
                }
                let own_bands = strs(&v, "bands");
                for b in &own_bands {
                    if !r.bands.iter().any(|x| &x.id == b) {
                        c.add("graph.placed", &rel, 0, format!("the age band {b} is not in bands/age-bands.json"));
                    }
                }
                for t in v["statement"].as_object().into_iter().flatten().filter_map(|(_, t)| t.as_str()) {
                    copied(c, r, t, &rel, 0, "statement");
                }
                r.objectives.insert(
                    id.clone(),
                    Objective { subject, strand, topic, bands: own_bands, prerequisites: strs(&v, "prerequisites"), locales: applies, record: v.clone() },
                );
                prereq.insert(id.clone(), (rel, strs(&v, "prerequisites")));
                r.skills.insert(id, (st(&v, "subject").to_string(), nos));
                c.count("skills", 1);
            }
        }
    }
    // prerequisites exist and never loop
    for (id, (rel, needs)) in &prereq {
        for n in needs {
            if !prereq.contains_key(n) {
                c.add("refs.resolve", rel, 0, format!("the prerequisite {n} does not exist"));
            }
            // a prerequisite is learned first: it cannot start in an older band than the objective that needs it
            let start = |k: &str| r.objectives.get(k).and_then(|o| span_of(r, &o.bands)).map(|s| s.0);
            if let (Some(mine), Some(theirs)) = (start(id), start(n)) {
                if theirs > mine {
                    c.add("graph.placed", rel, 0, format!("the prerequisite {n} starts at age {theirs}, after {id} (age {mine})"));
                }
            }
        }
        let (mut seen, mut todo) = (BTreeSet::new(), needs.clone());
        while let Some(n) = todo.pop() {
            if &n == id {
                c.add("refs.resolve", rel, 0, format!("{id} is its own prerequisite (a loop)"));
                break;
            }
            if seen.insert(n.clone()) {
                todo.extend(prereq.get(&n).map(|x| x.1.clone()).unwrap_or_default());
            }
        }
    }
}

fn alignments(c: &mut Ctx, r: &mut Refs) {
    for dir in ls(&c.root.join("alignments")) {
        if !dir.is_dir() {
            stray(c, &dir, "<framework>/<subject>.jsonl");
            continue;
        }
        let fw = name(&dir);
        for p in ls(&dir) {
            let Some(subject) = stem(&p, ".jsonl") else {
                stray(c, &p, "<subject>.jsonl");
                continue;
            };
            let rel = c.rel(&p);
            let mut seen = BTreeSet::new();
            for (line, v) in c.read_jsonl(&p) {
                if !c.valid("alignment/1", &v, &rel, line) {
                    continue;
                }
                let (skill, standard) = (st(&v, "skill").to_string(), st(&v, "standard").to_string());
                if !standard.starts_with(&format!("{fw}:")) || !skill.starts_with(&format!("{subject}.")) {
                    c.add("layout.content-addressed", &rel, line, format!("{skill} -> {standard} belongs in alignments/<framework of the standard>/<subject of the skill>.jsonl"));
                }
                if !seen.insert((skill.clone(), standard.clone())) {
                    c.add("content.no-duplicates", &rel, line, format!("{skill} -> {standard} is recorded twice"));
                }
                reviewed_ok(c, r, &v, &rel, line, None);
                pii(c, &v, &rel, line);
                let (has_skill, has_standard) = (r.skills.contains_key(&skill), r.standards.contains_key(&standard));
                if !has_skill {
                    c.add("refs.resolve", &rel, line, format!("the skill {skill} does not exist"));
                }
                if !has_standard {
                    c.add("refs.resolve", &rel, line, format!("the standard {standard} does not exist"));
                }
                if has_skill && has_standard && st(&v, "strength") != "related" {
                    let theirs = node_bands(r, &standard);
                    let mine = r.objectives.get(&skill).map(|o| o.bands.clone()).unwrap_or_default();
                    if !theirs.is_empty() && !mine.is_empty() && !mine.iter().any(|b| theirs.contains(b)) {
                        c.add(
                            "alignment.band-matches",
                            &rel,
                            line,
                            format!("{skill} is taught in {} but {standard} is written for {}: an `exact` or `partial` alignment shares an age band (use `related` for no coverage claim)", mine.join(", "), theirs.join(", ")),
                        );
                    }
                }
                if has_skill && has_standard {
                    r.alignments.push(Aligned { skill: skill.clone(), standard: standard.clone(), strength: st(&v, "strength").to_string(), status: st(&v, "status").to_string() });
                    r.aligned.entry(skill).or_default().insert(standard);
                }
                c.count("alignments", 1);
            }
        }
    }
}

fn assets(c: &mut Ctx, r: &mut Refs, licences: &Value) {
    let allowed = strs(licences, "allowed");
    let no_credit = strs(licences, "attributionNotRequired");
    for dir in ls(&c.root.join("assets")) {
        if !dir.is_dir() {
            stray(c, &dir, "<shard>/<asset id>.json and its file");
            continue;
        }
        let mut files: BTreeSet<String> = ls(&dir).iter().map(|p| name(p)).collect();
        for p in ls(&dir) {
            let Some(id) = stem(&p, ".json") else { continue };
            files.remove(&name(&p));
            let rel = c.rel(&p);
            let Some(v) = c.read_json(&p) else { continue };
            if !c.valid("asset/1", &v, &rel, 0) {
                continue;
            }
            files.remove(st(&v, "file"));
            match fs::read(dir.join(st(&v, "file"))) {
                Ok(bytes) => {
                    let sha = sha256_hex(&bytes);
                    if sha != st(&v, "sha256") || asset_id(&sha) != st(&v, "id") {
                        c.add("layout.content-addressed", &rel, 0, format!("the file's sha256 is {sha}: the record's sha256 and id must come from it ({})", asset_id(&sha)));
                    }
                }
                Err(_) => c.add("refs.resolve", &rel, 0, format!("the file {} is not beside the record", st(&v, "file"))),
            }
            if st(&v, "id") != id || shard(&id) != name(&dir) || !st(&v, "file").starts_with(&id) {
                c.add("layout.content-addressed", &rel, 0, format!("an asset lives at assets/{}/{}.json", shard(st(&v, "id")), st(&v, "id")));
            }
            let licence = st(&v, "licence");
            if !allowed.iter().any(|a| a == licence) {
                c.add("asset.licensed", &rel, 0, format!("the licence {licence} is not on the allowed list (gates/data/licences.json)"));
            } else if v.get("attribution").is_none() && !no_credit.iter().any(|a| a == licence) {
                c.add("asset.licensed", &rel, 0, format!("{licence} needs an attribution (title, author, source)"));
            }
            reviewed_ok(c, r, &v, &rel, 0, None);
            pii(c, &v, &rel, 0);
            r.assets.insert(id, (st(&v, "kind").to_string(), v.get("attribution").is_some()));
            c.count("assets", 1);
        }
        for f in files {
            let p = dir.join(f);
            stray(c, &p, "asset records and the files they name");
        }
    }
}

pub struct IndexEntry {
    locale: String,
    subject: String,
    line: Value,
}

fn lessons(c: &mut Ctx, r: &mut Refs, bands: &[text::ReadingBand], lexicons: &BTreeMap<String, Lexicon>) -> Vec<IndexEntry> {
    let mut entries = Vec::new();
    // across the whole tree: item id -> where; (locale, subject, signature) -> where
    let mut item_ids: HashMap<String, String> = HashMap::new();
    let mut signatures: HashMap<String, String> = HashMap::new();
    for ldir in ls(&c.root.join("lessons")) {
        if !ldir.is_dir() {
            stray(c, &ldir, "<locale>/<subject>/<shard>/<lesson id>/");
            continue;
        }
        for sdir in ls(&ldir) {
            if !sdir.is_dir() {
                stray(c, &sdir, "<subject>/<shard>/<lesson id>/");
                continue;
            }
            for shdir in ls(&sdir) {
                if !shdir.is_dir() {
                    stray(c, &shdir, "<shard>/<lesson id>/");
                    continue;
                }
                for dir in ls(&shdir) {
                    if !dir.is_dir() {
                        stray(c, &dir, "one folder per lesson");
                        continue;
                    }
                    if let Some(e) = lesson(c, r, bands, lexicons, &dir, &mut item_ids, &mut signatures) {
                        entries.push(e);
                    }
                }
            }
        }
    }
    entries
}

#[allow(clippy::too_many_arguments)]
fn lesson(
    c: &mut Ctx,
    r: &mut Refs,
    bands: &[text::ReadingBand],
    lexicons: &BTreeMap<String, Lexicon>,
    dir: &Path,
    item_ids: &mut HashMap<String, String>,
    signatures: &mut HashMap<String, String>,
) -> Option<IndexEntry> {
    for p in ls(dir) {
        if !matches!(name(&p).as_str(), "lesson.json" | "items.jsonl") {
            stray(c, &p, "lesson.json and items.jsonl");
        }
    }
    let lpath = dir.join("lesson.json");
    let rel = c.rel(&lpath);
    let v = c.read_json(&lpath)?;
    if !c.valid("lesson/1", &v, &rel, 0) {
        return None;
    }
    let (id, locale, subject, slug) = (st(&v, "id"), st(&v, "locale"), st(&v, "subject"), st(&v, "slug"));
    let parts: Vec<String> = dir.strip_prefix(&c.root).unwrap_or(dir).components().map(|x| x.as_os_str().to_string_lossy().to_string()).collect();
    let want = lesson_id(locale, subject, slug);
    let want_dir = format!("lessons/{locale}/{subject}/{}/{want}", shard(&want));
    if id != want || parts.join("/") != want_dir {
        c.add("layout.content-addressed", &rel, 0, format!("a lesson with locale {locale}, subject {subject}, slug {slug} has the id {want} and lives at {want_dir}/"));
    }
    let ages = (v["ages"]["min"].as_i64().unwrap_or(0), v["ages"]["max"].as_i64().unwrap_or(0));
    if ages.0 > ages.1 {
        c.add("schema.valid", &rel, 0, "ages.min is above ages.max");
    }
    let segmentation = match r.locales.get(locale) {
        Some(s) => s.clone(),
        None => {
            c.add("refs.resolve", &rel, 0, format!("the locale {locale} is not in locales/"));
            "space".to_string()
        }
    };
    let target = st(&v, "targetLocale");
    if !target.is_empty() && !r.locales.contains_key(target) {
        c.add("refs.resolve", &rel, 0, format!("the target locale {target} is not in locales/"));
    }
    reviewed_ok(c, r, &v, &rel, 0, None);
    pii(c, &v, &rel, 0);

    // aligned: every skill exists and maps to at least one standard
    let skills = strs(&v, "skills");
    let mut standards: BTreeSet<String> = BTreeSet::new();
    let mut misconceptions: BTreeSet<u64> = BTreeSet::new();
    for s in &skills {
        match r.skills.get(s) {
            None => c.add("refs.resolve", &rel, 0, format!("the skill {s} does not exist")),
            Some((_, nos)) => {
                misconceptions.extend(nos);
                match r.aligned.get(s) {
                    Some(a) => standards.extend(a.iter().cloned()),
                    None => c.add("lesson.aligned", &rel, 0, format!("the skill {s} is aligned to no standard: add a row under alignments/")),
                }
            }
        }
    }
    // the lesson is written for ages its objectives are taught at
    for s in &skills {
        if let Some((lo, hi)) = r.objectives.get(s).and_then(|o| span_of(r, &o.bands)) {
            if ages.1 < lo || hi < ages.0 {
                c.add("alignment.band-matches", &rel, 0, format!("the lesson is for ages {}-{} but {s} is taught at ages {lo}-{hi}", ages.0, ages.1));
            }
        }
    }
    let mut refs = Vec::new();
    item::asset_refs(&v, &mut refs);

    // the lesson's own text
    let mut shown: Vec<(String, usize, String, String)> = Vec::new(); // (file, line, place, text)
    let mut own = vec![("title", st(&v, "title").to_string()), ("teach", st(&v["teach"], "text").to_string()), ("reteach", st(&v["teach"], "reteach").to_string())];
    own.extend(strs(&v["teach"], "examples").into_iter().map(|e| ("example", e)));
    shown.extend(own.into_iter().filter(|(_, t)| !t.is_empty()).map(|(p, t)| (rel.clone(), 0, p.to_string(), t)));

    // items
    let ipath = dir.join("items.jsonl");
    let irel = c.rel(&ipath);
    let mut n_items = 0u64;
    let mut statuses: BTreeMap<String, u64> = BTreeMap::new();
    let mut last_id = String::new();
    if ipath.exists() {
        for (line, it) in c.read_jsonl(&ipath) {
            if !c.valid("item/1", &it, &irel, line) {
                continue;
            }
            n_items += 1;
            *statuses.entry(st(&it, "status").to_string()).or_default() += 1;
            let iid = st(&it, "id").to_string();
            let here = format!("{irel}:{line}");
            match item_id(&it) {
                Ok(want) if want != iid => c.add("layout.content-addressed", &irel, line, format!("the id of this content is {want} (run `openlessons stamp`)")),
                Err(e) => c.add("layout.content-addressed", &irel, line, e.to_string()),
                _ => {}
            }
            if iid < last_id {
                c.add("layout.content-addressed", &irel, line, "items.jsonl is sorted by id (run `openlessons stamp`)");
            }
            last_id = iid.clone();
            if let Some(first) = item_ids.insert(iid.clone(), here.clone()) {
                c.add("content.no-duplicates", &irel, line, format!("the same item is already at {first}"));
            } else if let Some(first) = signatures.insert(format!("{locale}#{subject}#{}", item::signature(&it)), here.clone()) {
                c.add("content.no-duplicates", &irel, line, format!("asks and accepts the same as the item at {first}"));
            }
            for s in strs(&it, "skills") {
                if !skills.contains(&s) {
                    c.add("refs.resolve", &irel, line, format!("the item's skill {s} is not one of the lesson's"));
                }
            }
            for d in arr(&it["key"], "distractors") {
                if !d["misconception"].as_u64().is_some_and(|n| misconceptions.contains(&n)) {
                    c.add("refs.resolve", &irel, line, format!("misconception {} is not on the lesson's skills", d["misconception"]));
                }
            }
            for m in item::answer_problems(&it) {
                c.add("item.answer-verified", &irel, line, m);
            }
            for m in item::leak_problems(&it) {
                c.add("item.no-answer-leak", &irel, line, m);
            }
            let prompt = &it["view"]["prompt"];
            if st(prompt, "mode") == "heard" {
                let asset = st(&prompt["audio"], "asset");
                let problem = match r.assets.get(asset) {
                    _ if asset.is_empty() => Some("a prompt that is heard, not shown, names a recorded audio asset (`audio.asset`): a cue is not a recording".to_string()),
                    None => Some(format!("the recording {asset} of this heard prompt has no record under assets/")),
                    Some((kind, _)) if kind.as_str() != "audio" => Some(format!("{asset} is not an audio asset, and this prompt is heard")),
                    Some((_, false)) => Some(format!("the recording {asset} has no attribution: a heard prompt carries the licence and the credit of its recording")),
                    _ => None,
                };
                if let Some(m) = problem {
                    c.add("item.heard-prompt-recorded", &irel, line, m);
                }
            }
            reviewed_ok(c, r, &it, &irel, line, Some((locale, subject)));
            pii(c, &it, &irel, line);
            item::asset_refs(&it, &mut refs);
            shown.extend(item::texts(&it).into_iter().map(|(p, t)| (irel.clone(), line, p, t)));
        }
    }
    c.count("lessons", 1);
    c.count("items", n_items);
    for a in refs.iter().collect::<BTreeSet<_>>() {
        if !r.assets.contains_key(a) {
            c.add("asset.licensed", &rel, 0, format!("the asset {a} has no record under assets/: a picture or sound without a licence cannot be used"));
        }
    }

    // text gates over everything a learner is shown
    let band = text::band_for(bands, ages.0);
    let languages: BTreeSet<&str> = [text::language(locale), text::language(target)].into_iter().filter(|l| !l.is_empty()).collect();
    if !lexicons.contains_key(text::language(locale)) {
        c.add("content.child-safe", &rel, 0, format!("there is no safety lexicon for {} (gates/data/safety/): add one before content in this language", text::language(locale)));
    }
    // one finding per (gate, file, text): a line repeated on a hundred items is one problem
    let mut told: BTreeSet<(u8, String)> = BTreeSet::new();
    for (file, line, place, t) in &shown {
        if let Some(m) = text::icu_issue(t) {
            c.add("schema.valid", file, *line, format!("{place}: {m}"));
        }
        if let Some(m) = r.protected.issue(t) {
            if told.insert((2, t.clone())) {
                c.add("content.no-copied-text", file, *line, format!("{place}: {m}"));
            }
        }
        if let Some(b) = band {
            if let Some(m) = text::reading_issue(t, &segmentation, b) {
                if told.insert((0, t.clone())) {
                    c.add("lesson.reading-level", file, *line, format!("{place}: {m}, for ages {}-{}: \"{t}\"", ages.0, ages.1));
                }
            }
        }
        for lang in &languages {
            if let Some((term, cat)) = lexicons.get(*lang).filter(|l| ages.0 <= l.max_age).and_then(|l| l.hit(t)) {
                if told.insert((1, t.clone())) {
                    c.add("content.child-safe", file, *line, format!("{place}: '{term}' ({cat}) is on the safety lexicon"));
                }
            }
        }
    }

    r.lessons.insert(id.to_string(), (locale.to_string(), subject.to_string()));
    let frameworks: BTreeSet<&str> = standards.iter().filter_map(|s| s.split(':').next()).collect();
    let mut line = json!({
        "id": id, "slug": slug, "title": st(&v, "title"), "locale": locale, "subject": subject,
        "ages": [ages.0, ages.1], "skills": skills, "standards": standards, "frameworks": frameworks,
        "status": st(&v, "status"), "origin": st(&v["provenance"], "origin"), "items": n_items, "itemStatus": statuses,
        "path": parts.join("/"),
    });
    if !target.is_empty() {
        line["targetLocale"] = json!(target);
    }
    Some(IndexEntry { locale: locale.to_string(), subject: subject.to_string(), line })
}

fn paths(c: &mut Ctx, r: &Refs) {
    for ldir in ls(&c.root.join("paths")) {
        if !ldir.is_dir() {
            stray(c, &ldir, "<locale>/<subject>/<slug>.json");
            continue;
        }
        for sdir in ls(&ldir) {
            if !sdir.is_dir() {
                stray(c, &sdir, "<subject>/<slug>.json");
                continue;
            }
            for p in ls(&sdir) {
                let Some(slug) = stem(&p, ".json") else {
                    stray(c, &p, "<slug>.json");
                    continue;
                };
                let rel = c.rel(&p);
                let Some(v) = c.read_json(&p) else { continue };
                if !c.valid("path/1", &v, &rel, 0) {
                    continue;
                }
                if st(&v, "slug") != slug || st(&v, "locale") != name(&ldir) || st(&v, "subject") != name(&sdir) {
                    c.add("layout.content-addressed", &rel, 0, "a path lives at paths/<its locale>/<its subject>/<its slug>.json");
                }
                for u in arr(&v, "units") {
                    for l in strs(u, "lessons") {
                        match r.lessons.get(&l) {
                            None => c.add("refs.resolve", &rel, 0, format!("the lesson {l} does not exist")),
                            Some((locale, _)) if locale != st(&v, "locale") => c.add("refs.resolve", &rel, 0, format!("the lesson {l} is in another locale ({locale})")),
                            _ => {}
                        }
                    }
                }
                reviewed_ok(c, r, &v, &rel, 0, None);
                pii(c, &v, &rel, 0);
                c.count("paths", 1);
            }
        }
    }
}

/// index/<locale>/<subject>.jsonl (one line per lesson, sorted by id) and index/catalog.json.
fn build_index(entries: &[IndexEntry], counts: &BTreeMap<String, u64>, r: &Refs) -> BTreeMap<String, String> {
    let mut files: BTreeMap<String, Vec<&IndexEntry>> = BTreeMap::new();
    for e in entries {
        files.entry(format!("{}/{}.jsonl", e.locale, e.subject)).or_default().push(e);
    }
    let mut out = BTreeMap::new();
    let mut shelves = Vec::new();
    for (file, mut list) in files {
        list.sort_by(|a, b| a.line["id"].as_str().cmp(&b.line["id"].as_str()));
        let items: u64 = list.iter().filter_map(|e| e.line["items"].as_u64()).sum();
        shelves.push(json!({"locale": list[0].locale, "subject": list[0].subject, "lessons": list.len(), "items": items, "file": format!("index/{file}")}));
        let body: String = list.iter().map(|e| format!("{}\n", e.line)).collect();
        out.insert(file, body);
    }
    // objectives.jsonl: every objective with its place in the graph, its bands, its standards and where it is taught
    let mut taught: BTreeMap<&str, BTreeMap<&str, (u64, u64)>> = BTreeMap::new();
    for e in entries {
        for s in arr(&e.line, "skills").iter().filter_map(Value::as_str) {
            let cell = taught.entry(s).or_default().entry(e.locale.as_str()).or_default();
            cell.0 += 1;
            cell.1 += e.line["items"].as_u64().unwrap_or(0);
        }
    }
    let mut objectives = String::new();
    for (id, o) in &r.objectives {
        let standards: Vec<Value> = r.alignments.iter().filter(|a| &a.skill == id).map(|a| json!({"id": a.standard, "strength": a.strength, "status": a.status})).collect();
        let lessons: BTreeMap<&str, Value> = taught.get(id.as_str()).into_iter().flatten().map(|(l, (n, items))| (*l, json!({"lessons": n, "items": items}))).collect();
        let statement: Vec<&String> = o.record["statement"].as_object().into_iter().flatten().map(|(l, _)| l).collect();
        let line = json!({
            "id": id, "subject": o.subject, "strand": o.strand, "topic": o.topic, "bands": o.bands, "prerequisites": o.prerequisites,
            "locales": o.locales, "statement": statement, "standards": standards, "taught": lessons, "status": st(&o.record, "status"),
        });
        objectives.push_str(&format!("{line}\n"));
    }
    if !r.objectives.is_empty() {
        out.insert("objectives.jsonl".to_string(), objectives);
    }
    let total = |k: &str| counts.get(k).copied().unwrap_or(0);
    let catalog = json!({
        "schema": "catalog/1",
        "note": "generated by `openlessons index`; never edit by hand",
        "counts": {
            "locales": total("locales"), "frameworks": total("frameworks"), "standards": total("standards"), "skills": total("skills"),
            "alignments": total("alignments"), "assets": total("assets"), "lessons": total("lessons"), "items": total("items"), "paths": total("paths"),
            "bands": total("bands"), "subjects": total("subjects"), "topics": total("topics"), "signals": total("signals"), "runs": total("runs"),
        },
        "shelves": shelves,
    });
    out.insert("catalog.json".to_string(), format!("{}\n", serde_json::to_string_pretty(&catalog).unwrap_or_default()));
    out
}
