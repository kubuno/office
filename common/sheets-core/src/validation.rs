//! Data validation rules: a port of the web editor's `data-validation.ts`.
//!
//! The types deserialise from the stored JSON (`sheet.data.validations`) with the web's field names
//! (`kind`, `values`, `source`, `strict`, `dropdown`, `op`, `v1`, `v2`, `on`, `off`, `crit`, `reject`,
//! `help`, `ranges`, `rule`) and serialise back to the same shape. Rules the web would not understand
//! (unknown `kind` / `op`) parse as `Unknown` variants that never validate, as the web's `switch` falls
//! through to `undefined`.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::js;

/// `NumOp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NumOp {
    Between,
    NotBetween,
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
    Ne,
    /// An operator the web does not know (its `cmp` returns `undefined`).
    #[serde(other)]
    Unknown,
}

/// A JavaScript `String(x)` of a stored JSON value (list entries may be numbers in old workbooks).
fn js_string(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.as_f64().map(js::to_string).unwrap_or_else(|| n.to_string()),
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(|x| if x.is_null() { String::new() } else { js_string(x) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

fn string_list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let v = Vec::<Value>::deserialize(d)?;
    Ok(v.iter().map(js_string).collect())
}

/// `DVCriterion`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DVCriterion {
    List {
        #[serde(deserialize_with = "string_list")]
        values: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dropdown: Option<bool>,
    },
    ListRange {
        source: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dropdown: Option<bool>,
    },
    Number {
        op: NumOp,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        v1: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        v2: Option<f64>,
    },
    TextLen {
        op: NumOp,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        v1: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        v2: Option<f64>,
    },
    Checkbox {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        off: Option<String>,
    },
    /// A kind the web does not know: it never validates.
    #[serde(other)]
    Unknown,
}

/// `DataValidation`: a criterion plus how to react.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataValidation {
    pub crit: DVCriterion,
    /// `true` = refuse the invalid entry; `false`/absent = a warning only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

/// `DVBlock`: a rule attached to ranges.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DVBlock {
    pub ranges: Vec<String>,
    pub rule: DataValidation,
}

/// `isCheckbox`.
pub fn is_checkbox(r: Option<&DataValidation>) -> bool {
    matches!(r, Some(DataValidation { crit: DVCriterion::Checkbox { .. }, .. }))
}

/// `hasDropdown`: a list rule whose `dropdown` is not explicitly `false`.
pub fn has_dropdown(r: Option<&DataValidation>) -> bool {
    match r.map(|r| &r.crit) {
        Some(DVCriterion::List { dropdown, .. }) | Some(DVCriterion::ListRange { dropdown, .. }) => *dropdown != Some(false),
        _ => false,
    }
}

/// `cmp(n, op, a, b)`; a missing bound is `undefined` in the web (`NaN` here: every comparison is false and
/// `!==` is true, the same).
fn cmp(n: f64, op: NumOp, a: f64, b: f64) -> bool {
    match op {
        NumOp::Between => n >= a && n <= b,
        NumOp::NotBetween => n < a || n > b,
        NumOp::Gt => n > a,
        NumOp::Lt => n < a,
        NumOp::Ge => n >= a,
        NumOp::Le => n <= a,
        NumOp::Eq => n == a,
        NumOp::Ne => n != a,
        NumOp::Unknown => false,
    }
}

/// `validateValue`: validates a raw entry against a criterion. `allowed` = the values of a list resolved
/// from its source range (the caller supplies them). An empty cell is tolerated.
pub fn validate_value(crit: &DVCriterion, raw: &str, allowed: Option<&[String]>) -> bool {
    let s = raw;
    match crit {
        DVCriterion::Checkbox { .. } => true, // handled by the widget (TRUE/FALSE)
        DVCriterion::List { values, .. } => list_ok(s, values.iter().map(|x| js::trim(x))),
        DVCriterion::ListRange { .. } => list_ok(s, allowed.unwrap_or(&[]).iter().map(|x| js::trim(x))),
        DVCriterion::Number { op, v1, v2 } => {
            if js::trim(s).is_empty() {
                return true;
            }
            let n = js::number_or_nan(s);
            if n.is_nan() {
                return false;
            }
            let a = v1.unwrap_or(f64::NAN);
            cmp(n, *op, a, v2.unwrap_or(a))
        }
        DVCriterion::TextLen { op, v1, v2 } => {
            let a = v1.unwrap_or(f64::NAN);
            cmp(js::len16(s) as f64, *op, a, v2.unwrap_or(a))
        }
        DVCriterion::Unknown => false,
    }
}

