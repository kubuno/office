//! Starting the app: what every OS entry point does, in one place.
//!
//! An entry point (`desktop/windows/kubuno-office-maths`, later `desktop/linux/…`, `desktop/macos/…`) is a
//! few lines: it builds its [`Platform`] and its [`UiHost`] and calls [`run`].
//!
//! `kubuno-maths [--dark] [--culture fr|en] [--no-splash] [--zoom 100] [--sample | --live]
//!  [--doc <formula-id>] [formule.kbmath]`

use std::io::Read as _;
use std::path::{Path, PathBuf};

use crate::model::MathsDoc;
use crate::platform::{self, Platform, UiHost};

/// The program's name, as the shell's token broker and the logs know it.
pub const PROGRAM: &str = "kubuno-maths";

/// How the app starts (its command line).
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// A formula file to open: a `.kbmath` (gzipped or plain JSON `{ version, latex }`, or bare LaTeX).
    pub file: Option<PathBuf>,
    /// `--dark`: the dark theme.
    pub dark: bool,
    /// `--culture fr|en`: the UI language (else the system's).
    pub culture: Option<String>,
    /// `--doc <id>`: a formula of the server, opened as the account the shell shows.
    pub doc: Option<String>,
    /// `--zoom <percent>`: the initial zoom.
    pub zoom: Option<f32>,
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
                "--doc" | "--formula" => options.doc = args.next(),
                "--zoom" => options.zoom = args.next().and_then(|z| z.parse().ok()),
                "--sample" => options.sample = true,
                "--no-splash" | "--light" | "--live" => {}
                _ if a.starts_with("--") => {}
                _ => options.file = Some(PathBuf::from(a)),
            }
        }
        options
    }

    /// The formula to open: the file named on the command line, else the sample. A file that cannot be read
    /// is reported and the sample opens.
    pub fn open_document(&self) -> MathsDoc {
        match &self.file {
            Some(path) => open_file(path).unwrap_or_else(|why| {
                tracing::error!("[maths] {why}");
                crate::sample::document()
            }),
            None => crate::sample::document(),
        }
    }
}

/// Reads a formula file (gzipped or not): a `.kbmath` carries gzipped JSON `{ version, latex }`; a bare file
/// is read as LaTeX. A nameless document takes the file's stem.
pub fn open_file(path: &Path) -> Result<MathsDoc, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = gunzip_if_needed(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let latex = extract_latex(&bytes);
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Formule".to_string());
    Ok(MathsDoc::from_latex(name, latex))
}

/// The LaTeX inside a `.kbmath` payload: the `latex` field of a `{ version, latex }` JSON, else the bytes as
/// text (a bare LaTeX file).
pub fn extract_latex(bytes: &[u8]) -> String {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) {
        if let Some(latex) = v.get("latex").and_then(|v| v.as_str()) {
            return latex.to_string();
        }
    }
    String::from_utf8_lossy(bytes).to_string()
}

/// Inflates gzip bytes (a `.kbmath` from Drive); anything else is returned as is.
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
/// A formula of the server (`--doc <id>`) borrows its access tokens from the Kubuno shell's token broker (the
/// installed shell's, or the sandbox's under `KUBUNO_SANDBOX_DIR`): Maths never holds a password or a refresh
/// token.
pub fn run(platform: Platform, ui: &dyn UiHost) -> i32 {
    let name = platform.name;
    if !platform::install(platform) {
        tracing::warn!("[maths] a platform was already registered; `{name}` is ignored");
    }
    let options = Options::from_args();
    if options.doc.is_some() {
        match kubuno_desktop_sync::tokens::BrokerProvider::for_app(PROGRAM) {
            Ok(p) => kubuno_desktop_sync::tokens::install(std::sync::Arc::new(p)),
            Err(e) => tracing::error!("[maths] no token broker: {e}"),
        }
    }
    // One window per document (`kubuno_office_desktop_shared::single`).
    let profile = kubuno_desktop_account::paths::user_data_dir().unwrap_or_default();
    let server = || kubuno_desktop_sync::tokens::current_account().ok().flatten().map(|a| a.server_url);
    let claim = kubuno_office_desktop_shared::single::claim_launch(PROGRAM, &profile, options.file.as_deref(), options.doc.as_deref(), server);
    if let kubuno_office_desktop_shared::single::Claim::Exit(code) = claim {
        return code;
    }
    ui.run(&options)
}

/// The portable user interface: the formula as text (its pages and blocks). What the app shows on a system
/// without a native window yet, and what tests run.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextUi;

impl TextUi {
    pub fn render(doc: &MathsDoc) -> Vec<String> {
        use kubuno_office_math_core::engine::doc::MathBlock;
        let mut lines = vec![format!("Kubuno Maths - {}", doc.name)];
        for (i, page) in doc.pages.iter().enumerate() {
            lines.push(format!("  [{}] {} - {} bloc(s)", i + 1, page.name, page.blocks.len()));
            for b in &page.blocks {
                let desc = match b {
                    MathBlock::Formula { latex, .. } => format!("formule : {latex}"),
                    MathBlock::Graph { spec, .. } => format!("graphe : {} fonction(s)", spec.fns.len()),
                    MathBlock::Text { text, .. } => format!("texte : {text}"),
                };
                lines.push(format!("    - {desc}"));
            }
        }
        lines
    }
}

impl UiHost for TextUi {
    fn run(&self, options: &Options) -> i32 {
        if options.doc.is_some() {
            eprintln!("{PROGRAM}: a server formula (--doc) opens in a native window only; showing the sample");
        }
        for line in TextUi::render(&options.open_document()) {
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
        let o = Options::parse(["--dark", "--culture", "en", "--zoom", "150", r"C:\x\f.kbmath"].map(String::from));
        assert!(o.dark);
        assert_eq!(o.culture.as_deref(), Some("en"));
        assert_eq!(o.zoom, Some(150.0));
        assert_eq!(o.file, Some(PathBuf::from(r"C:\x\f.kbmath")));
        let o = Options::parse(["--doc", "3f2a"].map(String::from));
        assert_eq!(o.doc.as_deref(), Some("3f2a"));
    }

    #[test]
    fn extracts_latex_from_both_forms() {
        assert_eq!(extract_latex(br#"{"version":1,"latex":"x^{2}"}"#), "x^{2}");
        assert_eq!(extract_latex(b"a+b"), "a+b");
    }

    #[test]
    fn the_text_interface_shows_the_formula() {
        let lines = TextUi::render(&crate::sample::document());
        assert!(lines[0].contains("Kubuno Maths"));
        assert!(lines.iter().any(|l| l.contains("bloc(s)")));
    }

    #[test]
    fn gzipped_files_open() {
        use std::io::Write as _;
        let json = br#"{"version":1,"latex":"\\frac{1}{2}"}"#;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(json).expect("gzip");
        let path = std::env::temp_dir().join(format!("kubuno-maths-test-{}.kbmath", std::process::id()));
        std::fs::write(&path, gz.finish().expect("gzip")).expect("write");
        let doc = open_file(&path).expect("opened");
        assert!(doc.name.starts_with("kubuno-maths-test"));
        assert_eq!(doc.latex, "\\frac{1}{2}");
        let _ = std::fs::remove_file(&path);
    }
}
