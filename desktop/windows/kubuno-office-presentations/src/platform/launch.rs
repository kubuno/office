//! Another window of the app (« Nouveau », « Dupliquer »): this executable started again, on a server
//! presentation (`--doc <id>`) or on a new local presentation.

/// Starts `kubuno-presentations.exe` again, on server presentation `doc` when given.
pub fn new_window(doc: Option<&str>) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut cmd = std::process::Command::new(exe);
    if let Some(id) = doc {
        cmd.arg("--doc").arg(id);
    }
    if let Err(e) = cmd.spawn() {
        kubuno_desktop::tracing::warn!("[presentations] new window: {e}");
    }
}
