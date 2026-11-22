//! `export`: one course out of the tree, in three shapes.
//!
//! - `bundle`: every record of the course in one file, as stored.
//! - `learner`: the same lessons with every `key` removed: safe to hand to a learner's device.
//! - `pack`: an app pack (`pack/1`): one path file with keyed strings beside it, the shape a lesson player loads
//!   (objectives, units, lessons, stored item envelopes with answer groups). [`crate::import`] reads it back.

use crate::canon::sha256_hex;
use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub fn st<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
pub fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}
pub fn rank(status: &str) -> u8 {
    match status {
        "proposed" => 1,
        "reviewed" => 2,
        "published" => 3,
        _ => 0,
    }
}
fn read(p: &Path) -> Result<Value> {
    serde_json::from_str(&fs::read_to_string(p).with_context(|| format!("{}", p.display()))?).with_context(|| format!("{}", p.display()))
}
fn read_lines(p: &Path) -> Result<Vec<Value>> {
    let raw = fs::read_to_string(p).with_context(|| format!("{}", p.display()))?;
    raw.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).with_context(|| format!("{}", p.display()))).collect()
}
fn ls(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir).map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
    out.sort();
    out
}

/// Everything one course refers to, read from the tree.
pub struct Course {
    pub path: Value,
    /// in course order
    pub lessons: Vec<(Value, Vec<Value>)>,
    pub skills: Vec<Value>,
    pub alignments: Vec<Value>,
    pub standards: Vec<Value>,
    pub frameworks: Vec<Value>,
}

/// Load `paths/<locale>/<subject>/<slug>.json` and what it reaches. Lessons and items below `min_status` are left out.
pub fn load(root: &Path, course: &str, min_status: &str) -> Result<Course> {
    let parts: Vec<&str> = course.split('/').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty() || p.starts_with('.')) {
        bail!("name a course as <locale>/<subject>/<slug> (see paths/)");
    }
    let path = read(&root.join("paths").join(parts[0]).join(parts[1]).join(format!("{}.json", parts[2])))?;
    let floor = rank(min_status);
    let mut lessons = Vec::new();
    let mut skill_ids: BTreeSet<String> = BTreeSet::new();
    for unit in arr(&path, "units") {
        for id in arr(unit, "lessons").iter().filter_map(Value::as_str) {
            let dir = root.join("lessons").join(parts[0]).join(parts[1]).join(id.get(4..6).unwrap_or("")).join(id);
            let lesson = read(&dir.join("lesson.json"))?;
            if rank(st(&lesson, "status")) < floor {
                continue;
            }
            let items: Vec<Value> = if dir.join("items.jsonl").exists() { read_lines(&dir.join("items.jsonl"))? } else { vec![] };
            let items: Vec<Value> = items.into_iter().filter(|i| rank(st(i, "status")) >= floor).collect();
            skill_ids.extend(arr(&lesson, "skills").iter().filter_map(Value::as_str).map(str::to_string));
            lessons.push((lesson, items));
        }
    }
    if lessons.is_empty() {
        bail!(
            "no lesson of {course} is `{min_status}` or better. Nothing is exported below the bar you set: pass --min-status proposed to take unreviewed content, and never serve that to children without an adult's review"
        );
    }
    let mut skills = Vec::new();
    for id in &skill_ids {
        let p: Vec<&str> = id.split('.').collect();
        skills.push(read(&root.join("skills").join(p[0]).join(p[1]).join(format!("{}.json", p[2])))?);
    }
    let mut alignments = Vec::new();
    let mut standard_ids: BTreeSet<String> = BTreeSet::new();
    for fw in ls(&root.join("alignments")) {
        let file = fw.join(format!("{}.jsonl", parts[1]));
        if file.exists() {
            for a in read_lines(&file)? {
                // an exact alignment is a coverage claim: below the bar it stays behind, like any unreviewed lesson
                let below = st(&a, "strength") == "exact" && rank(st(&a, "status")) < floor;
                if skill_ids.contains(st(&a, "skill")) && !below {
                    standard_ids.insert(st(&a, "standard").to_string());
                    alignments.push(a);
                }
            }
        }
    }
    alignments.sort_by_key(|a| (st(a, "skill").to_string(), st(a, "standard").to_string()));
    let (mut standards, mut frameworks) = (Vec::new(), Vec::new());
    let fw_ids: BTreeSet<&str> = standard_ids.iter().filter_map(|s| s.split(':').next()).collect();
    for fw in fw_ids {
        let dir = root.join("standards").join(fw);
        frameworks.push(read(&dir.join("framework.json"))?);
        for f in ls(&dir.join("nodes")) {
            standards.extend(read_lines(&f)?.into_iter().filter(|n| standard_ids.contains(st(n, "id"))));
        }
    }
    standards.sort_by_key(|n| st(n, "id").to_string());
    Ok(Course { path, lessons, skills, alignments, standards, frameworks })
}

