//! Raster exports (`renderExportCanvas` + `exportImage`, `DiagramEditorPage.tsx:2672-2721`, « Export PNG /
//! JPEG / PDF »): the page drawn at scale 2 with a 24 px margin around its content
//! (`render::export_frame`), through the shared Canvas 2D context onto an offscreen Direct2D target (a WIC
//! bitmap render target), then encoded by WIC - PNG, or JPEG at quality 0.92 like `toBlob(…, 0.92)`.
//!
//! The web multiplies the size by `devicePixelRatio`; the desktop uses 1 here, so an export has the same
//! pixel size on every screen (a 100 % web export). The background is filled with the page colour for PNG
//! too (the web's `opaqueBg ? bg : bg`); no grid, no selection, no hover.

use kubuno_office_diagrams_core::canvas::Ctx2D;
use kubuno_office_diagrams_core::model::{Connector, Shape};
use kubuno_office_diagrams_core::render::{self, Scene};
use windows::core::{Interface, GUID};
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1DeviceContext, ID2D1Factory1, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE,
};
use windows::Win32::Graphics::DirectWrite::{DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_ContainerFormatPng, GUID_WICPixelFormat24bppBGR, GUID_WICPixelFormat32bppBGRA, GUID_WICPixelFormat32bppPBGRA, IWICBitmap,
    IWICBitmapFrameEncode, IWICImagingFactory, WICBitmapCacheOnDemand, WICBitmapEncoderNoCache,
};
use windows::Win32::System::Com::StructuredStorage::{CreateStreamOnHGlobal, GetHGlobalFromStream, IPropertyBag2, PROPBAG2};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STREAM_SEEK_CUR};
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_R4};

use crate::platform::surface::{D2DSurface, SurfaceCache};

/// The JPEG quality of the web's `toBlob('image/jpeg', 0.92)`.
pub const JPEG_QUALITY: f32 = 0.92;

/// The largest side drawn (Direct2D's bitmap limit is 16384; a bigger page is refused rather than cut).
const MAX_SIDE: u32 = 16384;

