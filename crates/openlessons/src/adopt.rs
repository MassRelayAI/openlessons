//! `adopt`: open material (OER) from a registered source, imported as `draft` lessons with the item's own licence and
//! credit.
//!
//! The rules, in the order they are applied:
//!
//! 1. A source is a record under `sources/` (`source/1`): its publisher, the licences it publishes under, where an item
//!    states its own licence, and its class: `adopt`, `reference-only` or `sa-layer`.
//! 2. The licence is read per ITEM, never per site: the adapter takes it from the item itself (an IM lesson page's notice,
//!    a StoryWeaver story's licence line) and refuses an item that states none.
//! 3. Only CC BY, CC0 and public-domain items enter the core (`gates/data/licences.json`, `text.adoptable`). A
//!    ShareAlike item is kept out of it, and NonCommercial, NoDerivatives and closed items are reference only.
//! 4. Every adopted lesson carries `adopted`: the source, the item's licence and the deed it names, its authors, the
//!    attribution text and the original address. The gate `content.source-licensed` checks all of it on every scan; the
//!    importer runs the same checks before it writes anything.
//!
//! Adapters read what a publisher serves: an IM lesson page (HTML) and StoryWeaver's story JSON (the story record and
//! the reader's pages). Fetching is left to the person running the import: the tool never goes to the network.

use crate::canon::{item_id, lesson_id};
use crate::import::Records;
use crate::text::tokens;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

fn st<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn strs(v: &Value, k: &str) -> Vec<String> {
    v.get(k).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect()
}

// ---------- licences ----------

/// What the CC BY core may do with a licence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// CC BY, CC0, public domain: imported with attribution
    Adopt,
    /// BY-SA and other copyleft: kept out of the core
    ShareAlike,
    /// NonCommercial, NoDerivatives, all rights reserved, unknown: codes and links only
    Reference,
}

#[derive(Debug, Clone, Default)]
pub struct Licences {
    pub adoptable: BTreeSet<String>,
    pub share_alike: BTreeSet<String>,
    /// SPDX id -> the deed its `licenceUrl` names
    pub deeds: BTreeMap<String, String>,
}

impl Licences {
    /// The `text` section of gates/data/licences.json.
    pub fn from_json(v: &Value) -> Licences {
        let t = &v["text"];
        let set = |k: &str| strs(t, k).into_iter().collect::<BTreeSet<_>>();
        let deeds = t["deeds"].as_object().into_iter().flatten().filter_map(|(k, u)| u.as_str().map(|u| (k.clone(), u.to_string()))).collect();
        Licences { adoptable: set("adoptable"), share_alike: set("shareAlike"), deeds }
    }
    pub fn load(data: &Path) -> Result<Licences> {
        let p = data.join("licences.json");
        let v: Value = serde_json::from_str(&fs::read_to_string(&p).with_context(|| format!("gate data {}", p.display()))?).with_context(|| format!("gate data {}", p.display()))?;
        Ok(Licences::from_json(&v))
    }
    pub fn class(&self, spdx: &str) -> Class {
        if self.adoptable.contains(spdx) {
            Class::Adopt
        } else if self.share_alike.contains(spdx) {
            Class::ShareAlike
        } else {
            Class::Reference
        }
    }
    /// Does `url` name the deed of `spdx`? Scheme, `www.`, a trailing slash, `legalcode` and a `deed.<lang>` page are
    /// ignored; the licence and its version are not.
    pub fn deed_ok(&self, spdx: &str, url: &str) -> bool {
        let norm = |u: &str| {
            let u = u.trim().to_lowercase();
            let u = u.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.").to_string();
            let mut parts: Vec<&str> = u.split('/').filter(|p| !p.is_empty()).collect();
            while parts.last().is_some_and(|p| p.starts_with("legalcode") || p.starts_with("deed")) {
                parts.pop();
            }
            parts.join("/")
        };
        self.deeds.get(spdx).is_some_and(|d| norm(d) == norm(url))
    }
}

/// A licence as a publisher writes it ("CC BY 4.0", "Creative Commons Attribution 4.0", a deed's address, "Public
/// Domain") as an SPDX id. `None` when it cannot be recognised: an unknown licence is never adopted.
pub fn normalise_licence(raw: &str) -> Option<String> {
    let low = raw.to_lowercase();
    if low.contains("publicdomain/zero") || tokens(&low).iter().any(|t| t == "cc0") {
        return Some("CC0-1.0".to_string());
    }
    if low.contains("publicdomain/mark") || low.contains("public domain") {
        return Some("CC-PDM-1.0".to_string());
    }
    if let Some(i) = low.find("creativecommons.org/licenses/") {
        let rest: Vec<&str> = low[i + "creativecommons.org/licenses/".len()..].split('/').collect();
        let code = rest.first().copied().unwrap_or("");
        let version = rest.get(1).copied().unwrap_or("");
        let igo = rest.get(2).is_some_and(|p| *p == "igo");
        if code.starts_with("by") && !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return Some(format!("CC-{}-{version}{}", code.to_uppercase(), if igo { "-IGO" } else { "" }));
        }
        return None;
    }
    let toks = tokens(&low);
    let has = |w: &str| toks.iter().any(|t| t == w);
    let attribution = has("attribution") || toks.windows(2).any(|w| w[0] == "cc" && w[1] == "by");
    if !attribution {
        return None;
    }
    let nc = has("nc") || has("noncommercial") || low.contains("non-commercial") || low.contains("non commercial");
    let nd = has("nd") || toks.iter().any(|t| t.starts_with("noderiv")) || low.contains("no derivative");
    let sa = has("sa") || has("sharealike") || low.contains("share-alike") || low.contains("share alike");
    let version = toks.windows(2).find(|w| w[0].len() == 1 && w[0].chars().all(|c| c.is_ascii_digit()) && w[1].len() == 1 && w[1].chars().all(|c| c.is_ascii_digit())).map(|w| format!("{}.{}", w[0], w[1]))?;
    let mut id = "CC-BY".to_string();
    if nc {
        id.push_str("-NC");
    }
    if nd {
        id.push_str("-ND");
    } else if sa {
        id.push_str("-SA");
    }
    Some(format!("{id}-{version}{}", if has("igo") { "-IGO" } else { "" }))
}

/// "CC-BY-4.0" -> "CC BY 4.0", the way a credit line names it.
pub fn short_name(spdx: &str) -> String {
    match spdx {
        "CC0-1.0" => "CC0 1.0".to_string(),
        "CC-PDM-1.0" => "Public Domain".to_string(),
        s => {
            let parts: Vec<&str> = s.split('-').collect();
            match parts.iter().position(|p| p.chars().next().is_some_and(|c| c.is_ascii_digit())) {
                Some(i) if i >= 2 => format!("CC {} {}", parts[1..i].join("-"), parts[i..].join(" ")),
                _ => s.to_string(),
            }
        }
    }
}

