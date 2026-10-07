//! Code-behind of « Position et taille » (`geometry_dialog.kbview`, the web's `GeometryPopover`): pixels of the
//! 960×540 slide in, fractions out (`w`/`h` at least 0.01, the rotation in 0..360, the opacity in 0..1).

use kubuno_desktop::prelude::*;
use kubuno_office_slides_core::model::{Element, SLIDE_H, SLIDE_W};

#[kubuno_desktop::view("geometry_dialog.kbview")]
pub struct GeometryDialog {
    #[bind]
    gx: f64,
    #[bind]
    gy: f64,
    #[bind]
    gw: f64,
    #[bind]
    gh: f64,
    #[bind]
    grot: f64,
    #[bind]
    gop: f64,
}

/// The new box (fractions), rotation (degrees) and opacity (0..1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rotation: f64,
    pub opacity: f64,
}

impl Geometry {
    /// The web's conversions of the fields (`onUpdate({ x: n / SLIDE_W })`…).
    pub fn from_pixels(x: f64, y: f64, w: f64, h: f64, rotation: f64, percent: f64) -> Geometry {
        Geometry {
            x: x / SLIDE_W,
            y: y / SLIDE_H,
            w: (w / SLIDE_W).max(0.01),
            h: (h / SLIDE_H).max(0.01),
            rotation: ((rotation % 360.0) + 360.0) % 360.0,
            opacity: (percent / 100.0).clamp(0.0, 1.0),
        }
    }
}

impl GeometryDialog {
    pub fn new(el: &Element) -> Self {
        let mut dialog = Self {
            gx: (el.x() * SLIDE_W).round(),
            gy: (el.y() * SLIDE_H).round(),
            gw: (el.w() * SLIDE_W).round(),
            gh: (el.h() * SLIDE_H).round(),
            grot: el.rotation().round(),
            gop: (el.f_or("opacity", 1.0) * 100.0).round(),
            ..Self::default()
        };
        dialog.initialize_component();
        dialog
    }

    /// Asks; `None` when cancelled.
    pub fn ask(owner: &dyn AsForm, el: &Element) -> Option<Geometry> {
        let mut d = Self::new(el);
        (d.show_dialog(owner) == DialogResult::Ok).then(|| Geometry::from_pixels(d.gx, d.gy, d.gw, d.gh, d.grot, d.gop))
    }

    fn ok_click(&mut self) {
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }

    fn cancel_click(&mut self) {
        self.set_dialog_result(DialogResult::Cancel);
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_become_fractions_like_the_web() {
        let g = Geometry::from_pixels(96.0, 54.0, 0.0, 270.0, -90.0, 150.0);
        assert_eq!(g.x, 0.1);
        assert_eq!(g.y, 0.1);
        assert_eq!(g.w, 0.01);
        assert_eq!(g.h, 0.5);
        assert_eq!(g.rotation, 270.0);
        assert_eq!(g.opacity, 1.0);
    }
}
