//! `openlessons jury orders` and `jury run`: orders hold one role's checklist and never another verdict; three
//! unanimous, sourced verdicts settle a lesson whole; any objection keeps it a draft with notes; a dry run writes nothing.

use openlessons::{jury, mcp, scan};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

const LESSON_ID: &str = "les_2e620c67febffa6825275760";
const LESSON: &str = "lessons/en/maths/2e/les_2e620c67febffa6825275760/lesson.json";

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
/// The green tree with its lesson back at `draft`: no jury review, no decision.
fn draft_copy(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openlessons-juryrun-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&root().join("gates/fixtures/green"), &dir);
    fs::remove_dir_all(dir.join("review")).unwrap();
    let p = dir.join(LESSON);
    let mut l: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
    l["status"] = json!("draft");
    l["provenance"]["reviews"] = json!([]);
    fs::write(&p, serde_json::to_string_pretty(&l).unwrap()).unwrap();
    dir
}
fn s(tree: &Path) -> openlessons::Scan {
    scan(tree, &root().join("gates/data")).unwrap()
}
fn jury_entry(tree: &Path) -> Value {
    let v: Value = serde_json::from_str(&fs::read_to_string(tree.join("REVIEWERS.json")).unwrap()).unwrap();
    v["reviewers"].as_array().unwrap().iter().find(|e| e["kind"] == "jury").unwrap().clone()
}

/// What a reviewer that agrees with everything would hand back for an order: every check true, the subject's claim
/// sourced from the order's own standard.
fn verdict(order: &Value, model: &str) -> Value {
    let role = order["role"].as_str().unwrap();
    let mut records = Map::new();
    for (id, hash) in order["contentHashes"].as_object().unwrap() {
        let mut r = json!({"contentHash": hash, "verdict": "pass", "checks": order["checklist"].as_array().unwrap().iter().map(|c| (c["id"].as_str().unwrap().to_string(), json!(true))).collect::<Map<_, _>>(), "notes": []});
        if role == "subject" {
            let std = &order["standards"][0];
            r["claims"] = json!([{"claim": "the count of a group is the last number said", "registry": std["id"], "sources": [{"title": "the standard", "url": std["sourceUrl"], "accessed": "2026-11-23"}]}]);
        }
        if role == "language" {
            r["locale"] = json!("en");
        }
        records.insert(id.clone(), r);
    }
    json!({"schema": "jury-verdict/1", "role": role, "lesson": LESSON_ID, "model": model, "reviewer": format!("test/{role}"), "date": "2026-11-23", "records": records})
}

fn file_all(tree: &Path, tweak: impl Fn(&str, &mut Value)) {
    let sc = s(tree);
    let j = jury_entry(tree);
    for role in jury::ROLES {
        let order = jury::order(tree, &sc, &j, role, LESSON_ID).unwrap();
        let mut v = verdict(&order, &format!("model-{role}"));
        tweak(role, &mut v);
        let dir = tree.join(".jury/verdicts").join(role);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{LESSON_ID}.json")), v.to_string()).unwrap();
    }
}

fn opts(tree: &Path, dry: bool) -> jury::RunOptions<'_> {
    // leaked for the test's lifetime: the verdicts folder lives in the tree
    let verdicts: &'static Path = Box::leak(tree.join(".jury/verdicts").into_boxed_path());
    let data: &'static Path = Box::leak(root().join("gates/data").into_boxed_path());
    jury::RunOptions { root: tree, data, verdicts, date: "2026-11-23".into(), harness: "test".into(), independence: "separate test closures".into(), dry_run: dry, lessons: vec![] }
}

