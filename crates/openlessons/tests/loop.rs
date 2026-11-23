//! The content loop: the gap report, work orders, landing drafts, and what a draft may never do.

use openlessons::gaps::{self, Cell};
use openlessons::looprun::{self, Land, Mode};
use openlessons::{mcp, scan};
use serde_json::{json, Value};
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
/// A copy of the green fixture, and an empty folder for a run's drafts beside it.
fn green_copy(name: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("openlessons-loop-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    copy_tree(&root().join("gates/fixtures/green"), &base.join("tree"));
    fs::create_dir_all(base.join("run/drafts")).unwrap();
    (base.join("tree"), base.join("run"))
}
fn done(tree: &Path) {
    fs::remove_dir_all(tree.parent().unwrap()).unwrap();
}

/// A draft for the one gap of the green tree: count to three, ages 5 to 6, in English.
fn draft() -> Value {
    let item = |n: usize, thing: &str, role: &str, form: Option<&str>| {
        let answer = ["a", "b", "c"][n - 1];
        let mut i = json!({
            "kind": "tap_choice", "band": "easy", "role": role,
            "view": {"prompt": {"text": "How many can you see?", "image": {"emoji": thing.repeat(n)}}, "options": [{"id": "a", "text": "1"}, {"id": "b", "text": "2"}, {"id": "c", "text": "3"}]},
            "key": {"graded": true, "answer": {"option": answer}, "hints": ["Touch each one as you count."], "verification": {"method": "constructed"}},
        });
        if let Some(f) = form {
            i["form"] = json!(f);
        }
        i
    };
    json!({
        "schema": "proposal/1", "title": "Count to three",
        "provenance": {"origin": "ai", "maker": "example-agent", "model": "example-model", "created": "2026-11-23"},
        "lessons": [{
            "lesson": {"slug": "count-to-three-b05-06", "locale": "en", "subject": "maths", "title": "Count to three", "ages": {"min": 5, "max": 6}, "skills": ["maths.number.count-to-three"],
                "teach": {"text": "Count each thing once. One, two, three.", "examples": ["two is 2"], "reteach": "Touch each one as you say its number."}},
            "items": [item(1, "🍎", "guided", None), item(2, "🍎", "practice", None), item(3, "🍎", "practice", None), item(1, "🐟", "practice", None), item(2, "🐟", "practice", None),
                item(3, "🐟", "check", Some("a")), item(2, "🌙", "check", Some("b")), item(3, "🌙", "practice", None)],
        }],
    })
}
fn land(tree: &Path, run: &Path) -> Value {
    looprun::land(&Land { root: tree, data: &data(), from: run, mode: Mode::Files, date: "2026-11-23".into(), gaps: gaps::Options::default(), failed: vec![] }).unwrap()
}
fn put(run: &Path, name: &str, v: &Value) {
    fs::write(run.join("drafts").join(name), v.to_string()).unwrap();
}

#[test]
fn the_gap_report_lists_the_cell_with_no_lesson_and_ranks_by_the_standards() {
    let (tree, _) = green_copy("gaps");
    let r = gaps::report(&scan(&tree, &data()).unwrap(), &gaps::Options::default());
    // two objectives x one band x one language; one has a lesson with eight items
    assert_eq!((r.cells, r.filled, r.gaps.len()), (2, 1, 1));
    let g = &r.gaps[0];
    assert_eq!((g.id().as_str(), g.kind), ("maths.number.count-to-three@b05-06@en", "no-lesson"));
    // one framework of the registry covers maths at these ages (1), and we hold one partial alignment (4)
    assert_eq!((g.frameworks, g.demand), (1, 5));
    // the filled cell is reviewed, so a child-facing product may read it
    assert_eq!(r.served, 1);
    // ask for more items than the lesson has and its cell is a gap again
    let strict = gaps::report(&scan(&tree, &data()).unwrap(), &gaps::Options { min_items: 9, ..Default::default() });
    assert_eq!(strict.gaps.iter().map(|c| c.kind).collect::<Vec<_>>(), ["few-items", "no-lesson"]);
    done(&tree);
}

#[test]
fn usage_signals_raise_demand_and_can_hold_nothing_but_counts() {
    let (tree, _) = green_copy("signals");
    fs::create_dir_all(tree.join("signals")).unwrap();
    let row = json!({"schema": "signal/1", "period": "2026-W47", "objective": "maths.number.count-to-three", "band": "b05-06", "locale": "en", "kind": "miss", "count": 63, "source": "example-harness"});
    fs::write(tree.join("signals/2026-W47.jsonl"), format!("{row}\n")).unwrap();
    let s = scan(&tree, &data()).unwrap();
    assert!(s.findings.iter().all(|f| f.rule == "index.fresh"), "{:?}", s.findings);
    let g = gaps::report(&s, &gaps::Options::default()).gaps[0].clone();
    assert_eq!((g.usage, g.demand), (63, 5 + 5 * 6));

    let hit = |row: Value| -> Vec<String> {
        fs::write(tree.join("signals/2026-W47.jsonl"), format!("{row}\n")).unwrap();
        scan(&tree, &data()).unwrap().findings.into_iter().filter(|f| f.path.starts_with("signals/")).map(|f| f.rule).collect()
    };
    // a count small enough to point at one learner
    let mut small = row.clone();
    small["count"] = json!(3);
    assert_eq!(hit(small), ["schema.valid"]);
    // anything about a learner, a device or a day has no field to live in
    for (k, v) in [("learner", json!("a-child")), ("device", json!("abc")), ("age", json!(6)), ("note", json!("free text"))] {
        let mut extra = row.clone();
        extra[k] = v;
        assert_eq!(hit(extra), ["schema.valid"], "{k}");
    }
    let mut day = row.clone();
    day["period"] = json!("2026-11-23");
    assert_eq!(hit(day), ["schema.valid"]);
    // a cell that does not exist
    let mut nowhere = row.clone();
    nowhere["locale"] = json!("fr");
    assert_eq!(hit(nowhere), ["refs.resolve"]);
    done(&tree);
}

#[test]
fn a_work_order_carries_the_publishers_words_only_where_the_terms_allow() {
    let (tree, _) = green_copy("orders");
    let cell = |objective: &str| Cell { objective: objective.into(), subject: "maths".into(), band: "b05-06".into(), band_min: 5, locale: "en".into(), kind: "no-lesson", demand: 0, frameworks: 0, usage: 0, lessons: 0, items: 0, ready: true };
    let s = scan(&tree, &data()).unwrap();
    let o = &looprun::orders(&data(), &s, &[cell("maths.number.count-to-five")], 8).unwrap()[0];
    assert_eq!(o["lesson"], json!({"slug": "count-to-five-b05-06", "locale": "en", "subject": "maths", "ages": {"min": 5, "max": 6}, "skills": ["maths.number.count-to-five"], "minItems": 8}));
    assert_eq!(o["standards"][0]["statementOfficial"], "Pupils count up to five objects.");
    assert_eq!(o["objective"]["misconceptions"][0]["no"], 1);
    assert!(o["objective"].get("status").is_none() && o["reading"]["maxSentenceWords"].is_number());

    // the same framework under terms that allow identifiers only: the order keeps our paraphrase and drops their text
    let f = tree.join("standards/example-framework/framework.json");
    let mut v: Value = serde_json::from_str(&fs::read_to_string(&f).unwrap()).unwrap();
    v["terms"]["class"] = json!("reference_only");
    v["terms"]["quote"] = json!("ids_only");
    fs::write(&f, v.to_string()).unwrap();
    let s = scan(&tree, &data()).unwrap();
    let o = &looprun::orders(&data(), &s, &[cell("maths.number.count-to-five")], 8).unwrap()[0];
    assert!(o["standards"][0].get("statementOfficial").is_none());
    assert_eq!(o["standards"][0]["statementOwn"], "Count a small group of things.");
    done(&tree);
}

#[test]
fn a_draft_that_passes_lands_as_draft_closes_its_gap_and_cannot_reach_a_child() {
    let (tree, run) = green_copy("land");
    let s = scan(&tree, &data()).unwrap();
    let cells = gaps::pick(&gaps::report(&s, &gaps::Options::default()).gaps, 5);
    fs::write(run.join("orders.json"), serde_json::to_string(&looprun::orders(&data(), &s, &cells, 8).unwrap()).unwrap()).unwrap();
    put(&run, "001.json", &draft());
    let r = land(&tree, &run);
    assert_eq!((r["landed"].as_array().unwrap().len(), r["refused"].as_array().unwrap().len()), (1, 0), "{r:#}");
    assert_eq!(r["gaps"], json!({"cells": 2, "before": 1, "after": 0, "closed": ["maths.number.count-to-three@b05-06@en"]}));
    assert_eq!((&r["items"]["added"], &r["lessons"]["added"], &r["gates"]["red"], &r["gates"]["count"]), (&json!(8), &json!(1), &json!(0), &json!(openlessons::RULES.len())));

    // every record is a draft, whatever the proposal might have said
    let dir = tree.join(r["landed"][0]["path"].as_str().unwrap());
    let lesson: Value = serde_json::from_str(&fs::read_to_string(dir.join("lesson.json")).unwrap()).unwrap();
    assert_eq!((&lesson["status"], &lesson["provenance"]["origin"]), (&json!("draft"), &json!("ai")));
    assert!(fs::read_to_string(dir.join("items.jsonl")).unwrap().lines().all(|l| l.contains("\"status\":\"draft\"")));
    // the tree is whole: gates green, index fresh, the report on disk and in the scan
    let after = scan(&tree, &data()).unwrap();
    assert_eq!(after.findings.len(), 0, "{:?}", after.findings);
    assert_eq!(after.counts["runs"], 1);
    assert!(tree.join(format!("runs/{}.md", r["id"].as_str().unwrap())).exists());

    // a child-facing reader takes the default bar: the draft is refused, and so is `reviewed` as a bar
    let server = mcp::Server { root: tree.clone(), data: data(), allow_write: false };
    let get = |bar: Option<&str>| {
        let mut args = json!({"id": lesson["id"]});
        if let Some(b) = bar {
            args["min_status"] = json!(b);
        }
        server.handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "get_lesson", "arguments": args}})).unwrap()["result"].clone()
    };
    assert_eq!(get(None)["isError"], true);
    assert_eq!(get(Some("reviewed"))["isError"], true);
    assert_eq!(get(Some("draft"))["isError"], false);

    // nobody is listed in REVIEWERS.json for this lesson's maker to lean on: marking it reviewed turns two gates red
    let mut claimed = lesson.clone();
    claimed["status"] = json!("reviewed");
    claimed["provenance"]["reviews"] = json!([{"by": "someone-unlisted", "kind": "human", "role": "subject", "verdict": "pass", "date": "2026-11-23"}]);
    fs::write(dir.join("lesson.json"), claimed.to_string()).unwrap();
    let rules: Vec<String> = scan(&tree, &data()).unwrap().findings.into_iter().map(|f| f.rule).collect();
    assert!(rules.contains(&"loop.reviewer-listed".to_string()), "{rules:?}");
    done(&tree);
}

