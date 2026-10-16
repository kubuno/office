//! `ResourcesView` - the « Ressources » plan view, the web's resource-management workspace
//! (`web/src/project/resources/ResourcesView.tsx`): a small tab strip and four sub-views,
//!
//! * « Registre » (`ResourceRegistry.tsx`, [`registry`]): the project's resources in a grid - type, name,
//!   role, skills, capacity, rate, tasks, utilisation - edited in place, with « Ajouter », « Grouper par » and
//!   a search;
//! * « Plan de charge » (`ResourceHeatmap.tsx`, [`heatmap`]): utilisation per resource and per day / week /
//!   month on the semantic gradient, rows that unfold into their tasks;
//! * « Disponibilité » (`ResourceAvailability.tsx`, [`availability`]): each resource's time off;
//! * « Coûts » (`ResourceCostView.tsx`, [`cost`]): planned and actual cost per resource.
//!
//! The figures come from the core (`kubuno_office_projects_core::resources`). The view edits the shared editor
//! directly and raises `Edited` after each command, so the window feeds the server session; what it cannot do
//! itself (a menu of the window, the confirmation of a deletion, reading side data) leaves as an `Action`.

pub mod availability;
pub mod cost;
pub mod heatmap;
pub mod paint;
pub mod registry;

use std::collections::HashSet;

use kubuno_desktop::prelude::*;
use kubuno_desktop::ui::graphics::{Brush, StringAlignment};
use kubuno_desktop::ui::{Canvas, Rect, Size};
use kubuno_desktop::views::component::{AccessiblePart, Component, Control, ControlCore, EventCx, Keys, PaintEventCx};
use kubuno_desktop::views::events::Event;
use kubuno_office_projects_common::model::line_edit::LineEdit;
use kubuno_office_projects_core::resources::{GroupMode, Scale};

use crate::controls::model::{sample_model, Palette, Shared};
use crate::Resources;

use self::paint::{font, text};

/// The sub-tab strip's height (`Tabs size="sm"`: 6 + 20 + 9, and its rule).
const TABS_H: f32 = 36.0;
/// One wheel notch.
const WHEEL: f32 = 100.0;

/// Something the window must do: `add_menu` / `group_menu` (open that menu at `x`, `y`), `delete` (confirm,
/// then delete resource `key`), `refresh` (re-read what the view wants: [`ResourcesView::wanted`]).
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct ResourcesActionEventArgs {
    pub action: String,
    pub key: String,
    pub x: f32,
    pub y: f32,
}

/// A command of the view changed the project.
#[derive(kubuno_desktop::views::events::EventArgs, Debug, Clone, Default, PartialEq)]
pub struct ResourcesEditedEventArgs {
    pub label: String,
}

/// The sub-views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sub {
    #[default]
    Registry,
    Load,
    Availability,
    Cost,
}

const SUBS: [Sub; 4] = [Sub::Registry, Sub::Load, Sub::Availability, Sub::Cost];

impl Sub {
    fn index(self) -> usize {
        SUBS.iter().position(|s| *s == self).unwrap_or(0)
    }

    fn label(self) -> &'static str {
        match self {
            Sub::Registry => Resources::res_tab_registry(),
            Sub::Load => Resources::res_tab_load(),
            Sub::Availability => Resources::res_tab_avail(),
            Sub::Cost => Resources::res_tab_cost(),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Sub::Registry => "Boxes",
            Sub::Load => "BarChart3",
            Sub::Availability => "CalendarOff",
            Sub::Cost => "TrendingUp",
        }
    }
}

/// A field edited in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKey {
    Search,
    Name(String),
    Role(String),
    Capacity(String),
    Rate(String),
    Skill(String),
    Reason,
    From,
    To,
}

/// What a point of the view is (recorded while painting, in window coordinates).
#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    Tab(Sub),
    Add,
    Group,
    Field(FieldKey),
    SkillRemove(String, String),
    Delete(String),
    Row(String),
    Scale(Scale),
    Prev,
    Next,
    Today,
    HeatRow(String),
    Pick(String),
    AddTimeOff,
    DeleteTimeOff(String, String),
    CostSort(usize),
    VBar,
    HBar,
}

/// The field being edited.
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    pub key: FieldKey,
    pub le: LineEdit,
}

