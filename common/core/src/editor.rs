//! `Editor`: the document being edited, its selection, its history, its layout and its pages -
//! the façade a platform drives (the desktop's `PageCanvas` now; a WASM build later).
//!
//! The platform supplies text measurement ([`Measure`]) and the time of each change; it forwards
//! keys and pointer events as calls ([`Editor::move_caret`], [`Editor::press`],
//! [`Editor::drag_to`], the edit commands), paints [`Editor::pages`], and asks where the caret and
//! the selection are. Everything that is behaviour lives here, ported from the web's
//! `nav-keys.ts`, `pointer-select.ts` and its TipTap configuration.

use serde_json::Value;

use crate::edit::commands::{self, Change, EditState};
use crate::edit::history::{History, Selection};
use crate::edit::step::{EditError, Step, TopTouch};
use crate::edit::track;
use crate::layout::caret;
use crate::layout::flow::Flow;
use crate::layout::paginate::{self, SectionGeom};
use crate::layout::{CursorMetrics, DocPx, DocumentLayout, PageLayout, SelectionRect};
use crate::marks;
use crate::measure::Measure;
use crate::model::{self, Node};
use crate::pm::{self, node_at, textblock_at};

/// A4 at 96 dpi and the factory margin (`DocumentEditorPage.tsx`, `content_files.rs`).
pub const PAGE_W: DocPx = 794.0;
pub const PAGE_H: DocPx = 1123.0;
pub const MARGIN: DocPx = 96.0;
/// `COL_GAP` (`DocumentEditorPage.tsx:286`).
pub const COL_GAP: DocPx = 36.0;

/// The page of a section: size, margins, columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSetup {
    pub width: DocPx,
    pub height: DocPx,
    pub top: DocPx,
    pub right: DocPx,
    pub bottom: DocPx,
    pub left: DocPx,
    pub columns: usize,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self { width: PAGE_W, height: PAGE_H, top: MARGIN, right: MARGIN, bottom: MARGIN, left: MARGIN, columns: 1 }
    }
}

impl PageSetup {
    pub fn content_w(&self) -> DocPx {
        (self.width - self.left - self.right).max(1.0)
    }

    pub fn content_h(&self) -> DocPx {
        (self.height - self.top - self.bottom).max(1.0)
    }

    pub fn col_gap(&self) -> DocPx {
        if self.columns > 1 { COL_GAP } else { 0.0 }
    }

    /// The width of one text column.
    pub fn col_w(&self) -> DocPx {
        let n = self.columns.max(1) as f32;
        ((self.content_w() - (n - 1.0) * self.col_gap()) / n).max(1.0)
    }

    fn geom(&self) -> SectionGeom {
        SectionGeom { content_h: self.content_h(), columns: self.columns.max(1), col_w: self.col_w(), col_gap: self.col_gap() }
    }
}

/// Paper sizes in cm (`PAPER_SIZES`).
pub fn paper_size(name: &str) -> (f32, f32) {
    match name {
        "a5" => (14.8, 21.0),
        "a3" => (29.7, 42.0),
        "letter" => (21.59, 27.94),
        "legal" => (21.59, 35.56),
        _ => (21.0, 29.7),
    }
}

/// `PX_PER_CM`.
pub const PX_PER_CM: f32 = 96.0 / 2.54;

/// Caret motions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
    /// One view height (document px) up or down.
    PageUp(i32),
    PageDown(i32),
}

/// What the ribbon shows of the selection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionInfo {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub subscript: bool,
    pub superscript: bool,
    /// `None` when mixed.
    pub font_family: Option<String>,
    pub font_size: Option<String>,
    pub color: Option<String>,
    pub highlight: Option<String>,
    pub align: String,
    /// `paragraph`, `heading`, `codeBlock`.
    pub block: String,
    pub level: u32,
    /// `bulletList`, `orderedList`, `taskList` when in one.
    pub list: Option<String>,
    pub line_height: f32,
    pub space_before: f32,
    pub space_after: f32,
    pub indent_left: f32,
    pub indent_first_line: f32,
    pub indent_right: f32,
    /// `(position, type)`.
    pub tab_stops: Vec<(f32, String)>,
    pub style_name: Option<String>,
    pub link: Option<String>,
    pub in_table: bool,
    pub has_selection: bool,
    pub keep_next: bool,
    pub keep_lines: bool,
    pub page_break_before: bool,
    /// Small capitals on the typing marks.
    pub small_caps: bool,
}

/// The document being edited.
pub struct Editor {
    stored: model::Document,
    state: EditState,
    history: History,
    revs: Vec<u64>,
    next_rev: u64,
    flow: Flow,
    layout: Option<DocumentLayout>,
    pages: Vec<PageLayout>,
    setups: Vec<PageSetup>,
    /// `nav.goalX`: the column kept across ↑/↓/Page moves.
    goal_x: Option<DocPx>,
    /// `nav.atEnd`: the caret's affinity at a wrap boundary.
    at_end: bool,
    /// The unit and range of a press in progress (word / paragraph selection by drag).
    press: Option<Press>,
    /// The inverses of a live preview not yet recorded.
    transient: Vec<Step>,
    dirty: bool,
    /// Bumped on every change of the document (not of the selection).
    pub revision: u64,
    /// The author stamped on tracked changes (`setTrackChangesUser`).
    track_user: track::TrackUser,
}

#[derive(Clone, Copy, Debug)]
struct Press {
    unit: u8,
    anchor: usize,
    range: (usize, usize),
}

impl Editor {
    /// Opens stored bytes (`content_json`).
    pub fn open(bytes: &[u8]) -> Result<Self, model::Error> {
        Ok(Self::from_document(model::Document::from_slice(bytes)?))
    }

    pub fn from_document(stored: model::Document) -> Self {
        let blocks: Vec<Node> = stored.blocks().into_iter().cloned().collect();
        let doc = Node::doc(if blocks.is_empty() { vec![Node::of_type("paragraph")] } else { blocks });
        let n = doc.children().len();
        let setups = Self::setups_of(&stored, &doc);
        let mut e = Self {
            stored,
            state: EditState { doc, sel: Selection::caret(1), stored_marks: None },
            history: History::new(),
            revs: (1..=n as u64).collect(),
            next_rev: n as u64 + 1,
            flow: Flow::new(),
            layout: None,
            pages: Vec::new(),
            setups,
            goal_x: None,
            at_end: false,
            press: None,
            transient: Vec::new(),
            dirty: false,
            revision: 0,
            track_user: track::TrackUser::default(),
        };
        // The caret starts at the first text position.
        if let Some((_, s)) = pm::textblocks(&e.state.doc).first() {
            e.state.sel = Selection::caret(*s);
        }
        e
    }

