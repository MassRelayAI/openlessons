//! Checks on one item: is the key a real, single answer; does the prompt give it away; what is its content signature.

use crate::canon::canonical;
use crate::text::{contains_tokens, tokens};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}
fn st<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn picture(v: &Value) -> String {
    match v.get("image") {
        Some(i) => format!("{}{}", st(i, "emoji"), st(i, "asset")),
        None => String::new(),
    }
}
/// Text as written, with runs of white space folded: capitals and punctuation can be the whole point of an item.
fn exact(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
/// What a choice shows: its text, or its picture.
fn label(c: &Value) -> String {
    let t = st(c, "text");
    let t = if t.is_empty() { st(c, "label") } else { t };
    if t.is_empty() {
        picture(c)
    } else {
        exact(t)
    }
}
/// Two choices with the same face cannot be told apart by a learner.
fn face(c: &Value) -> String {
    format!("{}|{}|{}", exact(st(c, "text")), exact(st(c, "label")), picture(c))
}
fn ids(list: &[Value]) -> Vec<&str> {
    list.iter().map(|c| st(c, "id")).collect()
}
fn str_list(v: &Value, k: &str) -> Vec<String> {
    arr(v, k).iter().filter_map(Value::as_str).map(str::to_string).collect()
}

fn unique(what: &str, values: &[String], out: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    for v in values {
        if !seen.insert(v) {
            out.push(format!("two {what} are the same ('{v}'): more than one answer could be right"));
            return;
        }
    }
}

/// The list a kind's answer points into.
fn choice_list<'a>(kind: &str, view: &'a Value) -> &'a [Value] {
    match kind {
        "tap_choice" | "multi_select" => arr(view, "options"),
        "sort_order" => arr(view, "items"),
        "tap_image" => arr(view, "regions"),
        "word_tiles" => arr(view, "tiles"),
        _ => &[],
    }
}

/// The ids an answer of this kind names (for drag_match: left ids then right ids).
fn answer_ids(kind: &str, answer: &Value) -> Vec<String> {
    match kind {
        "tap_choice" => vec![st(answer, "option").to_string()],
        "multi_select" => str_list(answer, "options"),
        "sort_order" | "word_tiles" => str_list(answer, "order"),
        "tap_image" => vec![st(answer, "region").to_string()],
        _ => vec![],
    }
}

/// Problems with the key: an answer that points nowhere, two right answers, a check a guess can pass.
pub fn answer_problems(item: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let kind = st(item, "kind");
    let (view, key) = (&item["view"], &item["key"]);
    let answer = &key["answer"];
    if kind == "story_choice" {
        let nodes = arr(view, "nodes");
        let node_ids = ids(nodes);
        unique("node ids", &node_ids.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &mut out);
        let by: BTreeMap<&str, &Value> = nodes.iter().map(|n| (st(n, "id"), n)).collect();
        if !by.contains_key(st(view, "start")) {
            out.push(format!("the start node '{}' does not exist", st(view, "start")));
            return out;
        }
        let (mut seen, mut todo, mut ends) = (BTreeSet::new(), vec![st(view, "start")], 0);
        while let Some(id) = todo.pop() {
            if !seen.insert(id) {
                continue;
            }
            let n = by[id];
            let end = n.get("end").and_then(Value::as_bool).unwrap_or(false);
            if end {
                ends += 1;
            } else if arr(n, "choices").is_empty() {
                out.push(format!("node '{id}' is not an end and offers no choice"));
            }
            for c in arr(n, "choices") {
                match by.get(st(c, "to")) {
                    Some(_) => todo.push(st(c, "to")),
                    None => out.push(format!("a choice of node '{id}' leads to '{}', which does not exist", st(c, "to"))),
                }
            }
        }
        if ends == 0 {
            out.push("no end can be reached from the start".to_string());
        }
        return out;
    }

    if kind == "drag_match" {
        let (left, right) = (arr(view, "left"), arr(view, "right"));
        unique("left cards", &left.iter().map(face).collect::<Vec<_>>(), &mut out);
        unique("right cards", &right.iter().map(face).collect::<Vec<_>>(), &mut out);
        let (lids, rids): (BTreeSet<&str>, BTreeSet<&str>) = (ids(left).into_iter().collect(), ids(right).into_iter().collect());
        let (mut l_used, mut r_used) = (BTreeSet::new(), BTreeSet::new());
        for p in arr(answer, "pairs") {
            let (l, r) = (p.get(0).and_then(Value::as_str).unwrap_or(""), p.get(1).and_then(Value::as_str).unwrap_or(""));
            if !lids.contains(l) || !rids.contains(r) {
                out.push(format!("the pair {l} - {r} names a card that does not exist"));
            }
            if !l_used.insert(l) || !r_used.insert(r) {
                out.push(format!("the pair {l} - {r} uses a card twice"));
            }
        }
        if l_used.len() != lids.len() {
            out.push("the answer does not pair every left card".to_string());
        }
    } else {
        let list = choice_list(kind, view);
        let all: Vec<String> = ids(list).iter().map(|s| s.to_string()).collect();
        unique("ids", &all, &mut out);
        // cards that are put in order may repeat ("the" twice, "3" twice): an order is judged by what the cards show
        if !matches!(kind, "sort_order" | "word_tiles") {
            unique("choices", &list.iter().map(face).collect::<Vec<_>>(), &mut out);
        }
        let named = answer_ids(kind, answer);
        for id in &named {
            if !all.contains(id) {
                out.push(format!("the answer names '{id}', which is not among the choices"));
            }
        }
        unique("answer entries", &named, &mut out);
        match kind {
            "sort_order" if named.len() != all.len() => out.push("the order does not place every card".to_string()),
            "word_tiles" => {
                let slots = view.get("slots").and_then(Value::as_u64).unwrap_or(0) as usize;
                if named.len() != slots {
                    out.push(format!("the answer has {} tiles for {slots} slots", named.len()));
                }
            }
            "multi_select" if named.len() == all.len() => out.push("every option is correct: nothing is being asked".to_string()),
            _ => {}
        }
        if st(item, "role") == "check" && matches!(kind, "tap_choice" | "tap_image") && all.len() < 3 {
            out.push("a check with fewer than three choices: a guess passes more than one time in three".to_string());
        }
    }
    let right = canonical(answer).unwrap_or_default();
    for d in arr(key, "distractors") {
        if canonical(&d["answer"]).unwrap_or_default() == right {
            out.push("a distractor is the right answer".to_string());
        }
        if kind != "drag_match" {
            let all = ids(choice_list(kind, view));
            for id in answer_ids(kind, &d["answer"]) {
                if !all.contains(&id.as_str()) {
                    out.push(format!("a distractor names '{id}', which is not among the choices"));
                }
            }
        }
    }
    if matches!(st(&key["verification"], "method"), "human" | "solver") && st(&key["verification"], "by").is_empty() {
        out.push("verification by a person or a solver must say who (`by`)".to_string());
    }
    out
}

