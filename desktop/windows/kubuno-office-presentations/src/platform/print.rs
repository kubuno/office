//! « Imprimer » (the web Backstage's `onPrint: () => window.print()`): the visible slides through the framework's
//! printing stack (`kubuno_desktop::printing`, Windows Forms' `PrintDocument` and `PrintPreviewDialog`), one sheet a
//! slide, each drawn as the PNG export draws it (1920×1080, thumbnail mode), fitted into the printable bounds and
//! centred, after a preview from which the user prints.

use std::path::PathBuf;
use std::rc::Rc;

use kubuno_desktop::forms::AsForm;
use kubuno_desktop::printing::{PrintDocument, PrintPreviewDialog};
use kubuno_desktop::ui::graphics::Image;
use kubuno_desktop::ui::Rect;

use super::d2d::Fonts;
use crate::model::Doc;

/// A `w`×`h` image fitted into `bounds`, centred, keeping its ratio.
pub fn fit(w: f32, h: f32, bounds: Rect) -> Rect {
    let (bw, bh) = (bounds.right - bounds.left, bounds.bottom - bounds.top);
    if w <= 0.0 || h <= 0.0 || bw <= 0.0 || bh <= 0.0 {
        return bounds;
    }
    let k = (bw / w).min(bh / h);
    let (dw, dh) = (w * k, h * k);
    let left = bounds.left + (bw - dw) / 2.0;
    let top = bounds.top + (bh - dh) / 2.0;
    Rect::new(left, top, left + dw, top + dh)
}

fn spool_dir() -> PathBuf {
    std::env::temp_dir().join("kubuno-presentations-print").join(std::process::id().to_string())
}

/// Shows the print preview of the visible slides over `owner`. `Ok(false)`: nothing to print.
pub fn print_slides(owner: &dyn AsForm, doc: &Doc, fonts: Option<Rc<Fonts>>) -> Result<bool, String> {
    let (title, theme, slides) = {
        let ws = doc.ws.borrow();
        let slides: Vec<_> = ws.deck.slides.iter().filter(|s| !s.hidden).map(|s| s.data.clone()).collect();
        (ws.deck.title.clone(), ws.deck.theme.clone(), slides)
    };
    let fonts = match fonts {
        Some(f) => f,
        None => Rc::new(Fonts::new(super::export::dwrite()?)),
    };
    let dir = spool_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    for (i, data) in slides.iter().enumerate() {
        super::export::preload(doc, data);
        let png = super::export::render(doc, &fonts, &theme, data, true)?;
        let file = dir.join(format!("slide-{i}.png"));
        std::fs::write(&file, png).map_err(|e| e.to_string())?;
        files.push(file);
    }
    if files.is_empty() {
        return Ok(false);
    }
    let document = PrintDocument::new().document_name(if title.is_empty() { "Présentation" } else { &title });
    let images: Vec<Image> = files.iter().map(|f| Image::from_file(f.to_string_lossy())).collect();
    let mut next = 0usize;
    document.on_begin_print(|_, _| {});
    document.on_print_page(move |_, e| {
        if let Some(image) = images.get(next) {
            let dest = fit(1920.0, 1080.0, e.margin_bounds);
            e.graphics().draw_image(image, dest);
        }
        next += 1;
        e.has_more_pages = next < images.len();
        if !e.has_more_pages {
            // A preview renders the pages again from the first when it prints.
            next = 0;
        }
    });
    PrintPreviewDialog::new().document(&document).show_dialog(owner);
    let failed = document.last_error();
    for f in &files {
        let _ = std::fs::remove_file(f);
    }
    match failed {
        Some(e) => Err(e),
        None => Ok(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slide_fills_the_page_width_and_is_centred() {
        let page = Rect::new(48.0, 48.0, 768.0, 1008.0);
        let r = fit(1920.0, 1080.0, page);
        assert!((r.right - r.left - 720.0).abs() < 0.01);
        assert!((r.bottom - r.top - 405.0).abs() < 0.01);
        assert!((r.top - (48.0 + (960.0 - 405.0) / 2.0)).abs() < 0.01);
    }
}
