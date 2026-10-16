//! `SheetTabs` - the sheet tabs under the grid (`SA:7898-7953`): « + » first (add a sheet), then one tab
//! per sheet; a click activates a tab, a double click renames it, the active tab shows a delete button on
//! hover when there is more than one sheet. The active tab is white with the accent's top border and text,
//! the others transparent with the secondary text, as on the web.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{AccessiblePart, Component, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;

/// A tab action (`index` of the sheet; `-1` for « + »).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct SheetTabEventArgs {
    pub index: i32,
}

const PLUS_W: f32 = 36.0;
const PAD_X: f32 = 12.0;
const GAP: f32 = 6.0;
const TRASH_W: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Plus,
    Tab(usize),
    Trash(usize),
}

/// The sheet tabs (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Spreadsheets")]
#[toolbox(icon = "panel-bottom")]
#[default_event("TabClick")]
pub struct SheetTabs {
    base: ControlCore,
    /// The sheet names, one per line.
    #[property(bindable)]
    #[category("Data")]
    pub tabs: String,
    /// The active tab.
    #[property(bindable)]
    #[category("Data")]
    pub active: i32,
    /// A tab was clicked.
    #[event]
    #[category("Action")]
    pub tab_click: Event<SheetTabEventArgs>,
    /// A tab was double-clicked (rename).
    #[event]
    #[category("Action")]
    pub tab_double_click: Event<SheetTabEventArgs>,
    /// « + » was clicked.
    #[event]
    #[category("Action")]
    pub add_click: Event<SheetTabEventArgs>,
    /// The active tab's delete button was clicked.
    #[event]
    #[category("Action")]
    pub delete_click: Event<SheetTabEventArgs>,
    hot: Option<Hit>,
    widths: std::cell::RefCell<Vec<f32>>,
}

impl Default for SheetTabs {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            tabs: "Feuille 1".into(),
            active: 0,
            tab_click: Event::default(),
            tab_double_click: Event::default(),
            add_click: Event::default(),
            delete_click: Event::default(),
            hot: None,
            widths: Default::default(),
        }
    }
}

impl SheetTabs {
    fn names(&self) -> Vec<&str> {
        self.tabs.split('\n').filter(|s| !s.is_empty()).collect()
    }

    fn font(active: bool) -> Font {
        Font::with_dip("Segoe UI", 12.0, if active { FontStyle::BOLD } else { FontStyle::REGULAR })
    }

    /// Each tab's (left, right) in local coordinates.
    fn layout(&self) -> Vec<(f32, f32)> {
        let widths = self.widths.borrow();
        let mut x = PLUS_W + 4.0;
        let mut out = Vec::new();
        for (i, _) in self.names().iter().enumerate() {
            let w = widths.get(i).copied().unwrap_or(80.0);
            out.push((x, x + w));
            x += w + 2.0;
        }
        out
    }

    fn hit(&self, x: f32, _y: f32) -> Option<Hit> {
        if x < PLUS_W {
            return Some(Hit::Plus);
        }
        let several = self.names().len() > 1;
        for (i, (l, r)) in self.layout().into_iter().enumerate() {
            if x >= l && x < r {
                if several && i as i32 == self.active && x >= r - PAD_X - TRASH_W + 4.0 {
                    return Some(Hit::Trash(i));
                }
                return Some(Hit::Tab(i));
            }
        }
        None
    }
}