fn host(url: &str) -> String {
    url.trim_start_matches("https://").trim_start_matches("http://").split(['/', '?', '#']).next().unwrap_or("").trim_start_matches("www.").to_lowercase()
}

// ---------- the checks (shared by the gate and the importer) ----------

/// Problems with a source record: its class must follow from the licences it publishes under.
pub fn source_problems(s: &Value, lic: &Licences) -> Vec<String> {
    let mut out = Vec::new();
    let classes: Vec<Class> = strs(s, "licences").iter().map(|l| lic.class(l)).collect();
    let (any_adopt, any_sa) = (classes.contains(&Class::Adopt), classes.contains(&Class::ShareAlike));
    match st(s, "class") {
        "adopt" if !any_adopt => out.push("class `adopt`, but none of its licences is CC BY, CC0 or public domain".to_string()),
        "sa-layer" if !any_sa || any_adopt => out.push("class `sa-layer` is for a source whose open licences are ShareAlike only".to_string()),
        "reference-only" if any_adopt || any_sa => out.push("class `reference-only`, but it publishes under an open licence: say `adopt` or `sa-layer`, and let each item's licence decide".to_string()),
        _ => {}
    }
    if st(s, "class") == "adopt" && st(s, "importer").is_empty() && s.get("importer").is_some() {
        out.push("`importer` names no adapter".to_string());
    }
    out
}

/// Problems with a lesson's `adopted` block. `credit` is the lesson's own `attribution`, which must name the source's
/// first author: it is the line an app shows beside the lesson.
pub fn adopted_problems(a: &Value, credit: &str, sources: &BTreeMap<String, Value>, lic: &Licences) -> Vec<String> {
    let mut out = Vec::new();
    let id = st(a, "source");
    let Some(src) = sources.get(id) else {
        return vec![format!("the source {id} is not registered under sources/")];
    };
    match st(src, "class") {
        "reference-only" => out.push(format!("{id} is reference only (NonCommercial, NoDerivatives or closed): codes and links, never content")),
        "sa-layer" => out.push(format!("{id} is a share-alike source: its material is kept out of the CC BY core")),
        _ => {}
    }
    let licence = st(a, "licence");
    match lic.class(licence) {
        Class::Adopt => {}
        Class::ShareAlike => out.push(format!("the item is {licence}: ShareAlike material is kept out of the CC BY core")),
        Class::Reference => out.push(format!("the item is {licence}: only CC BY, CC0 and public-domain items are adopted; NonCommercial, NoDerivatives and closed items are reference only")),
    }
    if !strs(src, "licences").iter().any(|l| l == licence) {
        out.push(format!("{id} does not publish under {licence} (its record lists {}): read the item's own licence again", strs(src, "licences").join(", ")));
    }
    match a.get("licenceUrl").and_then(Value::as_str) {
        None => out.push("no `licenceUrl`: the credit links the licence's deed".to_string()),
        Some(u) if !lic.deed_ok(licence, u) => out.push(format!("`licenceUrl` {u} is not the deed of {licence}")),
        _ => {}
    }
    if st(a, "attribution").trim().is_empty() {
        out.push("no `attribution`: adopted material carries the credit its licence asks for".to_string());
    }
    let authors = strs(a, "authors");
    if authors.iter().all(|x| x.trim().is_empty()) {
        out.push("no `authors`".to_string());
    }
    let url = st(a, "sourceUrl");
    if url.is_empty() {
        out.push("no `sourceUrl`: the credit links the original".to_string());
    } else {
        let mut hosts = strs(src, "hosts");
        hosts.push(host(st(src, "url")));
        let h = host(url);
        if !hosts.iter().any(|x| h == *x || h.ends_with(&format!(".{x}"))) {
            out.push(format!("`sourceUrl` is on {h}, which is not a host of {id} ({})", hosts.join(", ")));
        }
    }
    if let Some(first) = authors.first() {
        if !credit.contains(first.as_str()) {
            out.push(format!("the lesson's `attribution` does not name {first}: it is the credit an app shows"));
        }
    }
    out
}

// ---------- intake: what an adapter reads from one item ----------

/// One item as an adapter read it from the publisher.
#[derive(Debug, Clone, Default)]
pub struct Intake {
    pub source: String,
    pub item: String,
    pub url: String,
    pub title: String,
    /// the licence as the item states it, before it is recognised
    pub licence_stated: String,
    pub licence: Option<String>,
    pub authors: Vec<String>,
    pub attribution: String,
    /// the language the item is written in, as a locale id (when the item says)
    pub locale: Option<String>,
    /// IM: the student-facing prompts in page order; StoryWeaver: the text of each story page
    pub texts: Vec<String>,
    /// the original publisher, when a distributor carries the item (a GDL book from StoryWeaver, Book Dash, ...)
    pub publisher: String,
}

fn entity(s: &str) -> String {
    let mut out = s.to_string();
    for (from, to) in [("&nbsp;", " "), ("&amp;", "&"), ("&quot;", "\""), ("&#39;", "'"), ("&apos;", "'"), ("&rsquo;", "\u{2019}"), ("&lsquo;", "\u{2018}"), ("&ldquo;", "\u{201c}"), ("&rdquo;", "\u{201d}"), ("&ndash;", "\u{2013}"), ("&mdash;", "\u{2014}"), ("&reg;", "\u{ae}"), ("&copy;", "\u{a9}"), ("&lt;", "<"), ("&gt;", ">"), ("&iquest;", "\u{bf}"), ("&iexcl;", "\u{a1}"), ("&aacute;", "\u{e1}"), ("&eacute;", "\u{e9}"), ("&iacute;", "\u{ed}"), ("&oacute;", "\u{f3}"), ("&uacute;", "\u{fa}"), ("&ntilde;", "\u{f1}"), ("&#8217;", "\u{2019}"), ("&#8220;", "\u{201c}"), ("&#8221;", "\u{201d}")] {
        out = out.replace(from, to);
    }
    out
}

/// The text lines of an HTML page: tags and scripts removed, entities decoded, white space folded, empty lines dropped.
pub fn html_lines(html: &str) -> Vec<String> {
    let mut text = String::new();
    let mut rest = html;
    // drop script and style bodies first
    loop {
        let low = rest.to_lowercase();
        let next = ["<script", "<style"].iter().filter_map(|t| low.find(t).map(|i| (i, *t))).min();
        match next {
            Some((i, t)) => {
                text.push_str(&rest[..i]);
                let close = if t == "<script" { "</script>" } else { "</style>" };
                match low[i..].find(close) {
                    Some(j) => rest = &rest[i + j + close.len()..],
                    None => break,
                }
            }
            None => {
                text.push_str(rest);
                break;
            }
        }
    }
    let mut out = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push('\n');
            }
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    entity(&out).lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty()).collect()
}

