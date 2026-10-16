//! The drawing vocabulary of the resources workspace: fonts, text, the `@ui` button / input / chip shapes the
//! web's sub-views are made of, avatars and kind badges - in the plan views' palette (`model::Palette`).

use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Graphics, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::Rect;
use kubuno_office_projects_common::model::line_edit::LineEdit;
use kubuno_office_projects_core::model::Resource;
use kubuno_office_projects_core::resources::KindMeta;
use kubuno_office_projects_core::views::initial;

use crate::controls::model::{hex, Palette};

const FACE: &str = "Segoe UI";

pub fn font(size: f32, bold: bool) -> Font {
    Font::with_dip(FACE, size, if bold { FontStyle::BOLD } else { FontStyle::REGULAR })
}

pub fn italic(size: f32) -> Font {
    Font::with_dip(FACE, size, FontStyle::ITALIC)
}

/// One line, vertically centred, ellipsis.
pub fn one(align: StringAlignment) -> StringFormat {
    StringFormat { alignment: align, line_alignment: StringAlignment::Center, trimming: StringTrimming::EllipsisCharacter, flags: StringFormatFlags::NO_WRAP }
}

/// Wrapped text from the top.
pub fn wrapped(align: StringAlignment) -> StringFormat {
    StringFormat { alignment: align, line_alignment: StringAlignment::Near, trimming: StringTrimming::None, flags: StringFormatFlags::default() }
}

pub fn text_w(g: &Graphics<'_>, s: &str, f: &Font) -> f32 {
    g.measure_string(s, f, None, &StringFormat { flags: StringFormatFlags::NO_WRAP | StringFormatFlags::MEASURE_TRAILING_SPACES, ..one(StringAlignment::Near) }).width
}

/// The height `s` takes wrapped in `w`.
pub fn text_h(g: &Graphics<'_>, s: &str, f: &Font, w: f32) -> f32 {
    g.measure_string(s, f, Some(w), &wrapped(StringAlignment::Near)).height
}

pub fn text(g: &Graphics<'_>, s: &str, f: &Font, c: Color, r: Rect, align: StringAlignment) {
    g.draw_string(s, f, Brush::solid(c), r, &one(align));
}

pub fn hline(g: &Graphics<'_>, c: Color, x0: f32, x1: f32, y: f32) {
    g.draw_line(&Pen::new(c, 1.0), PointF::new(x0, y - 0.5), PointF::new(x1, y - 0.5));
}

pub fn vline(g: &Graphics<'_>, c: Color, x: f32, y0: f32, y1: f32) {
    g.draw_line(&Pen::new(c, 1.0), PointF::new(x - 0.5, y0), PointF::new(x - 0.5, y1));
}

/// A rounded box with a 1-px border.
pub fn card(g: &Graphics<'_>, r: Rect, fill: Color, border: Color, radius: f32) {
    g.fill_rounded_rectangle(Brush::solid(fill), r, radius);
    g.draw_rounded_rectangle(&Pen::new(border, 1.0), Rect::new(r.left + 0.5, r.top + 0.5, r.right - 0.5, r.bottom - 0.5), radius);
}

/// `#rrggbb`, or the theme's primary for `primary`.
pub fn tint_of(m: &KindMeta, th: &Palette) -> Color {
    if m.tint == "primary" {
        th.primary
    } else {
        Color::from_hex(m.tint).unwrap_or(th.text_secondary)
    }
}

/// `color-mix(in srgb, c p%, transparent)`.
pub fn wash(c: Color, p: f32) -> Color {
    c.with_alpha(p)
}

/// The `@ui` Button: `primary`, `secondary` (bordered) or `ghost`; an optional lucide icon before the label.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Secondary,
    Ghost,
}

/// The width a button needs (`px-4`, icon 15 + `gap-2`, 14-px label; an icon alone is square).
pub fn button_w(g: &Graphics<'_>, label: &str, icon: bool, h: f32) -> f32 {
    if label.is_empty() {
        return h;
    }
    16.0 + if icon { 15.0 + 8.0 } else { 0.0 } + text_w(g, label, &font(14.0, true)) + 16.0
}