/// The encoded image (PNG, or JPEG at 92 %) and its size in pixels; `None` for an empty page or when the
/// image cannot be made.
pub fn render_export(shapes: &[Shape], connectors: &[Connector], bg: &str, jpeg: bool) -> Option<(Vec<u8>, u32, u32)> {
    let (css_w, css_h, zoom, pan_x, pan_y) = render::export_frame(shapes, connectors)?;
    // `devicePixelRatio` = 1 (see the module doc).
    let (w, h) = (css_w.ceil().max(1.0) as u32, css_h.ceil().max(1.0) as u32);
    if w > MAX_SIDE || h > MAX_SIDE {
        kubuno_desktop::tracing::warn!("[diagrams] export: {w}×{h} px is larger than Direct2D can draw");
        return None;
    }
    match unsafe { draw_and_encode(shapes, connectors, bg, jpeg, w, h, zoom, pan_x, pan_y) } {
        Ok(bytes) => Some((bytes, w, h)),
        Err(e) => {
            kubuno_desktop::tracing::warn!("[diagrams] export failed: {e}");
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_and_encode(shapes: &[Shape], connectors: &[Connector], bg: &str, jpeg: bool, w: u32, h: u32, zoom: f64, pan_x: f64, pan_y: f64) -> windows::core::Result<Vec<u8>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let bitmap: IWICBitmap = wic.CreateBitmap(w, h, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnDemand)?;
        let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let target = factory.CreateWicBitmapRenderTarget(&bitmap, &props)?;
        // A WIC render target made by an `ID2D1Factory1` is a device context.
        let ctx: ID2D1DeviceContext = target.cast()?;
        let cache = SurfaceCache::default();
        ctx.BeginDraw();
        {
            let mut surface = D2DSurface::new(&ctx, &factory, &dwrite, &cache)?;
            let mut c = Ctx2D::new(&mut surface);
            let bg = if bg.is_empty() { "#ffffff" } else { bg };
            render::render_canvas(
                &mut c,
                &Scene {
                    shapes,
                    connectors,
                    zoom,
                    pan_x,
                    pan_y,
                    selected: &[],
                    selected_conns: &[],
                    hovered: None,
                    drawing_conn: None,
                    lasso: None,
                    bg_color: bg,
                    magnet_segs: &[],
                    show_grid: false,
                    grid_size: kubuno_office_diagrams_core::geometry::GRID_SIZE,
                    guides: None,
                    drawing_shape: None,
                    width: w as f64,
                    height: h as f64,
                },
            );
        }
        ctx.EndDraw(None, None)?;
        drop(ctx);
        drop(target);
        encode(&wic, &bitmap, w, h, jpeg)
    }
}

unsafe fn encode(wic: &IWICImagingFactory, bitmap: &IWICBitmap, w: u32, h: u32, jpeg: bool) -> windows::core::Result<Vec<u8>> {
    unsafe {
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
        let container: GUID = if jpeg { GUID_ContainerFormatJpeg } else { GUID_ContainerFormatPng };
        let encoder = wic.CreateEncoder(&container, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut frame: Option<IWICBitmapFrameEncode> = None;
        let mut options: Option<IPropertyBag2> = None;
        encoder.CreateNewFrame(&mut frame, &mut options)?;
        let frame = frame.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
        if jpeg {
            if let Some(bag) = &options {
                let mut name: Vec<u16> = "ImageQuality".encode_utf16().chain(std::iter::once(0)).collect();
                let prop = PROPBAG2 { pstrName: windows::core::PWSTR(name.as_mut_ptr()), ..Default::default() };
                let value = VARIANT {
                    Anonymous: VARIANT_0 {
                        Anonymous: core::mem::ManuallyDrop::new(VARIANT_0_0 { vt: VT_R4, wReserved1: 0, wReserved2: 0, wReserved3: 0, Anonymous: VARIANT_0_0_0 { fltVal: JPEG_QUALITY } }),
                    },
                };
                bag.Write(1, &prop, &value)?;
            }
        }
        frame.Initialize(options.as_ref())?;
        frame.SetSize(w, h)?;
        // JPEG has no alpha: 24-bit BGR (WIC converts while writing); PNG keeps 32-bit BGRA.
        let mut format = if jpeg { GUID_WICPixelFormat24bppBGR } else { GUID_WICPixelFormat32bppBGRA };
        frame.SetPixelFormat(&mut format)?;
        frame.WriteSource(bitmap, std::ptr::null())?;
        frame.Commit()?;
        encoder.Commit()?;
        let mut len = 0u64;
        stream.Seek(0, STREAM_SEEK_CUR, Some(&mut len))?;
        let hglobal = GetHGlobalFromStream(&stream)?;
        let p = GlobalLock(hglobal);
        if p.is_null() {
            return Err(windows::core::Error::from(windows::Win32::Foundation::E_OUTOFMEMORY));
        }
        let mut out = vec![0u8; len as usize];
        std::ptr::copy_nonoverlapping(p.cast::<u8>(), out.as_mut_ptr(), len as usize);
        let _ = GlobalUnlock(hglobal);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_office_diagrams_core::model::Obj;

    fn page() -> (Vec<Shape>, Vec<Connector>) {
        let mut style = Obj::new();
        style.insert("fillColor".into(), "#dae8fc".into());
        let shapes = vec![
            Shape::new("a", "rect", 0.0, 0.0, 120.0, 60.0, "Début", style.clone(), 0.0, "default"),
            Shape::new("b", "ellipse", 220.0, 100.0, 120.0, 70.0, "Fin", style, 1.0, "default"),
        ];
        (shapes, vec![Connector::between("c", "a", "b", "default")])
    }

    #[test]
    fn a_png_export_has_the_frame_size() {
        let (shapes, conns) = page();
        let (png, w, h) = render_export(&shapes, &conns, "#ffffff", false).expect("a png");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        let iw = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        let ih = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
        // Content 340×170 + 2×24 margin, at scale 2.
        assert_eq!((iw, ih), (w, h));
        assert_eq!((w, h), ((340 + 48) * 2, (170 + 48) * 2));
        let out = std::path::Path::new("C:/kubuno-build/agent-office-diagrams/out");
        if out.is_dir() {
            let _ = std::fs::write(out.join("exp-test.png"), &png);
        }
    }

    #[test]
    fn a_jpeg_export_is_a_jpeg() {
        let (shapes, conns) = page();
        let (jpg, _, _) = render_export(&shapes, &conns, "#ffffff", true).expect("a jpeg");
        assert_eq!(&jpg[..2], &[0xFF, 0xD8]);
        assert_eq!(&jpg[jpg.len() - 2..], &[0xFF, 0xD9]);
        let pdf = kubuno_office_diagrams_core::pdf::build_jpeg_pdf(&jpg, 10, 10);
        assert!(pdf.starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn an_empty_page_exports_nothing() {
        assert!(render_export(&[], &[], "#ffffff", false).is_none());
    }
}
