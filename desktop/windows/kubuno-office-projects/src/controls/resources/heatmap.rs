//! « Plan de charge » (`ResourceHeatmap.tsx`): the title with the scale switch and the « Aujourd'hui »
//! navigation, then a card — the resources on the left (sticky), one column per day / week / month on the
//! right, each cell the mean utilisation on the semantic gradient (overload flagged), a row unfolding into the
//! resource's tasks — and the legend.

use kubuno_desktop::ui::graphics::{Brush, Color, Graphics, StringAlignment};
use kubuno_desktop::ui::Rect;
use kubuno_office_projects_core::resources::{self as res, Scale, HEAT_LEFT_WIDTH};
use kubuno_office_projects_core::rollup::js_round;
use kubuno_office_projects_core::view;

use super::paint::{self, button, font, text, text_w, Variant};
use super::{Hit, ResourcesView, Sub};
use crate::controls::model::{hex, Palette, Shared};
use crate::Resources;

const PAD: f32 = 24.0;
const ROW: f32 = 52.0;
const BAND: f32 = 24.0;
const SUBH: f32 = 28.0;

pub fn scale_label(s: Scale) -> &'static str {
    match s {
        Scale::Day => Resources::res_heat_day(),
        Scale::Week => Resources::res_heat_week(),
        Scale::Month => Resources::res_heat_month(),
    }
}

