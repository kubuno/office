//! Starting the app: what every OS entry point does, in one place.
//!
//! An entry point (`desktop/windows/kubuno-office-spreadsheets`, later `desktop/linux/…`, `desktop/macos/…`)
//! is a few lines: it builds its [`Platform`] and its [`UiHost`] and calls [`run`].
//!
//! `kubuno-spreadsheets [--dark] [--culture fr|en] [--no-splash] [--zoom 100] [--sample | --live] [--doc <id> | workbook.kbcal]`

use std::path::PathBuf;

use kubuno_office_sheets_core::formula::eval::Ctx;
use kubuno_office_sheets_core::model::workbook::Workbook;
use kubuno_office_sheets_core::{addr, grid};

use crate::platform::{self, Platform, SystemEnv, UiHost};

/// The program's name, as the shell's token broker and the logs know it.
pub const PROGRAM: &str = "kubuno-spreadsheets";

/// How the app starts (its command line).
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// A workbook file to open (a content file `.kbcal`, gzipped or not, or the app's own bundle).
    pub file: Option<PathBuf>,
    /// `--dark`: the dark theme.
    pub dark: bool,
    /// `--culture fr|en`: the UI language (else the system's).
    pub culture: Option<String>,
    /// `--zoom 100`: the zoom to open at, in percent.
    pub zoom: Option<f32>,
    /// `--doc <id>`: a workbook of the server, opened as the account the shell shows.
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
                "--zoom" => options.zoom = args.next().and_then(|z| z.trim_end_matches('%').parse::<f32>().ok()),
                "--doc" => options.doc = args.next(),
                "--sample" => options.sample = true,
                "--no-splash" | "--light" | "--live" => {}
                _ if a.starts_with("--") => {}
                _ => options.file = Some(PathBuf::from(a)),
            }
        }
        options
    }

    /// The workbook to open: the file named on the command line, else the sample. A file that cannot be
    /// read is reported and the sample opens.
    pub fn open_workbook(&self) -> Workbook {
        match &self.file {
            Some(path) => open_file(path).unwrap_or_else(|why| {
                tracing::error!("[spreadsheets] {why}");
                crate::sample::workbook()
            }),
            None => crate::sample::workbook(),
        }
    }
}

/// Reads a workbook file; its title is the file name without the extension.
pub fn open_file(path: &std::path::Path) -> Result<Workbook, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let title = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut wb = Workbook::from_bytes(&bytes, &title).map_err(|e| format!("{}: {e}", path.display()))?;
    if wb.title.is_empty() {
        wb.title = title;
    }
    Ok(wb)
}

/// Registers `platform`, reads the command line, and runs `ui` until it closes; returns the exit code.
///
/// A workbook of the server (`--doc <id>`) borrows its access tokens from the Kubuno shell's token broker
/// (the installed shell's, or the sandbox's under `KUBUNO_SANDBOX_DIR`): Spreadsheets never holds a
/// password or a refresh token.
pub fn run(platform: Platform, ui: &dyn UiHost) -> i32 {
    let name = platform.name;
    if !platform::install(platform) {
        tracing::warn!("[spreadsheets] a platform was already registered; `{name}` is ignored");
    }
    let options = Options::from_args();
    if options.doc.is_some() {
        match kubuno_desktop_sync::tokens::BrokerProvider::for_app(PROGRAM) {
            Ok(p) => kubuno_desktop_sync::tokens::install(std::sync::Arc::new(p)),
            Err(e) => tracing::error!("[spreadsheets] no token broker: {e}"),
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

/// The portable user interface: each sheet's used range as tab-separated text, values as the grid shows
/// them. What the app shows on a system without a native window yet, and what tests run.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextUi;

impl TextUi {
    /// The workbook as lines of text.
    pub fn render(wb: &Workbook, lang: &str) -> Vec<String> {
        let env = SystemEnv::default();
        let names = wb.names_map();
        let by_name = wb.sheets_by_name();
        let mut lines = vec![format!("Kubuno Spreadsheets - {}", wb.title)];
        for (i, sheet) in wb.sheets.iter().enumerate() {
            lines.push(format!("── {} ──", sheet.meta.name));
            let Some(data) = wb.data(i, &names, by_name.as_ref()) else { continue };
            let ctx = Ctx::new(&env);
            let spill = kubuno_office_sheets_core::formula::spill::compute_spills(data, &ctx);
            let (mc, mr) = sheet.used_extent();
            if sheet.cells.is_empty() {
                continue;
            }
            for r in 1..=mr {
                let row: Vec<String> = (0..=mc)
                    .map(|c| {
                        let key = addr::key(c, r);
                        grid::shown(sheet.cells.get(&key), &key, &sheet.style_at(c, r), data, &ctx, Some(&spill), lang).text
                    })
                    .collect();
                lines.push(row.join("\t").trim_end().to_string());
            }
        }
        lines
    }
}

impl UiHost for TextUi {
    fn run(&self, options: &Options) -> i32 {
        if options.doc.is_some() {
            eprintln!("{PROGRAM}: a server workbook (--doc) opens in a native window only; showing the local workbook");
        }
        let lang = options.culture.clone().unwrap_or_else(|| "fr".into());
        for line in TextUi::render(&options.open_workbook(), &lang) {
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
        let o = Options::parse(["--dark", "--culture", "en", r"C:\x\budget.kbcal"].map(String::from));
        assert!(o.dark);
        assert_eq!(o.culture.as_deref(), Some("en"));
        assert_eq!(o.file, Some(PathBuf::from(r"C:\x\budget.kbcal")));
        let o = Options::parse(["--doc", "3f2a", "--zoom", "150%"].map(String::from));
        assert_eq!((o.doc.as_deref(), o.zoom), (Some("3f2a"), Some(150.0)));
    }

    #[test]
    fn the_text_interface_shows_the_sample_with_its_formats() {
        let lines = TextUi::render(&crate::sample::workbook(), "fr");
        assert!(lines[0].contains("Budget 2026"));
        let total = lines.iter().find(|l| l.starts_with("Total\t")).expect("a total row");
        assert!(total.contains("113\u{202F}500,00\u{A0}€"), "{total}");
        assert!(lines.iter().any(|l| l.starts_with("Total TTC\t136\u{202F}200,00")), "{lines:?}");
    }
}
