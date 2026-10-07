//! `Swatch` — the small colour chips of the formatting dialogs (`ConditionalFormatDialog.tsx`): a rule's
//! swatch in the rule list, the colour-scale and data-bar choices, the « 123 » style presets and the B / I
//! toggles. What it paints is a short spec, so a `.kbview` binds it:
//!
//! - `solid:#ffc7ce` — a fill; `none` — no fill;
//! - `grad:#57bb8a,#ffd666,#e67c73` — a left-to-right gradient;
//! - `bar:#638ec6` — the rule list's bar over 60 % of the width; `barchip:#638ec6` — a bar choice
//!   (the colour fading to 33 % from 70 %);
//! - `icons` — red / amber / green thirds;
//! - `sample:#bg:#fg:b` — « 123 » on the fill (`b`: bold), a style preset;
//! - `glyph:B` / `glyph:I` — the bold / italic toggles.
//!
//! `Selected` draws the primary ring (`border-primary ring-1`). A click raises `OnClick`.


use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, GradientStop, LinearGradientBrush, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{Component, Control, ControlCore, EventCx, PaintEventCx};

/// The chip (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Spreadsheets")]
#[toolbox(icon = "palette")]
#[default_event("Click")]
pub struct Swatch {
    base: ControlCore,
    /// What it paints (see the module doc).
    #[property(bindable)]
    #[category("Appearance")]
    pub spec: String,
    /// Draws the selection ring.
    #[property(bindable)]
    #[category("Appearance")]
    pub selected: bool,
    hot: bool,
}

impl Default for Swatch {
    fn default() -> Self {
        Self { base: ControlCore::default(), spec: "solid:#ffc7ce".into(), selected: false, hot: false }
    }
}

fn hex(s: &str) -> Color {
    Color::from_hex(s.trim()).unwrap_or(Color::TRANSPARENT)
}

impl Control for Swatch {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
        Size { width: proposed.width.max(24.0), height: proposed.height.max(20.0) }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let g = e.graphics;
        let c: &dyn Canvas = e.graphics;
        let t = c.theme();
        let radius = 4.0;
        let inner = Rect::new(b.left + 0.5, b.top + 0.5, b.right - 0.5, b.bottom - 0.5);
        let saved = g.save();
        g.intersect_clip(b);
        let (kind, rest) = self.spec.split_once(':').unwrap_or((self.spec.as_str(), ""));
        let border = if self.selected { Color::from(t.accent) } else { Color::from(t.card_stroke) };
        match kind {
            "solid" => g.fill_rounded_rectangle(Brush::solid(hex(rest)), inner, radius),
            "grad" => {
                let stops: Vec<Color> = rest.split(',').map(hex).collect();
                if stops.len() >= 2 {
                    let n = (stops.len() - 1) as f32;
                    let pairs: Vec<GradientStop> = stops.iter().enumerate().map(|(i, c)| GradientStop::new(i as f32 / n, *c)).collect();
                    let brush = LinearGradientBrush::new(PointF::new(inner.left, 0.0), PointF::new(inner.right, 0.0), stops[0], stops[stops.len() - 1]).with_stops(&pairs);
                    g.fill_rounded_rectangle(brush, inner, radius);
                }
            }
            "bar" => {
                let w = (inner.right - inner.left) * 0.6;
                g.fill_rectangle(Brush::solid(hex(rest)), Rect::new(inner.left, inner.top, inner.left + w, inner.bottom));
            }
            "barchip" => {
                let c0 = hex(rest);
                let faded = c0.with_alpha(0x55 as f32 / 255.0);
                let brush = LinearGradientBrush::new(PointF::new(inner.left, 0.0), PointF::new(inner.right, 0.0), c0, faded).with_stops(&[GradientStop::new(0.0, c0), GradientStop::new(0.7, c0), GradientStop::new(1.0, faded)]);
                g.fill_rounded_rectangle(brush, inner, radius);
            }
            "icons" => {
                let w = (inner.right - inner.left) / 3.0;
                for (i, col) in ["#d93025", "#f9ab00", "#188038"].iter().enumerate() {
                    let l = inner.left + w * i as f32;
                    g.fill_rectangle(Brush::solid(hex(col)), Rect::new(l, inner.top, l + w, inner.bottom));
                }
            }
            "sample" => {
                let mut parts = rest.split(':');
                let bg = parts.next().filter(|s| !s.is_empty()).unwrap_or("#ffffff");
                let fg = parts.next().filter(|s| !s.is_empty()).unwrap_or("#000000");
                let bold = parts.next() == Some("b");
                g.fill_rounded_rectangle(Brush::solid(hex(bg)), inner, radius);
                let font = Font::with_dip("Segoe UI", 12.0, if bold { FontStyle::BOLD } else { FontStyle::REGULAR });
                let fmt = StringFormat { alignment: StringAlignment::Center, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP };
                g.draw_string("123", &font, Brush::solid(hex(fg)), b, &fmt);
            }
            "glyph" => {
                if self.selected {
                    g.fill_rounded_rectangle(Brush::solid(Color::from(t.accent).with_alpha(0.12)), inner, radius);
                }
                let (bold, italic) = (rest == "B", rest == "I");
                let mut style = FontStyle::REGULAR.0;
                if bold {
                    style |= FontStyle::BOLD.0;
                }
                if italic {
                    style |= FontStyle::ITALIC.0;
                }
                let font = Font::with_dip("Segoe UI", 12.0, FontStyle(style));
                let fmt = StringFormat { alignment: StringAlignment::Center, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP };
                g.draw_string(rest, &font, Brush::solid(Color::from(t.text_primary)), b, &fmt);
            }
            _ => {}
        }
        if kind != "bar" && kind != "icons" || self.selected {
            g.draw_rounded_rectangle(&Pen::new(border, if self.selected { 2.0 } else { 1.0 }), inner, radius);
        } else {
            g.draw_rounded_rectangle(&Pen::new(border, 1.0), inner, radius);
        }
        if self.hot && !self.design_mode() && kind != "bar" && kind != "icons" {
            g.fill_rounded_rectangle(Brush::solid(Color::rgba_f(0.0, 0.0, 0.0, 0.04)), inner, radius);
        }
        g.restore(saved);
        e.raise(self, "OnPaint");
    }

    fn on_mouse_enter(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        self.hot = true;
        self.invalidate();
        e.raise(&*self, "OnMouseEnter");
    }

    fn on_mouse_leave(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        self.hot = false;
        self.invalidate();
        e.raise(&*self, "OnMouseLeave");
    }
}

/// The swatch spec of a CF block in the rule list (`swatch(b)`).
pub fn spec_of(s: &kubuno_office_sheets_core::dialogs::Swatch) -> String {
    use kubuno_office_sheets_core::dialogs::Swatch as S;
    match s {
        S::None => "none".into(),
        S::Solid(c) => format!("solid:{c}"),
        S::Gradient(stops) => format!("grad:{}", stops.join(",")),
        S::Bar(c) => format!("bar:{c}"),
        S::Icons => "icons".into(),
    }
}