const CREDIT: &str = "openlessons contributors, CC BY 4.0, https://github.com/MassRelayAI/openlessons";

pub fn bundle(c: &Course) -> Value {
    json!({
        "schema": "bundle/1", "credit": CREDIT, "path": c.path,
        "lessons": c.lessons.iter().map(|(l, items)| json!({"lesson": l, "items": items})).collect::<Vec<_>>(),
        "skills": c.skills, "alignments": c.alignments, "standards": c.standards, "frameworks": c.frameworks,
    })
}

/// One item as a learner may receive it: the view, and how many hints exist. The script of a heard prompt stays behind:
/// it is spoken from the recording, and printed it could give the answer away.
pub fn learner_item(i: &Value) -> Value {
    let mut view = i["view"].clone();
    if st(&view["prompt"], "mode") == "heard" {
        if let Some(p) = view["prompt"].as_object_mut() {
            p.remove("text");
        }
    }
    let mut o = json!({"id": i["id"], "kind": i["kind"], "band": i["band"], "role": i["role"], "view": view, "hints": arr(&i["key"], "hints").len()});
    if let Some(f) = i.get("form") {
        o["form"] = f.clone();
    }
    o
}

/// What may reach a learner's device: no key, no hint, no feedback, no provenance.
pub fn learner(c: &Course) -> Value {
    let lessons: Vec<Value> = c
        .lessons
        .iter()
        .map(|(l, items)| {
            let mut out = json!({
                "id": l["id"], "title": l["title"], "locale": l["locale"], "ages": l["ages"], "skills": l["skills"], "teach": l["teach"],
                "status": l["status"], "attribution": l["attribution"], "licence": l["licence"],
                "items": items.iter().map(learner_item).collect::<Vec<_>>(),
            });
            for k in ["image", "targetLocale"] {
                if let Some(v) = l.get(k) {
                    out[k] = v.clone();
                }
            }
            out
        })
        .collect();
    json!({
        "schema": "learner-bundle/1", "credit": CREDIT,
        "path": {"slug": c.path["slug"], "title": c.path["title"], "locale": c.path["locale"], "subject": c.path["subject"], "ages": c.path["ages"], "units": c.path["units"]},
        "lessons": lessons,
    })
}

// ---------- the app pack ----------

/// Role and form to the pool name a lesson player uses.
pub fn pool(role: &str, form: &str) -> String {
    match (role, form) {
        ("guided", "") | ("practice", "") | ("exam", "") => role.to_string(),
        ("check", f) if !f.is_empty() => format!("check_{f}"),
        ("unit_test", f) if !f.is_empty() => format!("unit_{f}"),
        ("path_test", f) if !f.is_empty() => format!("path_{f}"),
        ("placement", f) if !f.is_empty() => format!("placement_{f}"),
        (r, "") => r.to_string(),
        (r, f) => format!("{r}_{f}"),
    }
}

/// The keyed strings of a pack. A key is its use and the hash of its text, so the same text has one key.
#[derive(Default)]
struct Strings(BTreeMap<String, String>);
impl Strings {
    fn key(&mut self, usage: &str, text: &str) -> Value {
        let k = format!("{usage}.{}", &sha256_hex(text.as_bytes())[..12]);
        self.0.insert(k.clone(), text.to_string());
        Value::String(k)
    }
}

