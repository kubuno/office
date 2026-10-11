//! JSON ↔ Yjs values. The web writes elements and strokes as plain JSON
//! (`JSON.parse(JSON.stringify(el))`), which lib0 encodes as `Any` (integers as varints, other numbers as
//! floats). Reading back, an integral number becomes a JSON integer, as `JSON.stringify` would print it, so
//! an element read from the room compares equal to the same element read from the model.
//!
//! Shared verbatim with `kubuno-office-sheets-core::collab::json` in spirit (the mapping is generic); kept
//! per-crate to avoid coupling the two editors' cores.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Map, Number, Value as Json};
use yrs::{Any, Out};

/// The largest integer a double holds exactly (`Number.MAX_SAFE_INTEGER`).
const MAX_SAFE: f64 = 9_007_199_254_740_991.0;

/// A JSON value as Yjs stores it.
pub fn to_any(v: &Json) -> Any {
    match v {
        Json::Null => Any::Null,
        Json::Bool(b) => Any::Bool(*b),
        Json::Number(n) => Any::Number(n.as_f64().unwrap_or(0.0)),
        Json::String(s) => Any::String(Arc::from(s.as_str())),
        Json::Array(a) => Any::Array(a.iter().map(to_any).collect::<Vec<_>>().into()),
        Json::Object(o) => Any::Map(Arc::new(o.iter().map(|(k, v)| (k.clone(), to_any(v))).collect::<HashMap<_, _>>())),
    }
}

/// A number as `JSON.stringify` prints it (an integral double as an integer).
pub fn number(f: f64) -> Json {
    if f.is_finite() && f.fract() == 0.0 && f.abs() <= MAX_SAFE {
        Json::from(f as i64)
    } else {
        Number::from_f64(f).map(Json::Number).unwrap_or(Json::Null)
    }
}

/// A Yjs value as JSON (`undefined` → `None`: `JSON.stringify` drops it from objects).
pub fn from_any(a: &Any) -> Option<Json> {
    Some(match a {
        Any::Null => Json::Null,
        Any::Undefined => return None,
        Any::Bool(b) => Json::Bool(*b),
        Any::Number(f) => number(*f),
        Any::BigInt(i) => Json::from(*i),
        Any::String(s) => Json::String(s.to_string()),
        Any::Buffer(_) => Json::Null,
        Any::Array(a) => Json::Array(a.iter().map(|v| from_any(v).unwrap_or(Json::Null)).collect()),
        Any::Map(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let mut o = Map::new();
            for k in keys {
                if let Some(v) = m.get(k).and_then(from_any) {
                    o.insert(k.clone(), v);
                }
            }
            Json::Object(o)
        }
    })
}

/// A value read from a Y.Map / Y.Array (only plain values are expected; a nested shared type reads as
/// `None`).
pub fn from_out(o: &Out) -> Option<Json> {
    match o {
        Out::Any(a) => from_any(a),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_round_trips_through_any() {
        let v = json!({"id": "a1", "type": "shape", "x": 10, "y": 20.5, "adj": [0.25, null], "locked": true});
        assert_eq!(from_any(&to_any(&v)), Some(v));
    }

    #[test]
    fn integral_numbers_read_as_integers() {
        assert_eq!(number(3.0), json!(3));
        assert_eq!(number(-0.0), json!(0));
        assert_eq!(number(0.1), json!(0.1));
    }
}
