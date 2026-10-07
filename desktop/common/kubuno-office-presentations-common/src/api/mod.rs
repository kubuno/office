//! Talking to the server: [`client`] knows the routes, [`session`] when to call them (the per-slide save
//! rules), [`journal`] keeps the work on disk, [`live`] runs a session on its own thread for an open window.

pub mod client;
pub mod journal;
pub mod live;
pub mod session;
