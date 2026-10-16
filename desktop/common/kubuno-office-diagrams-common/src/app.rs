//! Starting the app: what every OS entry point does, in one place.
//!
//! `kubuno-diagrams [--dark] [--culture fr|en] [--no-splash] [--zoom 50] [--sample | --live] [--doc <id> | file.kbdia]`

use std::path::PathBuf;

use crate::doc::OpenDiagram;
use crate::platform::{self, Platform, UiHost};

/// The program's name, as the shell's token broker and the logs know it.
pub const PROGRAM: &str = "kubuno-diagrams";

/// How the app starts (its command line).
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// A `.kbdia` (or JSON) file to open instead of the sample.
    pub file: Option<PathBuf>,
    pub dark: bool,
    pub culture: Option<String>,
    /// The zoom to open at, in percent.
    pub zoom: Option<f32>,
    /// `--doc <id>`: a diagram of the server, opened as the account the shell shows.
    pub doc: Option<String>,
    /// The title bar's header shows the offline sample instead of the account's.
    pub sample: bool,
}

impl Options {
    pub fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();
        Self { sample: platform::current().launch.sample_requested(&args), ..Self::parse(args.into_iter().skip(1)) }
    }

    /// Parses the arguments, program name excluded. Unknown `--options` are ignored.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Self {
        let mut o = Options::default();
        let mut args = args.into_iter();
        while let Some(a) = args.next() {
            match a.as_str() {
                "--dark" => o.dark = true,
                "--culture" => o.culture = args.next(),
                "--zoom" => o.zoom = args.next().and_then(|z| z.trim_end_matches('%').parse::<f32>().ok()),
                "--doc" => o.doc = args.next(),
                "--sample" => o.sample = true,
                "--no-splash" | "--light" | "--live" => {}
                _ if a.starts_with("--") => {}
                _ => o.file = Some(PathBuf::from(a)),
            }
        }
        o
    }

    /// The diagram to show first: the file named on the command line, else the sample (a file that
    /// cannot be read is reported and the sample opens). A server diagram (`--doc`) is opened by the
    /// window's session; nothing is shown until it arrives.
    pub fn open_diagram(&self) -> OpenDiagram {
        match &self.file {
            Some(path) => OpenDiagram::open_file(path).unwrap_or_else(|why| {
                tracing::error!("[diagrams] {why}");
                OpenDiagram::sample()
            }),
            None => OpenDiagram::sample(),
        }
    }
}

/// Registers `platform`, reads the command line, and runs `ui` until it closes; returns the exit code.
///
/// A diagram of the server (`--doc <id>`) borrows its access tokens from the Kubuno shell's token broker
/// (the installed shell's, or the sandbox's under `KUBUNO_SANDBOX_DIR`): the app never holds a password.
pub fn run(platform: Platform, ui: &dyn UiHost) -> i32 {
    let name = platform.name;
    if !platform::install(platform) {
        tracing::warn!("[diagrams] a platform was already registered; `{name}` is ignored");
    }
    let options = Options::from_args();
    if options.doc.is_some() {
        match kubuno_desktop_sync::tokens::BrokerProvider::for_app(PROGRAM) {
            Ok(p) => kubuno_desktop_sync::tokens::install(std::sync::Arc::new(p)),
            Err(e) => tracing::error!("[diagrams] no token broker: {e}"),
        }
    }
    // One window per document (`kubuno_office_desktop_shared::single`): a document this app already shows, in this
    // profile, brings its window to the front instead of opening a second editor; this launch then exits.
    let profile = kubuno_desktop_account::paths::user_data_dir().unwrap_or_default();
    let server = || kubuno_desktop_sync::tokens::current_account().ok().flatten().map(|a| a.server_url);
    let claim = kubuno_office_desktop_shared::single::claim_launch(PROGRAM, &profile, options.file.as_deref(), options.doc.as_deref(), server);
    if let kubuno_office_desktop_shared::single::Claim::Exit(code) = claim {
        return code;
    }
    ui.run(&options)
}

/// The portable user interface: the diagram as text (its pages, shapes and connectors). What the app
/// shows on a system without a native window yet, and what tests run.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextUi;

impl TextUi {
    pub fn render(d: &OpenDiagram) -> Vec<String> {
        let mut lines = vec![format!("Kubuno Diagrams - {}", d.title)];
        for p in &d.pages {
            lines.push(format!("── {} ({} formes, {} connecteurs) ──", p.name, p.data.shapes.len(), p.data.connectors.len()));
            for s in &p.data.shapes {
                lines.push(format!("{} « {} » {}×{} @ {},{}", s.kind(), s.label(), s.w(), s.h(), s.x(), s.y()));
            }
        }
        lines
    }
}

impl UiHost for TextUi {
    fn run(&self, options: &Options) -> i32 {
        if options.doc.is_some() {
            eprintln!("{PROGRAM}: a server diagram (--doc) opens in a native window only; showing the local diagram");
        }
        for line in TextUi::render(&options.open_diagram()) {
            println!("{line}");
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_names_a_file_and_the_switches() {
        let o = Options::parse(["--dark", "--culture", "en", r"C:\d\plan.kbdia"].map(String::from));
        assert!(o.dark);
        assert_eq!(o.culture.as_deref(), Some("en"));
        assert_eq!(o.file, Some(PathBuf::from(r"C:\d\plan.kbdia")));
        let o = Options::parse(["--doc", "3f2a", "--zoom", "150%", "--sample"].map(String::from));
        assert_eq!((o.doc.as_deref(), o.zoom, o.sample), (Some("3f2a"), Some(150.0), true));
    }

    #[test]
    fn the_text_interface_lists_the_sample() {
        let lines = TextUi::render(&Options::default().open_diagram());
        assert!(lines[0].starts_with("Kubuno Diagrams - "));
        assert!(lines.iter().any(|l| l.contains("flow_start") && l.contains("Début")), "{lines:?}");
    }
}