#[test]
fn an_order_holds_one_checklist_and_never_another_verdict() {
    let tree = draft_copy("orders");
    let sc = s(&tree);
    let j = jury_entry(&tree);
    // a safety verdict already filed, with a note nobody else may see
    fs::create_dir_all(tree.join(".jury/verdicts/safety")).unwrap();
    fs::write(tree.join(format!(".jury/verdicts/safety/{LESSON_ID}.json")), json!({"note": "PLANTED-SAFETY-NOTE"}).to_string()).unwrap();
    for role in jury::ROLES {
        let o = jury::order(&tree, &sc, &j, role, LESSON_ID).unwrap();
        let text = o.to_string();
        assert!(!text.contains("PLANTED-SAFETY-NOTE"), "{role}");
        assert!(!text.contains("provenance"), "{role}: who drafted it is not the jury's business");
        assert_eq!(o["checklist"].as_array().unwrap().len(), jury::checks(role).len());
        assert_eq!(o["contentHashes"].as_object().unwrap().len(), 9, "the lesson and its eight items");
    }
    assert!(jury::order(&tree, &sc, &j, "licence", LESSON_ID).is_err());
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn three_unanimous_sourced_verdicts_settle_the_lesson_whole() {
    let tree = draft_copy("pass");
    file_all(&tree, |_, _| {});

    // a dry run decides and writes nothing
    let r = jury::run(&opts(&tree, true)).unwrap();
    assert!(r.lessons[0].passed && r.written.is_empty() && !tree.join("review").exists());

    let r = jury::run(&opts(&tree, false)).unwrap();
    assert!(r.lessons[0].passed, "{:?}", r.lessons);
    assert_eq!(r.spot_checks.len(), 1, "5% of nine, rounded up");
    let after = s(&tree);
    let l = after.lessons.iter().find(|l| l["id"] == LESSON_ID).unwrap();
    assert_eq!(l["status"], "reviewed");
    assert_eq!(l["itemStatus"], json!({"reviewed": 8}));
    for rule in ["review.jury-decision", "loop.review-before-reviewed", "loop.reviewer-listed", "schema.valid", "refs.resolve"] {
        let h: Vec<_> = after.findings.iter().filter(|f| f.rule == rule).collect();
        assert!(h.is_empty(), "{rule}: {h:#?}");
    }
    let d: Value = serde_json::from_str(&fs::read_to_string(tree.join(format!("review/decisions/{LESSON_ID}.json"))).unwrap()).unwrap();
    assert_eq!(d["outcome"], "pass");
    assert_eq!(d["roles"]["safety"]["model"], "model-safety", "the model of every role is recorded");
    let pending = fs::read_dir(tree.join("review/decisions")).unwrap().filter(|e| fs::read_to_string(e.as_ref().unwrap().path()).unwrap().contains("\"pending\"")).count();
    assert_eq!(pending, 1);
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn one_objection_keeps_the_lesson_a_draft_with_notes() {
    let tree = draft_copy("object");
    file_all(&tree, |role, v| {
        if role == "language" {
            let first = v["records"].as_object().unwrap().keys().find(|k| k.starts_with("itm_")).unwrap().clone();
            v["records"][&first]["verdict"] = json!("object");
            v["records"][&first]["checks"]["natural"] = json!(false);
            v["records"][&first]["notes"] = json!(["the prompt reads as a translation"]);
        }
    });
    let r = jury::run(&opts(&tree, false)).unwrap();
    assert!(!r.lessons[0].passed);
    assert_eq!((r.lessons[0].records, r.lessons[0].passed_records), (9, 8));
    assert!(r.lessons[0].notes.iter().any(|n| n.contains("language: objects: the prompt reads as a translation")), "{:?}", r.lessons[0].notes);
    assert!(r.spot_checks.is_empty(), "nothing settled, nothing drawn");
    let after = s(&tree);
    assert_eq!(after.lessons.iter().find(|l| l["id"] == LESSON_ID).unwrap()["status"], "draft");
    assert_eq!(fs::read_dir(tree.join("review/decisions")).unwrap().count(), 9, "every decision is kept, the objections with it");
    assert!(after.findings.iter().all(|f| f.rule != "review.jury-decision"));
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn an_unsourced_pass_is_not_a_pass_and_stale_content_is_refused() {
    let tree = draft_copy("unsourced");
    file_all(&tree, |role, v| {
        if role == "subject" {
            for (_, r) in v["records"].as_object_mut().unwrap() {
                r["claims"][0]["sources"] = json!([]);
            }
        }
        if role == "safety" {
            v["records"][LESSON_ID]["contentHash"] = json!("0".repeat(64));
        }
    });
    let r = jury::run(&opts(&tree, false)).unwrap();
    assert!(!r.lessons[0].passed);
    assert_eq!(r.lessons[0].passed_records, 0);
    assert!(r.lessons[0].notes.iter().any(|n| n.contains("an unsourced claim fails")));
    assert!(r.lessons[0].notes.iter().any(|n| n.contains("safety: judged other content")));
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn a_role_waits_for_the_others_and_gives_its_verdict_once() {
    let tree = draft_copy("missing");
    file_all(&tree, |_, _| {});
    fs::remove_file(tree.join(format!(".jury/verdicts/language/{LESSON_ID}.json"))).unwrap();
    let r = jury::run(&opts(&tree, false)).unwrap();
    assert!(!r.lessons[0].passed && r.lessons[0].notes[0].contains("no verdict yet from language"));
    assert!(!tree.join("review").exists(), "an undecided lesson writes no decision");

    // over MCP: read-only refuses, and a second verdict from the same role is refused
    let v: Value = serde_json::from_str(&fs::read_to_string(tree.join(format!(".jury/verdicts/safety/{LESSON_ID}.json"))).unwrap()).unwrap();
    let call = |allow: bool, verdict: &Value| -> Value {
        let server = mcp::Server { root: tree.clone(), data: root().join("gates/data"), allow_write: allow };
        server.handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "jury_submit", "arguments": {"verdict": verdict}}})).unwrap()["result"].clone()
    };
    assert_eq!(call(false, &v)["isError"], true);
    assert_eq!(call(true, &v)["isError"], true, "already given");
    let mut lang = v.clone();
    lang["role"] = json!("language");
    assert_eq!(call(true, &lang)["isError"], false);
    assert_eq!(call(true, &lang)["isError"], true, "once");
    fs::remove_dir_all(&tree).unwrap();
}
