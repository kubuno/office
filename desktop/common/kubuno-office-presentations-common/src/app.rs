//! Starting the app: what every OS entry point does, in one place.
//!
//! `kubuno-presentations [--dark] [--culture fr|en] [--zoom 100] [--slide N] [--show] [--sample | --live] [--doc <id> | file.kbsld]`

use std::io::Read;
use std::path::{Path, PathBuf};

use kubuno_office_slides_core::model::{is_gzip, Deck};
use kubuno_office_slides_core::richtext;

use crate::platform::{self, Platform, UiHost};

/// The program's name, as the shell's token broker and the logs know it.
pub const PROGRAM: &str = "kubuno-presentations";

/// How the app starts (its command line).
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// A `.kbsld` (gzip or plain JSON) or a desktop bundle to open.
    pub file: Option<PathBuf>,
    pub dark: bool,
    pub culture: Option<String>,
    /// `--zoom 100`: the zoom to open at, in percent of the fitted size (`Ajusté` otherwise).
    pub zoom: Option<f32>,
    /// `--doc <id>`: a presentation of the server, opened as the account the shell shows.
    pub doc: Option<String>,
    /// `--slide N`: the slide shown first (1-based).
    pub slide: Option<usize>,
    /// `--show`: starts the slideshow.
    pub show: bool,
    /// The header shows the offline sample (`--sample`, or the platform's rule).
    pub sample: bool,
}

impl Options {
    pub fn from_args() -> Options {
        let args: Vec<String> = std::env::args().collect();
        Options { sample: platform::current().launch.sample_requested(&args), ..Options::parse(args.into_iter().skip(1)) }
    }

    /// The arguments, program name excluded; unknown `--options` are ignored.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Options {
        let mut o = Options::default();
        let mut args = args.into_iter();
        while let Some(a) = args.next() {
            match a.as_str() {
                "--dark" => o.dark = true,
                "--culture" => o.culture = args.next(),
                "--zoom" => o.zoom = args.next().and_then(|z| z.trim_end_matches('%').parse::<f32>().ok()),
                "--doc" => o.doc = args.next(),
                "--slide" => o.slide = args.next().and_then(|s| s.parse().ok()),
                "--show" => o.show = true,
                "--sample" => o.sample = true,
                "--no-splash" | "--light" | "--live" => {}
                _ if a.starts_with("--") => {}
                _ => o.file = Some(PathBuf::from(a)),
            }
        }
        o
    }
}

/// Reads a presentation file: gzip or plain JSON, a content file or a desktop bundle.
pub fn read_file(path: &Path) -> Result<Deck, String> {
    let raw = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = if is_gzip(&raw) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(raw.as_slice()).read_to_end(&mut out).map_err(|e| format!("gzip: {e}"))?;
        out
    } else {
        raw
    };
    let mut deck = Deck::from_json(&bytes).map_err(|e| e.to_string())?;
    if deck.title.is_empty() {
        deck.title = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    }
    Ok(deck)
}

/// Writes a local presentation: a `.kbsld` (gzip) gets the content file only — what the server stores; any
/// other path the desktop bundle (content plus order, hidden flags, title), plain JSON.
pub fn write_file(path: &Path, deck: &Deck) -> Result<(), String> {
    use std::io::Write;
    let kbsld = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("kbsld"));
    let bytes = if kbsld {
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(deck.content_json().as_bytes()).map_err(|e| e.to_string())?;
        enc.finish().map_err(|e| e.to_string())?
    } else {
        deck.to_bundle().into_bytes()
    };
    let tmp = path.with_extension("tmp-kubuno");
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// A small built-in presentation (no file named): a title slide, as a new presentation of the web starts.
pub fn sample_deck() -> Deck {
    let meta = serde_json::json!({ "presentation": { "title": "Présentation sans titre" }, "slides": [{ "id": "00000000-0000-4000-8000-000000000001", "position": 0 }] });
    let content = format!(
        r##"{{"version":1,"slides":{{"00000000-0000-4000-8000-000000000001":{{"elements":[{}],"background":{{"type":"color","color":"#ffffff"}},"notes":"","transition":{{"type":"none","duration":0.3}}}}}}}}"##,
        kubuno_office_slides_core::insert::placeholder_slide_elements(&mut kubuno_office_slides_core::insert::SeqIds::default(), &Default::default()).iter().map(|e| e.to_json()).collect::<Vec<_>>().join(",")
    );
    Deck::from_server(&meta, &content)
}