/// An IM K-12 Math lesson page. The licence is the page's own notice ("Licensed under the Creative Commons Attribution
/// 4.0 license"); a page without one is refused, and a NonCommercial notice (as on IM v.360 material) is recognised as
/// such and refused later.
pub fn from_im_page(html: &str, url: &str) -> Result<Intake> {
    let lines = html_lines(html);
    let title = lines.iter().position(|l| l.starts_with("Lesson ") && l["Lesson ".len()..].chars().all(|c| c.is_ascii_digit())).and_then(|i| lines.get(i + 1)).cloned().ok_or_else(|| anyhow!("no lesson title (a line `Lesson <n>` and the title after it)"))?;
    let at = lines.iter().position(|l| l.contains("Licensed under")).ok_or_else(|| anyhow!("the page states no licence: an item without its own licence is never adopted"))?;
    let notice: String = lines[at..(at + 3).min(lines.len())].join(" ");
    let stated = notice[notice.find("Licensed under").unwrap_or(0)..].split(" license").next().unwrap_or("").trim().to_string();
    let texts = lines
        .iter()
        .filter(|l| l.starts_with('\u{201c}'))
        .map(|l| l.trim_end_matches("//").trim().trim_start_matches('\u{201c}').trim_end_matches('\u{201d}').trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Ok(Intake {
        source: "illustrative-math".to_string(),
        item: url.trim_start_matches("https://").split_once('/').map(|x| x.1).unwrap_or(url).trim_end_matches("/lesson.html").to_string(),
        url: url.to_string(),
        title,
        licence: normalise_licence(&stated),
        licence_stated: stated,
        authors: vec!["Illustrative Mathematics".to_string()],
        attribution: String::new(),
        locale: None,
        texts,
        publisher: "Illustrative Mathematics".to_string(),
    })
}

/// A StoryWeaver story: its record (`GET /api/v1/stories/<slug>`) and its pages as the reader serves them
/// (`GET /api/v1/stories/<slug>/read`, which needs a signed-in reader). The licence is the story's own: its
/// `licenseType` field when present, else the licence line on its attribution page.
pub fn from_storyweaver(meta: &Value, read: &Value) -> Result<Intake> {
    let m = meta.get("data").unwrap_or(meta);
    let r = read.get("data").unwrap_or(read);
    let names = |v: &Value| v.as_array().into_iter().flatten().filter_map(|a| a["name"].as_str()).map(str::to_string).collect::<Vec<_>>();
    let pages = r["pages"].as_array().ok_or_else(|| anyhow!("the reader JSON has no `pages`"))?;
    let page_text = |p: &Value| html_lines(st(p, "html")).join(" ");
    let texts: Vec<String> = pages.iter().filter(|p| st(p, "pageType") == "StoryPage").map(page_text).filter(|t| !t.is_empty()).collect();
    if texts.is_empty() {
        bail!("the story has no text pages");
    }
    // the item's own licence: a field, or the attribution page's licence line
    let mut stated = [st(m, "licenseType"), st(r, "licenseType"), st(m, "license_type")].into_iter().find(|s| !s.is_empty()).unwrap_or("").to_string();
    if stated.is_empty() {
        for p in pages.iter().filter(|p| st(p, "pageType") != "StoryPage") {
            let t = page_text(p);
            let low = t.to_lowercase();
            if let Some(i) = low.find("under a ").or_else(|| low.find("released under ")) {
                let tail = &t[i..];
                let end = tail.to_lowercase().find("licen").unwrap_or(tail.len());
                stated = tail[..end].replace("Released under", "").replace("released under", "").replace("under a", "").trim().to_string();
                break;
            }
        }
    }
    if stated.is_empty() {
        bail!("the story states no licence: an item without its own licence is never adopted");
    }
    let authors = names(&m["authors"]);
    let illustrators = names(&m["illustrators"]);
    let publisher = st(&m["publisher"], "name").to_string();
    let licence = normalise_licence(&stated);
    let attribution = format!(
        "'{}' written by {}{}, published by {} under a {} license on StoryWeaver.{}",
        st(m, "name"),
        authors.join(", "),
        if illustrators.is_empty() { String::new() } else { format!(", illustrated by {}", illustrators.join(", ")) },
        if publisher.is_empty() { "StoryWeaver" } else { &publisher },
        licence.as_deref().map(short_name).unwrap_or_else(|| stated.clone()),
        if st(m, "copyrightNotice").is_empty() { String::new() } else { format!(" {}", st(m, "copyrightNotice")) },
    );
    let locale = match st(m, "language") {
        "English" => Some("en"),
        "Hindi" => Some("hi"),
        "Spanish" | "Español" => Some("es"),
        "Bahasa Indonesia" | "Indonesian" => Some("id"),
        "Arabic" => Some("ar"),
        "Chinese (Simplified)" => Some("zh-Hans"),
        "Chinese (Traditional)" => Some("zh-Hant"),
        _ => None,
    }
    .map(str::to_string);
    let slug = st(m, "slug").to_string();
    Ok(Intake {
        source: "storyweaver".to_string(),
        url: format!("https://storyweaver.org.in/en/stories/{slug}"),
        item: slug,
        title: st(m, "name").to_string(),
        licence,
        licence_stated: stated,
        authors,
        attribution,
        locale,
        texts,
        publisher,
    })
}

/// The text of an XHTML page: the head dropped, then as [`html_lines`].
fn page_text(xhtml: &str) -> String {
    let low = xhtml.to_lowercase();
    let body = match (low.find("<head"), low.find("</head>")) {
        (Some(a), Some(b)) if a < b => format!("{}{}", &xhtml[..a], &xhtml[b + "</head>".len()..]),
        _ => xhtml.to_string(),
    };
    html_lines(&body).join(" ")
}

/// Every licence a text names ("Released under CC BY 4.0 license", "Creative Commons: Attribution 3.0", "licensed under
/// a Creative Commons Attribution 4.0 International License", "la licencia CC BY 4.0"), recognised.
pub fn licences_named(text: &str) -> Vec<String> {
    let low = text.to_lowercase();
    let mut out = Vec::new();
    for start in ["cc by", "cc-by", "creative commons attribution", "creative commons: attribution"] {
        let mut from = 0;
        while let Some(i) = low[from..].find(start) {
            let at = from + i;
            // up to the first version number ("4.0")
            let tail: Vec<char> = low[at..].chars().take(90).collect();
            if let Some(j) = (0..tail.len().saturating_sub(2)).find(|&j| tail[j].is_ascii_digit() && tail[j + 1] == '.' && tail[j + 2].is_ascii_digit()) {
                let phrase: String = tail[..j + 3].iter().collect();
                if let Some(l) = normalise_licence(&phrase) {
                    out.push(l);
                }
            }
            from = at + start.len();
        }
    }
    // Chinese: 知识共享：署名 3.0 (Creative Commons: Attribution 3.0), with 非商业 (NC), 禁止演绎 (ND), 相同方式共享 (SA)
    let mut from = 0;
    while let Some(i) = text[from..].find("\u{77e5}\u{8bc6}\u{5171}\u{4eab}") {
        let at = from + i;
        let tail: String = text[at..].chars().take(30).collect();
        if tail.contains("\u{7f72}\u{540d}") {
            let c: Vec<char> = tail.chars().collect();
            if let Some(j) = (0..c.len().saturating_sub(2)).find(|&j| c[j].is_ascii_digit() && c[j + 1] == '.' && c[j + 2].is_ascii_digit()) {
                let head: String = c[..j].iter().collect();
                let mut id = "CC-BY".to_string();
                if head.contains("\u{975e}\u{5546}\u{4e1a}") {
                    id.push_str("-NC");
                }
                if head.contains("\u{7981}\u{6b62}\u{6f14}\u{7ece}") {
                    id.push_str("-ND");
                } else if head.contains("\u{76f8}\u{540c}\u{65b9}\u{5f0f}\u{5171}\u{4eab}") {
                    id.push_str("-SA");
                }
                out.push(format!("{id}-{}.{}", c[j], c[j + 2]));
            }
        }
        from = at + "\u{77e5}\u{8bc6}\u{5171}\u{4eab}".len();
    }
    out.sort();
    out.dedup();
    out
}

/// A credit line without web addresses (the original's address travels in `sourceUrl`).
fn without_addresses(s: &str) -> String {
    s.split_whitespace().filter(|w| !(w.contains("://") || w.starts_with("www.") || w.contains(".org") || w.contains(".com"))).collect::<Vec<_>>().join(" ")
}

/// The family of a licence: `CC-BY-4.0` -> `CC-BY`, `CC-BY-NC-SA-4.0` -> `CC-BY-NC-SA`.
fn family(spdx: &str) -> String {
    spdx.split('-').take_while(|p| !p.chars().next().is_some_and(|c| c.is_ascii_digit())).collect::<Vec<_>>().join("-")
}

/// A Global Digital Library book: its record from the GDL content API (`/wp-json/wp/v2/book/<id>`, public) and its
/// EPUB, unpacked (the package file, and its pages in spine order). The licence is the item's own twice over: the
/// record's `license` term (`class_list`: `license-cc-by-4-0`) and every licence the book's own credits page names.
/// When the book names a licence, that is the item's licence; two that disagree, or one of another family than the
/// record's (CC BY against CC BY-NC), and the book is refused.
pub fn from_gdl(record: &Value, opf: &str, pages: &[String]) -> Result<Intake> {
    let field = strs(record, "class_list").iter().find_map(|c| c.strip_prefix("license-").map(str::to_string)).unwrap_or_default();
    let tag = |t: &str| -> Vec<String> {
        let open = format!("<dc:{t}");
        opf.split(open.as_str())
            .skip(1)
            .filter_map(|r| r.split_once('>').map(|x| x.1))
            .map(|body| entity(body.split('<').next().unwrap_or("").trim()))
            .filter(|v| !v.is_empty())
            .collect()
    };
    let title_full = tag("title").first().cloned().unwrap_or_else(|| entity(st(&record["title"], "rendered")));
    let title = title_full.lines().next().unwrap_or("").trim().to_string();
    let lang = tag("language").first().cloned().unwrap_or_default().to_lowercase();
    let locale = match lang.split(['-', '_']).next().unwrap_or("") {
        "zh" => Some("zh-Hans"),
        "en" => Some("en"),
        "es" => Some("es"),
        "hi" => Some("hi"),
        "id" | "in" => Some("id"),
        "ar" => Some("ar"),
        _ => None,
    }
    .map(str::to_string);
    let authors = tag("creator");
    let others = tag("contributor");
    let publisher = st(&record["customMeta"], "publisher").to_string();
    // story pages: after the title page, up to the credits; empty pages and the table of contents are skipped
    let credits = |t: &str| {
        let l = t.to_lowercase();
        // the credits page: a licence, the copyright sign, or "free to download" (in Chinese, 自由下载)
        l.contains("creative commons") || l.contains("released under") || l.contains("free to download") || l.contains("storyweaver") || t.contains('\u{a9}') || t.contains("\u{81ea}\u{7531}\u{4e0b}\u{8f7d}")
    };
    let mut texts = Vec::new();
    let mut back = String::new();
    for t in pages {
        if !back.is_empty() || credits(t) {
            back.push_str(t);
            back.push(' ');
            continue;
        }
        let l = t.to_lowercase();
        let title_page = ["author:", "autor:", "illustrator:", "ilustrador:"].iter().any(|k| l.contains(k));
        if t.is_empty() || l.starts_with("chapter 1 chapter 2") || title_page {
            continue;
        }
        texts.push(t.clone());
    }
    if texts.is_empty() {
        bail!("the book has no story pages");
    }
    let named = licences_named(&back);
    let low_back = back.to_lowercase();
    if named.is_empty() && (low_back.contains("creative commons") || back.contains("\u{77e5}\u{8bc6}\u{5171}\u{4eab}") || low_back.contains("licen")) {
        bail!("the book's credits name a licence that could not be read: refused rather than trust the record alone");
    }
    let from_field = normalise_licence(&field);
    let licence = match named.as_slice() {
        [] => from_field.clone(),
        [one] => Some(one.clone()),
        many => bail!("the book names more than one licence ({}): refused until each part is read", many.join(", ")),
    };
    if let (Some(l), Some(f)) = (&licence, &from_field) {
        if family(l) != family(f) {
            bail!("the book says {l} but its GDL record says {f}: refused until they agree");
        }
    }
    let stated = if named.is_empty() { field.clone() } else { named.join(", ") };
    let story_credit = ["Story Attribution:", "Atribuci\u{f3}n de la historia:"].iter().find_map(|k| back.find(k).map(|i| back[i..].to_string())).map(|t| {
        let end = ["Other Credits", "Otros cr\u{e9}ditos", "Illustration Attributions", "Atribuciones de la ilustraci\u{f3}n", "Disclaimer"].iter().filter_map(|k| t.find(k)).min().unwrap_or(t.len());
        without_addresses(&t[..end])
    });
    let lic_name = licence.as_deref().map(short_name).unwrap_or_else(|| stated.clone());
    let mut attribution = format!(
        "'{title}' by {}{}, {}, licensed under {lic_name}; via the Global Digital Library.",
        authors.join(", "),
        if others.is_empty() { String::new() } else { format!(" (with {})", others.join(", ")) },
        if publisher.is_empty() { "Global Digital Library" } else { &publisher },
    );
    if let Some(c) = story_credit {
        attribution = format!("{attribution} {c}");
    }
    Ok(Intake {
        source: "global-digital-library".to_string(),
        item: record["id"].as_u64().map(|n| n.to_string()).unwrap_or_default(),
        url: st(record, "link").to_string(),
        title,
        licence,
        licence_stated: stated,
        authors,
        attribution: attribution.chars().take(600).collect(),
        locale,
        texts,
        publisher,
    })
}


// ---------- the plan: which items, mapped onto which objectives ----------

/// Practice items made by a generator beside the adopted text (our own wording; the key is built with the question).
fn practice(spec: &Value, locale: &str, skill: &str, slug: &str) -> Result<Vec<Value>> {
    let kind = st(spec, "kind");
    let max = spec["max"].as_i64().unwrap_or(10);
    let n = spec["n"].as_u64().unwrap_or(6) as usize;
    let es = locale == "es";
    let mut questions: Vec<(String, i64)> = Vec::new();
    let mut a = 2;
    while questions.len() < n && a < 200 {
        let x = (a * 7) % (max - 1) + 2; // 2..=max
        let y = (a * 3) % (x - 1).max(1) + 1; // 1..x-1
        a += 1;
        let (text, answer) = match kind {
            "count" => {
                let stars = "\u{2b50}".repeat(x as usize);
                (if es { format!("\u{bf}Cu\u{e1}ntas estrellas hay? {stars}") } else { format!("How many stars? {stars}") }, x)
            }
            "add" if x <= max => (if es { format!("Tienes {}. Te dan {} m\u{e1}s. \u{bf}Cu\u{e1}ntos tienes ahora?", x - y, y) } else { format!("You have {}. You get {} more. How many now?", x - y, y) }, x),
            "subtract" if x - y != y => (if es { format!("Tienes {x}. Regalas {y}. \u{bf}Cu\u{e1}ntos quedan?") } else { format!("You have {x}. You give away {y}. How many are left?") }, x - y),
            "add" | "subtract" => continue,
            other => bail!("practice kind {other} is not count, add or subtract"),
        };
        if questions.iter().any(|q| q.0 == text) || (kind == "add" && (x - y == x || y == x)) {
            continue;
        }
        questions.push((text, answer));
    }
    let hint = if es { "Usa tus dedos para ayudarte." } else { "Use your fingers to help." };
    let mut out = Vec::new();
    for (text, answer) in questions {
        let wrong = [answer + 1, if answer > 1 { answer - 1 } else { answer + 2 }];
        let mut opts = [answer, wrong[0], wrong[1]];
        opts.sort();
        let options: Vec<Value> = opts.iter().enumerate().map(|(i, v)| json!({"id": format!("o{i}"), "text": v.to_string()})).collect();
        let right = opts.iter().position(|v| *v == answer).unwrap_or(0);
        let mut it = json!({
            "schema": "item/1", "id": "", "kind": "tap_choice", "skills": [skill], "band": "easy", "role": "practice",
            "family": format!("adopt:{slug}:{kind}"),
            "view": {"prompt": {"text": text}, "options": options},
            "key": {"graded": true, "answer": {"option": format!("o{right}")}, "hints": [hint], "verification": {"method": "constructed"}},
            "status": "draft",
        });
        it["id"] = json!(item_id(&it)?);
        out.push(it);
    }
    Ok(out)
}

/// The story as one `story_choice` item: one node per page, a "next" choice between them.
fn story_item(title: &str, pages: &[String], next: &str, skill: &str, slug: &str) -> Result<Value> {
    if pages.len() > 40 {
        bail!("a story of {} pages is longer than an item holds (40 nodes)", pages.len());
    }
    let nodes: Vec<Value> = pages
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let last = i + 1 == pages.len();
            let choices = if last { json!([]) } else { json!([{"id": "c0", "label": next, "to": format!("n{}", i + 1)}]) };
            json!({"id": format!("n{i}"), "text": t, "end": last, "choices": choices})
        })
        .collect();
    let mut it = json!({
        "schema": "item/1", "id": "", "kind": "story_choice", "skills": [skill], "band": "easy", "role": "guided",
        "family": format!("adopt:{slug}:story"),
        "view": {"prompt": {"text": title}, "start": "n0", "nodes": nodes},
        "key": {"graded": false, "hints": []},
        "status": "draft",
    });
    it["id"] = json!(item_id(&it)?);
    Ok(it)
}

