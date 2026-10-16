//! The open presentation and everything the user does to it - the state of the web's `PresentationEditorPage`
//! without its React: the slides and the active one, the slide list's selection, the canvas editor
//! (`kubuno_office_slides_core::edit::Editor`), the history, the text being edited, the slide and element
//! clipboards. Every change bumps [`Workspace::revision`]; the server session saves what differs from the
//! server's copy (api/session.rs), so the operations here never call the network.

use kubuno_office_slides_core::edit::{Editor, Mods, Outcome};
use kubuno_office_slides_core::history::History;
use kubuno_office_slides_core::insert::{self, IdGen, Ids, Placeholders};
use kubuno_office_slides_core::model::{Deck, Element, Slide, SlideData};
use kubuno_office_slides_core::richtext::{self, Measure};
use kubuno_office_slides_core::textedit::TextEdit;
use serde_json::Value;

/// A table cell being edited (the web's `<input>` over the cell).
#[derive(Debug, Clone, PartialEq)]
pub struct CellEdit {
    pub id: String,
    pub row: usize,
    pub col: usize,
    pub value: String,
    /// Caret, in characters.
    pub caret: usize,
}

/// The open presentation.
pub struct Workspace {
    pub deck: Deck,
    /// The slide shown in the canvas (`activeSlideId`), `None` when the list's selection is empty.
    pub active: Option<String>,
    /// The slide list's selection (`selection`), in click order.
    pub slide_sel: Vec<String>,
    pub editor: Editor,
    pub history: History,
    pub text: Option<TextEdit>,
    pub cell: Option<CellEdit>,
    pub ids: Ids,
    /// Bumped by every change of the deck (the session compares it).
    pub revision: u64,
    /// Copied slides (`slideClipboardRef`).
    pub slide_clipboard: Vec<SlideData>,
    pub placeholders: Placeholders,
    /// « Élément n » in the UI language (SmartArt captions).
    pub item_label: String,
}

impl Workspace {
    pub fn new(deck: Deck, seed: u64) -> Workspace {
        let active = deck.slides.first().map(|s| s.id.clone());
        Workspace {
            slide_sel: active.iter().cloned().collect(),
            active,
            deck,
            editor: Editor::new(),
            history: History::default(),
            text: None,
            cell: None,
            ids: Ids::seeded(seed),
            revision: 0,
            slide_clipboard: Vec::new(),
            placeholders: Placeholders::default(),
            item_label: "Élément {n}".into(),
        }
    }

    fn bump(&mut self) {
        self.revision += 1;
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active.as_ref().and_then(|id| self.deck.index_of(id))
    }

    pub fn active_slide(&self) -> Option<&Slide> {
        self.active.as_ref().and_then(|id| self.deck.slide(id))
    }

    pub fn active_elements(&self) -> &[Element] {
        self.active_slide().map(|s| s.data.elements.as_slice()).unwrap_or(&[])
    }

    /// Runs an edit of the active slide's elements; a change records the previous elements (the web's
    /// `handleElementsChange`) and bumps the revision.
    pub fn edit(&mut self, now_ms: u64, f: impl FnOnce(&mut Vec<Element>, &mut Editor, &mut Ids) -> Outcome) -> Outcome {
        let Some(id) = self.active.clone() else { return Outcome::default() };
        let Some(slide) = self.deck.slides.iter_mut().find(|s| s.id == id) else { return Outcome::default() };
        let before = slide.data.elements.clone();
        let out = f(&mut slide.data.elements, &mut self.editor, &mut self.ids);
        if out.changed && slide.data.elements != before {
            self.history.record(&id, &before, now_ms);
            self.revision += 1;
        }
        out
    }

    /// Replaces the active slide's elements (`commitElements` + history).
    pub fn set_elements(&mut self, now_ms: u64, els: Vec<Element>) {
        self.edit(now_ms, |e, _, _| {
            *e = els;
            Outcome { changed: true, repaint: true, tool_reset: false }
        });
    }

    // ── Undo / redo (`undo`, `redo`) ────────────────────────────────────────

    pub fn undo(&mut self) -> bool {
        let deck = &self.deck;
        let Some(entry) = self.history.undo(|sid| deck.slide(sid).map(|s| s.data.elements.clone()).unwrap_or_default()) else { return false };
        self.apply_entry(entry.slide, entry.elements);
        true
    }

