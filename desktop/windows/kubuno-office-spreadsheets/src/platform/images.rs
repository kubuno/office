//! Pictures through WIC: the sheet's `data:` images decoded into Direct2D bitmaps (cached per device and
//! source), their natural size, and a local file read as the `data:` URL the web's picker produces
//! (`FileReader.readAsDataURL`: the file's bytes as they are, typed by their extension).
//!
//! A picture whose `src` is not a `data:` URL (a Drive link) is not fetched: it paints nothing, as the web
//! paints an image that has not loaded.

use std::cell::RefCell;
use std::collections::HashMap;

use windows::core::Interface;
use windows::Win32::Graphics::Direct2D::{ID2D1Bitmap1, ID2D1DeviceContext};
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, IStream, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::SHCreateMemStream;

fn wic() -> Option<IWICImagingFactory> {
    // SAFETY: COM creation on the UI thread (already initialised there; a second call is harmless).
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 (with padding).
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (chunk.get(1).copied().unwrap_or(0) as u32) << 8 | chunk.get(2).copied().unwrap_or(0) as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Base64 (standard or URL-safe, padding optional, whitespace ignored).
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6 | v) & 0xFF_FFFF;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// The bytes of a `data:` URL (base64 or plain).
pub fn data_url_bytes(src: &str) -> Option<Vec<u8>> {
    let rest = src.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if meta.ends_with(";base64") {
        base64_decode(data)
    } else {
        Some(data.as_bytes().to_vec())
    }
}

/// A local file as the `data:` URL the web's picker makes of it.
pub fn file_data_url(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    };
    Ok(format!("data:{mime};base64,{}", base64_encode(&bytes)))
}

/// The natural size of encoded image bytes.
pub fn natural_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let wic = wic()?;
    // SAFETY: WIC decoding of an in-memory stream; the stream copies the bytes.
    unsafe {
        let stream: IStream = SHCreateMemStream(Some(bytes))?;
        let decoder = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        Some((w, h))
    }
}

fn to_bitmap(ctx: &ID2D1DeviceContext, bytes: &[u8]) -> Option<ID2D1Bitmap1> {
    let wic = wic()?;
    // SAFETY: as above; the converter feeds Direct2D a premultiplied BGRA copy.
    unsafe {
        let stream: IStream = SHCreateMemStream(Some(bytes))?;
        let decoder = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let src: IWICBitmapSource = frame.cast().ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&src, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        ctx.CreateBitmapFromWicBitmap(&conv, None).ok()
    }
}

type Cache = HashMap<(usize, u64), Option<(ID2D1Bitmap1, f32, f32)>>;

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(HashMap::new());
}

fn key_of(src: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    h.finish()
}

/// The bitmap of a picture's `src` for `ctx`'s device and its pixel size, decoded once.
pub fn bitmap(ctx: &ID2D1DeviceContext, src: &str) -> Option<(ID2D1Bitmap1, f32, f32)> {
    let key = (ctx.as_raw() as usize, key_of(src));
    if let Some(v) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return v;
    }
    let v = data_url_bytes(src).and_then(|b| {
        let bm = to_bitmap(ctx, &b)?;
        // SAFETY: a plain COM getter on a live bitmap.
        let s = unsafe { bm.GetPixelSize() };
        Some((bm, s.width as f32, s.height as f32))
    });
    CACHE.with(|c| c.borrow_mut().insert(key, v.clone()));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        for s in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            assert_eq!(base64_decode(&base64_encode(s)).as_deref(), Some(s));
        }
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(data_url_bytes("data:text/plain;base64,aGk="), Some(b"hi".to_vec()));
    }
}