/// Choices as token lists.
type Phrases = Vec<Vec<String>>;

/// The text of the right answer(s) and of the wrong ones, for the kinds where the learner picks among written choices.
fn right_and_wrong(item: &Value) -> Option<(Phrases, Phrases)> {
    let kind = st(item, "kind");
    // a region's label names its picture for a screen reader; it is not printed beside it, so tap_image is not judged here
    if !matches!(kind, "tap_choice" | "multi_select") {
        return None;
    }
    let named = answer_ids(kind, &item["key"]["answer"]);
    let (mut right, mut wrong) = (Vec::new(), Vec::new());
    for c in choice_list(kind, &item["view"]) {
        let t = st(c, "text");
        let toks = tokens(if t.is_empty() { st(c, "label") } else { t });
        if toks.is_empty() {
            continue;
        }
        if named.iter().any(|n| n == st(c, "id")) {
            right.push(toks);
        } else {
            wrong.push(toks);
        }
    }
    Some((right, wrong))
}

/// A leak: the prompt (or a hint) shows every right answer and none of the wrong ones.
/// A prompt that names right and wrong choices alike ("Which is more: 1 or 8?") tells the learner nothing.
pub fn leak_problems(item: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let Some((right, wrong)) = right_and_wrong(item) else { return out };
    if right.is_empty() {
        return out;
    }
    let prompt = &item["view"]["prompt"];
    let alt = prompt.get("image").map(|i| st(i, "alt")).unwrap_or("");
    let shown = tokens(&format!("{} . {alt}", st(prompt, "text")));
    let gives_away = |hay: &[String]| right.iter().all(|r| contains_tokens(hay, r)) && !wrong.iter().any(|w| contains_tokens(hay, w));
    if gives_away(&shown) {
        out.push(format!("the prompt shows the answer ('{}') and no other choice", right[0].join(" ")));
    }
    // a hint may guide; it gives the answer away only when the answer is a word of its own (three letters or more)
    if right.iter().all(|r| r.iter().map(|t| t.chars().count()).sum::<usize>() >= 3) {
        for h in arr(&item["key"], "hints").iter().filter_map(Value::as_str) {
            if gives_away(&tokens(h)) {
                out.push(format!("a hint shows the answer ('{}')", right[0].join(" ")));
            }
        }
    }
    out
}