/// The resources workspace (see the module doc).
#[derive(kubuno_desktop::views::component::Component)]
#[kubuno(extends = Control, overrides(Control))]
#[category("Projects")]
#[toolbox(icon = "users")]
#[default_event("Action")]
pub struct ResourcesView {
    base: ControlCore,
    /// Occurs when the window must act (a menu, a confirmation, a read).
    #[event]
    #[category("Action")]
    pub action: Event<ResourcesActionEventArgs>,
    /// Occurs after an edit made in the view.
    #[event]
    #[category("Action")]
    pub edited: Event<ResourcesEditedEventArgs>,
    model: Option<Shared>,
    bounds: Rect,
    sub: Sub,
    hits: Vec<(Rect, Hit)>,
    hover: Option<Hit>,
    hover_row: Option<String>,
    has_focus: bool,
    caret_ms: u64,
    edit: Option<Edit>,
    /// The registry's committed search and grouping.
    search: String,
    group: GroupMode,
    /// The heatmap's scale, unfolded rows and horizontal scroll.
    scale: Scale,
    expanded: HashSet<String>,
    heat_x: f32,
    heat_view: (f32, f32),
    /// The availability sub-view's resource and its form.
    picked: Option<String>,
    from: String,
    to: String,
    reason: String,
    /// The cost table's sort: column and descending.
    cost_sort: Option<(usize, bool)>,
    /// Vertical scroll per sub-view, the registry's horizontal scroll.
    scroll: [f32; 4],
    content_h: f32,
    reg_x: f32,
    reg_w: f32,
    drag: Option<(Hit, f32, f32)>,
    /// The side reads already asked for (`side`, `time_off:<id>`).
    asked: HashSet<String>,
}

impl Default for ResourcesView {
    fn default() -> Self {
        Self {
            base: ControlCore::default(),
            action: Event::default(),
            edited: Event::default(),
            model: None,
            bounds: Rect::default(),
            sub: Sub::Registry,
            hits: Vec::new(),
            hover: None,
            hover_row: None,
            has_focus: false,
            caret_ms: 0,
            edit: None,
            search: String::new(),
            group: GroupMode::None,
            scale: Scale::Week,
            expanded: HashSet::new(),
            heat_x: 0.0,
            heat_view: (0.0, 0.0),
            picked: None,
            from: String::new(),
            to: String::new(),
            reason: String::new(),
            cost_sort: None,
            scroll: [0.0; 4],
            content_h: 0.0,
            reg_x: 0.0,
            reg_w: 0.0,
            drag: None,
            asked: HashSet::new(),
        }
    }
}

fn now() -> u64 {
    kubuno_desktop::controls::host::now_ms()
}

fn palette() -> Palette {
    Palette::of(kubuno_desktop::Application::theme().mode == kubuno_desktop::ui::ThemeMode::Dark)
}

impl ResourcesView {
    pub fn set_model(&mut self, model: Shared) {
        self.model = Some(model);
        self.invalidate();
    }

    /// The grouping chosen in the window's « Grouper par » menu.
    pub fn set_group(&mut self, mode: &str) {
        self.group = GroupMode::parse(mode);
        self.scroll[0] = 0.0;
        self.invalidate();
    }

