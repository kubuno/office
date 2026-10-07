//! Another window of the app (« Nouveau », « Dupliquer »): this executable started again, on a server
//! diagram (`--doc <id>`) or on a new local diagram.

/// Starts `kubuno-diagrams.exe` again, on server diagram `doc` when given.
pub fn new_window(doc: Option<&str>) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut cmd = std::process::Command::new(exe);
    if let Some(id) = doc {
        cmd.arg("--doc").arg(id);
    }
    if let Err(e) = cmd.spawn() {
        kubuno_desktop::tracing::warn!("[diagrams] new window: {e}");
    }
}