#[test]
fn a_draft_the_gates_refuse_leaves_no_trace() {
    let (tree, run) = green_copy("refuse");
    let lessons_before = scan(&tree, &data()).unwrap().counts["lessons"];
    let edit = |f: &dyn Fn(&mut Value)| {
        let mut d = draft();
        f(&mut d);
        d
    };
    // 1: carries the mark of copied material
    put(&run, "001-copied.json", &edit(&|d| d["lessons"][0]["lesson"]["teach"]["text"] = json!("Count each thing once. All rights reserved.")));
    // 2: written for ages the objective is not taught at
    put(&run, "002-ages.json", &edit(&|d| d["lessons"][0]["lesson"]["ages"] = json!({"min": 9, "max": 10})));
    // 3: brings its own skill: the loop does not change the topic graph
    put(&run, "003-skill.json", &edit(&|d| d["skills"] = json!([{"id": "maths.number.count-to-nine", "statement": {"en": "I can count to nine."}, "topic": "counting", "bands": ["b05-06"]}])));
    // 4: a contact detail
    put(&run, "004-pii.json", &edit(&|d| d["lessons"][0]["lesson"]["teach"]["reteach"] = json!("Ask sam@example.org for help.")));
    // 5: an AI draft that does not name its model
    put(&run, "005-model.json", &edit(&|d| {
        d["provenance"].as_object_mut().unwrap().remove("model");
    }));
    // 6: not JSON at all
    fs::write(run.join("drafts/006-broken.json"), "{").unwrap();
    let r = land(&tree, &run);
    assert_eq!(r["landed"].as_array().unwrap().len(), 0, "{r:#}");
    let why: Vec<String> = r["refused"].as_array().unwrap().iter().map(|d| format!("{} {}", d["draft"].as_str().unwrap(), d["why"][0]["rule"].as_str().unwrap())).collect();
    assert_eq!(why, ["001-copied.json content.no-copied-text", "002-ages.json alignment.band-matches", "003-skill.json loop", "004-pii.json content.no-pii", "005-model.json loop", "006-broken.json loop"]);
    let after = scan(&tree, &data()).unwrap();
    assert_eq!((after.counts["lessons"], after.findings.len()), (lessons_before, 0), "{:?}", after.findings);
    assert!(!tree.join("skills/maths/number/count-to-nine.json").exists());
    assert_eq!(r["gaps"]["closed"], json!([]));
    done(&tree);
}

