//! The system's file dialogs (`IFileOpenDialog`, `IFileSaveDialog`): open a diagram, import draw.io / CSV,
//! save and export.

use std::path::PathBuf;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, IShellItem, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, SIGDN_FILESYSPATH};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

fn path_of(item: &IShellItem) -> Option<PathBuf> {
    unsafe {
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = PCWSTR(name.0).to_string().ok();
        CoTaskMemFree(Some(name.0 as *const _));
        path.map(PathBuf::from)
    }
}

/// The filter of one extension (`Diagramme (*.kbdia)`).
fn filter(ext: &str) -> (HSTRING, HSTRING) {
    let label = match ext {
        "kbdia" => "Diagramme Kubuno (*.kbdia)",
        "png" => "Image PNG (*.png)",
        "jpg" => "Image JPEG (*.jpg)",
        "pdf" => "PDF (*.pdf)",
        "drawio" => "draw.io (*.drawio)",
        "json" => "JSON (*.json)",
        _ => "*.*",
    };
    (HSTRING::from(label), HSTRING::from(format!("*.{ext}")))
}

/// Asks where to save `title.ext`; `None` when cancelled.
pub fn save_as(title: &str, ext: &str) -> Option<PathBuf> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let (label, spec) = filter(ext);
        let filters = [COMDLG_FILTERSPEC { pszName: PCWSTR(label.as_ptr()), pszSpec: PCWSTR(spec.as_ptr()) }];
        let _ = dialog.SetFileTypes(&filters);
        let ext_h = HSTRING::from(ext);
        let _ = dialog.SetDefaultExtension(&ext_h);
        let safe: String = title.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
        let name = HSTRING::from(format!("{}.{ext}", if safe.trim().is_empty() { "diagramme" } else { safe.trim() }));
        let _ = dialog.SetFileName(&name);
        let opts = dialog.GetOptions().ok()?;
        let _ = dialog.SetOptions(opts | FOS_OVERWRITEPROMPT | FOS_FORCEFILESYSTEM);
        dialog.Show(Some(GetForegroundWindow())).ok()?;
        path_of(&dialog.GetResult().ok()?)
    }
}

fn open(filters: &[(&str, &str)]) -> Option<PathBuf> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let owned: Vec<(HSTRING, HSTRING)> = filters.iter().map(|(l, s)| (HSTRING::from(*l), HSTRING::from(*s))).collect();
        let specs: Vec<COMDLG_FILTERSPEC> = owned.iter().map(|(l, s)| COMDLG_FILTERSPEC { pszName: PCWSTR(l.as_ptr()), pszSpec: PCWSTR(s.as_ptr()) }).collect();
        let _ = dialog.SetFileTypes(&specs);
        let opts = dialog.GetOptions().ok()?;
        let _ = dialog.SetOptions(opts | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM);
        dialog.Show(Some(GetForegroundWindow())).ok()?;
        path_of(&dialog.GetResult().ok()?)
    }
}

/// « Importer »: a draw.io / mxGraph file or a CSV (`.drawio,.xml,.csv`).
pub fn open_import() -> Option<PathBuf> {
    open(&[("draw.io, XML, CSV", "*.drawio;*.xml;*.csv"), ("*.*", "*.*")])
}

/// « Ouvrir »: a Kubuno diagram.
pub fn open_diagram() -> Option<PathBuf> {
    open(&[("Diagramme Kubuno (*.kbdia)", "*.kbdia;*.json"), ("*.*", "*.*")])
}