fn put(o: &mut Map<String, Value>, k: &str, v: Value) {
    o.insert(k.to_string(), v);
}

/// image/audio of our records -> imageKey / imageAltKey / audioKey on a pack object
fn media(src: &Value, out: &mut Map<String, Value>, s: &mut Strings) {
    if let Some(i) = src.get("image") {
        if !st(i, "emoji").is_empty() {
            put(out, "imageKey", s.key("img", st(i, "emoji")));
        } else {
            put(out, "imageKey", json!(format!("asset.{}", st(i, "asset"))));
        }
        if !st(i, "alt").is_empty() {
            put(out, "imageAltKey", s.key("alt", st(i, "alt")));
        }
    }
    if let Some(a) = src.get("audio") {
        let k = if !st(a, "cue").is_empty() { format!("aud.{}", st(a, "cue")) } else { format!("asset.{}", st(a, "asset")) };
        put(out, "audioKey", json!(k));
    }
}

fn choice(c: &Value, usage: &str, s: &mut Strings) -> Value {
    let mut o = Map::new();
    put(&mut o, "id", c["id"].clone());
    if !st(c, "text").is_empty() {
        put(&mut o, "textKey", s.key(usage, st(c, "text")));
    }
    media(c, &mut o, s);
    Value::Object(o)
}

fn lo(skill: &Value) -> String {
    format!("lo:{}@{}", st(skill, "id"), skill["version"].as_u64().unwrap_or(1))
}

/// The solution and the rule inputs of an answer, in the player's shape.
fn solution(kind: &str, answer: &Value) -> (Value, &'static str, Value) {
    match kind {
        "tap_choice" => (json!({"kind": kind, "selected": answer["option"]}), "Equals", json!({"optionId": answer["option"]})),
        "multi_select" => (json!({"kind": kind, "selected": answer["options"]}), "Equals", json!({"optionIds": answer["options"]})),
        "drag_match" => {
            let pairs: Vec<Value> = arr(answer, "pairs").iter().map(|p| json!({"left": p[0], "right": p[1]})).collect();
            (json!({"kind": kind, "pairs": pairs}), "MatchesPairs", json!({"pairs": pairs}))
        }
        "sort_order" => (json!({"kind": kind, "order": answer["order"]}), "IsEqualToOrdering", json!({"order": answer["order"]})),
        "tap_image" => (json!({"kind": kind, "regionId": answer["region"]}), "IsInRegion", json!({"regionId": answer["region"]})),
        _ => (json!({"kind": kind, "tiles": answer["order"]}), "IsEqualToOrdering", json!({"tiles": answer["order"]})),
    }
}