#[test]
fn a_draft_must_answer_an_order_of_its_run() {
    let (tree, run) = green_copy("order");
    let s = scan(&tree, &data()).unwrap();
    let cells = gaps::pick(&gaps::report(&s, &gaps::Options::default()).gaps, 5);
    fs::write(run.join("orders.json"), serde_json::to_string(&looprun::orders(&data(), &s, &cells, 8).unwrap()).unwrap()).unwrap();
    let mut d = draft();
    d["lessons"][0]["lesson"]["slug"] = json!("my-own-idea");
    put(&run, "001.json", &d);
    let r = land(&tree, &run);
    assert!(r["refused"][0]["why"][0]["message"].as_str().unwrap().contains("answers no work order"), "{r:#}");
    done(&tree);
}

#[test]
fn a_drafter_that_fails_is_reported_not_trusted() {
    let (tree, run) = green_copy("drafter");
    let s = scan(&tree, &data()).unwrap();
    let cells = gaps::pick(&gaps::report(&s, &gaps::Options::default()).gaps, 5);
    let orders = looprun::orders(&data(), &s, &cells, 8).unwrap();
    // `git --version` reads no order and prints no proposal
    let failed = looprun::draft_with(&run, &orders, "git --version").unwrap();
    assert_eq!(failed.len(), 1);
    assert!(failed[0].1.contains("not one JSON object"), "{failed:?}");
    let r = looprun::land(&Land { root: &tree, data: &data(), from: &run, mode: Mode::Files, date: "2026-11-23".into(), gaps: gaps::Options::default(), failed }).unwrap();
    assert_eq!((r["landed"].as_array().unwrap().len(), &r["refused"][0]["why"][0]["rule"]), (0, &json!("drafter")));
    done(&tree);
}

