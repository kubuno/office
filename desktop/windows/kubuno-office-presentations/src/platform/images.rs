//! Slide images: where their bytes come from and their Direct2D bitmaps.
//!
//! An image element stores a reference (`storagePath`): `kbfile:<id>` (an asset of the presentation, served by
//! `GET /presentations/:id/assets/:id`), a legacy `data:` URL, or an `http(s)` URL. The [`ImageStore`] holds the
//! bytes, fetched off the UI thread (the web's `resolveSlideImage`: one fetch per asset, a repaint when it
//! lands); [`BitmapCache`] turns them into bitmaps of the device context that draws them. WIC decodes every
//! format Windows has a codec for.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use windows::core::{Interface, GUID};
use windows::Win32::Graphics::Direct2D::{ID2D1Bitmap1, ID2D1DeviceContext};
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, IStream, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::SHCreateMemStream;

/// Fetches an asset's bytes (`file_id` → bytes), on a worker thread.
pub type Fetch = Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;
/// Asks the window to repaint (from any thread).
pub type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
enum Entry {
    Loading,
    Ready(Arc<Vec<u8>>),
    Failed,
}

/// Image bytes by reference, shared by the window's controls.
#[derive(Clone, Default)]
pub struct ImageStore {
    map: Arc<Mutex<HashMap<String, Entry>>>,
    fetch: Option<Fetch>,
    wake: Option<Wake>,
}

impl ImageStore {
    /// A store fetching `kbfile:` assets through `fetch` and waking the window through `wake`.
    pub fn new(fetch: Option<Fetch>, wake: Option<Wake>) -> ImageStore {
        ImageStore { map: Arc::default(), fetch, wake }
    }

    /// Adds bytes under a reference (an image just inserted).
    pub fn put(&self, src: &str, bytes: Vec<u8>) {
        self.map.lock().unwrap_or_else(|p| p.into_inner()).insert(src.to_string(), Entry::Ready(Arc::new(bytes)));
    }

    /// Whether `src` is being fetched.
    pub fn is_loading(&self, src: &str) -> bool {
        matches!(self.map.lock().unwrap_or_else(|p| p.into_inner()).get(src), Some(Entry::Loading))
    }

    /// The bytes of `src` when available; starts loading them otherwise.
    pub fn bytes(&self, src: &str) -> Option<Arc<Vec<u8>>> {
        let mut map = self.map.lock().unwrap_or_else(|p| p.into_inner());
        match map.get(src) {
            Some(Entry::Ready(b)) => return Some(b.clone()),
            Some(Entry::Loading) | Some(Entry::Failed) => return None,
            None => {}
        }
        if let Some(data) = src.strip_prefix("data:") {
            let bytes = decode_data_url(data);
            map.insert(src.to_string(), bytes.clone().map(|b| Entry::Ready(Arc::new(b))).unwrap_or(Entry::Failed));
            return bytes.map(Arc::new);
        }
        let Some(file_id) = src.strip_prefix("kbfile:") else {
            // http(s) images need the web's signed URLs: not fetched by the desktop yet.
            map.insert(src.to_string(), Entry::Failed);
            return None;
        };
        let Some(fetch) = self.fetch.clone() else {
            map.insert(src.to_string(), Entry::Failed);
            return None;
        };
        let key = src.to_string();
        map.insert(key.clone(), Entry::Loading);
        let (store, src, file_id, wake) = (self.map.clone(), key.clone(), file_id.to_string(), self.wake.clone());
        let spawned = std::thread::Builder::new().name("presentations-image".into()).spawn(move || {
            let got = fetch(&file_id);
            store.lock().unwrap_or_else(|p| p.into_inner()).insert(src, got.map(|b| Entry::Ready(Arc::new(b))).unwrap_or(Entry::Failed));
            if let Some(w) = wake {
                w();
            }
        });
        if spawned.is_err() {
            map.insert(key, Entry::Failed);
        }
        None
    }
}

/// `data:[mime][;base64],payload` → bytes.
pub fn decode_data_url(rest: &str) -> Option<Vec<u8>> {
    let (head, payload) = rest.split_once(',')?;
    if head.ends_with(";base64") {
        base64_decode(payload)
    } else {
        Some(percent_decode(payload))
    }
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// Standard base64 (padding optional, whitespace ignored).
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Standard base64 with padding.
pub fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

pub(crate) fn wic() -> Option<IWICImagingFactory> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()
    }
}

fn frame_of(bytes: &[u8]) -> Option<(IWICImagingFactory, IWICBitmapFrameDecode)> {
    let wic = wic()?;
    unsafe {
        let stream: IStream = SHCreateMemStream(Some(bytes))?;
        let decoder = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        Some((wic, frame))
    }
}

/// The pixel size of encoded image bytes.
pub fn size_of(bytes: &[u8]) -> Option<(u32, u32)> {
    let (_, frame) = frame_of(bytes)?;
    let (mut w, mut h) = (0u32, 0u32);
    unsafe { frame.GetSize(&mut w, &mut h).ok()? };
    Some((w, h))
}

