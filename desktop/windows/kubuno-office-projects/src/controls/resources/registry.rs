//! « Registre » (`ResourceRegistry.tsx`): the toolbar (« Ajouter », « Grouper par », the search, the count)
//! and the grid - type · name · role · skills · capacity · rate · tasks · utilisation · delete - one shared
//! column template (`GRID_COLS`), a sticky header, groups, fields edited in place.

use kubuno_desktop::ui::graphics::{Brush, Color, Graphics, Pen, StringAlignment};
use kubuno_desktop::ui::Rect;
use kubuno_office_projects_core::model::Resource;
use kubuno_office_projects_core::resources::{self as res, Behavior, Category, GroupMode};
use kubuno_office_projects_core::view;

use super::paint::{self, button, button_w, font, input, text, text_w, InputText, Variant};
use super::{FieldKey, Hit, ResourcesView};
use crate::controls::model::{Palette, Shared};
use crate::Resources;

/// `MIN_WIDTH`, the toolbar's and the rows' heights.
const MIN_WIDTH: f32 = 980.0;
const TOOLBAR_H: f32 = 52.0;
const HEAD_H: f32 = 36.0;
const ROW_H: f32 = 48.0;
const GROUP_H: f32 = 32.0;

/// `GRID_COLS`: fixed widths and `minmax(min, fr)` tracks, with `gap-x-2` and `px-3`.
fn columns(width: f32) -> [f32; 9] {
    let fixed = [40.0, 0.0, 0.0, 0.0, 96.0, 128.0, 72.0, 160.0, 44.0];
    let flex: [(usize, f32, f32); 3] = [(1, 180.0, 1.6), (2, 120.0, 1.0), (3, 170.0, 1.4)];
    let free = width - 24.0 - 8.0 * 8.0 - fixed.iter().sum::<f32>();
    let mut out = fixed;
    // The CSS grid's fr resolution: a track whose minimum exceeds its share keeps the minimum, the others
    // share what is left.
    let mut frozen = [false; 3];
    loop {
        let left = free - flex.iter().zip(frozen).filter(|(_, f)| *f).map(|((_, m, _), _)| m).sum::<f32>();
        let fr: f32 = flex.iter().zip(frozen).filter(|(_, f)| !*f).map(|((_, _, k), _)| k).sum();
        let mut changed = false;
        for (j, (i, min, k)) in flex.iter().enumerate() {
            if frozen[j] {
                out[*i] = *min;
                continue;
            }
            let share = if fr > 0.0 { left * k / fr } else { 0.0 };
            if share < *min {
                frozen[j] = true;
                changed = true;
            }
            out[*i] = share.max(*min);
        }
        if !changed {
            break;
        }
    }
    out
}

