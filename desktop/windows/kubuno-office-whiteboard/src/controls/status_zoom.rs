//! `StatusZoom` — the status bar's zoom cluster (`shell/StatusBar.tsx:48-69`): « − » (`px-1.5`, title « Zoom
//! arrière »), a slider over 50–200 % with a step of 10 (`w-28` inside `px-1`), « + », and « N % » (`w-14`),
//! which resets to 100 %. « − » / « + » move by 10 points (`Math.round((zoom ± 0.1) × 100) / 100`, clamped to
//! 25–300 %), the editor then clamps to its own 10–400 %. A zoom outside 50–200 % parks the thumb at the end
//! of the rail, as the web's `RangeSlider` does.

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::Color;
use kubuno_desktop::ui::range::Slider;
use kubuno_desktop::ui::{Canvas, Rect, Size, Widget, WidgetState};
use kubuno_desktop::views::component::{AccessiblePart, Control, ControlCore, EventCx, PaintEventCx};
use kubuno_desktop::views::events::{ChangeSource, Event};

/// The slider's range and step, in percent (`sliderMin`, `sliderMax`, `step`).
pub const SLIDER_MIN: f32 = 50.0;
pub const SLIDER_MAX: f32 = 200.0;
pub const SLIDER_STEP: f32 = 10.0;
/// What « − » / « + » may reach (`min = 0.25`, `max = 3`).
pub const BUTTON_MIN: f32 = 25.0;
pub const BUTTON_MAX: f32 = 300.0;
/// The parts' widths (DIP): a button is `px-1.5` around a 14 DIP glyph, the slider `w-28` + `px-1`, the
/// percentage `w-14`.
const BUTTON_W: f32 = 26.0;
const SLIDER_W: f32 = 120.0;
const PERCENT_W: f32 = 56.0;

/// One « − » (`dir` = -1) or « + » (1): `Math.round((zoom + d) * 100) / 100` in percent, clamped.
pub fn stepped(percent: f32, dir: f32) -> f32 {
    let z = percent / 100.0 + 0.1 * dir;
    ((z * 100.0).round()).clamp(BUTTON_MIN, BUTTON_MAX)
}

/// The slider's value for a zoom (clamped to its range, rounded to its step).
pub fn slider_value(percent: f32) -> f32 {
    let v = percent.clamp(SLIDER_MIN, SLIDER_MAX);
    ((v - SLIDER_MIN) / SLIDER_STEP).round() * SLIDER_STEP + SLIDER_MIN
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Minus,
    Track,
    Plus,
    Percent,
}

/// The status bar's zoom (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Whiteboard")]
#[toolbox(icon = "zoom-in")]
#[default_event("ZoomChanged")]
pub struct StatusZoom {
    base: ControlCore,
    /// The zoom, in percent.
    #[property(bindable)]
    #[category("Data")]
    pub zoom: f32,
    /// Occurs when the user changes the zoom, `e.new` in percent.
    #[event]
    #[category("Action")]
    pub zoom_changed: Event<NumericValueChangedEventArgs>,
    pressed: Option<Part>,
    hot: Option<Part>,
}

impl Default for StatusZoom {
    fn default() -> Self {
        Self { base: ControlCore::default(), zoom: 100.0, zoom_changed: Event::default(), pressed: None, hot: None }
    }
}

impl StatusZoom {
    /// The parts' rectangles in `bounds`, right-aligned.
    fn parts(bounds: Rect) -> [(Part, Rect); 4] {
        let total = 2.0 * BUTTON_W + SLIDER_W + PERCENT_W;
        let x = (bounds.right - total).max(bounds.left);
        let r = |l: f32, w: f32| Rect::new(l, bounds.top, l + w, bounds.bottom);
        [
            (Part::Minus, r(x, BUTTON_W)),
            (Part::Track, r(x + BUTTON_W, SLIDER_W)),
            (Part::Plus, r(x + BUTTON_W + SLIDER_W, BUTTON_W)),
            (Part::Percent, r(x + 2.0 * BUTTON_W + SLIDER_W, PERCENT_W)),
        ]
    }

    fn local(&self) -> Rect {
        Rect::new(0.0, 0.0, self.size().width, self.size().height)
    }

    fn part_at(&self, x: f32, y: f32) -> Option<Part> {
        Self::parts(self.local()).into_iter().find(|(_, r)| r.contains(x, y)).map(|(p, _)| p)
    }

    fn slider(&self) -> Slider {
        let mut s = Slider::new();
        s.set_minimum(SLIDER_MIN as i32);
        s.set_maximum(SLIDER_MAX as i32);
        s.set_value_clamped(slider_value(self.zoom) as i32);
        s
    }

    fn track_rect(bounds: Rect) -> Rect {
        let r = Self::parts(bounds)[1].1;
        Rect::new(r.left + 4.0, r.top, r.right - 4.0, r.bottom)
    }

    fn change(&mut self, percent: f32) {
        if (percent - self.zoom).abs() < 0.01 {
            return;
        }
        let old = self.zoom;
        self.zoom = percent;
        self.raise_zoom_changed(NumericValueChangedEventArgs::new(old, percent, ChangeSource::User));
        self.invalidate();
    }

