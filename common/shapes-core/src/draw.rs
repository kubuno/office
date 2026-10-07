//! Draw-to-create: the rubber-band box between the gesture's start and the pointer (`shapes/draw.ts`).

use crate::path::Rect;

/// `drawBoxFrom`: always normalised; `square` (Shift) keeps the longer side, `from_centre` (Alt) grows
/// from the start point as the centre.
pub fn draw_box_from(start_x: f64, start_y: f64, cur_x: f64, cur_y: f64, square: bool, from_centre: bool) -> Rect {
    let mut dx = cur_x - start_x;
    let mut dy = cur_y - start_y;
    if square {
        let m = dx.abs().max(dy.abs());
        dx = m * if dx < 0.0 { -1.0 } else { 1.0 };
        dy = m * if dy < 0.0 { -1.0 } else { 1.0 };
    }
    if from_centre {
        return Rect { x: start_x - dx.abs(), y: start_y - dy.abs(), w: dx.abs() * 2.0, h: dy.abs() * 2.0 };
    }
    Rect { x: start_x.min(start_x + dx), y: start_y.min(start_y + dy), w: dx.abs(), h: dy.abs() }
}

/// `finalizeDrawBox`: the drawn box when the user really dragged, else a `default_size` box centred on
/// the click.
pub fn finalize_draw_box(kind: &str, b: Rect, start_x: f64, start_y: f64, min_size: f64, default_size: Option<(f64, f64)>) -> Rect {
    if b.w >= min_size && b.h >= min_size {
        return b;
    }
    let (w, h) = default_size.unwrap_or_else(|| crate::catalog::shape_default_size(kind));
    Rect { x: start_x - w / 2.0, y: start_y - h / 2.0, w, h }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_normalises_squares_and_centres() {
        assert_eq!(draw_box_from(10.0, 10.0, 0.0, 30.0, false, false), Rect { x: 0.0, y: 10.0, w: 10.0, h: 20.0 });
        assert_eq!(draw_box_from(0.0, 0.0, 5.0, -20.0, true, false), Rect { x: 0.0, y: -20.0, w: 20.0, h: 20.0 });
        assert_eq!(draw_box_from(10.0, 10.0, 12.0, 13.0, false, true), Rect { x: 8.0, y: 7.0, w: 4.0, h: 6.0 });
        let click = finalize_draw_box("rect", Rect { x: 0.5, y: 0.5, w: 0.0, h: 0.0 }, 0.5, 0.5, 0.02, Some((0.2, 0.15)));
        assert!((click.x - 0.4).abs() < 1e-12 && (click.w - 0.2).abs() < 1e-12);
    }
}
