//! Code-behind of the chart data dialog (`chart_dialog.kbview`): the web `ChartEditor` popover's title and data.

use kubuno_desktop::prelude::*;
use kubuno_office_slides_core::chart::{chart_data_to_text, data_json, parse_chart_data, stored_series};
use kubuno_office_slides_core::model::Element;
use serde_json::Value as Json;

#[kubuno_desktop::view("chart_dialog.kbview")]
pub struct ChartDialog {
    #[bind]
    chart_title: String,
    #[bind]
    data_text: String,
}

/// What the dialog changes: the title, then the categories and the series (stored JSON).
pub struct ChartEdit {
    pub title: String,
    pub categories: Json,
    pub series: Json,
}

impl ChartDialog {
    pub fn new(el: &Element) -> Self {
        let categories: Vec<String> = el.get("categories").and_then(Json::as_array).map(|a| a.iter().map(|c| c.as_str().map(str::to_string).unwrap_or_else(|| c.to_string())).collect()).unwrap_or_default();
        let text = chart_data_to_text(&categories, &stored_series(el));
        let mut dialog = Self { chart_title: el.s("title").unwrap_or("").to_string(), data_text: text, ..Self::default() };
        dialog.initialize_component();
        dialog
    }

    /// Asks; `None` when cancelled.
    pub fn ask(owner: &dyn AsForm, el: &Element) -> Option<ChartEdit> {
        let mut d = Self::new(el);
        if d.show_dialog(owner) != DialogResult::Ok {
            return None;
        }
        let (cats, series) = parse_chart_data(&d.data_text);
        let (categories, series) = data_json(&cats, &series);
        Some(ChartEdit { title: d.chart_title.clone(), categories, series })
    }

    fn ok_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }

    fn cancel_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }
}