#[test]
fn the_mcp_server_offers_the_gap_report_and_work_orders_without_writing() {
    let (tree, _) = green_copy("mcp");
    assert_eq!(mcp::tool_names(), ["find_lessons", "get_lesson", "gap_report", "get_work_orders", "validate_proposal", "jury_order", "jury_submit", "propose_lesson"]);
    let server = mcp::Server { root: tree.clone(), data: data(), allow_write: false };
    let call = |tool: &str, args: Value| -> Value {
        let r = server.handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": tool, "arguments": args}})).unwrap();
        assert_eq!(r["result"]["isError"], false, "{r}");
        serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    };
    let g = call("gap_report", json!({}));
    assert_eq!((&g["summary"]["gaps"], &g["gaps"][0]["cell"]), (&json!(1), &json!("maths.number.count-to-three@b05-06@en")));
    assert_eq!(call("gap_report", json!({"locales": ["id"]}))["summary"]["cells"], 0);
    let o = call("get_work_orders", json!({"top": 3}));
    assert_eq!((o["orders"].as_array().unwrap().len(), &o["orders"][0]["schema"]), (1, &json!("order/1")));
    // the draft of that order passes the server's own check, which saves nothing
    let v = call("validate_proposal", json!({"proposal": draft()}));
    assert_eq!(v["ok"], true, "{v}");
    assert!(!tree.join("proposals").exists());
    done(&tree);
}

