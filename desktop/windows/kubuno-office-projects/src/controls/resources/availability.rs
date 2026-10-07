//! « Disponibilité » (`ResourceAvailability.tsx`): the schedulable resources on the left (`w-64`), the chosen
//! one's time off on the right — the « Du / Au / Motif » form and the entries, newest read from the server.

use kubuno_desktop::ui::graphics::{Brush, Graphics, StringAlignment};
use kubuno_desktop::ui::Rect;
use kubuno_office_projects_core::date::{self, Date, DateFormat};
use kubuno_office_projects_core::resources as res;

use super::paint::{self, button, button_w, font, input, italic, text, text_w, InputText, Variant};
use super::{FieldKey, Hit, ResourcesView, Sub};
use crate::controls::model::{Palette, Shared};
use crate::Resources;

const LIST_W: f32 = 256.0;
/// `max-w-xl`.
const PANEL_MAX: f32 = 576.0;

impl ResourcesView {
    pub(super) fn paint_availability(&mut self, g: &Graphics<'_>, th: &Palette, model: &Shared, page: Rect) {
        let m = model.borrow();
        let Some(ed) = m.ed.as_ref() else { return };
        let d = ed.data();
        let lang = m.view.lang;
        let list = res::work_resources(d);
        let picked = self.picked_resource(d);
        let si = Sub::Availability.index();

        // ── The list ──
        let lr = Rect::new(page.left, page.top, page.left + LIST_W, page.bottom);
        paint::vline(g, th.border, lr.right, lr.top, lr.bottom);
        let f12 = font(12.0, false);
        if list.is_empty() {
            g.draw_string(Resources::res_avail_none(), &font(14.0, false), Brush::solid(th.text_tertiary), Rect::new(lr.left + 16.0, lr.top + 40.0, lr.right - 16.0, lr.top + 120.0), &paint::wrapped(StringAlignment::Center));
        }
        let mut y = lr.top;
        for &i in &list {
            let r = &d.resources[i];
            let h = if r.role.is_empty() { 44.0 } else { 51.0 };
            let row = Rect::new(lr.left, y, lr.right - 1.0, y + h);
            let on = picked.as_deref() == Some(r.id.as_str());
            if on {
                g.fill_rectangle(Brush::solid(th.primary.with_alpha(0.05)), row);
            } else if self.hover == Some(Hit::Pick(r.id.clone())) {
                g.fill_rectangle(Brush::solid(th.surface1), row);
            }
            paint::identity(g, th, r, row.left + 12.0, row.right - 8.0, y + h / 2.0, 28.0, 14.0, 12.0);
            paint::hline(g, th.row_line, row.left, row.right, row.bottom);
            self.hits.push((row, Hit::Pick(r.id.clone())));
            y += h;
        }

        // ── The panel ──
        let px = lr.right + 16.0;
        let right = (page.right - 16.0).min(px + PANEL_MAX);
        let mut y = page.top + 16.0 - self.scroll[si];
        let Some(id) = picked else {
            let mid = (lr.right + page.right) / 2.0;
            let label = Resources::res_avail_pick();
            let w = text_w(g, label, &font(14.0, false));
            g.draw_icon("UserRound", Rect::new(mid - w / 2.0 - 26.0, page.top, mid - w / 2.0 - 8.0, page.bottom), 18.0, th.text_tertiary.with_alpha(0.5));
            text(g, label, &font(14.0, false), th.text_tertiary, Rect::new(mid - w / 2.0, page.top, mid + w / 2.0 + 2.0, page.bottom), StringAlignment::Near);
            self.content_h = 0.0;
            return;
        };
        let Some(r) = d.resources.iter().find(|r| r.id == id) else { return };
        g.draw_icon("CalendarOff", Rect::new(px, y + 2.0, px + 16.0, y + 18.0), 16.0, th.text_secondary);
        text(g, &format!("{} — {}", Resources::res_avail_title(), r.name), &font(14.0, true), th.text_primary, Rect::new(px + 24.0, y, right, y + 20.0), StringAlignment::Near);
        y += 20.0 + 12.0;
        // The form (`flex flex-wrap items-end gap-2 p-3 rounded-xl border bg-surface-1`).
        let caret = self.caret_on();
        let inner_w = right - px - 24.0;
        let add = Resources::add();
        let bw = button_w(g, add, true, 36.0);
        let date_w = 150.0;
        let reason_room = inner_w - 2.0 * (date_w + 8.0);
        let wrap = reason_room - (bw + 8.0) < 140.0;
        let reason_w = if wrap { reason_room } else { reason_room - bw - 8.0 };
        let form_h = 12.0 + 20.0 + 36.0 + if wrap { 8.0 + 36.0 } else { 0.0 } + 12.0;
        let form = Rect::new(px, y, right, y + form_h);
        paint::card(g, form, th.surface1, th.border, 12.0);
        let fx = px + 12.0;
        let fy = y + 12.0;
        let fields = [(FieldKey::From, Resources::res_avail_from(), date_w, self.from.clone(), date_placeholder(lang)), (FieldKey::To, Resources::res_avail_to(), date_w, self.to.clone(), date_placeholder(lang)), (FieldKey::Reason, Resources::res_avail_reason(), reason_w, self.reason.clone(), Resources::res_avail_reason_ph())];
        let mut x = fx;
        for (key, label, w, value, ph) in fields {
            text(g, label, &f12, th.text_tertiary, Rect::new(x, fy, x + w, fy + 16.0), StringAlignment::Near);
            let ir = Rect::new(x, fy + 20.0, x + w, fy + 56.0);
            let date = key != FieldKey::Reason;
            match (&self.edit, date) {
                (Some(e), _) if e.key == key => input(g, th, ir, InputText::Editing(&e.le, caret), ph, None, 14.0, StringAlignment::Near, false),
                _ => {
                    // A typed date shows as the web's `jj/mm/aaaa` field does once filled.
                    let shown = if date { self.parse_date(&value).and_then(|s| Date::parse(&s)).map(|d| numeric(d, lang)).unwrap_or(value) } else { value };
                    input(g, th, ir, InputText::Value(&shown), ph, None, 14.0, StringAlignment::Near, false)
                }
            }
            if date {
                g.draw_icon("Calendar", Rect::new(ir.right - 30.0, ir.top, ir.right - 14.0, ir.bottom), 15.0, th.text_primary);
            }
            self.hits.push((ir, Hit::Field(key)));
            x += w + 8.0;
        }
        let ok = self.parse_date(&self.edit_or(&FieldKey::From, &self.from)).is_some() && self.parse_date(&self.edit_or(&FieldKey::To, &self.to)).is_some();
        let br = if wrap { Rect::new(fx, fy + 56.0 + 8.0, fx + bw, fy + 56.0 + 8.0 + 36.0) } else { Rect::new(x, fy + 20.0, x + bw, fy + 56.0) };
        button(g, th, br, add, Some("Plus"), Variant::Primary, ok, self.hover == Some(Hit::AddTimeOff));
        if ok {
            self.hits.push((br, Hit::AddTimeOff));
        }
        y = form.bottom + 16.0;
        // The entries.
        match ed.resources.time_off.get(&id) {
            Some(list) if !list.is_empty() => {
                for t in list {
                    let row = Rect::new(px, y, right, y + if t.reason.is_empty() { 42.0 } else { 56.0 });
                    paint::card(g, row, th.surface1, th.border, 8.0);
                    let cy = (row.top + row.bottom) / 2.0;
                    g.draw_icon("CalendarOff", Rect::new(row.left + 10.0, cy - 7.0, row.left + 24.0, cy + 7.0), 14.0, th.text_tertiary);
                    let when = format!("{} → {}", fmt_date(&t.from_date, lang), fmt_date(&t.to_date, lang));
                    let tx = row.left + 36.0;
                    if t.reason.is_empty() {
                        text(g, &when, &font(14.0, false), th.text_primary, Rect::new(tx, row.top, row.right - 40.0, row.bottom), StringAlignment::Near);
                    } else {
                        text(g, &when, &font(14.0, false), th.text_primary, Rect::new(tx, cy - 19.0, row.right - 40.0, cy + 1.0), StringAlignment::Near);
                        text(g, &t.reason, &f12, th.text_tertiary, Rect::new(tx, cy + 1.0, row.right - 40.0, cy + 18.0), StringAlignment::Near);
                    }
                    let del = Rect::new(row.right - 32.0, cy - 11.0, row.right - 10.0, cy + 11.0);
                    let hot = self.hover == Some(Hit::DeleteTimeOff(id.clone(), t.id.clone()));
                    g.draw_icon("Trash2", del, 14.0, if hot { th.danger } else { th.text_tertiary });
                    self.hits.push((del, Hit::DeleteTimeOff(id.clone(), t.id.clone())));
                    y = row.bottom + 8.0;
                }
            }
            _ => {
                text(g, Resources::res_avail_empty(), &italic(14.0), th.text_tertiary, Rect::new(px, y + 16.0, right, y + 40.0), StringAlignment::Center);
                y += 56.0;
            }
        }
        self.content_h = y + self.scroll[si] - page.top + 16.0;
    }

    /// The text of `key` as typed so far (the field being edited) or as kept.
    fn edit_or(&self, key: &FieldKey, kept: &str) -> String {
        match &self.edit {
            Some(e) if e.key == *key => e.le.text(),
            _ => kept.to_string(),
        }
    }
}

/// `formatDate(toDate(x), 'date')` — « 19 oct. 2026 ».
fn fmt_date(iso: &str, lang: date::Lang) -> String {
    Date::parse(iso).map(|d| date::format(d, DateFormat::Date, lang)).unwrap_or_else(|| iso.to_string())
}

/// The native date input's empty mask.
fn date_placeholder(lang: date::Lang) -> &'static str {
    match lang {
        date::Lang::Fr => "jj/mm/aaaa",
        date::Lang::En => "mm/dd/yyyy",
    }
}

/// A date as the native input shows it.
fn numeric(d: Date, lang: date::Lang) -> String {
    match lang {
        date::Lang::Fr => format!("{:02}/{:02}/{}", d.day, d.month, d.year),
        date::Lang::En => format!("{:02}/{:02}/{}", d.month, d.day, d.year),
    }
}
