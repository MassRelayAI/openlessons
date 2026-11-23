//! Text checks shared by the gates: tokens, reading level, personal data, the safety lexicon, ICU arguments.

use crate::canon::sha256_hex;
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// CJK ideographs and kana: counted by character, never as letters of a word.
pub fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F)
}

/// Marks that sit inside a word without being letters: combining accents, the signs and the virama of Devanagari, the
/// vowel marks of Arabic, and the joiners. Without them a Hindi word would split at every conjunct.
fn is_word_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x0900..=0x0963 | 0x0966..=0x097F | 0x064B..=0x065F | 0x0670 | 0x200C | 0x200D)
}

/// Lower-cased runs of letters and digits; each CJK character is its own token.
pub fn tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if is_cjk(c) {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            out.push(c.to_string());
        } else if c.is_alphanumeric() || is_word_mark(c) {
            cur.extend(c.to_lowercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// True when `needle` appears in `hay` as a run of whole tokens.
pub fn contains_tokens(hay: &[String], needle: &[String]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

// ---------- reading level ----------

#[derive(Debug, Clone)]
pub struct ReadingBand {
    pub max_age: i64,
    pub max_sentence_words: usize,
    pub max_word_chars: usize,
    pub max_sentence_chars: usize,
}

pub fn reading_bands(data: &Value) -> Vec<ReadingBand> {
    let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(u64::MAX) as usize;
    data.get("bands")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|b| ReadingBand {
                    max_age: b.get("maxAge").and_then(Value::as_i64).unwrap_or(i64::MAX),
                    max_sentence_words: n(b, "maxSentenceWords"),
                    max_word_chars: n(b, "maxWordChars"),
                    max_sentence_chars: n(b, "maxSentenceChars"),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The band of the youngest learner the lesson says it is for.
pub fn band_for(bands: &[ReadingBand], min_age: i64) -> Option<&ReadingBand> {
    bands.iter().find(|b| min_age <= b.max_age)
}

/// `segmentation` is the locale's: `space` counts words, `character` counts CJK characters per sentence.
pub fn reading_issue(text: &str, segmentation: &str, band: &ReadingBand) -> Option<String> {
    for sentence in text.split(['.', '!', '?', '。', '！', '？', '\n', ';', ':', '।', '؟', '۔', '¿', '¡']) {
        let toks = tokens(sentence);
        if segmentation == "character" {
            let n = sentence.chars().filter(|c| is_cjk(*c)).count();
            if n > band.max_sentence_chars {
                return Some(format!("a sentence of {n} characters (limit {})", band.max_sentence_chars));
            }
            continue;
        }
        if toks.len() > band.max_sentence_words {
            return Some(format!("a sentence of {} words (limit {})", toks.len(), band.max_sentence_words));
        }
        for t in &toks {
            // marks are not letters: a Hindi or Arabic word is as long as its letters, not its signs
            let letters = t.chars().filter(|c| c.is_alphabetic() && !is_cjk(*c) && !is_word_mark(*c)).count();
            if letters > band.max_word_chars {
                return Some(format!("the word '{t}' has {letters} letters (limit {})", band.max_word_chars));
            }
        }
    }
    None
}

// ---------- personal data ----------

fn pii_patterns() -> &'static [(&'static str, Regex)] {
    static P: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    P.get_or_init(|| {
        let r = |s: &str| Regex::new(s).expect("pattern");
        vec![
            ("an email address", r(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}")),
            ("a web address", r(r"(?i)(https?://|\bwww\.)\S+")),
            ("a phone number", r(r"\+\d[\d ().-]{7,}\d|\b\d{3}[-.]\d{3,4}[-.]\d{4}\b|\(\d{2,4}\)\s?\d{3,}")),
            ("an account handle", r(r"(^|[\s(])@[A-Za-z0-9_]{2,}")),
        ]
    })
}

pub fn pii_issue(text: &str) -> Option<&'static str> {
    pii_patterns().iter().find(|(_, re)| re.is_match(text)).map(|(what, _)| *what)
}

// ---------- safety lexicon ----------

#[derive(Debug, Clone)]
pub struct Lexicon {
    /// the lexicon binds lessons whose youngest learner is this age or younger
    pub max_age: i64,
    /// term (tokens joined by one space) -> category
    pub terms: BTreeMap<String, String>,
    /// sha256 of a lower-cased word, for words we do not print
    pub hashed: BTreeSet<String>,
}

impl Lexicon {
    pub fn from_json(v: &Value) -> Lexicon {
        let mut lex = Lexicon { max_age: v.get("maxAge").and_then(Value::as_i64).unwrap_or(i64::MAX), terms: BTreeMap::new(), hashed: BTreeSet::new() };
        if let Some(cats) = v.get("terms").and_then(Value::as_object) {
            for (cat, list) in cats {
                for t in list.as_array().into_iter().flatten().filter_map(Value::as_str) {
                    lex.terms.insert(tokens(t).join(" "), cat.clone());
                }
            }
        }
        for h in v.get("hashed").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            lex.hashed.insert(h.to_string());
        }
        lex
    }

    /// The first blocked term in the text, with its category.
    pub fn hit(&self, text: &str) -> Option<(String, String)> {
        let toks = tokens(text);
        for (term, cat) in &self.terms {
            let needle: Vec<String> = term.split(' ').map(str::to_string).collect();
            if contains_tokens(&toks, &needle) {
                return Some((term.clone(), cat.clone()));
            }
        }
        for t in &toks {
            if self.hashed.contains(&sha256_hex(t.as_bytes())) {
                return Some(("a hashed term".to_string(), "profanity".to_string()));
            }
        }
        None
    }
}

// ---------- copied text ----------

/// What the gate `content.no-copied-text` looks for in our own wording (gates/data/protected.json): marks that travel
/// with copied material, and fingerprints of protected wording. A fingerprint is the hash of a run of `window` words, so
/// the list can say "this sentence is the publisher's" without holding the sentence.
#[derive(Debug, Clone, Default)]
pub struct Protected {
    /// substrings, matched as written (the copyright sign)
    pub symbols: Vec<String>,
    /// phrases, matched as runs of whole tokens
    pub marks: Vec<Vec<String>>,
    pub window: usize,
    /// fingerprint -> framework id
    pub prints: BTreeMap<String, String>,
}

/// The fingerprints of a text: one per run of `window` tokens.
pub fn fingerprints(text: &str, window: usize) -> Vec<String> {
    let toks = tokens(text);
    if window == 0 || toks.len() < window {
        return Vec::new();
    }
    toks.windows(window).map(|w| sha256_hex(w.join(" ").as_bytes())[..16].to_string()).collect()
}

impl Protected {
    pub fn from_json(v: &Value) -> Protected {
        let list = |k: &str| v.get(k).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>();
        let mut p = Protected { symbols: list("symbols"), marks: list("marks").iter().map(|m| tokens(m)).filter(|m| !m.is_empty()).collect(), window: v.get("window").and_then(Value::as_u64).unwrap_or(6) as usize, prints: BTreeMap::new() };
        for (framework, prints) in v.get("fingerprints").and_then(Value::as_object).into_iter().flatten() {
            for h in prints.as_array().into_iter().flatten().filter_map(Value::as_str) {
                p.prints.insert(h.to_string(), framework.clone());
            }
        }
        p
    }

    /// Why this text looks copied, if it does.
    pub fn issue(&self, text: &str) -> Option<String> {
        if let Some(s) = self.symbols.iter().find(|s| text.contains(s.as_str())) {
            return Some(format!("'{s}' marks copied material: write it in your own words and credit sources in `attribution`"));
        }
        let toks = tokens(text);
        if let Some(m) = self.marks.iter().find(|m| contains_tokens(&toks, m)) {
            return Some(format!("'{}' marks copied material: write it in your own words and credit sources in `attribution`", m.join(" ")));
        }
        if self.window > 0 && toks.len() >= self.window {
            for w in toks.windows(self.window) {
                if let Some(framework) = self.prints.get(&sha256_hex(w.join(" ").as_bytes())[..16]) {
                    return Some(format!("'{}' is the wording of {framework}, whose terms do not allow copying it: paraphrase", w.join(" ")));
                }
            }
        }
        None
    }
}

/// The language subtag of a BCP 47 tag (`zh-Hans` -> `zh`).
pub fn language(tag: &str) -> &str {
    tag.split('-').next().unwrap_or(tag)
}

// ---------- ICU MessageFormat, the part we rely on ----------

/// Braces must balance and an argument must be a plain name: `{character}` or `{count, plural, ...}`.
pub fn icu_issue(text: &str) -> Option<String> {
    let mut depth = 0usize;
    let mut name = String::new();
    let mut in_name = false;
    for c in text.chars() {
        match c {
            '{' => {
                depth += 1;
                in_name = true;
                name.clear();
            }
            '}' | ',' if in_name => {
                let n = name.trim();
                let ok = !n.is_empty()
                    && n.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                    && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                // a nested message such as `one {# apple}` is text, not an argument
                if !ok && depth == 1 {
                    return Some(format!("'{{{n}}}' is not an ICU argument name"));
                }
                in_name = false;
                if c == '}' {
                    depth = depth.saturating_sub(1);
                }
            }
            '}' => {
                if depth == 0 {
                    return Some("a closing brace without an opening one".to_string());
                }
                depth -= 1;
            }
            _ if in_name => name.push(c),
            _ => {}
        }
    }
    (depth != 0).then(|| "an opening brace is never closed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band() -> ReadingBand {
        ReadingBand { max_age: 7, max_sentence_words: 8, max_word_chars: 10, max_sentence_chars: 12 }
    }

    #[test]
    fn tokens_split_words_and_cjk() {
        assert_eq!(tokens("What does 晚安 (wǎn ān) mean?"), ["what", "does", "晚", "安", "wǎn", "ān", "mean"]);
        assert!(contains_tokens(&tokens("Which letter says n?"), &tokens("N")));
        assert!(!contains_tokens(&tokens("Which one is new?"), &tokens("n")));
        // a conjunct and a vowel mark stay inside their word
        assert_eq!(tokens("बच्चे खेलते हैं।").len(), 3);
        assert_eq!(tokens("كَتَبَ الولد").len(), 2);
    }

    #[test]
    fn reading_level_limits_sentence_and_word() {
        assert!(reading_issue("The cat sat. The dog ran!", "space", &band()).is_none());
        assert!(reading_issue("one two three four five six seven eight nine", "space", &band()).is_some());
        assert!(reading_issue("An extraordinarily big cat.", "space", &band()).is_some());
        assert!(reading_issue("你好。", "character", &band()).is_none());
        assert!(reading_issue("我我我我我我我我我我我我我我", "character", &band()).is_some());
    }

    #[test]
    fn pii_finds_contact_details_and_leaves_sums_alone() {
        assert!(pii_issue("write to sam@example.org").is_some());
        assert!(pii_issue("see https://example.org/x").is_some());
        assert!(pii_issue("call +44 20 7946 0000").is_some());
        assert!(pii_issue("call 555-010-9999").is_some());
        assert!(pii_issue("follow @some_handle").is_some());
        assert!(pii_issue("Count back from 18: 17 16 15").is_none());
        assert!(pii_issue("10 + 10 = 20, and 1000 + 2000 = 3000").is_none());
    }

    #[test]
    fn lexicon_matches_whole_words_phrases_and_hashes() {
        let lex = Lexicon::from_json(&serde_json::json!({
            "terms": {"weapons": ["gun"], "commerce": ["buy now"]},
            "hashed": [sha256_hex(b"zzword")]
        }));
        assert!(lex.hit("He has a gun.").is_some());
        assert!(lex.hit("A day of rain has begun.").is_none());
        assert!(lex.hit("Buy now!").is_some());
        assert!(lex.hit("a ZZWORD here").is_some());
        assert!(lex.hit("The sun is hot.").is_none());
    }

    #[test]
    fn copied_text_is_found_by_mark_and_by_fingerprint() {
        let theirs = "Pupils should recite the seven wonders in order every single morning";
        let p = Protected::from_json(&serde_json::json!({
            "symbols": ["\u{a9}"], "marks": ["all rights reserved"], "window": 6,
            "fingerprints": {"example-framework": fingerprints(theirs, 6)}
        }));
        assert!(p.issue("Count the dots. \u{a9} 2020 A Publisher").is_some());
        assert!(p.issue("All Rights Reserved.").is_some());
        assert!(p.issue("I can say the seven wonders, one by one.").is_none());
        // six of their words in a row, inside our sentence
        assert!(p.issue("Now recite the seven wonders in order, please.").is_some());
        assert!(p.issue("Recite the seven wonders.").is_none());
        assert_eq!(fingerprints("one two three", 6), Vec::<String>::new());
    }

    #[test]
    fn icu_braces() {
        assert!(icu_issue("Hello {character}!").is_none());
        assert!(icu_issue("{count, plural, one {# apple} other {# apples}}").is_none());
        assert!(icu_issue("Hello {character").is_some());
        assert!(icu_issue("Hello }").is_some());
        assert!(icu_issue("Hello {two words}").is_some());
    }
}
