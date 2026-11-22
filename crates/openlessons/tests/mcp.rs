//! The MCP server, driven as a client drives it: the real binary, JSON-RPC lines on its standard input and output.

use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
    let dir = std::env::temp_dir().join(format!("openlessons-mcp-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&root().join("gates/fixtures/green"), &dir);
    dir
}

/// Start the server on a tree, send the requests, close its input, and return the responses by id.
fn talk(tree: &Path, allow_write: bool, requests: &[Value]) -> Vec<Value> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_openlessons"));
    cmd.arg("mcp").arg("--root").arg(tree).arg("--data").arg(root().join("gates/data"));
    if allow_write {
        cmd.arg("--allow-write");
    }
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        for r in requests {
            writeln!(stdin, "{r}").unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str(l).expect("every output line is one JSON message")).collect()
}
fn call(id: u64, tool: &str, args: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": tool, "arguments": args}})
}
/// The text of a tool result, and whether it is an error.
fn result(replies: &[Value], id: u64) -> (String, bool) {
    let r = replies.iter().find(|r| r["id"] == id).unwrap_or_else(|| panic!("no reply to {id}"));
    (r["result"]["content"][0]["text"].as_str().unwrap().to_string(), r["result"]["isError"] == true)
}

fn proposal() -> Value {
    json!({
        "schema": "proposal/1", "title": "Count to four",
        "provenance": {"origin": "ai", "maker": "example-agent", "model": "example-model", "created": "2026-11-23"},
        "skills": [{"id": "maths.number.count-to-four", "statement": {"en": "I can count up to four things."}, "prerequisites": ["maths.number.count-to-three"]}],
        "alignments": [{"skill": "maths.number.count-to-four", "standard": "example-framework:y1", "strength": "partial"}],
        "lessons": [{
            "lesson": {"slug": "count-to-four", "locale": "en", "subject": "maths", "title": "Count to four", "ages": {"min": 5, "max": 7},
                       "skills": ["maths.number.count-to-four"], "teach": {"text": "Count each thing once. One, two, three, four."}},
            "items": [{"kind": "tap_choice", "band": "easy", "role": "practice",
                       "view": {"prompt": {"text": "How many moons?", "image": {"emoji": "🌙🌙🌙🌙"}}, "options": [{"id": "a", "text": "3"}, {"id": "b", "text": "4"}, {"id": "c", "text": "5"}]},
                       "key": {"graded": true, "answer": {"option": "b"}, "hints": [], "verification": {"method": "computed"}}}]
        }]
    })
}

