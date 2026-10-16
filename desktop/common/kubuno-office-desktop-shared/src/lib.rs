//! What every Kubuno Office desktop app shares to talk to the server.
//!
//! [`session`] decides when to fetch, compare and save - every rule that protects the user's work (the digest
//! guard, the editing session's join/ping/leave, conflicts only a click resolves, the terminal « too large »,
//! the crash journal) - over two traits each app implements with its own routes ([`session::Server`],
//! [`session::Journal`]). [`live`] runs a session on its own thread for an open window. [`collab`] is the core's
//! real-time collaboration room (the Yjs transport). [`ids`] mints the
//! unique keys both need. [`single`] keeps one window per document (a second open focuses the first).
//! [`ribbon`] is the shared "no document open" rule: the ribbon shows only the « Fichier » tab until a
//! document is open, like the web's `ModuleHome`.

pub mod collab;
pub mod ids;
pub mod live;
pub mod ribbon;
pub mod session;
pub mod single;