#[allow(clippy::too_many_arguments)]
pub fn button(g: &Graphics<'_>, th: &Palette, r: Rect, label: &str, icon: Option<&'static str>, v: Variant, enabled: bool, hot: bool) {
    let (fill, ink, border) = match v {
        Variant::Primary => (if hot && enabled { th.primary.lerp(Color::BLACK, 0.12) } else { th.primary }, if th.dark { hex(0x202124) } else { Color::WHITE }, None),
        Variant::Secondary => (if hot && enabled { th.surface2 } else { th.surface0 }, th.text_primary, Some(th.border)),
        Variant::Ghost => (if hot && enabled { th.surface2 } else { Color::TRANSPARENT }, th.text_secondary, None),
    };
    let alpha = if enabled { 1.0 } else { 0.5 };
    g.fill_rounded_rectangle(Brush::solid(fill.with_alpha(fill.a * alpha)), r, 8.0);
    if let Some(b) = border {
        g.draw_rounded_rectangle(&Pen::new(b, 1.0), Rect::new(r.left + 0.5, r.top + 0.5, r.right - 0.5, r.bottom - 0.5), 8.0);
    }
    let ink = ink.with_alpha(alpha);
    let f = font(14.0, true);
    if label.is_empty() {
        if let Some(i) = icon {
            g.draw_icon(i, r, 16.0, ink);
        }
        return;
    }
    let tw = text_w(g, label, &f);
    let iw = if icon.is_some() { 15.0 + 8.0 } else { 0.0 };
    let x = r.left + ((r.right - r.left) - (iw + tw)) / 2.0;
    if let Some(i) = icon {
        g.draw_icon(i, Rect::new(x, r.top, x + 15.0, r.bottom), 15.0, ink);
    }
    text(g, label, &f, ink, Rect::new(x + iw, r.top, x + iw + tw + 2.0, r.bottom), StringAlignment::Near);
}

/// What an input shows: its committed text, or the line being edited (with its caret).
pub enum InputText<'a> {
    Value(&'a str),
    Editing(&'a LineEdit, bool),
}

/// The `@ui` Input (`h-9`, 8-px radius, border, a left icon) or a borderless ghost field (`ghost`).
#[allow(clippy::too_many_arguments)]
pub fn input(g: &Graphics<'_>, th: &Palette, r: Rect, value: InputText<'_>, placeholder: &str, icon: Option<&'static str>, size: f32, align: StringAlignment, ghost: bool) {
    let focused = matches!(value, InputText::Editing(..));
    if ghost {
        if focused {
            g.fill_rounded_rectangle(Brush::solid(th.surface0), r, 4.0);
            g.draw_rounded_rectangle(&Pen::new(th.primary, 1.0), Rect::new(r.left + 0.5, r.top + 0.5, r.right - 0.5, r.bottom - 0.5), 4.0);
        }
    } else {
        g.fill_rounded_rectangle(Brush::solid(th.surface0), r, 8.0);
        let pen = Pen::new(if focused { th.primary } else { th.border }, if focused { 2.0 } else { 1.0 });
        let k = if focused { 1.0 } else { 0.5 };
        g.draw_rounded_rectangle(&pen, Rect::new(r.left + k, r.top + k, r.right - k, r.bottom - k), 8.0);
    }
    let pad = if ghost { 4.0 } else { 12.0 };
    let mut left = r.left + pad;
    if let Some(i) = icon {
        g.draw_icon(i, Rect::new(left, r.top, left + 14.0, r.bottom), 14.0, th.text_tertiary);
        left += 14.0 + 8.0;
    }
    let inner = Rect::new(left, r.top, r.right - pad, r.bottom);
    let f = font(size, false);
    match value {
        InputText::Value(s) => {
            if s.is_empty() {
                text(g, placeholder, &f, th.text_tertiary, inner, align);
            } else {
                text(g, s, &f, th.text_primary, inner, align);
            }
        }
        InputText::Editing(le, caret_on) => {
            let s = le.text();
            let tw = text_w(g, &s, &f);
            let x0 = if align == StringAlignment::Far { inner.right - tw } else { inner.left };
            let (s0, s1) = le.selection();
            if s1 > s0 {
                let (a, b) = (text_w(g, &le.prefix(s0), &f), text_w(g, &le.prefix(s1), &f));
                g.fill_rectangle(Brush::solid(th.primary.with_alpha(0.3)), Rect::new(x0 + a, r.top + 5.0, x0 + b, r.bottom - 5.0));
            }
            if s.is_empty() {
                text(g, placeholder, &f, th.text_tertiary, inner, align);
            }
            let saved = g.save();
            g.intersect_clip(Rect::new(inner.left - 2.0, r.top, inner.right + 2.0, r.bottom));
            g.draw_string(&s, &f, Brush::solid(th.text_primary), Rect::new(x0, inner.top, x0 + tw + 4.0, inner.bottom), &StringFormat { trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP | StringFormatFlags::NO_CLIP, ..one(StringAlignment::Near) });
            if caret_on {
                let cx = x0 + text_w(g, &le.prefix(le.caret), &f);
                g.draw_line(&Pen::new(th.text_primary, 1.0), PointF::new(cx, r.top + 6.0), PointF::new(cx, r.bottom - 6.0));
            }
            g.restore(saved);
        }
    }
}

/// A round avatar: the resource's colour and its initial (`r.name[0]`), white.
pub fn avatar(g: &Graphics<'_>, th: &Palette, r: &Resource, cx: f32, cy: f32, d: f32) {
    let a = Rect::new(cx - d / 2.0, cy - d / 2.0, cx + d / 2.0, cy + d / 2.0);
    g.fill_ellipse(Brush::solid(Color::from_hex(&r.color).unwrap_or(th.primary)), a);
    let i = r.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| initial(&r.name));
    let i = if i.is_empty() { "?".to_string() } else { i };
    g.draw_string(&i, &font((d * 0.4).round().max(9.0), true), Brush::solid(Color::WHITE), a, &StringFormat::centered());
}