    pub fn redo(&mut self) -> bool {
        let deck = &self.deck;
        let Some(entry) = self.history.redo(|sid| deck.slide(sid).map(|s| s.data.elements.clone()).unwrap_or_default()) else { return false };
        self.apply_entry(entry.slide, entry.elements);
        true
    }

    fn apply_entry(&mut self, sid: String, els: Vec<Element>) {
        self.end_text();
        if self.active.as_deref() != Some(sid.as_str()) {
            self.active = Some(sid.clone());
            self.slide_sel = vec![sid.clone()];
            self.editor.reset_for_slide();
        }
        if let Some(s) = self.deck.slide_mut(&sid) {
            s.data.elements = els;
        }
        let valid: Vec<String> = self.active_elements().iter().map(|e| e.id().to_string()).collect();
        self.editor.selection.retain(|i| valid.contains(i));
        self.bump();
    }

    // ── The slide list (`SlidePanel`) ───────────────────────────────────────

    /// `selectSlide`: a click (Ctrl toggles, Shift extends from the active slide).
    pub fn select_slide(&mut self, sid: &str, ctrl: bool, shift: bool) {
        let next: Vec<String> = if ctrl {
            if self.slide_sel.iter().any(|s| s == sid) {
                self.slide_sel.iter().filter(|s| *s != sid).cloned().collect()
            } else {
                let mut n = self.slide_sel.clone();
                n.push(sid.to_string());
                n
            }
        } else if shift && self.active.is_some() {
            let a = self.active_index();
            let b = self.deck.index_of(sid);
            match (a, b) {
                (Some(a), Some(b)) => self.deck.slides[a.min(b)..=a.max(b)].iter().map(|s| s.id.clone()).collect(),
                _ => vec![sid.to_string()],
            }
        } else {
            vec![sid.to_string()]
        };
        let active = if next.iter().any(|s| s == sid) { Some(sid.to_string()) } else { next.last().cloned() };
        self.slide_sel = next;
        self.set_active(active);
    }

    /// Clicking the list's empty space: nothing selected, no slide shown.
    pub fn clear_slide_selection(&mut self) {
        self.slide_sel.clear();
        self.set_active(None);
    }

    fn set_active(&mut self, id: Option<String>) {
        if self.active != id {
            self.end_text();
            self.editor.reset_for_slide();
            self.active = id;
        }
    }

    /// The list's selection in presentation order (`orderedSelection`).
    pub fn ordered_selection(&self) -> Vec<String> {
        self.deck.slides.iter().filter(|s| self.slide_sel.contains(&s.id)).map(|s| s.id.clone()).collect()
    }

    /// A new slide id (the server honours client-minted ids).
    fn new_slide_id(&mut self) -> String {
        kubuno_office_documents_common::api::client::fresh_uuid_v4()
    }

    /// « Nouvelle diapositive » after `after` (else at the end): seeded with the title and subtitle
    /// placeholders, made active.
    pub fn new_slide_after(&mut self, after: Option<&str>) -> String {
        let pos = after.and_then(|a| self.deck.index_of(a)).map(|i| i + 1).unwrap_or(self.deck.slides.len());
        let id = self.new_slide_id();
        let mut data = SlideData::empty();
        data.elements = insert::placeholder_slide_elements(&mut self.ids, &self.placeholders);
        self.deck.slides.insert(pos, Slide { id: id.clone(), hidden: false, data });
        self.slide_sel = vec![id.clone()];
        self.set_active(Some(id.clone()));
        self.bump();
        id
    }

    /// « Dupliquer » the selected slides (each right after itself, like the server's duplicate).
    pub fn duplicate_selected(&mut self) {
        let ids = self.ordered_selection();
        let mut last = None;
        for sid in ids {
            let Some(i) = self.deck.index_of(&sid) else { continue };
            let copy = Slide { id: self.new_slide_id(), hidden: self.deck.slides[i].hidden, data: self.deck.slides[i].data.clone() };
            last = Some(copy.id.clone());
            self.deck.slides.insert(i + 1, copy);
        }
        if let Some(l) = last {
            self.slide_sel = vec![l.clone()];
            self.set_active(Some(l));
            self.bump();
        }
    }

