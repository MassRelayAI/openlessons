//! The reviewer list and heard prompts.

use openlessons::{export, import, scan};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
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
    let dir = std::env::temp_dir().join(format!("openlessons-ra-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&root().join("gates/fixtures/green"), &dir);
    dir
}
fn hits(tree: &Path, rule: &str) -> Vec<String> {
    scan(tree, &root().join("gates/data")).unwrap().findings.iter().filter(|f| f.rule == rule).map(|f| format!("{}: {}", f.path, f.message)).collect()
}
fn reviewers(tree: &Path, f: impl Fn(&mut Value)) {
    let file = tree.join("REVIEWERS.json");
    let mut v: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    f(&mut v["reviewers"][0]);
    fs::write(&file, v.to_string()).unwrap();
}

#[test]
fn the_shipped_list_names_no_person_and_only_the_jury_settles_lessons() {
    let v: Value = serde_json::from_str(&fs::read_to_string(root().join("REVIEWERS.json")).unwrap()).unwrap();
    // people are named by the project owner, not by a change that also adds content; the jury is the owner's decision K4
    let list = v["reviewers"].as_array().unwrap();
    assert_eq!(list.len(), 1, "{list:?}");
    assert_eq!((list[0]["kind"].as_str(), list[0]["id"].as_str(), list[0]["owner"].as_str()), (Some("jury"), Some("jury-v1"), Some("MassRelay")));
    let s = scan(&root(), &root().join("gates/data")).unwrap();
    // a lesson above `proposed` is there only by a jury decision record the gate accepts
    for l in s.lessons.iter().filter(|l| l["status"] != "proposed" && l["status"] != "draft") {
        assert_eq!(l["status"], "reviewed", "{l}");
        assert!(root().join(format!("review/decisions/{}.json", l["id"].as_str().unwrap())).is_file(), "{l}");
    }
    assert!(s.findings.iter().all(|f| f.rule != "review.jury-decision" && f.rule != "loop.reviewer-listed"), "{:?}", s.findings);
}

#[test]
fn a_review_counts_only_from_a_listed_reviewer_in_scope() {
    const RULE: &str = "loop.reviewer-listed";
    // the green tree: a lesson and an exact alignment reviewed by a listed teacher
    let tree = green_copy("listed");
    assert_eq!(hits(&tree, RULE), Vec::<String>::new());

    // nobody listed
    fs::write(tree.join("REVIEWERS.json"), r#"{"schema":"reviewers/1","reviewers":[]}"#).unwrap();
    assert_eq!(hits(&tree, RULE).len(), 2, "the lesson and the alignment");
    fs::remove_dir_all(&tree).unwrap();

    // no file at all is the same as nobody
    let tree = green_copy("nofile");
    fs::remove_file(tree.join("REVIEWERS.json")).unwrap();
    assert_eq!(hits(&tree, RULE).len(), 2);
    fs::remove_dir_all(&tree).unwrap();

    // listed for another language: the alignment still stands, and so does the lesson, which the jury settles
    let tree = green_copy("locale");
    reviewers(&tree, |r| r["locales"] = json!(["id"]));
    assert_eq!(hits(&tree, RULE), Vec::<String>::new());
    fs::remove_dir_all(&tree).unwrap();

    // listed for another framework: the lesson stands, the alignment does not
    let tree = green_copy("framework");
    reviewers(&tree, |r| r["frameworks"] = json!(["another-framework"]));
    let h = hits(&tree, RULE);
    assert!(h.len() == 1 && h[0].contains("alignments/") && h[0].contains("framework example-framework"), "{h:#?}");
    fs::remove_dir_all(&tree).unwrap();

    // listed without the role the review was given in
    let tree = green_copy("role");
    reviewers(&tree, |r| r["roles"] = json!(["licence"]));
    assert_eq!(hits(&tree, RULE).len(), 1, "the alignment; the lesson is the jury's");
    fs::remove_dir_all(&tree).unwrap();

    // a review dated before the reviewer's term began
    let tree = green_copy("term");
    reviewers(&tree, |r| r["since"] = json!("2027-01-01"));
    assert_eq!(hits(&tree, RULE).len(), 1);
    reviewers(&tree, |r| {
        r["since"] = json!("2026-01-01");
        r["until"] = json!("2026-06-30");
    });
    assert_eq!(hits(&tree, RULE).len(), 1, "and after it ended");
    reviewers(&tree, |r| r["locales"] = json!(["*"]));
    reviewers(&tree, |r| r["until"] = json!("2027-12-31"));
    assert_eq!(hits(&tree, RULE), Vec::<String>::new());
    fs::remove_dir_all(&tree).unwrap();
}

/// The items file of the green copy and its lines.
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

#[test]
fn a_heard_prompt_needs_a_recording_with_its_credit() {
    const RULE: &str = "item.heard-prompt-recorded";
    let tree = green_copy("heard");
    assert_eq!(hits(&tree, RULE), Vec::<String>::new());
    assert_eq!(hits(&tree, "item.no-answer-leak"), Vec::<String>::new(), "the script of a heard prompt is not printed, so it leaks nothing");
    let (file, lines) = items(&tree);
    let at = lines.iter().position(|i| i["view"]["prompt"]["mode"] == "heard").expect("the green tree has a heard prompt");
    let sound = lines[at]["view"]["prompt"]["audio"]["asset"].as_str().unwrap().to_string();
    let save = |f: &dyn Fn(&mut Value)| {
        let mut l = lines.clone();
        f(&mut l[at]);
        l[at]["id"] = json!(openlessons::canon::item_id(&l[at]).unwrap());
        l.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        fs::write(&file, l.iter().map(|v| format!("{v}\n")).collect::<String>()).unwrap();
    };

    // a cue is a wish for a recording, not a recording
    save(&|i| i["view"]["prompt"]["audio"] = json!({"cue": "word.cat"}));
    assert_eq!(hits(&tree, RULE).len(), 1);
    // no audio at all
    save(&|i| {
        i["view"]["prompt"].as_object_mut().unwrap().remove("audio");
    });
    assert_eq!(hits(&tree, RULE).len(), 1);
    // shown instead of heard: now the printed prompt gives the answer away
    save(&|i| {
        i["view"]["prompt"].as_object_mut().unwrap().remove("mode");
    });
    assert_eq!(hits(&tree, RULE).len(), 0);
    assert_eq!(hits(&tree, "item.no-answer-leak").len(), 1);
    // a picture is not a recording
    let picture = fs::read_dir(tree.join("assets")).unwrap().flat_map(|d| fs::read_dir(d.unwrap().path()).unwrap()).map(|e| e.unwrap().path()).find(|p| p.extension().is_some_and(|x| x == "svg")).unwrap();
    let picture_id = picture.file_stem().unwrap().to_string_lossy().to_string();
    save(&|i| i["view"]["prompt"]["audio"] = json!({"asset": picture_id}));
    assert!(hits(&tree, RULE).iter().any(|h| h.contains("not an audio asset")));
    // the recording loses its attribution
    save(&|_| {});
    let record = fs::read_dir(tree.join("assets")).unwrap().flat_map(|d| fs::read_dir(d.unwrap().path()).unwrap()).map(|e| e.unwrap().path()).find(|p| p.file_name().unwrap().to_string_lossy() == format!("{sound}.json")).unwrap();
    let mut v: Value = serde_json::from_str(&fs::read_to_string(&record).unwrap()).unwrap();
    v.as_object_mut().unwrap().remove("attribution");
    v["licence"] = json!("CC0-1.0");
    fs::write(&record, v.to_string()).unwrap();
    assert!(hits(&tree, RULE).iter().any(|h| h.contains("no attribution")), "a recording is credited whatever its licence");
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn a_heard_prompt_reaches_the_learner_without_its_script_and_survives_a_pack() {
    let c = export::load(&root().join("gates/fixtures/green"), "en/maths/first-counting", "draft").unwrap();
    let heard = |v: &Value| -> Vec<Value> {
        v["lessons"][0]["items"].as_array().unwrap().iter().filter(|i| i["view"]["prompt"]["mode"] == "heard").cloned().collect()
    };
    let learner = heard(&export::learner(&c));
    assert_eq!(learner.len(), 1);
    assert!(learner[0]["view"]["prompt"].get("text").is_none(), "the script stays behind");
    assert!(learner[0]["view"]["prompt"]["audio"]["asset"].is_string());
    let (pack, strings) = export::pack(&c).unwrap();
    let back = import::from_pack(&pack, &strings).unwrap();
    let ids = |items: &[Value]| -> Vec<String> { items.iter().map(|i| i["id"].as_str().unwrap().to_string()).collect() };
    assert_eq!(ids(&back.lessons[0].1), ids(&c.lessons[0].1));
}

#[test]
fn the_proposal_inbox_holds_only_well_formed_proposals() {
    let tree = green_copy("inbox");
    fs::create_dir_all(tree.join("proposals")).unwrap();
    fs::write(tree.join("proposals/a.json"), r#"{"schema":"proposal/1","title":"x","provenance":{"origin":"ai","maker":"example-agent","created":"2026-11-23"},"skills":[{"id":"maths.number.count-to-six","statement":{"en":"I can count to six."}}]}"#).unwrap();
    assert_eq!(hits(&tree, "schema.valid"), Vec::<String>::new());
    fs::write(tree.join("proposals/b.json"), r#"{"schema":"proposal/1","title":"x"}"#).unwrap();
    let h = hits(&tree, "schema.valid");
    assert!(!h.is_empty() && h.iter().all(|f| f.starts_with("proposals/b.json")), "{h:#?}");
    fs::remove_dir_all(&tree).unwrap();
}
