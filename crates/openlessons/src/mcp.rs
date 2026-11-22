//! A Model Context Protocol server over the tree: JSON-RPC 2.0, one message per line, on standard input and output.
//!
//! Four tools. Three only read: `find_lessons`, `get_lesson` (the learner's view: never an answer), `validate_proposal`.
//! One writes, and only when the server was started with `--allow-write`: `propose_lesson` saves a proposal file under
//! `proposals/`. It never writes a lesson, and nothing it writes can be `reviewed`: a maintainer applies the file, the
//! gates judge it, a person reviews it.
//!
//! The protocol surface used here is small (initialize, ping, tools/list, tools/call), so it is written out by hand:
//! the tool stays one binary with no runtime of its own.

use crate::canon::{canonical, sha256_hex};
use crate::export::{arr, learner_item, rank, st};
use crate::import::{from_proposal, write_records};
use crate::{find, scan, Query};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Protocol revisions this server speaks, newest first.
const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const MAX_LINE: usize = 8 * 1024 * 1024;
const BAR: &str = "published, reviewed, proposed or draft";

pub struct Server {
    pub root: PathBuf,
    /// the gate data folder
    pub data: PathBuf,
    pub allow_write: bool,
}

fn tools() -> Value {
    let status = json!({"type": "string", "enum": ["published", "reviewed", "proposed", "draft"]});
    let proposal = json!({"type": "object", "description": "a proposal/1 object (schemas/proposal.schema.json): provenance, then skills, alignments and lessons with their items, without ids or status"});
    json!([
        {
            "name": "find_lessons",
            "description": "Search the lesson index. Every result carries its `status`: only `published` lessons have been reviewed by an educator.",
            "inputSchema": {"type": "object", "additionalProperties": false, "properties": {
                "locale": {"type": "string", "description": "language the lesson is written in (en, id)"},
                "subject": {"type": "string"},
                "skill": {"type": "string", "description": "a skill id or a prefix (english.phonics)"},
                "standard": {"type": "string", "description": "a standard id (eng-nc-2014:y1.maths)"},
                "framework": {"type": "string", "description": "a framework id (ccss-2010)"},
                "age": {"type": "integer"},
                "status": status,
                "text": {"type": "string", "description": "words in the title, slug or skill ids"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
            }}
        },
        {
            "name": "get_lesson",
            "description": "One lesson as a learner may see it: what to teach and each item's `view`. Answers, hints and feedback are never returned. Lessons below `min_status` (default published) are refused.",
            "inputSchema": {"type": "object", "additionalProperties": false, "required": ["id"], "properties": {
                "id": {"type": "string", "pattern": "^les_[0-9a-f]{24}$"},
                "min_status": {"type": "string", "enum": ["published", "reviewed", "proposed", "draft"], "default": "published", "description": "lower it only when an adult responsible for the learner accepts unreviewed content"}
            }}
        },
        {
            "name": "validate_proposal",
            "description": "Run the quality gates over a proposal without saving anything. Returns the findings to fix.",
            "inputSchema": {"type": "object", "additionalProperties": false, "required": ["proposal"], "properties": {"proposal": proposal}}
        },
        {
            "name": "propose_lesson",
            "description": "Validate a proposal and, if the gates pass, save it as a proposal file under proposals/ for a maintainer to apply and a person to review. Writes no lesson and can never mark anything reviewed. Refused unless the server was started with --allow-write.",
            "inputSchema": {"type": "object", "additionalProperties": false, "required": ["proposal"], "properties": {"proposal": proposal}}
        }
    ])
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    if !from.exists() {
        return Ok(());
    }
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &dest)?;
        } else {
            fs::copy(e.path(), &dest)?;
        }
    }
    Ok(())
}

