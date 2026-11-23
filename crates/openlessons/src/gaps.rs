//! The gap report: which cells of objective x age band x language have no lesson, or too few items, ranked by demand.
//!
//! A cell exists for every objective of the topic graph, each age band it is taught in, and each language of instruction
//! it applies to. A lesson fills a cell when it teaches the objective, in that language, for ages inside the band.
//! Demand comes from the standards first: one point for every framework of the registry that teaches the objective's
//! subject area at the band's ages, plus the weight of each alignment we hold for it (exact 10, partial 4, related 1), then from usage counts under `signals/`, which hold numbers only.
//!
//! The report needs no model and no secret: it is a pure function of the tree.

use crate::scan::Scan;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct Options {
    /// a cell with a lesson but fewer items than this is still a gap
    pub min_items: u64,
    pub subjects: Vec<String>,
    pub locales: Vec<String>,
    pub bands: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options { min_items: 8, subjects: vec![], locales: vec![], bands: vec![] }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub objective: String,
    pub subject: String,
    pub band: String,
    pub band_min: i64,
    pub locale: String,
    /// `no-lesson` or `few-items`
    pub kind: &'static str,
    pub demand: u64,
    /// frameworks of the registry that cover the objective's subject area at the band's ages
    pub frameworks: usize,
    /// usage counts for the cell
    pub usage: u64,
    pub lessons: u64,
    pub items: u64,
    /// every prerequisite already has a lesson in this language
    pub ready: bool,
}

impl Cell {
    pub fn id(&self) -> String {
        format!("{}@{}@{}", self.objective, self.band, self.locale)
    }
    pub fn json(&self) -> Value {
        json!({
            "cell": self.id(), "objective": self.objective, "subject": self.subject, "band": self.band, "locale": self.locale,
            "gap": self.kind, "demand": self.demand, "frameworks": self.frameworks, "usage": self.usage,
            "lessons": self.lessons, "items": self.items, "ready": self.ready,
        })
    }
}

#[derive(Debug, Default)]
pub struct Report {
    /// every cell the filters let through
    pub cells: usize,
    /// cells with a lesson and enough items, whatever their status
    pub filled: usize,
    /// filled cells whose lessons are all `reviewed` or `published`: what a child-facing product may read
    pub served: usize,
    /// the gaps, most wanted first
    pub gaps: Vec<Cell>,
    /// subject -> (cells, filled)
    pub by_subject: BTreeMap<String, (usize, usize)>,
    /// locale -> (cells, filled)
    pub by_locale: BTreeMap<String, (usize, usize)>,
}

/// A lesson, as a cell sees it: (youngest age, oldest age, items, reviewed or published).
type Taught = (i64, i64, u64, bool);

fn weight(strength: &str) -> u64 {
    match strength {
        "exact" => 10,
        "partial" => 4,
        _ => 1,
    }
}

/// floor(log2(1 + n)): usage counts add to demand slowly, so one busy product cannot drown the standards
fn log2(n: u64) -> u64 {
    63 - (n + 1).leading_zeros() as u64
}

