//! Canonical JSON and the ids derived from it.
//!
//! The canonical form is: UTF-8, object keys sorted by code point, no insignificant whitespace, strings escaped as
//! serde_json escapes them, integers in decimal. A hashed body holds no floating-point number, so two implementations
//! cannot disagree on how one is printed.

use anyhow::{bail, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const ID_HEX: usize = 24;

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(64);
    for b in Sha256::digest(bytes) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

pub fn canonical(v: &Value) -> Result<String> {
    let mut out = String::new();
    write(v, &mut out)?;
    Ok(out)
}

fn write(v: &Value, out: &mut String) -> Result<()> {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if !(n.is_i64() || n.is_u64()) {
                bail!("a hashed body holds a non-integer number ({n})");
            }
            out.push_str(&n.to_string());
        }
        Value::String(s) => out.push_str(&serde_json::to_string(s)?),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(x, out)?;
            }
            out.push(']');
        }
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k)?);
                out.push(':');
                write(&o[*k], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// A lesson's id names its identity (locale, subject, slug), so adding items never moves the lesson.
pub fn lesson_id(locale: &str, subject: &str, slug: &str) -> String {
    let h = sha256_hex(format!("lesson/1\n{locale}\n{subject}\n{slug}").as_bytes());
    format!("les_{}", &h[..ID_HEX])
}

/// An item's id is the hash of what it asks and what it accepts: the same item has the same id everywhere.
pub fn item_id(item: &Value) -> Result<String> {
    let body = serde_json::json!({
        "kind": item.get("kind").cloned().unwrap_or(Value::Null),
        "view": item.get("view").cloned().unwrap_or(Value::Null),
        "key": item.get("key").cloned().unwrap_or(Value::Null),
    });
    let h = sha256_hex(format!("item/1\n{}", canonical(&body)?).as_bytes());
    Ok(format!("itm_{}", &h[..ID_HEX]))
}

/// An asset's id is the hash of its bytes.
pub fn asset_id(sha256: &str) -> String {
    format!("ast_{}", &sha256[..ID_HEX])
}

/// The two-character shard folder of an id (`les_ab12…` lives under `ab/`).
pub fn shard(id: &str) -> &str {
    id.get(4..6).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_are_sorted_and_order_does_not_matter() {
        let a: Value = serde_json::from_str(r#"{"b":1,"a":{"d":[1,2],"c":"x"}}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a":{"c":"x","d":[1,2]},"b":1}"#).unwrap();
        assert_eq!(canonical(&a).unwrap(), r#"{"a":{"c":"x","d":[1,2]},"b":1}"#);
        assert_eq!(canonical(&a).unwrap(), canonical(&b).unwrap());
    }

    #[test]
    fn a_float_is_refused() {
        assert!(canonical(&json!({"x": 0.25})).is_err());
    }

    #[test]
    fn item_id_ignores_metadata_and_follows_content() {
        let a = json!({"kind":"tap_choice","view":{"prompt":{"text":"2 + 2"}},"key":{"answer":{"option":"o4"}},"status":"draft"});
        let mut b = a.clone();
        b["status"] = json!("published");
        assert_eq!(item_id(&a).unwrap(), item_id(&b).unwrap());
        b["view"]["prompt"]["text"] = json!("2 + 3");
        assert_ne!(item_id(&a).unwrap(), item_id(&b).unwrap());
        assert!(item_id(&a).unwrap().starts_with("itm_"));
        assert_eq!(item_id(&a).unwrap().len(), 4 + ID_HEX);
    }

    #[test]
    fn lesson_id_is_stable() {
        assert_eq!(lesson_id("en", "maths", "count-to-20"), lesson_id("en", "maths", "count-to-20"));
        assert_ne!(lesson_id("en", "maths", "count-to-20"), lesson_id("id", "maths", "count-to-20"));
    }
}