fn list_ok<'a>(s: &str, mut set: impl Iterator<Item = &'a str>) -> bool {
    let t = js::trim(s);
    if t.is_empty() {
        return true;
    }
    set.any(|x| x == t)
}

fn op_label(op: NumOp) -> &'static str {
    match op {
        NumOp::Between => "entre",
        NumOp::NotBetween => "hors de",
        NumOp::Gt => ">",
        NumOp::Lt => "<",
        NumOp::Ge => "≥",
        NumOp::Le => "≤",
        NumOp::Eq => "=",
        NumOp::Ne => "≠",
        NumOp::Unknown => "undefined",
    }
}

fn opt_num(v: Option<f64>) -> String {
    v.map(js::to_string).unwrap_or_else(|| "undefined".to_string())
}

/// `criterionLabel`: the short French label of a criterion (rule list and help).
pub fn criterion_label(crit: &DVCriterion) -> String {
    match crit {
        DVCriterion::List { values, .. } => {
            let head: Vec<&str> = values.iter().take(4).map(String::as_str).collect();
            format!("Liste : {}{}", head.join(", "), if values.len() > 4 { "…" } else { "" })
        }
        DVCriterion::ListRange { source, .. } => format!("Liste depuis {source}"),
        DVCriterion::Checkbox { .. } => "Case à cocher".to_string(),
        DVCriterion::Number { op, v1, v2 } => format!("Nombre {} {}{}", op_label(*op), opt_num(*v1), range_tail(*op, *v2)),
        DVCriterion::TextLen { op, v1, v2 } => format!("Longueur {} {}{}", op_label(*op), opt_num(*v1), range_tail(*op, *v2)),
        DVCriterion::Unknown => "undefined".to_string(),
    }
}

fn range_tail(op: NumOp, v2: Option<f64>) -> String {
    if matches!(op, NumOp::Between | NumOp::NotBetween) {
        format!("–{}", opt_num(v2))
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crit(j: &str) -> DVCriterion {
        serde_json::from_str(j).unwrap()
    }

    #[test]
    fn parses_stored_json() {
        let b: DVBlock = serde_json::from_str(r#"{"ranges":["A1:A5"],"rule":{"crit":{"kind":"list","values":["a",2],"dropdown":false},"reject":true,"help":"h"}}"#).unwrap();
        assert_eq!(b.rule.reject, Some(true));
        assert!(!has_dropdown(Some(&b.rule)));
        assert!(!is_checkbox(Some(&b.rule)));
        assert!(matches!(&b.rule.crit, DVCriterion::List { values, .. } if values == &["a", "2"]));
        assert!(matches!(crit(r#"{"kind":"weird"}"#), DVCriterion::Unknown));
    }

    #[test]
    fn validates() {
        let n = crit(r#"{"kind":"number","op":"between","v1":1,"v2":5}"#);
        assert!(validate_value(&n, "3", None));
        assert!(!validate_value(&n, "6", None));
        assert!(validate_value(&n, "  ", None));
        assert!(!validate_value(&n, "abc", None));
        let l = crit(r#"{"kind":"listRange","source":"A1:A3"}"#);
        assert!(validate_value(&l, " x ", Some(&["x".to_string()])));
        assert!(!validate_value(&l, "y", None));
        let t = crit(r#"{"kind":"textLen","op":"le","v1":3}"#);
        assert!(validate_value(&t, "abc", None));
        assert!(!validate_value(&t, "abcd", None));
        assert!(!validate_value(&t, "😀😀", None) || validate_value(&t, "😀", None));
    }

    #[test]
    fn labels() {
        assert_eq!(criterion_label(&crit(r#"{"kind":"list","values":["a","b","c","d","e"]}"#)), "Liste : a, b, c, d…");
        assert_eq!(criterion_label(&crit(r#"{"kind":"number","op":"between","v1":1}"#)), "Nombre entre 1–undefined");
        assert_eq!(criterion_label(&crit(r#"{"kind":"textLen","op":"ge","v1":2}"#)), "Longueur ≥ 2");
    }
}