pub fn report(scan: &Scan, o: &Options) -> Report {
    // objective -> framework -> the strongest alignment
    let mut asked: BTreeMap<&str, BTreeMap<&str, u64>> = BTreeMap::new();
    for a in &scan.alignments {
        let framework = a.standard.split(':').next().unwrap_or("");
        let w = asked.entry(a.skill.as_str()).or_default().entry(framework).or_default();
        *w = (*w).max(weight(&a.strength));
    }
    let mut used: BTreeMap<(&str, &str, &str), u64> = BTreeMap::new();
    for s in &scan.signals {
        *used.entry((s.objective.as_str(), s.band.as_str(), s.locale.as_str())).or_default() += s.count;
    }
    // (objective, locale) -> the lessons that teach it
    let mut taught: BTreeMap<(&str, &str), Vec<Taught>> = BTreeMap::new();
    for l in &scan.lessons {
        let locale = l["locale"].as_str().unwrap_or("");
        let row = (l["ages"][0].as_i64().unwrap_or(0), l["ages"][1].as_i64().unwrap_or(0), l["items"].as_u64().unwrap_or(0), matches!(l["status"].as_str(), Some("reviewed" | "published")));
        for s in l["skills"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            taught.entry((s, locale)).or_default().push(row);
        }
    }
    let wanted = |list: &Vec<String>, x: &str| list.is_empty() || list.iter().any(|y| y == x);

    let mut r = Report::default();
    for (id, obj) in &scan.objectives {
        if !wanted(&o.subjects, &obj.subject) {
            continue;
        }
        let standards: u64 = asked.get(id.as_str()).map(|f| f.values().sum()).unwrap_or(0);
        let area = scan.areas.get(&obj.subject).map(String::as_str).unwrap_or("");
        for band in scan.bands.iter().filter(|b| obj.bands.contains(&b.id) && wanted(&o.bands, &b.id)) {
            for locale in obj.locales.iter().filter(|l| wanted(&o.locales, l)) {
                let rows: Vec<&Taught> =
                    taught.get(&(id.as_str(), locale.as_str())).into_iter().flatten().filter(|t| t.0 <= band.max && band.min <= t.1).collect();
                let (lessons, items) = (rows.len() as u64, rows.iter().map(|t| t.2).sum::<u64>());
                let filled = lessons > 0 && items >= o.min_items;
                r.cells += 1;
                let (s, l) = (r.by_subject.entry(obj.subject.clone()).or_default(), r.by_locale.entry(locale.clone()).or_default());
                s.0 += 1;
                l.0 += 1;
                if filled {
                    r.filled += 1;
                    s.1 += 1;
                    l.1 += 1;
                    if rows.iter().all(|t| t.3) {
                        r.served += 1;
                    }
                    continue;
                }
                // every framework of the registry that teaches this area at these ages asks for the cell, whether or not
                // we have aligned the objective to one of its nodes yet
                let cover = scan
                    .registry
                    .iter()
                    .filter(|f| {
                        let a = &f["span"]["ages"];
                        a["min"].as_i64().unwrap_or(0) <= band.max
                            && band.min <= a["max"].as_i64().unwrap_or(0)
                            && f["subjects"].as_array().into_iter().flatten().filter_map(Value::as_str).any(|s| s == area || s == "all")
                    })
                    .count();
                let usage = used.get(&(id.as_str(), band.id.as_str(), locale.as_str())).copied().unwrap_or(0);
                let ready = obj.prerequisites.iter().all(|p| taught.contains_key(&(p.as_str(), locale.as_str())) || !scan.objectives.get(p).is_some_and(|x| x.locales.contains(locale)));
                r.gaps.push(Cell {
                    objective: id.clone(),
                    subject: obj.subject.clone(),
                    band: band.id.clone(),
                    band_min: band.min,
                    locale: locale.clone(),
                    kind: if lessons == 0 { "no-lesson" } else { "few-items" },
                    demand: cover as u64 + standards + 5 * log2(usage),
                    frameworks: cover,
                    usage,
                    lessons,
                    items,
                    ready,
                });
            }
        }
    }
    r.gaps.sort_by(|a, b| (b.demand, b.ready, a.band_min, &a.objective, &a.locale).cmp(&(a.demand, a.ready, b.band_min, &b.objective, &b.locale)));
    r
}

/// The next cells to draft: the ranked gaps, dealt out in turn to each subject and language so one run does not spend
/// itself on a single corner, prerequisites before what needs them, and one objective at most once per run while
/// others wait.
pub fn pick(gaps: &[Cell], top: usize) -> Vec<Cell> {
    let mut groups: Vec<((&str, &str), Vec<&Cell>)> = Vec::new();
    for c in gaps {
        let key = (c.subject.as_str(), c.locale.as_str());
        match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => g.1.push(c),
            None => groups.push((key, vec![c])),
        }
    }
    let mut out: Vec<Cell> = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    // cells whose prerequisites are already taught in that language come first; within each, every objective new to
    // the run first, then (if the run is still short) letting objectives repeat
    for (ready_only, distinct) in [(true, true), (true, false), (false, true), (false, false)] {
        loop {
            let before = out.len();
            for (_, cells) in &groups {
                if out.len() >= top {
                    return out;
                }
                if let Some(c) = cells.iter().find(|c| (c.ready || !ready_only) && !taken.contains(&c.id()) && (!distinct || !seen.contains(c.objective.as_str()))) {
                    seen.insert(c.objective.as_str());
                    taken.insert(c.id());
                    out.push((*c).clone());
                }
            }
            if out.len() == before {
                break;
            }
        }
    }
    out
}

