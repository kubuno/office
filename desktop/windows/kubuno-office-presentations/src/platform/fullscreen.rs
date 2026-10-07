//! « Affichage › Plein écran » (the web's browser full screen, F11): the window maximized, and back to where it
//! was. Done on the window handle (the framework reads `WindowState` only when the window opens). Not the monitor's
//! full size yet: a window resized over the taskbar is not laid out again by the framework at desktop-v0.1.1-alpha.

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowPlacement, PostMessageW, SC_MAXIMIZE, SC_RESTORE, SW_SHOWMAXIMIZED, WINDOWPLACEMENT, WM_SYSCOMMAND};

/// Where the window was before full screen.
#[derive(Debug, Clone, Copy)]
pub struct Saved {
    placement: WINDOWPLACEMENT,
}

/// Puts window `hwnd` in full screen; what to restore it with.
pub fn enter(hwnd: isize) -> Option<Saved> {
    let h = HWND(hwnd as *mut _);
    unsafe {
        let mut placement = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        GetWindowPlacement(h, &mut placement).ok()?;
        // Posted like the caption button's: the framework lays the view out again on that path.
        PostMessageW(Some(h), WM_SYSCOMMAND, WPARAM(SC_MAXIMIZE as usize), LPARAM(0)).ok()?;
        Some(Saved { placement })
    }
}

/// Gives window `hwnd` back its place (a window maximized before stays maximized).
pub fn leave(hwnd: isize, saved: &Saved) {
    let h = HWND(hwnd as *mut _);
    unsafe {
        if saved.placement.showCmd == SW_SHOWMAXIMIZED.0 as u32 {
            return;
        }
        let _ = PostMessageW(Some(h), WM_SYSCOMMAND, WPARAM(SC_RESTORE as usize), LPARAM(0));
    }
}

/// The pointer in DIP of window `hwnd`'s client area: where a title band menu opens (its button is in the
/// framework's header cluster, out of reach of the view's names).
pub fn pointer_dip(hwnd: isize) -> Option<(f32, f32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let h = HWND(hwnd as *mut _);
    unsafe {
        let mut p = POINT::default();
        GetCursorPos(&mut p).ok()?;
        if !ScreenToClient(h, &mut p).as_bool() {
            return None;
        }
        let scale = (GetDpiForWindow(h).max(96) as f32) / 96.0;
        Some((p.x as f32 / scale, p.y as f32 / scale))
    }
}

/// The monitor of window `hwnd` in DIP of that window (left, top, width, height): where a window opened over the
/// whole screen goes, at its final size from the start (the slideshow).
pub fn monitor_dip(hwnd: isize) -> Option<(f32, f32, f32, f32)> {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    let h = HWND(hwnd as *mut _);
    unsafe {
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST), &mut info).as_bool() {
            return None;
        }
        let scale = (GetDpiForWindow(h).max(96) as f32) / 96.0;
        let r = info.rcMonitor;
        Some((r.left as f32 / scale, r.top as f32 / scale, (r.right - r.left) as f32 / scale, (r.bottom - r.top) as f32 / scale))
    }
}
