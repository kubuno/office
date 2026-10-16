//! Talking to the server.
//!
//! [`routes`] knows the spreadsheet routes, [`diff`] turns a workbook into the requests that bring the
//! server's copy to it, and [`remote`] plugs them into the save session shared with Kubuno Documents
//! (`kubuno_office_documents_common::api::{session, live}`: the digest guard, the editing session, the crash
//! journal and the worker thread - generic over the kind of document).

pub mod crypt;
pub mod diff;
pub mod remote;
pub mod routes;

pub use kubuno_office_desktop_shared::{live, session};
