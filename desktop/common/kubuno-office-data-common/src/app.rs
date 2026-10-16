//! Starting the app: what every OS entry point does, in one place.
//!
//! An entry point (`desktop/windows/kubuno-office-data`, later `desktop/linux/…`, `desktop/macos/…`) is a
//! few lines: it builds its [`Platform`] and its [`UiHost`] and calls [`run`].
//!
//! `kubuno-data [--dark] [--culture fr|en] [--no-splash] [--sample | --live] [--doc <report-id>] [rapport.kbdrp]`

use std::io::Read as _;
use std::path::PathBuf;

use kubuno_office_data_core::model::{self, ReportDoc};

use crate::platform::{self, Platform, UiHost};

/// The program's name, as the shell's token broker and the logs know it.
pub const PROGRAM: &str = "kubuno-data";

/// How the app starts (its command line).
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// A report file to open: a `.kbdrp` (gzipped or plain JSON `{ version, report, pages, widgets }`).
    pub file: Option<PathBuf>,
    /// `--dark`: the dark theme.
    pub dark: bool,
    /// `--culture fr|en`: the UI language (else the system's).
    pub culture: Option<String>,
    /// `--doc <id>`: a report of the server, opened as the account the shell shows.
    pub doc: Option<String>,
    /// The title bar's waffle and avatar show the offline sample.
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
                "--doc" | "--report" => options.doc = args.next(),
                "--sample" => options.sample = true,
                "--no-splash" | "--light" | "--live" => {}
                _ if a.starts_with("--") => {}
                _ => options.file = Some(PathBuf::from(a)),
            }
        }
        options
    }

    /// The report to open: the file named on the command line, else the sample. A file that cannot be read
    /// is reported and the sample opens.
    pub fn open_report(&self) -> ReportDoc {
        match &self.file {
            Some(path) => open_file(path).unwrap_or_else(|why| {
                tracing::error!("[data] {why}");
                crate::sample::report()
            }),
            None => crate::sample::report(),
        }
    }
}

/// Reads a report file (gzipped or not); a nameless report takes the file's name.
pub fn open_file(path: &std::path::Path) -> Result<ReportDoc, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = gunzip_if_needed(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut doc = model::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if doc.report.title.is_empty() {
        doc.report.title = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    }
    Ok(doc)
}

/// Inflates gzip bytes (a `.kbdrp` from Drive); anything else is returned as is.
pub fn gunzip_if_needed(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if bytes.len() > 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&bytes[..]).read_to_end(&mut out).map_err(|e| format!("gzip : {e}"))?;
        Ok(out)
    } else {
        Ok(bytes)
    }
}

/// Registers `platform`, reads the command line, and runs `ui` until it closes; returns the exit code.
///
/// A report of the server (`--doc <id>`) borrows its access tokens from the Kubuno shell's token broker (the
/// installed shell's, or the sandbox's under `KUBUNO_SANDBOX_DIR`): Data never holds a password or a
/// refresh token.
pub fn run(platform: Platform, ui: &dyn UiHost) -> i32 {
    let name = platform.name;
    if !platform::install(platform) {
        tracing::warn!("[data] a platform was already registered; `{name}` is ignored");
    }
    let options = Options::from_args();
    if options.doc.is_some() {
        match kubuno_desktop_sync::tokens::BrokerProvider::for_app(PROGRAM) {
            Ok(p) => kubuno_desktop_sync::tokens::install(std::sync::Arc::new(p)),
            Err(e) => tracing::error!("[data] no token broker: {e}"),
        }
    }
    // One window per document (`kubuno_office_desktop_shared::single`): a report this app already shows, in
    // this profile, brings its window to the front instead of opening a second editor; this launch then exits.
    let profile = kubuno_desktop_account::paths::user_data_dir().unwrap_or_default();
    let server = || kubuno_desktop_sync::tokens::current_account().ok().flatten().map(|a| a.server_url);
    let claim = kubuno_office_desktop_shared::single::claim_launch(PROGRAM, &profile, options.file.as_deref(), options.doc.as_deref(), server);
    if let kubuno_office_desktop_shared::single::Claim::Exit(code) = claim {
        return code;
    }
    ui.run(&options)
}

/// The portable user interface: the report as text (title, pages, widgets). What the app shows on a system
/// without a native window yet, and what tests run.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextUi;

impl TextUi {
    pub fn render(doc: &ReportDoc) -> Vec<String> {
        let mut lines = vec![format!("Kubuno Data - {}", doc.report.title)];
        for page in &doc.pages {
            let n = doc.widgets.iter().filter(|w| w.page_id.as_deref() == Some(page.id.as_str())).count();
            lines.push(format!("  [{}] {} - {} visuel(s)", page.position, page.title, n));
            for w in doc.widgets.iter().filter(|w| w.page_id.as_deref() == Some(page.id.as_str())) {
                let title = w.title().unwrap_or_else(|| w.widget_type.clone());
                lines.push(format!("    - {} ({})", title, w.widget_type));
            }
        }
        lines
    }
}

impl UiHost for TextUi {
    fn run(&self, options: &Options) -> i32 {
        if options.doc.is_some() {
            eprintln!("{PROGRAM}: a server report (--doc) opens in a native window only; showing the sample");
        }
        for line in TextUi::render(&options.open_report()) {
            println!("{line}");
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line() {
        let o = Options::parse(["--dark", "--culture", "en", r"C:\x\r.kbdrp"].map(String::from));
        assert!(o.dark);
        assert_eq!(o.culture.as_deref(), Some("en"));
        assert_eq!(o.file, Some(PathBuf::from(r"C:\x\r.kbdrp")));
        let o = Options::parse(["--doc", "3f2a"].map(String::from));
        assert_eq!(o.doc.as_deref(), Some("3f2a"));
    }

    #[test]
    fn the_text_interface_shows_the_report() {
        let lines = TextUi::render(&crate::sample::report());
        assert!(lines[0].contains("Ventes"));
        assert!(lines.iter().any(|l| l.contains("visuel(s)")));
    }

    #[test]
    fn gzipped_reports_open() {
        use std::io::Write as _;
        let json = br#"{"version":1,"report":{"id":"r","title":""},"pages":[],"widgets":[]}"#;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(json).expect("gzip");
        let path = std::env::temp_dir().join(format!("kubuno-data-test-{}.kbdrp", std::process::id()));
        std::fs::write(&path, gz.finish().expect("gzip")).expect("write");
        let doc = open_file(&path).expect("opened");
        assert!(doc.report.title.starts_with("kubuno-data-test"));
        let _ = std::fs::remove_file(&path);
    }
}
