//! The system's file dialog (`IFileOpenDialog`): Insertion › Images (the web's image picker, local file).

use std::path::PathBuf;

use windows::core::{w, PCWSTR};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, SIGDN_FILESYSPATH};

/// Asks for one image file; `None` when cancelled.
pub fn open_image(title: &str) -> Option<PathBuf> {
    // SAFETY: COM calls on the UI thread; every interface is released by its wrapper, the display name is
    // freed with CoTaskMemFree as the API requires.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let filters = [
            COMDLG_FILTERSPEC { pszName: w!("Images"), pszSpec: w!("*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp;*.svg;*.tif;*.tiff;*.ico") },
            COMDLG_FILTERSPEC { pszName: w!("*.*"), pszSpec: w!("*.*") },
        ];
        let _ = dialog.SetFileTypes(&filters);
        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = dialog.SetTitle(PCWSTR(wide.as_ptr()));
        let opts = dialog.GetOptions().ok()?;
        let _ = dialog.SetOptions(opts | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM);
        let owner = GetActiveWindow();
        dialog.Show(Some(owner)).ok()?;
        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = PCWSTR(name.0).to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(name.0 as *const _));
        path.map(PathBuf::from)
    }
}