    pub fn group(&self) -> &'static str {
        self.group.as_str()
    }

    /// Opens a sub-view (`registry`, `load`, `availability`, `cost`: the `--view resources:<sub>` option).
    pub fn set_sub(&mut self, name: &str) {
        self.sub = match name {
            "load" => Sub::Load,
            "availability" => Sub::Availability,
            "cost" => Sub::Cost,
            _ => Sub::Registry,
        };
        self.invalidate();
    }

    /// The side reads the view needs and has not asked for yet (`side`, `time_off` + resource id); marked as
    /// asked. The window passes them to the server session.
    pub fn wanted(&mut self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let Some(model) = self.model.clone() else { return out };
        let m = model.borrow();
        let Some(ed) = m.ed.as_ref() else { return out };
        if ed.resources.members.is_none() && self.asked.insert("side".into()) {
            out.push(("side".into(), String::new()));
        }
        if self.sub == Sub::Availability {
            if let Some(id) = self.picked_resource(ed.data()) {
                if !ed.resources.time_off.contains_key(&id) && self.asked.insert(format!("time_off:{id}")) {
                    out.push(("time_off".into(), id));
                }
            }
        }
        out
    }

    /// Forgets what was asked (a new session, a re-read project): asked again on the next refresh.
    pub fn reset_asked(&mut self) {
        self.asked.clear();
    }

    /// Runs an editor command, then tells the window.
    fn command(&mut self, label: &str, f: impl FnOnce(&mut kubuno_office_projects_core::editor::Editor) -> Result<(), kubuno_office_projects_core::change::Refused>) {
        if self.run(f) {
            self.raise_edited(ResourcesEditedEventArgs { label: label.into() });
        }
    }

    /// Runs an editor command without raising `Edited` (the window asked for it and feeds the session).
    fn run(&mut self, f: impl FnOnce(&mut kubuno_office_projects_core::editor::Editor) -> Result<(), kubuno_office_projects_core::change::Refused>) -> bool {
        let Some(model) = self.model.clone() else { return false };
        {
            let mut m = model.borrow_mut();
            let Some(ed) = m.ed.as_mut() else { return false };
            ed.set_now(kubuno_office_projects_common::platform::current().clock.now_ms());
            if let Err(e) = f(ed) {
                ed.messages.push(e.to_string());
            }
        }
        self.invalidate();
        true
    }

    /// Adds a resource from the « Ajouter » menu: `member:<user id>` or `kind:<kind>`. The window feeds the
    /// session after it.
    pub fn add_from_menu(&mut self, key: &str) {
        let Some(model) = self.model.clone() else { return };
        let lang = model.borrow().view.lang;
        let body = if let Some(uid) = key.strip_prefix("member:") {
            let m = model.borrow();
            let Some(member) = m.ed.as_ref().and_then(|e| e.resources.members.as_ref()).and_then(|ms| ms.iter().find(|x| x.id == uid)).cloned() else { return };
            let mut b = serde_json::Map::new();
            b.insert("name".into(), member.display_name.clone().into());
            b.insert("user_id".into(), member.id.clone().into());
            b.insert("kind".into(), "person".into());
            b.insert("color".into(), kubuno_office_projects_core::resources::member_color(&member.id).into());
            b
        } else if let Some(kind) = key.strip_prefix("kind:") {
            kubuno_office_projects_core::resources::new_resource_body(kind, lang, Resources::res_unit())
        } else {
            return;
        };
        self.run(|ed| ed.create_resource_with(body).map(|_| ()));
    }

    fn raise_at(&mut self, action: &str, key: &str, r: Rect) {
        self.raise_action(ResourcesActionEventArgs { action: action.into(), key: key.into(), x: r.left, y: r.bottom + 4.0 });
    }

    fn hit_at(&self, ax: f32, ay: f32) -> Option<Hit> {
        self.hits.iter().rev().find(|(r, _)| r.contains(ax, ay)).map(|(_, h)| h.clone())
    }

    fn hit_rect(&self, h: &Hit) -> Option<Rect> {
        self.hits.iter().find(|(_, x)| x == h).map(|(r, _)| *r)
    }

    fn caret_on(&self) -> bool {
        self.has_focus && (now().saturating_sub(self.caret_ms) < 500 || (now() - self.caret_ms) / 500 % 2 == 1)
    }

    /// The page under the tab strip.
    fn page(&self) -> Rect {
        let b = self.bounds;
        Rect::new(b.left, b.top + TABS_H, b.right, b.bottom)
    }

    /// The resource the availability sub-view shows (the first schedulable one by default).
    fn picked_resource(&self, d: &kubuno_office_projects_core::model::ProjectData) -> Option<String> {
        let list = kubuno_office_projects_core::resources::work_resources(d);
        self.picked.clone().filter(|id| list.iter().any(|&i| d.resources[i].id == *id)).or_else(|| list.first().map(|&i| d.resources[i].id.clone()))
    }

    /// Starts editing `key` with `text`.
    fn begin(&mut self, key: FieldKey, text: &str) {
        self.commit();
        let mut le = LineEdit::new(text);
        le.select_all();
        self.edit = Some(Edit { key, le });
        self.caret_ms = now();
        self.invalidate();
    }

    fn cancel(&mut self) {
        if let Some(e) = self.edit.take() {
            if e.key == FieldKey::Search {
                self.search.clear();
            }
        }
        self.invalidate();
    }

    /// Commits the field being edited (blur / Enter), one PATCH per change - the web's ghost fields.
    pub fn commit(&mut self) {
        let Some(e) = self.edit.take() else { return };
        let v = e.le.text();
        let t = v.trim().to_string();
        let current = |id: &str| -> Option<kubuno_office_projects_core::model::Resource> { self.model.as_ref()?.borrow().data()?.resources.iter().find(|r| r.id == id).cloned() };
        let num = |s: &str| -> Option<f64> { s.replace(',', ".").parse::<f64>().ok().filter(|n| n.is_finite()) };
        let patch = |k: &str, v: serde_json::Value| {
            let mut p = serde_json::Map::new();
            p.insert(k.into(), v);
            p
        };
        match e.key {
            FieldKey::Search => self.search = v,
            FieldKey::Reason => self.reason = v,
            FieldKey::From => self.from = t,
            FieldKey::To => self.to = t,
            FieldKey::Name(id) => {
                if let Some(r) = current(&id).filter(|r| !t.is_empty() && t != r.name) {
                    let p = patch("name", t.clone().into());
                    self.command("resource-edit", |ed| ed.update_resource(&r.id, p));
                }
            }
            FieldKey::Role(id) => {
                if let Some(r) = current(&id).filter(|r| t != r.role) {
                    let p = patch("role", t.clone().into());
                    self.command("resource-edit", |ed| ed.update_resource(&r.id, p));
                }
            }
            FieldKey::Capacity(id) => {
                if let Some(r) = current(&id) {
                    let pct = num(&t).unwrap_or(0.0);
                    if (pct - (r.capacity * 100.0).round()).abs() > f64::EPSILON {
                        let p = patch("capacity", serde_json::json!(pct / 100.0));
                        self.command("resource-edit", |ed| ed.update_resource(&r.id, p));
                    }
                }
            }
            FieldKey::Rate(id) => {
                if let Some(r) = current(&id) {
                    let next = num(&t);
                    if next != r.hourly_rate {
                        let p = patch("hourly_rate", next.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null));
                        self.command("resource-edit", |ed| ed.update_resource(&r.id, p));
                    }
                }
            }
            FieldKey::Skill(id) => {
                if let Some(r) = current(&id) {
                    if !t.is_empty() && !r.skills.contains(&t) {
                        let mut skills = r.skills.clone();
                        skills.push(t.clone());
                        let p = patch("skills", serde_json::json!(skills));
                        self.command("resource-edit", |ed| ed.update_resource(&r.id, p));
                    }
                }
            }
        }
        self.invalidate();
    }

    fn remove_skill(&mut self, id: &str, skill: &str) {
        let Some(r) = self.model.as_ref().and_then(|m| m.borrow().data().and_then(|d| d.resources.iter().find(|r| r.id == id).cloned())) else { return };
        let skills: Vec<String> = r.skills.iter().filter(|s| *s != skill).cloned().collect();
        let mut p = serde_json::Map::new();
        p.insert("skills".into(), serde_json::json!(skills));
        self.command("resource-edit", |ed| ed.update_resource(&r.id, p));
    }

    /// The time-off form's dates as ISO (`jj/mm/aaaa`, `aaaa-mm-jj` or `mm/dd/yyyy` in English).
    fn parse_date(&self, s: &str) -> Option<String> {
        let s = s.trim();
        if let Some(d) = kubuno_office_projects_core::date::Date::parse(s) {
            return Some(d.iso());
        }
        let p: Vec<&str> = s.split(['/', '.', '-']).collect();
        if p.len() != 3 {
            return None;
        }
        let n = |x: &str| x.trim().parse::<u32>().ok();
        let en = self.model.as_ref().is_some_and(|m| m.borrow().view.lang == kubuno_office_projects_core::date::Lang::En);
        let (d, m, y) = if en { (n(p[1])?, n(p[0])?, n(p[2])?) } else { (n(p[0])?, n(p[1])?, n(p[2])?) };
        let y = if y < 100 { 2000 + y } else { y };
        kubuno_office_projects_core::date::Date::new(y as i32, m, d).map(|x| x.iso())
    }

    fn add_time_off(&mut self) {
        let (Some(from), Some(to)) = (self.parse_date(&self.from), self.parse_date(&self.to)) else { return };
        let Some(id) = self.model.as_ref().and_then(|m| m.borrow().data().and_then(|d| self.picked_resource(d))) else { return };
        let reason = self.reason.clone();
        self.command("time-off", |ed| ed.create_time_off(&id, &from, &to, Some(&reason)));
        self.from.clear();
        self.to.clear();
        self.reason.clear();
    }

    fn scroll_by(&mut self, dy: f32, dx: f32) {
        let i = self.sub.index();
        let view = self.page().bottom - self.page().top;
        self.scroll[i] = (self.scroll[i] + dy).clamp(0.0, (self.content_h - view).max(0.0));
        match self.sub {
            Sub::Registry => self.reg_x = (self.reg_x + dx).clamp(0.0, (self.reg_w - (self.bounds.right - self.bounds.left)).max(0.0)),
            Sub::Load => self.heat_x = (self.heat_x + dx).clamp(0.0, (self.heat_view.0 - self.heat_view.1).max(0.0)),
            _ => {}
        }
        self.invalidate();
    }

    fn paint_tabs(&mut self, g: &kubuno_desktop::ui::graphics::Graphics<'_>, th: &Palette) {
        let b = self.bounds;
        let strip = Rect::new(b.left, b.top, b.right, b.top + TABS_H);
        g.fill_rectangle(Brush::solid(th.surface1), strip);
        paint::hline(g, th.border, strip.left, strip.right, strip.bottom);
        let f = font(14.0, true);
        let mut x = b.left + 8.0;
        for s in SUBS {
            let label = s.label();
            let w = 24.0 + 14.0 + 6.0 + paint::text_w(g, label, &f) + 24.0;
            let r = Rect::new(x, strip.top, x + w, strip.bottom - 1.0);
            let active = s == self.sub;
            let hot = self.hover == Some(Hit::Tab(s));
            if hot {
                g.fill_rectangle(Brush::solid(if active { th.primary.with_alpha(0.08) } else { th.surface2 }), r);
            }
            let ink = if active { th.primary } else if hot { th.text_primary } else { th.text_secondary };
            let tw = paint::text_w(g, label, &f);
            let ix = r.left + (w - (14.0 + 6.0 + tw)) / 2.0;
            g.draw_icon(s.icon(), Rect::new(ix, r.top + 1.0, ix + 14.0, r.bottom - 2.0), 14.0, ink);
            text(g, label, &f, ink, Rect::new(ix + 20.0, r.top + 1.0, ix + 20.0 + tw + 2.0, r.bottom - 2.0), StringAlignment::Near);
            if active {
                g.fill_rounded_rectangle(Brush::solid(th.primary), Rect::new(r.left, r.bottom - 3.0, r.right, r.bottom + 2.0), 3.0);
            }
            self.hits.push((r, Hit::Tab(s)));
            x += w;
        }
    }
}

