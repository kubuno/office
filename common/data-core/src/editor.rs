//! The report-editing façade a platform drives, ported from `DataReportEditor.tsx`. It holds the open
//! [`ReportDoc`], the current page and the selection, and applies [`EditOp`]s with an **inverse-operation**
//! undo/redo history (not snapshots) that coalesces a live drag into one entry — exactly as the web's
//! `useEditHistory` (`:56-59`, `:142-255`) does.
//!
//! Every op is also the unit the desktop maps to a REST mutation (`widgets.create/update/delete`,
//! `pages.create/update/delete`, `reports.update`): the Data app auto-saves one edit at a time, so there is
//! no file digest/PATCH path (see vskubuno `docs/DATA-DESKTOP.md` §3). This crate only mutates the in-memory
//! document and tracks history; issuing the calls is the platform's job.

use crate::model::{Report, ReportDoc, ReportPage, Widget};
use serde_json::Value;

/// A reversible editing operation.
#[derive(Debug, Clone, PartialEq)]
pub enum EditOp {
    /// Add a widget to its page.
    AddWidget(Widget),
    /// Remove a widget by id.
    DeleteWidget(String),
    /// Move a widget to an absolute position.
    MoveWidget { id: String, x: i64, y: i64 },
    /// Resize a widget.
    ResizeWidget { id: String, width: i64, height: i64 },
    /// Set a widget's z-index (bring-to-front / send-to-back resolve to this).
    SetZIndex { id: String, z_index: i64 },
    /// Replace a widget's whole `config` (format pane, wells, lock, palette…).
    SetConfig { id: String, config: Value },
    /// Change a widget's visual type.
    SetWidgetType { id: String, widget_type: String },
    /// Apply a report theme: the new `report.theme` plus the repainted config of each affected widget.
    ApplyTheme { theme: Value, widget_configs: Vec<(String, Value)> },
    /// Insert a page, optionally with the widgets to restore (used by undo of a delete).
    AddPage { page: ReportPage, widgets: Vec<Widget> },
    /// Delete a page and cascade its widgets.
    DeletePage(String),
    /// Replace a page's fields (rename, resize, recolour, reposition).
    SetPage(ReportPage),
}

/// Why an op could not apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    WidgetNotFound(String),
    PageNotFound(String),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::WidgetNotFound(id) => write!(f, "widget not found: {id}"),
            EditError::PageNotFound(id) => write!(f, "page not found: {id}"),
        }
    }
}

impl std::error::Error for EditError {}

/// A coalescing key: consecutive ops with the same key and no redo in between merge into one history entry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CoalesceKey(&'static str, String);

fn coalesce_key(op: &EditOp) -> Option<CoalesceKey> {
    match op {
        EditOp::MoveWidget { id, .. } => Some(CoalesceKey("move", id.clone())),
        EditOp::ResizeWidget { id, .. } => Some(CoalesceKey("resize", id.clone())),
        _ => None,
    }
}

/// The open report plus selection and history.
#[derive(Debug, Clone)]
pub struct Editor {
    pub doc: ReportDoc,
    pub current_page: Option<String>,
    pub selected_ids: Vec<String>,
    undo: Vec<(EditOp, Option<CoalesceKey>)>,
    redo: Vec<(EditOp, Option<CoalesceKey>)>,
}

impl Editor {
    /// Open a document; the current page is its first page.
    pub fn new(doc: ReportDoc) -> Self {
        let current_page = doc.pages.first().map(|p| p.id.clone());
        Editor { doc, current_page, selected_ids: Vec::new(), undo: Vec::new(), redo: Vec::new() }
    }

    /// Widgets of the current page, sorted by z-index (paint order), as the canvas shows them.
    pub fn current_widgets(&self) -> Vec<&Widget> {
        let page = self.current_page.as_deref();
        let mut ws: Vec<&Widget> = self
            .doc
            .widgets
            .iter()
            .filter(|w| w.page_id.as_deref() == page)
            .collect();
        ws.sort_by_key(|w| w.z_index);
        ws
    }

