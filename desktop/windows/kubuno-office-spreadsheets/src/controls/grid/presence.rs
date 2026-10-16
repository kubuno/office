//! The collaborators on the grid (`SA:2855-2870`, `SA:7872-7892`, `web/src/collab/presence.tsx`): each
//! peer's active cell framed in the peer's colour with a name tag above it, and each peer's pointer as an
//! arrow with a name tag - clipped to the cells area, never over the headers.

use kubuno_desktop::ui::graphics::{Brush, Color, Font, FontStyle, Pen, PointF, StringAlignment, StringFormat, StringFormatFlags, StringTrimming};
use kubuno_desktop::ui::Rect;
use kubuno_desktop::views::component::Control as _;
use kubuno_office_sheets_core::addr;
use kubuno_office_sheets_core::editor::Editor;
use kubuno_office_sheets_core::grid::{Geometry, COL_HEADER_HEIGHT, ROW_HEADER_WIDTH};

use super::icons::hex;
use super::GridCanvas;

/// A collaborator as the grid draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct PeerMark {
    pub name: String,
    /// `#rrggbb`.
    pub color: String,
    /// The active cell (0-based column, 1-based row) on the sheet shown.
    pub cell: Option<(u32, u32)>,
    /// The pointer, in the grid's content coordinates (viewport + scroll).
    pub cursor: Option<(f64, f64)>,
}

fn tag_format() -> StringFormat {
    StringFormat { alignment: StringAlignment::Near, line_alignment: StringAlignment::Center, trimming: StringTrimming::None, flags: StringFormatFlags::NO_WRAP | StringFormatFlags::NO_CLIP }
}

impl GridCanvas {
    /// The collaborators to show (replaces the previous set).
    pub fn set_peers(&mut self, peers: Vec<PeerMark>) {
        if self.peers != peers {
            self.peers = peers;
            self.invalidate();
        }
    }

    /// The scroll offsets: the window turns a pointer position into content coordinates with them
    /// (`e.clientX - r.left + el.scrollLeft`, `SA:668-672`).
    pub fn scroll_offset(&self) -> (f64, f64) {
        (self.view.scroll_x, self.view.scroll_y)
    }

    /// Paints the peers' selections and pointers (after the cells, inside the grid's clip).
    pub(super) fn paint_peers(&self, g: &kubuno_desktop::ui::graphics::Graphics<'_>, ed: &Editor, geom: &Geometry) {
        if self.peers.is_empty() {
            return;
        }
        let (ox, oy) = (self.bounds.left, self.bounds.top);
        let cells_area = Rect::new(ox + ROW_HEADER_WIDTH as f32, oy + COL_HEADER_HEIGHT as f32, self.bounds.right, self.bounds.bottom);
        let saved = g.save();
        g.intersect_clip(cells_area);
        let tag_font = Font::with_dip("Segoe UI", 10.0, FontStyle::BOLD);
        let fmt = tag_format();
        for p in &self.peers {
            let c = hex(&p.color);
            if let Some((col, row)) = p.cell {
                if col < addr::MAX_COLS && (1..=addr::MAX_ROWS).contains(&row) {
                    let x = self.view.screen_x(ed, geom, col) as f32 + ox;
                    let y = self.view.screen_y(ed, geom, row) as f32 + oy;
                    let w = geom.col_width(col) as f32;
                    let h = geom.row_height(row) as f32;
                    // `border: 2px solid`, box-sizing border-box.
                    g.draw_rectangle(&Pen::new(c, 2.0), Rect::new(x + 1.0, y + 1.0, x + w - 1.0, y + h - 1.0));
                    // The name tag: top -15, left -2, 10px bold white on the colour, radius 3, padding 0 4.
                    let tw = g.measure_string(&p.name, &tag_font, None, &fmt).width;
                    let tag = Rect::new(x - 2.0, y - 15.0, x - 2.0 + tw + 8.0, y - 2.0);
                    g.fill_rounded_rectangle(Brush::solid(c), tag, 3.0);
                    g.draw_string(&p.name, &tag_font, Brush::solid(Color::WHITE), Rect::new(tag.left + 4.0, tag.top, tag.right, tag.bottom), &fmt);
                }
            }
            if let Some((cx, cy)) = p.cursor {
                let x = (cx - self.view.scroll_x) as f32 + ox;
                let y = (cy - self.view.scroll_y) as f32 + oy;
                // The arrow of `CursorPointer` (an 18 px box over a 24 px viewBox): M4 2 l6.5 16.5 L13 12 l6.5-2.5z.
                let k = 18.0 / 24.0;
                let pt = |px: f32, py: f32| PointF::new(x + px * k, y + py * k);
                let arrow = [pt(4.0, 2.0), pt(10.5, 18.5), pt(13.0, 12.0), pt(19.5, 9.5)];
                g.fill_polygon(Brush::solid(c), &arrow);
                g.draw_polygon(&Pen::new(Color::WHITE, 1.3 * k), &arrow);
                let font = Font::with_dip("Segoe UI", 11.0, FontStyle::BOLD);
                let tw = g.measure_string(&p.name, &font, None, &fmt).width;
                let tag = Rect::new(x + 14.0, y + 12.0, x + 14.0 + tw + 12.0, y + 28.0);
                g.fill_rounded_rectangle(Brush::solid(c), tag, 8.0);
                g.draw_string(&p.name, &font, Brush::solid(Color::WHITE), Rect::new(tag.left + 6.0, tag.top, tag.right, tag.bottom), &fmt);
            }
        }
        g.restore(saved);
    }
}
