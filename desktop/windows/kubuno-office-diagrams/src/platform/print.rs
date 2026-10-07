//! « Imprimer » (the web Backstage's `onPrint: () => window.print()`): the diagram's pages through the
//! framework's printing stack (`kubuno_desktop::printing`, Windows Forms' `PrintDocument` and
//! `PrintPreviewDialog`). The browser prints what the editor shows; the desktop prints each page of the
//! diagram as its PNG export draws it (`raster::render_export`: the visible layers, at scale 2, a 24 px
//! margin), one sheet a page, fitted into the printable bounds and centred, after a preview from which the
//! user prints.

use std::path::PathBuf;

use kubuno_desktop::forms::AsForm;
use kubuno_desktop::printing::{PrintDocument, PrintPreviewDialog};
use kubuno_desktop::ui::graphics::Image;
use kubuno_desktop::ui::Rect;
use kubuno_office_diagrams_core::file::Page;
use kubuno_office_diagrams_core::model::{Connector, PageData, Shape, DEFAULT_LAYER_ID};

/// The shapes and connectors of `data` that its visible layers hold, in layer order (the editor's
/// `visible_data`, for any page and not only the current one).
pub fn page_visible(data: &PageData) -> (Vec<Shape>, Vec<Connector>) {
    let layers = data.layers();
    let first = layers.first().map(|l| l.id().to_string()).unwrap_or_else(|| DEFAULT_LAYER_ID.to_string());
    let of = |lid: Option<&str>| lid.unwrap_or(&first).to_string();
    let index = |lid: &str| layers.iter().position(|l| l.id() == lid).unwrap_or(0);
    let shown = |lid: &str| layers.iter().find(|l| l.id() == lid).map(|l| l.visible()).unwrap_or(true);
    let mut shapes: Vec<Shape> = data.shapes.iter().filter(|s| shown(&of(s.layer_id()))).cloned().collect();
    shapes.sort_by_key(|s| index(&of(s.layer_id())));
    let mut conns: Vec<Connector> = data.connectors.iter().filter(|c| shown(&of(c.layer_id()))).cloned().collect();
    conns.sort_by_key(|c| index(&of(c.layer_id())));
    (shapes, conns)
}

/// `w`×`h` fitted into `bounds` (never enlarged beyond 1 DIP per pixel / 2 — the export's scale), centred.
pub fn fit(w: f32, h: f32, bounds: Rect) -> Rect {
    let (bw, bh) = (bounds.right - bounds.left, bounds.bottom - bounds.top);
    if w <= 0.0 || h <= 0.0 || bw <= 0.0 || bh <= 0.0 {
        return bounds;
    }
    // The export is drawn at scale 2: its natural size is half its pixels.
    let k = (bw / w).min(bh / h).min(0.5);
    let (dw, dh) = (w * k, h * k);
    let left = bounds.left + (bw - dw) / 2.0;
    let top = bounds.top + (bh - dh) / 2.0;
    Rect::new(left, top, left + dw, top + dh)
}

/// One sheet: the page's image file and its size in pixels.
struct Sheet {
    file: PathBuf,
    w: f32,
    h: f32,
}

/// The folder of this process's print images (`%TEMP%\kubuno-diagrams-print\<pid>`).
fn spool_dir() -> PathBuf {
    std::env::temp_dir().join("kubuno-diagrams-print").join(std::process::id().to_string())
}

/// Shows the print preview of `pages` (from which the user prints) over `owner`. `Err` names what failed;
/// an empty diagram prints nothing (`Ok(false)`).
pub fn print_pages(owner: &dyn AsForm, title: &str, pages: &[Page]) -> Result<bool, String> {
    let dir = spool_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut sheets = Vec::new();
    for (i, p) in pages.iter().enumerate() {
        let (shapes, conns) = page_visible(&p.data);
        let Some((png, w, h)) = super::raster::render_export(&shapes, &conns, &p.bg_color, false) else { continue };
        let file = dir.join(format!("page-{i}.png"));
        std::fs::write(&file, png).map_err(|e| e.to_string())?;
        sheets.push(Sheet { file, w: w as f32, h: h as f32 });
    }
    if sheets.is_empty() {
        return Ok(false);
    }
    let doc = PrintDocument::new().document_name(title);
    let mut next = 0usize;
    doc.on_begin_print(|_, _| {});
    let images: Vec<(Image, f32, f32)> = sheets.iter().map(|s| (Image::from_file(s.file.to_string_lossy()), s.w, s.h)).collect();
    doc.on_print_page(move |_, e| {
        if let Some((image, w, h)) = images.get(next) {
            let dest = fit(*w, *h, e.margin_bounds);
            e.graphics().draw_image(image, dest);
        }
        next += 1;
        e.has_more_pages = next < images.len();
        if !e.has_more_pages {
            // A preview renders the pages again from the first when it prints.
            next = 0;
        }
    });
    PrintPreviewDialog::new().document(&doc).show_dialog(owner);
    let failed = doc.last_error();
    for s in &sheets {
        let _ = std::fs::remove_file(&s.file);
    }
    match failed {
        Some(e) => Err(e),
        None => Ok(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_hidden_layer_is_not_printed_and_the_layers_keep_their_order() {
        let data = PageData::from_value(&json!({
            "shapes": [
                { "id": "top", "type": "rect", "x": 0, "y": 0, "w": 10, "h": 10, "layerId": "l2" },
                { "id": "hidden", "type": "rect", "x": 0, "y": 0, "w": 10, "h": 10, "layerId": "l3" },
                { "id": "bottom", "type": "rect", "x": 0, "y": 0, "w": 10, "h": 10 }
            ],
            "connectors": [{ "id": "c", "sourceId": "top", "targetId": "bottom", "layerId": "l3" }],
            "layers": [
                { "id": "l1", "name": "Calque 1", "visible": true, "locked": false },
                { "id": "l2", "name": "Calque 2", "visible": true, "locked": true },
                { "id": "l3", "name": "Calque 3", "visible": false, "locked": false }
            ]
        }));
        let (shapes, conns) = page_visible(&data);
        assert_eq!(shapes.iter().map(|s| s.id()).collect::<Vec<_>>(), ["bottom", "top"]);
        assert!(conns.is_empty());
    }

    #[test]
    fn a_page_without_layers_prints_everything() {
        let data = PageData::from_value(&json!({ "shapes": [{ "id": "a", "type": "rect", "x": 0, "y": 0, "w": 1, "h": 1 }], "connectors": [] }));
        assert_eq!(page_visible(&data).0.len(), 1);
    }

    #[test]
    fn the_image_is_fitted_and_centred_but_never_enlarged() {
        let page = Rect::new(96.0, 96.0, 720.0, 960.0);
        // A small export keeps its natural size (half its pixels) and is centred.
        let r = fit(200.0, 100.0, page);
        assert_eq!((r.right - r.left, r.bottom - r.top), (100.0, 50.0));
        assert_eq!(r.left, 96.0 + (624.0 - 100.0) / 2.0);
        // A wide one is shrunk to the width.
        let r = fit(4000.0, 1000.0, page);
        assert!((r.right - r.left - 624.0).abs() < 0.01);
        assert!((r.bottom - r.top - 156.0).abs() < 0.01);
    }
}
