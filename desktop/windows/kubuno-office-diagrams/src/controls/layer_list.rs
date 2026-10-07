//! `LayerList` — the Calques panel (`DiagramEditorPage.tsx:3272-3316`): a header (`px-2 py-1.5 border-b`,
//! « Calques » `text-xs font-medium text-secondary`, « + » of 14), the layers top first (`px-2 py-1.5 gap-1`,
//! the active one `bg-primary/5`, hover `bg-surface-1`), each with its visibility and lock toggles (13), its
//! name (`text-xs truncate`, double click to rename), up / down and delete (dimmed to 30 % when it is the last
//! layer); and, when something is selected, « Déplacer la sélection ici » (`text-xs text-primary border-t`).

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{AccessiblePart, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::Event;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

/// Whether a row's button acts, row `i` of `n` (top first): the top layer cannot go up, the bottom one
/// cannot go down, the last layer cannot be deleted (the web dims them to 30 %).
fn part_enabled(part: RowPart, i: usize, n: usize) -> bool {
    match part {
        RowPart::Up => i > 0,
        RowPart::Down => i + 1 < n,
        RowPart::Delete => n > 1,
        _ => true,
    }
}

/// A layer as the list shows it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LayerRow {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
}

/// What the user asked of a layer: `activate`, `add`, `delete`, `visible`, `lock`, `up`, `down`, `rename`,
/// `move_here`.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct LayersCommandEventArgs {
    pub command: String,
    pub id: String,
    pub name: String,
}

/// `py-1.5` around a `text-xs` line.
const ROW_H: f32 = 28.0;
/// `px-2`.
const PAD: f32 = 8.0;
/// A 13 DIP glyph in its `p-0.5` button.
const ICON_W: f32 = 17.0;
/// `gap-1`.
const GAP: f32 = 4.0;

/// One clickable part of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowPart {
    Visible,
    Lock,
    Name,
    Up,
    Down,
    Delete,
}

impl RowPart {
    fn command(self) -> &'static str {
        match self {
            RowPart::Visible => "visible",
            RowPart::Lock => "lock",
            RowPart::Name => "activate",
            RowPart::Up => "up",
            RowPart::Down => "down",
            RowPart::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Add,
    Row(usize, RowPart),
    MoveHere,
}

/// The header's « + », the rows and the footer, in a `w`×`h` box at the origin.
fn header_add(w: f32) -> Rect {
    Rect::new(w - PAD - ICON_W - 2.0, 0.0, w - PAD + 2.0, ROW_H)
}

fn row_rect(i: usize, w: f32) -> Rect {
    let top = ROW_H + 1.0 + i as f32 * ROW_H;
    Rect::new(0.0, top, w, top + ROW_H)
}

/// The parts of a row, left to right.
fn row_parts(row: Rect) -> [(RowPart, Rect); 6] {
    let r = |l: f32, w: f32| Rect::new(l, row.top, l + w, row.bottom);
    let left = row.left + PAD;
    let right = row.right - PAD;
    let name_l = left + 2.0 * (ICON_W + GAP);
    let name_r = (right - 3.0 * ICON_W - 3.0 * GAP).max(name_l);
    [
        (RowPart::Visible, r(left, ICON_W)),
        (RowPart::Lock, r(left + ICON_W + GAP, ICON_W)),
        (RowPart::Name, Rect::new(name_l, row.top, name_r, row.bottom)),
        (RowPart::Up, r(right - 3.0 * ICON_W - 2.0 * GAP, ICON_W)),
        (RowPart::Down, r(right - 2.0 * ICON_W - GAP, ICON_W)),
        (RowPart::Delete, r(right - ICON_W, ICON_W)),
    ]
}

fn footer_rect(rows: usize, w: f32) -> Rect {
    let top = ROW_H + 1.0 + rows as f32 * ROW_H;
    Rect::new(0.0, top, w, top + ROW_H + 1.0)
}

fn hit(rows: usize, has_selection: bool, w: f32, x: f32, y: f32) -> Option<Hit> {
    if header_add(w).contains(x, y) {
        return Some(Hit::Add);
    }
    for i in 0..rows {
        let row = row_rect(i, w);
        if row.contains(x, y) {
            // A click between two buttons activates the layer, as a click on the row does on the web.
            let part = row_parts(row).into_iter().find(|(_, r)| r.contains(x, y)).map_or(RowPart::Name, |(p, _)| p);
            return Some(Hit::Row(i, part));
        }
    }
    (has_selection && footer_rect(rows, w).contains(x, y)).then_some(Hit::MoveHere)
}

fn with_alpha(c: D2D1_COLOR_F, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { a: c.a * a, ..c }
}

/// The layers list (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Diagrams")]
#[toolbox(icon = "layers")]
#[default_event("LayersCommand")]
pub struct LayerList {
    base: ControlCore,
    #[event]
    #[category("Action")]
    pub layers_command: Event<LayersCommandEventArgs>,
    layers: Vec<LayerRow>,
    active: String,
    has_selection: bool,
    hot: Option<Hit>,
    pressed: Option<Hit>,
}

impl Default for LayerList {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            layers_command: Event::default(),
            layers: vec![LayerRow { id: "default".into(), name: "Calque 1".into(), visible: true, locked: false }],
            active: "default".into(),
            has_selection: false,
            hot: None,
            pressed: None,
        }
    }
}

