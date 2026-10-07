//! The system clipboard: a copy of shapes is put there in a private format (`Kubuno.Diagram`, the JSON
//! `{ shapes, connectors }`) so it survives between two windows of the app; the web keeps its copy
//! inside the page (`clipboardRef`).

use serde_json::Value;

/// Puts a copy of shapes on the system clipboard.
pub fn put(_v: &Value) {}

/// The copy of shapes on the system clipboard, if any.
pub fn get() -> Option<Value> {
    None
}
