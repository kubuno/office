//! « Coûts » (`ResourceCostView.tsx`): the title, four KPI cards (total planned, resources, mean per resource,
//! the mix by type), the assumption callout and the table — resource, type, rate, planned quantity, planned
//! and actual cost — most expensive first (a header sorts), with its total.

use kubuno_desktop::ui::graphics::{Brush, Color, Graphics, Pen, StringAlignment};
use kubuno_desktop::ui::Rect;
use kubuno_office_projects_core::resources::{self as res, CostRow};

use super::paint::{self, font, text, text_h, text_w};
use super::{Hit, ResourcesView, Sub};
use crate::controls::model::{Palette, Shared};
use crate::Resources;

const PAD: f32 = 24.0;
const HEAD_H: f32 = 46.0;
const ROW_H: f32 = 57.0;
/// `minTableWidth`, the fixed columns (type, rate, quantity, planned, actual) and the resource's minimum.
const MIN_TABLE: f32 = 880.0;
const FIXED: [f32; 5] = [130.0, 130.0, 160.0, 150.0, 150.0];

/// A column's header.
pub fn header(c: usize) -> &'static str {
    match c {
        0 => Resources::res_cost_col_resource(),
        1 => Resources::res_cost_col_kind(),
        2 => Resources::res_cost_col_rate(),
        3 => Resources::res_cost_col_units(),
        4 => Resources::res_cost_col_planned(),
        _ => Resources::res_cost_col_actual(),
    }
}

fn sort_value(d: &kubuno_office_projects_core::model::ProjectData, r: &CostRow, c: usize) -> (f64, String) {
    let res = &d.resources[r.resource];
    match c {
        0 => (0.0, res::collation_key(&res.name).0),
        1 => (0.0, res.kind.clone()),
        2 => (r.rate, String::new()),
        3 => (r.planned_hours.or(r.planned_qty).unwrap_or(-1.0), String::new()),
        4 => (r.planned_cost, String::new()),
        _ => (r.actual_cost.unwrap_or(-1.0), String::new()),
    }
}