impl Control for ResourcesView {
    fn get_preferred_size(&self, _canvas: &dyn Canvas, proposed: Size) -> Size {
        proposed
    }

    fn on_paint(&mut self, e: &mut PaintEventCx<'_>) {
        self.bounds = e.clip_rectangle;
        if self.model.is_none() && self.design_mode() {
            self.model = Some(std::rc::Rc::new(std::cell::RefCell::new(sample_model())));
        }
        let th = palette();
        let g = e.graphics;
        self.hits.clear();
        g.fill_rectangle(Brush::solid(th.surface0), self.bounds);
        if let Some(model) = self.model.clone() {
            if model.borrow().ed.is_some() {
                let page = self.page();
                let saved = g.save();
                g.intersect_clip(page);
                match self.sub {
                    Sub::Registry => self.paint_registry(g, &th, &model, page),
                    Sub::Load => self.paint_heatmap(g, &th, &model, page),
                    Sub::Availability => self.paint_availability(g, &th, &model, page),
                    Sub::Cost => self.paint_cost(g, &th, &model, page),
                }
                g.restore(saved);
            }
        }
        self.paint_tabs(g, &th);
        if self.edit.is_some() && self.has_focus {
            kubuno_desktop::controls::host::request_repaint_after(500);
        }
        e.raise(self, "OnPaint");
    }