/// Registers `platform`, reads the command line and runs `ui` until it closes; returns the exit code. A server
/// presentation (`--doc <id>`) borrows its access tokens from the Kubuno shell's token broker.
pub fn run(platform: Platform, ui: &dyn UiHost) -> i32 {
    let name = platform.name;
    if !platform::install(platform) {
        tracing::warn!("[presentations] a platform was already registered; `{name}` is ignored");
    }
    let options = Options::from_args();
    if options.doc.is_some() {
        match kubuno_desktop_sync::tokens::BrokerProvider::for_app(PROGRAM) {
            Ok(p) => kubuno_desktop_sync::tokens::install(std::sync::Arc::new(p)),
            Err(e) => tracing::error!("[presentations] no token broker: {e}"),
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

/// The portable interface: the presentation as text (each slide's text boxes, in z-order).
#[derive(Debug, Default, Clone, Copy)]
pub struct TextUi;

impl TextUi {
    pub fn render(deck: &Deck) -> Vec<String> {
        let mut lines = vec![format!("Kubuno Presentations — {}", deck.title)];
        for (i, s) in deck.slides.iter().enumerate() {
            lines.push(format!("── diapositive {} / {}{} ──", i + 1, deck.slides.len(), if s.hidden { " (masquée)" } else { "" }));
            let mut els: Vec<_> = s.data.elements.iter().collect();
            els.sort_by(|a, b| a.z_index().total_cmp(&b.z_index()));
            for e in els {
                let text = richtext::paras_to_plain(&richtext::doc_to_paras(e.get("content")));
                if !text.is_empty() {
                    lines.extend(text.lines().map(str::to_string));
                }
            }
            if !s.data.notes().is_empty() {
                lines.push(format!("[notes] {}", s.data.notes()));
            }
        }
        lines
    }
}

impl UiHost for TextUi {
    fn run(&self, options: &Options) -> i32 {
        if options.doc.is_some() {
            eprintln!("{PROGRAM}: a server presentation (--doc) opens in a native window only");
            return 2;
        }
        let deck = match &options.file {
            Some(p) => match read_file(p) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("{PROGRAM}: {e}");
                    return 1;
                }
            },
            None => sample_deck(),
        };
        for l in TextUi::render(&deck) {
            println!("{l}");
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line() {
        let o = Options::parse(["--dark", "--doc", "3f2a", "--slide", "4", "--show", "--zoom", "150%"].map(String::from));
        assert!(o.dark && o.show);
        assert_eq!((o.doc.as_deref(), o.slide, o.zoom), (Some("3f2a"), Some(4), Some(150.0)));
        assert_eq!(Options::parse(["deck.kbsld".to_string()]).file, Some(PathBuf::from("deck.kbsld")));
    }

    #[test]
    fn a_kbsld_round_trips_gzipped_and_the_text_interface_reads_it() {
        let mut deck = sample_deck();
        let p = std::env::temp_dir().join(format!("kubuno-pres-{}.kbsld", std::process::id()));
        deck.slides[0].data.set_notes("hello");
        write_file(&p, &deck).expect("written");
        let raw = std::fs::read(&p).unwrap_or_default();
        assert!(is_gzip(&raw));
        let back = read_file(&p).expect("read");
        assert_eq!(back.slides[0].data, deck.slides[0].data);
        let lines = TextUi::render(&back);
        assert!(lines.iter().any(|l| l == "[notes] hello"), "{lines:?}");
        let _ = std::fs::remove_file(p);
    }
}
