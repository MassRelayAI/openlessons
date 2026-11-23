//! `import`: read an app pack (`pack/1`, see [`crate::export`]) back into records, and write records into a tree.
//!
//! Export then import gives the same skills, alignments, lessons, items and path: the round trip is tested on the seed.
//! Imported records are data like any other: they pass the gates or they do not go in.

use crate::canon::{item_id, lesson_id, shard};
use crate::export::{arr, st};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Records ready to be written into a tree.
#[derive(Debug, Default)]
pub struct Records {
    pub skills: Vec<Value>,
    pub alignments: Vec<Value>,
    /// (lesson, items)
    pub lessons: Vec<(Value, Vec<Value>)>,
    pub paths: Vec<Value>,
}

struct Strings<'a>(&'a Value);
impl Strings<'_> {
    fn get(&self, key: &Value) -> Result<String> {
        let k = key.as_str().ok_or_else(|| anyhow!("a string key is missing"))?;
        self.0.get(k).and_then(Value::as_str).map(str::to_string).ok_or_else(|| anyhow!("the strings file has no text for {k}"))
    }
}

fn put(o: &mut Map<String, Value>, k: &str, v: Value) {
    o.insert(k.to_string(), v);
}

/// imageKey / imageAltKey / audioKey of a pack object -> image and audio of our records
fn media(src: &Value, out: &mut Map<String, Value>, s: &Strings) -> Result<()> {
    if let Some(k) = src.get("imageKey") {
        let key = k.as_str().unwrap_or("");
        let mut i = Map::new();
        match key.strip_prefix("asset.") {
            Some(id) => put(&mut i, "asset", json!(id)),
            None => put(&mut i, "emoji", json!(s.get(k)?)),
        }
        if let Some(a) = src.get("imageAltKey") {
            put(&mut i, "alt", json!(s.get(a)?));
        }
        put(out, "image", Value::Object(i));
    }
    if let Some(k) = src.get("audioKey").and_then(Value::as_str) {
        let a = match k.strip_prefix("asset.") {
            Some(id) => json!({"asset": id}),
            None => json!({"cue": k.strip_prefix("aud.").unwrap_or(k)}),
        };
        put(out, "audio", a);
    }
    Ok(())
}

fn choice(c: &Value, s: &Strings) -> Result<Value> {
    let mut o = Map::new();
    put(&mut o, "id", c["id"].clone());
    if let Some(k) = c.get("textKey") {
        put(&mut o, "text", json!(s.get(k)?));
    }
    media(c, &mut o, s)?;
    Ok(Value::Object(o))
}
fn choices(list: &[Value], s: &Strings) -> Result<Value> {
    Ok(Value::Array(list.iter().map(|c| choice(c, s)).collect::<Result<Vec<_>>>()?))
}

/// `lo:maths.number.count-to-20@1` -> `maths.number.count-to-20`
fn skill_of(lo: &Value) -> String {
    let s = lo.as_str().unwrap_or("");
    let s = s.strip_prefix("lo:").unwrap_or(s);
    s.rsplit_once('@').map(|(a, _)| a).unwrap_or(s).to_string()
}

/// A rule's inputs (or a solution) -> our answer shape.
fn answer(kind: &str, inputs: &Value) -> Value {
    match kind {
        "tap_choice" => json!({"option": inputs.get("optionId").or(inputs.get("selected")).cloned().unwrap_or(Value::Null)}),
        "multi_select" => json!({"options": inputs.get("optionIds").or(inputs.get("selected")).cloned().unwrap_or(Value::Null)}),
        "drag_match" => json!({"pairs": arr(inputs, "pairs").iter().map(|p| json!([p["left"], p["right"]])).collect::<Vec<_>>()}),
        "sort_order" => json!({"order": inputs["order"]}),
        "tap_image" => json!({"region": inputs["regionId"]}),
        _ => json!({"order": inputs["tiles"]}),
    }
}

