//! The one-off migration of legacy images on open (the web's « images base64 héritées → assets fichiers »): an
//! image element or a slide background whose reference is a `data:` URL is uploaded as an asset of the
//! presentation and rewritten as its `kbfile:` reference, so the slides stop carrying the bytes (and stop
//! counting them against the server's 2 MiB per slide). The window runs the uploads off the UI thread with
//! [`legacy_images`] and applies the answers with [`rewrite`]; the session then saves the slides that changed
//! like any edit (digest-guarded).

use std::collections::HashMap;

use kubuno_office_slides_core::model::{Background, Deck};
use serde_json::{json, Value};

/// A legacy image to upload: its `data:` URL, the decoded bytes, their type, and the file name the web gives it
/// (`migrated` for an element, `bg` for a background).
#[derive(Debug, Clone, PartialEq)]
pub struct Legacy {
    pub url: String,
    pub bytes: Vec<u8>,
    pub mime: String,
    pub name: &'static str,
}

/// Every distinct `data:` image of the deck (an image used twice is uploaded once).
pub fn legacy_images(deck: &Deck) -> Vec<Legacy> {
    let mut out: Vec<Legacy> = Vec::new();
    let mut push = |url: &str, name: &'static str| {
        if out.iter().any(|l| l.url == url) {
            return;
        }
        if let Some((mime, bytes)) = decode_data_url(url) {
            out.push(Legacy { url: url.to_string(), bytes, mime, name });
        }
    };
    for slide in &deck.slides {
        for el in slide.data.elements.iter().filter(|e| e.kind() == "image") {
            if let Some(src) = el.s("storagePath").filter(|s| s.starts_with("data:")) {
                push(src, "migrated");
            }
        }
        if let Background::Image(p) = slide.data.background() {
            if p.starts_with("data:") {
                push(&p, "bg");
            }
        }
    }
    out
}

/// Replaces every `data:` reference found in `refs` (data URL → `kbfile:` reference); returns the ids of the
/// slides that changed.
pub fn rewrite(deck: &mut Deck, refs: &HashMap<String, String>) -> Vec<String> {
    let mut changed = Vec::new();
    for slide in &mut deck.slides {
        let mut touched = false;
        for el in slide.data.elements.iter_mut().filter(|e| e.kind() == "image") {
            if let Some(r) = el.s("storagePath").and_then(|s| refs.get(s)).cloned() {
                el.set_s("storagePath", &r);
                touched = true;
            }
        }
        if let Some(bg) = slide.data.get("background").cloned() {
            if let Some(r) = bg.get("imagePath").and_then(Value::as_str).and_then(|p| refs.get(p)).cloned() {
                let mut nb = bg.as_object().cloned().unwrap_or_default();
                nb.insert("imagePath".into(), json!(r));
                slide.data.set("background", Value::Object(nb));
                touched = true;
            }
        }
        if touched {
            changed.push(slide.id.clone());
        }
    }
    changed
}

/// `data:[mime][;base64],payload` → (mime, bytes).
pub fn decode_data_url(url: &str) -> Option<(String, Vec<u8>)> {
    let rest = url.strip_prefix("data:")?;
    let (head, payload) = rest.split_once(',')?;
    let (mime, base64) = match head.strip_suffix(";base64") {
        Some(m) => (m, true),
        None => (head, false),
    };
    let mime = mime.split(';').next().filter(|m| !m.is_empty()).unwrap_or("application/octet-stream").to_string();
    let bytes = if base64 { base64_decode(payload)? } else { percent_decode(payload) };
    Some((mime, bytes))
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
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

/// Standard or URL-safe base64, padding optional, whitespace ignored.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn deck() -> Deck {
        let content = r##"{"version":1,"slides":{
            "a":{"elements":[
                {"id":"i1","type":"image","storagePath":"data:image/png;base64,iVBORw=="},
                {"id":"i2","type":"image","storagePath":"kbfile:old"},
                {"id":"i3","type":"image","storagePath":"data:image/png;base64,iVBORw=="}],
              "background":{"type":"color","color":"#fff"}},
            "b":{"elements":[],"background":{"type":"image","imagePath":"data:image/jpeg;base64,/9j/"}}}}"##;
        Deck::from_json(content.as_bytes()).unwrap_or_else(|e| panic!("{e:?}"))
    }

    #[test]
    fn each_distinct_data_image_is_listed_once() {
        let l = legacy_images(&deck());
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].mime.as_str(), l[0].name, l[0].bytes.as_slice()), ("image/png", "migrated", &[0x89, b'P', b'N', b'G'][..]));
        assert_eq!((l[1].mime.as_str(), l[1].name), ("image/jpeg", "bg"));
    }

    #[test]
    fn rewrite_replaces_elements_and_backgrounds_and_names_the_slides() {
        let mut d = deck();
        let l = legacy_images(&d);
        let refs: HashMap<String, String> = l.iter().enumerate().map(|(i, x)| (x.url.clone(), format!("kbfile:{i}"))).collect();
        let changed = rewrite(&mut d, &refs);
        assert_eq!(changed, ["a", "b"]);
        let a = &d.slides[0].data;
        assert_eq!(a.elements[0].s("storagePath"), Some("kbfile:0"));
        assert_eq!(a.elements[1].s("storagePath"), Some("kbfile:old"));
        assert_eq!(a.elements[2].s("storagePath"), Some("kbfile:0"));
        assert_eq!(d.slides[1].data.background(), Background::Image("kbfile:1".into()));
        assert!(legacy_images(&d).is_empty());
        // A second pass changes nothing.
        assert!(rewrite(&mut d, &refs).is_empty());
    }

    #[test]
    fn data_urls_decode_with_or_without_base64() {
        assert_eq!(decode_data_url("data:text/plain,a%20b"), Some(("text/plain".into(), b"a b".to_vec())));
        assert_eq!(decode_data_url("data:;base64,QUJD"), Some(("application/octet-stream".into(), b"ABC".to_vec())));
        assert_eq!(decode_data_url("kbfile:x"), None);
    }
}