impl ResourcesView {
    pub(super) fn paint_cost(&mut self, g: &Graphics<'_>, th: &Palette, model: &Shared, page: Rect) {
        let m = model.borrow();
        let Some(ed) = m.ed.as_ref() else { return };
        let d = ed.data();
        let lang = m.view.lang;
        let currency = ed.resources.currency_code();
        let money = |v: f64| res::money(v, &currency, lang);
        let (rows, has_actuals) = res::cost_rows(d, ed.resources.default_hourly_rate.unwrap_or(0.0));
        let total_planned: f64 = rows.iter().map(|r| r.planned_cost).sum();
        let total_actual: f64 = rows.iter().map(|r| r.actual_cost.unwrap_or(0.0)).sum();
        let avg = if rows.is_empty() { 0.0 } else { total_planned / rows.len() as f64 };
        g.fill_rectangle(Brush::solid(th.surface1), page);
        let si = Sub::Cost.index();
        let left = page.left + PAD;
        let right = page.right - PAD;
        let mut y = page.top + PAD - self.scroll[si];

        g.draw_icon("Coins", Rect::new(left, y + 4.0, left + 20.0, y + 24.0), 20.0, th.text_secondary);
        text(g, Resources::res_cost_title(), &font(20.0, true), th.text_primary, Rect::new(left + 32.0, y, right, y + 28.0), StringAlignment::Near);
        y += 28.0 + 16.0;

        // ── KPI cards (`grid-cols-2 lg:grid-cols-4 gap-4`) ──
        let four = right - left >= 760.0;
        let per_row = if four { 4 } else { 2 };
        let cw = (right - left - 16.0 * (per_row as f32 - 1.0)) / per_row as f32;
        let mix: Vec<(String, f64)> = res::cost_by_kind(d, &rows).into_iter().filter(|(k, a)| *a > 0.0 && res::meta(k).is_some()).collect();
        let mix_h = 16.0 + 6.0 + if mix.is_empty() { 16.0 } else { mix.len() as f32 * 20.0 - 4.0 };
        let hint = has_actuals.then(|| Resources::res_cost_total_actual().replace("{v}", &money(total_actual)));
        let fig_h: f32 = 16.0 + 28.0 + if hint.is_some() { 16.0 } else { 0.0 };
        let card_h = 32.0 + fig_h.max(mix_h);
        let figures: [(&'static str, &str, String, Option<String>, Color); 3] = [
            ("Wallet", Resources::res_cost_kpi_total(), money(total_planned), hint, th.primary),
            ("Users", Resources::res_cost_kpi_count(), res::number(rows.len() as f64, lang, 0), None, th.text_secondary),
            ("Coins", Resources::res_cost_kpi_avg(), money(avg), None, th.text_secondary),
        ];
        for (k, (icon, label, value, hint, ink)) in figures.into_iter().enumerate() {
            let (cx, cy) = (left + (k % per_row) as f32 * (cw + 16.0), y + (k / per_row) as f32 * (card_h + 16.0));
            let card = Rect::new(cx, cy, cx + cw, cy + card_h);
            paint::card(g, card, th.surface0, th.border, 12.0);
            g.draw_icon(icon, Rect::new(card.left + 16.0, card.top + 18.0, card.left + 34.0, card.top + 36.0), 18.0, ink);
            let tx = card.left + 16.0 + 18.0 + 12.0;
            text(g, label, &font(12.0, false), th.text_secondary, Rect::new(tx, card.top + 16.0, card.right - 12.0, card.top + 32.0), StringAlignment::Near);
            text(g, &value, &font(18.0, true), th.text_primary, Rect::new(tx, card.top + 32.0, card.right - 12.0, card.top + 60.0), StringAlignment::Near);
            if let Some(h) = hint {
                text(g, &h, &font(11.0, false), th.text_tertiary, Rect::new(tx, card.top + 60.0, card.right - 12.0, card.top + 76.0), StringAlignment::Near);
            }
        }
        // The mix recap.
        let (cx, cy) = (left + (3 % per_row) as f32 * (cw + 16.0), y + (3 / per_row) as f32 * (card_h + 16.0));
        let card = Rect::new(cx, cy, cx + cw, cy + card_h);
        paint::card(g, card, th.surface0, th.border, 12.0);
        g.draw_icon("PieChart", Rect::new(card.left + 16.0, card.top + 16.0, card.left + 29.0, card.top + 32.0), 13.0, th.text_secondary);
        text(g, Resources::res_cost_kpi_mix(), &font(12.0, false), th.text_secondary, Rect::new(card.left + 35.0, card.top + 16.0, card.right - 12.0, card.top + 32.0), StringAlignment::Near);
        let mut my = card.top + 16.0 + 16.0 + 6.0;
        let f11 = font(11.0, false);
        if mix.is_empty() {
            text(g, Resources::res_cost_mix_empty(), &f11, th.text_tertiary, Rect::new(card.left + 16.0, my, card.right - 16.0, my + 16.0), StringAlignment::Near);
        }
        for (kind, amount) in &mix {
            let Some(meta) = res::meta(kind) else { continue };
            let dot = paint::tint_of(meta, th);
            g.fill_ellipse(Brush::solid(dot), Rect::new(card.left + 16.0, my + 4.0, card.left + 24.0, my + 12.0));
            let pct = if total_planned > 0.0 { amount / total_planned * 100.0 } else { 0.0 };
            text(g, meta.label(lang), &f11, th.text_secondary, Rect::new(card.left + 32.0, my, card.right - 56.0, my + 16.0), StringAlignment::Near);
            text(g, &format!("{}%", kubuno_office_projects_core::rollup::js_round(pct) as i64), &f11, th.text_primary, Rect::new(card.right - 60.0, my, card.right - 12.0, my + 16.0), StringAlignment::Far);
            my += 20.0;
        }
        y += (4 / per_row) as f32 * (card_h + 16.0);

        // ── The assumption callout ──
        let note = Resources::res_cost_assumption().replace("{h}", &(res::HOURS_PER_DAY as i64).to_string());
        let f14 = font(14.0, false);
        let tw = right - left - 16.0 - 16.0 - 12.0 - 16.0;
        let nh = text_h(g, &note, &f14, tw).max(20.0);
        let call = Rect::new(left, y, right, y + nh + 24.0);
        let (fill, border) = if th.dark { (th.primary.with_alpha(0.12), th.primary.with_alpha(0.3)) } else { (paint::wash(th.primary, 0.1), th.primary.with_alpha(0.2)) };
        g.fill_rounded_rectangle(Brush::solid(fill), call, 8.0);
        g.draw_rounded_rectangle(&Pen::new(border, 1.0), Rect::new(call.left + 0.5, call.top + 0.5, call.right - 0.5, call.bottom - 0.5), 8.0);
        g.draw_icon("Info", Rect::new(call.left + 16.0, call.top + 14.0, call.left + 32.0, call.top + 30.0), 16.0, th.primary);
        g.draw_string(&note, &f14, Brush::solid(th.text_primary), Rect::new(call.left + 44.0, call.top + 12.0, call.left + 44.0 + tw, call.bottom), &paint::wrapped(StringAlignment::Near));
        y = call.bottom + 16.0;

        // ── The table ──
        let width = right - left;
        let table_w = width.max(MIN_TABLE);
        let res_w = table_w - 32.0 - FIXED.iter().sum::<f32>();
        let mut xs = [0.0f32; 7];
        xs[0] = left + 16.0;
        xs[1] = xs[0] + res_w.max(200.0);
        for i in 0..5 {
            xs[i + 2] = xs[i + 1] + FIXED[i];
        }
        let mut order: Vec<&CostRow> = rows.iter().collect();
        match self.cost_sort {
            None => order.sort_by(|a, b| b.planned_cost.partial_cmp(&a.planned_cost).unwrap_or(std::cmp::Ordering::Equal)),
            Some((c, desc)) => order.sort_by(|a, b| {
                let o = sort_value(d, a, c).partial_cmp(&sort_value(d, b, c)).unwrap_or(std::cmp::Ordering::Equal);
                if desc {
                    o.reverse()
                } else {
                    o
                }
            }),
        }
        let footer_h = if rows.is_empty() { 0.0 } else { 52.0 };
        let body_h = if rows.is_empty() { 200.0 } else { rows.len() as f32 * ROW_H };
        let card = Rect::new(left, y, right, y + HEAD_H + body_h + footer_h);
        paint::card(g, card, th.surface0, th.border, 12.0);
        let aligns = [StringAlignment::Near, StringAlignment::Near, StringAlignment::Far, StringAlignment::Far, StringAlignment::Far, StringAlignment::Far];
        for c in 0..6 {
            let hr = Rect::new(xs[c], y, xs[c + 1] - 16.0, y + HEAD_H);
            let hot = self.hover == Some(Hit::CostSort(c));
            let mut label = header(c).to_string();
            if let Some((_, desc)) = self.cost_sort.filter(|(sc, _)| *sc == c) {
                label = if aligns[c] == StringAlignment::Far { format!("{} {label}", if desc { "↓" } else { "↑" }) } else { format!("{label} {}", if desc { "↓" } else { "↑" }) };
            }
            text(g, &label, &font(14.0, false), if hot { th.text_primary } else { th.text_secondary }, hr, aligns[c]);
            self.hits.push((Rect::new(xs[c], y, xs[c + 1], y + HEAD_H), Hit::CostSort(c)));
        }
        paint::hline(g, th.border, card.left, card.right, y + HEAD_H);
        let mut ry = y + HEAD_H;
        if rows.is_empty() {
            let mid = (left + right) / 2.0;
            g.fill_ellipse(Brush::solid(th.surface2), Rect::new(mid - 28.0, ry + 28.0, mid + 28.0, ry + 84.0));
            g.draw_icon("Coins", Rect::new(mid - 13.0, ry + 43.0, mid + 13.0, ry + 69.0), 26.0, th.text_secondary);
            text(g, Resources::res_cost_empty_title(), &font(16.0, true), th.text_primary, Rect::new(left, ry + 96.0, right, ry + 120.0), StringAlignment::Center);
            text(g, Resources::res_cost_empty(), &f14, th.text_secondary, Rect::new(left, ry + 124.0, right, ry + 146.0), StringAlignment::Center);
        }
        let f12 = font(12.0, false);
        for r in &order {
            let res_ = &d.resources[r.resource];
            let cy = ry + ROW_H / 2.0;
            paint::identity(g, th, res_, xs[0], xs[1] - 16.0, cy, 28.0, 14.0, 11.0);
            // The type badge (wraps like the web's 130-px column).
            if let Some(meta) = res::meta(&res_.kind) {
                let tint = paint::tint_of(meta, th);
                let label = meta.label(lang);
                let room = xs[2] - xs[1] - 16.0 - 8.0 - 13.0 - 6.0 - 8.0;
                let lw = text_w(g, label, &f12).min(room);
                let lh = text_h(g, label, &f12, room + 1.0);
                let two = lh > 20.0;
                let bh = if two { 36.0 } else { 22.0 };
                let br = Rect::new(xs[1], cy - bh / 2.0, xs[1] + 8.0 + 13.0 + 6.0 + lw + 8.0, cy + bh / 2.0);
                g.fill_rounded_rectangle(Brush::solid(paint::wash(tint, 0.12)), br, 6.0);
                g.draw_icon(meta.icon, Rect::new(br.left + 8.0, br.top, br.left + 21.0, br.bottom), 13.0, tint);
                g.draw_string(label, &f12, Brush::solid(tint), Rect::new(br.left + 27.0, br.top + if two { 2.0 } else { 3.0 }, br.right - 6.0, br.bottom), &paint::wrapped(StringAlignment::Near));
            } else {
                text(g, &res_.kind, &f12, th.text_secondary, Rect::new(xs[1], cy - 10.0, xs[2] - 16.0, cy + 10.0), StringAlignment::Near);
            }
            // Rate with its suffix.
            let rr = Rect::new(xs[2], cy - 10.0, xs[3] - 16.0, cy + 10.0);
            let suffix_w = text_w(g, &r.rate_suffix, &f14);
            text(g, &r.rate_suffix, &f14, th.text_tertiary, Rect::new(rr.right - suffix_w - 1.0, rr.top, rr.right, rr.bottom), StringAlignment::Far);
            text(g, &money(r.rate), &f14, th.text_primary, Rect::new(rr.left, rr.top, rr.right - suffix_w, rr.bottom), StringAlignment::Far);
            // Planned quantity.
            let qr = Rect::new(xs[3], cy - 10.0, xs[4] - 16.0, cy + 10.0);
            match (r.planned_hours, r.planned_qty) {
                (Some(h), _) => text(g, &format!("{} {}", res::number(h, lang, 1), Resources::res_cost_hours()), &f14, th.text_secondary, qr, StringAlignment::Far),
                (None, Some(q)) => {
                    let unit = res_.unit_label.clone().unwrap_or_else(|| Resources::res_cost_unit_generic().to_string());
                    text(g, &format!("{} {unit}", res::number(q, lang, 0)), &f14, th.text_secondary, qr, StringAlignment::Far)
                }
                _ => text(g, "—", &font(12.0, false), th.text_tertiary, qr, StringAlignment::Far),
            }
            text(g, &money(r.planned_cost), &font(14.0, true), th.text_primary, Rect::new(xs[4], cy - 10.0, xs[5] - 16.0, cy + 10.0), StringAlignment::Far);
            let ar = Rect::new(xs[5], cy - 10.0, xs[6] - 16.0, cy + 10.0);
            match r.actual_cost {
                Some(a) => text(g, &money(a), &f14, th.text_primary, ar, StringAlignment::Far),
                None => text(g, "—", &font(12.0, false), th.text_tertiary, ar, StringAlignment::Far),
            }
            ry += ROW_H;
            paint::hline(g, th.border, card.left, card.right, ry);
        }
        if !rows.is_empty() {
            // The footer: « Total » and the sums.
            let fr = Rect::new(card.left + 1.0, ry, card.right - 1.0, card.bottom - 1.0);
            g.fill_rectangle(Brush::solid(th.surface1), Rect::new(fr.left, fr.top, fr.right, fr.bottom - 10.0));
            g.fill_rounded_rectangle(Brush::solid(th.surface1), fr, 11.0);
            text(g, Resources::res_cost_total_row(), &font(14.0, true), th.text_primary, Rect::new(fr.left + 20.0, fr.top, fr.right, fr.bottom), StringAlignment::Near);
            let total = money(total_planned);
            let fb = font(16.0, true);
            let tw = text_w(g, &total, &fb);
            text(g, &total, &fb, th.text_primary, Rect::new(fr.right - 20.0 - tw - 2.0, fr.top, fr.right - 20.0, fr.bottom), StringAlignment::Far);
            if has_actuals {
                let a = Resources::res_cost_actual_total().replace("{v}", &money(total_actual));
                text(g, &a, &f14, th.text_secondary, Rect::new(fr.left, fr.top, fr.right - 20.0 - tw - 32.0, fr.bottom), StringAlignment::Far);
            }
        }
        y = card.bottom + PAD;
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
}