    fn is_input_key(&self, key: Keys) -> bool {
        use kubuno_desktop::controls::host::vk;
        self.edit.is_some() && [vk::LEFT, vk::RIGHT, vk::HOME, vk::END, vk::TAB, vk::ENTER, vk::BACK, vk::DELETE, vk::ESCAPE].contains(&key.key.0)
    }

    fn on_got_focus(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        self.has_focus = true;
        self.invalidate();
        e.raise(&*self, "OnGotFocus");
    }

    fn on_lost_focus(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        self.has_focus = false;
        self.commit();
        e.raise(&*self, "OnLostFocus");
    }

    fn on_mouse_wheel(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let d = e.args().delta * WHEEL;
        if e.args().mods.shift {
            self.scroll_by(0.0, -d);
        } else {
            self.scroll_by(-d, 0.0);
        }
        e.raise(&*self, "OnMouseWheel");
    }

    fn on_mouse_down(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (ax, ay) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        self.focus();
        self.has_focus = true;
        let hit = self.hit_at(ax, ay);
        // Clicking elsewhere commits the field being edited (the web's blur).
        let keeps = matches!((&hit, &self.edit), (Some(Hit::Field(k)), Some(ed)) if *k == ed.key);
        if !keeps {
            self.commit();
        }
        let rect = hit.as_ref().and_then(|h| self.hit_rect(h)).unwrap_or_default();
        let data = self.model.as_ref().and_then(|m| m.borrow().data().cloned());
        match hit {
            Some(Hit::Tab(s)) => {
                if s != self.sub {
                    self.sub = s;
                    self.raise_at("refresh", "", rect);
                }
            }
            Some(Hit::Add) => self.raise_at("add_menu", "", rect),
            Some(Hit::Group) => self.raise_at("group_menu", self.group.as_str(), rect),
            Some(Hit::Field(k)) if !keeps => {
                let r = |id: &str| data.as_ref().and_then(|d| d.resources.iter().find(|r| r.id == id).cloned());
                let start = match &k {
                    FieldKey::Search => self.search.clone(),
                    FieldKey::Reason => self.reason.clone(),
                    FieldKey::From => self.from.clone(),
                    FieldKey::To => self.to.clone(),
                    FieldKey::Name(id) => r(id).map(|r| r.name).unwrap_or_default(),
                    FieldKey::Role(id) => r(id).map(|r| r.role).unwrap_or_default(),
                    FieldKey::Capacity(id) => r(id).map(|r| ((r.capacity * 100.0).round() as i64).to_string()).unwrap_or_default(),
                    FieldKey::Rate(id) => r(id).and_then(|r| r.hourly_rate).map(|v| kubuno_office_projects_core::resources::number(v, kubuno_office_projects_core::date::Lang::En, 2).replace(',', "")).unwrap_or_default(),
                    FieldKey::Skill(_) => String::new(),
                };
                // A name is renamed by a double click (`onDoubleClick`), the other fields by a click.
                if !matches!(k, FieldKey::Name(_)) {
                    self.begin(k, &start);
                }
            }
            Some(Hit::SkillRemove(id, s)) => self.remove_skill(&id, &s),
            Some(Hit::Delete(id)) => self.raise_at("delete", &id, rect),
            Some(Hit::Scale(s)) => {
                self.scale = s;
                self.heat_x = 0.0;
            }
            Some(Hit::Prev) => self.scroll_by(0.0, -(self.heat_view.1 * 0.6).round()),
            Some(Hit::Next) => self.scroll_by(0.0, (self.heat_view.1 * 0.6).round()),
            Some(Hit::Today) => self.scroll_heat_to_today(),
            Some(Hit::HeatRow(id)) => {
                if !self.expanded.remove(&id) {
                    self.expanded.insert(id);
                }
            }
            Some(Hit::Pick(id)) => {
                if self.picked.as_deref() != Some(id.as_str()) {
                    self.picked = Some(id);
                    self.from.clear();
                    self.to.clear();
                    self.reason.clear();
                    self.raise_at("refresh", "", rect);
                }
            }
            Some(Hit::AddTimeOff) => self.add_time_off(),
            Some(Hit::DeleteTimeOff(rid, id)) => self.command("time-off", |ed| ed.delete_time_off(&rid, &id)),
            Some(Hit::CostSort(c)) => {
                self.cost_sort = match self.cost_sort {
                    Some((x, desc)) if x == c => {
                        if desc {
                            Some((c, false))
                        } else {
                            None
                        }
                    }
                    _ => Some((c, true)),
                };
            }
            Some(h @ (Hit::VBar | Hit::HBar)) => self.drag = Some((h, ax, ay)),
            _ => {}
        }
        self.invalidate();
        e.raise(&*self, "OnMouseDown");
    }

