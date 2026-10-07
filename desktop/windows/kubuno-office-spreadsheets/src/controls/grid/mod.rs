//! `GridCanvas` — the sheet: the cells of the open workbook's active sheet with their headers, painted like
//! the web's canvas grid, and the input of `SpreadsheetApp.tsx` turned into the editor's calls.
//!
//! * the pointer (`handleGridMouseDown`, `SA:5812`): a cell selects (Shift+click extends, a drag extends
//!   with auto-scroll near the edges), a header selects its column or row (a drag extends; near its edge
//!   resizes, a double click autofits a column), the corner selects everything, the fill handle drags a
//!   fill, a checkbox cell toggles, an outline button folds its group; while a formula is edited, a click
//!   where an operand is expected inserts the reference (point mode);
//! * the keys (`SA:4366-4463`): arrows (Ctrl: to the edge of the data), Tab, Enter and F2 edit, Delete
//!   clears, a printable character starts editing; Shift+arrows, PageUp/PageDown and Home are desktop
//!   conventions the web does not have;
//! * the in-cell editor (`CellEditor`): the text over the cell in the cell's font, coloured like the web's
//!   `FormulaInput`, with the function autocomplete, F4, bracket pairing; Enter/Tab/arrows commit and move,
//!   Escape cancels.
//!
//! The window's commands arrive as calls through [`GridCanvas::run`]; what the window shows (formula bar,
//! Name Box, status bar, ribbon states, tabs) leaves as `StateChanged`.

pub mod equations;
pub mod icons;
pub mod object_input;
pub mod objects;
pub mod paint;
pub mod presence;
pub mod prims;
pub mod shapes;

use std::collections::HashMap;

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::range::{ScrollBar, ScrollPart};
use kubuno_desktop::ui::{Canvas, Rect, Size, Widget, WidgetState};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, Keys, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_sheets_core::addr::{self, Cell as Pos, MAX_COLS, MAX_ROWS};
use kubuno_office_sheets_core::editor::commands::{fill_bounds, Refused};
use kubuno_office_sheets_core::editor::{Dir, Editor};
use kubuno_office_sheets_core::grid::{COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};
use kubuno_office_sheets_core::refs;
use kubuno_office_spreadsheets_common::model::line_edit::{LineEdit, Suggestion};
use kubuno_office_spreadsheets_common::model::state::{AggLabels, GridState};
use kubuno_office_spreadsheets_common::model::view::GridView;

use self::paint::{GridPainter, Overlay};

/// One wheel notch, in DIP (Chromium's 100 px).
pub const WHEEL_STEP: f64 = 100.0;
/// The resize grab zone around a header edge (`RESIZE_GRAB`).
const RESIZE_GRAB: f64 = 4.0;
/// The fill handle's hit zone around the selection's corner.
const HANDLE_GRAB: f64 = 6.0;

/// What the window shows of the workbook (raised as `StateChanged` when it differs).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct GridStateEventArgs {
    pub state: GridState,
    /// The cell is being edited (the formula bar mirrors the draft).
    pub editing: bool,
    /// The draft being edited (the formula bar's text while editing).
    pub draft: String,
    /// How many floating objects are selected, and their single kind (`chart`, `image`, `shape`, `equation`;
    /// empty when none or mixed).
    pub objects: usize,
    pub object_kind: String,
    /// A shape is armed in the gallery (the next drag draws it).
    pub armed: bool,
}

/// A floating object's menu or editor is asked for (right click, double click).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct ObjectEventArgs {
    /// Where, in the control (DIP).
    pub x: f32,
    pub y: f32,
    /// The object's kind (`chart`, `image`, `shape`, `equation`) and id (a picture's index).
    pub kind: String,
    pub id: String,
}

/// A message for the window to show (a refused value, a protected sheet).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct GridMessageEventArgs {
    pub message: String,
    /// `protected`, `invalid`.
    pub kind: String,
}

/// The pointer over the grid, in content coordinates (viewport + scroll, as the web publishes it for the
/// collaborators, `SA:668-672`); `inside == false` when it left the grid.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct GridPointerEventArgs {
    pub x: f64,
    pub y: f64,
    pub inside: bool,
}

/// The cell being edited.
#[derive(Clone, Debug)]
struct CellEdit {
    at: Pos,
    le: LineEdit,
    /// The draft is typed in the formula bar (the grid only shows it).
    from_bar: bool,
    suggestions: Vec<Suggestion>,
    sug: usize,
}

/// A pointer gesture in progress.
#[derive(Clone, Debug, PartialEq)]
enum Drag {
    None,
    Select,
    Cols(u32),
    Rows(u32),
    Fill { src: addr::Rect, to: Pos },
    ResizeCol { c: u32, x0: f64, w0: f64 },
    ResizeRow { r: u32, y0: f64, h0: f64 },
    Point { start: Pos },
    VScroll(f32),
    HScroll(f32),
}

/// The sheet (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Spreadsheets")]
#[toolbox(icon = "table")]
#[default_event("StateChanged")]
pub struct GridCanvas {
    base: ControlCore,
    /// Occurs when what the window shows of the workbook changes.
    #[event]
    #[category("Action")]
    pub state_changed: Event<GridStateEventArgs>,
    /// Occurs when an edit is refused (validation, protection).
    #[event]
    #[category("Action")]
    pub message: Event<GridMessageEventArgs>,
    /// Occurs when an equation is double-clicked (the window opens the equation dialog).
    #[event]
    #[category("Action")]
    pub equation_edit: Event<equations::EquationEventArgs>,
    /// Occurs when the pointer moves over the grid or leaves it.
    #[event]
    #[category("Mouse")]
    pub pointer_moved: Event<GridPointerEventArgs>,
    /// Occurs on a right click on a floating object (the window opens the object menu).
    #[event]
    #[category("Action")]
    pub object_menu: Event<ObjectEventArgs>,
    /// Occurs on a double click on a floating object (chart: the wizard; shape: its text).
    #[event]
    #[category("Action")]
    pub object_open: Event<ObjectEventArgs>,
    ed: Option<Editor>,
    /// The floating objects' selection, gestures and drawings.
    pub objects: objects::ObjectLayer,
    view: GridView,
    bounds: Rect,
    scale: f32,
    edit: Option<CellEdit>,
    drag: Drag,
    drag_at: Option<(f64, f64)>,
    has_focus: bool,
    caret_moved_ms: u64,
    reported: Option<GridStateEventArgs>,
    reveal: bool,
    labels: AggLabels,
    vhot: bool,
    hhot: bool,
    eq: equations::EquationLayer,
    /// The collaborators shown on the grid (`presence.rs`).
    peers: Vec<presence::PeerMark>,
}

