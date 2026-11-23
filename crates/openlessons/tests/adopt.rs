//! Open material (OER) adopted from a registered source: the licence of each item decides, never the site's.
//!
//! Red first: each refusal below was seen to pass before the gate `content.source-licensed` and the adopted-text rules of
//! `content.no-copied-text` existed.

use openlessons::{canon, scan};
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
    let dir = std::env::temp_dir().join(format!("openlessons-adopt-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&root().join("gates/fixtures/green"), &dir);
    dir
}
/// Findings other than the index (which these tests do not regenerate), as "rule: message".
fn hits(tree: &Path) -> Vec<String> {
    scan(tree, &root().join("gates/data")).unwrap().findings.iter().filter(|f| f.rule != "index.fresh").map(|f| format!("{}: {}", f.rule, f.message)).collect()
}
fn rule_hit(tree: &Path, rule: &str) -> bool {
    hits(tree).iter().any(|h| h.starts_with(&format!("{rule}:")))
}

fn write_source(tree: &Path, id: &str, class: &str, licences: &[&str]) {
    let s = json!({
        "schema": "source/1", "id": id, "title": format!("Source {id}"), "publisher": "Example Publisher",
        "url": "https://example.org/",
        "class": class, "licences": licences, "perItem": "each item's page states its own licence",
        "terms": {"url": "https://example.org/licence", "read": "2026-11-23", "quotation": "Our material is openly licensed."},
    });
    fs::create_dir_all(tree.join("sources")).unwrap();
    fs::write(tree.join(format!("sources/{id}.json")), serde_json::to_string_pretty(&s).unwrap()).unwrap();
}

fn adopted(source: &str, licence: &str, licence_url: &str) -> Value {
    json!({
        "source": source, "title": "Counting Hens", "sourceUrl": "https://example.org/items/counting-hens",
        "licence": licence, "licenceUrl": licence_url, "authors": ["Ada Writer"],
        "attribution": "\"Counting Hens\" by Ada Writer, \u{a9} Example Publisher 2020, released under CC BY 4.0.",
        "changes": "shortened; practice items added", "retrieved": "2026-11-23",
    })
}

/// An adopted lesson on the green tree's skill, with the text it takes from its source.
fn write_lesson(tree: &Path, slug: &str, teach: &str, adopted: Option<Value>) -> PathBuf {
    let id = canon::lesson_id("en", "maths", slug);
    let mut l = json!({
        "schema": "lesson/1", "id": id, "slug": slug, "locale": "en", "subject": "maths", "title": "Count the hens",
        "ages": {"min": 5, "max": 7}, "skills": ["maths.number.count-to-five"], "teach": {"text": teach},
        "status": "draft", "licence": "CC-BY-4.0", "attribution": "Adapted from \"Counting Hens\" by Ada Writer, CC BY 4.0; openlessons contributors",
        "provenance": {"origin": "generator", "maker": "openlessons-adopt", "created": "2026-11-23"},
    });
    if let Some(a) = adopted {
        l["adopted"] = a;
    }
    let dir = tree.join(format!("lessons/en/maths/{}/{id}", canon::shard(&id)));
    fs::create_dir_all(&dir).unwrap();
    let p = dir.join("lesson.json");
    fs::write(&p, serde_json::to_string_pretty(&l).unwrap()).unwrap();
    p
}
fn edit(p: &Path, f: impl Fn(&mut Value)) {
    let mut v: Value = serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap();
    f(&mut v);
    fs::write(p, serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

const HENS: &str = "The little red hen counts her chicks in the yard every morning.";
const BY: &str = "https://creativecommons.org/licenses/by/4.0/";

#[test]
fn adopted_cc_by_text_with_its_attribution_passes_every_gate() {
    let tree = green_copy("ok");
    write_source(&tree, "example-open", "adopt", &["CC-BY-4.0"]);
    write_lesson(&tree, "counting-hens", HENS, Some(adopted("example-open", "CC-BY-4.0", BY)));
    // the attribution carries the publisher's copyright sign: credit is not copying
    assert_eq!(hits(&tree), Vec::<String>::new());
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn an_item_under_a_noncommercial_licence_is_refused() {
    let tree = green_copy("nc");
    write_source(&tree, "example-open", "adopt", &["CC-BY-4.0", "CC-BY-NC-4.0"]);
    write_lesson(&tree, "counting-hens", HENS, Some(adopted("example-open", "CC-BY-NC-4.0", "https://creativecommons.org/licenses/by-nc/4.0/")));
    assert!(rule_hit(&tree, "content.source-licensed"), "{:?}", hits(&tree));
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn an_item_under_a_noderivatives_licence_is_refused() {
    let tree = green_copy("nd");
    write_source(&tree, "example-open", "adopt", &["CC-BY-3.0", "CC-BY-ND-3.0"]);
    write_lesson(&tree, "counting-hens", HENS, Some(adopted("example-open", "CC-BY-ND-3.0", "https://creativecommons.org/licenses/by-nd/3.0/")));
    assert!(rule_hit(&tree, "content.source-licensed"), "{:?}", hits(&tree));
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn an_item_missing_its_attribution_is_refused() {
    for field in ["attribution", "authors", "licenceUrl", "sourceUrl"] {
        let tree = green_copy(&format!("noattr-{field}"));
        write_source(&tree, "example-open", "adopt", &["CC-BY-4.0"]);
        let mut a = adopted("example-open", "CC-BY-4.0", BY);
        a.as_object_mut().unwrap().remove(field);
        write_lesson(&tree, "counting-hens", HENS, Some(a));
        assert!(rule_hit(&tree, "content.source-licensed"), "without {field}: {:?}", hits(&tree));
        fs::remove_dir_all(&tree).unwrap();
    }
    // a lesson credit that names nobody from the source
    let tree = green_copy("noattr-lesson");
    write_source(&tree, "example-open", "adopt", &["CC-BY-4.0"]);
    let p = write_lesson(&tree, "counting-hens", HENS, Some(adopted("example-open", "CC-BY-4.0", BY)));
    edit(&p, |l| l["attribution"] = json!("openlessons contributors"));
    assert!(rule_hit(&tree, "content.source-licensed"), "{:?}", hits(&tree));
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn a_share_alike_item_headed_for_the_core_is_refused() {
    // from a share-alike source
    let tree = green_copy("sa");
    write_source(&tree, "example-wiki", "sa-layer", &["CC-BY-SA-4.0"]);
    write_lesson(&tree, "counting-hens", HENS, Some(adopted("example-wiki", "CC-BY-SA-4.0", "https://creativecommons.org/licenses/by-sa/4.0/")));
    assert!(rule_hit(&tree, "content.source-licensed"), "{:?}", hits(&tree));
    fs::remove_dir_all(&tree).unwrap();
    // and a share-alike item of a mixed source that is otherwise adoptable
    let tree = green_copy("sa-mixed");
    write_source(&tree, "example-open", "adopt", &["CC-BY-4.0", "CC-BY-SA-4.0"]);
    write_lesson(&tree, "counting-hens", HENS, Some(adopted("example-open", "CC-BY-SA-4.0", "https://creativecommons.org/licenses/by-sa/4.0/")));
    assert!(rule_hit(&tree, "content.source-licensed"), "{:?}", hits(&tree));
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn a_reference_only_source_gives_codes_and_links_never_content() {
    let tree = green_copy("ref");
    write_source(&tree, "example-closed", "reference-only", &["CC-BY-NC-SA-4.0"]);
    write_lesson(&tree, "counting-hens", HENS, Some(adopted("example-closed", "CC-BY-4.0", BY)));
    assert!(rule_hit(&tree, "content.source-licensed"), "{:?}", hits(&tree));
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn an_unregistered_source_a_wrong_licence_url_or_a_foreign_host_is_refused() {
    let tree = green_copy("unreg");
    write_lesson(&tree, "counting-hens", HENS, Some(adopted("nowhere", "CC-BY-4.0", BY)));
    assert!(rule_hit(&tree, "content.source-licensed"), "{:?}", hits(&tree));
    fs::remove_dir_all(&tree).unwrap();

    let tree = green_copy("badurl");
    write_source(&tree, "example-open", "adopt", &["CC-BY-4.0"]);
    write_lesson(&tree, "counting-hens", HENS, Some(adopted("example-open", "CC-BY-4.0", "https://creativecommons.org/licenses/by-nc/4.0/")));
    assert!(rule_hit(&tree, "content.source-licensed"), "{:?}", hits(&tree));
    fs::remove_dir_all(&tree).unwrap();

    let tree = green_copy("host");
    write_source(&tree, "example-open", "adopt", &["CC-BY-4.0"]);
    let mut a = adopted("example-open", "CC-BY-4.0", BY);
    a["sourceUrl"] = json!("https://elsewhere.example.com/counting-hens");
    write_lesson(&tree, "counting-hens", HENS, Some(a));
    assert!(rule_hit(&tree, "content.source-licensed"), "{:?}", hits(&tree));
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn a_source_record_whose_class_contradicts_its_licences_is_refused() {
    // NonCommercial material registered as adoptable
    let tree = green_copy("src-class");
    write_source(&tree, "example-closed", "adopt", &["CC-BY-NC-SA-4.0"]);
    assert!(rule_hit(&tree, "content.source-licensed"), "{:?}", hits(&tree));
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn adopted_text_without_its_attribution_is_copied_text() {
    let tree = green_copy("copied");
    write_source(&tree, "example-open", "adopt", &["CC-BY-4.0"]);
    write_lesson(&tree, "counting-hens", HENS, Some(adopted("example-open", "CC-BY-4.0", BY)));
    // the same sentence in a lesson that does not say where it came from
    write_lesson(&tree, "hens-again", &format!("Read this: {HENS}"), None);
    assert!(rule_hit(&tree, "content.no-copied-text"), "{:?}", hits(&tree));
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn an_attribution_naming_a_noncommercial_licence_is_refused_as_copied_text() {
    let tree = green_copy("nc-mark");
    write_source(&tree, "example-open", "adopt", &["CC-BY-4.0"]);
    let mut a = adopted("example-open", "CC-BY-4.0", BY);
    a["attribution"] = json!("\"Counting Hens\" by Ada Writer, released under CC BY-NC 4.0.");
    write_lesson(&tree, "counting-hens", HENS, Some(a));
    assert!(rule_hit(&tree, "content.no-copied-text"), "{:?}", hits(&tree));
    fs::remove_dir_all(&tree).unwrap();
}
