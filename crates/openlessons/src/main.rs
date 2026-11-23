//! The `openlessons` command line. Exit codes: 0 ok, 1 refused (a gate is red), 2 usage or format error.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use openlessons::{canon, export, find, gaps, import, judge, looprun, observe, scan, selftest, stamp, text, write_index, Query, RULES};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "openlessons", version, about = "Validate, index, search and export the openlessons curriculum commons")]
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
        /// a git revision to judge the change against (origin/main): nothing reviewed there may change silently
        #[arg(long)]
        base: Option<String>,
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
    /// Export one course: a bundle, a learner bundle without answers, or an app pack
    Export {
        /// the course, as <locale>/<subject>/<slug> (see paths/)
        course: String,
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// bundle, learner or pack
        #[arg(long, default_value = "bundle")]
        format: String,
        /// the lowest status to take: published, reviewed, proposed or draft
        #[arg(long, default_value = "published")]
        min_status: String,
        /// the folder to write into
        #[arg(long)]
        out: PathBuf,
    },
    /// Read an app pack (path.json and its strings file) into records under a tree
    Import {
        /// the pack folder
        pack: PathBuf,
        /// the tree to write into
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Place the records of a proposal file into the tree, all at status `proposed`
    Apply {
        proposal: PathBuf,
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Print what a scan saw as observation/1 records (gates and standards coverage)
    Observe {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// the version the observations are about (a commit id)
        #[arg(long)]
        version: String,
        #[arg(long)]
        base: Option<String>,
        /// write to this file instead of standard output
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Serve the tree over the Model Context Protocol on standard input and output (read-only unless --allow-write)
    Mcp {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// the gate data folder (default: <root>/gates/data)
        #[arg(long)]
        data: Option<PathBuf>,
        /// let `propose_lesson` save proposal files under <root>/proposals/ (it writes nothing else)
        #[arg(long)]
        allow_write: bool,
    },
    /// List the gaps: objective x age band x language cells with no lesson or too few items, ranked by demand
    Gaps {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// how many gaps to print
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// only these subjects (comma separated)
        #[arg(long, value_delimiter = ',')]
        subject: Vec<String>,
        /// only these languages (comma separated)
        #[arg(long, value_delimiter = ',')]
        locale: Vec<String>,
        /// only these age bands (comma separated: b05-06,b07-08)
        #[arg(long, value_delimiter = ',')]
        band: Vec<String>,
        /// a cell with fewer items than this is still a gap
        #[arg(long, default_value_t = 8)]
        min_items: u64,
        /// one JSON object per gap, then a summary object
        #[arg(long)]
        json: bool,
        /// Markdown (for a pull request or a CI summary)
        #[arg(long)]
        md: bool,
    },
    /// The content loop: pick the top gaps, land drafts, or do both around a drafter command
    Loop {
        #[command(subcommand)]
        step: LoopStep,
    },
    /// List the standards registry: every framework with its licence class and what may be copied
    Registry {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Print the fingerprint list of a text file, for gates/data/protected/<framework>.txt (the text itself is never stored)
    Fingerprint {
        /// a plain-text file of the protected wording; keep it outside the repository
        file: PathBuf,
        /// tokens per fingerprint: 8 for running text, about 14 for a script counted by character
        #[arg(long, default_value_t = 8)]
        window: usize,
        /// keep one fingerprint in this many (1 keeps all); thin a long document
        #[arg(long, default_value_t = 1)]
        sample: u64,
    },
    /// Write content ids into items.jsonl files and sort them (a file or a folder)
    Stamp { paths: Vec<PathBuf> },
    /// Print the id and folder of a lesson
    Id { locale: String, subject: String, slug: String },
}

#[derive(clap::Args)]
struct PickArgs {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// how many gaps to take
    #[arg(long, default_value_t = 20)]
    top: usize,
    /// only these subjects (comma separated)
    #[arg(long, value_delimiter = ',')]
    subject: Vec<String>,
    /// only these languages (comma separated)
    #[arg(long, value_delimiter = ',')]
    locale: Vec<String>,
    /// only these age bands (comma separated)
    #[arg(long, value_delimiter = ',')]
    band: Vec<String>,
    #[arg(long, default_value_t = 8)]
    min_items: u64,
    /// the run folder: orders.json goes here, drafts under drafts/ (keep it outside the tree, or under .loop/)
    #[arg(long, default_value = ".loop")]
    run: PathBuf,
}

#[derive(clap::Args)]
struct LandArgs {
    /// commit on a new local branch loop/<run id>; push nothing, open nothing
    #[arg(long)]
    dry_run: bool,
    /// only write the files and the report into the working tree; touch no branch
    #[arg(long, conflicts_with = "dry_run")]
    no_git: bool,
    /// the run's date (default: today, UTC)
    #[arg(long)]
    date: Option<String>,
}

#[derive(Subcommand)]
enum LoopStep {
    /// Write the work orders (order/1) for the top gaps to <run>/orders.json
    Pick {
        #[command(flatten)]
        pick: PickArgs,
    },
    /// Judge the proposal files in a folder and place the ones that pass as `draft`, with a run report
    Land {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// the folder of proposal files (and orders.json, when the run has one)
        #[arg(long, default_value = ".loop")]
        from: PathBuf,
        #[arg(long, default_value_t = 8)]
        min_items: u64,
        #[command(flatten)]
        land: LandArgs,
    },
    /// pick, hand each order to a drafter command, then land: the whole loop in one command
    Run {
        #[command(flatten)]
        pick: PickArgs,
        /// a command that reads one order/1 on standard input and writes one proposal/1 on standard output
        #[arg(long)]
        drafter: String,
        #[command(flatten)]
        land: LandArgs,
    },
}

fn picked(p: &PickArgs) -> Result<(Vec<Value>, gaps::Options)> {
    let data = p.root.join("gates").join("data");
    let s = scan(&p.root, &data)?;
    let o = gaps::Options { min_items: p.min_items, subjects: p.subject.clone(), locales: p.locale.clone(), bands: p.band.clone() };
    let cells = gaps::pick(&gaps::report(&s, &o).gaps, p.top);
    fs::create_dir_all(&p.run)?;
    let orders = looprun::orders(&data, &s, &cells, p.min_items)?;
    fs::write(p.run.join("orders.json"), format!("{}\n", serde_json::to_string_pretty(&orders)?))?;
    Ok((orders, o))
}

fn landed(root: &Path, from: &Path, l: &LandArgs, gap: gaps::Options, failed: Vec<(String, String)>) -> Result<u8> {
    let mode = if l.no_git { looprun::Mode::Files } else if l.dry_run { looprun::Mode::DryRun } else { looprun::Mode::PullRequest };
    let data = root.join("gates").join("data");
    let r = looprun::land(&looprun::Land { root, data: &data, from, mode, date: l.date.clone().unwrap_or_else(looprun::today), gaps: gap, failed })?;
    print!("{}", looprun::markdown(&r));
    println!("\nreport: runs/{}.json and runs/{}.md", r["id"].as_str().unwrap_or(""), r["id"].as_str().unwrap_or(""));
    if let Some(b) = r["branch"].as_str() {
        println!("branch: {b}{}", if l.dry_run { " (dry run: local only, nothing pushed)" } else { "" });
    }
    if let Some(pr) = r["pullRequest"].as_str() {
        println!("pull request: {pr}");
    }
    Ok(0)
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

fn next_steps(written: &[String]) {
    for w in written {
        println!("wrote {w}");
    }
    println!("{} file(s) written. Next: `openlessons index`, then `openlessons validate --base origin/main`.", written.len());
}

fn run(cli: Cli) -> Result<u8> {
    match cli.cmd {
        Cmd::Selftest { root } => Ok(if gates_proven(&root, false)? { 0 } else { 1 }),
        Cmd::Validate { root, base, json, max } => {
            if !gates_proven(&root, true)? {
                return Ok(1);
            }
            let s = judge(&root, &root.join("gates").join("data"), base.as_deref())?;
            let per_rule = |rule: &str| s.findings.iter().filter(|f| f.rule == rule).count();
            if json {
                for f in &s.findings {
                    println!("{}", json!({"rule": f.rule, "path": f.path, "line": f.line, "message": f.message}));
                }
                let rules: serde_json::Map<String, Value> = RULES.iter().map(|r| (r.to_string(), json!(per_rule(r)))).collect();
                println!("{}", json!({"summary": {"findings": s.findings.len(), "rules": rules, "counts": s.counts, "base": base}}));
            } else {
                for rule in RULES {
                    let n = per_rule(rule);
                    let skipped = (*rule == "loop.base-respected" || *rule == "graph.ids-stable") && base.is_none();
                    let mark = if skipped { "--  " } else if n == 0 { "ok  " } else { "RED " };
                    let note = if skipped { " (no --base given: not judged)".to_string() } else if n == 0 { String::new() } else { format!(": {n} finding(s)") };
                    println!("{mark} {rule}{note}");
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
            let s = judge(&root, &data.unwrap_or_else(|| root.join("gates").join("data")), None)?;
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
        Cmd::Export { course, root, format, min_status, out } => {
            if !matches!(min_status.as_str(), "published" | "reviewed" | "proposed" | "draft") {
                anyhow::bail!("--min-status is published, reviewed, proposed or draft");
            }
            let c = export::load(&root, &course, &min_status)?;
            let files = export::write(&c, &format, &out)?;
            let items: usize = c.lessons.iter().map(|l| l.1.len()).sum();
            for f in &files {
                println!("wrote {}", f.display());
            }
            println!("{format}: {} lesson(s), {items} item(s), {} skill(s), status {min_status} or better", c.lessons.len(), c.skills.len());
            if export::rank(&min_status) < 3 {
                eprintln!("note: this export holds content no educator has reviewed. Do not serve it to children as it is.");
            }
            Ok(0)
        }
        Cmd::Import { pack, root } => {
            let path: Value = serde_json::from_str(&fs::read_to_string(pack.join("path.json")).context("path.json in the pack folder")?).context("path.json")?;
            let sfile = pack.join(format!("strings.{}.json", path["locale"].as_str().unwrap_or("")));
            let strings: Value = serde_json::from_str(&fs::read_to_string(&sfile).with_context(|| format!("{}", sfile.display()))?).context("the strings file")?;
            let records = import::from_pack(&path, &strings)?;
            next_steps(&import::write_records(&root, &records)?);
            Ok(0)
        }
        Cmd::Apply { proposal, root } => {
            let p: Value = serde_json::from_str(&fs::read_to_string(&proposal).with_context(|| format!("{}", proposal.display()))?).context("the proposal file")?;
            let records = import::from_proposal(&p)?;
            next_steps(&import::write_records(&root, &records)?);
            Ok(0)
        }
        Cmd::Observe { root, version, base, out } => {
            if !gates_proven(&root, true)? {
                return Ok(1);
            }
            let s = judge(&root, &root.join("gates").join("data"), base.as_deref())?;
            let body: String = observe::observations(&s, &version, base.is_some()).iter().map(|o| format!("{o}\n")).collect();
            match out {
                Some(p) => fs::write(&p, &body).with_context(|| format!("{}", p.display()))?,
                None => print!("{body}"),
            }
            Ok(0)
        }
        Cmd::Mcp { root, data, allow_write } => {
            let server = openlessons::mcp::Server { data: data.unwrap_or_else(|| root.join("gates").join("data")), root, allow_write };
            server.serve(std::io::stdin().lock(), std::io::stdout().lock())?;
            Ok(0)
        }
        Cmd::Gaps { root, top, subject, locale, band, min_items, json, md } => {
            let s = scan(&root, &root.join("gates").join("data"))?;
            let r = gaps::report(&s, &gaps::Options { min_items, subjects: subject, locales: locale, bands: band });
            if json {
                for c in r.gaps.iter().take(top) {
                    println!("{}", c.json());
                }
                println!("{}", json!({"summary": gaps::summary(&r)}));
            } else if md {
                print!("{}", gaps::markdown(&r, top));
            } else {
                println!("{:>3}  {:<44} {:<7} {:<8} {:<10} {:>6} {:>3} {:>6}  ready", "#", "objective", "band", "language", "gap", "demand", "fw", "usage");
                for (i, c) in r.gaps.iter().take(top).enumerate() {
                    println!("{:>3}  {:<44} {:<7} {:<8} {:<10} {:>6} {:>3} {:>6}  {}", i + 1, c.objective, c.band, c.locale, c.kind, c.demand, c.frameworks, c.usage, if c.ready { "yes" } else { "prerequisites first" });
                }
                eprintln!("{} cells; {} filled ({} reviewed); {} gaps{}", r.cells, r.filled, r.served, r.gaps.len(), if r.gaps.len() > top { format!("; showing {top} (--top)") } else { String::new() });
            }
            Ok(0)
        }
        Cmd::Loop { step } => match step {
            LoopStep::Pick { pick } => {
                let (orders, _) = picked(&pick)?;
                for o in &orders {
                    println!("{}  demand {}", o["cell"].as_str().unwrap_or(""), o["demand"]);
                }
                println!("{} work order(s) in {}. Answer each with a proposal file under {}, then `openlessons loop land --from {}`.", orders.len(), pick.run.join("orders.json").display(), pick.run.join("drafts").display(), pick.run.display());
                Ok(0)
            }
            LoopStep::Land { root, from, min_items, land } => {
                if !gates_proven(&root, true)? {
                    return Ok(1);
                }
                landed(&root, &from, &land, gaps::Options { min_items, ..Default::default() }, vec![])
            }
            LoopStep::Run { pick, drafter, land } => {
                if !gates_proven(&pick.root, true)? {
                    return Ok(1);
                }
                let (orders, gap) = picked(&pick)?;
                let failed = looprun::draft_with(&pick.run, &orders, &drafter)?;
                // the run's filters chose the orders; the report counts gaps over the whole tree
                let _ = gap;
                landed(&pick.root, &pick.run, &land, gaps::Options { min_items: pick.min_items, ..Default::default() }, failed)
            }
        },
        Cmd::Registry { root, json } => {
            let mut classes: std::collections::BTreeMap<String, usize> = Default::default();
            let mut dirs: Vec<PathBuf> = fs::read_dir(root.join("standards"))?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir()).collect();
            dirs.sort();
            for d in &dirs {
                let f: Value = serde_json::from_str(&fs::read_to_string(d.join("framework.json"))?).with_context(|| format!("{}", d.display()))?;
                let t = &f["terms"];
                let s = |v: &Value| v.as_str().unwrap_or("").to_string();
                *classes.entry(format!("{} / {}", s(&t["class"]), s(&t["quote"]))).or_default() += 1;
                if json {
                    println!("{f}");
                } else {
                    println!("{:<28} {:<14} {:<15} {:<15} {:<16} read {}  {}", s(&f["id"]), s(&f["jurisdiction"]), s(&t["class"]), s(&t["quote"]), t["licence"].as_str().unwrap_or("-"), t["reviewed"].as_str().unwrap_or("never"), s(&f["title"]));
                }
            }
            if !json {
                eprintln!("{} framework(s): {}", dirs.len(), classes.iter().map(|(k, v)| format!("{v} {k}")).collect::<Vec<_>>().join("; "));
            }
            Ok(0)
        }
        Cmd::Fingerprint { file, window, sample } => {
            let raw = String::from_utf8_lossy(&fs::read(&file).with_context(|| format!("{}", file.display()))?).to_string();
            println!("# window={window}");
            for h in text::fingerprints(&raw, window, sample) {
                println!("{h}");
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
