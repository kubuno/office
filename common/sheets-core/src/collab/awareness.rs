//! Presence: the y-protocols awareness protocol (`y-protocols/awareness.js`) as the web uses it
//! (`web/src/collab/collabProvider.ts`, `web/src/collab/presence.tsx`).
//!
//! An awareness update is `varUint(n)` then, per client, `varUint(clientID) varUint(clock)
//! varString(JSON.stringify(state))` (`null` for a client that left). The web relays it as a text frame
//! `{"kbAwareness": base64(update)}`. A client renews its state every 15 s; a peer not heard from for 30 s
//! is dropped (`outdatedTimeout = 30000`). Times are milliseconds the caller passes in.

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json};

use super::CollabError;

/// `outdatedTimeout` of y-protocols: a peer silent for this long is gone.
pub const OUTDATED_MS: u64 = 30_000;

/// The palette of `userColor` (`presence.tsx:16-20`).
const PALETTE: [&str; 10] = ["#1a73e8", "#d93025", "#1e8e3e", "#f9ab00", "#9334e6", "#e8710a", "#12b5cb", "#d01884", "#7cb342", "#3949ab"];

/// `userColor(id)` (`presence.tsx:23-27`): a stable colour per user id (hash over UTF-16 units).
pub fn user_color(id: &str) -> &'static str {
    let mut h: u32 = 0;
    for u in id.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(u as u32);
    }
    PALETTE[(h % PALETTE.len() as u32) as usize]
}

// ── lib0 encoding ───────────────────────────────────────────────────────────

fn write_var_uint(out: &mut Vec<u8>, mut n: u64) {
    while n > 0x7f {
        out.push(0x80 | (n & 0x7f) as u8);
        n >>= 7;
    }
    out.push(n as u8);
}

fn write_var_string(out: &mut Vec<u8>, s: &str) {
    write_var_uint(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn var_uint(&mut self) -> Result<u64, CollabError> {
        let mut n: u64 = 0;
        let mut shift = 0;
        loop {
            let byte = *self.b.get(self.at).ok_or_else(|| CollabError::BadAwareness("truncated number".into()))?;
            self.at += 1;
            if shift < 64 {
                n |= ((byte & 0x7f) as u64) << shift;
            }
            if byte < 0x80 {
                return Ok(n);
            }
            shift += 7;
            if shift > 70 {
                return Err(CollabError::BadAwareness("number too long".into()));
            }
        }
    }

    fn var_string(&mut self) -> Result<String, CollabError> {
        let len = self.var_uint()? as usize;
        let end = self.at.checked_add(len).filter(|e| *e <= self.b.len()).ok_or_else(|| CollabError::BadAwareness("truncated string".into()))?;
        let s = std::str::from_utf8(&self.b[self.at..end]).map_err(|e| CollabError::BadAwareness(e.to_string()))?;
        self.at = end;
        Ok(s.to_string())
    }
}

/// One entry of an awareness update.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub client: u64,
    pub clock: u64,
    /// `None`: the client left (`null`).
    pub state: Option<Json>,
}

/// `encodeAwarenessUpdate`.
pub fn encode(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    write_var_uint(&mut out, entries.len() as u64);
    for e in entries {
        write_var_uint(&mut out, e.client);
        write_var_uint(&mut out, e.clock);
        let text = match &e.state {
            Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "null".into()),
            None => "null".into(),
        };
        write_var_string(&mut out, &text);
    }
    out
}

/// The entries of an awareness update.
pub fn decode(bytes: &[u8]) -> Result<Vec<Entry>, CollabError> {
    let mut r = Reader { b: bytes, at: 0 };
    let n = r.var_uint()?;
    let mut out = Vec::new();
    for _ in 0..n {
        let client = r.var_uint()?;
        let clock = r.var_uint()?;
        let text = r.var_string()?;
        let state: Json = serde_json::from_str(&text).map_err(|e| CollabError::BadAwareness(e.to_string()))?;
        out.push(Entry { client, clock, state: if state.is_null() { None } else { Some(state) } });
    }
    Ok(out)
}

// ── base64 (the text channel carries the binary update) ─────────────────────

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `btoa` of the bytes.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