impl LayerList {
    pub fn set_layers(&mut self, layers: Vec<LayerRow>, active: String, has_selection: bool) {
        if self.layers != layers || self.active != active || self.has_selection != has_selection {
            self.layers = layers;
            self.active = active;
            self.has_selection = has_selection;
            self.invalidate();
        }
    }

    fn hit(&self, x: f32, y: f32) -> Option<Hit> {
        hit(self.layers.len(), self.has_selection, self.size().width, x, y)
    }

    fn command(&mut self, command: &str, layer: Option<usize>) {
        let (id, name) = layer.and_then(|i| self.layers.get(i)).map(|l| (l.id.clone(), l.name.clone())).unwrap_or_default();
        self.raise_layers_command(LayersCommandEventArgs { command: command.to_string(), id, name });
    }

    fn part_name(&self, i: usize, part: RowPart) -> String {
        let Some(l) = self.layers.get(i) else { return String::new() };
        match part {
            RowPart::Visible => if l.visible { format!("Masquer {}", l.name) } else { format!("Afficher {}", l.name) },
            RowPart::Lock => if l.locked { format!("Déverrouiller {}", l.name) } else { format!("Verrouiller {}", l.name) },
            RowPart::Name => l.name.clone(),
            RowPart::Up => format!("Monter {}", l.name),
            RowPart::Down => format!("Descendre {}", l.name),
            RowPart::Delete => format!("Supprimer {}", l.name),
        }
    }
}