    /// « Supprimer » the selected slides; the next one (or the previous) becomes active.
    pub fn delete_selected(&mut self) {
        let ids = self.ordered_selection();
        if ids.is_empty() {
            return;
        }
        let first = self.deck.slides.iter().position(|s| ids.contains(&s.id)).unwrap_or(0);
        self.deck.slides.retain(|s| !ids.contains(&s.id));
        for id in &ids {
            self.history.forget(id);
        }
        let next = self.deck.slides.get(first.min(self.deck.slides.len().saturating_sub(1))).map(|s| s.id.clone());
        self.slide_sel = next.iter().cloned().collect();
        self.set_active(next);
        self.bump();
    }

    /// « Copier » the selected slides (elements, background, notes, transition).
    pub fn copy_selected(&mut self) {
        let ids = self.ordered_selection();
        self.slide_clipboard = ids.iter().filter_map(|i| self.deck.slide(i)).map(|s| s.data.clone()).collect();
    }

    pub fn cut_selected(&mut self) {
        self.copy_selected();
        self.delete_selected();
    }

    /// « Coller » after `after` (else at the end); the last pasted slide becomes active.
    pub fn paste_slides_after(&mut self, after: Option<&str>) {
        if self.slide_clipboard.is_empty() {
            return;
        }
        let pos = after.and_then(|a| self.deck.index_of(a)).map(|i| i + 1).unwrap_or(self.deck.slides.len());
        let mut last = None;
        for (k, data) in self.slide_clipboard.clone().into_iter().enumerate() {
            let id = self.new_slide_id();
            self.deck.slides.insert(pos + k, Slide { id: id.clone(), hidden: false, data });
            last = Some(id);
        }
        if let Some(l) = last {
            self.slide_sel = vec![l.clone()];
            self.set_active(Some(l));
            self.bump();
        }
    }

    /// « Afficher / Masquer »: hides the selection if any of it is visible, else shows it.
    pub fn toggle_hidden_selected(&mut self) {
        let ids = self.ordered_selection();
        let any_visible = self.deck.slides.iter().any(|s| ids.contains(&s.id) && !s.hidden);
        for s in self.deck.slides.iter_mut().filter(|s| ids.contains(&s.id)) {
            s.hidden = any_visible;
        }
        if !ids.is_empty() {
            self.bump();
        }
    }

    /// Moves slide `from` to `to` (`handleReorderSlide`).
    pub fn reorder(&mut self, from: usize, to: usize) {
        if from >= self.deck.slides.len() || from == to {
            return;
        }
        let s = self.deck.slides.remove(from);
        let to = to.min(self.deck.slides.len());
        self.deck.slides.insert(to, s);
        self.bump();
    }

    // ── Slide-level settings (not in the history, like the web) ────────────

    pub fn set_notes(&mut self, notes: &str) {
        let Some(id) = self.active.clone() else { return };
        if let Some(s) = self.deck.slide_mut(&id) {
            if s.data.notes() != notes {
                s.data.set_notes(notes);
                self.bump();
            }
        }
    }

    /// The active slide's background (`setSlideBg`), JSON text in the web's key order.
    pub fn set_background(&mut self, json: &str) {
        let Some(id) = self.active.clone() else { return };
        if let Some(s) = self.deck.slide_mut(&id) {
            s.data.set_raw("background", json);
            self.bump();
        }
    }

    /// « Appliquer à toutes les diapos ».
    pub fn apply_background_to_all(&mut self) {
        let Some(bg) = self.active_slide().and_then(|s| s.data.json_of("background")) else { return };
        for s in &mut self.deck.slides {
            s.data.set_raw("background", &bg);
        }
        self.bump();
    }