/// The credit line an app shows beside the lesson (at most 200 characters).
fn credit_line(title: &str, authors: &[String], licence: &str) -> String {
    let lic = short_name(licence);
    for who in [authors.join(", "), format!("{} and others", authors.first().cloned().unwrap_or_default())] {
        let line = format!("Adapted from \"{title}\" by {who}, {lic}; openlessons contributors");
        if line.chars().count() <= 200 {
            return line;
        }
    }
    format!("Adapted from work by {}, {lic}; openlessons contributors", authors.first().cloned().unwrap_or_default()).chars().take(200).collect()
}

/// An item that was not adopted, and why.
#[derive(Debug, Clone)]
pub struct Refusal {
    pub entry: String,
    pub reasons: Vec<String>,
}

pub struct Adoption {
    pub records: Records,
    pub refused: Vec<Refusal>,
}

/// Run an adoption plan (`adopt-plan/1`): read each entry's input with the source's adapter, check the item's own
/// licence and credit, and build `draft` records. Nothing is written here.
pub fn run_plan(plan: &Value, base: &Path, sources: &BTreeMap<String, Value>, lic: &Licences, maker: &str, created: &str) -> Result<Adoption> {
    if st(plan, "schema") != "adopt-plan/1" {
        bail!("an adoption plan has `schema`: \"adopt-plan/1\"");
    }
    let source_id = st(plan, "source");
    let src = sources.get(source_id).ok_or_else(|| anyhow!("the source {source_id} is not registered under sources/"))?;
    let retrieved = if st(plan, "retrieved").is_empty() { created } else { st(plan, "retrieved") };
    let mut records = Records::default();
    let mut refused = Vec::new();
    for e in plan["entries"].as_array().into_iter().flatten() {
        let name = if st(e, "slug").is_empty() { st(e, "input").to_string() } else { st(e, "slug").to_string() };
        let read = |k: &str| -> Result<String> {
            let p = base.join(st(e, k));
            fs::read_to_string(&p).with_context(|| format!("{}", p.display()))
        };
        let intake = match st(src, "importer") {
            "illustrative-math" => read("input").and_then(|h| from_im_page(&h, st(e, "url"))),
            "storyweaver" => read("input").and_then(|m| {
                let r = read("read")?;
                from_storyweaver(&serde_json::from_str(&m).context("the story record")?, &serde_json::from_str(&r).context("the reader pages")?)
            }),
            "global-digital-library" => read("input").and_then(|r| {
                let record: Value = serde_json::from_str(&r).context("the GDL book record")?;
                let dir = base.join(st(e, "epub"));
                let opf_path = find_opf(&dir).ok_or_else(|| anyhow!("no package file (*.opf) under {}", dir.display()))?;
                let opf = fs::read_to_string(&opf_path)?;
                let at = opf_path.parent().map(Path::to_path_buf).unwrap_or_default();
                let pages: Vec<String> = spine(&opf).iter().filter_map(|h| fs::read_to_string(at.join(h)).ok()).map(|x| page_text(&x)).collect();
                from_gdl(&record, &opf, &pages)
            }),
            other => Err(anyhow!("no adapter `{other}` for {source_id}")),
        };
        let mut intake = match intake {
            Ok(i) => i,
            Err(err) => {
                refused.push(Refusal { entry: name, reasons: vec![format!("{err:#}")] });
                continue;
            }
        };
        if !st(e, "url").is_empty() {
            intake.url = st(e, "url").to_string();
        }
        // a distributor's copy is no way round its publisher's class: an item first published by a source we hold as
        // reference only or share-alike is refused, whatever the distributor's field says
        if let Some(held) = sources.values().find(|s| !intake.publisher.is_empty() && st(s, "class") != "adopt" && (st(s, "publisher") == intake.publisher || st(s, "title") == intake.publisher)) {
            refused.push(Refusal { entry: name, reasons: vec![format!("first published by {}, which is {} ({}): refused", intake.publisher, st(held, "class"), st(held, "id"))] });
            continue;
        }
        let Some(licence) = intake.licence.clone() else {
            refused.push(Refusal { entry: name, reasons: vec![format!("the licence '{}' is not one we recognise: nothing is adopted under an unknown licence", intake.licence_stated)] });
            continue;
        };
        // the credit the publisher asks for, from the source's record; no template, no credit, and the checks refuse it
        let template = st(&src["terms"], "attribution");
        if intake.attribution.is_empty() && !template.is_empty() {
            intake.attribution = format!("{} Lesson: \"{}\".", template.replace("{licence}", &short_name(&licence)), intake.title);
        }
        let locale = st(e, "locale").to_string();
        let mut reasons = Vec::new();
        if let Some(l) = &intake.locale {
            if *l != locale {
                reasons.push(format!("the item is written in {l}, the plan says {locale}"));
            }
        }
        let title = if st(e, "title").is_empty() { intake.title.clone() } else { st(e, "title").to_string() };
        let skills = strs(e, "skills");
        let skill = skills.first().cloned().ok_or_else(|| anyhow!("{name}: the entry names no skill"))?;
        let adopted = json!({
            "source": source_id, "item": intake.item, "title": intake.title, "sourceUrl": intake.url,
            "licence": licence, "licenceUrl": lic.deeds.get(&licence).cloned().unwrap_or_default(),
            "authors": intake.authors, "attribution": intake.attribution,
            "changes": if st(e, "changes").is_empty() { "shortened and adapted for a learner's screen; practice items added by openlessons" } else { st(e, "changes") },
            "retrieved": retrieved,
        });
        let credit = credit_line(&intake.title, &intake.authors, &licence);
        reasons.extend(adopted_problems(&adopted, &credit, sources, lic));
        if !reasons.is_empty() {
            refused.push(Refusal { entry: name, reasons });
            continue;
        }
        let (subject, slug) = (st(e, "subject"), st(e, "slug"));
        let (teach, items) = match st(src, "importer") {
            "storyweaver" | "global-digital-library" => {
                let intro = st(e, "intro");
                if intro.is_empty() {
                    bail!("{name}: a story entry gives the `intro` the lesson opens with, in its own language");
                }
                (intro.to_string(), vec![story_item(&title, &intake.texts, if st(e, "next").is_empty() { "Next" } else { st(e, "next") }, &skill, slug)?])
            }
            _ => {
                // a prompt said aloud in class often has no full stop: each becomes its own sentence
                let picked: Vec<String> = e["teach"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .filter_map(|i| intake.texts.get(i as usize).cloned())
                    .map(|t| if t.ends_with(['.', '?', '!', '\u{2026}']) { t } else { format!("{t}.") })
                    .collect();
                if picked.is_empty() {
                    refused.push(Refusal { entry: name, reasons: vec!["`teach` picks none of the page's student-facing prompts".to_string()] });
                    continue;
                }
                let items = match e.get("practice") {
                    Some(p) => practice(p, &locale, &skill, slug)?,
                    None => vec![],
                };
                (picked.join(" "), items)
            }
        };
        let id = lesson_id(&locale, subject, slug);
        let provenance = json!({"origin": "generator", "maker": maker, "tool": "openlessons adopt", "source": format!("{source_id}:{}", intake.item).chars().take(200).collect::<String>(), "created": created});
        let lesson = json!({
            "schema": "lesson/1", "id": id, "slug": slug, "locale": locale, "subject": subject, "title": title,
            "ages": e["ages"], "skills": skills, "teach": {"text": teach},
            "status": "draft", "licence": "CC-BY-4.0", "attribution": credit, "adopted": adopted, "provenance": provenance,
        });
        let items: Vec<Value> = items
            .into_iter()
            .map(|mut i| {
                i["provenance"] = json!({"origin": "generator", "maker": maker, "tool": "openlessons adopt", "created": created});
                i
            })
            .collect();
        records.lessons.push((lesson, items));
    }
    Ok(Adoption { records, refused })
}

/// The package file of an unpacked EPUB.
fn find_opf(dir: &Path) -> Option<std::path::PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let mut entries: Vec<_> = fs::read_dir(&d).ok()?.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "opf") {
                return Some(p);
            }
        }
    }
    None
}