/// The avatar, then the name over the role (`text-sm` / `text-[11px]`), in `[x, right]` around `cy`.
#[allow(clippy::too_many_arguments)]
pub fn identity(g: &Graphics<'_>, th: &Palette, r: &Resource, x: f32, right: f32, cy: f32, d: f32, name_size: f32, role_size: f32) {
    avatar(g, th, r, x + d / 2.0, cy, d);
    let tx = x + d + 10.0;
    if r.role.is_empty() {
        text(g, &r.name, &font(name_size, false), th.text_primary, Rect::new(tx, cy - 10.0, right, cy + 10.0), StringAlignment::Near);
    } else {
        text(g, &r.name, &font(name_size, false), th.text_primary, Rect::new(tx, cy - 18.0, right, cy + 1.0), StringAlignment::Near);
        text(g, &r.role, &font(role_size, false), th.text_tertiary, Rect::new(tx, cy, right, cy + 16.0), StringAlignment::Near);
    }
}

#[allow(clippy::too_many_arguments)]
/// A scroll bar's thumb over its rail (vertical when `vertical`), the plan views' style.
pub fn scroll_thumb(g: &Graphics<'_>, th: &Palette, rail: Rect, vertical: bool, offset: f32, content: f32, view: f32, hot: bool) -> Option<Rect> {
    if content <= view + 0.5 {
        return None;
    }
    let len = if vertical { rail.bottom - rail.top } else { rail.right - rail.left };
    let thumb = (len * view / content).max(24.0).min(len);
    let travel = len - thumb;
    let at = if content - view > 0.0 { offset / (content - view) * travel } else { 0.0 };
    let r = if vertical { Rect::new(rail.left + 3.0, rail.top + at, rail.right - 3.0, rail.top + at + thumb) } else { Rect::new(rail.left + at, rail.top + 3.0, rail.left + at + thumb, rail.bottom - 3.0) };
    g.fill_rounded_rectangle(Brush::solid(if hot { th.text_tertiary } else { th.text_tertiary.with_alpha(0.55) }), r, 3.0);
    Some(r)
}
