//! `PresenceAvatars` - the collaborators present, as the web's `PresenceAvatarList`
//! (`web/src/collab/presence.tsx:60-86`): 28-DIP discs in the user's colour with the initials (white, 11 px
//! bold), a 2-DIP ring of the surface colour, overlapping by 8 DIP, five at most then « +N »; each disc's name
//! is its tooltip and its accessible name. Vendored from the spreadsheet app (apps never link each other).

use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{AccessiblePart, Control, ControlCore, PaintEventCx};

fn hex(c: &str) -> Color {
    Color::from_hex(c).unwrap_or(Color::BLACK)
}

const D: f32 = 28.0;
const STEP: f32 = 20.0;
const MAX: usize = 5;

/// The collaborators present (see the module doc).
#[derive(kubuno_desktop::views::component::Component, Default)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Presentations")]
#[toolbox(icon = "users")]
pub struct PresenceAvatars {
    base: ControlCore,
    /// One collaborator per line: `name|#rrggbb`.
    #[property(bindable)]
    #[category("Data")]
    pub users: String,
}

/// `initials(name)` (`presence.tsx:29-34`).
pub fn initials(name: &str) -> String {
    let parts: Vec<&str> = name.split_whitespace().collect();
    match parts.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(2).collect::<String>().to_uppercase(),
        [first, .., last] => format!("{}{}", first.chars().next().unwrap_or('?'), last.chars().next().unwrap_or('?')).to_uppercase(),
    }
}

impl PresenceAvatars {
    fn list(&self) -> Vec<(String, String)> {
        self.users
            .lines()
            .filter_map(|l| {
                let (n, c) = l.rsplit_once('|')?;
                (!n.is_empty()).then(|| (n.to_string(), c.to_string()))
            })
            .collect()
    }

    fn slots(&self) -> Vec<(Rect, String, String)> {
        let users = self.list();
        let h = self.size().height;
        let top = ((h - D) / 2.0).max(0.0);
        let mut out = Vec::new();
        for (i, (name, color)) in users.iter().take(MAX).enumerate() {
            let x = i as f32 * STEP;
            out.push((Rect::new(x, top, x + D, top + D), initials(name), format!("{name}|{color}")));
        }
        if users.len() > MAX {
            let x = MAX as f32 * STEP;
            out.push((Rect::new(x, top, x + D, top + D), format!("+{}", users.len() - MAX), String::new()));
        }
        out
    }
}

impl Control for PresenceAvatars {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
        let n = self.list().len().min(MAX + 1);
        let w = if n == 0 { 0.0 } else { STEP * (n as f32 - 1.0) + D + 4.0 };
        Size { width: w, height: proposed.height.max(D) }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let g = e.graphics;
        let c: &dyn Canvas = e.graphics;
        let ring = Color::from(c.theme().card_background);
        let font = Font::with_dip("Segoe UI", 11.0, FontStyle::BOLD);
        let fmt = StringFormat { alignment: StringAlignment::Center, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP };
        for (r, label, key) in self.slots() {
            let r = Rect::new(b.left + r.left, b.top + r.top, b.left + r.right, b.top + r.bottom);
            let fill = match key.rsplit_once('|') {
                Some((_, color)) => hex(color),
                None => Color::from_hex("#80868b").unwrap_or(Color::GRAY),
            };
            g.fill_ellipse(Brush::solid(ring), Rect::new(r.left - 2.0, r.top - 2.0, r.right + 2.0, r.bottom + 2.0));
            g.fill_ellipse(Brush::solid(fill), r);
            g.draw_string(&label, &font, Brush::solid(Color::WHITE), r, &fmt);
        }
        e.raise(self, "OnPaint");
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        self.list().into_iter().take(MAX).zip(self.slots()).map(|((name, _), (r, _, _))| AccessiblePart { name, role: AccessRole::Button, bounds: r }).collect()
    }

    fn tool_tip_at(&self, x: f32, _y: f32) -> Option<String> {
        let users = self.list();
        let i = (x / STEP).floor() as usize;
        users.get(i.min(MAX)).map(|(n, _)| n.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_like_the_web() {
        assert_eq!(initials("Martinien Olinga"), "MO");
        assert_eq!(initials("ana"), "AN");
        assert_eq!(initials("  "), "?");
        assert_eq!(initials("Jean Paul Sartre"), "JS");
    }
}

/// `colorFor(id)` of `CollaboratorsDialog.tsx`: a stable colour of the palette per user.
pub fn color_for(id: &str) -> &'static str {
    const PALETTE: [&str; 8] = ["#1a73e8", "#d93025", "#1e8e3e", "#f9ab00", "#9334e6", "#e8710a", "#12b5cb", "#d01884"];
    let mut h: u32 = 0;
    for c in id.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(c as u32);
    }
    PALETTE[(h as usize) % PALETTE.len()]
}

#[cfg(test)]
mod color_tests {
    #[test]
    fn colours_are_stable_and_from_the_palette() {
        assert_eq!(super::color_for("a"), super::color_for("a"));
        // "a" = 97 → 97 % 8 = 1.
        assert_eq!(super::color_for("a"), "#d93025");
    }
}
