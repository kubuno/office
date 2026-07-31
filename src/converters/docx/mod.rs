//! DOCX (OOXML WordprocessingML) reader and writer.
//!
//! Split by domain so each part stays independently readable:
//! `read/` maps a `.docx` package onto our ProseMirror document model,
//! `write/` does the reverse. `xml`, `zip_io` and `model` are shared by both.

#[cfg(test)]
mod tests;

pub(crate) mod model;
pub(crate) mod read;
pub(crate) mod write;
pub(crate) mod xml;
pub(crate) mod zip_io;

pub use model::SectionInfo;
pub use read::import_docx;
pub use write::ctx::{CommentReplyIn, CommentThreadIn};
pub use write::{export_docx, export_docx_all, export_docx_full, export_docx_hf, export_docx_with_layout};
