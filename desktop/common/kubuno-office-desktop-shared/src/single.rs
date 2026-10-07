//! One window per document, like Word (core `desktop/README.md`, "Single instance").
//!
//! Each Office app opens one document per process. Before it shows anything, the app [`claim`]s the document it
//! was asked for: when a window of the **same app**, in the **same profile**, already shows that document (the same
//! server document, or the same local file however its path is spelt: case, `\\?\`, `/` or `\`, `..`), the launch
//! hands its command line to that process, which brings its window to the front (restored, focused, its taskbar
//! button flashing when the system refuses the focus), and exits with code 0. Another document opens in a window
//! of its own, as before; a new blank document or the sample is never single.
//!
//! The coordination is Kubuno Desktop's single-instance primitive (`kubuno-desktop-single-instance`: a named mutex
//! and a named pipe restricted to the user on Windows, a lock file and a Unix socket on Linux and macOS), keyed by
//! [`InstanceKey::per_document`]. A crashed window never blocks its document: the OS releases the lock with the
//! process.

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use kubuno_desktop_single_instance::{acquire, document_of_file, document_of_server, Activation, InstanceKey, Outcome, Primary};

/// The document a launch opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Document<'a> {
    /// A local file (Documents' `content.json`, a workbook, a diagram…).
    File(&'a Path),
    /// A document of a server: the server's URL (empty when it cannot be known yet) and the document's id.
    Server { server: &'a str, id: &'a str },
}

impl Document<'_> {
    /// The document's identity in the key.
    pub fn identity(&self) -> String {
        match self {
            Document::File(path) => document_of_file(path),
            Document::Server { server, id } => document_of_server(server, id),
        }
    }
}

/// What the start does after [`claim`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// Open the document: this process holds it (or nothing is to be held).
    Run,
    /// Another window of this app shows the document and was brought to the front: exit with this code.
    Exit(i32),
}

/// [`claim`] for an app's command line: `doc` (a server document, which wins) or `file`; `server` gives the URL of the
/// account the document opens as (asked only for a server document; `None` when unknown).
pub fn claim_launch(program: &str, profile: &Path, file: Option<&Path>, doc: Option<&str>, server: impl FnOnce() -> Option<String>) -> Claim {
    match (doc.map(str::trim).filter(|d| !d.is_empty()), file) {
        (Some(id), _) => {
            let server = server().unwrap_or_default();
            claim(program, profile, Some(Document::Server { server: &server, id }))
        }
        (None, Some(path)) => claim(program, profile, Some(Document::File(path))),
        (None, None) => Claim::Run,
    }
}

/// The key of `document` for `program` in the profile `profile`.
pub fn key(program: &str, profile: &Path, document: &Document<'_>) -> InstanceKey {
    InstanceKey::per_document(program, profile, &document.identity())
}

/// The document this process holds, for the life of the process.
static HELD: Mutex<Option<Primary>> = Mutex::new(None);

/// Takes `document` for this process, or hands the launch to the window that already shows it (see the module
/// doc). `None`: a new document, nothing to hold. Call it from the main thread, before any window.
pub fn claim(program: &str, profile: &Path, document: Option<Document<'_>>) -> Claim {
    let Some(document) = document else { return Claim::Run };
    let key = key(program, profile, &document);
    match acquire(&key, &Activation::from_env(), kubuno_desktop_single_instance::DEFAULT_WAIT) {
        Ok(Outcome::Primary(mut primary)) => {
            primary.serve(|activation| {
                tracing::info!("[single] the document of this window was opened again ({:?}): bringing the window to the front", activation.args);
                focus_window();
            });
            *HELD.lock().unwrap_or_else(PoisonError::into_inner) = Some(primary);
            Claim::Run
        }
        Ok(Outcome::Forwarded { pid }) => {
            tracing::info!("[single] {} is already open in process {pid}: its window comes to the front, this launch exits", document.identity());
            Claim::Exit(0)
        }
        Ok(Outcome::Unreachable(e)) => {
            // A window that holds the document but does not answer (hung): better a second editor, whose saves the
            // server's digest guard still protects, than a document the user cannot open at all.
            tracing::warn!("[single] the window holding {} does not answer ({e}): opening it anyway", document.identity());
            Claim::Run
        }
        Err(e) => {
            tracing::warn!("[single] the per-document lock is unavailable ({e}): opening anyway");
            Claim::Run
        }
    }
}

/// Brings this process's window to the front.
fn focus_window() {
    #[cfg(windows)]
    {
        match kubuno_desktop_single_instance::windows::focus_process_window() {
            Some(foreground) => tracing::info!("[single] window brought to the front (foreground: {foreground})"),
            None => tracing::warn!("[single] no window to bring to the front yet"),
        }
    }
    #[cfg(not(windows))]
    {
        // The window systems of Linux and macOS activate a running app's window themselves when asked by the
        // desktop; a native window, once there is one, raises itself here.
        tracing::debug!("[single] activation received");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_document_one_key_however_it_is_named() {
        let dir = std::env::temp_dir().join(format!("kubuno-single-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).expect("dir");
        let file = dir.join("Rapport.json");
        std::fs::write(&file, b"{}").expect("write");
        let profile = dir.join("profile");
        let k = |d: Document<'_>| key("kubuno-documents", &profile, &d);
        let dotted = dir.join("sub").join("..").join("Rapport.json");
        assert_eq!(k(Document::File(&file)), k(Document::File(&dotted)));
        #[cfg(windows)]
        {
            let upper = std::path::PathBuf::from(file.to_string_lossy().to_uppercase());
            let verbatim = std::path::PathBuf::from(format!(r"\\?\{}", file.display()));
            assert_eq!(k(Document::File(&file)), k(Document::File(&upper)), "case");
            assert_eq!(k(Document::File(&file)), k(Document::File(&verbatim)), r"\\?\ prefix");
        }
        let other = dir.join("Autre.json");
        assert_ne!(k(Document::File(&file)), k(Document::File(&other)), "another document, another window");
        let a = Document::Server { server: "https://cloud.example.org/", id: "3f2a" };
        assert_eq!(k(a), k(Document::Server { server: "HTTPS://cloud.example.org", id: "3f2a" }));
        assert_ne!(k(a), k(Document::Server { server: "https://cloud.example.org", id: "77aa" }));
        assert_ne!(k(a), key("kubuno-spreadsheets", &profile, &a), "another app");
        assert_ne!(k(a), key("kubuno-documents", &dir.join("other-profile"), &a), "another profile");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_document_is_never_single() {
        assert_eq!(claim("kubuno-documents", Path::new("/p"), None), Claim::Run);
    }
}