    fn on_mouse_double_click(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (ax, ay) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        if let Some(Hit::Field(FieldKey::Name(id))) = self.hit_at(ax, ay) {
            let name = self.model.as_ref().and_then(|m| m.borrow().data().and_then(|d| d.resources.iter().find(|r| r.id == id).map(|r| r.name.clone()))).unwrap_or_default();
            self.begin(FieldKey::Name(id), &name);
        }
        e.raise(&*self, "OnMouseDoubleClick");
    }

    fn on_mouse_move(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        let (ax, ay) = (e.args().x + self.bounds.left, e.args().y + self.bounds.top);
        if let Some((h, x0, y0)) = self.drag.clone() {
            // Dragging a thumb: content moves by the drag scaled to the rail.
            let page = self.page();
            match h {
                Hit::VBar => {
                    let view = page.bottom - page.top;
                    let k = if view > 0.0 { self.content_h / view } else { 1.0 };
                    self.scroll_by((ay - y0) * k, 0.0);
                }
                _ => {
                    let (content, view) = if self.sub == Sub::Load { self.heat_view } else { (self.reg_w, self.bounds.right - self.bounds.left) };
                    let k = if view > 0.0 { content / view } else { 1.0 };
                    self.scroll_by(0.0, (ax - x0) * k);
                }
            }
            self.drag = Some((h, ax, ay));
        } else {
            let hit = self.hit_at(ax, ay);
            let row = self.hits.iter().rev().find(|(r, h)| matches!(h, Hit::Row(_)) && r.contains(ax, ay)).map(|(_, h)| match h {
                Hit::Row(id) => id.clone(),
                _ => String::new(),
            });
            if hit != self.hover || row != self.hover_row {
                self.hover = hit;
                self.hover_row = row;
                self.invalidate();
            }
        }
        e.raise(&*self, "OnMouseMove");
    }

