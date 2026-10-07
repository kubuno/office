//! The grid's viewport (`SpreadsheetApp.tsx`: the scrolling `div` around the canvas): the scroll offsets in
//! content coordinates (headers included, as the web's `scrollLeft`/`scrollTop`), the dynamic scroll extent
//! that grows as the user goes further (`extentRows`/`extentCols`, `trackViewEnd`), keeping the active
//! cell in view past the frozen bands (`doScrollIntoView`), and the visible ranges of the four freeze
//! quadrants the painter draws (`drawGrid`).

use kubuno_office_sheets_core::addr::{MAX_COLS, MAX_ROWS};
use kubuno_office_sheets_core::editor::Editor;
use kubuno_office_sheets_core::grid::{self, Geometry, COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};

/// One block of cells to paint: columns `c0..=cn`, rows `r0..=rn`, offset by `(off_x, off_y)` (the scroll
/// it follows) and clipped to `clip` (left, top, right, bottom) in viewport coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quadrant {
    pub c0: u32,
    pub cn: u32,
    pub r0: u32,
    pub rn: u32,
    pub off_x: f64,
    pub off_y: f64,
    pub clip: (f64, f64, f64, f64),
}

/// The viewport.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridView {
    pub scroll_x: f64,
    pub scroll_y: f64,
    /// The size of the grid area (headers included), in DIP.
    pub width: f64,
    pub height: f64,
    /// The last row and column in view (`viewEnd`): the extent stays a margin beyond them.
    pub view_end: (u32, u32),
}

impl GridView {
    /// The scrollable size (`colLeft[extentCols]` × `rowYof(extentRows + 1)`).
    pub fn content_size(&self, ed: &Editor, g: &Geometry) -> (f64, f64) {
        let (cols, rows) = grid::extent(ed.sheet().used_extent(), (ed.sel.active.col, ed.sel.active.row), self.view_end);
        (g.col_left(cols), g.row_top(rows + 1))
    }

    /// The largest scroll offsets.
    pub fn max_scroll(&self, ed: &Editor, g: &Geometry) -> (f64, f64) {
        let (w, h) = self.content_size(ed, g);
        ((w - self.width).max(0.0), (h - self.height).max(0.0))
    }

    /// Keeps the offsets within the content, then updates `view_end` (`trackViewEnd`).
    pub fn clamp(&mut self, ed: &Editor, g: &Geometry) {
        let (mx, my) = self.max_scroll(ed, g);
        self.scroll_x = self.scroll_x.clamp(0.0, mx);
        self.scroll_y = self.scroll_y.clamp(0.0, my);
        let row = g.row_at_y(self.scroll_y + self.height).unwrap_or(MAX_ROWS);
        let col = g.col_at_x(self.scroll_x + self.width).unwrap_or(MAX_COLS - 1);
        self.view_end = (col, row);
    }

    /// Scrolls by `(dx, dy)` DIP.
    pub fn scroll_by(&mut self, dx: f64, dy: f64, ed: &Editor, g: &Geometry) {
        self.scroll_x += dx;
        self.scroll_y += dy;
        // Let the extent grow before clamping (scrolling into the void extends it, as on the web).
        let (cw, ch) = self.content_size(ed, g);
        self.scroll_x = self.scroll_x.clamp(0.0, (cw - self.width).max(0.0));
        self.scroll_y = self.scroll_y.clamp(0.0, (ch - self.height).max(0.0));
        self.clamp(ed, g);
    }

    /// `doScrollIntoView(c, r)`: the minimal scroll that shows the cell beyond the frozen bands.
    pub fn scroll_into_view(&mut self, ed: &Editor, g: &Geometry, c: u32, r: u32) {
        let s = ed.sheet();
        let fx = g.col_left(s.frozen_cols().min(MAX_COLS));
        let fy = g.row_top(s.frozen_rows().min(MAX_ROWS) + 1);
        let (cx, cw) = (g.col_left(c), g.col_width(c));
        let (cy, ch) = (g.row_top(r), g.row_height(r));
        if cx < self.scroll_x + fx {
            self.scroll_x = (cx - fx).max(0.0);
        } else if cx + cw > self.scroll_x + self.width {
            self.scroll_x = cx + cw - self.width;
        }
        if cy < self.scroll_y + fy {
            self.scroll_y = (cy - fy).max(0.0);
        } else if cy + ch > self.scroll_y + self.height {
            self.scroll_y = cy + ch - self.height;
        }
        self.clamp(ed, g);
    }