/// The pages of an EPUB package in reading order: each spine `itemref` resolved through the manifest.
pub fn spine(opf: &str) -> Vec<String> {
    let attr = |tag: &str, name: &str| -> Option<String> {
        let k = format!("{name}=\"");
        tag.find(&k).map(|i| &tag[i + k.len()..]).and_then(|r| r.split('"').next()).map(str::to_string)
    };
    let tags = |t: &str| -> Vec<String> { opf.split(&format!("<{t}")).skip(1).filter_map(|r| r.split('>').next()).map(str::to_string).collect() };
    let manifest: BTreeMap<String, String> = tags("item ").iter().filter_map(|t| Some((attr(t, "id")?, attr(t, "href")?))).collect();
    tags("itemref").iter().filter_map(|t| attr(t, "idref")).filter_map(|id| manifest.get(&id).cloned()).collect()
}

/// Every record under `<root>/sources/`.
pub fn load_sources(root: &Path) -> Result<BTreeMap<String, Value>> {
    let mut out = BTreeMap::new();
    let dir = root.join("sources");
    let mut files: Vec<_> = fs::read_dir(&dir).map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
    files.sort();
    for p in files.into_iter().filter(|p| p.extension().is_some_and(|x| x == "json")) {
        let v: Value = serde_json::from_str(&fs::read_to_string(&p)?).with_context(|| format!("{}", p.display()))?;
        out.insert(st(&v, "id").to_string(), v);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn licences_are_recognised_as_publishers_write_them() {
        let n = |s: &str| normalise_licence(s);
        assert_eq!(n("Creative Commons Attribution 4.0").as_deref(), Some("CC-BY-4.0"));
        assert_eq!(n("CC BY 4.0").as_deref(), Some("CC-BY-4.0"));
        assert_eq!(n("CC-BY-4.0").as_deref(), Some("CC-BY-4.0"));
        assert_eq!(n("Attribution-NonCommercial 4.0 International").as_deref(), Some("CC-BY-NC-4.0"));
        assert_eq!(n("CC BY-NC-SA 4.0").as_deref(), Some("CC-BY-NC-SA-4.0"));
        assert_eq!(n("CC BY-ND 3.0").as_deref(), Some("CC-BY-ND-3.0"));
        assert_eq!(n("https://creativecommons.org/licenses/by-sa/3.0/igo/").as_deref(), Some("CC-BY-SA-3.0-IGO"));
        assert_eq!(n("Public Domain").as_deref(), Some("CC-PDM-1.0"));
        assert_eq!(n("CC0").as_deref(), Some("CC0-1.0"));
        assert_eq!(n("All rights reserved"), None);
        assert_eq!(n("Creative Commons"), None);
        assert_eq!(short_name("CC-BY-NC-SA-4.0"), "CC BY-NC-SA 4.0");
        assert_eq!(short_name("CC-BY-4.0"), "CC BY 4.0");
    }

    #[test]
    fn a_deed_is_matched_by_licence_and_version() {
        let lic = Licences::from_json(&json!({"text": {"adoptable": ["CC-BY-4.0"], "deeds": {"CC-BY-4.0": "https://creativecommons.org/licenses/by/4.0/"}}}));
        assert!(lic.deed_ok("CC-BY-4.0", "http://creativecommons.org/licenses/by/4.0/legalcode"));
        assert!(lic.deed_ok("CC-BY-4.0", "https://creativecommons.org/licenses/by/4.0/deed.es"));
        assert!(!lic.deed_ok("CC-BY-4.0", "https://creativecommons.org/licenses/by/3.0/"));
        assert!(!lic.deed_ok("CC-BY-4.0", "https://creativecommons.org/licenses/by-nc/4.0/"));
    }

    const PAGE: &str = "<html><script>var x = '<b>';</script><body><div>Lesson 1</div><h1>Count and Add</h1>\
        <p>&ldquo;How many objects are in your collection?&rdquo;</p><p>&ldquo;&iquest;Cu&aacute;ntos hay?&rdquo; //</p>\
        <footer>&copy; 2021 Illustrative Mathematics&reg;. Licensed under the <a>Creative Commons Attribution 4.0</a> license.</footer></body></html>";

    #[test]
    fn an_im_page_gives_its_title_prompts_and_its_own_licence() {
        let i = from_im_page(PAGE, "https://im.kendallhunt.com/k5/teachers/grade-1/unit-1/lesson-1/lesson.html").unwrap();
        assert_eq!(i.title, "Count and Add");
        assert_eq!(i.licence.as_deref(), Some("CC-BY-4.0"));
        assert_eq!(i.texts[0], "How many objects are in your collection?");
        assert_eq!(i.texts.len(), 2);
        assert_eq!(i.item, "k5/teachers/grade-1/unit-1/lesson-1");
        // the v.360 edition is NonCommercial: recognised as such, so the checks refuse it
        let nc = from_im_page(&PAGE.replace("Creative Commons Attribution 4.0", "Creative Commons Attribution-NonCommercial 4.0"), "https://x/y").unwrap();
        assert_eq!(nc.licence.as_deref(), Some("CC-BY-NC-4.0"));
        // a page with no notice is refused outright
        assert!(from_im_page(&PAGE.replace("Licensed under the", "See"), "https://x/y").is_err());
    }

    fn story(licence_line: &str) -> (Value, Value) {
        let meta = json!({"ok": true, "data": {"name": "Hen Counts", "slug": "1-hen-counts", "language": "English", "copyrightNotice": "",
            "authors": [{"name": "Ada Writer"}], "illustrators": [{"name": "Bo Painter"}], "publisher": {"name": "Pratham Books"}}});
        let read = json!({"ok": true, "data": {"pages": [
            {"pageType": "FrontCoverPage", "html": "<p>Hen Counts</p>"},
            {"pageType": "StoryPage", "html": "<p>Hen has three eggs.</p>"},
            {"pageType": "StoryPage", "html": "<p>One, two, three!</p>"},
            {"pageType": "BackInnerCoverPage", "html": format!("<p>This story is written by Ada Writer. Released under {licence_line} license.</p>")}]}});
        (meta, read)
    }

    #[test]
    fn a_storyweaver_story_gives_its_pages_and_its_own_licence() {
        let (m, r) = story("CC BY 4.0");
        let i = from_storyweaver(&m, &r).unwrap();
        assert_eq!(i.texts, ["Hen has three eggs.", "One, two, three!"]);
        assert_eq!(i.licence.as_deref(), Some("CC-BY-4.0"));
        assert_eq!(i.locale.as_deref(), Some("en"));
        assert!(i.attribution.contains("Ada Writer") && i.attribution.contains("CC BY 4.0"));
        let (m, r) = story("CC BY-NC 4.0");
        assert_eq!(from_storyweaver(&m, &r).unwrap().licence.as_deref(), Some("CC-BY-NC-4.0"));
        let (m, mut r) = story("x");
        r["data"]["pages"][3]["html"] = json!("<p>The end.</p>");
        assert!(from_storyweaver(&m, &r).is_err(), "a story that states no licence is refused");
    }

    fn sources() -> BTreeMap<String, Value> {
        let mut s = BTreeMap::new();
        s.insert("storyweaver".to_string(), json!({"id": "storyweaver", "class": "adopt", "url": "https://storyweaver.org.in/", "licences": ["CC-BY-4.0"], "importer": "storyweaver", "terms": {}}));
        s.insert("illustrative-math".to_string(), json!({"id": "illustrative-math", "class": "adopt", "url": "https://illustrativemathematics.org/", "hosts": ["im.kendallhunt.com"],
            "licences": ["CC-BY-4.0", "CC-BY-NC-4.0"], "importer": "illustrative-math",
            "terms": {"attribution": "Based on IM\u{ae} K\u{2013}12 Math authored by Illustrative Mathematics and licensed under {licence}."}}));
        s
    }
    fn lic() -> Licences {
        Licences::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../gates/data")).unwrap()
    }

    #[test]
    fn the_importer_refuses_nc_sa_and_unattributed_items_and_writes_drafts_otherwise() {
        let dir = std::env::temp_dir().join(format!("openlessons-adopt-unit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("by.html"), PAGE).unwrap();
        fs::write(dir.join("nc.html"), PAGE.replace("Attribution 4.0", "Attribution-NonCommercial 4.0")).unwrap();
        fs::write(dir.join("sa.html"), PAGE.replace("Attribution 4.0", "Attribution-ShareAlike 4.0")).unwrap();
        let entry = |input: &str, slug: &str| json!({"input": input, "url": "https://im.kendallhunt.com/k5/teachers/grade-1/unit-1/lesson-1/lesson.html",
            "locale": "en", "subject": "maths", "slug": slug, "skills": ["maths.calc.add-within-10"], "ages": {"min": 6, "max": 7}, "teach": [0],
            "practice": {"kind": "add", "max": 10, "n": 4}});
        let plan = json!({"schema": "adopt-plan/1", "source": "illustrative-math", "retrieved": "2026-11-23",
            "entries": [entry("by.html", "im-ok"), entry("nc.html", "im-nc"), entry("sa.html", "im-sa")]});
        let a = run_plan(&plan, &dir, &sources(), &lic(), "openlessons-adopt", "2026-11-23").unwrap();
        let refused: Vec<&str> = a.refused.iter().map(|r| r.entry.as_str()).collect();
        assert_eq!(refused, ["im-nc", "im-sa"], "{:?}", a.refused);
        assert!(a.refused[0].reasons.iter().any(|r| r.contains("NonCommercial")));
        assert!(a.refused[1].reasons.iter().any(|r| r.contains("ShareAlike")));
        let (l, items) = &a.records.lessons[0];
        assert_eq!(l["status"], "draft");
        assert_eq!(l["adopted"]["licence"], "CC-BY-4.0");
        assert_eq!(l["adopted"]["licenceUrl"], "https://creativecommons.org/licenses/by/4.0/");
        assert!(st(&l["adopted"], "attribution").contains("Illustrative Mathematics"));
        assert_eq!(items.len(), 4);
        assert!(items.iter().all(|i| i["status"] == "draft"));

        // a source whose credit template is missing gives an item with no attribution: refused
        let mut s = sources();
        s.get_mut("illustrative-math").unwrap()["terms"] = json!({});
        let a = run_plan(&json!({"schema": "adopt-plan/1", "source": "illustrative-math", "entries": [entry("by.html", "im-ok")]}), &dir, &s, &lic(), "openlessons-adopt", "2026-11-23").unwrap();
        assert_eq!(a.records.lessons.len(), 0);
        assert!(a.refused[0].reasons.iter().any(|r| r.contains("attribution")), "{:?}", a.refused);
        fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod gdl_tests {
    use super::*;

    fn record(licence: &str, publisher: &str) -> Value {
        json!({"id": 1, "link": "https://content.digitallibrary.io/book/hens/", "class_list": ["book", format!("license-{licence}")],
            "customMeta": {"publisher": publisher}, "title": {"rendered": "Hens"}})
    }
    const OPF: &str = "<package><metadata><dc:title>Hens</dc:title><dc:language>en</dc:language><dc:creator id=\"c1\">Ada Writer</dc:creator></metadata>\
        <manifest><item id=\"p1\" href=\"chapter-1.xhtml\"/><item id=\"p2\" href=\"chapter-2.xhtml\"/></manifest><spine><itemref idref=\"p1\"/><itemref idref=\"p2\"/></spine></package>";
    fn pages(credit: &str) -> Vec<String> {
        vec!["Hens Author: Ada Writer".into(), "A hen has three eggs.".into(), "One, two, three!".into(), credit.into()]
    }

    #[test]
    fn the_spine_gives_pages_in_reading_order() {
        assert_eq!(spine(OPF), ["chapter-1.xhtml", "chapter-2.xhtml"]);
    }

    #[test]
    fn the_book_s_own_statement_is_its_licence() {
        let i = from_gdl(&record("cc-by-4-0", "African Storybook"), OPF, &pages("\u{a9} African Storybook Initiative 2014 Creative Commons: Attribution 3.0")).unwrap();
        assert_eq!(i.licence.as_deref(), Some("CC-BY-3.0"));
        assert_eq!(i.texts, ["A hen has three eggs.", "One, two, three!"]);
        assert_eq!(i.locale.as_deref(), Some("en"));
        assert!(i.attribution.contains("Ada Writer") && i.attribution.contains("African Storybook"));
        // in Chinese: 知识共享：署名3.0
        let zh = from_gdl(&record("cc-by-4-0", "African Storybook"), OPF, &pages("\u{a9} 2014 \u{77e5}\u{8bc6}\u{5171}\u{4eab}\u{ff1a}\u{7f72}\u{540d}3.0")).unwrap();
        assert_eq!(zh.licence.as_deref(), Some("CC-BY-3.0"));
    }

    #[test]
    fn a_book_whose_statement_and_record_disagree_or_cannot_be_read_is_refused() {
        // the record says CC BY; the book says NonCommercial
        assert!(from_gdl(&record("cc-by-4-0", "x"), OPF, &pages("Released under CC BY-NC 4.0 license.")).is_err());
        // two licences named
        assert!(from_gdl(&record("cc-by-4-0", "x"), OPF, &pages("Released under CC BY 4.0 license. Also under CC BY 3.0 license.")).is_err());
        // a Creative Commons notice nobody can read
        assert!(from_gdl(&record("cc-by-4-0", "x"), OPF, &pages("Creative Commons, some rights.")).is_err());
        // an NC record with no statement stays NC, and the checks refuse it
        let nc = from_gdl(&record("cc-by-nc-sa-4-0", "x"), OPF, &pages("The end \u{a9} 2020")).unwrap();
        assert_eq!(nc.licence.as_deref(), Some("CC-BY-NC-SA-4.0"));
    }

    #[test]
    fn a_distributor_s_copy_of_a_reference_only_publisher_is_refused() {
        let dir = std::env::temp_dir().join(format!("openlessons-gdl-unit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("book/content")).unwrap();
        fs::write(dir.join("book/content/book.opf"), OPF).unwrap();
        for (n, t) in [(1, "<p>A hen has three eggs.</p>"), (2, "<p>Released under CC BY 4.0 license.</p>")] {
            fs::write(dir.join(format!("book/content/chapter-{n}.xhtml")), t).unwrap();
        }
        fs::write(dir.join("rec.json"), record("cc-by-4-0", "The Asia Foundation").to_string()).unwrap();
        let mut sources = BTreeMap::new();
        sources.insert("global-digital-library".to_string(), json!({"id": "global-digital-library", "class": "adopt", "url": "https://digitallibrary.io/", "licences": ["CC-BY-4.0"], "importer": "global-digital-library", "terms": {}}));
        sources.insert("lets-read".to_string(), json!({"id": "lets-read", "class": "reference-only", "publisher": "The Asia Foundation", "licences": ["CC-BY-NC-4.0"]}));
        let lic = Licences::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../gates/data")).unwrap();
        let plan = json!({"schema": "adopt-plan/1", "source": "global-digital-library", "entries": [{"input": "rec.json", "epub": "book", "locale": "en", "subject": "literacy",
            "slug": "hens", "skills": ["literacy.reading.retell-a-story"], "ages": {"min": 5, "max": 7}, "intro": "Read the story.", "next": "Next"}]});
        let a = run_plan(&plan, &dir, &sources, &lic, "openlessons-adopt", "2026-11-23").unwrap();
        assert_eq!(a.records.lessons.len(), 0);
        assert!(a.refused[0].reasons[0].contains("reference-only"), "{:?}", a.refused);
        // the same book from an adoptable publisher is adopted
        fs::write(dir.join("rec.json"), record("cc-by-4-0", "Book Dash").to_string()).unwrap();
        let a = run_plan(&plan, &dir, &sources, &lic, "openlessons-adopt", "2026-11-23").unwrap();
        assert_eq!(a.records.lessons.len(), 1, "{:?}", a.refused);
        fs::remove_dir_all(&dir).unwrap();
    }
}
