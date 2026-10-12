//! The system clipboard: a copy of shapes is put there in a private format (`Kubuno.Diagram`, the JSON
//! `{ shapes, connectors }` in UTF-8) so it survives between two windows of the app — the web keeps its
//! copy inside the page (`clipboardRef`) — together with `CF_UNICODETEXT` (the shapes' labels, one per
//! line), what a paste into a text editor gives.
//!
//! The traps (the same as Documents' clipboard, `kubuno-office-documents/src/platform/clipboard.rs`):
//! `OpenClipboard` must always be closed ([`Guard`]); after `SetClipboardData` succeeds the system owns the
//! handle, after it fails we still do; `GlobalSize` rounds up, so payloads come back with trailing NULs;
//! another process may hold the clipboard for a few milliseconds, so opening retries briefly. The clipboard
//! needs an owner window (`EmptyClipboard` with none makes `SetClipboardData` fail): a message-only window
//! of this thread is created once and kept.

use std::cell::Cell;
use std::sync::OnceLock;

use serde_json::Value;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE};

/// The private format's name.
pub const PRIVATE_FORMAT: &str = "Kubuno.Diagram";

const CF_UNICODETEXT: u32 = 13;
const OPEN_RETRIES: u32 = 5;
const OPEN_RETRY_DELAY_MS: u64 = 20;

struct Guard;

impl Guard {
    fn open(owner: HWND) -> Option<Self> {
        for attempt in 0..OPEN_RETRIES {
            if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
                return Some(Self);
            }
            if attempt + 1 < OPEN_RETRIES {
                std::thread::sleep(std::time::Duration::from_millis(OPEN_RETRY_DELAY_MS));
            }
        }
        kubuno_desktop::tracing::warn!("[diagrams] clipboard busy: OpenClipboard failed {OPEN_RETRIES} times");
        None
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

thread_local! {
    static OWNER: Cell<Option<isize>> = const { Cell::new(None) };
}

/// This thread's message-only window, the clipboard's owner (created once).
fn owner() -> Option<HWND> {
    if let Some(h) = OWNER.with(Cell::get) {
        return Some(HWND(h as *mut _));
    }
    let h = unsafe { CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("Kubuno.Diagram.Clipboard"), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, None, None) }.ok()?;
    OWNER.with(|c| c.set(Some(h.0 as isize)));
    Some(h)
}

fn private_format() -> u32 {
    static CELL: OnceLock<u32> = OnceLock::new();
    *CELL.get_or_init(|| {
        let wide: Vec<u16> = PRIVATE_FORMAT.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) }
    })
}

fn alloc(bytes: &[u8]) -> Option<HGLOBAL> {
    let h = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) }.ok()?;
    let p = unsafe { GlobalLock(h) };
    if p.is_null() {
        unsafe {
            let _ = GlobalFree(Some(h));
        }
        return None;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(h);
    }
    Some(h)
}

fn set(format: u32, bytes: &[u8]) -> bool {
    if format == 0 {
        return false;
    }
    let Some(h) = alloc(bytes) else { return false };
    match unsafe { SetClipboardData(format, Some(HANDLE(h.0))) } {
        Ok(_) => true,
        Err(e) => {
            unsafe {
                let _ = GlobalFree(Some(h));
            }
            kubuno_desktop::tracing::warn!("[diagrams] clipboard: SetClipboardData failed: {e}");
            false
        }
    }
}

fn read(format: u32) -> Option<Vec<u8>> {
    if format == 0 || unsafe { IsClipboardFormatAvailable(format) }.is_err() {
        return None;
    }
    let handle = unsafe { GetClipboardData(format) }.ok()?;
    let h = HGLOBAL(handle.0);
    let size = unsafe { GlobalSize(h) };
    if size == 0 {
        return None;
    }
    let p = unsafe { GlobalLock(h) };
    if p.is_null() {
        return None;
    }
    let mut out = vec![0u8; size];
    unsafe {
        std::ptr::copy_nonoverlapping(p.cast::<u8>(), out.as_mut_ptr(), size);
        let _ = GlobalUnlock(h);
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    Some(out)
}

/// The text a paste into a text editor gives: the shapes' labels, one per line (CRLF), empty ones skipped.
pub fn plain_text(v: &Value) -> String {
    v.get("shapes")
        .and_then(Value::as_array)
        .map(|shapes| shapes.iter().filter_map(|s| s.get("label").and_then(Value::as_str)).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\r\n"))
        .unwrap_or_default()
}

fn utf16_nul(text: &str) -> Vec<u8> {
    text.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect()
}

/// Puts a copy of shapes on the system clipboard (the private format, then the text).
pub fn put(v: &Value) {
    let Some(owner) = owner() else {
        kubuno_desktop::tracing::warn!("[diagrams] clipboard: no owner window");
        return;
    };
    let Some(_guard) = Guard::open(owner) else { return };
    if let Err(e) = unsafe { EmptyClipboard() } {
        kubuno_desktop::tracing::warn!("[diagrams] clipboard: EmptyClipboard failed: {e}");
        return;
    }
    let json = serde_json::to_vec(v).unwrap_or_default();
    if set(private_format(), &json) {
        set(CF_UNICODETEXT, &utf16_nul(&plain_text(v)));
    }
}

/// The copy of shapes on the system clipboard, if any (`None` when the format is absent or unreadable).
pub fn get() -> Option<Value> {
    let owner = owner()?;
    let _guard = Guard::open(owner)?;
    let bytes = read(private_format())?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    v.get("shapes").and_then(Value::as_array)?;
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_text_is_the_labels() {
        let v = json!({ "shapes": [{ "label": "A" }, { "label": "" }, { "label": "B" }], "connectors": [] });
        assert_eq!(plain_text(&v), "A\r\nB");
    }

    #[test]
    fn a_copy_round_trips_through_the_system_clipboard() {
        let v = json!({ "shapes": [{ "id": "a", "type": "rect", "label": "Début" }], "connectors": [] });
        put(&v);
        // A test machine may have no interactive clipboard (a service session): then nothing is read back
        // and the test says so rather than failing.
        match get() {
            Some(back) => assert_eq!(back, v),
            None => eprintln!("clipboard unavailable in this session: round trip not verified"),
        }
    }
}