pub(crate) fn converted(wic: &IWICImagingFactory, source: &IWICBitmapSource, format: &GUID) -> Option<IWICFormatConverter> {
    unsafe {
        let c = wic.CreateFormatConverter().ok()?;
        c.Initialize(source, format, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        Some(c)
    }
}

/// A Direct2D bitmap of image bytes for `ctx`.
pub fn to_bitmap(ctx: &ID2D1DeviceContext, bytes: &[u8]) -> Option<ID2D1Bitmap1> {
    let (wic, frame) = frame_of(bytes)?;
    let src: IWICBitmapSource = frame.cast().ok()?;
    let conv = converted(&wic, &src, &GUID_WICPixelFormat32bppPBGRA)?;
    unsafe { ctx.CreateBitmapFromWicBitmap(&conv, None).ok() }
}

/// `loadImageFile`: the image reduced to `max_dim` on its long side (kept when smaller), re-encoded as PNG
/// when it was a PNG, JPEG (quality 0.9) otherwise; returns the bytes, the MIME type and the NATURAL size.
pub fn prepare_upload(bytes: &[u8], is_png: bool, max_dim: u32) -> Option<(Vec<u8>, &'static str, u32, u32)> {
    let (wic, frame) = frame_of(bytes)?;
    let (mut w, mut h) = (0u32, 0u32);
    unsafe { frame.GetSize(&mut w, &mut h).ok()? };
    let scale = (max_dim as f64 / w.max(h).max(1) as f64).min(1.0);
    let mime = if is_png { "image/png" } else { "image/jpeg" };
    if scale >= 1.0 {
        return Some((bytes.to_vec(), mime, w, h));
    }
    let (cw, ch) = (((w as f64 * scale).round() as u32).max(1), ((h as f64 * scale).round() as u32).max(1));
    unsafe {
        let src: IWICBitmapSource = frame.cast().ok()?;
        let scaler = wic.CreateBitmapScaler().ok()?;
        scaler.Initialize(&src, cw, ch, WICBitmapInterpolationModeHighQualityCubic).ok()?;
        let scaled: IWICBitmapSource = scaler.cast().ok()?;
        let out = encode(&wic, &scaled, cw, ch, is_png)?;
        Some((out, mime, w, h))
    }
}

/// Encodes `source` (`w`×`h`) as PNG (32-bit BGRA) or JPEG (24-bit).
pub(crate) fn encode(wic: &IWICImagingFactory, source: &IWICBitmapSource, w: u32, h: u32, png: bool) -> Option<Vec<u8>> {
    unsafe {
        let fmt = if png { GUID_WICPixelFormat32bppBGRA } else { GUID_WICPixelFormat24bppBGR };
        let conv = converted(wic, source, &fmt)?;
        let stream: IStream = SHCreateMemStream(None)?;
        let container = if png { GUID_ContainerFormatPng } else { GUID_ContainerFormatJpeg };
        let encoder = wic.CreateEncoder(&container, std::ptr::null()).ok()?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
        let mut frame_enc = None;
        let mut props = None;
        encoder.CreateNewFrame(&mut frame_enc, &mut props).ok()?;
        let frame_enc = frame_enc?;
        frame_enc.Initialize(props.as_ref()).ok()?;
        frame_enc.SetSize(w, h).ok()?;
        let mut pf = fmt;
        frame_enc.SetPixelFormat(&mut pf).ok()?;
        frame_enc.WriteSource(&conv, std::ptr::null()).ok()?;
        frame_enc.Commit().ok()?;
        encoder.Commit().ok()?;
        read_stream(&stream)
    }
}

unsafe fn read_stream(stream: &IStream) -> Option<Vec<u8>> {
    use windows::Win32::System::Com::{STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET};
    let mut stat = STATSTG::default();
    stream.Stat(&mut stat, STATFLAG_NONAME).ok()?;
    let len = stat.cbSize as usize;
    stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
    let mut buf = vec![0u8; len];
    let mut read = 0u32;
    if stream.Read(buf.as_mut_ptr() as *mut _, len as u32, Some(&mut read)).is_err() {
        return None;
    }
    buf.truncate(read as usize);
    Some(buf)
}

/// Bitmaps by reference, for one device context (rebuilt when the context changes).
#[derive(Default)]
pub struct BitmapCache {
    ctx: usize,
    map: HashMap<String, Option<ID2D1Bitmap1>>,
}

impl BitmapCache {
    /// The bitmap of `src`, `None` while its bytes load or when they are not an image.
    pub fn bitmap(&mut self, ctx: &ID2D1DeviceContext, store: &ImageStore, src: &str) -> Option<ID2D1Bitmap1> {
        let id = ctx.as_raw() as usize;
        if id != self.ctx {
            self.ctx = id;
            self.map.clear();
        }
        if let Some(found) = self.map.get(src) {
            return found.clone();
        }
        let bytes = store.bytes(src)?;
        let bitmap = to_bitmap(ctx, &bytes);
        self.map.insert(src.to_string(), bitmap.clone());
        bitmap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_and_data_urls() {
        assert_eq!(base64_decode("aGVsbG8="), Some(b"hello".to_vec()));
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_encode(b"hell"), "aGVsbA==");
        assert_eq!(decode_data_url("text/plain,a%20b"), Some(b"a b".to_vec()));
        assert_eq!(decode_data_url("image/png;base64,AAE="), Some(vec![0, 1]));
    }
}