fn pack_item(it: &Value, lo_of: &BTreeMap<String, String>, s: &mut Strings) -> Value {
    let kind = st(it, "kind");
    let (view, key) = (&it["view"], &it["key"]);
    let mut o = Map::new();
    put(&mut o, "schemaVersion", json!(1));
    put(&mut o, "id", it["id"].clone());
    if let Some(f) = it.get("family") {
        put(&mut o, "family", f.clone());
    }
    put(&mut o, "kind", json!(kind));
    let objectives: Vec<Value> = arr(it, "skills").iter().filter_map(Value::as_str).map(|k| json!(lo_of.get(k).cloned().unwrap_or_else(|| format!("lo:{k}@1")))).collect();
    put(&mut o, "objectiveIds", json!(objectives));
    put(&mut o, "band", it["band"].clone());
    put(&mut o, "pool", json!(pool(st(it, "role"), st(it, "form"))));
    put(&mut o, "difficulty", json!(match st(it, "band") { "easy" => 0.3, "medium" => 0.6, _ => 0.9 }));
    let mut prompt = Map::new();
    put(&mut prompt, "textKey", s.key("prompt", st(&view["prompt"], "text")));
    media(&view["prompt"], &mut prompt, s);
    if let Some(m) = view["prompt"].get("mode") {
        put(&mut prompt, "mode", m.clone());
    }
    put(&mut o, "prompt", Value::Object(prompt));
    put(&mut o, "hints", json!(arr(key, "hints").iter().filter_map(Value::as_str).map(|h| json!({"textKey": s.key("hint", h)})).collect::<Vec<_>>()));
    let graded = kind != "story_choice";
    put(&mut o, "defaultOutcome", json!({"correct": !graded, "feedbackKey": if graded { "feedback.try_again" } else { "feedback.correct" }, "misconceptionNo": null}));
    match kind {
        "tap_choice" | "multi_select" => put(&mut o, "options", json!(arr(view, "options").iter().map(|c| choice(c, "opt", s)).collect::<Vec<_>>())),
        "drag_match" => {
            put(&mut o, "left", json!(arr(view, "left").iter().map(|c| choice(c, "opt", s)).collect::<Vec<_>>()));
            put(&mut o, "right", json!(arr(view, "right").iter().map(|c| choice(c, "opt", s)).collect::<Vec<_>>()));
        }
        "sort_order" => put(&mut o, "items", json!(arr(view, "items").iter().map(|c| choice(c, "opt", s)).collect::<Vec<_>>())),
        "word_tiles" => {
            put(&mut o, "tiles", json!(arr(view, "tiles").iter().map(|c| choice(c, "opt", s)).collect::<Vec<_>>()));
            put(&mut o, "slots", view["slots"].clone());
        }
        "tap_image" => {
            let regions: Vec<Value> = arr(view, "regions")
                .iter()
                .map(|r| {
                    let mut ro = Map::new();
                    put(&mut ro, "id", r["id"].clone());
                    for k in ["x", "y", "w", "h"] {
                        put(&mut ro, k, json!(r[k].as_f64().unwrap_or(0.0) / 1000.0));
                    }
                    if !st(r, "label").is_empty() {
                        put(&mut ro, "labelKey", s.key("label", st(r, "label")));
                    }
                    media(r, &mut ro, s);
                    Value::Object(ro)
                })
                .collect();
            put(&mut o, "regions", json!(regions));
        }
        _ => {
            put(&mut o, "startNodeId", view["start"].clone());
            let nodes: Vec<Value> = arr(view, "nodes")
                .iter()
                .map(|n| {
                    let mut no = Map::new();
                    put(&mut no, "id", n["id"].clone());
                    put(&mut no, "textKey", s.key("story", st(n, "text")));
                    media(n, &mut no, s);
                    put(&mut no, "end", n["end"].clone());
                    put(&mut no, "choices", json!(arr(n, "choices").iter().map(|c| json!({"id": c["id"], "labelKey": s.key("story", st(c, "label")), "to": c["to"]})).collect::<Vec<_>>()));
                    Value::Object(no)
                })
                .collect();
            let end = arr(view, "nodes").iter().find(|n| n["end"] == json!(true)).map(|n| n["id"].clone()).unwrap_or(Value::Null);
            put(&mut o, "nodes", json!(nodes));
            put(&mut o, "solution", json!({"kind": kind, "endNodeId": end}));
        }
    }
    let mut ext = json!({"role": it["role"], "status": it["status"], "provenance": it["provenance"]});
    if graded {
        let (sol, rule, inputs) = solution(kind, &key["answer"]);
        put(&mut o, "solution", sol);
        let mut groups = vec![json!({"rule": rule, "inputs": inputs, "outcome": {"correct": true, "feedbackKey": "feedback.correct", "misconceptionNo": null}})];
        for d in arr(key, "distractors") {
            let (_, rule, inputs) = solution(kind, &d["answer"]);
            let fb = if st(d, "feedback").is_empty() { json!("feedback.try_again") } else { s.key("feedback", st(d, "feedback")) };
            groups.push(json!({"rule": rule, "inputs": inputs, "outcome": {"correct": false, "feedbackKey": fb, "misconceptionNo": d["misconception"]}}));
        }
        put(&mut o, "answerGroups", json!(groups));
        ext["verification"] = key["verification"].clone();
        if key.get("distractors").is_some_and(|d| arr(key, "distractors").is_empty() && d.is_array()) {
            ext["emptyDistractors"] = json!(true);
        }
    }
    for k in ["form", "supersedes"] {
        if let Some(v) = it.get(k) {
            ext[k] = v.clone();
        }
    }
    if key.get("hints").is_none() {
        ext["noHints"] = json!(true);
    }
    put(&mut o, "ext", ext);
    Value::Object(o)
}