    fn drag(&mut self, x: f32, y: f32) {
        let mut s = self.slider();
        s.drag_to(Self::track_rect(self.local()), x, y);
        self.change(slider_value(s.value() as f32));
    }

    fn percent_text(&self) -> String {
        format!("{} %", self.zoom.round() as i32)
    }
}

impl Control for StatusZoom {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, _proposed: Size) -> Size {
        Size { width: 2.0 * BUTTON_W + SLIDER_W + PERCENT_W, height: 28.0 }
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        let b = e.clip_rectangle;
        let c: &dyn Canvas = e.graphics;
        let t = c.theme();
        let dark = t.mode == kubuno_desktop::ui::ThemeMode::Dark;
        // `hover:bg-black/5`.
        let hover = if dark { Color::rgba_f(1.0, 1.0, 1.0, 0.08) } else { Color::rgba_f(0.0, 0.0, 0.0, 0.05) }.to_d2d();
        let ink = t.text_secondary;
        for (part, r) in Self::parts(b) {
            if part != Part::Track && (self.hot == Some(part) || self.pressed == Some(part)) {
                c.fill_rounded(&r, 0.0, &hover);
            }
            match part {
                Part::Minus | Part::Plus => {
                    let name = if part == Part::Minus { "Minus" } else { "Plus" };
                    let (cx, cy) = ((r.left + r.right) / 2.0, (r.top + r.bottom) / 2.0);
                    c.vector_icon(name, &Rect::new(cx - 7.0, cy - 7.0, cx + 7.0, cy + 7.0), 14.0, &ink);
                }
                Part::Track => {
                    let state = WidgetState::REST.hot(self.hot == Some(Part::Track)).pressed(self.pressed == Some(Part::Track));
                    self.slider().paint(c, Self::track_rect(b), state);
                }
                Part::Percent => c.text(&self.percent_text(), &r, &c.formats().caption, &ink, true),
            }
        }
        e.raise(self, "OnPaint");
    }

    fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
        Some(match self.part_at(x, y)? {
            Part::Minus => "Zoom arrière".to_string(),
            Part::Plus => "Zoom avant".to_string(),
            Part::Percent => "Rétablir à 100 %".to_string(),
            Part::Track => format!("Zoom {}", self.percent_text()),
        })
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        Self::parts(self.local())
            .into_iter()
            .map(|(p, r)| {
                let (name, role) = match p {
                    Part::Minus => ("Zoom arrière".to_string(), AccessRole::Button),
                    Part::Plus => ("Zoom avant".to_string(), AccessRole::Button),
                    Part::Percent => ("Rétablir à 100 %".to_string(), AccessRole::Button),
                    Part::Track => (format!("Zoom {}", self.percent_text()), AccessRole::Slider),
                };
                AccessiblePart { name, role, bounds: r }
            })
            .collect()
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        if e.args().button == MouseButton::Left {
            let (x, y) = (e.args().x, e.args().y);
            self.pressed = self.part_at(x, y);
            if self.pressed == Some(Part::Track) {
                self.drag(x, y);
            }
            self.invalidate();
        }
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (x, y) = (e.args().x, e.args().y);
        let hot = self.part_at(x, y);
        if hot != self.hot {
            self.hot = hot;
            self.invalidate();
        }
        if self.pressed == Some(Part::Track) {
            if e.args().button == MouseButton::Left {
                self.drag(x, y);
            } else {
                self.pressed = None;
            }
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let released = self.part_at(e.args().x, e.args().y);
        match self.pressed.take() {
            Some(Part::Minus) if released == Some(Part::Minus) => self.change(stepped(self.zoom, -1.0)),
            Some(Part::Plus) if released == Some(Part::Plus) => self.change(stepped(self.zoom, 1.0)),
            Some(Part::Percent) if released == Some(Part::Percent) => self.change(100.0),
            _ => {}
        }
        self.invalidate();
        e.raise(&*self, "OnMouseUp");
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
    fn the_buttons_move_ten_points_and_stop_at_25_and_300() {
        assert_eq!(stepped(100.0, 1.0), 110.0);
        assert_eq!(stepped(100.0, -1.0), 90.0);
        assert_eq!(stepped(132.0, 1.0), 142.0);
        assert_eq!(stepped(30.0, -1.0), 25.0);
        assert_eq!(stepped(295.0, 1.0), 300.0);
    }

    #[test]
    fn the_slider_covers_50_to_200_by_tens() {
        assert_eq!(slider_value(10.0), 50.0);
        assert_eq!(slider_value(400.0), 200.0);
        assert_eq!(slider_value(134.0), 130.0);
        assert_eq!(slider_value(136.0), 140.0);
    }

    #[test]
    fn the_parts_are_right_aligned_in_the_webs_order() {
        let p = StatusZoom::parts(Rect::new(0.0, 0.0, 300.0, 28.0));
        assert_eq!(p[3].1.right, 300.0);
        assert_eq!(p.map(|(k, _)| k), [Part::Minus, Part::Track, Part::Plus, Part::Percent]);
    }
}
