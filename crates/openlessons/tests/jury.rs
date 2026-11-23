//! review.jury-decision: a lesson or an item is `reviewed` only by a jury decision record that matches its content, is
//! unanimous, and sources every correctness claim. Each case breaks one thing in a copy of the green tree.

use openlessons::scan;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const RULE: &str = "review.jury-decision";
const LESSON: &str = "lessons/en/maths/2e/les_2e620c67febffa6825275760/lesson.json";
const DECISION: &str = "review/decisions/les_2e620c67febffa6825275760.json";

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
    let dir = std::env::temp_dir().join(format!("openlessons-jury-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&root().join("gates/fixtures/green"), &dir);
    dir
}
fn hits(tree: &Path, rule: &str) -> Vec<String> {
    scan(tree, &root().join("gates/data")).unwrap().findings.iter().filter(|f| f.rule == rule).map(|f| format!("{}: {}", f.path, f.message)).collect()
}
fn edit(tree: &Path, file: &str, f: impl Fn(&mut Value)) {
    let p = tree.join(file);
    let mut v: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
    f(&mut v);
    fs::write(&p, serde_json::to_string_pretty(&v).unwrap()).unwrap();
}
/// One broken copy: the rule fires, and the message says why.
fn refused(name: &str, why: &str, f: impl Fn(&Path)) {
    let tree = green_copy(name);
    f(&tree);
    let h = hits(&tree, RULE);
    assert!(h.iter().any(|m| m.contains(why)), "{name}: expected a finding containing {why:?}, got {h:#?}");
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn the_green_lesson_is_settled_by_its_decision() {
    let tree = green_copy("green");
    assert_eq!(hits(&tree, RULE), Vec::<String>::new());
    for rule in ["loop.review-before-reviewed", "loop.reviewer-listed", "refs.resolve", "schema.valid"] {
        assert_eq!(hits(&tree, rule), Vec::<String>::new(), "{rule}");
    }
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn reviewed_without_a_decision_record_is_refused() {
    refused("no-file", "no decision record", |t| fs::remove_file(t.join(DECISION)).unwrap());
    // a person's pass alone no longer settles a lesson: only the jury does
    refused("human-only", "given only by the jury", |t| {
        edit(t, LESSON, |l| {
            let r = l["provenance"]["reviews"].as_array_mut().unwrap();
            r.retain(|e| e["kind"] != "jury");
        })
    });
    // a jury review that points somewhere else
    refused("elsewhere", "the decision of", |t| {
        edit(t, LESSON, |l| {
            for e in l["provenance"]["reviews"].as_array_mut().unwrap() {
                if e["kind"] == "jury" {
                    e["decision"] = json!("review/decisions/les_000000000000000000000000.json");
                }
            }
        })
    });
}

#[test]
fn a_decision_that_is_not_unanimous_is_refused() {
    for role in ["subject", "safety", "language"] {
        refused(&format!("object-{role}"), &format!("{role}: objects"), |t| edit(t, DECISION, |d| d["roles"][role]["verdict"] = json!("object")));
    }
    refused("false-check", "check `age-fit` is false", |t| edit(t, DECISION, |d| d["roles"]["safety"]["checks"]["age-fit"] = json!(false)));
    refused("unanswered-check", "check `culture` was not answered", |t| {
        edit(t, DECISION, |d| {
            d["roles"]["language"]["checks"].as_object_mut().unwrap().remove("culture");
        })
    });
    refused("outcome-draft", "outcome is `draft`", |t| edit(t, DECISION, |d| d["outcome"] = json!("draft")));
    refused("wrong-locale", "judged the locale `id`", |t| edit(t, DECISION, |d| d["roles"]["language"]["locale"] = json!("id")));
}

#[test]
fn an_unsourced_claim_or_a_source_that_does_not_resolve_is_refused() {
    refused("no-claims", "an unsourced pass is not a pass", |t| edit(t, DECISION, |d| d["roles"]["subject"]["claims"] = json!([])));
    refused("no-sources", "an unsourced claim fails", |t| edit(t, DECISION, |d| d["roles"]["subject"]["claims"][0]["sources"] = json!([])));
    refused("no-access-date", "has no access date", |t| {
        edit(t, DECISION, |d| {
            d["roles"]["subject"]["claims"][0]["sources"][0].as_object_mut().unwrap().remove("accessed");
        })
    });
    refused("read-later", "not a date on or before the decision", |t| edit(t, DECISION, |d| d["roles"]["subject"]["claims"][0]["sources"][0]["accessed"] = json!("2027-01-01")));
    refused("closed-licence", "neither a web address", |t| edit(t, DECISION, |d| d["roles"]["subject"]["claims"][0]["sources"] = json!([{"title": "a textbook", "licence": "all-rights-reserved"}])));
    refused("unknown-entry", "is not a standard in standards/", |t| edit(t, DECISION, |d| d["roles"]["subject"]["claims"][0]["registry"] = json!("example-framework:y9.z9")));
    // a real entry, but not one this lesson's skill is aligned to
    refused("unaligned-entry", "is not aligned to a skill of this record", |t| {
        let nodes = t.join("standards/example-framework/nodes/y1.jsonl");
        let mut raw = fs::read_to_string(&nodes).unwrap();
        raw.push_str(&format!("{}\n", json!({"schema":"standard/1","id":"example-framework:y2","framework":"example-framework","kind":"level","code":"Y2","subject":"maths","label":"Year 2","sourceUrl":"https://example.org/framework/y2","bands":["b05-06"]})));
        fs::write(&nodes, raw).unwrap();
        edit(t, DECISION, |d| d["roles"]["subject"]["claims"][0]["registry"] = json!("example-framework:y2"));
    });
    // a source cited by an open licence resolves without an address
    let tree = green_copy("by-licence");
    edit(&tree, DECISION, |d| d["roles"]["subject"]["claims"][0]["sources"] = json!([{"title": "an open textbook", "licence": "CC-BY-4.0"}]));
    assert_eq!(hits(&tree, RULE), Vec::<String>::new());
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn a_decision_lapses_when_the_content_or_the_policy_moves() {
    refused("content-changed", "the decision lapsed", |t| edit(t, LESSON, |l| l["teach"]["reteach"] = json!("Point at each one as you say its number.")));
    refused("policy-moved", "the jury is at version 2", |t| {
        edit(t, "REVIEWERS.json", |r| {
            for e in r["reviewers"].as_array_mut().unwrap() {
                if e["kind"] == "jury" {
                    e["policyVersion"] = json!("2");
                }
            }
        })
    });
    refused("jury-unlisted", "is not listed in REVIEWERS.json", |t| {
        edit(t, "REVIEWERS.json", |r| r["reviewers"].as_array_mut().unwrap().retain(|e| e["kind"] != "jury"))
    });
    refused("model-missing", "the model is not recorded", |t| edit(t, DECISION, |d| d["roles"]["safety"]["model"] = json!(" ")));
}

#[test]
fn a_decision_names_a_record_of_the_tree() {
    let tree = green_copy("orphan");
    let d: Value = serde_json::from_str(&fs::read_to_string(tree.join(DECISION)).unwrap()).unwrap();
    let mut o = d.clone();
    o["record"] = json!("itm_000000000000000000000000");
    fs::write(tree.join("review/decisions/itm_000000000000000000000000.json"), o.to_string()).unwrap();
    let h = hits(&tree, "refs.resolve");
    assert!(h.iter().any(|m| m.contains("no lesson or item of the tree")), "{h:#?}");
    fs::remove_dir_all(&tree).unwrap();
}
