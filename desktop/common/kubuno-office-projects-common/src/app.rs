//! Starting the app: what every OS entry point does, in one place.
//!
//! An entry point (`desktop/windows/kubuno-office-projects`, later `desktop/linux/…`, `desktop/macos/…`) is a
//! few lines: it builds its [`Platform`] and its [`UiHost`] and calls [`run`].
//!
//! `kubuno-projects [--dark] [--culture fr|en] [--no-splash] [--zoom day|week|month] [--view gantt|board|…]
//!  [--sample | --live] [--doc <id> | projet.kbprj | projet.json]`

use std::io::Read as _;
use std::path::PathBuf;

use kubuno_office_projects_core::date::Lang;
use kubuno_office_projects_core::model::ProjectData;
use kubuno_office_projects_core::view;

use crate::platform::{self, Platform, UiHost};

/// The program's name, as the shell's token broker and the logs know it.
pub const PROGRAM: &str = "kubuno-projects";

/// How the app starts (its command line).
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// A project file to open: the API's project JSON (what the journal holds) or a `.kbprj` export.
    pub file: Option<PathBuf>,
    /// `--dark`: the dark theme.
    pub dark: bool,
    /// `--culture fr|en`: the UI language (else the system's).
    pub culture: Option<String>,
    /// `--zoom day|week|month`: the Gantt's time scale.
    pub zoom: Option<String>,
    /// `--view gantt|board|calendar|resources|load|pert|roadmap`: the plan view to open (the web's `?view=`).
    pub view: Option<String>,
    /// `--doc <id>`: a project of the server, opened as the account the shell shows.
    pub doc: Option<String>,
    /// `--select <task id>`: a task selected at start (captures, tests).
    pub select: Option<String>,
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
                "--zoom" => options.zoom = args.next(),
                "--view" => options.view = args.next(),
                "--doc" => options.doc = args.next(),
                "--select" => options.select = args.next(),
                "--sample" => options.sample = true,
                "--no-splash" | "--light" | "--live" => {}
                _ if a.starts_with("--") => {}
                _ => options.file = Some(PathBuf::from(a)),
            }
        }
        options
    }

    /// The project to open: the file named on the command line, else the sample. A file that cannot be read
    /// is reported and the sample opens.
    pub fn open_project(&self) -> ProjectData {
        match &self.file {
            Some(path) => open_file(path).unwrap_or_else(|why| {
                tracing::error!("[projects] {why}");
                crate::sample::project()
            }),
            None => crate::sample::project(),
        }
    }

    /// The UI language of the dates.
    pub fn lang(&self) -> Lang {
        Lang::from_tag(self.culture.as_deref().unwrap_or("fr"))
    }
}

/// Reads a project file (gzipped or not); a nameless project takes the file's name.
pub fn open_file(path: &std::path::Path) -> Result<ProjectData, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = gunzip_if_needed(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut data = ProjectData::from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if data.project.title.is_empty() {
        data.project.title = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    }
    Ok(data)
}

/// Inflates gzip bytes (a `.kbprj` from Drive); anything else is returned as is.
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
/// A project of the server (`--doc <id>`) borrows its access tokens from the Kubuno shell's token broker (the
/// installed shell's, or the sandbox's under `KUBUNO_SANDBOX_DIR`): Projects never holds a password or a
/// refresh token.
pub fn run(platform: Platform, ui: &dyn UiHost) -> i32 {
    let name = platform.name;
    if !platform::install(platform) {
        tracing::warn!("[projects] a platform was already registered; `{name}` is ignored");
    }
    let options = Options::from_args();
    if options.doc.is_some() {
        match kubuno_desktop_sync::tokens::BrokerProvider::for_app(PROGRAM) {
            Ok(p) => kubuno_desktop_sync::tokens::install(std::sync::Arc::new(p)),
            Err(e) => tracing::error!("[projects] no token broker: {e}"),
        }
    }
    ui.run(&options)
}

/// The portable user interface: the plan as text (outline, dates, duration, completion). What the app shows
/// on a system without a native window yet, and what tests run.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextUi;

impl TextUi {
    pub fn render(data: &ProjectData, lang: Lang) -> Vec<String> {
        let today = platform::current().clock.today();
        let start = data.project_start(today);
        let progress = kubuno_office_projects_core::rollup::effective_progress(&data.tasks);
        let mut lines = vec![format!("Kubuno Projects — {}", data.project.title)];
        let rows = view::visible_rows(data, &view::Filter::default(), &Default::default());
        for r in rows {
            let t = &data.tasks[r.index];
            let fmt = |d| kubuno_office_projects_core::date::format(d, kubuno_office_projects_core::date::DateFormat::DayMonthYear2, lang);
            lines.push(format!(
                "{}{}\t{}\t{} j\t{}\t{}\t{} %",
                "  ".repeat(r.depth),
                t.wbs,
                t.name,
                t.duration_days,
                fmt(view::sched_start(t, start)),
                fmt(view::sched_end(t, start)),
                progress.get(&t.id).copied().unwrap_or(t.progress)
            ));
        }
        lines
    }
}

impl UiHost for TextUi {
    fn run(&self, options: &Options) -> i32 {
        if options.doc.is_some() {
            eprintln!("{PROGRAM}: a server project (--doc) opens in a native window only; showing the local project");
        }
        for line in TextUi::render(&options.open_project(), options.lang()) {
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
        let o = Options::parse(["--dark", "--culture", "en", "--view", "board", r"C:\x\plan.kbprj"].map(String::from));
        assert!(o.dark);
        assert_eq!(o.culture.as_deref(), Some("en"));
        assert_eq!(o.view.as_deref(), Some("board"));
        assert_eq!(o.file, Some(PathBuf::from(r"C:\x\plan.kbprj")));
        let o = Options::parse(["--doc", "3f2a", "--zoom", "week"].map(String::from));
        assert_eq!((o.doc.as_deref(), o.zoom.as_deref()), (Some("3f2a"), Some("week")));
    }

    #[test]
    fn the_text_interface_shows_the_outline() {
        let lines = TextUi::render(&crate::sample::project(), Lang::Fr);
        assert!(lines[0].contains("Déménagement"));
        assert!(lines.iter().any(|l| l.starts_with("  1.1\tInventaire du mobilier\t3 j\t5 oct. 26\t7 oct. 26\t100 %")), "{lines:?}");
    }

    #[test]
    fn gzipped_exports_open() {
        use std::io::Write as _;
        let json = br#"{"format":"kubuno-project/v1","title":"","status":"active","tasks":[{"id":"a","name":"A","position":0,"duration_days":2,"progress":0}],"deps":[],"resources":[]}"#;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(json).expect("gzip");
        let path = std::env::temp_dir().join(format!("kubuno-projects-test-{}.kbprj", std::process::id()));
        std::fs::write(&path, gz.finish().expect("gzip")).expect("write");
        let data = open_file(&path).expect("opened");
        assert_eq!(data.tasks.len(), 1);
        assert!(data.project.title.starts_with("kubuno-projects-test"));
        let _ = std::fs::remove_file(&path);
    }
}