fn item(p: &Value, s: &Strings) -> Result<Value> {
    let kind = st(p, "kind");
    let ext = &p["ext"];
    let mut prompt = Map::new();
    put(&mut prompt, "text", json!(s.get(&p["prompt"]["textKey"])?));
    media(&p["prompt"], &mut prompt, s)?;
    if let Some(m) = p["prompt"].get("mode") {
        put(&mut prompt, "mode", m.clone());
    }
    let mut view = Map::new();
    put(&mut view, "prompt", Value::Object(prompt));
    match kind {
        "tap_choice" | "multi_select" => put(&mut view, "options", choices(arr(p, "options"), s)?),
        "drag_match" => {
            put(&mut view, "left", choices(arr(p, "left"), s)?);
            put(&mut view, "right", choices(arr(p, "right"), s)?);
        }
        "sort_order" => put(&mut view, "items", choices(arr(p, "items"), s)?),
        "word_tiles" => {
            put(&mut view, "tiles", choices(arr(p, "tiles"), s)?);
            put(&mut view, "slots", p["slots"].clone());
        }
        "tap_image" => {
            let mut regions = Vec::new();
            for r in arr(p, "regions") {
                let mut o = Map::new();
                put(&mut o, "id", r["id"].clone());
                for k in ["x", "y", "w", "h"] {
                    put(&mut o, k, json!((r[k].as_f64().unwrap_or(0.0) * 1000.0).round() as i64));
                }
                if let Some(k) = r.get("labelKey") {
                    put(&mut o, "label", json!(s.get(k)?));
                }
                media(r, &mut o, s)?;
                regions.push(Value::Object(o));
            }
            put(&mut view, "regions", json!(regions));
        }
        "story_choice" => {
            put(&mut view, "start", p["startNodeId"].clone());
            let mut nodes = Vec::new();
            for n in arr(p, "nodes") {
                let mut o = Map::new();
                put(&mut o, "id", n["id"].clone());
                put(&mut o, "text", json!(s.get(&n["textKey"])?));
                media(n, &mut o, s)?;
                put(&mut o, "end", n["end"].clone());
                let mut cs = Vec::new();
                for c in arr(n, "choices") {
                    cs.push(json!({"id": c["id"], "label": s.get(&c["labelKey"])?, "to": c["to"]}));
                }
                put(&mut o, "choices", json!(cs));
                nodes.push(Value::Object(o));
            }
            put(&mut view, "nodes", json!(nodes));
        }
        other => bail!("item {}: unknown kind {other}", p["id"]),
    }

    let graded = kind != "story_choice";
    let mut key = Map::new();
    put(&mut key, "graded", json!(graded));
    if graded {
        put(&mut key, "answer", answer(kind, &p["solution"]));
        let mut distractors = Vec::new();
        for g in arr(p, "answerGroups") {
            let o = &g["outcome"];
            if o["correct"] == json!(true) || o["misconceptionNo"].is_null() {
                continue;
            }
            let mut d = json!({"answer": answer(kind, &g["inputs"]), "misconception": o["misconceptionNo"]});
            // a feedback key the strings file does not hold is the player's own line, not content
            if let Ok(text) = s.get(&o["feedbackKey"]) {
                d["feedback"] = json!(text);
            }
            distractors.push(d);
        }
        if !distractors.is_empty() || ext["emptyDistractors"] == json!(true) {
            put(&mut key, "distractors", json!(distractors));
        }
    }
    if ext["noHints"] != json!(true) {
        let mut hints = Vec::new();
        for h in arr(p, "hints") {
            hints.push(json!(s.get(&h["textKey"])?));
        }
        put(&mut key, "hints", json!(hints));
    }
    if graded {
        put(&mut key, "verification", ext.get("verification").cloned().ok_or_else(|| anyhow!("item {}: the pack does not say how the answer was verified (ext.verification)", p["id"]))?);
    }

    let mut o = Map::new();
    put(&mut o, "schema", json!("item/1"));
    put(&mut o, "id", json!(""));
    put(&mut o, "kind", json!(kind));
    put(&mut o, "skills", json!(arr(p, "objectiveIds").iter().map(skill_of).collect::<Vec<_>>()));
    put(&mut o, "band", p["band"].clone());
    put(&mut o, "role", ext.get("role").cloned().ok_or_else(|| anyhow!("item {}: ext.role is missing", p["id"]))?);
    for k in ["form", "supersedes"] {
        if let Some(v) = ext.get(k) {
            put(&mut o, k, v.clone());
        }
    }
    if let Some(f) = p.get("family") {
        put(&mut o, "family", f.clone());
    }
    put(&mut o, "view", Value::Object(view));
    put(&mut o, "key", Value::Object(key));
    put(&mut o, "status", ext["status"].clone());
    put(&mut o, "provenance", ext["provenance"].clone());
    let mut v = Value::Object(o);
    v["id"] = json!(item_id(&v)?);
    Ok(v)
}

