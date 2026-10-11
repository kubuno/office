//! Talking to the server: [`client`] knows the board routes (metadata only — the board's *content* travels
//! over the Yjs WebSocket, see [`crate::collab`]). There is no digest-PATCH save session here, unlike
//! Documents and Diagrams.

pub mod client;