impl Control for SheetTabs {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
        Size { width: proposed.width.max(200.0), height: 36.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let g = e.graphics;
        let c: &dyn Canvas = e.graphics;
        let t = c.theme();
        let dark = t.mode == kubuno_desktop::ui::ThemeMode::Dark;
        let ground = if dark { Color::from(t.layer_background) } else { Color::from_hex("#f8f9fa").unwrap_or(Color::WHITE) };
        g.fill_rectangle(Brush::solid(ground), b);
        let border = if dark { Color::from(t.card_stroke) } else { Color::from_hex("#e2e4e6").unwrap_or(Color::GRAY) };
        g.draw_line(&Pen::new(border, 1.0), PointF::new(b.left, b.top + 0.5), PointF::new(b.right, b.top + 0.5));
        let accent = Color::from_hex("#1a73e8").unwrap_or(Color::BLUE);
        let secondary = Color::from(t.text_secondary);
        let hover = if dark { Color::rgba_f(1.0, 1.0, 1.0, 0.08) } else { Color::from_hex("#e8eaed").unwrap_or(Color::GRAY) };
        // « + ».
        let plus = Rect::new(b.left + 4.0, b.top + 4.0, b.left + PLUS_W, b.bottom - 4.0);
        if self.hot == Some(Hit::Plus) && !self.design_mode() {
            g.fill_rounded_rectangle(Brush::solid(hover), plus, 4.0);
        }
        let (pcx, pcy) = ((plus.left + plus.right) / 2.0, (plus.top + plus.bottom) / 2.0);
        c.vector_icon("Plus", &Rect::new(pcx - 8.0, pcy - 8.0, pcx + 8.0, pcy + 8.0), 16.0, &t.text_secondary);
        // Measure the tabs.
        let names: Vec<String> = self.names().iter().map(|s| s.to_string()).collect();
        let fmt = StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP };
        let several = names.len() > 1;
        let widths: Vec<f32> = names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let active = i as i32 == self.active;
                let tw = g.measure_string(n, &Self::font(active), None, &fmt).width;
                PAD_X * 2.0 + tw + if active && several { GAP + TRASH_W - 4.0 } else { 0.0 }
            })
            .collect();
        *self.widths.borrow_mut() = widths;
        for (i, (l, r)) in self.layout().into_iter().enumerate() {
            let active = i as i32 == self.active;
            let tab = Rect::new(b.left + l, b.top, b.left + r, b.bottom);
            if active {
                let bg = if dark { Color::from(t.card_background) } else { Color::WHITE };
                g.fill_rectangle(Brush::solid(bg), tab);
                g.fill_rectangle(Brush::solid(accent), Rect::new(tab.left, tab.top, tab.right, tab.top + 2.0));
            } else if self.hot == Some(Hit::Tab(i)) && !self.design_mode() {
                g.fill_rectangle(Brush::solid(hover), tab);
            }
            let ink = if active { accent } else { secondary };
            g.draw_string(&names[i], &Self::font(active), Brush::solid(ink), Rect::new(tab.left + PAD_X, tab.top + 2.0, tab.right, tab.bottom), &fmt);
            if active && several && matches!(self.hot, Some(Hit::Tab(j)) | Some(Hit::Trash(j)) if j == i) {
                let tx = tab.right - PAD_X - TRASH_W + 4.0;
                let cy = (tab.top + tab.bottom) / 2.0 + 1.0;
                c.vector_icon("Trash2", &Rect::new(tx, cy - 5.0, tx + 10.0, cy + 5.0), 10.0, &t.text_secondary);
            }
        }
        e.raise(self, "OnPaint");
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let h = self.size().height;
        let mut out = vec![AccessiblePart { name: crate::Resources::sheet_add().to_string(), role: AccessRole::Button, bounds: Rect::new(0.0, 0.0, PLUS_W, h) }];
        for (n, (l, r)) in self.names().into_iter().zip(self.layout()) {
            out.push(AccessiblePart { name: n.to_string(), role: AccessRole::Button, bounds: Rect::new(l, 0.0, r, h) });
        }
        out
    }

    fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
        match self.hit(x, y)? {
            Hit::Plus => Some(crate::Resources::sheet_add().to_string()),
            Hit::Trash(_) => Some(crate::Resources::sheet_delete().to_string()),
            Hit::Tab(_) => None,
        }
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let hot = self.hit(e.args().x, e.args().y);
        if hot != self.hot {
            self.hot = hot;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_leave(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        if self.hot.take().is_some() {
            self.invalidate();
        }
        e.raise(&*self, "OnMouseLeave");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            match self.hit(e.args().x, e.args().y) {
                Some(Hit::Plus) => self.raise_add_click(SheetTabEventArgs { index: -1 }),
                Some(Hit::Trash(i)) => self.raise_delete_click(SheetTabEventArgs { index: i as i32 }),
                Some(Hit::Tab(i)) => {
                    if e.args().clicks >= 2 {
                        self.raise_tab_double_click(SheetTabEventArgs { index: i as i32 });
                    } else {
                        self.raise_tab_click(SheetTabEventArgs { index: i as i32 });
                    }
                }
                None => {}
            }
        }
        e.raise(&*self, "OnMouseDown");
    }
}
