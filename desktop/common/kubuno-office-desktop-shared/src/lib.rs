//! What every Kubuno Office desktop app shares to talk to the server.
//!
//! [`session`] decides when to fetch, compare and save — every rule that protects the user's work (the digest
//! guard, the editing session's join/ping/leave, conflicts only a click resolves, the terminal « too large »,
//! the crash journal) — over two traits each app implements with its own routes ([`session::Server`],
//! [`session::Journal`]). [`live`] runs a session on its own thread for an open window. [`ids`] mints the
//! unique keys both need.

pub mod ids;
pub mod live;
pub mod session;