impl Default for GridCanvas {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            state_changed: Event::default(),
            message: Event::default(),
            equation_edit: Event::default(),
            pointer_moved: Event::default(),
            object_menu: Event::default(),
            object_open: Event::default(),
            ed: None,
            objects: objects::ObjectLayer::default(),
            view: GridView::default(),
            bounds: Rect::default(),
            scale: 1.0,
            edit: None,
            drag: Drag::None,
            drag_at: None,
            has_focus: false,
            caret_moved_ms: 0,
            reported: None,
            reveal: false,
            labels: AggLabels::default(),
            vhot: false,
            hhot: false,
            eq: equations::EquationLayer::default(),
            peers: Vec::new(),
        }
    }
}

fn now() -> u64 {
    kubuno_desktop::controls::host::now_ms()
}

impl GridCanvas {
    /// Opens a workbook.
    pub fn open(&mut self, ed: Editor) {
        self.ed = Some(ed);
        self.edit = None;
        self.view.scroll_x = 0.0;
        self.view.scroll_y = 0.0;
        self.reported = None;
        self.invalidate();
    }

    /// The open workbook's editor.
    pub fn editor(&self) -> Option<&Editor> {
        self.ed.as_ref()
    }

    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        self.ed.as_mut()
    }

    /// The labels of the status bar's aggregates (the UI culture's).
    pub fn set_labels(&mut self, labels: AggLabels) {
        self.labels = labels;
        self.reported = None;
    }

    /// Runs an editor command; a refusal is raised as `Message`. The active cell is kept in view.
    pub fn run<R>(&mut self, f: impl FnOnce(&mut Editor) -> Result<R, Refused>) -> Option<R> {
        self.commit_edit_quietly();
        let ed = self.ed.as_mut()?;
        let r = f(ed);
        self.reveal = true;
        self.invalidate();
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.refused(e);
                None
            }
        }
    }

    /// Runs a closure on the editor that cannot be refused (selection, zoom…).
    pub fn with_editor<R>(&mut self, f: impl FnOnce(&mut Editor) -> R) -> Option<R> {
        let ed = self.ed.as_mut()?;
        let r = f(ed);
        self.reveal = true;
        self.invalidate();
        Some(r)
    }

    fn refused(&mut self, e: Refused) {
        let (message, kind) = match e {
            Refused::Protected => (String::new(), "protected"),
            Refused::Invalid(help) => (help, "invalid"),
        };
        self.raise_message(GridMessageEventArgs { message, kind: kind.into() });
    }

    // ── Editing ─────────────────────────────────────────────────────────────

    pub fn is_editing(&self) -> bool {
        self.edit.is_some()
    }

    /// `startEditCell(col, row, initial)`: the draft is `initial`, else the cell's formula or text.
    pub fn begin_edit(&mut self, initial: Option<&str>) {
        let Some(ed) = self.ed.as_ref() else { return };
        if !ed.selected {
            return;
        }
        if ed.sheet().is_protected() {
            self.refused(Refused::Protected);
            return;
        }
        let at = ed.sel.active;
        let text = match initial {
            Some(t) => t.to_string(),
            None => ed.formula_bar_text(),
        };
        let mut le = LineEdit::new(&text);
        le.end(false);
        self.edit = Some(CellEdit { at, le, from_bar: false, suggestions: Vec::new(), sug: 0 });
        self.refresh_assist();
        self.caret_moved_ms = now();
        self.invalidate();
    }

    /// The formula bar's text changed: the draft follows it.
    pub fn bar_text_changed(&mut self, text: &str) {
        let Some(ed) = self.ed.as_ref() else { return };
        let at = ed.sel.active;
        match &mut self.edit {
            Some(e) => {
                if e.le.text() != text {
                    e.le.set_text(text);
                }
                e.from_bar = true;
            }
            None => {
                if ed.formula_bar_text() == text {
                    return;
                }
                self.edit = Some(CellEdit { at, le: LineEdit::new(text), from_bar: true, suggestions: Vec::new(), sug: 0 });
            }
        }
        self.invalidate();
    }

    /// Commits the draft (`handleEditCommit`), then moves.
    pub fn commit_edit(&mut self, then: Option<Dir>) {
        let Some(e) = self.edit.take() else { return };
        let text = e.le.text();
        let Some(ed) = self.ed.as_mut() else { return };
        let r = ed.update_cell(e.at, &text);
        if let Err(err) = r {
            // A refused value keeps the editor open with the draft, as the web's alert does.
            self.edit = Some(e);
            self.refused(err);
            return;
        }
        if let Some(d) = then {
            ed.move_selection(d);
        }
        self.reveal = true;
        self.invalidate();
    }

    fn commit_edit_quietly(&mut self) {
        if self.edit.is_some() {
            self.commit_edit(None);
        }
    }

    /// Escape (`handleEditAbort`).
    pub fn cancel_edit(&mut self) {
        self.edit = None;
        self.invalidate();
    }

    fn refresh_assist(&mut self) {
        let names: Vec<String> = self.ed.as_ref().map(|e| e.wb.names.keys().cloned().collect()).unwrap_or_default();
        if let Some(e) = &mut self.edit {
            e.suggestions = e.le.suggestions(&names);
            e.sug = 0;
        }
    }

    // ── Geometry helpers ────────────────────────────────────────────────────

    fn viewport(&self) -> Rect {
        self.bounds
    }

    fn sync_view_size(&mut self) {
        self.view.width = (self.bounds.right - self.bounds.left) as f64;
        self.view.height = (self.bounds.bottom - self.bounds.top) as f64;
    }

    fn vbar(&self) -> Option<(ScrollBar, Rect)> {
        let ed = self.ed.as_ref()?;
        let g = ed.geometry();
        let (_, ch) = self.view.content_size(ed, &g);
        let vp = self.viewport();
        let inner = Rect::new(vp.left, vp.top + COL_HEADER_HEIGHT as f32, vp.right, vp.bottom);
        let bar = ScrollBar::from_content(false, ch as f32, self.view.height as f32, self.view.scroll_y as f32)?.with_expanded(self.vhot);
        let rail = bar.rail(&inner);
        Some((bar, rail))
    }

    fn hbar(&self) -> Option<(ScrollBar, Rect)> {
        let ed = self.ed.as_ref()?;
        let g = ed.geometry();
        let (cw, _) = self.view.content_size(ed, &g);
        let vp = self.viewport();
        let inner = Rect::new(vp.left + ROW_HEADER_WIDTH as f32, vp.top, vp.right, vp.bottom);
        let bar = ScrollBar::from_content(true, cw as f32, self.view.width as f32, self.view.scroll_x as f32)?.with_expanded(self.hhot);
        let rail = bar.rail(&inner);
        Some((bar, rail))
    }

    fn reveal_active(&mut self) {
        let Some(ed) = self.ed.as_ref() else { return };
        let g = ed.geometry();
        let a = ed.sel.end.unwrap_or(ed.sel.active);
        let a = if self.drag == Drag::None { ed.sel.active } else { a };
        self.view.scroll_into_view(ed, &g, a.col.min(MAX_COLS - 1), a.row.clamp(1, MAX_ROWS));
    }

    fn scroll(&mut self, dx: f64, dy: f64) {
        let Some(ed) = self.ed.as_ref() else { return };
        let g = ed.geometry();
        self.view.scroll_by(dx, dy, ed, &g);
        self.invalidate();
    }

    /// The edited cell's rectangle in viewport coordinates.
    fn edit_rect(&self) -> Option<(f64, f64, f64, f64)> {
        let e = self.edit.as_ref()?;
        let ed = self.ed.as_ref()?;
        let g = ed.geometry();
        let m = ed.sheet().merges().into_iter().find(|m| m.top_left() == e.at);
        let (c2, r2) = m.map(|m| (m.c2, m.r2)).unwrap_or((e.at.col, e.at.row));
        let x = self.view.screen_x(ed, &g, e.at.col);
        let y = self.view.screen_y(ed, &g, e.at.row);
        Some((x, y, g.col_left(c2 + 1) - g.col_left(e.at.col), g.row_top(r2 + 1) - g.row_top(e.at.row)))
    }

    fn state_args(&self) -> Option<GridStateEventArgs> {
        let ed = self.ed.as_ref()?;
        let mut state = GridState::of(ed, &self.labels);
        if let Some(e) = &self.edit {
            state.formula = e.le.text();
        }
        let sel = &self.objects.sel;
        if let Some(label) = objects::selection_label(ed, sel, &obj_words()) {
            state.name_box = label;
            state.formula = String::new();
        }
        let kind = match kubuno_office_sheets_core::objects::sole_kind(sel) {
            Some(k) => k.key().trim_end_matches('s').to_string(),
            None => String::new(),
        };
        Some(GridStateEventArgs { state, editing: self.edit.is_some(), draft: self.edit.as_ref().map(|e| e.le.text()).unwrap_or_default(), objects: sel.len(), object_kind: kind, armed: self.objects.armed.is_some() })
    }

    /// The references of the formula being edited, coloured (`parseRefs` → the boxes of `SA:2821`).
    fn edit_refs(&self) -> Vec<(addr::Rect, String)> {
        let Some(e) = &self.edit else { return Vec::new() };
        let text = e.le.text();
        if !text.starts_with('=') {
            return Vec::new();
        }
        let Some(ed) = self.ed.as_ref() else { return Vec::new() };
        let (uc, ur) = ed.sheet().used_extent();
        refs::parse_refs(&text)
            .into_iter()
            .filter(|r| !r.text.contains('!'))
            .filter_map(|r| {
                let b = refs::ref_bounds(&r.text)?;
                let r2 = if b.r2 >= MAX_ROWS as i64 { (ur + 1) as i64 } else { b.r2 };
                let c2 = if b.c2 >= MAX_COLS as i64 - 1 { (uc + 1) as i64 } else { b.c2 };
                Some((addr::Rect::new(b.c1.max(0) as u32, b.r1.max(1) as u32, c2.max(0) as u32, r2.max(1) as u32), r.color.to_string()))
            })
            .collect()
    }

    // ── Painting the editor ─────────────────────────────────────────────────

    fn paint_editor(&self, g: &kubuno_desktop::ui::graphics::Graphics<'_>) {
        let (Some(e), Some((x, y, w, h)), Some(ed)) = (&self.edit, self.edit_rect(), self.ed.as_ref()) else { return };
        let (ox, oy) = (self.bounds.left, self.bounds.top);
        let r = Rect::new(ox + x as f32, oy + y as f32, ox + (x + w) as f32, oy + (y + h) as f32);
        g.fill_rectangle(Brush::solid(Color::WHITE), r);
        let st = ed.style_at(e.at.col, e.at.row);
        let fs = (st.font_size.unwrap_or(13.0) * ed.zoom) as f32;
        let font = Font::with_dip("Segoe UI", fs, FontStyle::REGULAR);
        let text = e.le.text();
        let fmt = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP | StringFormatFlags::NO_CLIP | StringFormatFlags::MEASURE_TRAILING_SPACES };
        let measure = |s: &str| g.measure_string(s, &font, None, &fmt).width;
        let chars: Vec<char> = text.chars().collect();
        let prefix = |n: usize| -> String { chars[..n.min(chars.len())].iter().collect() };
        let caret_x = measure(&prefix(e.le.caret));
        let inner_w = (w as f32 - 8.0).max(1.0);
        let shift = (caret_x - inner_w + 2.0).max(0.0);
        let tx = r.left + 4.0 - shift;
        let clip = Rect::new(r.left + 2.0, r.top + 2.0, r.right - 2.0, r.bottom - 2.0);
        let saved = g.save();
        g.intersect_clip(clip);
        let (s0, s1) = e.le.selection();
        if s1 > s0 {
            let (a, b) = (measure(&prefix(s0)), measure(&prefix(s1)));
            g.fill_rectangle(Brush::solid(Color::rgb(160, 195, 255).with_alpha(0.55)), Rect::new(tx + a, r.top + 3.0, tx + b, r.bottom - 3.0));
        }
        // The text, coloured like FormulaInput (`colorSegments`).
        let segs = if text.starts_with('=') { refs::color_segments(&text, None, None) } else { Vec::new() };
        let mut colours: Vec<Option<String>> = vec![None; chars.len()];
        // The segments cover the text in order.
        let mut at = 0usize;
        for s in &segs {
            let n = s.text.chars().count();
            if let Some(c) = s.color {
                for slot in colours.iter_mut().skip(at).take(n) {
                    *slot = Some(c.to_string());
                }
            }
            at += n;
        }
        let mut i = 0;
        while i < chars.len() {
            let c0 = colours[i].clone();
            let mut j = i + 1;
            while j < chars.len() && colours[j] == c0 {
                j += 1;
            }
            let run: String = chars[i..j].iter().collect();
            let x0 = measure(&prefix(i));
            let col = c0.as_deref().and_then(Color::from_hex).unwrap_or(Color::rgb(0x20, 0x21, 0x24));
            g.draw_string(&run, &font, Brush::solid(col), Rect::new(tx + x0, r.top, tx + x0 + 100_000.0, r.bottom), &fmt);
            i = j;
        }
        if self.caret_visible() {
            let cx = (tx + caret_x).round() + 0.5;
            g.draw_line(&Pen::new(Color::BLACK, 1.0), PointF::new(cx, r.top + 4.0), PointF::new(cx, r.bottom - 4.0));
        }
        g.restore(saved);
        let accent = Color::from_hex("#1a73e8").unwrap_or(Color::BLUE);
        g.draw_rectangle(&Pen::new(accent, 2.0), Rect::new(r.left + 1.0, r.top + 1.0, r.right - 1.0, r.bottom - 1.0));
        // The autocomplete list under the cell.
        if !e.suggestions.is_empty() {
            let row_h = 22.0f32;
            let n = e.suggestions.len().min(10);
            let lw = 300.0f32;
            let top = r.bottom + 2.0;
            let list = Rect::new(r.left, top, r.left + lw, top + row_h * n as f32 + 8.0);
            g.fill_rectangle(Brush::solid(Color::WHITE), list);
            g.draw_rectangle(&Pen::new(Color::rgb(0xda, 0xdc, 0xe0), 1.0), list);
            let item_font = Font::with_dip("Segoe UI", 12.0, FontStyle::REGULAR);
            let bold = Font::with_dip("Segoe UI", 12.0, FontStyle::BOLD);
            for (k, s) in e.suggestions.iter().take(n).enumerate() {
                let iy = top + 4.0 + k as f32 * row_h;
                let ir = Rect::new(list.left + 1.0, iy, list.right - 1.0, iy + row_h);
                if k == e.sug {
                    g.fill_rectangle(Brush::solid(Color::rgb(0xe8, 0xf0, 0xfe)), ir);
                }
                let f = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP };
                g.draw_string(&s.name, &bold, Brush::solid(Color::rgb(0x20, 0x21, 0x24)), Rect::new(ir.left + 8.0, ir.top, ir.left + 120.0, ir.bottom), &f);
                g.draw_string(&s.syntax, &item_font, Brush::solid(Color::rgb(0x5f, 0x63, 0x68)), Rect::new(ir.left + 124.0, ir.top, ir.right - 6.0, ir.bottom), &f);
            }
        }
    }

    fn caret_visible(&self) -> bool {
        if !self.has_focus {
            return false;
        }
        let t = now().saturating_sub(self.caret_moved_ms);
        t < 500 || (t / 500) % 2 == 1
    }

    // ── Keys ────────────────────────────────────────────────────────────────

    /// A key while editing; `true` when handled.
    fn edit_key(&mut self, key: u16, mods: kubuno_desktop::controls::host::Modifiers) -> bool {
        use kubuno_desktop::controls::host::vk;
        let Some(e) = &mut self.edit else { return false };
        self.caret_moved_ms = now();
        if !e.suggestions.is_empty() {
            match key {
                vk::DOWN => {
                    e.sug = (e.sug + 1).min(e.suggestions.len() - 1);
                    self.invalidate();
                    return true;
                }
                vk::UP => {
                    e.sug = e.sug.saturating_sub(1);
                    self.invalidate();
                    return true;
                }
                vk::TAB | vk::ENTER => {
                    let s = e.suggestions[e.sug].clone();
                    e.le.accept(&s);
                    e.suggestions.clear();
                    self.invalidate();
                    return true;
                }
                vk::ESCAPE => {
                    e.suggestions.clear();
                    self.invalidate();
                    return true;
                }
                _ => {}
            }
        }
        let shift = mods.shift;
        match key {
            vk::ENTER => self.commit_edit(Some(if shift { Dir::Up } else { Dir::Down })),
            vk::ESCAPE => self.cancel_edit(),
            vk::TAB => self.commit_edit(Some(if shift { Dir::Left } else { Dir::Right })),
            vk::UP => self.commit_edit(Some(Dir::Up)),
            vk::DOWN => self.commit_edit(Some(Dir::Down)),
            vk::LEFT => {
                if e.le.caret == 0 && e.le.anchor.is_none() && !e.from_bar {
                    self.commit_edit(Some(Dir::Left));
                } else {
                    e.le.left(mods.ctrl, shift);
                }
            }
            vk::RIGHT => {
                if e.le.caret == e.le.len() && e.le.anchor.is_none() && !e.from_bar {
                    self.commit_edit(Some(Dir::Right));
                } else {
                    e.le.right(mods.ctrl, shift);
                }
            }
            vk::HOME => e.le.home(shift),
            vk::END => e.le.end(shift),
            vk::BACK => {
                e.le.backspace(mods.ctrl);
                self.refresh_assist();
            }
            vk::DELETE => {
                e.le.delete(mods.ctrl);
                self.refresh_assist();
            }
            vk::F4 => {
                e.le.f4();
            }
            k if mods.ctrl && k == vk::letter('A') => e.le.select_all(),
            _ => return false,
        }
        self.invalidate();
        true
    }

    /// A key on the grid (not editing); `true` when handled.
    pub fn key(&mut self, key: u16, mods: kubuno_desktop::controls::host::Modifiers) -> bool {
        use kubuno_desktop::controls::host::vk;
        if self.object_key(key, mods) {
            return true;
        }
        if self.edit.is_some() {
            return self.edit_key(key, mods);
        }
        let Some(ed) = self.ed.as_mut() else { return false };
        // Nothing selected yet: the grid takes no key (`if (!selectedCell || editingCell) return`, `SA:4429`).
        if !ed.selected {
            return false;
        }
        let dir = match key {
            vk::UP => Some(Dir::Up),
            vk::DOWN => Some(Dir::Down),
            vk::LEFT => Some(Dir::Left),
            vk::RIGHT => Some(Dir::Right),
            _ => None,
        };
        if let Some(d) = dir {
            if mods.ctrl {
                ed.jump_to_edge(d, mods.shift);
            } else if mods.shift {
                ed.extend_selection(d);
            } else {
                ed.move_selection(d);
            }
            self.reveal = true;
            self.invalidate();
            return true;
        }
        match key {
            vk::TAB => ed.move_selection(if mods.shift { Dir::Left } else { Dir::Right }),
            vk::ENTER | vk::F2 => {
                self.begin_edit(None);
                return true;
            }
            vk::DELETE | vk::BACK => {
                let r = ed.clear_selection();
                if let Err(e) = r {
                    self.refused(e);
                }
            }
            vk::HOME if mods.ctrl => ed.go_home(),
            vk::HOME => {
                let r = ed.sel.active.row;
                ed.select(Pos::new(0, r));
            }
            vk::PAGE_DOWN | vk::PAGE_UP => {
                let g = ed.geometry();
                let rows = ((self.view.height - COL_HEADER_HEIGHT) / g.row_height(ed.sel.active.row).max(1.0)).floor().max(1.0) as u32;
                let a = ed.sel.active;
                let r = if key == vk::PAGE_DOWN { (a.row + rows).min(MAX_ROWS) } else { a.row.saturating_sub(rows).max(1) };
                ed.select(Pos::new(a.col, r));
            }
            k if mods.ctrl && k == vk::letter('A') => ed.select_all_used(),
            _ => return false,
        }
        self.reveal = true;
        self.invalidate();
        true
    }

    // ── Pointer ─────────────────────────────────────────────────────────────

    /// The cell under a viewport point.
    fn hit_cell(&self, x: f64, y: f64) -> Option<Pos> {
        let ed = self.ed.as_ref()?;
        let g = ed.geometry();
        self.view.cell_at(ed, &g, x, y).map(|(c, r)| Pos::new(c, r))
    }

    /// Whether a viewport point is on the fill handle.
    fn on_handle(&self, x: f64, y: f64) -> bool {
        let Some(ed) = self.ed.as_ref() else { return false };
        if self.edit.is_some() {
            return false;
        }
        let g = ed.geometry();
        let corner = ed.handle_corner();
        let hx = self.view.screen_x(ed, &g, corner.col + 1).min(self.view.screen_x(ed, &g, corner.col) + g.col_width(corner.col));
        let hy = self.view.screen_y(ed, &g, corner.row) + g.row_height(corner.row);
        (x - hx).abs() <= HANDLE_GRAB && (y - hy).abs() <= HANDLE_GRAB
    }

    /// The column whose right edge is under x in the header (resize).
    fn col_edge_at(&self, x: f64) -> Option<u32> {
        let ed = self.ed.as_ref()?;
        let g = ed.geometry();
        let c = self.view.col_at(ed, &g, x)?;
        let left = self.view.screen_x(ed, &g, c);
        let right = left + g.col_width(c);
        if (x - right).abs() <= RESIZE_GRAB {
            Some(c)
        } else if (x - left).abs() <= RESIZE_GRAB && c > 0 {
            Some(c - 1)
        } else {
            None
        }
    }

    fn row_edge_at(&self, y: f64) -> Option<u32> {
        let ed = self.ed.as_ref()?;
        let g = ed.geometry();
        let r = self.view.row_at(ed, &g, y)?;
        let top = self.view.screen_y(ed, &g, r);
        let bottom = top + g.row_height(r);
        if (y - bottom).abs() <= RESIZE_GRAB {
            Some(r)
        } else if (y - top).abs() <= RESIZE_GRAB && r > 1 {
            Some(r - 1)
        } else {
            None
        }
    }

    fn drag_to(&mut self, x: f64, y: f64) {
        let Some(ed) = self.ed.as_mut() else { return };
        let g = ed.geometry();
        let cx = x.clamp(ROW_HEADER_WIDTH, self.view.width - 1.0);
        let cy = y.clamp(COL_HEADER_HEIGHT, self.view.height - 1.0);
        match self.drag.clone() {
            Drag::Select => {
                if let Some((c, r)) = self.view.cell_at(ed, &g, cx, cy) {
                    ed.extend_to(Pos::new(c, r));
                }
            }
            Drag::Cols(c0) => {
                if let Some(c) = self.view.col_at(ed, &g, cx) {
                    ed.select_cols(c0.min(c), c0.max(c));
                    ed.sel.active = Pos::new(c0, 1);
                    ed.sel.end = Some(Pos::new(c, MAX_ROWS));
                }
            }
            Drag::Rows(r0) => {
                if let Some(r) = self.view.row_at(ed, &g, cy) {
                    ed.sel.active = Pos::new(0, r0);
                    ed.sel.end = Some(Pos::new(MAX_COLS - 1, r));
                }
            }
            Drag::Fill { src, .. } => {
                if let Some((c, r)) = self.view.cell_at(ed, &g, cx, cy) {
                    self.drag = Drag::Fill { src, to: Pos::new(c, r) };
                }
            }
            Drag::ResizeCol { c, x0, w0 } => {
                let w = (w0 + (x - x0) / ed.zoom).max(20.0);
                ed.sheet_mut_for_preview().set_col_width(c, w.round());
                ed.touch();
            }
            Drag::ResizeRow { r, y0, h0 } => {
                let h = (h0 + (y - y0) / ed.zoom).max(14.0);
                ed.sheet_mut_for_preview().set_row_height(r, h.round());
                ed.touch();
            }
            Drag::Point { start } => {
                if let Some((c, r)) = self.view.cell_at(ed, &g, cx, cy) {
                    let rr = addr::Rect::new(start.col, start.row, c, r);
                    if let Some(e) = &mut self.edit {
                        e.le.point(&rr.a1());
                    }
                }
            }
            _ => {}
        }
        self.invalidate();
    }

    /// Auto-scroll while a drag is near the body's edges (`SA:6001-6030`).
    fn auto_scroll(&mut self) {
        let Some((x, y)) = self.drag_at else { return };
        if !matches!(self.drag, Drag::Select | Drag::Cols(_) | Drag::Rows(_) | Drag::Fill { .. } | Drag::Point { .. }) {
            return;
        }
        let speed = |over: f64| over.signum() * (6.0 + over.abs() * 0.35).min(40.0);
        let mut dx = 0.0;
        let mut dy = 0.0;
        if x < ROW_HEADER_WIDTH {
            dx = speed(x - ROW_HEADER_WIDTH);
        } else if x > self.view.width {
            dx = speed(x - self.view.width);
        }
        if y < COL_HEADER_HEIGHT {
            dy = speed(y - COL_HEADER_HEIGHT);
        } else if y > self.view.height {
            dy = speed(y - self.view.height);
        }
        if dx != 0.0 || dy != 0.0 {
            self.scroll(dx, dy);
            self.drag_to(x, y);
            kubuno_desktop::controls::host::request_repaint_after(16);
        }
    }

    fn end_drag(&mut self) {
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        self.drag_at = None;
        let Some(ed) = self.ed.as_mut() else { return };
        match drag {
            Drag::Fill { src, to } => {
                if let Err(e) = ed.fill(src, to) {
                    self.refused(e);
                }
            }
            Drag::ResizeCol { c, w0, .. } => {
                let w = ed.sheet().col_width_override(&addr::index_to_col(c as i64)).unwrap_or(ed.sheet().default_col_width());
                ed.sheet_mut_for_preview().set_col_width(c, w0);
                let _ = ed.set_col_widths(c, c, w);
            }
            Drag::ResizeRow { r, h0, .. } => {
                let h = ed.sheet().row_height_override(r).unwrap_or(ed.sheet().default_row_height());
                ed.sheet_mut_for_preview().set_row_height(r, h0);
                let _ = ed.set_row_heights(r, r, h);
            }
            _ => {}
        }
        self.invalidate();
    }

    fn report(&mut self) {
        if self.design_mode() {
            return;
        }
        if let Some(args) = self.state_args() {
            if self.reported.as_ref() != Some(&args) {
                self.reported = Some(args.clone());
                self.raise_state_changed(args);
                kubuno_desktop::controls::host::request_repaint_after(0);
            }
        }
    }
}