/// What the learner is asked and what is accepted, with no ids and no order: two items with one signature are one item.
pub fn signature(item: &Value) -> String {
    let kind = st(item, "kind");
    let (view, key) = (&item["view"], &item["key"]);
    let prompt = &view["prompt"];
    let cue = prompt.get("audio").map(|a| format!("{}{}", st(a, "cue"), st(a, "asset"))).unwrap_or_default();
    let by_id = |list: &[Value], id: &str| list.iter().find(|c| st(c, "id") == id).map(label).unwrap_or_default();
    let mut faces: Vec<String>;
    let answer = match kind {
        "drag_match" => {
            let (left, right) = (arr(view, "left"), arr(view, "right"));
            faces = left.iter().chain(right).map(label).collect();
            let mut pairs: Vec<String> = arr(&key["answer"], "pairs")
                .iter()
                .map(|p| format!("{}>{}", by_id(left, p.get(0).and_then(Value::as_str).unwrap_or("")), by_id(right, p.get(1).and_then(Value::as_str).unwrap_or(""))))
                .collect();
            pairs.sort();
            pairs.join("+")
        }
        "story_choice" => {
            faces = arr(view, "nodes").iter().map(|n| exact(st(n, "text"))).collect();
            "story".to_string()
        }
        _ => {
            let list = choice_list(kind, view);
            faces = list.iter().map(label).collect();
            let mut named: Vec<String> = answer_ids(kind, &key["answer"]).iter().map(|id| by_id(list, id)).collect();
            if kind == "multi_select" {
                named.sort();
            }
            named.join(">")
        }
    };
    faces.sort();
    format!("{kind}#{}#{}#{cue}#{answer}#{}", exact(st(prompt, "text")), picture(prompt), faces.join("|"))
}

/// Every string a learner can be shown from this item, with where it sits.
pub fn texts(item: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut push = |place: &str, s: &str| {
        if !s.is_empty() {
            out.push((place.to_string(), s.to_string()));
        }
    };
    let (view, key) = (&item["view"], &item["key"]);
    push("prompt", st(&view["prompt"], "text"));
    if let Some(i) = view["prompt"].get("image") {
        push("prompt picture", st(i, "alt"));
    }
    for list in ["options", "left", "right", "items", "tiles", "regions"] {
        for c in arr(view, list) {
            push("choice", st(c, "text"));
            push("choice", st(c, "label"));
            if let Some(i) = c.get("image") {
                push("choice picture", st(i, "alt"));
            }
        }
    }
    for n in arr(view, "nodes") {
        push("story", st(n, "text"));
        for c in arr(n, "choices") {
            push("story choice", st(c, "label"));
        }
    }
    for h in arr(key, "hints").iter().filter_map(Value::as_str) {
        push("hint", h);
    }
    for d in arr(key, "distractors") {
        push("feedback", st(d, "feedback"));
    }
    out
}

/// Asset ids an item refers to.
pub fn asset_refs(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                if k == "asset" {
                    if let Some(s) = x.as_str() {
                        out.push(s.to_string());
                    }
                } else {
                    asset_refs(x, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| asset_refs(x, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn choice(prompt: &str, options: &[&str], right: &str) -> Value {
        json!({
            "kind": "tap_choice", "role": "check",
            "view": {"prompt": {"text": prompt}, "options": options.iter().map(|o| json!({"id": o, "text": o})).collect::<Vec<_>>()},
            "key": {"graded": true, "answer": {"option": right}, "hints": [], "verification": {"method": "constructed"}}
        })
    }

    #[test]
    fn a_sound_key_has_no_problems() {
        assert!(answer_problems(&choice("How many?", &["2", "3", "4"], "3")).is_empty());
    }

    #[test]
    fn a_missing_or_doubled_answer_is_found() {
        assert!(!answer_problems(&choice("How many?", &["2", "3", "4"], "5")).is_empty());
        assert!(!answer_problems(&choice("How many?", &["3", "3", "4"], "3")).is_empty());
        assert!(!answer_problems(&choice("How many?", &["2", "3"], "3")).is_empty());
        // capitals and full stops tell choices apart
        assert!(answer_problems(&choice("Which is right?", &["the pig sat.", "The pig sat.", "The pig sat"], "The pig sat.")).is_empty());
    }

    #[test]
    fn a_prompt_that_shows_only_the_answer_leaks() {
        assert!(!leak_problems(&choice("Which word is cat?", &["cat", "dog", "sun"], "cat")).is_empty());
        assert!(leak_problems(&choice("Which is more: 1 or 8?", &["1", "8", "10"], "8")).is_empty());
        assert!(leak_problems(&choice("Which animal says miaow?", &["cat", "dog", "sun"], "cat")).is_empty());
    }

    #[test]
    fn signature_ignores_ids_and_order() {
        let a = choice("How many?", &["2", "3", "4"], "3");
        let b = choice("How many?", &["4", "3", "2"], "3");
        assert_eq!(signature(&a), signature(&b));
        assert_ne!(signature(&a), signature(&choice("How many?", &["2", "3", "4"], "4")));
    }

    #[test]
    fn a_story_must_reach_an_end() {
        let story = |to: &str| json!({"kind": "story_choice", "view": {"prompt": {"text": "A story"}, "start": "n0", "nodes": [
            {"id": "n0", "text": "Go?", "end": false, "choices": [{"id": "c0", "label": "Yes", "to": to}]},
            {"id": "n1", "text": "Done.", "end": true, "choices": []}]}, "key": {"graded": false}});
        assert!(answer_problems(&story("n1")).is_empty());
        assert!(!answer_problems(&story("n9")).is_empty());
        assert!(!answer_problems(&story("n0")).is_empty());
    }
}
