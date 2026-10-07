//! The custom-drawn controls: the slide canvas (editing), the slide list, the slideshow stage.

pub mod presence_avatars;
pub mod slide_canvas;
pub mod slide_panel;
pub mod slide_show;
pub mod start_page;

use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

/// `0xRRGGBB` as an opaque colour.
pub const fn hex(rgb: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: ((rgb >> 16) & 0xff) as f32 / 255.0, g: ((rgb >> 8) & 0xff) as f32 / 255.0, b: (rgb & 0xff) as f32 / 255.0, a: 1.0 }
}
