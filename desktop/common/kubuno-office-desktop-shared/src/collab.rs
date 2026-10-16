//! The core's real-time collaboration room, on its own thread: the transport of the web's
//! `connectCollab` (`office/web/src/collab/collabProvider.ts`), reusable by every Office desktop app.
//!
//! The core serves one WebSocket per room, `/collab/<room>/sync` (core `server/src/collab/mod.rs`). A native
//! client authenticates with `?token=<access token>`. The server greets with the text frame
//! `{"type":"sync","empty":bool}`, then sends the persisted Yjs parts as binary frames; afterwards every
//! binary frame (a Yjs update) is persisted and relayed to everyone in the room - the sender included -, and
//! every text frame (presence) is relayed as is.
//!
//! This module only moves frames: the document and the presence are the app's (for spreadsheets,
//! `kubuno_office_sheets_core::collab`). It reconnects like the web (`min(1000·2ⁿ, 15000)` ms) with a fresh
//! token each time, and drops what is sent while the socket is down - on every (re)connection the app sends
//! its whole state again, as the web does in `onopen`.

use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// How long a read waits before the thread looks at what the window wants to send.
const POLL: Duration = Duration::from_millis(40);

/// The connection's state (`CollabStatus` of the web).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Connecting,
    Connected,
    /// The socket is down (the reason, for the log); the thread retries.
    Disconnected(String),
}

/// What the room tells the window.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Status(Status),
    /// A Yjs update.
    Binary(Vec<u8>),
    /// A text frame (the greeting, presence).
    Text(String),
}

/// Where the room's events go (the window's dispatcher).
pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

enum Out {
    Binary(Vec<u8>),
    Text(String),
    Close,
}

/// The web's reconnection delay after `retry` failures (`scheduleReconnect`).
pub fn backoff(retry: u32) -> Duration {
    Duration::from_millis((1000u64.saturating_mul(1u64 << retry.min(16))).min(15_000))
}

/// The room's URL on a server (`wsUrl` of the web; the token in the query, as native clients do).
pub fn room_url(server_url: &str, room: &str, token: &str) -> String {
    let base = server_url.trim_end_matches('/');
    let base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base.to_string()
    };
    format!("{base}/collab/{}/sync?token={}", encode(room), encode(token))
}

/// `encodeURIComponent`.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The window's handle on a room. Dropping it closes the room.
pub struct Room {
    tx: Sender<Out>,
}

impl Room {
    /// Opens the room on a thread. `url` runs on that thread before every (re)connection and returns the
    /// URL with a current token (`Err`: not possible now - retried after the backoff).
    pub fn open<F>(name: &str, url: F, sink: Sink) -> Room
    where
        F: Fn() -> Result<String, String> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name(format!("collab-{name}")).spawn(move || run(url, sink, rx));
        if let Err(e) = spawned {
            tracing::error!("[collab] thread: {e}");
        }
        Room { tx }
    }

    /// Sends a Yjs update (dropped while disconnected).
    pub fn send_binary(&self, update: Vec<u8>) {
        let _ = self.tx.send(Out::Binary(update));
    }

    /// Sends a text frame (presence).
    pub fn send_text(&self, text: String) {
        let _ = self.tx.send(Out::Text(text));
    }

    /// Closes the room (after sending what was queued before).
    pub fn close(&self) {
        let _ = self.tx.send(Out::Close);
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        self.close();
    }
}

fn set_poll(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) {
    let r = match socket.get_mut() {
        MaybeTlsStream::Plain(s) => s.set_read_timeout(Some(POLL)),
        MaybeTlsStream::Rustls(s) => s.sock.set_read_timeout(Some(POLL)),
        _ => Ok(()),
    };
    if let Err(e) = r {
        tracing::warn!("[collab] read timeout: {e}");
    }
}

/// Whether an error is the read timeout (nothing arrived).
fn idle(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if matches!(io.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut))
}

enum End {
    Closed,
    Lost(String),
}