#[test]
fn the_real_tree_has_a_registry_bands_a_graph_and_gaps() {
    let s = scan(&root(), &data()).unwrap();
    assert!(s.registry.len() >= 30 && s.bands.len() == 10 && s.objectives.len() >= 150, "{} {} {}", s.registry.len(), s.bands.len(), s.objectives.len());
    // no framework whose terms were not read, or do not allow it, may hold the publisher's wording
    for f in &s.registry {
        let t = &f["terms"];
        assert!(!t["reviewed"].is_null() || t["quote"] == "ids_only", "{}", f["id"]);
        assert!(t["class"] != "reference_only" || t["quote"] == "ids_only", "{}", f["id"]);
    }
    for (id, n) in &s.nodes {
        let quote = s.frameworks[n["framework"].as_str().unwrap()].as_str();
        assert!(n.get("statementOfficial").is_none() || quote == "official_text", "{id}");
    }
    // every lesson already in the tree hangs from the graph: its objectives have a topic and bands that hold its ages
    for l in &s.lessons {
        for k in l["skills"].as_array().unwrap() {
            let o = &s.objectives[k.as_str().unwrap()];
            assert!(!o.topic.is_empty() && !o.bands.is_empty(), "{k}");
        }
    }
    let r = gaps::report(&s, &gaps::Options::default());
    // a cell is served only by lessons the jury settled
    let settled = s.lessons.iter().filter(|l| l["status"] == "reviewed" || l["status"] == "published").count();
    assert!(r.cells > 1000 && r.filled >= 49 && r.served <= settled, "{} {} {} {settled}", r.cells, r.filled, r.served);
    // seven languages, and a pick is spread over them
    assert_eq!(s.locales.len(), 7);
    let picked = gaps::pick(&gaps::report(&s, &gaps::Options { subjects: vec!["maths".into(), "science".into(), "computing".into()], ..Default::default() }).gaps, 21);
    let (locales, subjects, objectives): (std::collections::BTreeSet<_>, std::collections::BTreeSet<_>, std::collections::BTreeSet<_>) =
        (picked.iter().map(|c| &c.locale).collect(), picked.iter().map(|c| &c.subject).collect(), picked.iter().map(|c| &c.objective).collect());
    assert_eq!((locales.len(), subjects.len(), objectives.len()), (7, 3, 21));
}