impl ResourcesView {
    pub(super) fn paint_registry(&mut self, g: &Graphics<'_>, th: &Palette, model: &Shared, page: Rect) {
        let m = model.borrow();
        let Some(ed) = m.ed.as_ref() else { return };
        let d = ed.data();
        let lang = m.view.lang;
        // The live search (the field being typed into) or the committed one.
        let query = match &self.edit {
            Some(e) if e.key == FieldKey::Search => e.le.text(),
            _ => self.search.clone(),
        };
        let groups = res::registry(d, &query, self.group, lang, Resources::res_no_role(), Resources::res_no_skill());
        let shown: usize = groups.iter().flat_map(|g| g.rows.iter()).collect::<std::collections::HashSet<_>>().len();
        let total = view::total_days(d, ed.project_start());
        let peaks = res::peak_load(d, total);
        let counts = res::task_counts(d);

        // ── Toolbar ──
        let tb = Rect::new(page.left, page.top, page.right, page.top + TOOLBAR_H);
        paint::hline(g, th.border, tb.left, tb.right, tb.bottom);
        let mut x = tb.left + 12.0;
        let cy = tb.top + TOOLBAR_H / 2.0;
        let add = Resources::res_add();
        let w = button_w(g, add, true, 36.0);
        let r = Rect::new(x, cy - 18.0, x + w, cy + 18.0);
        button(g, th, r, add, Some("Plus"), Variant::Primary, true, self.hover == Some(Hit::Add));
        self.hits.push((r, Hit::Add));
        x += w + 8.0;
        g.draw_icon("Layers", Rect::new(x, cy - 7.0, x + 14.0, cy + 7.0), 14.0, th.text_tertiary);
        x += 14.0 + 6.0;
        let gl = Resources::res_group_by();
        let f12 = font(12.0, false);
        let gw = text_w(g, gl, &f12);
        text(g, gl, &f12, th.text_secondary, Rect::new(x, cy - 10.0, x + gw + 2.0, cy + 10.0), StringAlignment::Near);
        x += gw + 6.0;
        let current = group_label(self.group);
        let f13 = font(13.0, false);
        let dw = 12.0 + text_w(g, current, &f13) + 6.0 + 14.0 + 10.0;
        let dr = Rect::new(x, cy - 16.0, x + dw, cy + 16.0);
        paint::card(g, dr, if self.hover == Some(Hit::Group) { th.surface1 } else { th.surface0 }, th.border, 6.0);
        text(g, current, &f13, th.text_primary, Rect::new(dr.left + 12.0, dr.top, dr.right - 24.0, dr.bottom), StringAlignment::Near);
        g.draw_icon("ChevronDown", Rect::new(dr.right - 24.0, dr.top, dr.right - 10.0, dr.bottom), 14.0, th.text_secondary);
        self.hits.push((dr, Hit::Group));
        x += dw + 8.0;
        // The count on the right; the search takes what is left (`flex-1 min-w-[160px] max-w-xs`).
        let count = Resources::res_count().replace("{n}", &shown.to_string());
        let cw = text_w(g, &count, &f12);
        text(g, &count, &f12, th.text_tertiary, Rect::new(tb.right - 12.0 - cw - 2.0, cy - 10.0, tb.right - 12.0, cy + 10.0), StringAlignment::Far);
        let sw = (tb.right - 12.0 - cw - 8.0 - x).clamp(160.0, 320.0);
        let sr = Rect::new(x, cy - 18.0, x + sw, cy + 18.0);
        let caret = self.caret_on();
        let value = match &self.edit {
            Some(e) if e.key == FieldKey::Search => InputText::Editing(&e.le, caret),
            _ => InputText::Value(&self.search),
        };
        input(g, th, sr, value, Resources::res_search_ph(), Some("Search"), 14.0, StringAlignment::Near, false);
        self.hits.push((sr, Hit::Field(FieldKey::Search)));

        // ── The grid ──
        let area = Rect::new(page.left, tb.bottom, page.right, page.bottom);
        let width = (area.right - area.left).max(MIN_WIDTH);
        self.reg_w = width;
        self.reg_x = self.reg_x.clamp(0.0, (width - (area.right - area.left)).max(0.0));
        let cols = columns(width);
        let left = area.left - self.reg_x;
        let mut xs = [0.0f32; 9];
        let mut cx = left + 12.0;
        for (i, w) in cols.iter().enumerate() {
            xs[i] = cx;
            cx += w + 8.0;
        }
        let cell = |i: usize, top: f32, bottom: f32| Rect::new(xs[i], top, xs[i] + cols[i], bottom);
        let rows_h: f32 = if d.resources.is_empty() || groups.iter().all(|g| g.rows.is_empty()) {
            160.0
        } else {
            groups.iter().map(|gr| gr.rows.len() as f32 * ROW_H + if self.group == GroupMode::None { 0.0 } else { GROUP_H }).sum()
        };
        let view_h = area.bottom - area.top - HEAD_H;
        let si = 0;
        self.scroll[si] = self.scroll[si].clamp(0.0, (rows_h - view_h).max(0.0));
        self.content_h = rows_h + (page.bottom - page.top) - view_h;
        let body = Rect::new(area.left, area.top + HEAD_H, area.right, area.bottom);
        let saved = g.save();
        g.intersect_clip(body);
        let mut y = body.top - self.scroll[si];
        let edit_key = self.edit.as_ref().map(|e| e.key.clone());
        if d.resources.is_empty() || groups.iter().all(|g| g.rows.is_empty()) {
            let mid = (left + left + width) / 2.0;
            g.draw_icon("UsersRound", Rect::new(mid - 15.0, y + 64.0, mid + 15.0, y + 94.0), 30.0, th.text_tertiary.with_alpha(0.4));
            let msg = if d.resources.is_empty() { Resources::res_empty() } else { Resources::res_no_match() };
            text(g, msg, &font(14.0, false), th.text_secondary, Rect::new(left, y + 100.0, left + width, y + 120.0), StringAlignment::Center);
            if d.resources.is_empty() {
                text(g, Resources::res_empty_hint(), &f12, th.text_tertiary, Rect::new(left, y + 122.0, left + width, y + 140.0), StringAlignment::Center);
            }
        }
        for gr in &groups {
            if gr.rows.is_empty() {
                continue;
            }
            if self.group != GroupMode::None {
                let gh = Rect::new(left, y, left + width, y + GROUP_H);
                g.fill_rectangle(Brush::solid(th.surface1.with_alpha(0.7)), gh);
                paint::hline(g, th.border, gh.left, gh.right, gh.bottom);
                let fb = font(12.0, true);
                let lw = text_w(g, &gr.label, &fb);
                text(g, &gr.label, &fb, th.text_primary, Rect::new(gh.left + 12.0, gh.top, gh.left + 12.0 + lw + 2.0, gh.bottom), StringAlignment::Near);
                text(g, &gr.rows.len().to_string(), &font(11.0, false), th.text_tertiary, Rect::new(gh.left + 20.0 + lw, gh.top, gh.right, gh.bottom), StringAlignment::Near);
                y += GROUP_H;
            }
            for &i in &gr.rows {
                let r = &d.resources[i];
                let row = Rect::new(left, y, left + width, y + ROW_H);
                if y + ROW_H >= body.top && y <= body.bottom {
                    let hot = self.hover_row.as_deref() == Some(r.id.as_str());
                    if hot {
                        g.fill_rectangle(Brush::solid(th.surface1), row);
                    }
                    self.hits.push((Rect::new(row.left, row.top.max(body.top), row.right, row.bottom.min(body.bottom)), Hit::Row(r.id.clone())));
                    paint::hline(g, th.border, row.left, row.right, row.bottom);
                    self.paint_row(g, th, r, &cell, row, counts.get(&r.id).copied().unwrap_or(0), res::utilisation(r, &peaks), hot, edit_key.as_ref(), lang);
                }
                y += ROW_H;
            }
        }
        g.restore(saved);

        // The sticky header.
        let head = Rect::new(left, area.top, left + width, area.top + HEAD_H);
        g.fill_rectangle(Brush::solid(th.surface1), Rect::new(area.left, head.top, area.right.max(head.right), head.bottom));
        paint::hline(g, th.border, area.left, area.right, head.bottom);
        let fh = font(11.0, true);
        let labels = [Resources::res_col_type(), Resources::res_col_name(), Resources::res_col_role(), Resources::res_col_skills(), Resources::res_col_capacity(), Resources::res_col_rate(), Resources::res_col_tasks(), Resources::res_col_load(), ""];
        let aligns = [StringAlignment::Center, StringAlignment::Near, StringAlignment::Near, StringAlignment::Near, StringAlignment::Far, StringAlignment::Far, StringAlignment::Center, StringAlignment::Near, StringAlignment::Near];
        for (i, l) in labels.iter().enumerate() {
            text(g, &l.to_uppercase(), &fh, th.text_tertiary, cell(i, head.top, head.bottom), aligns[i]);
        }

        // Scroll bars.
        let vrail = Rect::new(area.right - 10.0, body.top, area.right, body.bottom);
        if let Some(t) = paint::scroll_thumb(g, th, vrail, true, self.scroll[si], rows_h, body.bottom - body.top, self.hover == Some(Hit::VBar)) {
            self.hits.push((t, Hit::VBar));
        }
        let hrail = Rect::new(area.left, area.bottom - 10.0, area.right - 10.0, area.bottom);
        if let Some(t) = paint::scroll_thumb(g, th, hrail, false, self.reg_x, width, area.right - area.left, self.hover == Some(Hit::HBar)) {
            self.hits.push((t, Hit::HBar));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_row(&mut self, g: &Graphics<'_>, th: &Palette, r: &Resource, cell: &dyn Fn(usize, f32, f32) -> Rect, row: Rect, tasks: usize, load: Option<f64>, hot: bool, edit: Option<&FieldKey>, lang: kubuno_office_projects_core::date::Lang) {
        let (top, bottom) = (row.top, row.bottom);
        let cy = (top + bottom) / 2.0;
        let caret = self.caret_on();
        let meta = res::meta(&r.kind);
        // 1 · Type.
        let c = cell(0, top, bottom);
        let mid = (c.left + c.right) / 2.0;
        let chip = Rect::new(mid - 12.0, cy - 12.0, mid + 12.0, cy + 12.0);
        let tint = meta.map(|m| paint::tint_of(m, th)).unwrap_or(th.text_secondary);
        g.fill_rounded_rectangle(Brush::solid(paint::wash(tint, 0.14)), chip, 6.0);
        g.draw_icon(meta.map(|m| m.icon).unwrap_or("UserRound"), chip, 14.0, tint);
        // 2 · Name, avatar, member / external badge.
        let c = cell(1, top, bottom);
        paint::avatar(g, th, r, c.left + 14.0, cy, 28.0);
        let nx = c.left + 28.0 + 8.0;
        let fname = font(12.0, false);
        let human = meta.is_some_and(|m| m.category == Category::Human);
        let badge = if human { Some(if r.user_id.is_some() { Resources::res_member() } else { Resources::res_external() }) } else { None };
        let fbadge = font(10.0, true);
        let bw = badge.map(|b| text_w(g, b, &fbadge) + 12.0).unwrap_or(0.0);
        let name_max = (c.right - nx - if bw > 0.0 { bw + 6.0 } else { 0.0 }).max(20.0);
        let key = FieldKey::Name(r.id.clone());
        let name_w = if edit == Some(&key) {
            let fr = Rect::new(nx, cy - 11.0, nx + name_max, cy + 11.0);
            if let Some(e) = self.edit.as_ref() {
                input(g, th, fr, InputText::Editing(&e.le, caret), "", None, 12.0, StringAlignment::Near, true);
            }
            name_max
        } else {
            let w = text_w(g, &r.name, &fname).min(name_max);
            text(g, &r.name, &fname, th.text_primary, Rect::new(nx, cy - 10.0, nx + w + 1.0, cy + 10.0), StringAlignment::Near);
            w
        };
        self.hits.push((Rect::new(nx, cy - 12.0, nx + name_w, cy + 12.0), Hit::Field(key)));
        if let Some(b) = badge {
            let bx = nx + name_w + 6.0;
            let br = Rect::new(bx, cy - 8.0, bx + bw, cy + 8.0);
            let (fill, ink) = if r.user_id.is_some() { (th.primary.with_alpha(0.1), th.primary) } else { (th.surface2, th.text_secondary) };
            g.fill_rounded_rectangle(Brush::solid(fill), br, 4.0);
            text(g, b, &fbadge, ink, br, StringAlignment::Center);
        }
        // 3 · Role.
        let c = cell(2, top, bottom);
        let key = FieldKey::Role(r.id.clone());
        let fr = Rect::new(c.left - 4.0, cy - 12.0, c.right, cy + 12.0);
        match (edit == Some(&key), self.edit.as_ref()) {
            (true, Some(e)) => input(g, th, fr, InputText::Editing(&e.le, caret), Resources::res_role_ph(), None, 12.0, StringAlignment::Near, true),
            _ => input(g, th, fr, InputText::Value(&r.role), Resources::res_role_ph(), None, 12.0, StringAlignment::Near, true),
        }
        self.hits.push((fr, Hit::Field(key)));
        // 4 · Skills: chips, then the dashed « + compétence » field; wrapped, centred in the row.
        let c = cell(3, top, bottom);
        let f11 = font(11.0, false);
        let skill_key = FieldKey::Skill(r.id.clone());
        let mut items: Vec<(f32, Option<&String>)> = r.skills.iter().map(|s| (8.0 + text_w(g, s, &f11).min(c.right - c.left - 30.0) + 4.0 + 11.0 + 4.0, Some(s))).collect();
        items.push((6.0 + 11.0 + 2.0 + 64.0 + 6.0, None));
        let mut lines: Vec<Vec<(f32, Option<&String>)>> = vec![Vec::new()];
        let mut lw = 0.0;
        for it in items {
            if lw > 0.0 && lw + it.0 > c.right - c.left {
                lines.push(Vec::new());
                lw = 0.0;
            }
            lw += it.0 + 4.0;
            if let Some(l) = lines.last_mut() {
                l.push(it);
            }
        }
        let block = lines.len() as f32 * 20.0 + (lines.len() as f32 - 1.0) * 4.0;
        let mut ly = cy - block / 2.0;
        for l in lines {
            let mut lx = c.left;
            for (w, s) in l {
                let chip = Rect::new(lx, ly, lx + w, ly + 20.0);
                match s {
                    Some(s) => {
                        g.fill_rounded_rectangle(Brush::solid(th.surface2), chip, 10.0);
                        text(g, s, &f11, th.text_secondary, Rect::new(chip.left + 8.0, chip.top, chip.right - 19.0, chip.bottom), StringAlignment::Near);
                        let xr = Rect::new(chip.right - 16.0, chip.top + 2.0, chip.right - 2.0, chip.bottom - 2.0);
                        let hot_x = self.hover == Some(Hit::SkillRemove(r.id.clone(), s.clone()));
                        g.draw_icon("X", xr, 11.0, if hot_x { th.danger } else { th.text_tertiary });
                        self.hits.push((xr, Hit::SkillRemove(r.id.clone(), s.clone())));
                    }
                    None => {
                        g.draw_rounded_rectangle(&Pen::new(th.border, 1.0).with_dash_pattern(&[3.0, 2.0]), Rect::new(chip.left + 0.5, chip.top + 0.5, chip.right - 0.5, chip.bottom - 0.5), 10.0);
                        g.draw_icon("Plus", Rect::new(chip.left + 6.0, chip.top, chip.left + 17.0, chip.bottom), 11.0, th.text_tertiary);
                        let fr = Rect::new(chip.left + 19.0, chip.top + 1.0, chip.right - 6.0, chip.bottom - 1.0);
                        let saved = g.save();
                        g.intersect_clip(fr);
                        match (edit == Some(&skill_key), self.edit.as_ref()) {
                            (true, Some(e)) => input(g, th, Rect::new(fr.left - 4.0, fr.top, fr.right + 4.0, fr.bottom), InputText::Editing(&e.le, caret), Resources::res_skill_add(), None, 11.0, StringAlignment::Near, true),
                            _ => text(g, Resources::res_skill_add(), &f11, th.text_tertiary, fr, StringAlignment::Near),
                        }
                        g.restore(saved);
                        self.hits.push((chip, Hit::Field(skill_key.clone())));
                    }
                }
                lx += w + 4.0;
            }
            ly += 24.0;
        }
        // 5 · Capacity (work only) and 6 · rate, right-aligned with their suffix.
        let behavior = res::behavior_of(&r.kind);
        let c = cell(4, top, bottom);
        if behavior == Behavior::Work {
            let v = ((r.capacity * 100.0).round() as i64).to_string();
            self.number_field(g, th, c, cy, FieldKey::Capacity(r.id.clone()), &v, "%", edit);
        } else {
            text(g, "-", &font(12.0, false), th.text_tertiary, Rect::new(c.left, cy - 10.0, c.right, cy + 10.0), StringAlignment::Far);
        }
        let c = cell(5, top, bottom);
        let suffix = if behavior == Behavior::Material { format!("/{}", r.unit_label.clone().filter(|u| !u.is_empty()).unwrap_or_else(|| Resources::res_unit().to_string())) } else { "/h".into() };
        let v = r.hourly_rate.map(|v| res::number(v, kubuno_office_projects_core::date::Lang::En, 2).replace(',', "")).unwrap_or_default();
        self.number_field(g, th, c, cy, FieldKey::Rate(r.id.clone()), &v, &suffix, edit);
        // 7 · Tasks.
        let c = cell(6, top, bottom);
        text(g, &tasks.to_string(), &font(12.0, false), th.text_secondary, Rect::new(c.left, cy - 10.0, c.right, cy + 10.0), StringAlignment::Center);
        // 8 · Utilisation.
        let c = cell(7, top, bottom);
        match load {
            None => text(g, "-", &font(12.0, false), th.text_tertiary, Rect::new(c.left, cy - 10.0, c.right, cy + 10.0), StringAlignment::Near),
            Some(pct) => {
                let col = Color::from_hex(res::load_color(pct)).unwrap_or(th.success);
                let label = format!("{}%", kubuno_office_projects_core::rollup::js_round(pct) as i64);
                let fl = font(11.0, true);
                let tw = text_w(g, &label, &fl);
                let bar = Rect::new(c.left, cy - 4.0, c.right - tw - 8.0, cy + 4.0);
                g.fill_rounded_rectangle(Brush::solid(th.surface2), bar, 4.0);
                let fw = (bar.right - bar.left) * (pct.min(100.0) / 100.0) as f32;
                if fw > 0.0 {
                    g.fill_rounded_rectangle(Brush::solid(col), Rect::new(bar.left, bar.top, bar.left + fw, bar.bottom), 4.0);
                }
                text(g, &label, &fl, col, Rect::new(c.right - tw - 2.0, cy - 10.0, c.right, cy + 10.0), StringAlignment::Far);
            }
        }
        // 9 · Delete (shown on hover).
        let c = cell(8, top, bottom);
        let mid = (c.left + c.right) / 2.0;
        let dr = Rect::new(mid - 11.0, cy - 11.0, mid + 11.0, cy + 11.0);
        if hot {
            let on = self.hover == Some(Hit::Delete(r.id.clone()));
            g.draw_icon("Trash2", dr, 14.0, if on { th.danger } else { th.text_tertiary });
        }
        self.hits.push((dr, Hit::Delete(r.id.clone())));
        let _ = lang;
    }

    /// A `GhostNumber`: the value right-aligned, its suffix after it, edited in place.
    #[allow(clippy::too_many_arguments)]
    fn number_field(&mut self, g: &Graphics<'_>, th: &Palette, c: Rect, cy: f32, key: FieldKey, value: &str, suffix: &str, edit: Option<&FieldKey>) {
        let f11 = font(11.0, false);
        let sw = text_w(g, suffix, &f11);
        text(g, suffix, &f11, th.text_tertiary, Rect::new(c.right - sw - 1.0, cy - 10.0, c.right, cy + 10.0), StringAlignment::Far);
        let fr = Rect::new(c.left, cy - 12.0, c.right - sw - 2.0, cy + 12.0);
        match (edit == Some(&key), self.edit.as_ref()) {
            (true, Some(e)) => input(g, th, fr, InputText::Editing(&e.le, self.caret_on()), "", None, 12.0, StringAlignment::Far, true),
            _ => input(g, th, fr, InputText::Value(value), "", None, 12.0, StringAlignment::Far, true),
        }
        self.hits.push((fr, Hit::Field(key)));
    }

    /// The type chip's tooltip (`Tooltip label={kind label}`).
    pub(super) fn kind_tool_tip(&self, ax: f32, ay: f32) -> Option<String> {
        if self.sub != super::Sub::Registry {
            return None;
        }
        let Some(Hit::Row(id)) = self.hits.iter().rev().find(|(r, h)| matches!(h, Hit::Row(_)) && r.contains(ax, ay)).map(|(_, h)| h.clone()) else { return None };
        let model = self.model.as_ref()?.borrow();
        let r = model.data()?.resources.iter().find(|r| r.id == id)?;
        // Over the type column only (the row's first 12 + 40 px).
        let left = self.bounds.left - self.reg_x;
        (ax - left < 52.0).then(|| res::kind_label(&r.kind, model.view.lang))
    }
}

/// The « Grouper par » choices (`groupOptions`).
pub fn group_label(mode: GroupMode) -> &'static str {
    match mode {
        GroupMode::None => Resources::res_group_none(),
        GroupMode::Type => Resources::res_group_type(),
        GroupMode::Role => Resources::res_group_role(),
        GroupMode::Skill => Resources::res_group_skill(),
    }
}

#[cfg(test)]
mod tests {
    use super::columns;

    #[test]
    fn grid_tracks_follow_css() {
        // At the minimum width every flexible track sits at its minimum.
        let c = columns(980.0);
        assert!((c[1] - 180.0).abs() < 0.5 && (c[2] - 120.0).abs() < 0.5 && (c[3] - 170.0).abs() < 0.5);
        // Wider, the free space splits 1.6 : 1 : 1.4.
        let c = columns(1600.0);
        let free = 1600.0 - 24.0 - 64.0 - 540.0;
        assert!((c[1] - free * 0.4).abs() < 0.5);
        assert!((c[3] - free * 0.35).abs() < 0.5);
    }
}