    /// The page setup of each section: section 0 from the envelope's `sections[0].margins` and
    /// `paperSize`, the others from their `sectionBreak` node.
    fn setups_of(stored: &model::Document, doc: &Node) -> Vec<PageSetup> {
        let mut base = PageSetup::default();
        let paper = stored.root_value("paperSize").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "a4".into());
        let (w, h) = paper_size(&paper);
        let sec0 = stored.root_value("sections").and_then(|v| v.as_array().and_then(|a| a.first().cloned()));
        let landscape = sec0.as_ref().and_then(|s| s.get("orientation")).and_then(Value::as_str) == Some("landscape");
        base.width = ((if landscape { h } else { w }) * PX_PER_CM).round();
        base.height = ((if landscape { w } else { h }) * PX_PER_CM).round();
        if let Some(m) = sec0.as_ref().and_then(|s| s.get("margins")) {
            let g = |k: &str, d: f32| m.get(k).and_then(Value::as_f64).map(|v| v as f32).unwrap_or(d);
            base.top = g("top", MARGIN);
            base.right = g("right", MARGIN);
            base.bottom = g("bottom", MARGIN);
            base.left = g("left", MARGIN);
        }
        if let Some(c) = sec0.as_ref().and_then(|s| s.get("columns")).and_then(Value::as_u64) {
            base.columns = (c as usize).clamp(1, 3);
        }
        let mut out = vec![base];
        for node in doc.children() {
            if node.node_type() == Some("sectionBreak") {
                let a = node.attrs();
                let g = |k: &str, d: f32| a.get(k).and_then(Value::as_f64).map(|v| v as f32).unwrap_or(d);
                let land = a.get("orientation").and_then(Value::as_str) == Some("landscape");
                out.push(PageSetup {
                    width: ((if land { h } else { w }) * PX_PER_CM).round(),
                    height: ((if land { w } else { h }) * PX_PER_CM).round(),
                    top: g("top", base.top),
                    right: g("right", base.right),
                    bottom: g("bottom", base.bottom),
                    left: g("left", base.left),
                    columns: 1,
                });
            }
        }
        out
    }

    /// The document as stored, with the edits: the envelope and every sibling key kept.
    pub fn to_bytes(&mut self) -> Result<Vec<u8>, model::Error> {
        self.stored.set_blocks(self.state.doc.children().to_vec());
        self.stored.to_vec()
    }

    pub fn doc(&self) -> &Node {
        &self.state.doc
    }

    pub fn stored(&self) -> &model::Document {
        &self.stored
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// After a successful save.
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// The content is not what the server holds (a restored crash copy): unsaved until the next save.
    pub fn mark_unsaved(&mut self) {
        self.dirty = true;
        self.revision += 1;
    }

    pub fn selection(&self) -> Selection {
        self.state.sel
    }

    pub fn state(&self) -> &EditState {
        &self.state
    }

    pub fn page_setup(&self, section: usize) -> PageSetup {
        self.setups.get(section).or(self.setups.first()).copied().unwrap_or_default()
    }

    pub fn page_setups(&self) -> &[PageSetup] {
        &self.setups
    }

    /// Changes section 0's margins (the rulers). Live: `record` false while dragging; the release
    /// records it (written to `sections[0].margins`; a bare document becomes an envelope then,
    /// since it has nowhere else to keep its margins).
    pub fn set_margins(&mut self, left: f32, right: f32, top: f32, bottom: f32, record: bool, now_ms: u64) {
        // A later section keeps its margins in its `sectionBreak` node (`setSectionMargins`): a
        // previewed then recorded step of the body, undoable like on the web.
        let section = self.caret_section();
        if section > 0 {
            if let Some((index, _)) = self.section_breaks().get(section - 1).copied() {
                let patch = move |s: &mut EditState| {
                    commands::set_attrs_at(
                        s,
                        &[vec![index]],
                        &[("top", Value::from(top.round())), ("right", Value::from(right.round())), ("bottom", Value::from(bottom.round())), ("left", Value::from(left.round()))],
                    )
                };
                let _ = if record { self.commit_transient(now_ms, patch) } else { self.apply_transient(patch) };
            }
            return;
        }
        if let Some(s) = self.setups.first_mut() {
            s.left = left.max(0.0);
            s.right = right.max(0.0);
            s.top = top.max(0.0);
            s.bottom = bottom.max(0.0);
        }
        self.invalidate_layout();
        if record {
            let m = serde_json::json!({ "top": top.round(), "right": right.round(), "bottom": bottom.round(), "left": left.round() });
            self.stored.set_section_margins(&m);
            self.dirty = true;
            self.revision += 1;
        }
    }

    // ── Page setup (Mise en page) ───────────────────────────────────────────

    /// The top-level `sectionBreak` blocks: (index among the body's blocks, start position).
    fn section_breaks(&self) -> Vec<(usize, usize)> {
        let mut pos = 0;
        let mut out = Vec::new();
        for (i, block) in self.state.doc.children().iter().enumerate() {
            if block.node_type() == Some("sectionBreak") {
                out.push((i, pos));
            }
            pos += pm::node_size(block);
        }
        out
    }

    /// The section the caret is in (`currentSecIdx`: the number of section breaks before the
    /// selection's head; 0 is the base section of the envelope).
    pub fn caret_section(&self) -> usize {
        let head = self.state.sel.head;
        self.section_breaks().iter().filter(|(_, p)| *p < head).count()
    }

    /// The orientation of `section` (`orientationOfSec`).
    pub fn orientation(&self, section: usize) -> &'static str {
        let landscape = if section == 0 {
            self.stored.section0().and_then(|s| s.get("orientation").and_then(Value::as_str).map(str::to_string)).as_deref() == Some("landscape")
        } else {
            self.section_breaks()
                .get(section - 1)
                .and_then(|(i, _)| self.state.doc.children().get(*i))
                .is_some_and(|n| n.attrs().get("orientation").and_then(Value::as_str) == Some("landscape"))
        };
        if landscape {
            "landscape"
        } else {
            "portrait"
        }
    }

    /// The document's paper size (`paperSize`, global to the document; `a4` by default).
    pub fn paper_size(&self) -> String {
        self.stored.root_value("paperSize").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "a4".into())
    }

    /// The base section's columns (1–3; `sections[0].columns`, 1 by default).
    pub fn columns(&self) -> usize {
        self.stored.section0().and_then(|s| s.get("columns").and_then(Value::as_u64)).map(|c| (c as usize).clamp(1, 3)).unwrap_or(1)
    }

    /// A change of the envelope (page setup, header and footer, page numbers…): not a step of the
    /// body, so - like the web, where these live outside ProseMirror - not undone by Ctrl+Z.
    fn envelope_changed(&mut self) {
        self.setups = Self::setups_of(&self.stored, &self.state.doc);
        self.flow.clear();
        self.dirty = true;
        self.revision += 1;
        self.invalidate_layout();
    }

    // ── Track changes (`documents/track-changes.ts`) ─────────────────────────

    /// Tracking is on (`trackChanges` of the envelope, Word's `w:trackChanges`).
    pub fn tracking(&self) -> bool {
        self.stored.root_value("trackChanges").as_ref().is_some_and(marks::truthy)
    }

    /// Turns tracking on or off: a document setting, saved in the envelope (`trackChanges:
    /// meta.trackChanges || undefined`).
    pub fn set_tracking(&mut self, on: bool) -> Result<(), EditError> {
        self.set_envelope_value("trackChanges", on.then_some(Value::Bool(true)))
    }

    /// The author of the revisions made from now on (`setTrackChangesUser`).
    pub fn set_track_user(&mut self, user: track::TrackUser) {
        self.track_user = user;
    }

    /// A command inserting at the selection (typing, paste, an inline node): tracked when tracking
    /// is on. `epoch_ms` dates the revision.
    pub fn insert_edit(&mut self, now_ms: u64, epoch_ms: u64, f: impl FnOnce(&mut EditState) -> Result<Change, EditError>) -> Result<(), EditError> {
        if !self.tracking() {
            return self.apply(now_ms, f);
        }
        let stamp = track::Stamp::now(&self.track_user, epoch_ms);
        self.apply(now_ms, |s| track::tracked_insert(s, &stamp, f))
    }

    /// Backspace (`backward`) / Delete: tracked when tracking is on.
    pub fn delete_edit(&mut self, now_ms: u64, epoch_ms: u64, backward: bool, word: bool, word_pos: Option<usize>) -> Result<(), EditError> {
        if !self.tracking() {
            return self.apply(now_ms, |s| if backward { commands::delete_backward(s, word, word_pos) } else { commands::delete_forward(s, word, word_pos) });
        }
        let stamp = track::Stamp::now(&self.track_user, epoch_ms);
        self.apply(now_ms, |s| track::tracked_delete(s, &stamp, backward, word, word_pos))
    }

    /// Every revision of the document (`documentChanges`).
    pub fn changes(&self) -> Vec<track::TrackedChange> {
        track::document_changes(&self.state.doc)
    }

    /// Accepts (`accept`) or rejects the revisions `ids` (all when `None`), as one undoable step.
    pub fn decide_changes(&mut self, now_ms: u64, ids: Option<&[String]>, accept: bool) -> Result<(), EditError> {
        self.apply(now_ms, |s| {
            let mut c = track::decide(s, ids, accept)?;
            c.merge = false;
            Ok(c)
        })
    }

    /// « Document final » (`setRevisionDisplay('final')`): deleted text leaves the layout and the
    /// marks are not shown. A view setting, not saved.
    pub fn final_view(&self) -> bool {
        self.flow.final_view
    }

    pub fn set_final_view(&mut self, on: bool) {
        if self.flow.final_view != on {
            self.flow.final_view = on;
            self.flow.clear();
            self.invalidate_layout();
        }
    }

    /// Portrait or landscape for the caret's section (`setOrientation`): the base section's in
    /// `sections[0].orientation`, a later section's in its `sectionBreak` node (an undoable step,
    /// as on the web).
    pub fn set_orientation(&mut self, orientation: &str, now_ms: u64) -> Result<(), EditError> {
        let orientation = if orientation == "landscape" { "landscape" } else { "portrait" };
        let section = self.caret_section();
        if section == 0 {
            if self.orientation(0) == orientation {
                return Err(EditError::NotApplicable);
            }
            self.stored.set_section0_value("orientation", &Value::String(orientation.into()));
            self.envelope_changed();
            return Ok(());
        }
        let Some((index, _)) = self.section_breaks().get(section - 1).copied() else { return Err(EditError::NoSuchNode) };
        self.apply(now_ms, |s| commands::set_attrs_at(s, &[vec![index]], &[("orientation", Value::String(orientation.into()))]))
    }

    /// The paper size (`a4`, `a5`, `a3`, `letter`, `legal`), global to the document.
    pub fn set_paper_size(&mut self, paper: &str) -> Result<(), EditError> {
        if !matches!(paper, "a4" | "a5" | "a3" | "letter" | "legal") {
            return Err(EditError::NotApplicable);
        }
        if self.paper_size() == paper {
            return Err(EditError::NotApplicable);
        }
        self.stored.set_root_value("paperSize", &Value::String(paper.into()));
        self.envelope_changed();
        Ok(())
    }

    /// The base section's columns (`setColumns`: 1 to 3; the sections after a break keep one, as
    /// on the web).
    pub fn set_columns(&mut self, columns: usize) -> Result<(), EditError> {
        let columns = columns.clamp(1, 3);
        if self.columns() == columns {
            return Err(EditError::NotApplicable);
        }
        self.stored.set_section0_value("columns", &Value::from(columns as u64));
        self.envelope_changed();
        Ok(())
    }

    /// A section break at the caret (`insertBreak`): a `sectionBreak` carrying the current
    /// section's orientation and the default margins, then a paragraph - the web's node with all
    /// its default attributes, as TipTap writes it.
    pub fn insert_section_break(&mut self, now_ms: u64) -> Result<(), EditError> {
        let orientation = self.orientation(self.caret_section());
        let attrs = serde_json::json!({
            "bottom": 96, "footer": null, "header": null, "hfLinked": true, "left": 96,
            "orientation": orientation, "pageColor": null, "right": 96, "top": 96
        });
        let mut leaf = Node::of_type("sectionBreak");
        leaf.set_value("attrs", &attrs);
        self.apply(now_ms, |s| commands::insert_blocks(s, vec![leaf]))
    }

    /// A root key of the envelope (`pageNumbers`, `watermark`…), `None` when absent or bare.
    pub fn envelope_value(&self, key: &str) -> Option<Value> {
        self.stored.root_value(key)
    }

    /// Writes a root key of the envelope (`Some`) or removes it (`None`), as the web's
    /// `serializeDoc` would on its next save: page numbers, their format and start, the watermark,
    /// the page border, the line numbers, « première page différente »… Not undoable (React state
    /// on the web). `NotApplicable` when nothing changes.
    pub fn set_envelope_value(&mut self, key: &str, value: Option<Value>) -> Result<(), EditError> {
        let current = self.stored.root_value(key);
        match value {
            Some(v) if current.as_ref() == Some(&v) => return Err(EditError::NotApplicable),
            Some(v) => self.stored.set_root_value(key, &v),
            None if current.is_none() => return Err(EditError::NotApplicable),
            None => {
                self.stored.remove_root_value(key);
            }
        }
        self.envelope_changed();
        Ok(())
    }

    /// The page colour of the caret's section (`onPageColor`): the base section's in the envelope's
    /// `pageColor`, a later section's in its `sectionBreak` (an undoable step). `None` removes it.
    pub fn set_page_color(&mut self, color: Option<&str>, now_ms: u64) -> Result<(), EditError> {
        let section = self.caret_section();
        if section == 0 {
            return self.set_envelope_value("pageColor", color.map(|c| Value::String(c.to_string())));
        }
        let Some((index, _)) = self.section_breaks().get(section - 1).copied() else { return Err(EditError::NoSuchNode) };
        let value = color.map(|c| Value::String(c.to_string())).unwrap_or(Value::Null);
        self.apply(now_ms, |s| commands::set_attrs_at(s, &[vec![index]], &[("pageColor", value)]))
    }

    // ── Table of contents (Références › Table des matières) ──────────────────

    /// The outline with each entry's page (`ops.outline()`: `pageIndexForHead(pages, pos + 1) + 1`).
    pub fn outline(&mut self, m: &dyn Measure) -> Vec<crate::edit::toc::OutlineItem> {
        self.relayout(m);
        let pages = &self.pages;
        crate::edit::toc::collect_outline(&self.state.doc, &|pos| paginate::page_of_pos(pages, pos + 1).map(|p| p + 1).unwrap_or(1))
    }

    /// `generateToc`: the table built from the outline, in place of the existing one or before the
    /// caret's block - then a second pass, because the table itself moves the pages it numbers
    /// (the web runs it 400 ms later; here it joins the same undo step).
    pub fn generate_toc(&mut self, settings: &crate::edit::toc::TocSettings, title: &str, empty_text: &str, now_ms: u64, m: &dyn Measure) -> Result<(), EditError> {
        let outline = self.outline(m);
        self.apply(now_ms, |s| crate::edit::toc::generate(s, &outline, settings, title, empty_text))?;
        let outline = self.outline(m);
        let sel = self.state.sel;
        let again = self.apply(now_ms, |s| crate::edit::toc::generate(s, &outline, settings, title, empty_text));
        self.state.sel = sel;
        match again {
            Ok(()) | Err(EditError::NotApplicable) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// `detectTocOpts`: the settings of the table the document has (« Mettre à jour » rebuilds that
    /// table, not the one last configured); the defaults when there is none.
    pub fn detect_toc_settings(&self) -> crate::edit::toc::TocSettings {
        let mut s = crate::edit::toc::TocSettings::default();
        let (mut levels, mut pages, mut leader) = (0usize, false, false);
        for node in self.state.doc.children() {
            let a = node.attrs();
            if let Some(l) = a.get("tocLevel").filter(|v| !v.is_null()) {
                levels = levels.max(l.as_u64().unwrap_or(1) as usize);
                pages |= a.get("tocPage").is_some_and(|v| !v.is_null()) || a.get("tocPageText").is_some_and(|v| !v.is_null());
                leader |= a.get("tocLeader").and_then(Value::as_bool) == Some(true);
            }
        }
        if levels > 0 {
            s.levels = levels;
            s.show_page_numbers = pages;
            if !leader {
                s.leader = "none".into();
            }
        }
        s
    }

    /// The gallery's « Table manuelle »: a title and three placeholder entries to type over, put
    /// where the table is (or before the caret's block).
    pub fn manual_toc(&mut self, title: &str, entries: [&str; 3], now_ms: u64) -> Result<(), EditError> {
        let outline: Vec<crate::edit::toc::OutlineItem> =
            entries.iter().enumerate().map(|(i, t)| crate::edit::toc::OutlineItem { text: (*t).to_string(), level: i + 1, pos: usize::MAX, page: i + 1 }).collect();
        self.apply(now_ms, |s| crate::edit::toc::generate(s, &outline, &crate::edit::toc::TocSettings::default(), title, ""))
    }

    // ── Page decorations (header, footer, page number, footnotes, border…) ──

    /// What every page's decorations read from the document: the envelope's keys, the section
    /// metadata of the breaks, the line counts of the pages (continuous line numbering), with the
    /// title and today's date the platform supplies. Built once per paint.
    pub fn decor_context(&self, title: &str, date: &str) -> crate::layout::decor::DecorContext {
        use crate::layout::decor::{body_lines, hf_content, DecorContext, SectionMeta, ROOT_KEYS};
        let mut ctx = DecorContext { title: title.to_string(), date: date.to_string(), ..DecorContext::default() };
        for k in ROOT_KEYS {
            if let Some(v) = self.stored.root_value(k) {
                ctx.root.insert(k.to_string(), v);
            }
        }
        ctx.sections.push(SectionMeta { hf_linked: true, ..SectionMeta::default() });
        for block in self.state.doc.children().iter().filter(|b| b.node_type() == Some("sectionBreak")) {
            let a = block.attrs();
            ctx.sections.push(SectionMeta {
                hf_linked: a.get("hfLinked").and_then(Value::as_bool) != Some(false),
                header: a.get("header").filter(|v| !v.is_null()).and_then(hf_content),
                footer: a.get("footer").filter(|v| !v.is_null()).and_then(hf_content),
                page_color: a.get("pageColor").and_then(Value::as_str).map(str::to_string),
            });
        }
        if ctx.root.contains_key("lineNumbers") {
            let mut before = 0;
            for page in &self.pages {
                ctx.body_lines_before.push(before);
                before += body_lines(page, &|pos| self.suppresses_line_numbers(pos));
            }
        }
        ctx
    }

    /// Whether the block starting at `pos` has `suppressLineNumbers`.
    pub fn suppresses_line_numbers(&self, pos: usize) -> bool {
        pm::node_at_pos(&self.state.doc, pos).is_some_and(|n| n.attrs().get("suppressLineNumbers").and_then(Value::as_bool) == Some(true))
    }

    /// The decorations of page `index` (after [`Editor::relayout`]).
    pub fn page_decor(&self, index: usize, ctx: &crate::layout::decor::DecorContext, m: &dyn Measure) -> Option<crate::layout::decor::PageDecor> {
        use crate::layout::decor::{page_decor, PagePos, Sheet};
        let page = self.pages.get(index)?;
        let s = self.page_setup(page.sec_idx);
        let sheet = Sheet { page_w: s.width, page_h: s.height, margin_h: s.left, margin_v: s.top, margin_bottom: s.bottom, margin_right: s.right };
        let pos = PagePos { index, count: self.pages.len(), section: page.sec_idx };
        let para_style = |p: usize| pm::node_at_pos(&self.state.doc, p).map(|n| crate::layout::decor::para_style_of(&n.attrs())).unwrap_or((None, None));
        Some(page_decor(page, pos, &sheet, ctx, &|p| self.suppresses_line_numbers(p), &para_style, m))
    }

    /// The measurer changed (fonts were loaded, the device was recreated): lay everything out again.
    pub fn mark_measure_changed(&mut self) {
        self.flow.clear();
        self.invalidate_layout();
    }

    fn invalidate_layout(&mut self) {
        self.layout = None;
        self.pages.clear();
    }

    /// Lays the document out and paginates it if anything changed.
    pub fn relayout(&mut self, m: &dyn Measure) {
        if self.layout.is_some() {
            return;
        }
        // Sections are re-derived from the body (a section break may have been typed or deleted).
        let base = self.setups.first().copied().unwrap_or_default();
        let mut setups = vec![base];
        for node in self.state.doc.children() {
            if node.node_type() == Some("sectionBreak") {
                let a = node.attrs();
                let g = |k: &str, d: f32| a.get(k).and_then(Value::as_f64).map(|v| v as f32).unwrap_or(d);
                let land = a.get("orientation").and_then(Value::as_str) == Some("landscape");
                let (w, h) = if land { (base.height.max(base.width), base.width.min(base.height)) } else { (base.width.min(base.height), base.height.max(base.width)) };
                setups.push(PageSetup { width: w, height: h, top: g("top", base.top), right: g("right", base.right), bottom: g("bottom", base.bottom), left: g("left", base.left), columns: 1 });
            }
        }
        self.setups = setups;
        let widths: Vec<DocPx> = self.setups.iter().map(PageSetup::col_w).collect();
        let w0 = widths[0];
        // « Numérotation des titres » (`headingNumbers`): a document setting the parse reads.
        let numbers = self.stored.root_value("headingNumbers").and_then(|v| v.as_bool()).unwrap_or(false);
        if numbers != self.flow.heading_numbers {
            self.flow.clear();
            self.flow.heading_numbers = numbers;
        }
        let layout = self.flow.layout(&self.state.doc, &self.revs, &|s| widths.get(s).copied().unwrap_or(w0), m);
        let geoms: Vec<SectionGeom> = self.setups.iter().map(PageSetup::geom).collect();
        let mut pages = paginate::paginate(&layout, &geoms);
        paginate::append_endnote_pages(&mut pages, &layout, &geoms, m);
        self.pages = pages;
        self.layout = Some(layout);
    }

    /// The continuous layout (after [`Editor::relayout`]).
    pub fn layout(&self) -> Option<&DocumentLayout> {
        self.layout.as_ref()
    }

    /// The pages (after [`Editor::relayout`]).
    pub fn pages(&self) -> &[PageLayout] {
        &self.pages
    }

    /// How many blocks the last layout laid out afresh.
    pub fn last_layout_misses(&self) -> usize {
        self.flow.last_misses
    }

    // ── Coordinates: continuous layout ↔ page ───────────────────────────────

    /// The page and the column band showing the continuous ordinate `y` of a line holding `pos`
    /// (page index, band start, band x shift).
    pub fn page_band_of(&self, pos: usize, y: DocPx) -> Option<(usize, DocPx, DocPx)> {
        let page = paginate::page_of_pos(&self.pages, pos).unwrap_or_else(|| paginate::page_of_y(&self.pages, y));
        let p = self.pages.get(page)?;
        let band = p.columns.iter().rev().find(|b| b.start_y <= y + 0.01).or(p.columns.first()).copied().unwrap_or(crate::layout::ColumnBand { start_y: p.start_y, x_shift: 0.0 });
        Some((page, band.start_y, band.x_shift))
    }

    /// A point on page `page` (content-box coordinates) → the continuous layout's coordinates.
    pub fn page_to_continuous(&self, page: usize, x: DocPx, y: DocPx) -> (DocPx, DocPx) {
        let Some(p) = self.pages.get(page) else { return (x, y) };
        if p.columns.len() > 1 {
            let col_w = self.page_setup(p.sec_idx).col_w();
            for b in p.columns.iter().rev() {
                if x >= b.x_shift - COL_GAP / 2.0 || b.x_shift == 0.0 {
                    let _ = col_w;
                    return (x - b.x_shift, b.start_y + y);
                }
            }
        }
        let start = p.columns.first().map(|b| b.start_y).unwrap_or(p.start_y);
        (x, start + y)
    }

    /// The footnote or endnote reference at the continuous point (`x`, `y`), by its position - a
    /// click there opens the note's editor (`DocumentEditorPage.tsx:8461`).
    pub fn note_at_point(&self, x: DocPx, y: DocPx) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        let (_, line) = layout.lines().find(|(_, l)| !l.phantom && y >= l.y && y < l.y + l.height)?;
        line.spans
            .iter()
            .find(|s| matches!(s.kind, crate::layout::SpanKind::Footnote { .. } | crate::layout::SpanKind::Endnote { .. }) && x >= s.x - 2.0 && x <= s.x + s.width + 2.0)
            .map(|s| s.pm_pos)
    }

    // ── Caret and selection geometry ────────────────────────────────────────

    /// The caret (continuous coordinates).
    pub fn caret_metrics(&self, m: &dyn Measure) -> Option<CursorMetrics> {
        let layout = self.layout.as_ref()?;
        Some(caret::pos_to_coords(layout, self.state.sel.head, self.at_end, m))
    }

    /// The caret's page and its box on that page (content-box coordinates): `(page, x, y, h, lean)`.
    pub fn caret_on_page(&self, m: &dyn Measure) -> Option<(usize, DocPx, DocPx, DocPx, f32)> {
        let c = self.caret_metrics(m)?;
        let (page, start, shift) = self.page_band_of(self.state.sel.head, c.line_top.unwrap_or(c.y))?;
        Some((page, c.x + shift, c.y - start, c.height, c.italic_angle))
    }

    /// The selection's rectangles per page (content-box coordinates).
    pub fn selection_on_pages(&self, m: &dyn Measure) -> Vec<(usize, SelectionRect)> {
        let Some(layout) = self.layout.as_ref() else { return Vec::new() };
        let (from, to) = (self.state.sel.from(), self.state.sel.to());
        let mut out = Vec::new();
        for r in caret::selection_rects(layout, from, to, m) {
            let pos = caret::coords_to_pos(layout, r.x + 0.5, r.y + 0.5, m);
            if let Some((page, start, shift)) = self.page_band_of(pos, r.y) {
                let ch = self.page_setup(self.pages[page].sec_idx).content_h();
                let mut rr = SelectionRect { x: r.x + shift, y: r.y - start, w: r.w, h: r.h };
                // Never past the page's content box (the joint overlap reaches the next page).
                if rr.y + rr.h > ch {
                    rr.h = (ch - rr.y).max(0.0);
                }
                out.push((page, rr));
            }
        }
        out
    }

    // ── Selection and navigation ────────────────────────────────────────────

    pub fn set_selection(&mut self, sel: Selection) {
        let size = pm::content_size(&self.state.doc);
        self.state.sel = Selection::new(sel.anchor.min(size), sel.head.min(size));
        self.state.stored_marks = None;
        self.goal_x = None;
    }

    /// Ctrl+A.
    pub fn select_all(&mut self) {
        let tbs = pm::textblocks(&self.state.doc);
        if let (Some((_, s)), Some((lp, ls))) = (tbs.first(), tbs.last()) {
            let len = node_at(&self.state.doc, lp).map(|n| crate::edit::inline::content_len(n.children())).unwrap_or(0);
            self.state.sel = Selection::new(*s, ls + len);
        }
        self.goal_x = None;
        self.at_end = true;
    }

    /// Moves the caret (`extend`: Shift held), as `nav-keys.ts` and the browser do.
    pub fn move_caret(&mut self, motion: Motion, extend: bool, m: &dyn Measure) {
        let Some(layout) = self.layout.as_ref() else { return };
        let sel = self.state.sel;
        let head = sel.head;
        let vertical = matches!(motion, Motion::Up | Motion::Down | Motion::PageUp(_) | Motion::PageDown(_));
        let mut keep_goal = false;
        let mut new_at_end = false;
        let new_head = match motion {
            // A selection collapses to its edge (the browser's ←/→).
            Motion::Left if !extend && !sel.is_empty() => sel.from(),
            Motion::Right if !extend && !sel.is_empty() => sel.to(),
            Motion::Left => commands::prev_char_pos(&self.state.doc, head).unwrap_or(head),
            Motion::Right => commands::next_char_pos(&self.state.doc, head).unwrap_or(head),
            Motion::WordLeft => {
                let p = caret::prev_word_pos(layout, head);
                if p == head {
                    commands::prev_char_pos(&self.state.doc, head).map(|q| caret::prev_word_pos(layout, q).min(q)).unwrap_or(head)
                } else {
                    p
                }
            }
            Motion::WordRight => {
                let p = caret::next_word_pos(layout, head);
                if p == head {
                    commands::next_char_pos(&self.state.doc, head).unwrap_or(head)
                } else {
                    p
                }
            }
            Motion::LineStart => caret::line_start_at(layout, head, self.at_end),
            Motion::LineEnd => {
                new_at_end = true;
                caret::line_end_at(layout, head, self.at_end)
            }
            Motion::DocStart => caret::doc_start(layout),
            Motion::DocEnd => caret::doc_end(layout),
            Motion::Up | Motion::Down => {
                let cm = caret::pos_to_coords(layout, head, self.at_end, m);
                let gx = *self.goal_x.get_or_insert(cm.x);
                let dir = if motion == Motion::Up { -1 } else { 1 };
                match caret::adjacent_line_center(layout, cm.line_top.unwrap_or(cm.y), dir) {
                    None => head,
                    Some(ty) => {
                        let p = caret::coords_to_pos(layout, gx, ty, m);
                        let e = caret::pos_to_coords(layout, p, true, m);
                        let s = caret::pos_to_coords(layout, p, false, m);
                        let (te, ts) = (e.line_top.unwrap_or(e.y), s.line_top.unwrap_or(s.y));
                        new_at_end = te != ts
                            && (te + e.line_h.unwrap_or(e.height) / 2.0 - ty).abs() < (ts + s.line_h.unwrap_or(s.height) / 2.0 - ty).abs();
                        keep_goal = true;
                        p
                    }
                }
            }
            Motion::PageUp(vh) | Motion::PageDown(vh) => {
                let cm = caret::pos_to_coords(layout, head, self.at_end, m);
                let gx = *self.goal_x.get_or_insert(cm.x);
                let dy = if matches!(motion, Motion::PageDown(_)) { vh as f32 } else { -(vh as f32) };
                keep_goal = true;
                caret::coords_to_pos(layout, gx, cm.y + dy, m)
            }
        };
        self.at_end = new_at_end;
        if !(vertical && keep_goal) {
            self.goal_x = None;
        }
        let anchor = if extend { sel.anchor } else { new_head };
        self.state.sel = Selection::new(anchor, new_head);
        self.state.stored_marks = None;
    }

    /// A press at continuous `(x, y)`: `clicks` 1 = caret (Shift: extend), 2 = word, 3 = paragraph.
    pub fn press(&mut self, x: DocPx, y: DocPx, clicks: u32, shift: bool, m: &dyn Measure) {
        let Some(layout) = self.layout.as_ref() else { return };
        let pos = caret::coords_to_pos(layout, x, y, m);
        self.goal_x = None;
        self.state.stored_marks = None;
        // A click past the end of a wrapped line keeps the caret on that line.
        self.at_end = Self::is_line_end_hit(layout, pos, y);
        let unit = if clicks >= 3 { 2 } else if clicks == 2 { 1 } else { 0 };
        if unit == 0 {
            let anchor = if shift { self.state.sel.anchor } else { pos };
            self.state.sel = Selection::new(anchor, pos);
            self.press = Some(Press { unit: 0, anchor, range: (pos, pos) });
            return;
        }
        let range = if unit == 1 { caret::word_boundaries_at(layout, pos) } else { self.paragraph_range_at(pos) };
        self.state.sel = Selection::new(range.0, range.1);
        self.press = Some(Press { unit, anchor: range.0, range });
    }

    fn is_line_end_hit(layout: &DocumentLayout, pos: usize, y: DocPx) -> bool {
        for p in &layout.paragraphs {
            for (i, l) in p.lines.iter().enumerate() {
                if y >= l.y && y <= l.y + l.height && pos == l.pm_end {
                    return p.lines.get(i + 1).is_some_and(|n| n.pm_start == pos);
                }
            }
        }
        false
    }

    /// Triple-click: inside a table cell, the cell's deepest paragraph; elsewhere the layout's
    /// paragraph (`paragraphRangeAt`).
    pub fn paragraph_range_at(&self, pos: usize) -> (usize, usize) {
        if let Some((path, start)) = textblock_at(&self.state.doc, pos) {
            let in_cell = (0..path.len()).any(|d| node_at(&self.state.doc, &path[..d]).is_some_and(|n| matches!(n.node_type(), Some("tableCell") | Some("tableHeader"))));
            if in_cell {
                let len = node_at(&self.state.doc, &path).map(|n| crate::edit::inline::content_len(n.children())).unwrap_or(0);
                return (start, start + len);
            }
        }
        match self.layout.as_ref() {
            Some(l) => caret::paragraph_boundaries_at(l, pos),
            None => (pos, pos),
        }
    }

    /// Dragging with the button held: extends by character, word or paragraph.
    pub fn drag_to(&mut self, x: DocPx, y: DocPx, m: &dyn Measure) {
        let Some(press) = self.press else { return };
        let Some(layout) = self.layout.as_ref() else { return };
        let p2 = caret::coords_to_pos(layout, x, y, m);
        if press.unit == 0 {
            self.state.sel = Selection::new(press.anchor, p2);
            self.at_end = Self::is_line_end_hit(layout, p2, y);
            return;
        }
        let r2 = if press.unit == 1 { caret::word_boundaries_at(layout, p2) } else { self.paragraph_range_at(p2) };
        let (rf, rt) = press.range;
        self.state.sel = if p2 >= rf { Selection::new(rf, r2.1.max(rt)) } else { Selection::new(rt, r2.0.min(rf)) };
    }

    /// The button was released.
    pub fn release(&mut self) {
        self.press = None;
    }

    pub fn pressing(&self) -> bool {
        self.press.is_some()
    }

    // ── Editing ─────────────────────────────────────────────────────────────

    /// Runs an edit command, records it in the history (time `now_ms`), and keeps the layout's
    /// revision counters, the dirty flag and the trailing paragraph (TipTap `TrailingNode`).
    pub fn apply(&mut self, now_ms: u64, f: impl FnOnce(&mut EditState) -> Result<Change, EditError>) -> Result<(), EditError> {
        let mut change = f(&mut self.state)?;
        if change.is_empty() {
            // A stored-marks-only change (a toggle on a caret).
            self.revision += 1;
            return Ok(());
        }
        // TrailingNode: the body always ends with a paragraph.
        let ends_with_paragraph = self.state.doc.children().last().is_some_and(|n| n.node_type() == Some("paragraph"));
        if !ends_with_paragraph {
            let n = self.state.doc.children().len();
            let step = Step { parent: vec![], from: n, to: n, with: vec![Node::of_type("paragraph")] };
            if let Ok(inv) = step.apply(&mut self.state.doc) {
                change.steps.push(step);
                change.inverses.push(inv);
            }
        }
        self.touch(&change.steps);
        self.history.record(change.steps, change.inverses, change.before, change.after, now_ms, change.merge);
        self.after_change();
        Ok(())
    }

    fn after_change(&mut self) {
        self.dirty = true;
        self.revision += 1;
        self.goal_x = None;
        self.at_end = false;
        self.invalidate_layout();
    }

    /// Bumps the revision of every top-level block the steps touched.
    fn touch(&mut self, steps: &[Step]) {
        for s in steps {
            match s.top_level_touch() {
                TopTouch::Block(i) => {
                    if let Some(r) = self.revs.get_mut(i) {
                        *r = self.next_rev;
                        self.next_rev += 1;
                    }
                }
                TopTouch::Splice { from, to, inserted } => {
                    let to = to.min(self.revs.len());
                    let from = from.min(to);
                    let fresh: Vec<u64> = (0..inserted as u64).map(|k| self.next_rev + k).collect();
                    self.next_rev += inserted as u64;
                    self.revs.splice(from..to, fresh);
                }
            }
        }
    }

    /// A live preview (a ruler drag in progress): applied and laid out, not recorded. Successive
    /// previews replace each other; [`Editor::commit_transient`] records one step from the state
    /// before the first preview, [`Editor::cancel_transient`] restores it.
    pub fn apply_transient(&mut self, f: impl FnOnce(&mut EditState) -> Result<Change, EditError>) -> Result<(), EditError> {
        self.revert_transient();
        let sel = self.state.sel;
        let change = f(&mut self.state)?;
        self.state.sel = sel;
        self.touch(&change.steps);
        self.transient = change.inverses;
        self.revision += 1;
        self.invalidate_layout();
        Ok(())
    }

    fn revert_transient(&mut self) {
        let inverses = std::mem::take(&mut self.transient);
        let mut applied = Vec::new();
        for inv in inverses.iter().rev() {
            if let Ok(redo) = inv.apply(&mut self.state.doc) {
                applied.push(inv.clone());
                let _ = redo;
            }
        }
        if !applied.is_empty() {
            self.touch(&applied);
            self.invalidate_layout();
        }
    }

    /// Ends a preview with the final change, recorded as one undo step.
    pub fn commit_transient(&mut self, now_ms: u64, f: impl FnOnce(&mut EditState) -> Result<Change, EditError>) -> Result<(), EditError> {
        self.revert_transient();
        let r = self.apply(now_ms, f);
        self.history.stop_capturing();
        r
    }

    /// Ends a preview without a change.
    pub fn cancel_transient(&mut self) {
        self.revert_transient();
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo(&mut self) -> bool {
        match self.history.undo(&mut self.state.doc) {
            Some((sel, steps)) => {
                self.touch(&steps);
                self.state.sel = sel;
                self.state.stored_marks = None;
                self.after_change();
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.history.redo(&mut self.state.doc) {
            Some((sel, steps)) => {
                self.touch(&steps);
                self.state.sel = sel;
                self.state.stored_marks = None;
                self.after_change();
                true
            }
            None => false,
        }
    }

    /// The previous / next word position from the caret (for Ctrl+Backspace / Ctrl+Delete).
    pub fn word_targets(&self) -> (Option<usize>, Option<usize>) {
        match self.layout.as_ref() {
            Some(l) => (Some(caret::prev_word_pos(l, self.state.sel.head)), Some(caret::next_word_pos(l, self.state.sel.head))),
            None => (None, None),
        }
    }

    /// The selection as blocks and as plain text (Copy / Cut).
    pub fn selection_payload(&self) -> Option<(Vec<Node>, String)> {
        let (f, t) = (self.state.sel.from(), self.state.sel.to());
        if f == t {
            return None;
        }
        Some((crate::edit::clipboard::slice(&self.state.doc, f, t), crate::edit::clipboard::plain_text(&self.state.doc, f, t)))
    }

    // ── What the ribbon shows ───────────────────────────────────────────────

    pub fn selection_info(&self) -> SelectionInfo {
        let s = &self.state;
        let mut info = SelectionInfo {
            bold: commands::mark_active(s, "bold"),
            italic: commands::mark_active(s, "italic"),
            underline: commands::mark_active(s, "underline"),
            strike: commands::mark_active(s, "strike"),
            subscript: commands::mark_active(s, "subscript"),
            superscript: commands::mark_active(s, "superscript"),
            font_family: commands::uniform_text_style(s, "fontFamily", "Arial"),
            font_size: commands::uniform_text_style(s, "fontSize", "11").map(|v| {
                marks::parse_float(&Value::String(v.clone())).map(|f| format!("{}", f.round() as i64)).unwrap_or(v)
            }),
            has_selection: !s.sel.is_empty(),
            line_height: crate::layout::LH_RATIO,
            align: "left".into(),
            block: "paragraph".into(),
            ..Default::default()
        };
        let typing = commands::typing_marks(&s.doc, s.sel.from(), s.stored_marks.as_ref());
        let tm = marks::extract(&typing);
        info.color = tm.color.clone();
        info.highlight = typing
            .iter()
            .find(|m| m.get("type").and_then(Value::as_str) == Some("highlight"))
            .map(|m| m.get("attrs").and_then(|a| a.get("color")).and_then(Value::as_str).unwrap_or("#fff176").to_string());
        info.link = tm.link;
        info.small_caps = tm.caps;
        if let Some((path, _)) = textblock_at(&s.doc, s.sel.head) {
            if let Some(n) = node_at(&s.doc, &path) {
                let a = n.attrs();
                let num = |k: &str| a.get(k).and_then(marks::parse_float).unwrap_or(0.0);
                info.block = n.node_type().unwrap_or("paragraph").to_string();
                info.level = crate::layout::parse::heading_level(n) as u32;
                info.align = a.get("textAlign").and_then(Value::as_str).unwrap_or("left").to_string();
                info.line_height = a.get("lineHeight").and_then(marks::parse_float).filter(|v| *v > 0.0).unwrap_or(crate::layout::LH_RATIO);
                info.space_before = num("spaceBefore");
                info.space_after = num("spaceAfter");
                info.indent_left = num("indentLeft");
                info.indent_first_line = num("indentFirstLine");
                info.indent_right = num("indentRight");
                info.keep_next = a.get("keepNext").is_some_and(marks::truthy);
                info.keep_lines = a.get("keepLines").is_some_and(marks::truthy);
                info.page_break_before = a.get("pageBreakBefore").is_some_and(marks::truthy);
                info.style_name = a.get("styleName").and_then(Value::as_str).map(str::to_string);
                if let Some(Value::Array(ts)) = a.get("tabStops") {
                    info.tab_stops = ts
                        .iter()
                        .filter_map(|t| match t {
                            Value::Number(n) => n.as_f64().map(|p| (p as f32, "left".to_string())),
                            Value::Object(o) => o.get("pos").and_then(Value::as_f64).map(|p| (p as f32, o.get("type").and_then(Value::as_str).unwrap_or("left").to_string())),
                            _ => None,
                        })
                        .collect();
                }
            }
            for d in (0..path.len()).rev() {
                if let Some(n) = node_at(&s.doc, &path[..d]) {
                    match n.node_type() {
                        Some(t @ ("bulletList" | "orderedList" | "taskList")) if info.list.is_none() => info.list = Some(t.to_string()),
                        Some("tableCell") | Some("tableHeader") => info.in_table = true,
                        _ => {}
                    }
                }
            }
        }
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::commands as cmd;
    use crate::measure::FixedMeasure;

    fn editor(json: &str) -> Editor {
        let mut e = Editor::open(json.as_bytes()).expect("opens");
        e.relayout(&FixedMeasure);
        e
    }

    fn text(e: &Editor) -> String {
        crate::edit::clipboard::plain_text(e.doc(), 0, pm::content_size(e.doc()))
    }

    const DOC: &str = r#"{"content":[{"content":[{"text":"hello world","type":"text"}],"type":"paragraph"},{"content":[{"text":"second","type":"text"}],"type":"paragraph"}],"type":"doc"}"#;


    #[test]
    fn envelope_settings_are_written_and_removed_like_serialize_doc() {
        let mut e = editor(DOC);
        e.set_envelope_value("pageNumbers", Some(serde_json::json!("footer-center"))).expect("set");
        assert!(!e.stored().is_bare());
        assert_eq!(e.set_envelope_value("pageNumbers", Some(serde_json::json!("footer-center"))), Err(EditError::NotApplicable));
        e.relayout(&FixedMeasure);
        let ctx = e.decor_context("T", "d");
        assert_eq!(e.page_decor(0, &ctx, &FixedMeasure).and_then(|d| d.page_number).map(|l| l.text), Some("1".to_string()));
        e.set_envelope_value("pageNumbers", None).expect("removed");
        assert_eq!(e.envelope_value("pageNumbers"), None);
        assert_eq!(e.set_envelope_value("watermark", None), Err(EditError::NotApplicable));

        // Page colour: the base section in the envelope, a later section in its break (undoable).
        e.set_page_color(Some("#fff8e1"), 1000).expect("base");
        assert_eq!(e.envelope_value("pageColor"), Some(serde_json::json!("#fff8e1")));
        e.set_selection(Selection::caret(12));
        e.insert_section_break(2000).expect("break");
        e.set_page_color(Some("#f1f3f4"), 9000).expect("section 1");
        e.relayout(&FixedMeasure);
        let ctx = e.decor_context("T", "d");
        let last = e.pages().len() - 1;
        assert_eq!(e.page_decor(last, &ctx, &FixedMeasure).and_then(|d| d.background), Some("#f1f3f4".to_string()));
        assert_eq!(e.page_decor(0, &ctx, &FixedMeasure).and_then(|d| d.background), Some("#fff8e1".to_string()));
        assert!(e.undo());
        let ctx = e.decor_context("T", "d");
        e.relayout(&FixedMeasure);
        assert_eq!(e.page_decor(e.pages().len() - 1, &ctx, &FixedMeasure).and_then(|d| d.background), Some("#fff8e1".to_string()), "back to the document's colour");
    }


    // ── Notes (`handleInsertFootnote`, `insertEndnote`, the note editor) ──

    #[test]
    fn notes_are_inserted_edited_and_deleted_like_the_web() {
        let mut e = editor(DOC);
        e.set_selection(Selection::caret(6)); // after "hello"
        e.apply(1000, |s| cmd::insert_note(s, "footnote")).expect("inserts");
        let pos = e.selection().head - 1;
        assert_eq!(cmd::note_at(e.doc(), pos), Some(("footnote".to_string(), String::new())));
        e.apply(5000, |s| cmd::set_note_text(s, pos, "La note.")).expect("edits");
        assert_eq!(cmd::note_at(e.doc(), pos).map(|n| n.1).as_deref(), Some("La note."));
        // Stored as the web's node: `{"attrs":{"text":…},"type":"footnote"}`.
        let json = String::from_utf8(e.to_bytes().expect("serialises")).expect("utf-8");
        assert!(json.contains(r#"{"attrs":{"text":"La note."},"type":"footnote"}"#), "{json}");
        e.apply(9000, |s| cmd::delete_note(s, pos)).expect("deletes");
        assert_eq!(cmd::note_at(e.doc(), pos), None);
        assert_eq!(text(&e), "hello world\nsecond");
        assert!(e.undo());
        assert_eq!(cmd::note_at(e.doc(), pos).map(|n| n.0).as_deref(), Some("footnote"));
        assert_eq!(e.apply(9500, |s| cmd::insert_note(s, "comment")), Err(EditError::NotApplicable));
    }

    #[test]
    fn endnotes_are_typeset_once_after_the_last_content() {
        let mut e = editor(r#"{"content":[{"content":[{"text":"un","type":"text"},{"attrs":{"text":"Première note de fin"},"type":"endnote"}],"type":"paragraph"},{"content":[{"text":"deux","type":"text"},{"attrs":{"text":"Seconde"},"type":"endnote"}],"type":"paragraph"}],"type":"doc"}"#);
        e.relayout(&FixedMeasure);
        assert_eq!(e.pages().len(), 1);
        let last = e.pages()[0].paragraphs.last().expect("the notes block");
        assert!(last.lines.iter().all(|l| l.phantom), "painted, never a caret target");
        let texts: Vec<String> = last.lines.iter().map(|l| l.spans.iter().map(|s| &*s.text).collect()).collect();
        assert_eq!(texts[0], "", "the separator rule (an empty underlined span)");
        assert!(last.lines[0].spans[0].marks.underline);
        assert!(texts[1].starts_with("i.Première"), "{texts:?}");
        assert!(texts.iter().any(|t| t.starts_with("ii.Seconde")), "{texts:?}");
        // Below the body with the 12 px gap.
        let body_bottom = e.pages()[0].paragraphs[1].lines.last().map(|l| l.y + l.height).unwrap_or(0.0);
        assert!((last.lines[0].y - (body_bottom + 12.0)).abs() < 0.01);
    }


    #[test]
    fn note_navigation_wraps_around_like_goto_note() {
        let e = editor(r#"{"content":[{"content":[{"text":"a","type":"text"},{"attrs":{"text":"1"},"type":"footnote"},{"text":"b","type":"text"},{"attrs":{"text":"2"},"type":"footnote"}],"type":"paragraph"},{"content":[{"attrs":{"text":"e"},"type":"endnote"}],"type":"paragraph"}],"type":"doc"}"#);
        assert_eq!(cmd::note_positions(e.doc(), "footnote"), [2, 4]);
        assert_eq!(cmd::note_positions(e.doc(), "endnote"), [7]);
        assert_eq!(cmd::next_note(e.doc(), "footnote", 0, true), Some(2));
        assert_eq!(cmd::next_note(e.doc(), "footnote", 4, true), Some(2), "wraps to the first");
        assert_eq!(cmd::next_note(e.doc(), "footnote", 3, false), Some(2));
        assert_eq!(cmd::next_note(e.doc(), "footnote", 1, false), Some(4), "wraps to the last");
        assert_eq!(cmd::note_to_show(e.doc(), "footnote", 5), Some(4));
        assert_eq!(cmd::note_to_show(e.doc(), "footnote", 1), Some(2));
        assert_eq!(cmd::next_note(e.doc(), "comment", 0, true), None);
    }


    #[test]
    fn the_table_of_contents_numbers_the_pages_it_moves() {
        // Two chapters separated by a page break: once the table is inserted at the top, the
        // second pass numbers them with the pages they are on AFTER the insertion.
        let mut e = editor(r#"{"content":[{"content":[{"text":"Chapitre un","type":"text"}],"type":"heading","attrs":{"level":1}},{"type":"pageBreak"},{"content":[{"text":"Chapitre deux","type":"text"}],"type":"heading","attrs":{"level":1}}],"type":"doc"}"#);
        e.set_selection(Selection::caret(1));
        e.generate_toc(&crate::edit::toc::TocSettings::default(), "Table des matières", "Aucun titre", 1000, &FixedMeasure).expect("generated");
        let pages: Vec<_> = e.doc().children().iter().filter_map(|n| n.attrs().get("tocPage").and_then(serde_json::Value::as_i64)).collect();
        assert_eq!(pages, [1, 2]);
        assert!(e.undo(), "one undo step");
        assert!(e.doc().children().iter().all(|n| n.attrs().get("tocKind").is_none()));
        assert!(e.redo());
        let s = e.detect_toc_settings();
        assert_eq!((s.levels, s.show_page_numbers, s.leader.as_str()), (1, true, "dots"));
    }


    #[test]
    fn a_bookmark_marks_the_selection_or_a_zero_width_space() {
        let mut e = editor(DOC);
        e.set_selection(Selection::caret(3));
        e.apply(1000, |s| cmd::set_bookmark(s, "intro")).expect("at the caret");
        let json = String::from_utf8(e.to_bytes().expect("bytes")).expect("utf-8");
        assert!(json.contains("\"marks\":[{\"attrs\":{\"name\":\"intro\"},\"type\":\"bookmark\"}],\"text\":\"\u{200B}\"") || json.contains("\u{200B}"), "{json}");
        e.set_selection(Selection::new(1, 3));
        e.apply(5000, |s| cmd::set_bookmark(s, "deux")).expect("on the selection");
        assert!(String::from_utf8(e.to_bytes().expect("bytes")).expect("utf-8").contains(r#""name":"deux""#));
    }


    #[test]
    fn headings_are_numbered_when_the_document_asks() {
        let mut e = editor(r#"{"content":[{"attrs":{"level":1},"content":[{"text":"A","type":"text"}],"type":"heading"},{"attrs":{"level":2},"content":[{"text":"B","type":"text"}],"type":"heading"},{"attrs":{"level":2},"content":[{"text":"C","type":"text"}],"type":"heading"},{"attrs":{"level":1},"content":[{"text":"D","type":"text"}],"type":"heading"},{"attrs":{"level":3},"content":[{"text":"E","type":"text"}],"type":"heading"}],"type":"doc"}"#);
        let markers = |e: &Editor| -> Vec<String> {
            e.layout().expect("laid out").paragraphs.iter().map(|p| p.lines[0].spans.iter().find(|s| s.is_marker()).map(|s| s.text.to_string()).unwrap_or_default()).collect()
        };
        assert!(markers(&e).iter().all(String::is_empty));
        e.set_envelope_value("headingNumbers", Some(serde_json::json!(true))).expect("on");
        e.relayout(&FixedMeasure);
        assert_eq!(markers(&e), ["1.", "1.1", "1.2", "2.", "2.1.1"]);
        // The text hangs after the number.
        let p = &e.layout().expect("laid out").paragraphs[0];
        let marker_w = FixedMeasure.width("1. ", &crate::marks::TextMark { bold: true, font_size: Some(24.0), ..Default::default() });
        let text = p.lines[0].spans.iter().find(|s| !s.is_marker()).expect("text");
        assert!((text.x - marker_w).abs() < 0.01, "{} vs {marker_w}", text.x);
        e.set_envelope_value("headingNumbers", None).expect("off");
        e.relayout(&FixedMeasure);
        assert!(markers(&e).iter().all(String::is_empty));
    }

    // ── Page setup (`setOrientation`, `handlePaperSize`, `setColumns`, `insertBreak`) ──

    #[test]
    fn landscape_paper_and_columns_are_written_where_the_web_reads_them() {
        let mut e = editor(DOC);
        assert_eq!((e.orientation(0), e.paper_size().as_str(), e.columns()), ("portrait", "a4", 1));
        e.set_orientation("landscape", 1000).expect("base section");
        e.relayout(&FixedMeasure);
        // A4 landscape: `Math.round(29.7 * 96 / 2.54)` × `Math.round(21 * 96 / 2.54)`.
        assert_eq!((e.page_setup(0).width, e.page_setup(0).height), (1123.0, 794.0));
        assert!(!e.stored().is_bare(), "a bare document becomes an envelope to keep its page setup");
        assert_eq!(e.stored().section0().and_then(|s| s["orientation"].as_str().map(str::to_string)).as_deref(), Some("landscape"));
        assert!(!e.can_undo(), "the base section's orientation is not a step of the body (React state on the web)");

        e.set_paper_size("letter").expect("paper");
        e.relayout(&FixedMeasure);
        // Letter landscape: 27.94 × 21.59 cm.
        assert_eq!((e.page_setup(0).width, e.page_setup(0).height), (1056.0, 816.0));
        assert_eq!(e.stored().root_value("paperSize"), Some(serde_json::json!("letter")));
        assert_eq!(e.set_paper_size("letter"), Err(EditError::NotApplicable));
        assert_eq!(e.set_paper_size("tabloid"), Err(EditError::NotApplicable), "only the web's five sizes");

        e.set_columns(2).expect("columns");
        e.relayout(&FixedMeasure);
        assert_eq!(e.stored().section0().and_then(|s| s["columns"].as_u64()), Some(2));
        let s = e.page_setup(0);
        assert_eq!(s.columns, 2);
        // `(contentW - (columns - 1) * COL_GAP) / columns`.
        assert!((s.col_w() - (s.content_w() - COL_GAP) / 2.0).abs() < 1e-3);
        assert_eq!(e.columns(), 2);
        e.set_columns(9).expect("clamped");
        assert_eq!(e.columns(), 3);

        // Written and read back.
        let bytes = e.to_bytes().expect("serialises");
        let back = Editor::open(&bytes).expect("reopens");
        assert_eq!((back.orientation(0), back.paper_size().as_str(), back.columns()), ("landscape", "letter", 3));
    }

    #[test]
    fn a_section_break_carries_the_orientation_and_margins_of_the_section_after_it() {
        let mut e = editor(DOC);
        e.set_selection(Selection::caret(12)); // the end of "hello world"
        e.insert_section_break(1000).expect("inserts");
        let kinds: Vec<_> = e.doc().children().iter().filter_map(|n| n.node_type()).collect();
        assert_eq!(kinds[..2], ["paragraph", "sectionBreak"]);
        let brk = &e.doc().children()[1];
        assert_eq!(brk.attrs().get("orientation"), Some(&serde_json::json!("portrait")));
        assert_eq!(brk.attrs().get("hfLinked"), Some(&serde_json::json!(true)));
        assert_eq!(brk.attrs().get("top"), Some(&serde_json::json!(96)));
        assert_eq!(e.caret_section(), 1, "the caret is in the new section");

        // Landscape for the caret's section only: written into the break, undoable.
        e.set_orientation("landscape", 5000).expect("section 1");
        e.relayout(&FixedMeasure);
        assert_eq!((e.orientation(0), e.orientation(1)), ("portrait", "landscape"));
        assert_eq!((e.page_setup(1).width, e.page_setup(1).height), (1123.0, 794.0));
        assert!(e.stored().is_bare(), "nothing of the envelope changed");
        // Its margins too (the ruler on a page of section 1).
        e.set_margins(50.0, 60.0, 70.0, 80.0, false, 9000);
        e.set_margins(50.0, 60.0, 70.0, 80.0, true, 9000);
        e.relayout(&FixedMeasure);
        let s1 = e.page_setup(1);
        assert_eq!((s1.left, s1.right, s1.top, s1.bottom), (50.0, 60.0, 70.0, 80.0));
        assert_eq!(e.page_setup(0).left, MARGIN, "the base section kept its margins");
        assert!(e.undo());
        e.relayout(&FixedMeasure);
        assert_eq!(e.page_setup(1).left, 96.0, "the margins of section 1 were one undo step");
        assert!(e.undo());
        e.relayout(&FixedMeasure);
        assert_eq!(e.orientation(1), "portrait");
    }

    #[test]
    fn typing_enter_backspace_and_undo() {
        let mut e = editor(DOC);
        e.set_selection(Selection::caret(6));
        e.apply(1000, |s| cmd::insert_text(s, ",")).expect("types");
        e.apply(1100, cmd::enter).expect("splits");
        e.relayout(&FixedMeasure);
        assert_eq!(text(&e), "hello,\n world\nsecond");
        e.apply(1200, |s| cmd::delete_backward(s, false, None)).expect("joins");
        assert_eq!(text(&e), "hello, world\nsecond");
        // All within 500 ms of each other: one undo step.
        assert!(e.undo());
        assert_eq!(text(&e), "hello world\nsecond");
        assert_eq!(e.selection(), Selection::caret(6));
        assert!(e.redo());
        assert_eq!(text(&e), "hello, world\nsecond");
        assert!(e.is_dirty());
    }

    #[test]
    fn enter_at_a_wrap_point_keeps_the_space() {
        let mut e = editor(r#"{"content":[{"content":[{"text":"aaaa bbbb cccc","type":"text"}],"type":"paragraph"}],"type":"doc"}"#);
        e.set_selection(Selection::caret(5));
        e.apply(1000, cmd::enter).expect("splits");
        e.apply(1001, |s| cmd::insert_text(s, "X")).expect("types");
        assert_eq!(text(&e), "aaaa
X bbbb cccc");
    }

    #[test]
    fn the_layout_is_incremental() {
        let mut e = editor(DOC);
        e.set_selection(Selection::caret(3));
        e.apply(1000, |s| cmd::insert_text(s, "x")).expect("types");
        e.relayout(&FixedMeasure);
        assert_eq!(e.last_layout_misses(), 1);
    }

    #[test]
    fn arrows_move_across_blocks_and_keep_the_goal_column() {
        let mut e = editor(DOC);
        e.set_selection(Selection::caret(12));
        e.move_caret(Motion::Right, false, &FixedMeasure);
        assert_eq!(e.selection().head, 14, "end of the first block → start of the second");
        e.move_caret(Motion::Left, false, &FixedMeasure);
        assert_eq!(e.selection().head, 12);
        e.set_selection(Selection::caret(4));
        e.move_caret(Motion::Down, false, &FixedMeasure);
        assert_eq!(e.selection().head, 17);
        e.move_caret(Motion::Up, true, &FixedMeasure);
        assert_eq!(e.selection(), Selection::new(17, 4));
        e.move_caret(Motion::LineEnd, false, &FixedMeasure);
        assert_eq!(e.selection().head, 12);
        e.move_caret(Motion::DocEnd, true, &FixedMeasure);
        assert_eq!(e.selection(), Selection::new(12, 20));
    }

    #[test]
    fn double_and_triple_click_select_word_and_paragraph() {
        let mut e = editor(DOC);
        let layout = e.layout().cloned().expect("laid out");
        let c = caret::pos_to_coords(&layout, 9, false, &FixedMeasure);
        e.press(c.x, c.y + 2.0, 2, false, &FixedMeasure);
        assert_eq!(e.selection(), Selection::new(7, 12));
        e.release();
        e.press(c.x, c.y + 2.0, 3, false, &FixedMeasure);
        assert_eq!(e.selection(), Selection::new(1, 12));
        e.release();
    }

    #[test]
    fn bold_on_a_selection_and_on_a_caret() {
        let mut e = editor(DOC);
        e.set_selection(Selection::new(1, 6));
        e.apply(1000, |s| cmd::toggle_mark(s, "bold")).expect("bold");
        assert!(e.selection_info().bold);
        e.set_selection(Selection::caret(12));
        assert!(!e.selection_info().bold);
        e.apply(1000, |s| cmd::toggle_mark(s, "bold")).expect("stored");
        assert!(e.selection_info().bold);
        e.apply(1001, |s| cmd::insert_text(s, "!")).expect("types");
        let p = &e.doc().children()[0];
        assert!(crate::marks::text_mark_of(p.children().last().expect("a run")).bold);
    }

    #[test]
    fn a_ruler_drag_previews_then_records_one_step() {
        let mut e = editor(DOC);
        e.set_selection(Selection::caret(3));
        for left in [10.0, 20.0, 30.0] {
            e.apply_transient(|s| cmd::set_indents(s, left, 0.0, 0.0)).expect("previews");
        }
        assert_eq!(e.selection_info().indent_left, 30.0);
        assert!(!e.can_undo());
        e.commit_transient(1000, |s| cmd::set_indents(s, 40.0, 0.0, 0.0)).expect("commits");
        assert_eq!(e.selection_info().indent_left, 40.0);
        assert!(e.undo());
        assert_eq!(e.selection_info().indent_left, 0.0, "undo goes back to before the drag");
    }

    #[test]
    fn saving_keeps_the_envelope() {
        let env = r#"{"_type":"multi-page","footer":{"content":[],"type":"doc"},"pages":[{"content":{"content":[{"content":[{"text":"a","type":"text"}],"type":"paragraph"}],"type":"doc"},"id":"p0","sectionId":"s0"}],"sections":[{"id":"s0","margins":{"bottom":96,"left":120,"right":96,"top":96},"orientation":"portrait"}]}"#;
        let mut e = editor(env);
        assert_eq!(e.page_setup(0).left, 120.0);
        e.set_selection(Selection::caret(2));
        e.apply(1000, |s| cmd::insert_text(s, "b")).expect("types");
        let out = String::from_utf8(e.to_bytes().expect("serialises")).expect("utf-8");
        assert!(out.contains(r#""footer":{"content":[],"type":"doc"}"#));
        assert!(out.contains(r#""text":"ab""#));
        assert!(out.contains(r#""id":"p0""#));
    }

    #[test]
    fn tracked_edits_are_saved_and_the_final_view_hides_deleted_text() {
        let mut e = editor(r#"{"content":[{"content":[{"text":"le chat","type":"text"}],"type":"paragraph"}],"type":"doc"}"#);
        e.set_tracking(true).expect("on");
        assert!(e.tracking());
        e.set_track_user(track::TrackUser { id: "u1".into(), name: "Lina".into() });
        e.set_selection(Selection::new(4, 8));
        e.insert_edit(1000, 1_791_364_245_000, |s| cmd::insert_text(s, "chien")).expect("typed over");
        let json = String::from_utf8(e.to_bytes().expect("bytes")).expect("utf-8");
        assert!(json.contains(r#""trackChanges":true"#), "{json}");
        assert!(json.contains(r#""type":"deletion""#) && json.contains(r#""type":"insertion""#), "{json}");
        assert!(json.contains(r#""date":"2026-10-07T09:10:45Z""#), "{json}");
        let line_text = |e: &Editor| e.layout().expect("laid out").paragraphs[0].lines[0].spans.iter().map(|s| s.text.to_string()).collect::<String>();
        e.relayout(&FixedMeasure);
        assert_eq!(line_text(&e), "le chatchien");
        e.set_final_view(true);
        e.relayout(&FixedMeasure);
        assert_eq!(line_text(&e), "le chien");
        e.set_final_view(false);
        e.decide_changes(2000, None, true).expect("accepted");
        assert!(e.changes().is_empty());
        assert!(e.undo(), "a decision is undoable");
        assert_eq!(e.changes().len(), 2);
        e.set_tracking(false).expect("off");
        assert!(!String::from_utf8(e.to_bytes().expect("bytes")).expect("utf-8").contains("trackChanges"));
    }
}