#[test]
fn a_dry_run_commits_the_run_on_its_own_branch_and_pushes_nothing() {
    let (tree, run) = green_copy("dry");
    let git = |args: &[&str]| -> String {
        let out = std::process::Command::new("git").arg("-C").arg(&tree).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.name", "Example"]);
    git(&["config", "user.email", "example@example.org"]);
    git(&["config", "core.autocrlf", "false"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "the green tree"]);
    put(&run, "001.json", &draft());
    let land = |mode| looprun::land(&Land { root: &tree, data: &data(), from: &run, mode, date: "2026-11-23".into(), gaps: gaps::Options::default(), failed: vec![] });
    let r = land(Mode::DryRun).unwrap();
    let branch = r["branch"].as_str().unwrap().to_string();
    assert_eq!(branch, format!("loop/{}", r["id"].as_str().unwrap()));
    assert!(r["gates"]["base"].as_str().unwrap().contains("HEAD") && r.get("pullRequest").is_none());
    // the caller is back on its branch with a clean tree; the run is one commit on the new branch
    assert_eq!((git(&["rev-parse", "--abbrev-ref", "HEAD"]).as_str(), git(&["status", "--porcelain"]).as_str()), ("main", ""));
    assert_eq!(git(&["rev-list", "--count", &format!("main..{branch}")]), "1");
    let files = git(&["diff", "--name-only", "main", &branch]);
    assert!(files.contains("lessons/en/maths/") && files.contains("index/objectives.jsonl") && files.contains("runs/"), "{files}");
    assert!(git(&["log", "-1", "--format=%s", &branch]).starts_with("feat(loop): run "));
    assert_eq!(git(&["remote"]), "");

    // a run never starts from a tree with uncommitted work
    fs::write(tree.join("stray.txt"), "x").unwrap();
    assert!(land(Mode::DryRun).unwrap_err().to_string().contains("uncommitted"));
    done(&tree);
}

#[test]
fn every_held_back_framework_is_guarded_by_a_proven_list_or_says_it_is_not() {
    let s = scan(&root(), &data()).unwrap();
    let lists: Value = serde_json::from_str(&fs::read_to_string(data().join("protected.json")).unwrap()).unwrap();
    let held = openlessons::text::Protected::load(&data()).unwrap().coverage();
    let (mut listed, mut rule_only) = (0, 0);
    for f in &s.registry {
        let (id, t) = (f["id"].as_str().unwrap(), &f["terms"]);
        if t["quote"] == "official_text" {
            assert!(t.get("guard").is_none() && !held.contains_key(id), "{id}: wording that may be quoted needs no guard");
            continue;
        }
        match t["guard"].as_str() {
            Some("fingerprints") => {
                listed += 1;
                // the list is there, as large as its record says, made from a named official source, and proven red first
                let l = &lists["lists"][id];
                let lines = fs::read_to_string(data().join("protected").join(format!("{id}.txt"))).unwrap().lines().count() as u64 - 1;
                // (a run two publishers share is counted for the first of them, so the loaded count can be a little lower)
                assert!(lines == l["fingerprints"].as_u64().unwrap() && held[id] as u64 <= lines && held[id] > 200, "{id}");
                assert!(l["sources"][0]["url"].as_str().unwrap().starts_with("https://") && l["window"].as_u64().unwrap() >= 8, "{id}");
                assert!(l["proof"]["before"].as_str().unwrap().starts_with("not caught") && l["proof"]["after"].as_str().unwrap().contains(id), "{id}");
                assert!(t["note"].as_str().unwrap().contains("Copied-text guard: fingerprints"), "{id}");
            }
            Some("paraphrase_rule") => {
                rule_only += 1;
                assert!(!held.contains_key(id) && t["note"].as_str().unwrap().contains("Copied-text guard: the paraphrase rule alone"), "{id}");
            }
            other => panic!("{id}: terms.guard is {other:?}"),
        }
    }
    assert_eq!((listed, rule_only), (10, 22));
    // the lists hold hashes and nothing else: no line of a list is text
    for (id, _) in held {
        let body = fs::read_to_string(data().join("protected").join(format!("{id}.txt"))).unwrap();
        assert!(body.lines().skip(1).all(|l| l.len() == 12 && l.bytes().all(|b| b.is_ascii_hexdigit())), "{id}");
    }

    // a record that claims a list it does not have, or hides one it has, turns the terms gate red
    let (tree, _) = green_copy("guard");
    let f = tree.join("standards/example-framework/framework.json");
    let mut v: Value = serde_json::from_str(&fs::read_to_string(&f).unwrap()).unwrap();
    v["terms"]["class"] = json!("reference_only");
    v["terms"]["quote"] = json!("ids_only");
    let hit = |v: &Value| -> usize {
        fs::write(&f, v.to_string()).unwrap();
        scan(&tree, &data()).unwrap().findings.iter().filter(|x| x.rule == "standards.terms-respected" && x.message.contains("guard")).count()
    };
    assert_eq!(hit(&v), 1, "no guard named");
    v["terms"]["guard"] = json!("fingerprints");
    assert_eq!(hit(&v), 1, "a list that does not exist");
    v["terms"]["guard"] = json!("paraphrase_rule");
    assert_eq!(hit(&v), 0);
    done(&tree);
}

/// The copied-text guard, proven on every run with sentences made up for this test: no publisher's text is involved.
#[test]
fn a_planted_sentence_from_a_fingerprinted_text_is_refused_and_its_paraphrase_lands() {
    // our own "protected text", and a list of it in the same format as gates/data/protected/<framework>.txt
    let theirs = "Seven small green frogs hop over the old log by the pond. Then the frogs sit very still and wait for the rain to stop.";
    let planted = "Seven small green frogs hop over the old log by the pond.";
    let paraphrase = "By the pond, an old log has seven little frogs jumping across it.";
    let (tree, run) = green_copy("guard-ci");
    let data_dir = |name: &str, with_list: bool| -> PathBuf {
        let d = tree.parent().unwrap().join(name);
        fs::create_dir_all(d.join("safety")).unwrap();
        for f in ["reading-levels.json", "licences.json", "protected.json", "safety/en.json"] {
            fs::copy(data().join(f), d.join(f)).unwrap();
        }
        if with_list {
            fs::create_dir_all(d.join("protected")).unwrap();
            let list = format!("# window=8\n{}\n", openlessons::text::fingerprints(theirs, 8, 1).join("\n"));
            assert!(list.lines().skip(1).count() > 10 && !list.contains("frogs"), "hashes only");
            fs::write(d.join("protected/made-up-publisher.txt"), list).unwrap();
        }
        d
    };
    let with_text = |sentence: &str| {
        let mut d = draft();
        d["lessons"][0]["lesson"]["teach"]["text"] = json!(format!("Count each thing once. {sentence}"));
        d
    };
    let try_land = |data: &Path, name: &str, d: &Value| -> Value {
        let drafts = run.join(name);
        fs::create_dir_all(drafts.join("drafts")).unwrap();
        fs::write(drafts.join("drafts/001.json"), d.to_string()).unwrap();
        looprun::land(&Land { root: &tree, data, from: &drafts, mode: Mode::Files, date: "2026-11-23".into(), gaps: gaps::Options::default(), failed: vec![] }).unwrap()
    };

    // red first: without the list nothing knows the sentence, and a scan of the planted text is green
    let unguarded = data_dir("data-without-list", false);
    let mut lesson = with_text(planted)["lessons"][0]["lesson"].clone();
    assert!(openlessons::text::Protected::load(&unguarded).unwrap().issue(lesson["teach"]["text"].as_str().unwrap()).is_none());

    // with the list: the planted sentence is refused by the gate, naming the list, and leaves no trace
    let guarded = data_dir("data-with-list", true);
    let r = try_land(&guarded, "planted", &with_text(planted));
    assert_eq!(r["landed"].as_array().unwrap().len(), 0, "{r:#}");
    let why = &r["refused"][0]["why"][0];
    assert_eq!(why["rule"], "content.no-copied-text");
    assert!(why["message"].as_str().unwrap().contains("made-up-publisher"), "{why}");
    // a few words changed inside the sentence do not hide a run of eight that is left
    lesson["teach"]["text"] = json!("Look! Seven small green frogs hop over the old log today.");
    assert!(openlessons::text::Protected::load(&guarded).unwrap().issue(lesson["teach"]["text"].as_str().unwrap()).is_some());

    // the same idea in our own words lands
    let r = try_land(&guarded, "paraphrase", &with_text(paraphrase));
    assert_eq!((r["landed"].as_array().unwrap().len(), r["refused"].as_array().unwrap().len(), &r["gates"]["red"]), (1, 0, &json!(0)), "{r:#}");
    done(&tree);
}