/// Turn a pack (its path file and its strings) into records.
pub fn from_pack(pack: &Value, strings: &Value) -> Result<Records> {
    if st(pack, "schema") != "pack/1" {
        bail!("not a pack: `schema` must be \"pack/1\"");
    }
    let s = Strings(strings);
    let locale = st(pack, "locale");
    let subject = st(pack, "subjectKey").strip_prefix("subject.").unwrap_or("").to_string();
    let slug = st(pack, "id").strip_prefix("path:").unwrap_or("").to_string();
    if locale.is_empty() || subject.is_empty() || slug.is_empty() {
        bail!("the pack needs `locale`, `subjectKey` (subject.<name>) and `id` (path:<slug>)");
    }
    let ages: Vec<i64> = st(pack, "ageRange").split('-').filter_map(|n| n.parse().ok()).collect();
    if ages.len() != 2 {
        bail!("`ageRange` must look like 5-7");
    }
    let standard_of: BTreeMap<&str, &str> = arr(pack, "standards").iter().map(|x| (st(x, "lensKey"), st(x, "standardId"))).collect();
    let mut out = Records::default();

    for o in arr(pack, "objectives") {
        let ext = &o["ext"];
        let id = skill_of(&o["id"]);
        let parts: Vec<&str> = id.split('.').collect();
        if parts.len() != 3 {
            bail!("objective {}: expected lo:<subject>.<strand>.<slug>@<version>", o["id"]);
        }
        let version: u64 = st(o, "id").rsplit_once('@').and_then(|(_, v)| v.parse().ok()).unwrap_or(1);
        let mut statement = Map::new();
        put(&mut statement, locale, json!(s.get(&o["statementKey"])?));
        for (l, t) in ext["statement"].as_object().into_iter().flatten() {
            put(&mut statement, l, t.clone());
        }
        let mut k = Map::new();
        put(&mut k, "schema", json!("skill/1"));
        put(&mut k, "id", json!(id));
        put(&mut k, "version", json!(version));
        put(&mut k, "subject", json!(parts[0]));
        put(&mut k, "strand", json!(parts[1]));
        put(&mut k, "slug", json!(parts[2]));
        for field in ["topic", "bands", "locales"] {
            if let Some(v) = o.get(format!("ext:{field}")) {
                put(&mut k, field, v.clone());
            }
        }
        put(&mut k, "statement", Value::Object(statement));
        if let Some(t) = o.get("skillType") {
            put(&mut k, "type", t.clone());
        }
        put(&mut k, "prerequisites", json!(arr(o, "prerequisites").iter().map(skill_of).collect::<Vec<_>>()));
        if ext["noMisconceptions"] != json!(true) {
            let mut mis = Vec::new();
            for (i, m) in arr(o, "misconceptions").iter().enumerate() {
                let mut fb = Map::new();
                put(&mut fb, locale, json!(s.get(&m["feedbackKey"])?));
                for (l, t) in ext["misconceptionFeedback"][i].as_object().into_iter().flatten() {
                    put(&mut fb, l, t.clone());
                }
                let mut mo = json!({"no": m["no"], "feedback": fb});
                if let Some(b) = m.get("mustBeAddressed") {
                    mo["mustAddress"] = b.clone();
                }
                mis.push(mo);
            }
            put(&mut k, "misconceptions", json!(mis));
        }
        if let Some(c) = o.get("criterion") {
            put(&mut k, "mastery", json!({"model": c["mastery_model"], "m": c["m"], "n": c["n"]}));
        }
        put(&mut k, "status", ext["status"].clone());
        put(&mut k, "provenance", ext["provenance"].clone());
        out.skills.push(Value::Object(k));
        for a in arr(o, "alignments") {
            let standard = standard_of.get(st(a, "lensKey")).copied().ok_or_else(|| anyhow!("objective {}: the lens {} is not in `standards`", o["id"], a["lensKey"]))?;
            out.alignments.push(json!({"schema": "alignment/1", "skill": id, "standard": standard, "strength": a["strength"], "status": a["ext"]["status"], "provenance": a["ext"]["provenance"]}));
        }
    }

    let mut items_of: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for p in arr(pack, "items") {
        let it = item(p, &s)?;
        // the pack names the lesson; a pack made elsewhere is matched by the item's first skill
        let home = match p["ext"]["lesson"].as_str() {
            Some(id) => id.to_string(),
            None => arr(&it, "skills").first().and_then(Value::as_str).unwrap_or("").to_string(),
        };
        items_of.entry(home).or_default().push(it);
    }

    let mut units = Vec::new();
    for u in arr(pack, "units") {
        let mut ids = Vec::new();
        for l in arr(u, "lessons") {
            let ext = &l["ext"];
            let lslug = st(l, "id").strip_prefix("lesson:").unwrap_or("").to_string();
            let id = lesson_id(locale, &subject, &lslug);
            let skills: Vec<String> = arr(l, "objectiveIds").iter().map(skill_of).collect();
            let mut o = Map::new();
            put(&mut o, "schema", json!("lesson/1"));
            put(&mut o, "id", json!(id));
            put(&mut o, "slug", json!(lslug));
            put(&mut o, "locale", json!(locale));
            if let Some(t) = pack.get("targetLocale") {
                put(&mut o, "targetLocale", t.clone());
            }
            put(&mut o, "subject", json!(subject));
            put(&mut o, "title", json!(s.get(&l["titleKey"])?));
            put(&mut o, "ages", ext.get("ages").cloned().unwrap_or_else(|| json!({"min": ages[0], "max": ages[1]})));
            put(&mut o, "skills", json!(skills));
            let t = &l["teach"];
            let mut teach = Map::new();
            put(&mut teach, "text", json!(s.get(&t["teachKey"])?));
            if ext["noExamples"] != json!(true) {
                let mut ex = Vec::new();
                for k in arr(t, "exampleKeys") {
                    ex.push(json!(s.get(k)?));
                }
                put(&mut teach, "examples", json!(ex));
            }
            if let Some(k) = t.get("reteachKey") {
                put(&mut teach, "reteach", json!(s.get(k)?));
            }
            put(&mut o, "teach", Value::Object(teach));
            media(l, &mut o, &s)?;
            o.remove("audio");
            for k in ["revision", "minutes", "translationOf"] {
                if let Some(v) = ext.get(k) {
                    put(&mut o, k, v.clone());
                }
            }
            put(&mut o, "status", ext["status"].clone());
            put(&mut o, "licence", ext.get("licence").cloned().unwrap_or_else(|| json!("CC-BY-4.0")));
            put(&mut o, "attribution", ext.get("attribution").cloned().unwrap_or_else(|| json!("openlessons contributors")));
            // the credit of adopted material travels with the lesson, in every format
            if let Some(a) = ext.get("adopted") {
                put(&mut o, "adopted", a.clone());
            }
            put(&mut o, "provenance", ext["provenance"].clone());
            let mut items: Vec<Value> = items_of.remove(&id).unwrap_or_default();
            items.extend(skills.iter().flat_map(|k| items_of.remove(k).unwrap_or_default()));
            items.sort_by(|a, b| st(a, "id").cmp(st(b, "id")));
            out.lessons.push((Value::Object(o), items));
            ids.push(json!(id));
        }
        let listed = u["ext"].get("lessons").cloned().unwrap_or(json!(ids));
        units.push(json!({"title": s.get(&u["titleKey"])?, "lessons": listed}));
    }
    if let Some((skill, _)) = items_of.iter().next() {
        bail!("the pack has items for {skill}, but no lesson in it takes them");
    }

    let mut p = Map::new();
    put(&mut p, "schema", json!("path/1"));
    put(&mut p, "slug", json!(slug));
    put(&mut p, "locale", json!(locale));
    if let Some(t) = pack.get("targetLocale") {
        put(&mut p, "targetLocale", t.clone());
    }
    put(&mut p, "subject", json!(subject));
    put(&mut p, "title", json!(s.get(&pack["titleKey"])?));
    put(&mut p, "ages", json!({"min": ages[0], "max": ages[1]}));
    put(&mut p, "units", json!(units));
    if pack["ext"]["hasRevision"] == json!(true) {
        put(&mut p, "revision", pack["version"].clone());
    }
    put(&mut p, "status", pack["ext"]["status"].clone());
    put(&mut p, "provenance", pack["ext"]["provenance"].clone());
    out.paths.push(Value::Object(p));
    Ok(out)
}

