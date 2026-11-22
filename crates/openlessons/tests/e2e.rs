//! End to end over the repository itself: the gates prove themselves, the tree is clean, the index answers queries, and a
//! tampered copy is caught.

use openlessons::{find, scan, selftest, stamp, Query, RULES};
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

/// A private copy of the green fixture.
fn green_copy(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openlessons-e2e-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&root().join("gates/fixtures/green"), &dir);
    dir
}

fn items_file(tree: &Path) -> PathBuf {
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
    out.remove(0)
}

fn rules_hit(tree: &Path) -> Vec<String> {
    scan(tree, &root().join("gates/data")).unwrap().findings.iter().map(|f| f.rule.clone()).collect()
}

#[test]
fn every_gate_is_proven_on_its_fixtures() {
    let t = selftest(&root()).unwrap();
    assert!(t.ok, "{:#?}", t.lines);
    assert_eq!(t.lines.iter().filter(|l| l.starts_with("ok")).count(), RULES.len() + 1);
}

#[test]
fn the_tree_passes_every_gate() {
    let s = scan(&root(), &root().join("gates/data")).unwrap();
    assert!(s.findings.is_empty(), "{:#?}", &s.findings[..s.findings.len().min(10)]);
    assert!(s.counts["lessons"] >= 49 && s.counts["items"] >= 4000 && s.counts["skills"] >= 49);
}

#[test]
fn find_answers_from_the_index() {
    let q = |f: fn(&mut Query)| {
        let mut q = Query::default();
        f(&mut q);
        find(&root(), &q).unwrap()
    };
    assert!(q(|_| {}).len() >= 49);
    let maths = q(|q| q.subject = Some("maths".into()));
    assert!(!maths.is_empty() && maths.iter().all(|l| l["subject"] == "maths"));
    assert!(!q(|q| q.standard = Some("eng-nc-2014:y1.maths".into())).is_empty());
    assert!(!q(|q| q.framework = Some("yct".into())).is_empty());
    assert!(!q(|q| q.skill = Some("english.phonics".into())).is_empty());
    assert!(q(|q| q.age = Some(17)).is_empty());
    assert!(q(|q| q.text = Some("no-such-word-anywhere".into())).is_empty());
}

#[test]
fn an_edited_item_is_caught_until_it_is_stamped() {
    let tree = green_copy("edit");
    let file = items_file(&tree);
    let raw = fs::read_to_string(&file).unwrap();
    assert!(raw.contains("How many stars?"));
    fs::write(&file, raw.replace("How many stars?", "How many stars are there?")).unwrap();
    assert!(rules_hit(&tree).contains(&"layout.content-addressed".to_string()), "a changed item still carries its old id");
    let (_, items, changed) = stamp(&tree).unwrap();
    assert_eq!((items, changed), (8, 1));
    assert_eq!(rules_hit(&tree), Vec::<String>::new());
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn an_answer_inside_the_view_is_refused_by_the_schema() {
    let tree = green_copy("view");
    let file = items_file(&tree);
    let raw = fs::read_to_string(&file).unwrap();
    fs::write(&file, raw.replacen("\"view\":{", "\"view\":{\"answer\":{\"option\":\"b\"},", 1)).unwrap();
    assert!(rules_hit(&tree).contains(&"schema.valid".to_string()));
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn published_without_a_review_is_refused() {
    let tree = green_copy("review");
    let file = items_file(&tree);
    let raw = fs::read_to_string(&file).unwrap();
    // every item claims to be published, with no review at all
    fs::write(&file, raw.replace("\"status\":\"proposed\"", "\"status\":\"published\"")).unwrap();
    assert!(rules_hit(&tree).contains(&"loop.review-before-reviewed".to_string()));
    fs::remove_dir_all(&tree).unwrap();
}
