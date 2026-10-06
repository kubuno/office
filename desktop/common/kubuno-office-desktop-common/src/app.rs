//! Starting the app: what every OS entry point does, in one place.
//!
//! An entry point (`desktop/windows/kubuno-office-desktop`, `desktop/linux/…`, `desktop/macos/…`) is a few
//! lines: it builds its [`Platform`] (the portable defaults, with what that OS overrides) and its [`UiHost`],
//! and calls [`run`].
//!
//! `kubuno-documents [--dark] [--culture fr|en] [--no-splash] [--zoom 50] [--sample | --live] [--doc <id> | content.json]`

use std::path::{Path, PathBuf};

use kubuno_office_docs_core::measure::FixedMeasure;

use crate::model::state::App;
use crate::platform::{self, Platform, UiHost};

/// The program's name, as the shell's token broker and the logs know it.
pub const PROGRAM: &str = "kubuno-documents";

/// How the app starts (its command line).
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// A `content_json` file to open instead of the built-in sample.
    pub file: Option<PathBuf>,
    /// `--dark`: the dark theme.
    pub dark: bool,
    /// `--culture fr|en`: the UI language (else the system's).
    pub culture: Option<String>,
    /// `--zoom 50`: the zoom to open at, in percent.
    pub zoom: Option<f32>,
    /// `--doc <id>`: a document of the server, opened as the account the shell shows.
    pub doc: Option<String>,
    /// The title bar's waffle and avatar show the offline sample (the shell controls' design data) instead of
    /// the account's: `--sample`, or what the platform adds ([`crate::platform::LaunchRules`]).
    pub sample: bool,
}

impl Options {
    /// The options of this process, read with the registered platform's rules.
    pub fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();
        Self { sample: platform::current().launch.sample_requested(&args), ..Self::parse(args.into_iter().skip(1)) }
    }

    /// Parses the arguments, program name excluded. Unknown `--options` are ignored.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Self {
        let mut options = Options::default();
        let mut args = args.into_iter();
        while let Some(a) = args.next() {
            match a.as_str() {
                "--dark" => options.dark = true,
                "--culture" => options.culture = args.next(),
                "--zoom" => options.zoom = args.next().and_then(|z| z.trim_end_matches('%').parse::<f32>().ok()),
                "--doc" => options.doc = args.next(),
                "--sample" => options.sample = true,
                "--no-splash" | "--light" => {}
                _ if a.starts_with("--") => {}
                _ => options.file = Some(PathBuf::from(a)),
            }
        }
        options
    }

    /// The document to open: the file named on the command line, else the sample. A file that cannot be
    /// read is reported and the sample opens — a word processor that exits with no window because one
    /// argument was wrong is worse than one that tells you.
    pub fn open_document(&self) -> App {
        match &self.file {
            Some(path) => App::open_file(path).unwrap_or_else(|why| {
                tracing::error!("[documents] {why}");
                App::default()
            }),
            None => App::default(),
        }
    }
}

/// Registers `platform`, reads the command line, and runs `ui` until it closes; returns the exit code.
///
/// A document of the server (`--doc <id>`) borrows its access tokens from the Kubuno shell's token broker
/// (verified to be the installed shell, or the sandbox's under `KUBUNO_SANDBOX_DIR`): Documents never holds
/// a password or a refresh token.
pub fn run(platform: Platform, ui: &dyn UiHost) -> i32 {
    let name = platform.name;
    if !platform::install(platform) {
        tracing::warn!("[documents] a platform was already registered; `{name}` is ignored");
    }
    let options = Options::from_args();
    if options.doc.is_some() {
        match kubuno_desktop_sync::tokens::BrokerProvider::for_app(PROGRAM) {
            Ok(p) => kubuno_desktop_sync::tokens::install(std::sync::Arc::new(p)),
            Err(e) => tracing::error!("[documents] no token broker: {e}"),
        }
    }
    ui.run(&options)
}

/// The portable user interface: the document as text, laid out and paginated by the same engine as the
/// Windows window (with fixed font metrics, since no system font is measured here). It is what the app
/// shows on a system without a native window yet, and what tests run.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextUi;

impl TextUi {
    /// The document as lines of text: a header, then each page's lines.
    pub fn render(document: &mut App) -> Vec<String> {
        document.editor.relayout(&FixedMeasure);
        let pages = document.editor.pages();
        let mut lines = vec![format!("Kubuno Documents — {}", document.title)];
        for (i, page) in pages.iter().enumerate() {
            lines.push(format!("── page {} / {} ──", i + 1, pages.len()));
            for paragraph in &page.paragraphs {
                for line in &paragraph.lines {
                    lines.push(line.spans.iter().map(|s| s.text.as_str()).collect::<String>());
                }
            }
        }
        lines
    }

    /// The text of the file at `path` (or of the sample when it cannot be read).
    pub fn render_file(path: Option<&Path>) -> Vec<String> {
        let options = Options { file: path.map(Path::to_path_buf), ..Options::default() };
        TextUi::render(&mut options.open_document())
    }
}

impl UiHost for TextUi {
    fn run(&self, options: &Options) -> i32 {
        if options.doc.is_some() {
            eprintln!("{PROGRAM}: a server document (--doc) opens in a native window only; showing the local document");
        }
        for line in TextUi::render(&mut options.open_document()) {
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
        let o = Options::parse(["--no-splash", "--dark", "--culture", "en", r"C:\docs\rapport.json"].map(String::from));
        assert!(o.dark);
        assert_eq!(o.culture.as_deref(), Some("en"));
        assert_eq!(o.file, Some(PathBuf::from(r"C:\docs\rapport.json")));
        assert_eq!(Options::parse([]).file, None);
        let o = Options::parse(["--doc", "3f2a", "--zoom", "50%", "--sample"].map(String::from));
        assert_eq!((o.doc.as_deref(), o.zoom, o.file, o.sample), (Some("3f2a"), Some(50.0), None, true));
    }

    #[test]
    fn a_missing_file_opens_the_sample() {
        let o = Options::parse(["Z:/nowhere/missing.json".to_string()]);
        assert_eq!(o.open_document().title, App::default().title);
    }

    #[test]
    fn the_text_interface_paginates_the_sample() {
        let lines = TextUi::render_file(None);
        assert!(lines[0].starts_with("Kubuno Documents — "));
        assert_eq!(lines[1], "── page 1 / 1 ──");
        assert!(lines.iter().any(|l| l.contains("Kubuno Documents") && !l.contains('—')), "the heading is laid out: {lines:?}");
    }
}
