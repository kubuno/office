//! The exports of « Accueil › Diapositive » (`handleExportPdf`, `exportSlideImg`, `exportAllPng`): the slides
//! rendered off screen at 1920×1080 by the shared renderer (thumbnail mode: no placeholders) through a Direct2D
//! context over a WIC bitmap, encoded by WIC (PNG, or JPEG on white), and for the PDF assembled by desktop/common
//! (`pdf::pages_to_pdf`, one JPEG per visible slide).

use std::path::{Path, PathBuf};
use std::rc::Rc;

use kubuno_office_presentations_common::pdf::{pages_to_pdf, Page};
use kubuno_office_slides_core::model::{Background, SlideData, Theme};
use kubuno_office_slides_core::render::{Mode, Options, Renderer};
use kubuno_office_slides_core::surface::Matrix;
use windows::core::Interface;
use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::{DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::*;

use super::d2d::{D2dSurface, Fonts};
use super::images::{encode, wic, BitmapCache};
use crate::model::Doc;

/// What to export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pdf,
    Png,
    Jpeg,
    AllPng,
}

const W: u32 = 1920;
const H: u32 = 1080;

/// Asks where, renders and writes. `Ok(None)`: cancelled.
pub fn run(kind: Kind, doc: &Doc, fonts: Option<Rc<Fonts>>) -> Result<Option<PathBuf>, String> {
    let (title, active, slides) = {
        let ws = doc.ws.borrow();
        let title = if ws.deck.title.is_empty() { "presentation".to_string() } else { ws.deck.title.clone() };
        let slides: Vec<(usize, bool, SlideData)> = ws.deck.slides.iter().enumerate().map(|(i, s)| (i, s.hidden, s.data.clone())).collect();
        (title, ws.active_index(), slides)
    };
    let theme = doc.ws.borrow().deck.theme.clone();
    let fonts = match fonts {
        Some(f) => f,
        None => Rc::new(Fonts::new(dwrite()?)),
    };
    let safe = sanitize(&title);
    match kind {
        Kind::Png | Kind::Jpeg => {
            let Some(i) = active else { return Ok(None) };
            let png = kind == Kind::Png;
            let ext = if png { "png" } else { "jpg" };
            let name = format!("{safe}-{}.{ext}", i + 1);
            let Some(path) = super::file_dialog::save_as(&name, if png { "Image PNG" } else { "Image JPEG" }, ext) else { return Ok(None) };
            let data = &slides[i].2;
            preload(doc, data);
            let bytes = render(doc, &fonts, &theme, data, png)?;
            write(&path, &bytes)?;
            Ok(Some(path))
        }
        Kind::AllPng => {
            let Some(first) = super::file_dialog::save_as(&format!("{safe}-1.png"), "Image PNG", "png") else { return Ok(None) };
            let dir = first.parent().map(Path::to_path_buf).unwrap_or_default();
            let stem = first.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let base = stem.strip_suffix("-1").unwrap_or(&stem).to_string();
            for (i, hidden, data) in &slides {
                if *hidden {
                    continue;
                }
                preload(doc, data);
                let bytes = render(doc, &fonts, &theme, data, true)?;
                write(&dir.join(format!("{base}-{}.png", i + 1)), &bytes)?;
            }
            Ok(Some(dir))
        }
        Kind::Pdf => {
            let Some(path) = super::file_dialog::save_as(&format!("{safe}.pdf"), "PDF", "pdf") else { return Ok(None) };
            let mut pages = Vec::new();
            for (_, hidden, data) in &slides {
                if *hidden {
                    continue;
                }
                preload(doc, data);
                let jpeg = render(doc, &fonts, &theme, data, false)?;
                pages.push(Page { jpeg, image_w: W, image_h: H, w_px: W as f64, h_px: H as f64 });
            }
            if pages.is_empty() {
                return Ok(None);
            }
            write(&path, &pages_to_pdf(&pages, &title))?;
            Ok(Some(path))
        }
    }
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '_' } else { c }).collect::<String>().trim().trim_end_matches('.').to_string()
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

pub(crate) fn dwrite() -> Result<IDWriteFactory, String> {
    unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).map_err(|e| e.to_string()) }
}

/// Starts loading the slide's images and waits for them a little (the web waits up to 2.5 s per image).
pub(crate) fn preload(doc: &Doc, data: &SlideData) {
    let mut srcs: Vec<String> = data.elements.iter().filter(|e| e.kind() == "image").filter_map(|e| e.s("storagePath").map(str::to_string)).collect();
    if let Background::Image(p) = data.background() {
        srcs.push(p);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(2500);
    loop {
        let pending = srcs.iter().filter(|s| doc.store.bytes(s).is_none() && doc.store.is_loading(s)).count();
        if pending == 0 || std::time::Instant::now() >= deadline {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

/// The slide at 1920×1080: PNG, or JPEG on white.
pub(crate) fn render(doc: &Doc, fonts: &Fonts, theme: &Theme, data: &SlideData, png: bool) -> Result<Vec<u8>, String> {
    let wic = wic().ok_or("WIC indisponible")?;
    unsafe {
        let bitmap = wic.CreateBitmap(W, H, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad).map_err(|e| e.to_string())?;
        let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).map_err(|e| e.to_string())?;
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: 96.0,
            dpiY: 96.0,
            ..Default::default()
        };
        let target = factory.CreateWicBitmapRenderTarget(&bitmap, &props).map_err(|e| e.to_string())?;
        let ctx: ID2D1DeviceContext = target.cast().map_err(|e| e.to_string())?;
        ctx.BeginDraw();
        let clear = if png { D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 } } else { D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 } };
        ctx.Clear(Some(&clear));
        {
            let mut bitmaps = BitmapCache::default();
            let mut surface = D2dSurface::with_context(ctx.clone(), factory.clone(), fonts, &doc.store, &mut bitmaps);
            let base = surface.outer().mul(&Matrix::IDENTITY);
            let r = Renderer::new(W as f64, H as f64, fonts);
            r.render(&mut surface, base, data, theme, &Options { mode: Mode::Thumbnail, ..Options::default() });
        }
        ctx.EndDraw(None, None).map_err(|e| e.to_string())?;
        let source: IWICBitmapSource = bitmap.cast().map_err(|e| e.to_string())?;
        encode(&wic, &source, W, H, png).ok_or_else(|| "encodage de l'image impossible".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slide_renders_off_screen_to_png_and_jpeg() {
        let deck = kubuno_office_presentations_common::app::sample_deck();
        let ws = kubuno_office_presentations_common::model::workspace::Workspace::new(deck, 1);
        let data = ws.deck.slides[0].data.clone();
        let theme = ws.deck.theme.clone();
        let doc = crate::model::Doc::new(ws, crate::platform::images::ImageStore::default());
        let fonts = Fonts::new(dwrite().unwrap_or_else(|e| panic!("{e}")));
        let png = render(&doc, &fonts, &theme, &data, true).unwrap_or_else(|e| panic!("{e}"));
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        let jpg = render(&doc, &fonts, &theme, &data, false).unwrap_or_else(|e| panic!("{e}"));
        assert!(jpg.starts_with(&[0xff, 0xd8]));
        let pdf = pages_to_pdf(&[Page { jpeg: jpg, image_w: W, image_h: H, w_px: W as f64, h_px: H as f64 }], "t");
        assert!(pdf.starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn file_names_lose_what_windows_refuses() {
        assert_eq!(sanitize("Q3: plan/2026?"), "Q3_ plan_2026_");
        assert_eq!(sanitize("deck. "), "deck");
    }
}