impl Control for LayerList {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        let footer = if self.has_selection { ROW_H + 1.0 } else { 0.0 };
        Size { width: 240.0, height: ROW_H + 1.0 + self.layers.len() as f32 * ROW_H + footer }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let c: &dyn Canvas = e.graphics;
        let t = c.theme();
        let w = b.right - b.left;
        let at = |r: Rect| Rect::new(b.left + r.left, b.top + r.top, b.left + r.right, b.top + r.bottom);
        let icon = |name: &'static str, r: Rect, ink: &D2D1_COLOR_F| {
            let (cx, cy) = ((r.left + r.right) / 2.0, (r.top + r.bottom) / 2.0);
            c.vector_icon(name, &Rect::new(cx - 6.5, cy - 6.5, cx + 6.5, cy + 6.5), 13.0, ink);
        };
        c.fill_rounded(&b, 0.0, &t.card_background);
        // Header.
        c.text("Calques", &at(Rect::new(PAD, 0.0, w - PAD - ICON_W, ROW_H)), &c.formats().caption_strong, &t.text_secondary, false);
        let add = at(header_add(w));
        if self.hot == Some(Hit::Add) {
            c.fill_rounded(&add, 4.0, &t.surface_2);
        }
        let (cx, cy) = ((add.left + add.right) / 2.0, (add.top + add.bottom) / 2.0);
        c.vector_icon("Plus", &Rect::new(cx - 7.0, cy - 7.0, cx + 7.0, cy + 7.0), 14.0, &t.text_secondary);
        c.fill_rounded(&at(Rect::new(0.0, ROW_H, w, ROW_H + 1.0)), 0.0, &t.divider);
        // Rows.
        for (i, l) in self.layers.iter().enumerate() {
            let row = row_rect(i, w);
            let active = l.id == self.active;
            let hot_row = matches!(self.hot, Some(Hit::Row(j, _)) if j == i);
            if active {
                c.fill_rounded(&at(row), 0.0, &with_alpha(t.accent, 0.05));
            } else if hot_row {
                c.fill_rounded(&at(row), 0.0, &t.row_hover);
            }
            for (part, r) in row_parts(row) {
                let r = at(r);
                let hot = self.hot == Some(Hit::Row(i, part)) && part != RowPart::Name && part_enabled(part, i, self.layers.len());
                let ink = match part {
                    _ if !part_enabled(part, i, self.layers.len()) => with_alpha(t.text_tertiary, 0.3),
                    RowPart::Delete if hot => t.danger,
                    _ if hot => t.text_primary,
                    _ => t.text_tertiary,
                };
                match part {
                    RowPart::Visible => icon(if l.visible { "Eye" } else { "EyeOff" }, r, &ink),
                    RowPart::Lock => icon(if l.locked { "Lock" } else { "LockOpen" }, r, &ink),
                    RowPart::Name => {
                        // `text-xs text-text-primary`, active or not (the row's tint marks the active one).
                        c.text_ellipsis(&l.name, &r, &c.formats().caption, &t.text_primary);
                    }
                    RowPart::Up => icon("ChevronUp", r, &ink),
                    RowPart::Down => icon("ChevronDown", r, &ink),
                    RowPart::Delete => icon("Trash2", r, &ink),
                }
            }
        }
        // Footer.
        if self.has_selection {
            let f = at(footer_rect(self.layers.len(), w));
            c.fill_rounded(&Rect::new(f.left, f.top, f.right, f.top + 1.0), 0.0, &t.divider);
            if self.hot == Some(Hit::MoveHere) {
                c.fill_rounded(&Rect::new(f.left, f.top + 1.0, f.right, f.bottom), 0.0, &with_alpha(t.accent, 0.05));
            }
            c.text("Déplacer la sélection ici", &Rect::new(f.left + PAD, f.top + 1.0, f.right - PAD, f.bottom), &c.formats().caption, &t.accent, false);
        }
        e.raise(self, "OnPaint");
    }

    fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
        match self.hit(x, y)? {
            Hit::Add => Some("Ajouter un calque".to_string()),
            Hit::MoveHere => None,
            Hit::Row(_, RowPart::Name) => Some("Double-cliquez pour renommer".to_string()),
            Hit::Row(_, RowPart::Visible) => Some("Visibilité".to_string()),
            Hit::Row(_, RowPart::Lock) => Some("Verrouiller".to_string()),
            Hit::Row(_, RowPart::Up) => Some("Monter".to_string()),
            Hit::Row(_, RowPart::Down) => Some("Descendre".to_string()),
            Hit::Row(_, RowPart::Delete) => Some("Supprimer".to_string()),
        }
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let w = self.size().width;
        let mut parts = vec![AccessiblePart { name: "Ajouter un calque".to_string(), role: AccessRole::Button, bounds: header_add(w) }];
        for i in 0..self.layers.len() {
            for (part, r) in row_parts(row_rect(i, w)) {
                let role = if part == RowPart::Name { AccessRole::ListItem } else { AccessRole::Button };
                parts.push(AccessiblePart { name: self.part_name(i, part), role, bounds: r });
            }
        }
        if self.has_selection {
            parts.push(AccessiblePart { name: "Déplacer la sélection ici".to_string(), role: AccessRole::Button, bounds: footer_rect(self.layers.len(), w) });
        }
        parts
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            let h = self.hit(e.args().x, e.args().y);
            if e.args().clicks >= 2 {
                self.pressed = None;
                if let Some(Hit::Row(i, RowPart::Name)) = h {
                    self.command("rename", Some(i));
                }
            } else {
                self.pressed = h;
            }
            self.invalidate();
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let h = self.hit(e.args().x, e.args().y);
        if let Some(p) = self.pressed.take().filter(|p| Some(*p) == h) {
            match p {
                Hit::Add => self.command("add", None),
                Hit::MoveHere => {
                    let active = self.layers.iter().position(|l| l.id == self.active);
                    self.command("move_here", active);
                }
                Hit::Row(i, part) if !part_enabled(part, i, self.layers.len()) => {}
                Hit::Row(i, part) => self.command(part.command(), Some(i)),
            }
        }
        self.invalidate();
        e.raise(&*self, "OnMouseUp");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let h = self.hit(e.args().x, e.args().y);
        if h != self.hot {
            self.hot = h;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ends_of_the_list_cannot_move_further_and_the_last_layer_stays() {
        // Rows are top first: the top row cannot go up, the bottom one cannot go down (`disabled={idx === …}`).
        assert!(!part_enabled(RowPart::Up, 0, 3));
        assert!(part_enabled(RowPart::Down, 0, 3));
        assert!(part_enabled(RowPart::Up, 2, 3));
        assert!(!part_enabled(RowPart::Down, 2, 3));
        assert!(!part_enabled(RowPart::Delete, 0, 1));
        assert!(part_enabled(RowPart::Delete, 0, 2));
        assert!(part_enabled(RowPart::Name, 0, 1));
    }

    #[test]
    fn rows_follow_the_header_and_the_footer_follows_the_rows() {
        assert_eq!(row_rect(0, 240.0).top, ROW_H + 1.0);
        assert_eq!(row_rect(2, 240.0).top, ROW_H + 1.0 + 2.0 * ROW_H);
        assert_eq!(footer_rect(3, 240.0).top, row_rect(2, 240.0).bottom);
    }

    #[test]
    fn the_parts_of_a_row_are_hit_in_order() {
        let row = row_rect(1, 240.0);
        let y = (row.top + row.bottom) / 2.0;
        for (part, r) in row_parts(row) {
            assert_eq!(hit(3, false, 240.0, (r.left + r.right) / 2.0, y), Some(Hit::Row(1, part)));
        }
        assert_eq!(hit(3, false, 240.0, 230.0, 14.0), Some(Hit::Add));
    }

    #[test]
    fn the_footer_only_exists_with_a_selection() {
        let f = footer_rect(2, 240.0);
        let y = (f.top + f.bottom) / 2.0;
        assert_eq!(hit(2, false, 240.0, 50.0, y), None);
        assert_eq!(hit(2, true, 240.0, 50.0, y), Some(Hit::MoveHere));
    }

    #[test]
    fn row_parts_map_to_the_window_commands() {
        let cmds: Vec<&str> = [RowPart::Visible, RowPart::Lock, RowPart::Name, RowPart::Up, RowPart::Down, RowPart::Delete].iter().map(|p| p.command()).collect();
        assert_eq!(cmds, ["visible", "lock", "activate", "up", "down", "delete"]);
    }
}
