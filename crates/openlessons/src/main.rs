//! The `openlessons` command line. Exit codes: 0 ok, 1 refused (a gate is red), 2 usage or format error.

use anyhow::Result;
use clap::{Parser, Subcommand};
use openlessons::{canon, find, scan, selftest, stamp, write_index, Query, RULES};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "openlessons", version, about = "Validate, index and search the openlessons curriculum commons")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run every quality gate over the tree (after proving the gates on their fixtures)
    Validate {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// one JSON object per finding, then a summary object
        #[arg(long)]
        json: bool,
        /// findings printed per gate (text output)
        #[arg(long, default_value_t = 20)]
        max: usize,
    },
    /// Prove each gate: red on its red fixture, green on the green one
    Selftest {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Regenerate index/ from the data (refused while any gate is red)
    Index {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// the gate data folder (default: <root>/gates/data)
        #[arg(long)]
        data: Option<PathBuf>,
    },
    /// Find lessons in the index
    Find {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// BCP 47 locale the lesson is written in (en, id)
        #[arg(long)]
        locale: Option<String>,
        #[arg(long)]
        subject: Option<String>,
        /// a skill id or a prefix of one (english.phonics)
        #[arg(long)]
        skill: Option<String>,
        /// a standard id (eng-nc-2014:y1.maths)
        #[arg(long)]
        standard: Option<String>,
        /// a framework id (ccss-2010)
        #[arg(long)]
        framework: Option<String>,
        /// an age the lesson is written for
        #[arg(long)]
        age: Option<i64>,
        /// draft, proposed, reviewed or published
        #[arg(long)]
        status: Option<String>,
        /// words that must appear in the title, slug or skill ids
        #[arg(long)]
        text: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// one JSON object per lesson
        #[arg(long)]
        json: bool,
    },
    /// Write content ids into items.jsonl files and sort them (a file or a folder)
    Stamp { paths: Vec<PathBuf> },
    /// Print the id and folder of a lesson
    Id { locale: String, subject: String, slug: String },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn gates_proven(root: &Path, quiet: bool) -> Result<bool> {
    let t = selftest(root)?;
    if !quiet || !t.ok {
        t.lines.iter().for_each(|l| eprintln!("{l}"));
    }
    if !t.ok {
        eprintln!("refused: the gates did not pass their own self-test, so their verdict on the data means nothing");
    }
    Ok(t.ok)
}

fn run(cli: Cli) -> Result<u8> {
    match cli.cmd {
        Cmd::Selftest { root } => Ok(if gates_proven(&root, false)? { 0 } else { 1 }),
        Cmd::Validate { root, json, max } => {
            if !gates_proven(&root, true)? {
                return Ok(1);
            }
            let s = scan(&root, &root.join("gates").join("data"))?;
            let per_rule = |rule: &str| s.findings.iter().filter(|f| f.rule == rule).count();
            if json {
                for f in &s.findings {
                    println!("{}", json!({"rule": f.rule, "path": f.path, "line": f.line, "message": f.message}));
                }
                let rules: serde_json::Map<String, serde_json::Value> = RULES.iter().map(|r| (r.to_string(), json!(per_rule(r)))).collect();
                println!("{}", json!({"summary": {"findings": s.findings.len(), "rules": rules, "counts": s.counts}}));
            } else {
                for rule in RULES {
                    let n = per_rule(rule);
                    println!("{} {rule}{}", if n == 0 { "ok  " } else { "RED " }, if n == 0 { String::new() } else { format!(": {n} finding(s)") });
                    for f in s.findings.iter().filter(|f| f.rule == *rule).take(max) {
                        let at = if f.line > 0 { format!("{}:{}", f.path, f.line) } else { f.path.clone() };
                        println!("     {at}: {}", f.message);
                    }
                    if n > max {
                        println!("     ... and {} more", n - max);
                    }
                }
                let counts: Vec<String> = s.counts.iter().map(|(k, v)| format!("{v} {k}")).collect();
                println!("{} gates proven on their fixtures; read {}; {} finding(s)", RULES.len(), counts.join(", "), s.findings.len());
            }
            Ok(if s.findings.is_empty() { 0 } else { 1 })
        }
        Cmd::Index { root, data } => {
            let s = scan(&root, &data.unwrap_or_else(|| root.join("gates").join("data")))?;
            let blocking = s.findings.iter().filter(|f| f.rule != "index.fresh").count();
            if blocking > 0 {
                eprintln!("refused: {blocking} finding(s); an index is generated only from data that passes the gates (run `openlessons validate`)");
                return Ok(1);
            }
            let n = write_index(&root, &s)?;
            println!("index: {n} file(s) for {} lesson(s)", s.counts.get("lessons").copied().unwrap_or(0));
            Ok(0)
        }
        Cmd::Find { root, locale, subject, skill, standard, framework, age, status, text, limit, json } => {
            let found = find(&root, &Query { locale, subject, skill, standard, framework, age, status, text })?;
            for l in found.iter().take(limit) {
                if json {
                    println!("{l}");
                } else {
                    println!(
                        "{}  {:<8} {:<11} ages {}-{}  {:<9} {:>4} items  {}\n    {}",
                        l["id"].as_str().unwrap_or(""),
                        l["locale"].as_str().unwrap_or(""),
                        l["subject"].as_str().unwrap_or(""),
                        l["ages"][0],
                        l["ages"][1],
                        l["status"].as_str().unwrap_or(""),
                        l["items"],
                        l["title"].as_str().unwrap_or(""),
                        l["path"].as_str().unwrap_or("")
                    );
                }
            }
            if !json {
                eprintln!("{} lesson(s) match{}", found.len(), if found.len() > limit { format!("; showing {limit} (--limit)") } else { String::new() });
            }
            Ok(0)
        }
        Cmd::Stamp { paths } => {
            if paths.is_empty() {
                anyhow::bail!("name an items.jsonl file or a folder");
            }
            for p in paths {
                let (files, items, changed) = stamp(&p)?;
                println!("{}: {files} file(s), {items} item(s), {changed} id(s) written", p.display());
            }
            Ok(0)
        }
        Cmd::Id { locale, subject, slug } => {
            let id = canon::lesson_id(&locale, &subject, &slug);
            println!("{id}\nlessons/{locale}/{subject}/{}/{id}/", canon::shard(&id));
            Ok(0)
        }
    }
}