    fn widget_mut(&mut self, id: &str) -> Result<&mut Widget, EditError> {
        self.doc
            .widgets
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| EditError::WidgetNotFound(id.to_string()))
    }

    /// Borrow a widget by id.
    pub fn widget(&self, id: &str) -> Result<&Widget, EditError> {
        self.doc
            .widgets
            .iter()
            .find(|w| w.id == id)
            .ok_or_else(|| EditError::WidgetNotFound(id.to_string()))
    }

    /// Current z-index extent of the current page, for bring-to-front / send-to-back.
    pub fn z_range(&self) -> (i64, i64) {
        let zs: Vec<i64> = self.current_widgets().iter().map(|w| w.z_index).collect();
        (
            zs.iter().copied().min().unwrap_or(0),
            zs.iter().copied().max().unwrap_or(0),
        )
    }

    /// Apply an op WITHOUT touching history, returning the op that reverses it.
    fn apply(&mut self, op: EditOp) -> Result<EditOp, EditError> {
        match op {
            EditOp::AddWidget(w) => {
                let id = w.id.clone();
                self.doc.widgets.push(w);
                Ok(EditOp::DeleteWidget(id))
            }
            EditOp::DeleteWidget(id) => {
                let pos = self
                    .doc
                    .widgets
                    .iter()
                    .position(|w| w.id == id)
                    .ok_or_else(|| EditError::WidgetNotFound(id.clone()))?;
                let removed = self.doc.widgets.remove(pos);
                self.selected_ids.retain(|s| s != &id);
                Ok(EditOp::AddWidget(removed))
            }
            EditOp::MoveWidget { id, x, y } => {
                let w = self.widget_mut(&id)?;
                let inverse = EditOp::MoveWidget { id: id.clone(), x: w.x, y: w.y };
                w.x = x;
                w.y = y;
                Ok(inverse)
            }
            EditOp::ResizeWidget { id, width, height } => {
                let w = self.widget_mut(&id)?;
                let inverse = EditOp::ResizeWidget { id: id.clone(), width: w.width, height: w.height };
                w.width = width;
                w.height = height;
                Ok(inverse)
            }
            EditOp::SetZIndex { id, z_index } => {
                let w = self.widget_mut(&id)?;
                let inverse = EditOp::SetZIndex { id: id.clone(), z_index: w.z_index };
                w.z_index = z_index;
                Ok(inverse)
            }
            EditOp::SetConfig { id, config } => {
                let w = self.widget_mut(&id)?;
                let inverse = EditOp::SetConfig { id: id.clone(), config: w.config.clone() };
                w.config = config;
                Ok(inverse)
            }
            EditOp::SetWidgetType { id, widget_type } => {
                let w = self.widget_mut(&id)?;
                let inverse = EditOp::SetWidgetType { id: id.clone(), widget_type: w.widget_type.clone() };
                w.widget_type = widget_type;
                Ok(inverse)
            }
            EditOp::ApplyTheme { theme, widget_configs } => {
                let old_theme = std::mem::replace(&mut self.doc.report.theme, theme);
                let mut old_configs = Vec::with_capacity(widget_configs.len());
                for (id, config) in widget_configs {
                    let w = self.widget_mut(&id)?;
                    old_configs.push((id.clone(), w.config.clone()));
                    w.config = config;
                }
                Ok(EditOp::ApplyTheme { theme: old_theme, widget_configs: old_configs })
            }
            EditOp::AddPage { page, widgets } => {
                let id = page.id.clone();
                self.doc.pages.push(page);
                self.doc.widgets.extend(widgets);
                self.doc.report.page_count = self.doc.pages.len() as i64;
                Ok(EditOp::DeletePage(id))
            }
            EditOp::DeletePage(id) => {
                let pos = self
                    .doc
                    .pages
                    .iter()
                    .position(|p| p.id == id)
                    .ok_or_else(|| EditError::PageNotFound(id.clone()))?;
                let page = self.doc.pages.remove(pos);
                let (kept, removed): (Vec<Widget>, Vec<Widget>) = self
                    .doc
                    .widgets
                    .drain(..)
                    .partition(|w| w.page_id.as_deref() != Some(id.as_str()));
                self.doc.widgets = kept;
                self.doc.report.page_count = self.doc.pages.len() as i64;
                if self.current_page.as_deref() == Some(id.as_str()) {
                    self.current_page = self.doc.pages.first().map(|p| p.id.clone());
                }
                Ok(EditOp::AddPage { page, widgets: removed })
            }
            EditOp::SetPage(page) => {
                let pos = self
                    .doc
                    .pages
                    .iter()
                    .position(|p| p.id == page.id)
                    .ok_or_else(|| EditError::PageNotFound(page.id.clone()))?;
                let old = std::mem::replace(&mut self.doc.pages[pos], page);
                Ok(EditOp::SetPage(old))
            }
        }
    }

    /// Apply an op and record it on the undo stack (clearing redo). Consecutive move/resize of the same
    /// widget coalesce into one history entry, so a whole drag is one undo.
    pub fn do_op(&mut self, op: EditOp) -> Result<(), EditError> {
        let key = coalesce_key(&op);
        let inverse = self.apply(op)?;
        self.redo.clear();
        let coalesces = key.is_some()
            && self
                .undo
                .last()
                .map(|(_, k)| k.as_ref() == key.as_ref())
                .unwrap_or(false);
        if coalesces {
            // Keep the earlier inverse: it already restores the pre-gesture state.
            return Ok(());
        }
        self.undo.push((inverse, key));
        Ok(())
    }

    /// True when there is something to undo / redo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undo the last history entry; returns the op that was applied to undo (so the platform can mirror it
    /// to the server), or `None` if nothing to undo.
    pub fn undo(&mut self) -> Result<Option<EditOp>, EditError> {
        let Some((inverse, key)) = self.undo.pop() else {
            return Ok(None);
        };
        let applied = inverse.clone();
        let re_inverse = self.apply(inverse)?;
        self.redo.push((re_inverse, key));
        Ok(Some(applied))
    }

    /// Redo the last undone entry.
    pub fn redo(&mut self) -> Result<Option<EditOp>, EditError> {
        let Some((op, key)) = self.redo.pop() else {
            return Ok(None);
        };
        let applied = op.clone();
        let inverse = self.apply(op)?;
        self.undo.push((inverse, key));
        Ok(Some(applied))
    }

    /// Replace the report metadata (title, theme, stars…) kept apart from pages/widgets.
    pub fn report(&self) -> &Report {
        &self.doc.report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn widget(id: &str, page: &str, x: i64, y: i64) -> Widget {
        Widget {
            id: id.into(),
            page_id: Some(page.into()),
            report_id: Some("r".into()),
            widget_type: "card".into(),
            x,
            y,
            width: 200,
            height: 120,
            config: json!({}),
            z_index: 0,
            extra: Map::new(),
        }
    }

    fn page(id: &str) -> ReportPage {
        ReportPage {
            id: id.into(),
            report_id: Some("r".into()),
            title: "Page".into(),
            position: 0,
            width: 1200,
            height: 800,
            background: None,
            extra: Map::new(),
        }
    }

    fn editor() -> Editor {
        let doc = ReportDoc {
            version: 1,
            report: Report {
                id: "r".into(),
                owner_id: None,
                title: "T".into(),
                description: None,
                theme: json!({ "paletteId": "kubuno" }),
                page_count: 1,
                dataset_ids: vec![],
                file_id: None,
                is_public: false,
                is_trashed: false,
                is_starred: false,
                extra: Map::new(),
            },
            pages: vec![page("p1")],
            widgets: vec![widget("w1", "p1", 10, 10)],
        };
        Editor::new(doc)
    }

    #[test]
    fn move_then_undo_redo() {
        let mut ed = editor();
        ed.do_op(EditOp::MoveWidget { id: "w1".into(), x: 50, y: 60 }).unwrap();
        assert_eq!((ed.widget("w1").unwrap().x, ed.widget("w1").unwrap().y), (50, 60));
        ed.undo().unwrap();
        assert_eq!((ed.widget("w1").unwrap().x, ed.widget("w1").unwrap().y), (10, 10));
        ed.redo().unwrap();
        assert_eq!((ed.widget("w1").unwrap().x, ed.widget("w1").unwrap().y), (50, 60));
    }

    #[test]
    fn drag_coalesces_into_one_undo() {
        let mut ed = editor();
        ed.do_op(EditOp::MoveWidget { id: "w1".into(), x: 20, y: 20 }).unwrap();
        ed.do_op(EditOp::MoveWidget { id: "w1".into(), x: 30, y: 30 }).unwrap();
        ed.do_op(EditOp::MoveWidget { id: "w1".into(), x: 40, y: 40 }).unwrap();
        ed.undo().unwrap(); // one undo reverts the whole drag
        let w = ed.widget("w1").unwrap();
        assert_eq!((w.x, w.y), (10, 10));
        assert!(!ed.can_undo());
    }

    #[test]
    fn add_delete_widget_round_trips() {
        let mut ed = editor();
        ed.do_op(EditOp::AddWidget(widget("w2", "p1", 5, 5))).unwrap();
        assert_eq!(ed.current_widgets().len(), 2);
        ed.do_op(EditOp::DeleteWidget("w2".into())).unwrap();
        assert_eq!(ed.current_widgets().len(), 1);
        ed.undo().unwrap(); // bring w2 back
        assert_eq!(ed.current_widgets().len(), 2);
        ed.undo().unwrap(); // remove w2 again
        assert_eq!(ed.current_widgets().len(), 1);
    }

    #[test]
    fn delete_page_cascades_and_restores() {
        let mut ed = editor();
        ed.do_op(EditOp::AddPage { page: page("p2"), widgets: vec![widget("w9", "p2", 0, 0)] }).unwrap();
        assert_eq!(ed.doc.pages.len(), 2);
        ed.do_op(EditOp::DeletePage("p2".into())).unwrap();
        assert_eq!(ed.doc.pages.len(), 1);
        assert!(ed.doc.widgets.iter().all(|w| w.id != "w9"));
        ed.undo().unwrap(); // page and its widget come back
        assert_eq!(ed.doc.pages.len(), 2);
        assert!(ed.doc.widgets.iter().any(|w| w.id == "w9"));
    }

    #[test]
    fn apply_theme_is_one_undo() {
        let mut ed = editor();
        ed.do_op(EditOp::ApplyTheme {
            theme: json!({ "paletteId": "vibrant" }),
            widget_configs: vec![("w1".into(), json!({ "color": "#f94144" }))],
        })
        .unwrap();
        assert_eq!(ed.report().palette_id(), "vibrant");
        assert_eq!(ed.widget("w1").unwrap().config_get("color"), json!("#f94144"));
        ed.undo().unwrap();
        assert_eq!(ed.report().palette_id(), "kubuno");
        assert_eq!(ed.widget("w1").unwrap().config, json!({}));
    }

    #[test]
    fn new_op_clears_redo() {
        let mut ed = editor();
        ed.do_op(EditOp::MoveWidget { id: "w1".into(), x: 50, y: 50 }).unwrap();
        ed.undo().unwrap();
        assert!(ed.can_redo());
        ed.do_op(EditOp::ResizeWidget { id: "w1".into(), width: 300, height: 300 }).unwrap();
        assert!(!ed.can_redo());
    }

    #[test]
    fn unknown_widget_errors() {
        let mut ed = editor();
        let err = ed.do_op(EditOp::MoveWidget { id: "nope".into(), x: 0, y: 0 }).unwrap_err();
        assert_eq!(err, EditError::WidgetNotFound("nope".into()));
    }
}
