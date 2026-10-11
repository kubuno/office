//! Another window of the app (« Nouveau »): this executable started again, on a server board
//! (`--doc <id>`) or on a new local board.

/// Starts `kubuno-whiteboard.exe` again, on server board `doc` when given.
pub fn new_window(doc: Option<&str>) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut cmd = std::process::Command::new(exe);
    if let Some(id) = doc {
        cmd.arg("--doc").arg(id);
    }
    if let Err(e) = cmd.spawn() {
        kubuno_desktop::tracing::warn!("[whiteboard] new window: {e}");
    }
}