impl ResourcesView {
    pub(super) fn paint_heatmap(&mut self, g: &Graphics<'_>, th: &Palette, model: &Shared, page: Rect) {
        let m = model.borrow();
        let Some(ed) = m.ed.as_ref() else { return };
        let d = ed.data();
        let lang = m.view.lang;
        g.fill_rectangle(Brush::solid(th.surface1), page);
        let si = Sub::Load.index();
        let left = page.left + PAD;
        let right = page.right - PAD;
        let mut y = page.top + PAD - self.scroll[si];

        // ── Header ──
        g.draw_icon("Users", Rect::new(left, y + 6.0, left + 20.0, y + 26.0), 20.0, th.text_secondary);
        text(g, Resources::res_heat_title(), &font(20.0, true), th.text_primary, Rect::new(left + 32.0, y, right - 360.0, y + 32.0), StringAlignment::Near);
        let f14 = font(14.0, false);
        let mut x = right - 32.0;
        let next = Rect::new(x, y, x + 32.0, y + 32.0);
        button(g, th, next, "", Some("ChevronRight"), Variant::Ghost, true, self.hover == Some(Hit::Next));
        self.hits.push((next, Hit::Next));
        let tw = paint::button_w(g, Resources::res_heat_today(), false, 32.0) - 8.0;
        x -= 4.0 + tw;
        let today = Rect::new(x, y, x + tw, y + 32.0);
        button(g, th, today, Resources::res_heat_today(), None, Variant::Secondary, true, self.hover == Some(Hit::Today));
        self.hits.push((today, Hit::Today));
        x -= 4.0 + 32.0;
        let prev = Rect::new(x, y, x + 32.0, y + 32.0);
        button(g, th, prev, "", Some("ChevronLeft"), Variant::Ghost, true, self.hover == Some(Hit::Prev));
        self.hits.push((prev, Hit::Prev));
        // The scale switch (a segmented control).
        let scales = [Scale::Day, Scale::Week, Scale::Month];
        let widths: Vec<f32> = scales.iter().map(|s| 24.0 + text_w(g, scale_label(*s), &f14)).collect();
        let sw: f32 = widths.iter().sum::<f32>() + 4.0;
        x -= 12.0 + sw;
        let seg = Rect::new(x, y, x + sw, y + 32.0);
        paint::card(g, seg, th.surface0, th.border, 8.0);
        let mut sx = seg.left + 2.0;
        for (s, w) in scales.iter().zip(&widths) {
            let r = Rect::new(sx, seg.top + 2.0, sx + w, seg.bottom - 2.0);
            let on = *s == self.scale;
            if on {
                g.fill_rounded_rectangle(Brush::solid(th.primary), r, 6.0);
            }
            let ink = if on { if th.dark { hex(0x202124) } else { Color::WHITE } } else if self.hover == Some(Hit::Scale(*s)) { th.text_primary } else { th.text_secondary };
            text(g, scale_label(*s), &f14, ink, r, StringAlignment::Center);
            self.hits.push((r, Hit::Scale(*s)));
            sx += w;
        }
        y += 32.0 + 16.0;

        let shown = res::work_resources(d);
        let start = ed.project_start();
        let total = view::total_days(d, start);
        if shown.is_empty() {
            // `EmptyState variant="first-use"`.
            let card = Rect::new(left, y, right, y + 200.0);
            paint::card(g, card, th.surface0, th.border, 12.0);
            let mid = (left + right) / 2.0;
            g.fill_ellipse(Brush::solid(th.surface2), Rect::new(mid - 28.0, y + 32.0, mid + 28.0, y + 88.0));
            g.draw_icon("Users", Rect::new(mid - 13.0, y + 47.0, mid + 13.0, y + 73.0), 26.0, th.text_secondary);
            text(g, Resources::res_heat_empty_title(), &font(16.0, true), th.text_primary, Rect::new(left, y + 100.0, right, y + 124.0), StringAlignment::Center);
            g.draw_string(Resources::res_heat_empty(), &font(14.0, false), Brush::solid(th.text_secondary), Rect::new(mid - 220.0, y + 130.0, mid + 220.0, y + 190.0), &paint::wrapped(StringAlignment::Center));
            y += 200.0;
        } else {
            let today_off = m.view.today.days_since(start) as i32;
            let cols = res::heat_columns(self.scale, start, total, today_off, lang);
            let spans = res::group_spans(&cols);
            let ratios = res::daily_ratios(d, &shown, total);
            let col_w = self.scale.col_width() as f32;
            let time_w = cols.len() as f32 * col_w;
            let lw = HEAT_LEFT_WIDTH as f32;
            let time_view = (right - left - 2.0 - lw).max(0.0);
            self.heat_view = (time_w, time_view);
            self.heat_x = self.heat_x.clamp(0.0, (time_w - time_view).max(0.0));
            let rows_h: f32 = shown
                .iter()
                .map(|&i| {
                    let id = &d.resources[i].id;
                    ROW + if self.expanded.contains(id) { expanded_h(res::tasks_for_resource(d, id).len()) } else { 0.0 }
                })
                .sum();
            let has_bar = time_w > time_view;
            let card_h = BAND + SUBH + rows_h + 1.0 + if has_bar { 12.0 } else { 0.0 };
            let card = Rect::new(left, y, right, y + card_h + 1.0);
            paint::card(g, card, th.surface0, th.border, 12.0);
            let inner = Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0);
            let tl = inner.left + lw;
            let tx = |c: usize| tl + c as f32 * col_w - self.heat_x;
            let saved = g.save();
            g.intersect_clip(Rect::new(tl, inner.top, inner.right, inner.bottom));
            // The month / year band and the sub-columns.
            let f11 = font(11.0, false);
            let f11b = font(11.0, true);
            let mut c0 = 0;
            for (label, n) in &spans {
                let r = Rect::new(tx(c0), inner.top, tx(c0 + n), inner.top + BAND);
                text(g, label, &f11b, th.text_secondary, Rect::new(r.left + 4.0, r.top, r.right - 4.0, r.bottom), StringAlignment::Center);
                paint::vline(g, th.border, r.right, r.top, r.bottom);
                c0 += n;
            }
            paint::hline(g, th.border, tl, tl + time_w - self.heat_x, inner.top + BAND);
            let sub_top = inner.top + BAND;
            for (i, c) in cols.iter().enumerate() {
                let r = Rect::new(tx(i), sub_top, tx(i + 1), sub_top + SUBH);
                if c.is_today {
                    g.fill_rectangle(Brush::solid(th.primary.with_alpha(0.15)), r);
                }
                text(g, &c.sub_label, if c.is_today { &f11b } else { &f11 }, if c.is_today { th.primary } else { th.text_tertiary }, r, StringAlignment::Center);
                paint::vline(g, th.border, r.right, r.top, r.bottom);
            }
            g.restore(saved);
            let head_bottom = sub_top + SUBH;
            paint::hline(g, th.border, inner.left, inner.right, head_bottom + 1.0);
            // The corner.
            text(g, Resources::res_heat_resource(), &font(12.0, true), th.text_secondary, Rect::new(inner.left + 12.0, head_bottom - 22.0, tl - 4.0, head_bottom - 4.0), StringAlignment::Near);
            paint::vline(g, th.border, tl, inner.top, inner.top + card_h);
            // The rows.
            let mut ry = head_bottom + 1.0;
            let f12 = font(12.0, false);
            for &i in &shown {
                let r = &d.resources[i];
                let cap = if r.capacity > 0.0 { r.capacity } else { 1.0 };
                let open = self.expanded.contains(&r.id);
                let label = Rect::new(inner.left, ry, tl, ry + ROW);
                if self.hover == Some(Hit::HeatRow(r.id.clone())) {
                    g.fill_rectangle(Brush::solid(th.surface1), label);
                }
                paint::identity(g, th, r, label.left + 12.0, label.right - 8.0, ry + ROW / 2.0, 32.0, 14.0, 11.0);
                self.hits.push((label, Hit::HeatRow(r.id.clone())));
                let saved = g.save();
                g.intersect_clip(Rect::new(tl, ry, inner.right, ry + ROW));
                for (ci, c) in cols.iter().enumerate() {
                    let (ratio, _) = res::cell_value(ratios.get(&r.id), c, cap);
                    let p = res::paint_for(ratio);
                    let cell = Rect::new(tx(ci), ry, tx(ci + 1), ry + ROW);
                    if cell.right < tl || cell.left > inner.right {
                        continue;
                    }
                    let (fill, ink) = if p.zero { (th.surface1, th.text_tertiary) } else { (Color::rgb(p.rgb.0, p.rgb.1, p.rgb.2), if p.white_ink { Color::WHITE } else { hex(0x202124) }) };
                    g.fill_rectangle(Brush::solid(fill), cell);
                    if c.is_today {
                        g.draw_rectangle(&kubuno_desktop::ui::graphics::Pen::new(th.primary.with_alpha(0.4), 1.0), Rect::new(cell.left + 0.5, cell.top + 0.5, cell.right - 1.5, cell.bottom - 1.5));
                    }
                    paint::vline(g, th.border.with_alpha(0.6), cell.right, cell.top, cell.bottom);
                    paint::hline(g, th.border.with_alpha(0.6), cell.left, cell.right, cell.bottom);
                    let pct = js_round(ratio * 100.0) as i64;
                    if pct > 0 {
                        text(g, &format!("{pct}%"), &f12, ink, cell, StringAlignment::Center);
                    }
                    if p.overload {
                        g.draw_icon("AlertTriangle", Rect::new(cell.right - 14.0, cell.top + 2.0, cell.right - 3.0, cell.top + 13.0), 11.0, ink);
                    }
                }
                g.restore(saved);
                ry += ROW;
                if open {
                    let bars = res::tasks_for_resource(d, &r.id);
                    let h = expanded_h(bars.len());
                    g.fill_rectangle(Brush::solid(th.surface1.with_alpha(0.6)), Rect::new(inner.left, ry, inner.right, ry + h));
                    g.fill_rectangle(Brush::solid(th.surface1), Rect::new(inner.left, ry, tl, ry + h));
                    text(g, &Resources::res_heat_tasks().replace("{n}", &bars.len().to_string()), &f11, th.text_tertiary, Rect::new(inner.left + 12.0, ry + 8.0, tl - 4.0, ry + 24.0), StringAlignment::Near);
                    let saved = g.save();
                    g.intersect_clip(Rect::new(tl, ry, inner.right, ry + h));
                    if bars.is_empty() {
                        text(g, Resources::res_heat_no_tasks(), &f11, th.text_tertiary, Rect::new(tl + 12.0, ry + 8.0, inner.right, ry + 24.0), StringAlignment::Near);
                    }
                    let span = total.max(1) as f32;
                    let rc = Color::from_hex(&r.color).unwrap_or(th.primary);
                    for (k, (ti, _units)) in bars.iter().enumerate() {
                        let t = &d.tasks[*ti];
                        let es = t.early_start.unwrap_or(0).max(0) as f32;
                        let dur = t.duration_days.max(1) as f32;
                        let bl = tl - self.heat_x + es / span * time_w;
                        let bw = ((dur.min(span - es)) / span * time_w).max(6.0);
                        let by = ry + 8.0 + k as f32 * 24.0;
                        let br = Rect::new(bl, by, bl + bw, by + 20.0);
                        g.fill_rounded_rectangle(Brush::solid(rc), br, 4.0);
                        text(g, &t.name, &font(10.0, false), Color::WHITE, Rect::new(br.left + 6.0, br.top, br.right - 4.0, br.bottom), StringAlignment::Near);
                    }
                    g.restore(saved);
                    ry += h;
                }
                paint::hline(g, th.border, inner.left, inner.right, ry);
            }
            if has_bar {
                let rail = Rect::new(tl, card.bottom - 12.0, inner.right, card.bottom - 1.0);
                if let Some(t) = paint::scroll_thumb(g, th, rail, false, self.heat_x, time_w, time_view, self.hover == Some(Hit::HBar)) {
                    self.hits.push((t, Hit::HBar));
                }
            }
            y = card.bottom;
        }