impl Server {
    fn find_lessons(&self, a: &Value) -> Result<Value> {
        let s = |k: &str| a.get(k).and_then(Value::as_str).map(str::to_string);
        let q = Query { locale: s("locale"), subject: s("subject"), skill: s("skill"), standard: s("standard"), framework: s("framework"), age: a.get("age").and_then(Value::as_i64), status: s("status"), text: s("text") };
        let limit = a.get("limit").and_then(Value::as_u64).unwrap_or(20).clamp(1, 100) as usize;
        let found = find(&self.root, &q)?;
        Ok(json!({"count": found.len(), "shown": found.len().min(limit), "lessons": found.iter().take(limit).collect::<Vec<_>>()}))
    }

    fn get_lesson(&self, a: &Value) -> Result<Value> {
        let id = st(a, "id");
        if id.len() != 28 || !id.starts_with("les_") || !id[4..].bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("`id` is a lesson id: les_ and 24 hex digits");
        }
        let bar = a.get("min_status").and_then(Value::as_str).unwrap_or("published");
        if !matches!(bar, "published" | "reviewed" | "proposed" | "draft") {
            bail!("`min_status` is {BAR}");
        }
        let line = find(&self.root, &Query::default())?.into_iter().find(|l| l["id"] == id).ok_or_else(|| anyhow!("no lesson {id} in the index"))?;
        let rel = st(&line, "path");
        if !rel.starts_with("lessons/") || rel.split('/').any(|p| p == ".." || p.is_empty()) {
            bail!("the index names an unexpected folder for {id}");
        }
        let dir = self.root.join(rel);
        let lesson: Value = serde_json::from_str(&fs::read_to_string(dir.join("lesson.json"))?)?;
        if rank(st(&lesson, "status")) < rank(bar) {
            bail!(
                "this lesson is `{}`, below the bar `{bar}`: no educator has passed it. Lower `min_status` only when an adult responsible for the learner accepts unreviewed content",
                st(&lesson, "status")
            );
        }
        let mut items = Vec::new();
        for l in fs::read_to_string(dir.join("items.jsonl")).unwrap_or_default().lines().filter(|l| !l.trim().is_empty()) {
            let i: Value = serde_json::from_str(l)?;
            if rank(st(&i, "status")) >= rank(bar) {
                items.push(learner_item(&i));
            }
        }
        let mut out = json!({
            "id": lesson["id"], "title": lesson["title"], "locale": lesson["locale"], "subject": lesson["subject"], "ages": lesson["ages"],
            "skills": lesson["skills"], "teach": lesson["teach"], "status": lesson["status"],
            "credit": format!("{}, {}", st(&lesson, "attribution"), st(&lesson, "licence")), "items": items,
        });
        for k in ["image", "targetLocale"] {
            if let Some(v) = lesson.get(k) {
                out[k] = v.clone();
            }
        }
        Ok(out)
    }

    /// The gates over a proposal, in a scratch tree: the reference data, the lessons it touches, and its own records.
    fn check(&self, proposal: &Value) -> Result<Value> {
        static N: AtomicU64 = AtomicU64::new(0);
        let records = from_proposal(proposal)?;
        let tmp = std::env::temp_dir().join(format!("openlessons-mcp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&tmp);
        let run = || -> Result<Vec<Value>> {
            for d in ["locales", "standards", "skills", "alignments", "assets"] {
                copy_tree(&self.root.join(d), &tmp.join(d))?;
            }
            if self.root.join("REVIEWERS.json").exists() {
                fs::copy(self.root.join("REVIEWERS.json"), tmp.join("REVIEWERS.json"))?;
            }
            for (l, _) in &records.lessons {
                let rel = format!("lessons/{}/{}/{}/{}", st(l, "locale"), st(l, "subject"), st(l, "id").get(4..6).unwrap_or(""), st(l, "id"));
                copy_tree(&self.root.join(&rel), &tmp.join(&rel))?;
            }
            if let Err(e) = write_records(&tmp, &records) {
                return Ok(vec![json!({"rule": "loop.base-respected", "path": "", "message": e.to_string()})]);
            }
            let s = scan(&tmp, &self.data)?;
            Ok(s.findings.iter().filter(|f| f.rule != "index.fresh").map(|f| json!({"rule": f.rule, "path": f.path, "line": f.line, "message": f.message})).collect())
        };
        let findings = run();
        let _ = fs::remove_dir_all(&tmp);
        let findings = findings?;
        let items: usize = records.lessons.iter().map(|l| l.1.len()).sum();
        Ok(json!({
            "ok": findings.is_empty(), "findings": findings,
            "records": {"skills": records.skills.len(), "alignments": records.alignments.len(), "lessons": records.lessons.len(), "items": items},
            "note": "Judged against the skills, standards and assets of the tree and the lessons this proposal touches. Duplicates of items in other lessons are caught when the proposal is applied. Every record will be `proposed`: only a person can review it.",
        }))
    }

    fn propose(&self, proposal: &Value) -> Result<Value> {
        if !self.allow_write {
            bail!("this server is read-only. To accept proposals, start it with `openlessons mcp --allow-write`; it will then save proposal files under proposals/ and nothing else");
        }
        let verdict = self.check(proposal)?;
        if verdict["ok"] != json!(true) {
            return Ok(json!({"saved": false, "why": "the gates refused it: fix the findings and propose again", "findings": verdict["findings"]}));
        }
        let name = format!("{}.json", &sha256_hex(canonical(proposal)?.as_bytes())[..24]);
        let dir = self.root.join("proposals");
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(&name), format!("{}\n", serde_json::to_string_pretty(proposal)?))?;
        Ok(json!({
            "saved": true, "file": format!("proposals/{name}"), "records": verdict["records"],
            "next": "A maintainer runs `openlessons apply` on the file and opens a pull request. Everything in it is `proposed` until a listed reviewer passes it.",
        }))
    }

    fn call(&self, params: &Value) -> Value {
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let result = match st(params, "name") {
            "find_lessons" => self.find_lessons(&args),
            "get_lesson" => self.get_lesson(&args),
            "validate_proposal" => self.check(&args["proposal"]),
            "propose_lesson" => self.propose(&args["proposal"]),
            other => Err(anyhow!("unknown tool {other}")),
        };
        match result {
            Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}], "isError": false}),
            Err(e) => json!({"content": [{"type": "text", "text": format!("{e:#}")}], "isError": true}),
        }
    }

    /// One request in, one response out; None for a notification.
    pub fn handle(&self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned()?;
        let ok = |result: Value| Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
        match st(msg, "method") {
            "initialize" => {
                let asked = st(&msg["params"], "protocolVersion");
                let version = if PROTOCOLS.contains(&asked) { asked } else { PROTOCOLS[0] };
                ok(json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "openlessons", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": "An open curriculum commons. Find lessons, read one as a learner sees it (answers are never returned), validate a proposal, or propose a lesson. Check `status`: only `published` content has been reviewed by an educator; serve nothing else to children unless a responsible adult agrees. Proposals are always saved as `proposed`.",
                }))
            }
            "ping" => ok(json!({})),
            "tools/list" => ok(json!({"tools": tools()})),
            "tools/call" => ok(self.call(&msg["params"])),
            other => Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("method not found: {other}")}})),
        }
    }

    /// Serve until the input closes.
    pub fn serve(&self, input: impl BufRead, mut output: impl Write) -> Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let reply = if line.len() > MAX_LINE {
                Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32600, "message": "message too large"}}))
            } else {
                match serde_json::from_str::<Value>(&line) {
                    Ok(msg) => self.handle(&msg),
                    Err(e) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}})),
                }
            };
            if let Some(r) = reply {
                writeln!(output, "{r}")?;
                output.flush()?;
            }
        }
        Ok(())
    }
}

/// The names of the tools the server offers.
pub fn tool_names() -> Vec<String> {
    arr(&json!({"t": tools()}), "t").iter().map(|t| st(t, "name").to_string()).collect()
}