    /// `setSlideTransition`: the type, the duration kept (else 500 ms).
    pub fn set_transition(&mut self, kind: &str, duration: Option<f64>) {
        let Some(id) = self.active.clone() else { return };
        if let Some(s) = self.deck.slide_mut(&id) {
            let d = duration.or(s.data.transition().1).unwrap_or(500.0);
            let json = format!(r#"{{"type":{},"duration":{}}}"#, kubuno_office_slides_core::obj::quote(kind), kubuno_office_slides_core::model::num(d));
            s.data.set_raw("transition", &json);
            self.bump();
        }
    }

    /// `applyLayout`: the layout's placeholders replace the slide's elements (undoable).
    pub fn apply_layout(&mut self, now_ms: u64, layout: &str) {
        let ph = self.placeholders.clone();
        self.end_text();
        self.edit(now_ms, |els, ed, ids| {
            *els = insert::layout_elements(layout, ids, &ph);
            ed.selection.clear();
            Outcome { changed: true, repaint: true, tool_reset: false }
        });
    }

    // ── Insertions of the toolbar and the ribbon ────────────────────────────

    /// Adds elements on top of the active slide and selects them.
    pub fn insert(&mut self, now_ms: u64, build: impl FnOnce(&mut dyn IdGen, usize) -> Vec<Element>) {
        self.end_text();
        self.edit(now_ms, |els, ed, ids| {
            let news = build(ids, els.len() + 1);
            ed.selection = news.iter().map(|e| e.id().to_string()).collect();
            els.extend(news);
            Outcome { changed: true, repaint: true, tool_reset: false }
        });
    }

    // ── Text editing in place ───────────────────────────────────────────────

    /// Double click on a text box or a shape: edit its text (caret at the end).
    pub fn begin_text(&mut self, id: &str) -> bool {
        let Some(el) = self.active_elements().iter().find(|e| e.id() == id && (e.kind() == "text" || e.kind() == "shape")) else { return false };
        let paras = richtext::doc_to_paras(el.get("content"));
        self.text = Some(TextEdit::open(id, paras));
        self.editor.selection = vec![id.to_string()];
        true
    }

    /// Ends the text editing (blur, Escape).
    pub fn end_text(&mut self) {
        self.text = None;
    }

    /// Runs a text operation and writes the text back to its element (undoable, coalesced by time).
    pub fn text_op(&mut self, now_ms: u64, f: impl FnOnce(&mut TextEdit)) -> bool {
        let Some(mut te) = self.text.take() else { return false };
        f(&mut te);
        let snapshot = te.clone();
        self.text = Some(te);
        let out = self.edit(now_ms, |els, _, _| Outcome { changed: snapshot.write_to(els), repaint: true, tool_reset: false });
        out.changed
    }

    /// A table cell's editor (double click on a cell).
    pub fn begin_cell(&mut self, id: &str, row: usize, col: usize) {
        let value = self.active_elements().iter().find(|e| e.id() == id).map(|e| kubuno_office_slides_core::table::cell_text(e, row, col)).unwrap_or_default();
        let caret = value.chars().count();
        self.cell = Some(CellEdit { id: id.to_string(), row, col, value, caret });
        self.editor.selection = vec![id.to_string()];
    }

    /// `commitCell`: the cell's text written to the table.
    pub fn commit_cell(&mut self, now_ms: u64) {
        let Some(c) = self.cell.take() else { return };
        self.edit(now_ms, |els, _, _| {
            let Some(e) = els.iter_mut().find(|e| e.id() == c.id) else { return Outcome::default() };
            let mut patch = serde_json::Map::new();
            patch.insert("text".into(), Value::String(c.value.clone()));
            let cells = kubuno_office_slides_core::table::set_cell(e, c.row, c.col, &patch);
            e.set("cells", cells);
            Outcome { changed: true, repaint: true, tool_reset: false }
        });
    }

    // ── Canvas ──────────────────────────────────────────────────────────────

    /// A key the canvas handles (Delete, Ctrl+D, arrows…), the element clipboard included.
    pub fn canvas_key(&mut self, now_ms: u64, key: &str, mods: Mods, scale: f64) -> Option<Outcome> {
        let id = self.active.clone()?;
        let slide = self.deck.slides.iter_mut().find(|s| s.id == id)?;
        let before = slide.data.elements.clone();
        let out = self.editor.key(&mut slide.data.elements, key, mods, &mut self.ids, scale)?;
        if out.changed && slide.data.elements != before {
            self.history.record(&id, &before, now_ms);
            self.revision += 1;
        }
        Some(out)
    }

    /// The measured text height for « Redimensionner la forme pour l'adapter au texte ».
    pub fn autofit(&mut self, now_ms: u64, mode: &str, m: &dyn Measure) {
        let family = self.deck.theme.font_family.clone();
        self.edit(now_ms, |els, ed, _| ed.set_autofit(els, mode, |e| kubuno_office_slides_core::edit::measured_text_height(e, &family, m)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(n: usize) -> Deck {
        let ids: Vec<Value> = (0..n).map(|i| serde_json::json!({ "id": format!("s{i}"), "position": i })).collect();
        Deck::from_server(&serde_json::json!({ "presentation": { "title": "T" }, "slides": ids }), r#"{"version":1,"slides":{}}"#)
    }

    #[test]
    fn selection_follows_the_web() {
        let mut w = Workspace::new(deck(4), 1);
        assert_eq!(w.active.as_deref(), Some("s0"));
        w.select_slide("s2", false, true);
        assert_eq!(w.slide_sel, ["s0", "s1", "s2"]);
        assert_eq!(w.active.as_deref(), Some("s2"));
        w.select_slide("s1", true, false);
        assert_eq!(w.slide_sel, ["s0", "s2"]);
        assert_eq!(w.active.as_deref(), Some("s2"), "a toggled-off slide hands over to the last selected");
        w.clear_slide_selection();
        assert!(w.active.is_none());
    }

    #[test]
    fn slides_are_added_duplicated_hidden_and_deleted() {
        let mut w = Workspace::new(deck(2), 1);
        let id = w.new_slide_after(Some("s0"));
        assert_eq!(w.deck.slides[1].id, id);
        assert_eq!(w.deck.slides[1].data.elements.len(), 2, "title + subtitle placeholders");
        w.select_slide("s0", false, false);
        w.duplicate_selected();
        assert_eq!(w.deck.slides.len(), 4);
        assert_eq!(w.deck.slides[1].data, w.deck.slides[0].data);
        w.toggle_hidden_selected();
        assert!(w.deck.slides[1].hidden);
        w.delete_selected();
        assert_eq!(w.deck.slides.len(), 3);
        assert_eq!(w.active.as_deref(), Some(id.as_str()), "the next slide takes over");
    }

    #[test]
    fn element_edits_undo_across_slides() {
        let mut w = Workspace::new(deck(2), 1);
        w.insert(0, |ids, z| vec![insert::chart(ids, "column", z)]);
        assert_eq!(w.active_elements().len(), 1);
        w.select_slide("s1", false, false);
        assert!(w.undo());
        assert_eq!(w.active.as_deref(), Some("s0"), "undo goes back to the slide it changes");
        assert!(w.active_elements().is_empty());
        assert!(w.redo());
        assert_eq!(w.active_elements().len(), 1);
    }

    #[test]
    fn typing_writes_the_doc_and_is_one_step() {
        let mut w = Workspace::new(deck(1), 1);
        w.insert(0, |ids, z| vec![insert::text_tool_box(ids, 0.1, 0.1, z, "Texte")]);
        let id = w.active_elements()[0].id().to_string();
        assert!(w.begin_text(&id));
        w.text_op(1000, |t| t.insert_text(" A"));
        w.text_op(1100, |t| t.insert_text("B"));
        let content = w.active_elements()[0].0.slot("content").map(|s| s.text()).unwrap_or_default();
        assert_eq!(content, r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Texte AB"}]}]}"#);
        assert!(w.undo());
        assert_eq!(richtext::paras_to_plain(&richtext::doc_to_paras(w.active_elements()[0].get("content"))), "Texte");
    }

    #[test]
    fn transitions_and_backgrounds_are_written_like_the_web() {
        let mut w = Workspace::new(deck(2), 1);
        w.set_transition("fade", None);
        assert_eq!(w.active_slide().map(|s| s.data.transition()), Some(("fade".to_string(), Some(0.3))), "the server's empty slide has 0.3: kept");
        w.set_background(r##"{"type":"color","color":"#000000"}"##);
        w.apply_background_to_all();
        assert!(w.deck.slides.iter().all(|s| s.data.json_of("background").as_deref() == Some(r##"{"type":"color","color":"#000000"}"##)));
    }
}