/// `atob` to bytes (`None`: not base64).
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits = 0;
    for c in text.bytes() {
        if c == b'=' {
            break;
        }
        let v = B64.iter().position(|x| *x == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// A text frame of the room.
#[derive(Debug, Clone, PartialEq)]
pub enum TextFrame {
    /// `{"type":"sync","empty":bool}`: the server's greeting; `empty` = the room had no state.
    Sync { empty: bool },
    /// `{"kbAwareness": base64}`.
    Awareness(Vec<u8>),
    /// Anything else (relayed JSON).
    Other(Json),
}

/// Reads a text frame (`None`: not JSON).
pub fn parse_text(text: &str) -> Option<TextFrame> {
    let v: Json = serde_json::from_str(text).ok()?;
    if v.get("type").and_then(Json::as_str) == Some("sync") {
        return Some(TextFrame::Sync { empty: v.get("empty").and_then(Json::as_bool).unwrap_or(false) });
    }
    if let Some(b) = v.get("kbAwareness").and_then(Json::as_str) {
        return base64_decode(b).map(TextFrame::Awareness);
    }
    Some(TextFrame::Other(v))
}

/// The text frame carrying an awareness update.
pub fn awareness_frame(update: &[u8]) -> String {
    serde_json::json!({ "kbAwareness": base64_encode(update) }).to_string()
}

// ── the awareness state ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Meta {
    clock: u64,
    state: Option<Json>,
    updated: u64,
}

/// A peer as the editor shows it (`usePresenceUsers`, `remoteSelections`, `RemoteCursors`).
#[derive(Debug, Clone, PartialEq)]
pub struct Peer {
    pub client: u64,
    pub user_id: Option<String>,
    pub name: String,
    pub color: String,
    pub avatar: Option<String>,
    /// `sel`: the active cell (column letters, 1-based row) and its sheet id.
    pub sel: Option<(String, u32, Option<String>)>,
    /// `cursor`: the pointer in the grid's content coordinates.
    pub cursor: Option<(f64, f64)>,
}

/// The awareness of one client: its own state and the peers'.
#[derive(Debug, Clone)]
pub struct Awareness {
    client: u64,
    local: Meta,
    peers: BTreeMap<u64, Meta>,
}

impl Awareness {
    /// `new Awareness(doc)`: the local state starts as `{}` at clock 0.
    pub fn new(client: u64, now: u64) -> Awareness {
        Awareness { client, local: Meta { clock: 0, state: Some(Json::Object(Map::new())), updated: now }, peers: BTreeMap::new() }
    }

    pub fn client(&self) -> u64 {
        self.client
    }

    pub fn local_state(&self) -> Option<&Json> {
        self.local.state.as_ref()
    }

    /// The update announcing our current state.
    pub fn encode_local(&self) -> Vec<u8> {
        encode(&[Entry { client: self.client, clock: self.local.clock, state: self.local.state.clone() }])
    }

    /// `setLocalState`: the clock moves on; returns the update to send.
    pub fn set_local_state(&mut self, state: Option<Json>, now: u64) -> Vec<u8> {
        self.local.clock += 1;
        self.local.state = state;
        self.local.updated = now;
        self.encode_local()
    }

    /// `setLocalStateField`; returns the update to send (`None`: the field already holds that value).
    pub fn set_local_field(&mut self, key: &str, value: Json, now: u64) -> Option<Vec<u8>> {
        let mut state = match &self.local.state {
            Some(Json::Object(o)) => o.clone(),
            _ => Map::new(),
        };
        if state.get(key) == Some(&value) {
            return None;
        }
        state.insert(key.to_string(), value);
        Some(self.set_local_state(Some(Json::Object(state)), now))
    }

    /// `removeAwarenessStates([self])`: the update telling the peers we left.
    pub fn leave(&mut self, now: u64) -> Vec<u8> {
        self.set_local_state(None, now)
    }

    /// `applyAwarenessUpdate`. Returns whether a peer changed and, when a peer announced our own client as
    /// gone while we are still here, the update re-announcing us.
    pub fn apply(&mut self, bytes: &[u8], now: u64) -> Result<(bool, Option<Vec<u8>>), CollabError> {
        let mut changed = false;
        let mut resend = None;
        for e in decode(bytes)? {
            if e.client == self.client {
                // Someone says we left: we did not; outvote it with a newer clock (y-protocols does the same).
                if e.state.is_none() && self.local.state.is_some() && e.clock >= self.local.clock {
                    self.local.clock = e.clock;
                    resend = Some(self.set_local_state(self.local.state.clone(), now));
                }
                continue;
            }
            let curr = self.peers.get(&e.client).map(|m| m.clock).unwrap_or(0);
            let known = self.peers.get(&e.client).is_some_and(|m| m.state.is_some());
            if curr < e.clock || (curr == e.clock && e.state.is_none() && known) {
                let before = self.peers.get(&e.client).and_then(|m| m.state.clone());
                if before != e.state {
                    changed = true;
                }
                self.peers.insert(e.client, Meta { clock: e.clock, state: e.state, updated: now });
            }
        }
        Ok((changed, resend))
    }

    /// The periodic check (`_checkInterval`): the renewal of our state to send (every 15 s) and whether a
    /// silent peer was dropped (after 30 s).
    pub fn tick(&mut self, now: u64) -> (Option<Vec<u8>>, bool) {
        let renew = if self.local.state.is_some() && now.saturating_sub(self.local.updated) >= OUTDATED_MS / 2 {
            Some(self.set_local_state(self.local.state.clone(), now))
        } else {
            None
        };
        let mut dropped = false;
        for m in self.peers.values_mut() {
            if m.state.is_some() && now.saturating_sub(m.updated) >= OUTDATED_MS {
                m.state = None;
                dropped = true;
            }
        }
        (renew, dropped)
    }

    /// The peers present (not us, with a `user`), in client order.
    pub fn peers(&self) -> Vec<Peer> {
        self.peers
            .iter()
            .filter_map(|(client, m)| {
                let st = m.state.as_ref()?;
                let user = st.get("user")?;
                let name = user.get("name").and_then(Json::as_str).filter(|s| !s.is_empty())?.to_string();
                let sel = st.get("sel").and_then(|s| {
                    let col = s.get("col")?.as_str()?.to_string();
                    let row = s.get("row")?.as_f64()?;
                    let sheet = s.get("sheet").and_then(Json::as_str).map(str::to_string);
                    (row >= 1.0).then_some((col, row as u32, sheet))
                });
                let cursor = st.get("cursor").and_then(|c| Some((c.get("x")?.as_f64()?, c.get("y")?.as_f64()?)));
                Some(Peer {
                    client: *client,
                    user_id: user.get("id").and_then(Json::as_str).map(str::to_string),
                    color: user.get("color").and_then(Json::as_str).unwrap_or("#1a73e8").to_string(),
                    avatar: user.get("avatar").and_then(Json::as_str).map(str::to_string),
                    name,
                    sel,
                    cursor,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn user_colors_match_the_web() {
        // presence.tsx userColor: ((h*31 + code) >>> 0) % 10 over UTF-16 units.
        assert_eq!(user_color(""), "#1a73e8");
        assert_eq!(user_color("a"), PALETTE[97 % 10]);
        let id = "299bb680-ff17-470e-97e3-f9ecd324228a";
        let mut h: u64 = 0;
        for c in id.chars() {
            h = (h * 31 + c as u64) & 0xffff_ffff;
        }
        assert_eq!(user_color(id), PALETTE[(h % 10) as usize]);
    }

    #[test]
    fn codec_and_base64_round_trip() {
        let e = vec![Entry { client: 3_000_000_000, clock: 300, state: Some(json!({"user": {"name": "Zoé"}})) }, Entry { client: 7, clock: 1, state: None }];
        assert_eq!(decode(&encode(&e)).expect("decodes"), e);
        for n in 0..10 {
            let b: Vec<u8> = (0..n).map(|i| (i * 37 + 5) as u8).collect();
            assert_eq!(base64_decode(&base64_encode(&b)), Some(b));
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert!(decode(&[5, 1]).is_err());
    }

    #[test]
    fn peers_come_and_go_like_y_protocols() {
        let mut a = Awareness::new(1, 0);
        let mut b = Awareness::new(2, 0);
        let u = b.set_local_field("user", json!({"name": "Bea", "color": "#d93025", "id": "u2"}), 10).expect("changed");
        let u = {
            let mut s = b.clone();
            s.set_local_field("sel", json!({"col": "C", "row": 4, "sheet": "s1"}), 11).unwrap_or(u)
        };
        let (changed, resend) = a.apply(&u, 20).expect("applies");
        assert!(changed && resend.is_none());
        let p = a.peers();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].name, "Bea");
        assert_eq!(p[0].sel, Some(("C".into(), 4, Some("s1".into()))));
        // An older clock is ignored.
        assert!(!a.apply(&b.encode_local(), 21).expect("applies").0);
        // Silent for 30 s: dropped.
        assert!(a.tick(20 + OUTDATED_MS).1);
        assert!(a.peers().is_empty());
        // Our own renewal every 15 s.
        assert!(Awareness::new(9, 0).tick(OUTDATED_MS / 2).0.is_some());
        // Leaving sends null; the peer drops us.
        let bye = b.leave(40);
        let mut c = Awareness::new(3, 0);
        c.apply(&encode(&[Entry { client: 2, clock: 5, state: Some(json!({"user": {"name": "B"}})) }]), 0).expect("applies");
        assert_eq!(c.peers().len(), 1);
        c.apply(&encode(&[Entry { client: 2, clock: 6, state: None }]), 1).expect("applies");
        assert!(c.peers().is_empty());
        assert!(!bye.is_empty());
    }

    #[test]
    fn text_frames() {
        assert_eq!(parse_text(r#"{"type":"sync","empty":true}"#), Some(TextFrame::Sync { empty: true }));
        let f = awareness_frame(&[1, 2, 3]);
        assert_eq!(parse_text(&f), Some(TextFrame::Awareness(vec![1, 2, 3])));
        assert_eq!(parse_text("x"), None);
    }
}