fn session(url: &str, sink: &Sink, rx: &Receiver<Out>) -> End {
    let (mut socket, _) = match tungstenite::connect(url) {
        Ok(s) => s,
        Err(e) => return End::Lost(e.to_string()),
    };
    set_poll(&mut socket);
    // What was queued while down is stale: the app resends its state on Connected.
    while rx.try_recv().is_ok() {}
    sink(Event::Status(Status::Connected));
    loop {
        loop {
            match rx.try_recv() {
                Ok(Out::Binary(b)) => {
                    if let Err(e) = socket.send(Message::Binary(b)) {
                        return End::Lost(e.to_string());
                    }
                }
                Ok(Out::Text(t)) => {
                    if let Err(e) = socket.send(Message::Text(t)) {
                        return End::Lost(e.to_string());
                    }
                }
                Ok(Out::Close) | Err(TryRecvError::Disconnected) => {
                    let _ = socket.close(None);
                    let _ = socket.flush();
                    return End::Closed;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        match socket.read() {
            Ok(Message::Binary(b)) => sink(Event::Binary(b)),
            Ok(Message::Text(t)) => sink(Event::Text(t)),
            Ok(Message::Close(_)) => return End::Lost("closed by the server".into()),
            Ok(_) => {}
            Err(e) if idle(&e) => {
                let _ = socket.flush();
            }
            Err(e) => return End::Lost(e.to_string()),
        }
    }
}

fn run<F>(url: F, sink: Sink, rx: Receiver<Out>)
where
    F: Fn() -> Result<String, String>,
{
    let mut retry = 0u32;
    loop {
        sink(Event::Status(Status::Connecting));
        let reason = match url() {
            Ok(u) => match session(&u, &sink, &rx) {
                End::Closed => return,
                End::Lost(why) => {
                    retry = 0;
                    why
                }
            },
            Err(why) => why,
        };
        sink(Event::Status(Status::Disconnected(reason)));
        // Wait the backoff, but leave at once when the window closes the room.
        let wait = backoff(retry);
        retry = retry.saturating_add(1);
        let deadline = std::time::Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                break;
            }
            match rx.recv_timeout(left) {
                Ok(Out::Close) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_backoff_follow_the_web() {
        assert_eq!(room_url("https://dev.kubuno.com/", "office-spreadsheet:ab-1", "t.k+n"), "wss://dev.kubuno.com/collab/office-spreadsheet%3Aab-1/sync?token=t.k%2Bn");
        assert_eq!(room_url("http://127.0.0.1:18280", "r", "x"), "ws://127.0.0.1:18280/collab/r/sync?token=x");
        assert_eq!(backoff(0), Duration::from_millis(1000));
        assert_eq!(backoff(3), Duration::from_millis(8000));
        assert_eq!(backoff(4), Duration::from_millis(15_000));
        assert_eq!(backoff(40), Duration::from_millis(15_000));
    }

    #[test]
    fn a_room_that_cannot_connect_reports_and_closes() {
        let (tx, rx) = mpsc::channel();
        let sink: Sink = Arc::new(move |e| {
            let _ = tx.send(e);
        });
        let room = Room::open("t", || Err("no account".to_string()), sink);
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(Event::Status(Status::Connecting)));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(Event::Status(Status::Disconnected("no account".into()))));
        room.close();
    }

    #[test]
    fn a_room_talks_to_a_websocket_server() {
        // A local echo server standing in for the core's room.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
        let port = listener.local_addr().expect("addr").port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accepts");
            let mut ws = tungstenite::accept(stream).expect("handshake");
            ws.send(Message::Text(r#"{"type":"sync","empty":true}"#.into())).expect("greets");
            loop {
                match ws.read() {
                    Ok(Message::Binary(b)) => ws.send(Message::Binary(b)).expect("echo"),
                    Ok(Message::Text(t)) => ws.send(Message::Text(t)).expect("echo"),
                    _ => break,
                }
            }
        });
        let (tx, rx) = mpsc::channel();
        let sink: Sink = Arc::new(move |e| {
            let _ = tx.send(e);
        });
        let room = Room::open("t", move || Ok(format!("ws://127.0.0.1:{port}/collab/r/sync?token=x")), sink);
        let mut seen = Vec::new();
        while seen.len() < 3 {
            let e = rx.recv_timeout(Duration::from_secs(10)).expect("an event");
            if e == Event::Status(Status::Connected) {
                room.send_binary(vec![1, 2, 3]);
                room.send_text("hello".into());
            }
            seen.push(e);
        }
        assert!(seen.contains(&Event::Text(r#"{"type":"sync","empty":true}"#.into())));
        let next = rx.recv_timeout(Duration::from_secs(10)).expect("echo");
        let after = rx.recv_timeout(Duration::from_secs(10)).expect("echo");
        assert!([&next, &after].contains(&&Event::Binary(vec![1, 2, 3])));
        assert!([&next, &after].contains(&&Event::Text("hello".into())));
        room.close();
        let _ = server.join();
    }
}
