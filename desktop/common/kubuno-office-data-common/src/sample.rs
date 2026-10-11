//! A built-in sample report, so the app shows something without a server (the designer, `--sample`, and
//! the offline parity captures). The dataset rows are cached in memory and aggregated by
//! `kubuno_office_data_core::query`, exactly as the server's `execute_on_memory` path would.

use std::collections::HashMap;

use kubuno_office_data_core::model::{self, ReportDoc};
use serde_json::Value;

/// The id of the sample dataset the widgets bind to.
pub const DATASET_ID: &str = "00000000-0000-0000-0000-0000000000d1";

const REPORT_JSON: &str = include_str!("sample_report.json");
const ROWS_JSON: &str = include_str!("sample_rows.json");

/// The sample report document.
pub fn report() -> ReportDoc {
    model::parse(REPORT_JSON.as_bytes()).expect("the built-in sample report is valid JSON")
}

/// The cached rows of every sample dataset, keyed by dataset id (what a `.kbdst`'s `data_cache` holds).
pub fn dataset_rows() -> HashMap<String, Vec<Value>> {
    let rows: Vec<Value> = serde_json::from_str(ROWS_JSON).expect("the built-in sample rows are valid JSON");
    let mut map = HashMap::new();
    map.insert(DATASET_ID.to_string(), rows);
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_is_coherent() {
        let doc = report();
        assert!(!doc.pages.is_empty());
        assert!(!doc.widgets.is_empty());
        let rows = dataset_rows();
        assert!(rows.contains_key(DATASET_ID));
        assert!(!rows[DATASET_ID].is_empty());
    }
}