/// The pack: (path.json, strings.<locale>.json).
pub fn pack(c: &Course) -> Result<(Value, Value)> {
    let mut s = Strings::default();
    let p = &c.path;
    let locale = st(p, "locale");
    let lo_of: BTreeMap<String, String> = c.skills.iter().map(|k| (st(k, "id").to_string(), lo(k))).collect();
    let lo_ref = |id: &str| lo_of.get(id).cloned().unwrap_or_else(|| format!("lo:{id}@1"));
    let fw: BTreeMap<&str, &Value> = c.frameworks.iter().map(|f| (st(f, "id"), f)).collect();

    let standards: Vec<Value> = c
        .standards
        .iter()
        .map(|n| {
            let f = fw.get(st(n, "framework")).copied().unwrap_or(&Value::Null);
            json!({
                "lensKey": format!("lens.{}", st(n, "id")), "standardId": n["id"], "kind": n["kind"], "framework": f["title"], "publisher": f["publisher"],
                "level": n["label"], "termsClass": f["terms"]["class"], "sourceUri": n["sourceUrl"],
            })
        })
        .collect();

    let mut objectives = Vec::new();
    for k in &c.skills {
        let Some(statement) = k["statement"].get(locale).and_then(Value::as_str) else {
            bail!("the skill {} has no statement in {locale}, the language of this course", st(k, "id"));
        };
        let mut o = Map::new();
        put(&mut o, "id", json!(lo(k)));
        put(&mut o, "statementKey", s.key("objective", statement));
        if let Some(t) = k.get("type") {
            put(&mut o, "skillType", t.clone());
        }
        put(&mut o, "prerequisites", json!(arr(k, "prerequisites").iter().filter_map(Value::as_str).map(&lo_ref).collect::<Vec<_>>()));
        let mut mis = Vec::new();
        for m in arr(k, "misconceptions") {
            let Some(fb) = m["feedback"].get(locale).and_then(Value::as_str) else {
                bail!("misconception {} of {} has no feedback in {locale}", m["no"], st(k, "id"));
            };
            let mut mo = json!({"no": m["no"], "feedbackKey": s.key("feedback", fb)});
            if let Some(b) = m.get("mustAddress") {
                mo["mustBeAddressed"] = b.clone();
            }
            mis.push(mo);
        }
        put(&mut o, "misconceptions", json!(mis));
        if let Some(m) = k.get("mastery") {
            put(&mut o, "criterion", json!({"mastery_model": m["model"], "m": m["m"], "n": m["n"], "ext:scope": "single_check"}));
        }
        let aligned: Vec<Value> = c
            .alignments
            .iter()
            .filter(|a| st(a, "skill") == st(k, "id"))
            .map(|a| json!({"lensKey": format!("lens.{}", st(a, "standard")), "strength": a["strength"], "ext": {"status": a["status"], "provenance": a["provenance"]}}))
            .collect();
        put(&mut o, "alignments", json!(aligned));
        // what the pack shape has no place for, kept so the pack reads back whole
        let mut ext = json!({"status": k["status"], "provenance": k["provenance"]});
        let others = |m: &Value| -> Value { Value::Object(m.as_object().into_iter().flatten().filter(|(l, _)| l.as_str() != locale).map(|(l, t)| (l.clone(), t.clone())).collect()) };
        if k["statement"].as_object().is_some_and(|m| m.len() > 1) {
            ext["statement"] = others(&k["statement"]);
        }
        let more: Vec<Value> = arr(k, "misconceptions").iter().map(|m| others(&m["feedback"])).collect();
        if more.iter().any(|m| m.as_object().is_some_and(|o| !o.is_empty())) {
            ext["misconceptionFeedback"] = json!(more);
        }
        if k.get("misconceptions").is_none() {
            ext["noMisconceptions"] = json!(true);
        }
        put(&mut o, "ext", ext);
        objectives.push(Value::Object(o));
    }

    let by_id: BTreeMap<&str, &(Value, Vec<Value>)> = c.lessons.iter().map(|l| (st(&l.0, "id"), l)).collect();
    let slug = st(p, "slug");
    let (mut units, mut items) = (Vec::new(), Vec::new());
    for (ui, u) in arr(p, "units").iter().enumerate() {
        let mut lessons = Vec::new();
        for id in arr(u, "lessons").iter().filter_map(Value::as_str) {
            let Some((l, its)) = by_id.get(id).copied() else { continue };
            let mut o = Map::new();
            put(&mut o, "id", json!(format!("lesson:{}", st(l, "slug"))));
            put(&mut o, "seq", json!(lessons.len() + 1));
            put(&mut o, "kind", json!("lesson"));
            put(&mut o, "titleKey", s.key("lesson", st(l, "title")));
            media(l, &mut o, &mut s);
            put(&mut o, "objectiveIds", json!(arr(l, "skills").iter().filter_map(Value::as_str).map(&lo_ref).collect::<Vec<_>>()));
            let t = &l["teach"];
            let mut teach = json!({"teachKey": s.key("teach", st(t, "text")), "exampleKeys": arr(t, "examples").iter().filter_map(Value::as_str).map(|e| s.key("example", e)).collect::<Vec<_>>()});
            if !st(t, "reteach").is_empty() {
                teach["reteachKey"] = s.key("reteach", st(t, "reteach"));
            }
            put(&mut o, "teach", teach);
            let mut ext = json!({"id": l["id"], "ages": l["ages"], "status": l["status"], "licence": l["licence"], "attribution": l["attribution"], "provenance": l["provenance"]});
            for k in ["revision", "minutes", "translationOf"] {
                if let Some(v) = l.get(k) {
                    ext[k] = v.clone();
                }
            }
            if t.get("examples").is_none() {
                ext["noExamples"] = json!(true);
            }
            put(&mut o, "ext", ext);
            lessons.push(Value::Object(o));
            for i in its {
                let mut packed = pack_item(i, &lo_of, &mut s);
                packed["ext"]["lesson"] = l["id"].clone();
                items.push(packed);
            }
        }
        units.push(json!({"id": format!("unit:{slug}.{}", ui + 1), "seq": ui + 1, "titleKey": s.key("unit", st(u, "title")), "lessons": lessons, "ext": {"lessons": u["lessons"]}}));
    }

    let mut path = json!({
        "schema": "pack/1", "credit": CREDIT,
        "id": format!("path:{slug}"), "version": p.get("revision").and_then(Value::as_u64).unwrap_or(1),
        "titleKey": s.key("path", st(p, "title")), "subjectKey": format!("subject.{}", st(p, "subject")),
        "ageRange": format!("{}-{}", p["ages"]["min"], p["ages"]["max"]),
        "lensKeys": standards.iter().map(|x| x["lensKey"].clone()).collect::<Vec<_>>(),
        "standards": standards, "locale": locale,
        "units": units, "objectives": objectives, "items": items,
        "ext": {"status": p["status"], "provenance": p["provenance"], "hasRevision": p.get("revision").is_some()},
    });
    if let Some(t) = p.get("targetLocale") {
        path["targetLocale"] = t.clone();
    }
    Ok((path, json!(s.0)))
}

/// Write the chosen format into `out`; returns the files written.
pub fn write(c: &Course, format: &str, out: &Path) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(out)?;
    let pretty = |v: &Value| -> Result<String> { Ok(format!("{}\n", serde_json::to_string_pretty(v)?)) };
    let mut files = Vec::new();
    let mut save = |name: String, body: String| -> Result<()> {
        let p = out.join(name);
        fs::write(&p, body)?;
        files.push(p);
        Ok(())
    };
    match format {
        "bundle" => save("bundle.json".to_string(), pretty(&bundle(c))?)?,
        "learner" => save("learner.json".to_string(), pretty(&learner(c))?)?,
        "pack" => {
            let (path, strings) = pack(c)?;
            save("path.json".to_string(), pretty(&path)?)?;
            save(format!("strings.{}.json", st(&c.path, "locale")), pretty(&strings)?)?;
        }
        other => bail!("unknown format {other}: bundle, learner or pack"),
    }
    Ok(files)
}