pub fn summary(r: &Report) -> Value {
    let table = |m: &BTreeMap<String, (usize, usize)>| m.iter().map(|(k, v)| (k.clone(), json!({"cells": v.0, "filled": v.1, "gaps": v.0 - v.1}))).collect::<serde_json::Map<String, Value>>();
    json!({
        "schema": "gaps/1", "cells": r.cells, "filled": r.filled, "served": r.served, "gaps": r.gaps.len(),
        "bySubject": table(&r.by_subject), "byLocale": table(&r.by_locale),
    })
}

/// The report as Markdown: totals, the two breakdowns, and the top of the ranking.
pub fn markdown(r: &Report, top: usize) -> String {
    let mut s = format!(
        "## openlessons gap report\n\n{} cells (objective x age band x language): {} filled, {} gaps. {} filled cells are reviewed: only those may reach a child.\n\n",
        r.cells,
        r.filled,
        r.gaps.len(),
        r.served
    );
    for (title, m) in [("subject", &r.by_subject), ("language", &r.by_locale)] {
        s.push_str(&format!("| {title} | cells | filled | gaps |\n|---|---:|---:|---:|\n"));
        for (k, v) in m {
            s.push_str(&format!("| {k} | {} | {} | {} |\n", v.0, v.1, v.0 - v.1));
        }
        s.push('\n');
    }
    s.push_str(&format!("### Top {} gaps by demand\n\n| # | objective | band | language | gap | demand | frameworks | usage | ready |\n|---:|---|---|---|---|---:|---:|---:|---|\n", top.min(r.gaps.len())));
    for (i, c) in r.gaps.iter().take(top).enumerate() {
        s.push_str(&format!("| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n", i + 1, c.objective, c.band, c.locale, c.kind, c.demand, c.frameworks, c.usage, if c.ready { "yes" } else { "prerequisites first" }));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(objective: &str, subject: &str, locale: &str, demand: u64) -> Cell {
        Cell { objective: objective.into(), subject: subject.into(), band: "b05-06".into(), band_min: 5, locale: locale.into(), kind: "no-lesson", demand, frameworks: 1, usage: 0, lessons: 0, items: 0, ready: true }
    }

    #[test]
    fn a_pick_is_dealt_across_subjects_and_languages_with_new_objectives_first() {
        let gaps = vec![cell("m.a.one", "m", "en", 9), cell("m.a.one", "m", "id", 9), cell("m.a.two", "m", "en", 8), cell("m.a.two", "m", "id", 8), cell("s.a.one", "s", "en", 1)];
        let ids = |n| pick(&gaps, n).iter().map(Cell::id).collect::<Vec<_>>();
        assert_eq!(ids(3), ["m.a.one@b05-06@en", "m.a.two@b05-06@id", "s.a.one@b05-06@en"]);
        // a longer run comes back for the objectives it skipped, and never takes a cell twice
        assert_eq!(ids(9).len(), 5);
        // a cell whose prerequisites are not taught yet waits for the ones that are ready
        let mut later = gaps.clone();
        later[0].ready = false;
        assert_eq!(pick(&later, 1)[0].id(), "m.a.two@b05-06@en");
    }

    #[test]
    fn usage_adds_to_demand_slowly() {
        assert_eq!((log2(0), log2(1), log2(20), log2(1_000_000)), (0, 1, 4, 19));
    }
}
