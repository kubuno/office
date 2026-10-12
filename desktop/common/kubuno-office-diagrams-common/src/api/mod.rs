//! Talking to the server: [`client`] knows the diagram routes, [`remote`] makes them the shared office
//! save session's host (with the crash journal). The session itself and its worker thread are the ones
//! Documents uses (`kubuno_office_documents_common::api::{session, live}`).

pub mod client;
pub mod remote;

pub use kubuno_office_documents_common::api::{live, session};