        // ── Legend ──
        y += 16.0;
        let f12 = font(12.0, false);
        let mut lx = left;
        let swatches: [(Color, &str, bool); 4] = [(th.surface1, Resources::res_heat_free(), true), (hex(0x34a853), Resources::res_heat_busy(), false), (hex(0xe37400), Resources::res_heat_full(), false), (hex(0xd93025), Resources::res_heat_over(), false)];
        for (c, l, border) in swatches {
            let sq = Rect::new(lx, y + 3.0, lx + 14.0, y + 17.0);
            g.fill_rounded_rectangle(Brush::solid(c), sq, 2.0);
            if border {
                g.draw_rounded_rectangle(&kubuno_desktop::ui::graphics::Pen::new(th.border, 1.0), sq, 2.0);
            }
            let w = text_w(g, l, &f12);
            text(g, l, &f12, th.text_secondary, Rect::new(lx + 20.0, y, lx + 22.0 + w, y + 20.0), StringAlignment::Near);
            lx += 20.0 + w + 20.0;
        }
        let note = Resources::res_heat_note();
        let nw = text_w(g, note, &f12);
        if lx + nw > right {
            lx = left;
            y += 22.0;
        }
        text(g, note, &f12, th.text_tertiary, Rect::new(lx, y, right, y + 20.0), StringAlignment::Near);
        y += 20.0 + PAD;
        self.content_h = y + self.scroll[si] - page.top;
        let view_h = page.bottom - page.top;
        if self.scroll[si] > (self.content_h - view_h).max(0.0) {
            self.scroll[si] = (self.content_h - view_h).max(0.0);
        }
        let vrail = Rect::new(page.right - 10.0, page.top, page.right, page.bottom);
        if let Some(t) = paint::scroll_thumb(g, th, vrail, true, self.scroll[si], self.content_h, view_h, self.hover == Some(Hit::VBar)) {
            self.hits.push((t, Hit::VBar));
        }
    }

    /// Centres today's column (`scrollIntoView({ inline: 'center' })`).
    pub(super) fn scroll_heat_to_today(&mut self) {
        let Some(model) = self.model.clone() else { return };
        let m = model.borrow();
        let Some(ed) = m.ed.as_ref() else { return };
        let start = ed.project_start();
        let total = view::total_days(ed.data(), start);
        let cols = res::heat_columns(self.scale, start, total, m.view.today.days_since(start) as i32, m.view.lang);
        if let Some(i) = cols.iter().position(|c| c.is_today) {
            let w = self.scale.col_width() as f32;
            self.heat_x = (i as f32 * w + w / 2.0 - self.heat_view.1 / 2.0).clamp(0.0, (self.heat_view.0 - self.heat_view.1).max(0.0));
        }
    }

    /// A cell's tooltip: « 140 % · 11.2 h/j ».
    pub(super) fn heat_tool_tip(&self, ax: f32, ay: f32) -> Option<String> {
        if self.sub != Sub::Load {
            return None;
        }
        let (_, row) = self.hits.iter().find(|(r, h)| matches!(h, Hit::HeatRow(_)) && ay >= r.top && ay < r.bottom && ax > r.right)?;
        let Hit::HeatRow(id) = row else { return None };
        let model = self.model.as_ref()?.borrow();
        let ed = model.ed.as_ref()?;
        let d = ed.data();
        let r = d.resources.iter().find(|r| r.id == *id)?;
        let label = self.hits.iter().find(|(_, h)| h == row)?.0;
        let start = ed.project_start();
        let total = view::total_days(d, start);
        let cols = res::heat_columns(self.scale, start, total, model.view.today.days_since(start) as i32, model.view.lang);
        let w = self.scale.col_width() as f32;
        let ci = ((ax - label.right + self.heat_x) / w).floor();
        let c = cols.get(ci.max(0.0) as usize).filter(|_| ci >= 0.0)?;
        let ratios = res::daily_ratios(d, &res::work_resources(d), total);
        let cap = if r.capacity > 0.0 { r.capacity } else { 1.0 };
        let (ratio, hours) = res::cell_value(ratios.get(&r.id), c, cap);
        Some(Resources::res_heat_cell().replace("{pct}", &(js_round(ratio * 100.0) as i64).to_string()).replace("{h}", &format!("{hours:.1}")))
    }
}

/// The height of an unfolded row (`py-2`, 20-px bars `space-y-1`, or the « Aucune affectation. » line).
fn expanded_h(n: usize) -> f32 {
    16.0 + if n == 0 { 16.0 } else { n as f32 * 24.0 - 4.0 }
}
