//! Talking to the server: [`routes`] knows the project routes, [`worker`] runs them on a thread of their own
//! through the transport shared with the other Office apps, [`journal`] keeps unsent work on disk.

pub mod journal;
pub mod routes;
pub mod worker;
