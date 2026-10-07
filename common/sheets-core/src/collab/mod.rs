//! Real-time collaboration: the web editor's Y.Doc and presence, platform-neutral.
//!
//! The web binds a spreadsheet to a Yjs document in the core's generic collaboration room
//! `office-spreadsheet:<id>` (`web/src/SpreadsheetApp.tsx:643-680`, `web/src/collab/collabProvider.ts`):
//!
//! - a `Y.Map` named `cells:<sheetId>` per sheet — cell key (`A1`) → the stored cell object as plain JSON
//!   (`SA:3159-3201`);
//! - a `Y.Map` named `names` — the workbook's defined names, UPPERCASE name → definition (`SA:870-897`);
//!   this map is where defined names live (no REST route writes them);
//! - presence through the y-protocols awareness, relayed as text frames `{"kbAwareness": base64}`: the local
//!   state carries `user {id, name, color, avatar}`, `sel {col, row, sheet}` and `cursor {x, y}`
//!   (`SA:656-668`, `SA:3298-3301`, `web/src/collab/presence.tsx`).
//!
//! [`doc::SheetsDoc`] is that document (the `yrs` port of Yjs, the implementation the core's collaboration
//! service uses too); [`awareness`] is the presence protocol. The transport (a WebSocket) is the caller's.
//! Nothing here reads a clock or a random source: the client id and the time are passed in.

pub mod awareness;
pub mod doc;
pub mod json;

pub use awareness::{user_color, Awareness, Peer};
pub use doc::{Seed, SheetsDoc};

/// The room of a spreadsheet (`useCollab(\`office-spreadsheet:${ssId}\`)`, `SA:656`).
pub fn room(spreadsheet_id: &str) -> String {
    format!("office-spreadsheet:{spreadsheet_id}")
}

/// A failure to read a frame of the room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollabError {
    /// A binary frame is not a Yjs v1 update.
    BadUpdate(String),
    /// An awareness frame is malformed.
    BadAwareness(String),
}

impl std::fmt::Display for CollabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollabError::BadUpdate(e) => write!(f, "invalid Yjs update: {e}"),
            CollabError::BadAwareness(e) => write!(f, "invalid awareness update: {e}"),
        }
    }
}

impl std::error::Error for CollabError {}
