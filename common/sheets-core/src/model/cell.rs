//! One cell as the web stores it (`CellData` in `api.ts`): `{ v?, f?, s?, c?, link? }`.
//!
//! Every field is kept as the JSON it was read from - absent stays absent, `null` stays `null`, a number
//! keeps its spelling - and keys the web does not know are carried in `rest`, so a cell the desktop does
//! not change is written back byte for byte. The typed views (`formula`, `raw_scalar`, `style`) read
//! through that JSON the way the TypeScript reads `CellData`.

use serde_json::{Map, Value as Json};

use crate::formula::value::Scalar;
use crate::js;
use crate::model::style::Style;

/// A cell (`CellData`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cell {
    /// `v`: the raw value (`string | number | boolean | null`).
    pub v: Option<Json>,
    /// `f`: the formula (`"=A1+1"`), or `null`.
    pub f: Option<Json>,
    /// `s`: the style object.
    pub s: Option<Json>,
    /// `c`: the note (comment).
    pub c: Option<Json>,
    /// `link`: the hyperlink target.
    pub link: Option<Json>,
    /// Keys the web does not model, kept as read.
    pub rest: Map<String, Json>,
}

impl Cell {
    /// Reads a cell object (anything else is an empty cell).
    pub fn from_json(v: &Json) -> Cell {
        let Some(o) = v.as_object() else { return Cell::default() };
        let mut cell = Cell::default();
        for (k, val) in o {
            match k.as_str() {
                "v" => cell.v = Some(val.clone()),
                "f" => cell.f = Some(val.clone()),
                "s" => cell.s = Some(val.clone()),
                "c" => cell.c = Some(val.clone()),
                "link" => cell.link = Some(val.clone()),
                _ => {
                    cell.rest.insert(k.clone(), val.clone());
                }
            }
        }
        cell
    }

    /// The cell as JSON (keys in serde_json's sorted order, as the server writes them).
    pub fn to_json(&self) -> Json {
        let mut o = self.rest.clone();
        if let Some(v) = &self.v {
            o.insert("v".into(), v.clone());
        }
        if let Some(v) = &self.f {
            o.insert("f".into(), v.clone());
        }
        if let Some(v) = &self.s {
            o.insert("s".into(), v.clone());
        }
        if let Some(v) = &self.c {
            o.insert("c".into(), v.clone());
        }
        if let Some(v) = &self.link {
            o.insert("link".into(), v.clone());
        }
        Json::Object(o)
    }

    /// A cell holding a plain value.
    pub fn value(v: Json) -> Cell {
        Cell { v: Some(v), ..Cell::default() }
    }

    /// A cell holding a formula (`v` absent, as `updateCell` stores it).
    pub fn formula_cell(f: impl Into<String>) -> Cell {
        Cell { f: Some(Json::String(f.into())), ..Cell::default() }
    }

    /// `cell.f && cell.f.startsWith('=') ? cell.f : null` - the formula the engine evaluates.
    pub fn formula(&self) -> Option<&str> {
        match &self.f {
            Some(Json::String(s)) if s.starts_with('=') => Some(s),
            _ => None,
        }
    }

    /// `!!cell.f`: any non-empty formula text (truthy), evaluated or not.
    pub fn has_f(&self) -> bool {
        match &self.f {
            Some(Json::String(s)) => !s.is_empty(),
            Some(Json::Null) | None => false,
            Some(Json::Bool(b)) => *b,
            Some(Json::Number(n)) => n.as_f64() != Some(0.0),
            Some(_) => true,
        }
    }

    /// `cell.v == null`.
    pub fn v_is_nullish(&self) -> bool {
        matches!(self.v, None | Some(Json::Null))
    }

    /// `tcell.v != null && tcell.v !== ''`.
    pub fn has_value(&self) -> bool {
        match &self.v {
            None | Some(Json::Null) => false,
            Some(Json::String(s)) => !s.is_empty(),
            Some(_) => true,
        }
    }