fn pretty(v: &Value) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(v)?))
}
fn settled(v: &Value) -> bool {
    matches!(st(v, "status"), "reviewed" | "published")
}

/// Write one .json record; a reviewed record already there is never overwritten with something else.
fn place(root: &Path, rel: &str, v: &Value, written: &mut Vec<String>) -> Result<()> {
    let p = root.join(rel);
    if let Ok(raw) = fs::read_to_string(&p) {
        let old: Value = serde_json::from_str(&raw).with_context(|| rel.to_string())?;
        if &old == v {
            return Ok(());
        }
        if settled(&old) {
            bail!("{rel} is `{}` in this tree and the incoming record differs: revise it by hand (a higher version, status `proposed`)", st(&old, "status"));
        }
    }
    if let Some(d) = p.parent() {
        fs::create_dir_all(d)?;
    }
    fs::write(&p, pretty(v)?)?;
    written.push(rel.to_string());
    Ok(())
}

/// Write records into the tree at `root`: new files are added, items and alignment rows are merged into their files.
/// Returns the files written. Nothing reviewed is replaced.
pub fn write_records(root: &Path, r: &Records) -> Result<Vec<String>> {
    let mut written = Vec::new();
    for k in &r.skills {
        place(root, &format!("skills/{}/{}/{}.json", st(k, "subject"), st(k, "strand"), st(k, "slug")), k, &mut written)?;
    }
    let mut by_file: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for a in &r.alignments {
        let fw = st(a, "standard").split(':').next().unwrap_or("");
        let subject = st(a, "skill").split('.').next().unwrap_or("");
        by_file.entry(format!("alignments/{fw}/{subject}.jsonl")).or_default().push(a);
    }
    for (rel, rows) in by_file {
        let p = root.join(&rel);
        let mut all: BTreeMap<(String, String), Value> = BTreeMap::new();
        for line in fs::read_to_string(&p).unwrap_or_default().lines().filter(|l| !l.trim().is_empty()) {
            let v: Value = serde_json::from_str(line).with_context(|| rel.clone())?;
            all.insert((st(&v, "skill").to_string(), st(&v, "standard").to_string()), v);
        }
        let before = all.clone();
        for a in rows {
            let key = (st(a, "skill").to_string(), st(a, "standard").to_string());
            match all.get(&key) {
                Some(old) if old == a => {}
                Some(old) if settled(old) => bail!("{rel}: {} -> {} is `{}` in this tree and the incoming row differs", key.0, key.1, st(old, "status")),
                _ => {
                    all.insert(key, a.clone());
                }
            }
        }
        if all != before {
            if let Some(d) = p.parent() {
                fs::create_dir_all(d)?;
            }
            fs::write(&p, all.values().map(|v| format!("{v}\n")).collect::<String>())?;
            written.push(rel);
        }
    }
    for (l, items) in &r.lessons {
        let id = st(l, "id");
        let dir = format!("lessons/{}/{}/{}/{id}", st(l, "locale"), st(l, "subject"), shard(id));
        place(root, &format!("{dir}/lesson.json"), l, &mut written)?;
        let rel = format!("{dir}/items.jsonl");
        let p = root.join(&rel);
        let mut all: BTreeMap<String, Value> = BTreeMap::new();
        for line in fs::read_to_string(&p).unwrap_or_default().lines().filter(|l| !l.trim().is_empty()) {
            let v: Value = serde_json::from_str(line).with_context(|| rel.clone())?;
            all.insert(st(&v, "id").to_string(), v);
        }
        let before = all.len();
        let mut changed = false;
        for it in items {
            match all.get(st(it, "id")) {
                Some(old) if old == it => {}
                Some(old) if settled(old) => bail!("{rel}: the item {} is `{}` in this tree and the incoming copy differs", st(it, "id"), st(old, "status")),
                _ => {
                    all.insert(st(it, "id").to_string(), it.clone());
                    changed = true;
                }
            }
        }
        if changed || all.len() != before || (!p.exists() && !items.is_empty()) {
            fs::create_dir_all(root.join(&dir))?;
            fs::write(&p, all.values().map(|v| format!("{v}\n")).collect::<String>())?;
            written.push(rel);
        }
    }
    for p in &r.paths {
        place(root, &format!("paths/{}/{}/{}.json", st(p, "locale"), st(p, "subject"), st(p, "slug")), p, &mut written)?;
    }
    Ok(written)
}