    fn on_mouse_up(&mut self, e: &mut EventCx<'_, MouseEventArgs>) {
        self.drag = None;
        e.raise(&*self, "OnMouseUp");
    }

    fn on_mouse_leave(&mut self, e: &mut EventCx<'_, kubuno_desktop::views::events::EmptyEventArgs>) {
        if self.drag.is_none() {
            self.hover = None;
            self.hover_row = None;
            self.invalidate();
        }
        e.raise(&*self, "OnMouseLeave");
    }

    fn cursor_at(&self, x: f32, y: f32) -> Option<kubuno_desktop::controls::host::Cursor> {
        use kubuno_desktop::controls::host::Cursor;
        Some(match self.hit_at(x + self.bounds.left, y + self.bounds.top) {
            Some(Hit::Field(_)) => Cursor::IBeam,
            Some(Hit::Row(_)) | Some(Hit::VBar) | Some(Hit::HBar) | None => Cursor::Arrow,
            Some(_) => Cursor::Hand,
        })
    }

    fn tool_tip_at(&self, x: f32, y: f32) -> Option<String> {
        let (ax, ay) = (x + self.bounds.left, y + self.bounds.top);
        match self.hit_at(ax, ay)? {
            Hit::SkillRemove(..) => Some(Resources::res_skill_remove().to_string()),
            Hit::Delete(_) | Hit::DeleteTimeOff(..) => Some(Resources::res_delete().to_string()),
            Hit::Prev => Some(Resources::res_heat_prev().to_string()),
            Hit::Next => Some(Resources::res_heat_next().to_string()),
            _ => self.heat_tool_tip(ax, ay).or_else(|| self.kind_tool_tip(ax, ay)),
        }
    }

