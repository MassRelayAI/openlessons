//! `observe`: what a scan saw, as `observation/1` records a score ledger can ingest (the rules are in scoregraph/).
//!
//! Two families, named as the ledger names them. `quality.<gate>`: one observation per gate with the number of findings. `feature.coverage.level.*`: one per standards level and
//! locale, with how many skills, lessons and items reach that level and how much of it a person has reviewed.

use crate::scan::{Scan, RULES};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const PRODUCT: &str = "openlessons";

fn settled(status: &str) -> bool {
    matches!(status, "reviewed" | "published")
}

/// Coverage rows: (framework, level, locale, facts). A level nothing reaches is reported once, with locale `any`.
pub fn coverage(scan: &Scan) -> Vec<(String, String, String, Value)> {
    // every node under a level, the level included
    let level_of = |id: &str| -> Option<String> {
        let (mut cur, mut hops) = (id.to_string(), 0);
        loop {
            let n = scan.standards.get(&cur)?;
            if n.kind == "level" {
                return Some(cur);
            }
            if n.parent.is_empty() || hops > 16 {
                return None;
            }
            cur = n.parent.clone();
            hops += 1;
        }
    };
    let mut out = Vec::new();
    for (level, node) in scan.standards.iter().filter(|(_, n)| n.kind == "level") {
        let under: BTreeSet<&String> = scan.standards.keys().filter(|id| level_of(id).as_deref() == Some(level.as_str())).collect();
        let objectives: BTreeSet<&String> = under.iter().copied().filter(|id| scan.standards[*id].kind == "objective").collect();
        let rows: Vec<_> = scan.alignments.iter().filter(|a| under.contains(&a.standard)).collect();
        let mut by_locale: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for l in &scan.lessons {
            let teaches = l["skills"].as_array().into_iter().flatten().filter_map(Value::as_str).any(|k| rows.iter().any(|a| a.skill == k));
            if teaches {
                by_locale.entry(l["locale"].as_str().unwrap_or("").to_string()).or_default().push(l);
            }
        }
        if by_locale.is_empty() {
            by_locale.insert("any".to_string(), vec![]);
        }
        for (locale, lessons) in by_locale {
            let taught: BTreeSet<&str> = lessons.iter().flat_map(|l| l["skills"].as_array().into_iter().flatten().filter_map(Value::as_str)).collect();
            let here: Vec<_> = rows.iter().filter(|a| taught.contains(a.skill.as_str())).collect();
            let count = |f: &dyn Fn(&crate::scan::Aligned) -> bool| here.iter().filter(|a| f(a)).count();
            let objectives_reached: BTreeSet<&String> = here.iter().map(|a| &a.standard).filter(|s| objectives.contains(s)).collect();
            let facts = json!({
                "skills": here.iter().map(|a| a.skill.as_str()).collect::<BTreeSet<_>>().len(),
                "lessons": lessons.len(),
                "lessons_published": lessons.iter().filter(|l| l["status"] == "published").count(),
                "lessons_reviewed": lessons.iter().filter(|l| settled(l["status"].as_str().unwrap_or(""))).count(),
                "items": lessons.iter().filter_map(|l| l["items"].as_u64()).sum::<u64>(),
                "alignments": here.len(),
                "exact_reviewed": count(&|a| a.strength == "exact" && settled(&a.status)),
                "exact_proposed": count(&|a| a.strength == "exact" && !settled(&a.status)),
                "partial": count(&|a| a.strength == "partial"),
                "related": count(&|a| a.strength == "related"),
                "objectives": objectives.len(),
                "objectives_reached": objectives_reached.len(),
            });
            out.push((node.framework.clone(), level.clone(), locale, facts));
        }
    }
    out
}

/// The observations of one scan. `base_measured` says whether the scan was compared with a base revision.
pub fn observations(scan: &Scan, version: &str, base_measured: bool) -> Vec<Value> {
    let probe = json!({"id": "openlessons", "version": env!("CARGO_PKG_VERSION")});
    let records: u64 = scan.counts.values().sum();
    let mut out = Vec::new();
    for rule in RULES {
        let findings = scan.findings.iter().filter(|f| f.rule == *rule).count();
        let mut o = json!({
            "schema": "observation/1", "product": PRODUCT, "version": version, "rule": {"id": format!("quality.{rule}"), "version": 1},
            "context": {"scope": "tree"}, "probe": probe, "assertions": records,
        });
        if (*rule == "loop.base-respected" || *rule == "graph.ids-stable") && !base_measured {
            o["outcome"] = json!("unmeasured");
        } else {
            o["facts"] = json!({"findings": findings, "records": records});
        }
        out.push(o);
    }
    for (framework, level, locale, facts) in coverage(scan) {
        for rule in ["feature.coverage.level.taught", "feature.coverage.level.reviewed", "feature.coverage.level.exact"] {
            out.push(json!({
                "schema": "observation/1", "product": PRODUCT, "version": version, "rule": {"id": rule, "version": 1},
                "context": {"framework": framework, "level": level, "locale": locale}, "facts": facts, "probe": probe,
            }));
        }
    }
    out
}