// ---------- proposal files ----------

/// Turn a proposal file (`proposal/1`) into records. Whatever the file says, every record comes out `proposed`, made by
/// the proposal's maker: a proposal cannot review itself.
pub fn from_proposal(p: &Value) -> Result<Records> {
    from_proposal_as(p, "proposed")
}

/// The records of a proposal at the status its way in gives them: `proposed` through `apply`, `draft` through the loop.
/// Never anything higher: a proposal cannot review itself.
pub fn from_proposal_as(p: &Value, status: &str) -> Result<Records> {
    if !matches!(status, "draft" | "proposed") {
        bail!("a proposal lands as `draft` or `proposed`, never `{status}`");
    }
    if st(p, "schema") != "proposal/1" {
        bail!("not a proposal: `schema` must be \"proposal/1\"");
    }
    let schema: Value = serde_json::from_str(include_str!("../../../schemas/proposal.schema.json"))?;
    let validator = jsonschema::validator_for(&schema).map_err(|e| anyhow!("proposal schema: {e}"))?;
    if let Some(e) = validator.iter_errors(p).next() {
        bail!("the proposal does not match schemas/proposal.schema.json: {e} at {}", e.instance_path());
    }
    let prov = &p["provenance"];
    if !matches!(st(prov, "origin"), "human" | "ai" | "generator") || st(prov, "maker").is_empty() || st(prov, "created").is_empty() {
        bail!("a proposal says who made it: provenance.origin (human, ai or generator), provenance.maker and provenance.created");
    }
    let mut maker = prov.clone();
    if let Some(o) = maker.as_object_mut() {
        o.remove("reviews");
    }
    let mut full = maker.clone();
    full["reviews"] = json!([]);
    let stamp = |v: &Value, schema: &str, provenance: &Value| -> Value {
        let mut o = v.as_object().cloned().unwrap_or_default();
        put(&mut o, "schema", json!(schema));
        put(&mut o, "status", json!(status));
        put(&mut o, "provenance", provenance.clone());
        Value::Object(o)
    };
    let mut out = Records::default();
    for k in arr(p, "skills") {
        let mut v = stamp(k, "skill/1", &full);
        let id = st(&v, "id").to_string();
        let parts: Vec<&str> = id.split('.').collect();
        if parts.len() != 3 {
            bail!("a skill id looks like <subject>.<strand>.<slug>, not '{id}'");
        }
        for (name, part) in ["subject", "strand", "slug"].iter().zip(&parts) {
            v[*name] = json!(part);
        }
        if v.get("version").is_none() {
            v["version"] = json!(1);
        }
        if v.get("prerequisites").is_none() {
            v["prerequisites"] = json!([]);
        }
        out.skills.push(v);
    }
    for a in arr(p, "alignments") {
        out.alignments.push(stamp(a, "alignment/1", &full));
    }
    for entry in arr(p, "lessons") {
        let mut l = stamp(&entry["lesson"], "lesson/1", &full);
        let id = lesson_id(st(&l, "locale"), st(&l, "subject"), st(&l, "slug"));
        l["id"] = json!(id);
        if l.get("licence").is_none() {
            l["licence"] = json!("CC-BY-4.0");
        }
        if l.get("attribution").is_none() {
            l["attribution"] = json!("openlessons contributors");
        }
        let mut items = Vec::new();
        for it in arr(entry, "items") {
            let mut v = stamp(it, "item/1", &maker);
            if v.get("skills").is_none() {
                v["skills"] = l["skills"].clone();
            }
            v["id"] = json!(item_id(&v)?);
            items.push(v);
        }
        items.sort_by(|a, b| st(a, "id").cmp(st(b, "id")));
        out.lessons.push((l, items));
    }
    if out.skills.is_empty() && out.alignments.is_empty() && out.lessons.is_empty() {
        bail!("the proposal holds nothing: give `skills`, `alignments` or `lessons`");
    }
    Ok(out)
}