    fn accessible_parts(&self) -> Vec<AccessiblePart> {
        use kubuno_desktop::controls::host::access::AccessRole;
        let (l, t) = (self.bounds.left, self.bounds.top);
        let local = |r: &Rect| Rect::new(r.left - l, r.top - t, r.right - l, r.bottom - t);
        let model = self.model.clone();
        let m = model.as_ref().map(|m| m.borrow());
        let d = m.as_ref().and_then(|m| m.data());
        let name_of = |id: &str| d.and_then(|d| d.resources.iter().find(|r| r.id == id)).map(|r| r.name.clone()).unwrap_or_default();
        self.hits
            .iter()
            .filter_map(|(r, h)| {
                let (name, role) = match h {
                    Hit::Tab(s) => (s.label().to_string(), AccessRole::Tab),
                    Hit::Add => (Resources::add().to_string(), AccessRole::Button),
                    Hit::Group => (Resources::res_group_by().to_string(), AccessRole::Button),
                    Hit::Field(FieldKey::Search) => (Resources::res_search_ph().to_string(), AccessRole::Button),
                    Hit::Row(id) => (name_of(id), AccessRole::Row),
                    Hit::Delete(id) => (format!("{} {}", Resources::res_delete(), name_of(id)), AccessRole::Button),
                    Hit::Scale(s) => (heatmap::scale_label(*s).to_string(), AccessRole::Button),
                    Hit::Today => (Resources::res_heat_today().to_string(), AccessRole::Button),
                    Hit::Prev => (Resources::res_heat_prev().to_string(), AccessRole::Button),
                    Hit::Next => (Resources::res_heat_next().to_string(), AccessRole::Button),
                    Hit::HeatRow(id) | Hit::Pick(id) => (name_of(id), AccessRole::Row),
                    Hit::AddTimeOff => (Resources::add().to_string(), AccessRole::Button),
                    Hit::Field(FieldKey::From) => (Resources::res_avail_from().to_string(), AccessRole::Button),
                    Hit::Field(FieldKey::To) => (Resources::res_avail_to().to_string(), AccessRole::Button),
                    Hit::Field(FieldKey::Reason) => (Resources::res_avail_reason().to_string(), AccessRole::Button),
                    Hit::DeleteTimeOff(..) => (Resources::res_delete().to_string(), AccessRole::Button),
                    Hit::CostSort(c) => (cost::header(*c).to_string(), AccessRole::ColumnHeader),
                    _ => return None,
                };
                Some(AccessiblePart { name, role, bounds: local(r) })
            })
            .collect()
    }

    fn on_key_down(&mut self, e: &mut EventCx<'_, KeyEventArgs>) {
        use kubuno_desktop::controls::host::vk;
        let (key, mods) = (e.args().key.0, e.args().mods);
        let mut handled = true;
        if let Some(ed) = self.edit.as_mut() {
            match key {
                k if k == vk::ENTER || k == vk::TAB => {
                    let add = matches!(ed.key, FieldKey::Skill(_));
                    let key = ed.key.clone();
                    self.commit();
                    // « Entrée » in the skill field keeps it open for the next one.
                    if add {
                        self.begin(key, "");
                    }
                }
                k if k == vk::ESCAPE => self.cancel(),
                k if k == vk::LEFT => ed.le.left(mods.shift),
                k if k == vk::RIGHT => ed.le.right(mods.shift),
                k if k == vk::HOME => ed.le.home(mods.shift),
                k if k == vk::END => ed.le.end(mods.shift),
                k if k == vk::BACK => ed.le.backspace(),
                k if k == vk::DELETE => ed.le.delete(),
                0x41 if mods.ctrl => ed.le.select_all(),
                _ => handled = false,
            }
            self.caret_ms = now();
            self.invalidate();
        } else {
            handled = false;
        }
        if handled {
            e.args_mut().handled = true;
            e.args_mut().suppress_key_press = true;
        }
        e.raise(&*self, "OnKeyDown");
    }

    fn on_key_press(&mut self, e: &mut EventCx<'_, KeyPressEventArgs>) {
        let c = e.args().key_char;
        if let Some(ed) = self.edit.as_mut() {
            if c == ',' && matches!(ed.key, FieldKey::Skill(_)) {
                // `,` adds the skill typed so far (`onKeyDown` `,`).
                let key = ed.key.clone();
                self.commit();
                self.begin(key, "");
            } else if !c.is_control() {
                ed.le.insert(&c.to_string());
                if ed.key == FieldKey::Search {
                    self.scroll[0] = 0.0;
                }
            }
            self.caret_ms = now();
            self.invalidate();
            e.args_mut().handled = true;
        }
        e.raise(&*self, "OnKeyPress");
    }
}
