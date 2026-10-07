//! Kubuno Projects' platform-neutral core (vskubuno `docs/PROJECTS-DESKTOP.md` §4).
//!
//! A project of the office module is a set of rows behind a REST API, not a document file: the web editor
//! (`office/web/src/ProjectEditorPage.tsx` and `project/`) edits it one call at a time and asks the server to
//! schedule it (`POST /projects/:id/cpm`). This crate holds everything about a project that no platform does
//! differently, ported from the web and from the server so every client shows and edits the same plan:
//!
//! * [`date`] — civil dates and the display formats the editor uses;
//! * [`model`] — the project as the API serves it, held losslessly; the `.kbprj` export;
//! * [`calendar`] — working calendars (the server's service and the web's mirror);
//! * [`cpm`] — the server's critical-path scheduler;
//! * [`wbs`] — the server's outline numbering;
//! * [`rollup`] — completion rolled up from children;
//! * [`view`] — the table's rows (filter, sort, group, fold), predecessors, dates, status, CSV;
//! * [`gantt`] — the chart's geometry, hit testing and drag rules;
//! * [`views`] — the other plan views' data: board, calendar month, resource load, network, timeline band;
//! * [`change`] — one REST call and what it does to the rows;
//! * [`editor`] — the outbox, the undo history and the editor's commands.
//!
//! No UI and no OS API: the crate builds for `wasm32-unknown-unknown`, Linux, macOS and Windows. It never reads
//! a clock: the caller passes today's date and the current time.

pub mod calendar;
pub mod change;
pub mod cpm;
pub mod date;
pub mod editor;
pub mod gantt;
pub mod model;
pub mod rollup;
pub mod view;
pub mod views;
pub mod wbs;
