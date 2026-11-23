//! Export and import round trip, proposal files, the base comparison, and observations.

use openlessons::base::{against_base, DirBase};
use openlessons::canon::canonical;
use openlessons::{export, import, observe, scan, write_index};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}
fn data() -> PathBuf {
    root().join("gates/data")
}
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &to.join(e.file_name()));
        } else {
            fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}
fn green_copy(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openlessons-s2-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&root().join("gates/fixtures/green"), &dir);
    dir
}
fn canon_set(list: &[Value]) -> BTreeSet<String> {
    list.iter().map(|v| canonical(v).unwrap()).collect()
}
fn courses() -> Vec<String> {
    let mut out = Vec::new();
    for l in fs::read_dir(root().join("paths")).unwrap() {
        let l = l.unwrap().path();
        for s in fs::read_dir(&l).unwrap() {
            let s = s.unwrap().path();
            for p in fs::read_dir(&s).unwrap() {
                let p = p.unwrap().path();
                out.push(format!("{}/{}/{}", l.file_name().unwrap().to_string_lossy(), s.file_name().unwrap().to_string_lossy(), p.file_stem().unwrap().to_string_lossy()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn a_pack_reads_back_as_the_same_lessons_and_items() {
    let all = courses();
    assert!(all.len() >= 4);
    let (mut lessons, mut items) = (0, 0);
    for course in all {
        let c = export::load(&root(), &course, "draft").unwrap();
        let (pack, strings) = export::pack(&c).unwrap();
        // through text, as a consumer would receive it
        let pack: Value = serde_json::from_str(&serde_json::to_string(&pack).unwrap()).unwrap();
        let back = import::from_pack(&pack, &strings).unwrap();

        assert_eq!(canon_set(&back.skills), canon_set(&c.skills), "{course}: skills");
        assert_eq!(canon_set(&back.alignments), canon_set(&c.alignments), "{course}: alignments");
        assert_eq!(canon_set(&back.paths), canon_set(std::slice::from_ref(&c.path)), "{course}: path");
        assert_eq!(back.lessons.len(), c.lessons.len(), "{course}: lessons");
        for ((l0, i0), (l1, i1)) in c.lessons.iter().zip(&back.lessons) {
            assert_eq!(canonical(l0).unwrap(), canonical(l1).unwrap(), "{course}: lesson {}", l0["id"]);
            assert_eq!(i0.len(), i1.len(), "{course}: item count of {}", l0["id"]);
            assert_eq!(canon_set(i0), canon_set(i1), "{course}: items of {}", l0["id"]);
            lessons += 1;
            items += i0.len();
        }
    }
    assert!(lessons >= 49 && items >= 4000, "{lessons} lessons, {items} items went round");
}

#[test]
fn a_changed_pack_does_not_read_back_the_same() {
    let c = export::load(&root(), "en/maths/early-maths", "draft").unwrap();
    let (pack, mut strings) = export::pack(&c).unwrap();
    let key = pack["items"][0]["prompt"]["textKey"].as_str().unwrap().to_string();
    strings[key.as_str()] = json!("A different question?");
    let back = import::from_pack(&pack, &strings).unwrap();
    let before: BTreeSet<String> = c.lessons.iter().flat_map(|l| l.1.iter().map(|i| i["id"].as_str().unwrap().to_string())).collect();
    let after: BTreeSet<String> = back.lessons.iter().flat_map(|l| l.1.iter().map(|i| i["id"].as_str().unwrap().to_string())).collect();
    assert_ne!(before, after, "one changed prompt is one changed item id");
}

#[test]
fn a_pack_has_the_shape_a_player_loads() {
    let c = export::load(&root(), "en/maths/early-maths", "proposed").unwrap();
    let (pack, strings) = export::pack(&c).unwrap();
    assert_eq!(pack["schema"], "pack/1");
    assert_eq!(pack["ageRange"], "5-7");
    let item = &pack["items"][0];
    for k in ["id", "kind", "objectiveIds", "band", "pool", "difficulty", "prompt", "hints", "defaultOutcome", "solution"] {
        assert!(item.get(k).is_some(), "an item has `{k}`");
    }
    assert!(item["objectiveIds"][0].as_str().unwrap().starts_with("lo:maths."));
    assert!(strings.get(item["prompt"]["textKey"].as_str().unwrap()).is_some(), "every text key has its text");
    let pools: BTreeSet<&str> = pack["items"].as_array().unwrap().iter().filter_map(|i| i["pool"].as_str()).collect();
    assert!(pools.contains("check_a") && pools.contains("check_b") && pools.contains("practice"), "{pools:?}");
    assert!(pack["objectives"][0]["criterion"]["mastery_model"] == "m_of_n");
}

#[test]
fn nothing_is_exported_below_the_bar_and_a_learner_bundle_has_no_answers() {
    let err = export::load(&root(), "en/maths/early-maths", "published").err().expect("the seed is unreviewed");
    assert!(err.to_string().contains("--min-status proposed"), "{err}");
    let c = export::load(&root(), "en/maths/early-maths", "proposed").unwrap();
    let text = serde_json::to_string(&export::learner(&c)).unwrap();
    for leak in ["\"key\"", "\"answer\"", "\"distractors\"", "\"verification\"", "\"feedback\"", "\"provenance\""] {
        assert!(!text.contains(leak), "a learner bundle holds {leak}");
    }
    assert!(text.contains("\"view\""));
    let full = serde_json::to_string(&export::bundle(&c)).unwrap();
    assert!(full.contains("\"key\"") && full.contains("\"frameworks\""));
}

fn proposal() -> Value {
    json!({
        "schema": "proposal/1", "title": "Count to four",
        "provenance": {"origin": "ai", "maker": "example-agent", "model": "example-model", "created": "2026-11-23"},
        "skills": [{"id": "maths.number.count-to-four", "topic": "counting", "bands": ["b05-06"], "statement": {"en": "I can count up to four things."}, "prerequisites": ["maths.number.count-to-three"], "status": "published"}],
        "alignments": [{"skill": "maths.number.count-to-four", "standard": "example-framework:y1", "strength": "partial"}],
        "lessons": [{
            "lesson": {"slug": "count-to-four", "locale": "en", "subject": "maths", "title": "Count to four", "ages": {"min": 5, "max": 7},
                       "skills": ["maths.number.count-to-four"], "teach": {"text": "Count each thing once. One, two, three, four."}, "status": "published"},
            "items": [{"kind": "tap_choice", "band": "easy", "role": "practice",
                       "view": {"prompt": {"text": "How many moons?", "image": {"emoji": "🌙🌙🌙🌙"}}, "options": [{"id": "a", "text": "3"}, {"id": "b", "text": "4"}, {"id": "c", "text": "5"}]},
                       "key": {"graded": true, "answer": {"option": "b"}, "hints": [], "verification": {"method": "computed"}}, "status": "published"}]
        }]
    })
}

#[test]
fn a_proposal_file_lands_as_proposed_and_passes_the_gates() {
    let tree = green_copy("apply");
    let records = import::from_proposal(&proposal()).unwrap();
    assert!(records.skills.iter().chain(&records.alignments).all(|r| r["status"] == "proposed"), "a proposal cannot publish itself");
    assert!(records.lessons.iter().all(|(l, items)| l["status"] == "proposed" && items.iter().all(|i| i["status"] == "proposed" && i["provenance"]["origin"] == "ai")));
    let written = import::write_records(&tree, &records).unwrap();
    assert_eq!(written.len(), 4, "{written:?}");
    let s = scan(&tree, &data()).unwrap();
    assert!(s.findings.iter().all(|f| f.rule == "index.fresh"), "{:#?}", s.findings);
    write_index(&tree, &s).unwrap();
    assert!(scan(&tree, &data()).unwrap().findings.is_empty());
    // adding is always legal against the base
    let base = DirBase { base: root().join("gates/fixtures/green"), head: tree.clone() };
    assert!(against_base(&tree, &base).unwrap().is_empty());
    // applying it twice changes nothing
    assert!(import::write_records(&tree, &records).unwrap().is_empty());
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn a_proposal_cannot_overwrite_a_reviewed_record() {
    let tree = green_copy("overwrite");
    let mut p = proposal();
    p["lessons"][0]["lesson"]["slug"] = json!("count-to-five");
    p["lessons"][0]["lesson"]["skills"] = json!(["maths.number.count-to-five"]);
    let records = import::from_proposal(&p).unwrap();
    let err = import::write_records(&tree, &records).expect_err("the green lesson is reviewed");
    assert!(err.to_string().contains("reviewed"), "{err}");
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn a_malformed_proposal_is_refused() {
    let mut p = proposal();
    p["provenance"]["maker"] = json!("someone@example.org");
    assert!(import::from_proposal(&p).is_err(), "a maker is a handle, not an address");
    let mut p = proposal();
    p.as_object_mut().unwrap().remove("provenance");
    assert!(import::from_proposal(&p).is_err());
}

/// The lesson's items file in a tree, and its lines.
fn items(tree: &Path) -> (PathBuf, Vec<Value>) {
    fn walk(d: &Path, out: &mut Vec<PathBuf>) {
        for e in fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.file_name().unwrap() == "items.jsonl" {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&tree.join("lessons"), &mut out);
    let lines = fs::read_to_string(&out[0]).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    (out.remove(0), lines)
}
fn save(file: &Path, lines: &[Value]) {
    fs::write(file, lines.iter().map(|v| format!("{v}\n")).collect::<String>()).unwrap();
}

#[test]
fn a_reviewed_item_leaves_only_when_a_new_item_supersedes_it() {
    // the base: the green tree with its first item reviewed by a person
    let base = green_copy("base");
    let (file, mut lines) = items(&base);
    lines[0]["status"] = json!("reviewed");
    lines[0]["provenance"]["reviews"] = json!([{"by": "example-teacher", "kind": "human", "role": "subject", "verdict": "pass", "date": "2026-11-23"}]);
    save(&file, &lines);
    let old = lines[0]["id"].as_str().unwrap().to_string();

    let head = green_copy("head");
    let judge = |head: &Path| against_base(head, &DirBase { base: base.clone(), head: head.to_path_buf() }).unwrap();
    let (hfile, _) = items(&head);

    // unchanged
    save(&hfile, &lines);
    assert!(judge(&head).is_empty());
    // its metadata edited in place
    let mut edited = lines.clone();
    edited[0]["band"] = json!("hard");
    save(&hfile, &edited);
    assert_eq!(judge(&head).len(), 1);
    // removed
    save(&hfile, &lines[1..]);
    let f = judge(&head);
    assert!(f.len() == 1 && f[0].rule == "loop.base-respected" && f[0].message.contains("supersedes"), "{f:#?}");
    // revised by a new item that names it
    let mut revision = lines[1].clone();
    revision["supersedes"] = json!(old);
    let mut revised = lines[1..].to_vec();
    revised[0] = revision;
    save(&hfile, &revised);
    assert!(judge(&head).is_empty());
    // moved forward, reviews appended
    let mut forward = lines.clone();
    forward[0]["status"] = json!("published");
    forward[0]["provenance"]["reviews"].as_array_mut().unwrap().push(json!({"by": "example-editor", "kind": "human", "role": "safety", "verdict": "pass", "date": "2026-11-24"}));
    save(&hfile, &forward);
    assert!(judge(&head).is_empty());

    fs::remove_dir_all(&base).unwrap();
    fs::remove_dir_all(&head).unwrap();
}

#[test]
fn observations_cover_every_gate_and_every_level() {
    let s = scan(&root(), &data()).unwrap();
    let obs = observe::observations(&s, "0000000", false);
    assert!(obs.iter().all(|o| o["schema"] == "observation/1" && o["product"] == "openlessons" && o["version"] == "0000000" && (o.get("facts").is_some() || o.get("outcome").is_some())));
    let gates: Vec<&Value> = obs.iter().filter(|o| o["context"]["scope"] == "tree").collect();
    assert_eq!(gates.len(), openlessons::RULES.len());
    assert!(gates.iter().filter(|o| o.get("facts").is_some()).all(|o| o["facts"]["findings"] == 0));
    assert!(gates.iter().any(|o| o["rule"]["id"] == "quality.loop.base-respected" && o["outcome"] == "unmeasured"));
    let cov = |level: &str, locale: &str| obs.iter().find(|o| o["rule"]["id"] == "feature.coverage.level.taught" && o["context"]["level"] == level && o["context"]["locale"] == locale).map(|o| o["facts"].clone());
    let maths = cov("eng-nc-2014:y1.maths", "en").expect("Year 1 maths is taught in English");
    assert!(maths["lessons"].as_u64().unwrap() >= 12 && maths["objectives"].as_u64().unwrap() >= 10 && maths["objectives_reached"].as_u64().unwrap() >= 5, "{maths}");
    assert_eq!(maths["lessons_published"], 0, "nothing in the seed is reviewed");
    let ccss = cov("ccss-2010:math.k-1", "en").unwrap();
    assert!(ccss["exact_proposed"].as_u64().unwrap() >= 3 && ccss["exact_reviewed"] == 0, "{ccss}");
    assert!(cov("cefr:pre-a1", "any").is_some_and(|f| f["lessons"] == 0), "a level nothing reaches is reported once");
    // a rule file exists for every observed rule
    let rules: BTreeSet<String> = obs.iter().map(|o| o["rule"]["id"].as_str().unwrap().to_string()).collect();
    for r in rules {
        assert!(root().join("scoregraph/rules").join(format!("{r}.json")).is_file(), "scoregraph/rules/{r}.json");
    }
}
