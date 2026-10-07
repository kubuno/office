//! The custom-drawn controls: the grid (`grid`), the sheet tabs (`sheet_tabs`), the status bar's zoom
//! (`status_zoom`), the chart preview, type picker and gallery (`chart_view`) and the shape gallery
//! (`shape_gallery`).

pub mod chart_view;
pub mod equation_view;
pub mod grid;
pub mod presence_avatars;
pub mod print_paint;
pub mod shape_gallery;
pub mod sheet_tabs;
pub mod status_zoom;

use kubuno_desktop::ui::Rect;

/// A popup's anchor: a small box around the pointer, in the owner window's client DIPs (the button whose
/// command opened the popup is under it).
pub fn anchor_at_pointer(owner: isize) -> Rect {
    let geometry = kubuno_desktop::controls::host::screen_geometry(owner);
    let mut p = windows::Win32::Foundation::POINT::default();
    // SAFETY: a plain query into a local.
    let ok = unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut p) }.is_ok();
    match (geometry, ok) {
        (Some(g), true) => {
            let (cx, cy) = (p.x as f32 / g.scale - g.client_origin.0, p.y as f32 / g.scale - g.client_origin.1);
            Rect::new(cx - 8.0, cy - 8.0, cx + 8.0, cy + 8.0)
        }
        _ => Rect::new(0.0, 0.0, 0.0, 0.0),
    }
}

/// Opens the chart gallery under the pointer; `on_pick` gets (type, subtype), or `("wizard", "")`.
pub fn open_chart_gallery(on_pick: impl Fn(String, String) + 'static) {
    use kubuno_desktop::popup::{Placement, Popup};
    use kubuno_desktop::prelude::*;
    let owner = kubuno_desktop::popup::current_window();
    let panel = Custom::<chart_view::ChartGalleryPanel>::new().dock(kubuno_desktop::DockStyle::Fill).name("gallery");
    let back = {
        let c = kubuno_desktop::Application::theme().card_background;
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02X}{:02X}{:02X}", byte(c.r), byte(c.g), byte(c.b))
    };
    let title = kubuno_desktop::resources::string("sheet_chart_gallery");
    let popup = Popup::new(&panel, chart_view::ChartGalleryPanel::WIDTH, chart_view::ChartGalleryPanel::height()).corner_radius(8.0).back_color(&back).title(&title).placement(Placement::BottomStart).gap(4.0);
    let form = popup.form().clone();
    {
        let (form, on_pick) = (form.clone(), std::rc::Rc::new(on_pick));
        panel.on::<chart_view::ChartPickEventArgs>("OnPicked").subscribe(move |_p, e| {
            form.close();
            on_pick(e.type_id.clone(), e.subtype.clone());
        });
    }
    popup.show(owner, anchor_at_pointer(owner));
}
pub mod swatch;
pub mod value_list;
