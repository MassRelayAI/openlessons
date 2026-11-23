//! Two failure classes of the first jury run, caught before the jury:
//! - item.hint-fits: a hint that points at a picture the item does not show, or speaks of "both" on an item with three or
//!   more choices (gates/data/hints.json holds the words, per language);
//! - item.objective-listed: an item that tests an objective its lesson does not list.
//!
//! Both judge drafts and settled content; a `proposed` lesson meets them when it is put before the jury.

use openlessons::{jury, scan};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const LESSON_ID: &str = "les_2e620c67febffa6825275760";
const DIR: &str = "lessons/en/maths/2e/les_2e620c67febffa6825275760";

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
    let dir = std::env::temp_dir().join(format!("openlessons-hints-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&root().join("gates/fixtures/green"), &dir);
    dir
}
fn hits(tree: &Path, rule: &str) -> Vec<String> {
    scan(tree, &root().join("gates/data")).unwrap().findings.iter().filter(|f| f.rule == rule).map(|f| format!("{}:{}: {}", f.path, f.line, f.message)).collect()
}
/// Change the first item that `pick` accepts, restamp its id, keep the file sorted.
fn change_item(tree: &Path, pick: impl Fn(&Value) -> bool, f: impl Fn(&mut Value)) {
    let file = tree.join(DIR).join("items.jsonl");
    let mut items: Vec<Value> = fs::read_to_string(&file).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let at = items.iter().position(&pick).expect("the green lesson has such an item");
    f(&mut items[at]);
    items[at]["id"] = json!(openlessons::canon::item_id(&items[at]).unwrap());
    items.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    fs::write(&file, items.iter().map(|v| format!("{v}\n")).collect::<String>()).unwrap();
}
fn no_image(v: &Value) -> bool {
    !v["view"].to_string().contains("\"image\"")
}
fn choices(v: &Value) -> usize {
    ["options", "tiles", "right", "left"].iter().map(|k| v["view"][k].as_array().map(Vec::len).unwrap_or(0)).max().unwrap()
}
fn set_status(tree: &Path, status: &str) {
    let p = tree.join(DIR).join("lesson.json");
    let mut l: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
    l["status"] = json!(status);
    l["provenance"]["reviews"] = json!([]);
    fs::write(&p, serde_json::to_string_pretty(&l).unwrap()).unwrap();
}

#[test]
fn the_green_tree_passes_both() {
    let tree = green_copy("green");
    assert_eq!(hits(&tree, "item.hint-fits"), Vec::<String>::new());
    assert_eq!(hits(&tree, "item.objective-listed"), Vec::<String>::new());
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn a_hint_that_points_at_a_missing_picture_is_refused() {
    let tree = green_copy("picture");
    change_item(&tree, no_image, |it| it["key"]["hints"] = json!(["Look at the picture carefully."]));
    let h = hits(&tree, "item.hint-fits");
    assert!(h.len() == 1 && h[0].contains("picture") && h[0].contains("shows none"), "{h:#?}");
    fs::remove_dir_all(&tree).unwrap();

    // the same hint where the item shows one is fine
    let tree = green_copy("picture-shown");
    change_item(&tree, |v| !no_image(v), |it| it["key"]["hints"] = json!(["Look at the picture carefully."]));
    assert_eq!(hits(&tree, "item.hint-fits"), Vec::<String>::new());
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn a_hint_about_both_on_three_choices_is_refused() {
    let tree = green_copy("both");
    change_item(&tree, |v| choices(v) >= 3 && v["kind"] == "tap_choice", |it| it["key"]["hints"] = json!(["Count both groups."]));
    let h = hits(&tree, "item.hint-fits");
    assert!(h.len() == 1 && h[0].contains("both") && h[0].contains("3 choices"), "{h:#?}");
    fs::remove_dir_all(&tree).unwrap();

    let tree = green_copy("both-two");
    change_item(&tree, |v| choices(v) == 2, |it| it["key"]["hints"] = json!(["Count both groups."]));
    assert_eq!(hits(&tree, "item.hint-fits"), Vec::<String>::new());
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn an_item_on_an_objective_the_lesson_does_not_list_is_refused() {
    let tree = green_copy("objective");
    change_item(&tree, |v| v["kind"] == "tap_choice", |it| it["skills"] = json!(["maths.number.count-to-three"]));
    let h = hits(&tree, "item.objective-listed");
    assert!(h.len() == 1 && h[0].contains("maths.number.count-to-three"), "{h:#?}");
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn a_proposed_lesson_meets_the_hint_gate_before_the_jury() {
    let tree = green_copy("proposed");
    set_status(&tree, "proposed");
    fs::remove_dir_all(tree.join("review")).unwrap();
    change_item(&tree, no_image, |it| it["key"]["hints"] = json!(["Look at the picture carefully."]));
    // the seed is not judged by it until it is put before the jury ...
    assert_eq!(hits(&tree, "item.hint-fits"), Vec::<String>::new());
    // ... and there it is refused, before any reviewer spends a verdict on it
    let s = scan(&tree, &root().join("gates/data")).unwrap();
    let j = json!({"id": "jury-v1", "kind": "jury", "owner": "MassRelay", "policy": "review/jury-policy.md", "policyVersion": "1"});
    let err = jury::order(&tree, &s, &j, "subject", LESSON_ID).expect_err("refused before the jury").to_string();
    assert!(err.contains("item.hint-fits") && err.contains("picture"), "{err}");
    // a draft is judged at once
    set_status(&tree, "draft");
    assert_eq!(hits(&tree, "item.hint-fits").len(), 1);
    fs::remove_dir_all(&tree).unwrap();
}