impl Control for GridCanvas {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 800.0, height: 500.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        self.scale = e.graphics.dpi_scale().max(0.5);
        self.sync_view_size();
        if self.ed.is_none() && self.design_mode() {
            self.ed = Some(Editor::new(kubuno_office_spreadsheets_common::sample::workbook(), std::rc::Rc::new(kubuno_office_spreadsheets_common::platform::SystemEnv::default())));
        }
        if let Some(ed) = self.ed.as_ref() {
            let g = ed.geometry();
            self.view.clamp(ed, &g);
        }
        if self.reveal {
            self.reveal = false;
            self.reveal_active();
        }
        self.auto_scroll();
        let g = e.graphics;
        let Some(ed) = self.ed.as_ref() else {
            g.fill_rectangle(Brush::solid(Color::WHITE), self.bounds);
            return;
        };
        let geom = ed.geometry();
        let merges = ed.merges();
        let mut overlay = Overlay { editing: self.edit.is_some(), refs: self.edit_refs(), filter_mode: ed.filter_mode, ..Overlay::default() };
        overlay.filters = ed.col_filters.keys().map(|c| (*c, true)).collect::<HashMap<_, _>>();
        if let Drag::Fill { src, to } = &self.drag {
            let fb = fill_bounds(src, *to);
            if fb.axis != kubuno_office_sheets_core::editor::commands::FillAxis::None {
                overlay.fill = Some(addr::Rect::new(fb.c1, fb.r1, fb.c2, fb.r2));
            }
        }
        let lang = ed.lang.clone();
        // The charts' drawings read cell values: rendered before the painter holds the evaluator.
        self.objects.prepare(ed);
        let dark = {
            let c: &dyn Canvas = e.graphics;
            c.theme().mode == kubuno_desktop::ui::ThemeMode::Dark
        };
        let op = objects::ObjPaint { g, ox: self.bounds.left, oy: self.bounds.top, scroll_x: self.view.scroll_x, scroll_y: self.view.scroll_y, vw: self.view.width, vh: self.view.height, dark, family: "Segoe UI" };
        let paint_objects = || op.paint_objects(ed, &self.objects);
        let painter = GridPainter { g, ox: self.bounds.left, oy: self.bounds.top, dpr: self.scale, ed, geom: &geom, view: &self.view, merges: &merges, lang: &lang, objects: Some(&paint_objects) };
        let saved = g.save();
        g.intersect_clip(self.bounds);
        ed.with_eval(|data, ctx, spill, cf| painter.paint(data, ctx, spill, cf, &overlay));
        op.paint_chrome(ed, &self.objects);
        self.paint_peers(g, ed, &geom);
        self.paint_editor(g);
        g.restore(saved);
        self.paint_equations(g);
        {
            let c: &dyn Canvas = e.graphics;
            if let Some((bar, rail)) = self.vbar() {
                bar.paint(c, rail, if self.vhot { WidgetState::REST.hot(true) } else { WidgetState::REST });
            }
            if let Some((bar, rail)) = self.hbar() {
                bar.paint(c, rail, if self.hhot { WidgetState::REST.hot(true) } else { WidgetState::REST });
            }
        }
        if self.edit.is_some() && self.has_focus {
            kubuno_desktop::controls::host::request_repaint_after(500);
        }
        self.report();
        e.raise(self, "OnPaint");
    }

    fn is_input_key(&self, key: Keys) -> bool {
        use kubuno_desktop::controls::host::vk;
        [vk::PAGE_DOWN, vk::PAGE_UP, vk::DOWN, vk::UP, vk::LEFT, vk::RIGHT, vk::HOME, vk::END, vk::TAB, vk::ENTER, vk::BACK, vk::DELETE, vk::ESCAPE, vk::F2, vk::F4].contains(&key.key.0)
    }

    fn on_got_focus(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        self.has_focus = true;
        self.caret_moved_ms = now();
        self.invalidate();
        e.raise(&*self, "OnGotFocus");
    }

    fn on_lost_focus(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        self.has_focus = false;
        self.invalidate();
        e.raise(&*self, "OnLostFocus");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let notches = e.args().delta as f64;
        let mods = e.args().mods;
        if mods.ctrl {
            // A desktop convention (the web zooms from the status bar only): ±10 % per notch.
            let z = self.ed.as_ref().map(|ed| ed.zoom).unwrap_or(1.0);
            self.with_editor(|ed| ed.set_zoom(z + if notches < 0.0 { 0.1 } else { -0.1 }));
        } else if mods.shift {
            self.scroll(notches * WHEEL_STEP, 0.0);
        } else {
            self.scroll(0.0, notches * WHEEL_STEP);
        }
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x as f64, e.args().y as f64);
        let (ax, ay) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        let button = e.args().button;
        let mods = e.args().mods;
        self.focus();
        self.has_focus = true;
        // Scroll bars.
        if let Some((bar, rail)) = self.vbar().filter(|(_, r)| r.contains(ax, ay)) {
            let page = self.view.height - COL_HEADER_HEIGHT;
            match bar.part_at(rail, ax, ay) {
                Some(ScrollPart::Thumb) => self.drag = Drag::VScroll(ay - bar.thumb_rect(rail).top),
                Some(ScrollPart::PageLow) => self.scroll(0.0, -page),
                Some(ScrollPart::PageHigh) => self.scroll(0.0, page),
                Some(ScrollPart::ArrowLow) => self.scroll(0.0, -40.0),
                Some(ScrollPart::ArrowHigh) => self.scroll(0.0, 40.0),
                None => {}
            }
            e.raise(&*self, "OnMouseDown");
            return;
        }
        if let Some((bar, rail)) = self.hbar().filter(|(_, r)| r.contains(ax, ay)) {
            let page = self.view.width - ROW_HEADER_WIDTH;
            match bar.part_at(rail, ax, ay) {
                Some(ScrollPart::Thumb) => self.drag = Drag::HScroll(ax - bar.thumb_rect(rail).left),
                Some(ScrollPart::PageLow) => self.scroll(-page, 0.0),
                Some(ScrollPart::PageHigh) => self.scroll(page, 0.0),
                Some(ScrollPart::ArrowLow) => self.scroll(-40.0, 0.0),
                Some(ScrollPart::ArrowHigh) => self.scroll(40.0, 0.0),
                None => {}
            }
            e.raise(&*self, "OnMouseDown");
            return;
        }
        // Floating objects take the press first (`chartHitTest` / `imageHitTest` / `shapeHitTest`).
        if self.object_mouse_down(x, y, button, mods) {
            self.drag_at = None;
            e.raise(&*self, "OnMouseDown");
            return;
        }
        if button == MouseButton::Right {
            // A right click outside the selection selects the cell (or the header's column/row).
            if let Some(p) = self.hit_cell(x, y) {
                let inside = self.ed.as_ref().is_some_and(|ed| ed.sel.rect().contains(p));
                if !inside {
                    self.commit_edit_quietly();
                    self.with_editor(|ed| ed.select(p));
                }
            } else if y < COL_HEADER_HEIGHT && x >= ROW_HEADER_WIDTH {
                if let Some(c) = self.ed.as_ref().and_then(|ed| self.view.col_at(ed, &ed.geometry(), x)) {
                    let inside = self.ed.as_ref().is_some_and(|ed| ed.sel.whole_cols() && ed.sel.rect().c1 <= c && c <= ed.sel.rect().c2);
                    if !inside {
                        self.with_editor(|ed| ed.select_cols(c, c));
                    }
                }
            } else if x < ROW_HEADER_WIDTH && y >= COL_HEADER_HEIGHT {
                if let Some(r) = self.ed.as_ref().and_then(|ed| self.view.row_at(ed, &ed.geometry(), y)) {
                    let inside = self.ed.as_ref().is_some_and(|ed| ed.sel.whole_rows() && ed.sel.rect().r1 <= r && r <= ed.sel.rect().r2);
                    if !inside {
                        self.with_editor(|ed| ed.select_rows(r, r));
                    }
                }
            }
            e.raise(&*self, "OnMouseDown");
            return;
        }
        if button != MouseButton::Left {
            e.raise(&*self, "OnMouseDown");
            return;
        }
        if self.equations_mouse_down(x, y) {
            e.raise(&*self, "OnMouseDown");
            return;
        }
        let Some(ed) = self.ed.as_ref() else { return };
        let g = ed.geometry();
        // The corner.
        if x < ROW_HEADER_WIDTH && y < COL_HEADER_HEIGHT {
            self.commit_edit_quietly();
            self.with_editor(|ed| ed.select_everything());
        } else if y < COL_HEADER_HEIGHT {
            // A column header: outline toggle, resize, select.
            let c = self.view.col_at(ed, &g, x);
            let group = c.and_then(|c| ed.sheet().col_groups().into_iter().find(|gg| gg.start == c)).filter(|_| y <= 15.0);
            if let (Some(gr), Some(c)) = (group, c) {
                let left = self.view.screen_x(ed, &g, c);
                if x - left <= 15.0 {
                    self.run(|ed| ed.toggle_group_at(kubuno_office_sheets_core::editor::commands::Axis::Col, gr.start));
                    e.raise(&*self, "OnMouseDown");
                    return;
                }
            }
            if let Some(c) = self.col_edge_at(x) {
                let w0 = ed.geometry().col_width(c) / ed.zoom;
                self.drag = Drag::ResizeCol { c, x0: x, w0 };
            } else if let Some(c) = c {
                self.commit_edit_quietly();
                let shift_from = if mods.shift { self.ed.as_ref().map(|ed| ed.sel.active.col) } else { None };
                let c0 = shift_from.unwrap_or(c);
                self.with_editor(|ed| {
                    ed.select_cols(c0.min(c), c0.max(c));
                    ed.sel.active = Pos::new(c0, 1);
                    ed.sel.end = Some(Pos::new(c, MAX_ROWS));
                });
                self.drag = Drag::Cols(c0);
            }
        } else if x < ROW_HEADER_WIDTH {
            let r = self.view.row_at(ed, &g, y);
            let group = r.and_then(|r| ed.sheet().row_groups().into_iter().find(|gg| gg.start == r)).filter(|_| x <= 15.0);
            if let Some(gr) = group {
                self.run(|ed| ed.toggle_group_at(kubuno_office_sheets_core::editor::commands::Axis::Row, gr.start));
                e.raise(&*self, "OnMouseDown");
                return;
            }
            if let Some(r) = self.row_edge_at(y) {
                let h0 = ed.geometry().row_height(r) / ed.zoom;
                self.drag = Drag::ResizeRow { r, y0: y, h0 };
            } else if let Some(r) = r {
                self.commit_edit_quietly();
                let shift_from = if mods.shift { self.ed.as_ref().map(|ed| ed.sel.active.row) } else { None };
                let r0 = shift_from.unwrap_or(r);
                self.with_editor(|ed| {
                    ed.sel.active = Pos::new(0, r0);
                    ed.sel.end = Some(Pos::new(MAX_COLS - 1, r));
                });
                self.drag = Drag::Rows(r0);
            }
        } else if self.on_handle(x, y) {
            let src = ed.sel.rect();
            self.drag = Drag::Fill { src, to: Pos::new(src.c2, src.r2) };
        } else if let Some(p) = self.hit_cell(x, y) {
            // Point mode: a formula expecting an operand gets the reference.
            let pointing = self.edit.as_ref().is_some_and(|e| e.le.expects_ref() || e.le.pointed.is_some_and(|(_, end)| end == e.le.caret));
            if pointing && self.edit.as_ref().is_some_and(|e| e.at != p) {
                if let Some(e) = &mut self.edit {
                    e.le.point(&p.key());
                }
                self.drag = Drag::Point { start: p };
                self.invalidate();
                e.raise(&*self, "OnMouseDown");
                return;
            }
            self.commit_edit_quietly();
            let Some(ed) = self.ed.as_ref() else { return };
            // Validation widgets: a checkbox toggles; the dropdown arrow is left to the window (`OnDropdown`).
            if let Some(rule) = ed.validation_at(p) {
                if kubuno_office_sheets_core::validation::is_checkbox(Some(&rule)) {
                    self.with_editor(|ed| ed.select(p));
                    self.run(|ed| ed.toggle_checkbox(p));
                    e.raise(&*self, "OnMouseDown");
                    return;
                }
            }
            if mods.shift {
                self.with_editor(|ed| ed.extend_to(p));
            } else {
                self.with_editor(|ed| ed.select(p));
            }
            self.drag = Drag::Select;
        }
        self.drag_at = Some((x, y));
        self.invalidate();
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x as f64, e.args().y as f64);
        if self.equations_mouse_move(x, y, e.args().mods.shift) {
            e.raise(&*self, "OnMouseMove");
            return;
        }
        if self.objects.drag.is_some() {
            if e.args().button != MouseButton::Left {
                self.object_mouse_up();
            } else {
                self.object_mouse_move(x, y, e.args().mods);
            }
            e.raise(&*self, "OnMouseMove");
            return;
        }
        let (ax, ay) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        if e.args().button != MouseButton::Left && self.drag != Drag::None {
            self.end_drag();
        }
        match self.drag.clone() {
            Drag::VScroll(grab) => {
                if let (Some((bar, rail)), Some(ed)) = (self.vbar(), self.ed.as_ref()) {
                    let g = ed.geometry();
                    let (_, my) = self.view.max_scroll(ed, &g);
                    let thumb = bar.thumb_rect(rail);
                    let travel = (rail.bottom - rail.top) - (thumb.bottom - thumb.top);
                    if travel > 0.0 {
                        self.view.scroll_y = (((ay - grab - rail.top).clamp(0.0, travel)) / travel) as f64 * my;
                        self.invalidate();
                    }
                }
            }
            Drag::HScroll(grab) => {
                if let (Some((bar, rail)), Some(ed)) = (self.hbar(), self.ed.as_ref()) {
                    let g = ed.geometry();
                    let (mx, _) = self.view.max_scroll(ed, &g);
                    let thumb = bar.thumb_rect(rail);
                    let travel = (rail.right - rail.left) - (thumb.right - thumb.left);
                    if travel > 0.0 {
                        self.view.scroll_x = (((ax - grab - rail.left).clamp(0.0, travel)) / travel) as f64 * mx;
                        self.invalidate();
                    }
                }
            }
            Drag::None => {
                let vhot = self.vbar().is_some_and(|(_, r)| r.inflate(4.0, 0.0).contains(ax, ay));
                let hhot = self.hbar().is_some_and(|(_, r)| r.inflate(0.0, 4.0).contains(ax, ay));
                if vhot != self.vhot || hhot != self.hhot {
                    self.vhot = vhot;
                    self.hhot = hhot;
                    self.invalidate();
                }
            }
            _ => {
                self.drag_at = Some((x, y));
                self.drag_to(x, y);
                kubuno_desktop::controls::host::request_repaint_after(16);
            }
        }
        self.raise_pointer_moved(GridPointerEventArgs { x: x + self.view.scroll_x, y: y + self.view.scroll_y, inside: true });
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if self.equations_mouse_up() {
            e.raise(&*self, "OnMouseUp");
            return;
        }
        if !self.object_mouse_up() {
            self.end_drag();
        }
        e.raise(&*self, "OnMouseUp");
    }

    fn on_mouse_double_click(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x as f64, e.args().y as f64);
        if let Some((id, latex)) = self.equation_at(x, y) {
            self.raise_equation_edit(equations::EquationEventArgs { id, latex });
            e.raise(&*self, "OnMouseDoubleClick");
            return;
        }
        if self.object_double_click(x, y) {
            e.raise(&*self, "OnMouseDoubleClick");
            return;
        }
        if y < COL_HEADER_HEIGHT && x >= ROW_HEADER_WIDTH {
            if let Some(c) = self.col_edge_at(x) {
                let measure = |text: &str, s: &kubuno_office_sheets_core::model::style::Style| -> f64 {
                    let mut fst = 0;
                    if s.is_bold() {
                        fst |= FontStyle::BOLD.0;
                    }
                    if s.is_italic() {
                        fst |= FontStyle::ITALIC.0;
                    }
                    let family = s.font_family.clone().unwrap_or_else(|| "Arial".into());
                    let font = Font::with_dip(&family, s.font_size.unwrap_or(13.0) as f32, FontStyle(fst));
                    kubuno_desktop::ui::graphics::text::approximate_measure(text, &font, None).0 as f64
                };
                self.run(|ed| ed.autofit_columns(c, c, &measure));
            }
        } else if let Some(p) = self.hit_cell(x, y) {
            if self.edit.is_none() {
                self.with_editor(|ed| ed.select(p));
                self.begin_edit(None);
            }
        }
        e.raise(&*self, "OnMouseDoubleClick");
    }

    fn on_mouse_leave(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        self.raise_pointer_moved(GridPointerEventArgs { x: 0.0, y: 0.0, inside: false });
        if self.drag == Drag::None && (self.vhot || self.hhot) {
            self.vhot = false;
            self.hhot = false;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseLeave");
    }

    fn cursor_at(&self, x: f32, y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor;
        let (x, y) = (x as f64, y as f64);
        if let Some(c) = self.equation_cursor(x, y) {
            return Some(c);
        }
        if let Some(c) = self.object_cursor(x, y) {
            return Some(c);
        }
        if matches!(self.drag, Drag::ResizeCol { .. }) || (y < COL_HEADER_HEIGHT && x >= ROW_HEADER_WIDTH && self.col_edge_at(x).is_some()) {
            return Some(Cursor::ResizeEW);
        }
        if matches!(self.drag, Drag::ResizeRow { .. }) || (x < ROW_HEADER_WIDTH && y >= COL_HEADER_HEIGHT && self.row_edge_at(y).is_some()) {
            return Some(Cursor::ResizeNS);
        }
        Some(Cursor::Arrow)
    }

    fn on_key_down(&mut self, e: &mut EventCx<'_, KeyEventArgs>) {
        let (key, mods) = (e.args().key.0, e.args().mods);
        if self.eq.selected.is_some() && self.edit.is_none() && (key == kubuno_desktop::controls::host::vk::DELETE || key == kubuno_desktop::controls::host::vk::BACK) {
            self.delete_selected_equation();
            e.args_mut().handled = true;
            e.raise(&*self, "OnKeyDown");
            return;
        }
        if key == kubuno_desktop::controls::host::vk::ESCAPE && self.eq.selected.is_some() && self.edit.is_none() {
            self.select_equation(None);
        }
        if self.key(key, mods) {
            e.args_mut().handled = true;
            e.args_mut().suppress_key_press = true;
        }
        e.raise(&*self, "OnKeyDown");
    }

    fn on_key_press(&mut self, e: &mut EventCx<'_, KeyPressEventArgs>) {
        let c = e.args().key_char;
        if !c.is_control() {
            let s = c.to_string();
            match &mut self.edit {
                Some(ed) => {
                    ed.le.insert(&s);
                    self.caret_moved_ms = now();
                    self.refresh_assist();
                }
                None => self.begin_edit(Some(&s)),
            }
            self.invalidate();
            e.args_mut().handled = true;
        }
        e.raise(&*self, "OnKeyPress");
    }
}

/// Mutable access to the active sheet for a live preview that is recorded later (a resize drag).
trait PreviewSheet {
    fn sheet_mut_for_preview(&mut self) -> &mut kubuno_office_sheets_core::model::sheet::Sheet;
}

impl PreviewSheet for Editor {
    fn sheet_mut_for_preview(&mut self) -> &mut kubuno_office_sheets_core::model::sheet::Sheet {
        let i = self.active_index();
        &mut self.wb.sheets[i]
    }
}

/// The words of the objects' Name Box labels (`objLabel`, `sheet_obj_selected_count`).
fn obj_words() -> objects::ObjWords {
    objects::ObjWords {
        chart: crate::Resources::obj_chart_name().to_string(),
        image: crate::Resources::obj_image_name().to_string(),
        shape: crate::Resources::obj_shape_name().to_string(),
        equation: crate::Resources::obj_equation_name().to_string(),
        count: crate::Resources::obj_selected_count().to_string(),
    }
}