    /// The four freeze quadrants of `drawGrid`: body, frozen top, frozen left, corner (those that exist).
    pub fn quadrants(&self, ed: &Editor, g: &Geometry) -> Vec<Quadrant> {
        let s = ed.sheet();
        let f_cols = s.frozen_cols().min(MAX_COLS);
        let f_rows = s.frozen_rows().min(MAX_ROWS);
        let freeze_x = g.col_left(f_cols);
        let freeze_y = g.row_top(f_rows + 1);
        let (sl, st, vw, vh) = (self.scroll_x, self.scroll_y, self.width, self.height);
        let sc0 = g.col_at_x(sl + freeze_x).unwrap_or(f_cols).max(f_cols);
        let scn = g.col_at_x(sl + vw - 1.0).unwrap_or(MAX_COLS - 1);
        let sr0 = g.row_at_y(st + freeze_y).unwrap_or(f_rows + 1).max(f_rows + 1);
        let srn = g.row_at_y(st + vh - 1.0).unwrap_or(MAX_ROWS);
        let mut q = vec![Quadrant { c0: sc0, cn: scn, r0: sr0, rn: srn, off_x: sl, off_y: st, clip: (freeze_x, freeze_y, vw, vh) }];
        if f_rows > 0 {
            q.push(Quadrant { c0: sc0, cn: scn, r0: 1, rn: f_rows, off_x: sl, off_y: 0.0, clip: (freeze_x, COL_HEADER_HEIGHT, vw, freeze_y) });
        }
        if f_cols > 0 {
            q.push(Quadrant { c0: 0, cn: f_cols - 1, r0: sr0, rn: srn, off_x: 0.0, off_y: st, clip: (ROW_HEADER_WIDTH, freeze_y, freeze_x, vh) });
        }
        if f_cols > 0 && f_rows > 0 {
            q.push(Quadrant { c0: 0, cn: f_cols - 1, r0: 1, rn: f_rows, off_x: 0.0, off_y: 0.0, clip: (ROW_HEADER_WIDTH, COL_HEADER_HEIGHT, freeze_x, freeze_y) });
        }
        q.retain(|q| q.c0 <= q.cn && q.r0 <= q.rn && q.clip.2 > q.clip.0 && q.clip.3 > q.clip.1);
        q
    }

    /// The screen x of content x `cx` for a column (frozen columns do not scroll).
    pub fn screen_x(&self, ed: &Editor, g: &Geometry, c: u32) -> f64 {
        let x = g.col_left(c);
        if c < ed.sheet().frozen_cols() {
            x
        } else {
            x - self.scroll_x
        }
    }

    /// The screen y of a row's top (frozen rows do not scroll).
    pub fn screen_y(&self, ed: &Editor, g: &Geometry, r: u32) -> f64 {
        let y = g.row_top(r);
        if r <= ed.sheet().frozen_rows() {
            y
        } else {
            y - self.scroll_y
        }
    }

    /// The cell under a viewport point, honouring the frozen bands; `None` over the headers.
    pub fn cell_at(&self, ed: &Editor, g: &Geometry, x: f64, y: f64) -> Option<(u32, u32)> {
        if x < ROW_HEADER_WIDTH || y < COL_HEADER_HEIGHT {
            return None;
        }
        let s = ed.sheet();
        let freeze_x = g.col_left(s.frozen_cols().min(MAX_COLS));
        let freeze_y = g.row_top(s.frozen_rows().min(MAX_ROWS) + 1);
        let cx = if x < freeze_x { x } else { x + self.scroll_x };
        let cy = if y < freeze_y { y } else { y + self.scroll_y };
        Some((g.col_at_x(cx)?, g.row_at_y(cy)?))
    }

    /// The column whose header is under viewport x.
    pub fn col_at(&self, ed: &Editor, g: &Geometry, x: f64) -> Option<u32> {
        let freeze_x = g.col_left(ed.sheet().frozen_cols().min(MAX_COLS));
        g.col_at_x(if x < freeze_x { x } else { x + self.scroll_x })
    }

    /// The row whose header is under viewport y.
    pub fn row_at(&self, ed: &Editor, g: &Geometry, y: f64) -> Option<u32> {
        let freeze_y = g.row_top(ed.sheet().frozen_rows().min(MAX_ROWS) + 1);
        g.row_at_y(if y < freeze_y { y } else { y + self.scroll_y })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_office_sheets_core::addr::Cell as Pos;
    use std::rc::Rc;

    fn editor() -> Editor {
        Editor::new(crate::sample::workbook(), Rc::new(crate::platform::SystemEnv::default()))
    }

    #[test]
    fn the_active_cell_is_kept_in_view_past_the_frozen_rows() {
        let mut ed = editor();
        let g = ed.geometry();
        let mut v = GridView { width: 800.0, height: 400.0, ..GridView::default() };
        v.clamp(&ed, &g);
        ed.select(Pos::new(0, 100));
        v.scroll_into_view(&ed, &g, 0, 100);
        let y = v.screen_y(&ed, &g, 100);
        assert!(y + g.row_height(100) <= 400.0 + 0.01 && y >= 0.0, "{y}");
        // Frozen rows 1–2 stay at the top whatever the scroll.
        assert_eq!(v.screen_y(&ed, &g, 1), COL_HEADER_HEIGHT);
        let q = v.quadrants(&ed, &g);
        assert_eq!(q.len(), 2);
        assert_eq!((q[1].r0, q[1].rn), (1, 2));
        assert!(q[0].r0 >= 3);
    }

    #[test]
    fn hit_testing_follows_the_scroll() {
        let ed = editor();
        let g = ed.geometry();
        let mut v = GridView { width: 800.0, height: 400.0, ..GridView::default() };
        v.clamp(&ed, &g);
        assert_eq!(v.cell_at(&ed, &g, 50.0, 30.0), Some((0, 1)));
        assert_eq!(v.cell_at(&ed, &g, 10.0, 30.0), None);
        v.scroll_by(0.0, 240.0, &ed, &g);
        let (_, r) = v.cell_at(&ed, &g, 50.0, 400.0).expect("a cell");
        assert!(r > 10);
    }
}