#[test]
fn it_speaks_the_protocol() {
    let tree = green_copy("protocol");
    let replies = talk(&tree, false, &[
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "ping"}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "resources/list"}),
    ]);
    assert_eq!(replies.len(), 4, "a notification gets no reply");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "openlessons");
    assert!(replies[0]["result"]["capabilities"]["tools"].is_object());
    let names: Vec<&str> = replies[1]["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["find_lessons", "get_lesson", "validate_proposal", "propose_lesson"]);
    assert!(replies[1]["result"]["tools"].as_array().unwrap().iter().all(|t| t["inputSchema"]["type"] == "object" && t["description"].is_string()));
    assert_eq!(replies[2]["result"], json!({}));
    assert_eq!(replies[3]["error"]["code"], -32601);
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn finding_and_reading_a_lesson_never_returns_an_answer() {
    let tree = green_copy("read");
    let replies = talk(&tree, false, &[call(1, "find_lessons", json!({"subject": "maths", "age": 6}))]);
    let (text, err) = result(&replies, 1);
    assert!(!err, "{text}");
    let found: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(found["count"], 1);
    let id = found["lessons"][0]["id"].as_str().unwrap().to_string();

    let replies = talk(&tree, false, &[
        call(1, "get_lesson", json!({"id": id, "min_status": "reviewed"})),
        call(5, "get_lesson", json!({"id": id})),
        call(2, "get_lesson", json!({"id": id, "min_status": "proposed"})),
        call(3, "get_lesson", json!({"id": "../../etc/passwd"})),
        call(4, "get_lesson", json!({"id": "les_000000000000000000000000"})),
    ]);
    // the green lesson is reviewed, not published: the default bar refuses it
    assert!(result(&replies, 5).1);
    // its items are proposed: at the bar `reviewed` the lesson comes without them
    let (text, err) = result(&replies, 1);
    assert!(!err, "{text}");
    let lesson: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(lesson["items"], json!([]), "unreviewed items stay behind");
    let (text, err) = result(&replies, 2);
    assert!(!err, "{text}");
    let lesson: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(lesson["items"].as_array().unwrap().len(), 8);
    for secret in ["\"key\"", "\"answer\"", "\"distractors\"", "\"verification\"", "Touch each one as you count it", "Count them one at a time", "Tap the word cat"] {
        assert!(!text.contains(secret), "get_lesson returned {secret}");
    }
    assert!(text.contains("How many stars?") && text.contains("\"view\""));
    assert!(result(&replies, 3).1 && result(&replies, 4).1);
    fs::remove_dir_all(&tree).unwrap();
}

#[test]
fn an_unreviewed_lesson_is_refused_at_the_default_bar() {
    // the real tree: everything is proposed
    let replies = talk(&root(), false, &[call(1, "find_lessons", json!({"subject": "maths", "limit": 1}))]);
    let found: Value = serde_json::from_str(&result(&replies, 1).0).unwrap();
    assert!(found["count"].as_u64().unwrap() >= 12 && found["shown"] == 1);
    let id = found["lessons"][0]["id"].as_str().unwrap().to_string();
    let replies = talk(&root(), false, &[call(1, "get_lesson", json!({"id": id}))]);
    let (text, err) = result(&replies, 1);
    assert!(err && text.contains("proposed") && text.contains("min_status"), "{text}");
}

#[test]
fn a_proposal_is_validated_and_saved_only_when_writing_is_allowed() {
    let tree = green_copy("propose");
    let files = |tree: &Path| -> usize {
        fn count(d: &Path) -> usize {
            fs::read_dir(d).unwrap().map(|e| e.unwrap().path()).map(|p| if p.is_dir() { count(&p) } else { 1 }).sum()
        }
        count(tree)
    };
    let before = files(&tree);

    // read-only, the default: validated, never saved
    let replies = talk(&tree, false, &[call(1, "validate_proposal", json!({"proposal": proposal()})), call(2, "propose_lesson", json!({"proposal": proposal()}))]);
    let verdict: Value = serde_json::from_str(&result(&replies, 1).0).unwrap();
    assert_eq!(verdict["ok"], true, "{verdict}");
    assert_eq!(verdict["records"], json!({"skills": 1, "alignments": 1, "lessons": 1, "items": 1}));
    let (text, err) = result(&replies, 2);
    assert!(err && text.contains("read-only") && text.contains("--allow-write"), "{text}");
    assert_eq!(files(&tree), before, "a read-only server changed the tree");

    // a proposal the gates refuse is reported, and is not saved even when writing is allowed
    let mut bad = proposal();
    bad["lessons"][0]["items"][0]["view"]["prompt"]["text"] = json!("Which number is 4?");
    let replies = talk(&tree, true, &[call(1, "validate_proposal", json!({"proposal": bad})), call(2, "propose_lesson", json!({"proposal": bad}))]);
    let verdict: Value = serde_json::from_str(&result(&replies, 1).0).unwrap();
    assert_eq!(verdict["ok"], false);
    assert!(verdict["findings"].as_array().unwrap().iter().any(|f| f["rule"] == "item.no-answer-leak"), "{verdict}");
    let saved: Value = serde_json::from_str(&result(&replies, 2).0).unwrap();
    assert_eq!(saved["saved"], false);
    assert_eq!(files(&tree), before);

    // a proposal that tries to arrive reviewed: saved as written, but every record it yields is proposed
    let mut eager = proposal();
    eager["lessons"][0]["lesson"]["status"] = json!("published");
    let replies = talk(&tree, true, &[call(1, "propose_lesson", json!({"proposal": eager}))]);
    let saved: Value = serde_json::from_str(&result(&replies, 1).0).unwrap();
    assert_eq!(saved["saved"], true, "{saved}");
    let file = saved["file"].as_str().unwrap();
    assert!(file.starts_with("proposals/") && file.ends_with(".json"));
    assert_eq!(files(&tree), before + 1, "exactly one file: the proposal");
    assert!(!tree.join("skills/maths/number/count-to-four.json").exists(), "no record was written, only the proposal");
    let stored: Value = serde_json::from_str(&fs::read_to_string(tree.join(file)).unwrap()).unwrap();
    let records = openlessons::import::from_proposal(&stored).unwrap();
    assert!(records.lessons.iter().all(|(l, items)| l["status"] == "proposed" && items.iter().all(|i| i["status"] == "proposed")));
    // the inbox file is itself judged by the gates
    let s = openlessons::scan(&tree, &root().join("gates/data")).unwrap();
    assert!(s.findings.is_empty(), "{:#?}", s.findings);
    fs::remove_dir_all(&tree).unwrap();
}