    /// The raw value as the engine reads a non-formula cell (`cellValue`): `null` is `''`, a non-empty
    /// numeric string is that number, anything else as it is.
    pub fn raw_scalar(&self) -> Scalar {
        match &self.v {
            None | Some(Json::Null) => Scalar::empty(),
            Some(Json::String(s)) => {
                if !s.is_empty() {
                    if let Some(n) = js::number(s) {
                        return Scalar::Num(n);
                    }
                }
                Scalar::Str(s.clone())
            }
            Some(Json::Number(n)) => Scalar::Num(n.as_f64().unwrap_or(f64::NAN)),
            Some(Json::Bool(b)) => Scalar::Bool(*b),
            Some(other) => Scalar::Str(js_string_of(other)),
        }
    }

    /// `v` as the JavaScript value it is, without the numeric-string conversion (`cell.v`).
    pub fn v_scalar(&self) -> Option<Scalar> {
        match &self.v {
            None | Some(Json::Null) => None,
            Some(Json::String(s)) => Some(Scalar::Str(s.clone())),
            Some(Json::Number(n)) => Some(Scalar::Num(n.as_f64().unwrap_or(f64::NAN))),
            Some(Json::Bool(b)) => Some(Scalar::Bool(*b)),
            Some(other) => Some(Scalar::Str(js_string_of(other))),
        }
    }

    /// `cellDisplay`: `String(cell.v)`, `''` for null.
    pub fn display(&self) -> String {
        match &self.v {
            None | Some(Json::Null) => String::new(),
            Some(v) => js_string_of(v),
        }
    }

    /// The parsed style (`cell.s`), empty when absent.
    pub fn style(&self) -> Style {
        self.s.as_ref().map(Style::from_json).unwrap_or_default()
    }

    /// The note text.
    pub fn note(&self) -> Option<&str> {
        match &self.c {
            Some(Json::String(s)) if !s.is_empty() => Some(s),
            _ => None,
        }
    }

    /// The hyperlink target.
    pub fn link_target(&self) -> Option<&str> {
        match &self.link {
            Some(Json::String(s)) if !s.is_empty() => Some(s),
            _ => None,
        }
    }

    /// A cell with nothing in `v`, `f`, `s` and `c` is deleted by `writeCells`.
    pub fn is_void(&self) -> bool {
        self.v_is_nullish() && !self.has_f() && self.s.is_none() && self.c.as_ref().is_none_or(|c| c.is_null() || c.as_str() == Some(""))
    }
}

/// `String(v)` for a JSON value.
pub fn js_string_of(v: &Json) -> String {
    match v {
        Json::Null => "null".into(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => js::to_string(n.as_f64().unwrap_or(f64::NAN)),
        Json::String(s) => s.clone(),
        Json::Array(a) => a.iter().map(|x| if x.is_null() { String::new() } else { js_string_of(x) }).collect::<Vec<_>>().join(","),
        Json::Object(_) => "[object Object]".into(),
    }
}

/// A JavaScript number as the web's `JSON.stringify` writes it, read back by the server: an integral value
/// within the safe range becomes an integer, anything else a float.
pub fn json_number(x: f64) -> Json {
    if x.is_finite() && x == x.trunc() && x.abs() < 9_007_199_254_740_992.0 {
        if x == 0.0 {
            return Json::from(0);
        }
        return Json::from(x as i64);
    }
    serde_json::Number::from_f64(x).map(Json::Number).unwrap_or(Json::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_cell_round_trips_with_unknown_keys() {
        let j = json!({"v": 1.5, "f": null, "s": {"bold": true}, "zz": [1]});
        let c = Cell::from_json(&j);
        assert_eq!(c.to_json(), j);
        assert_eq!(c.raw_scalar(), Scalar::Num(1.5));
    }

    #[test]
    fn values_read_like_the_engine() {
        assert_eq!(Cell::value(json!(" 12 ")).raw_scalar(), Scalar::Num(12.0));
        assert_eq!(Cell::value(json!("")).raw_scalar(), Scalar::empty());
        assert_eq!(Cell::value(json!(null)).raw_scalar(), Scalar::empty());
        assert_eq!(Cell::value(json!(true)).raw_scalar(), Scalar::Bool(true));
        assert_eq!(Cell::value(json!(3)).display(), "3");
        assert!(Cell::formula_cell("=1").formula().is_some());
        assert!(Cell::formula_cell("1").formula().is_none());
    }

    #[test]
    fn numbers_are_written_like_javascript() {
        assert_eq!(json_number(5.0).to_string(), "5");
        assert_eq!(json_number(-0.0).to_string(), "0");
        assert_eq!(json_number(1.5).to_string(), "1.5");
    }
}
