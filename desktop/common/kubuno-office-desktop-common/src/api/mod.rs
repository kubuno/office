//! Talking to the server.
//!
//! [`client`] knows the routes and nothing about when to call them, [`session`] knows when — which
//! is where every rule that protects the user's work lives —, [`remote`] plugs the two together
//! with the crash journal, and [`live`] runs a session on its own thread for an open window.

#![allow(dead_code)]

pub mod client;
pub mod remote;

// The save session and its thread are shared by every Office desktop app (`kubuno-office-desktop-shared`);
// re-exported at their former place.
pub use kubuno_office_desktop_shared::{live, session};
