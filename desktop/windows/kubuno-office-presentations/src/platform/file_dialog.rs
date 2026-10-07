//! The system's file dialogs (`IFileOpenDialog`, `IFileSaveDialog`): an image to insert, a presentation file to
//! open or to save a copy to.

use std::path::PathBuf;

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog, IFileSaveDialog, IShellItem, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, SIGDN_FILESYSPATH};

unsafe fn result_path(item: IShellItem) -> Option<PathBuf> {
    let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let path = PCWSTR(name.0).to_string().ok();
    CoTaskMemFree(Some(name.0 as *const _));
    path.map(PathBuf::from)
}

unsafe fn open(filters: &[COMDLG_FILTERSPEC]) -> Option<PathBuf> {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
    let _ = dialog.SetFileTypes(filters);
    let opts = dialog.GetOptions().ok()?;
    let _ = dialog.SetOptions(opts | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM);
    dialog.Show(Some(GetActiveWindow())).ok()?;
    result_path(dialog.GetResult().ok()?)
}

/// Asks for one image file; `None` when cancelled.
pub fn open_image() -> Option<PathBuf> {
    unsafe {
        open(&[
            COMDLG_FILTERSPEC { pszName: w!("Images"), pszSpec: w!("*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp;*.tif;*.tiff") },
            COMDLG_FILTERSPEC { pszName: w!("*.*"), pszSpec: w!("*.*") },
        ])
    }
}

/// Asks for a presentation file to open.
pub fn open_presentation() -> Option<PathBuf> {
    unsafe {
        open(&[
            COMDLG_FILTERSPEC { pszName: w!("Présentation Kubuno"), pszSpec: w!("*.kbsld;*.json") },
            COMDLG_FILTERSPEC { pszName: w!("*.*"), pszSpec: w!("*.*") },
        ])
    }
}

/// Asks where to save, proposing `name` (`.kbsld`).
pub fn save_presentation(name: &str) -> Option<PathBuf> {
    save_as(name, "Présentation Kubuno", "kbsld")
}

/// Asks where to save a file of type `ext` (`pdf`, `png`, `jpg`…), proposing `name`.
pub fn save_as(name: &str, label: &str, ext: &str) -> Option<PathBuf> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let (label, spec, ext) = (HSTRING::from(label), HSTRING::from(format!("*.{ext}")), HSTRING::from(ext));
        let filters = [COMDLG_FILTERSPEC { pszName: PCWSTR(label.as_ptr()), pszSpec: PCWSTR(spec.as_ptr()) }];
        let _ = dialog.SetFileTypes(&filters);
        let _ = dialog.SetDefaultExtension(&ext);
        let _ = dialog.SetFileName(&HSTRING::from(name));
        let base: &IFileDialog = &dialog;
        let opts = base.GetOptions().ok()?;
        let _ = base.SetOptions(opts | FOS_OVERWRITEPROMPT | FOS_FORCEFILESYSTEM);
        dialog.Show(Some(GetActiveWindow())).ok()?;
        result_path(dialog.GetResult().ok()?)
    }
}
