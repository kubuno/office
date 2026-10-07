//! An ordered JSON object whose untouched values keep their original bytes.
//!
//! `serde_json`'s own map is ordered by key unless its `preserve_order` feature is on, and that feature is
//! workspace-wide: switching it on here would change the JSON every other crate of the desktop workspace
//! writes. So the presentation keeps its objects in this type instead: the keys in their stored order, each
//! value parsed (to read it) AND kept as the bytes it arrived as (to write it back unchanged). A value set
//! by the editor is written by `serde_json`; one it never touched is written byte for byte.

use serde::de::{Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::Value;

/// One value: parsed, and its original text when untouched.
#[derive(Debug, Clone)]
pub struct Slot {
    pub v: Value,
    raw: Option<Box<RawValue>>,
}

impl Slot {
    /// A value set by the editor.
    pub fn value(v: Value) -> Slot {
        Slot { v, raw: None }
    }

    /// A value from JSON text, kept byte for byte (invalid JSON gives `null`).
    pub fn raw(text: &str) -> Slot {
        match RawValue::from_string(text.to_string()) {
            Ok(raw) => Slot { v: serde_json::from_str(raw.get()).unwrap_or(Value::Null), raw: Some(raw) },
            Err(_) => Slot::value(Value::Null),
        }
    }

    fn from_raw(raw: Box<RawValue>) -> Slot {
        Slot { v: serde_json::from_str(raw.get()).unwrap_or(Value::Null), raw: Some(raw) }
    }

    /// The JSON text this slot writes.
    pub fn text(&self) -> String {
        match &self.raw {
            Some(r) => r.get().to_string(),
            None => serde_json::to_string(&self.v).unwrap_or_else(|_| "null".into()),
        }
    }

    /// An ordered object parsed from this slot's text (its keys in their stored order).
    pub fn as_obj(&self) -> Option<Obj> {
        match &self.raw {
            Some(r) => serde_json::from_str::<Obj>(r.get()).ok(),
            None => match &self.v {
                Value::Object(m) => Some(Obj(m.iter().map(|(k, v)| (k.clone(), Slot::value(v.clone()))).collect())),
                _ => None,
            },
        }
    }
}

impl PartialEq for Slot {
    fn eq(&self, o: &Slot) -> bool {
        self.v == o.v
    }
}

/// An ordered JSON object (see the module doc).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Obj(pub Vec<(String, Slot)>);

impl Obj {
    pub fn new() -> Obj {
        Obj(Vec::new())
    }

    /// Parses JSON text (an object), keeping key order and value bytes.
    pub fn parse(text: &str) -> Result<Obj, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// From a parsed value (its keys in `serde_json`'s order).
    pub fn from_value(v: &Value) -> Option<Obj> {
        v.as_object().map(|m| Obj(m.iter().map(|(k, v)| (k.clone(), Slot::value(v.clone()))).collect()))
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, s)| &s.v)
    }

    pub fn slot(&self, key: &str) -> Option<&Slot> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, s)| s)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.0.iter().any(|(k, _)| k == key)
    }

    /// Sets a key in place (a new key goes last).
    pub fn set_slot(&mut self, key: &str, slot: Slot) {
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some((_, s)) => *s = slot,
            None => self.0.push((key.to_string(), slot)),
        }
    }

    pub fn set(&mut self, key: &str, v: Value) {
        self.set_slot(key, Slot::value(v));
    }

    pub fn remove(&mut self, key: &str) {
        self.0.retain(|(k, _)| k != key);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|(k, _)| k.as_str())
    }

    /// The object as JSON text, keys in order.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    /// The object as a `serde_json` value (for reading; key order is `serde_json`'s).
    pub fn to_value(&self) -> Value {
        Value::Object(self.0.iter().map(|(k, s)| (k.clone(), s.v.clone())).collect())
    }
}

impl Serialize for Obj {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(self.0.len()))?;
        for (k, slot) in &self.0 {
            match &slot.raw {
                Some(raw) => m.serialize_entry(k, raw)?,
                None => m.serialize_entry(k, &slot.v)?,
            }
        }
        m.end()
    }
}

impl<'de> Deserialize<'de> for Obj {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Obj, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Obj;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Obj, A::Error> {
                let mut out = Vec::new();
                while let Some((k, raw)) = map.next_entry::<String, Box<RawValue>>()? {
                    // A repeated key: the last one wins, like `JSON.parse`.
                    out.retain(|(ok, _): &(String, Slot)| *ok != k);
                    out.push((k, Slot::from_raw(raw)));
                }
                Ok(Obj(out))
            }
        }
        d.deserialize_map(V)
    }
}

/// A JSON array of ordered objects, each kept as its slot (`[ {...}, {...} ]`).
pub fn parse_array_of_obj(text: &str) -> Vec<Obj> {
    serde_json::from_str::<Vec<Box<RawValue>>>(text).map(|items| items.into_iter().filter_map(|r| Obj::parse(r.get()).ok()).collect()).unwrap_or_default()
}

/// A JSON string literal.
pub fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_and_bytes_survive() {
        let src = r#"{"z":1,"a":{"y":2,"b":[1.50,2]},"m":"x"}"#;
        let mut o = Obj::parse(src).expect("obj");
        assert_eq!(o.to_json(), src, "untouched: byte for byte (1.50 included)");
        assert_eq!(o.keys().collect::<Vec<_>>(), ["z", "a", "m"]);
        o.set("z", Value::from(3));
        o.set("new", Value::Bool(true));
        o.remove("m");
        assert_eq!(o.to_json(), r#"{"z":3,"a":{"y":2,"b":[1.50,2]},"new":true}"#);
        let nested = o.slot("a").and_then(Slot::as_obj).expect("nested");
        assert_eq!(nested.keys().collect::<Vec<_>>(), ["y", "b"]);
    }
}
