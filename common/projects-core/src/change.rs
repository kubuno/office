//! A change to a project = one REST call of the office API (the web editor saves every edit as soon as it is
//! made, row by row: `projectsApi` in `web/src/api.ts`), and what that call does to the rows, so the desktop can
//! show it before the server answers — or without a server at all, replayed later.
//!
//! [`Change::request`] is the HTTP call; [`Change::apply`] mirrors the handler on a local [`ProjectData`]
//! (`server/src/handlers/projects.rs`: the `COALESCE` rules where `null` keeps a value, the tri-state fields
//! where `null` clears it, the cascades of a deletion, the dependency upsert, the outline renumbering, the
//! scheduler). Ids the server mints are minted locally as temporary ids ([`TEMP_PREFIX`]) and remapped when
//! the server answers ([`Change::created_id`]).

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::calendar::WorkingCalendar;
use crate::cpm;
use crate::date::Date;
use crate::model::{Assignment, Dependency, ProjectData, Resource, Task};
use crate::wbs;

/// The prefix of an id minted by the client for a row the server has not created yet.
pub const TEMP_PREFIX: &str = "local-";

pub fn is_temp(id: &str) -> bool {
    id.starts_with(TEMP_PREFIX)
}

/// One REST call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Change {
    /// `PATCH /projects/:id` (`UpdateProjectDto`).
    UpdateProject { patch: Map<String, Value> },
    /// `POST /projects/:id/tasks` (`CreateTaskDto`: name, parent_id, position, task_type, start_date,
    /// duration_days). `id` is the temporary id the client shows meanwhile.
    CreateTask { id: String, body: Map<String, Value> },
    /// `PATCH /projects/:id/tasks/:tid` (`UpdateTaskDto`).
    UpdateTask { id: String, patch: Map<String, Value> },
    /// `DELETE /projects/:id/tasks/:tid` (the subtree, its links and assignments cascade).
    DeleteTask { id: String },
    /// `POST /projects/:id/dependencies` — an upsert on `(from, to)`.
    CreateDependency { id: String, from: String, to: String, dep_type: Option<String>, lag_days: Option<i32> },
    /// `DELETE /projects/:id/dependencies/:did`.
    DeleteDependency { id: String },
    /// `POST /projects/:id/tasks/:tid/assign` `{ resource_id, units? }`.
    Assign { id: String, task: String, resource: String, units: Option<f64> },
    /// `DELETE /projects/:id/tasks/:tid/assign/:rid`.
    Unassign { task: String, resource: String },
    /// `POST /projects/:id/resources` (`CreateResourceDto`).
    CreateResource { id: String, body: Map<String, Value> },
    /// `PATCH /projects/:id/resources/:rid` (`UpdateResourceDto`).
    UpdateResource { id: String, patch: Map<String, Value> },
    /// `DELETE /projects/:id/resources/:rid`.
    DeleteResource { id: String },
    /// `POST /projects/:id/cpm` — the scheduler.
    ComputeCpm,
    /// `POST /projects/:id/baselines` `{ name? }`.
    CaptureBaseline { name: Option<String> },
    /// `PATCH /projects/:id/baselines/:bid` `{ name?, is_primary? }`.
    UpdateBaseline { id: String, patch: Map<String, Value> },
    /// `DELETE /projects/:id/baselines/:bid`.
    DeleteBaseline { id: String },
}

/// An HTTP request of the office API, relative to `/api/v1/office`.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpCall {
    pub method: &'static str,
    pub path: String,
    pub body: Option<Value>,
}

/// Percent-encodes a path segment (ids are UUIDs; a temporary id never reaches the server).
fn seg(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl Change {
    /// The call for project `project`.
    pub fn request(&self, project: &str) -> HttpCall {
        let p = format!("/projects/{}", seg(project));
        let call = |method, path: String, body: Option<Value>| HttpCall { method, path, body };
        match self {
            Change::UpdateProject { patch } => call("PATCH", p, Some(Value::Object(patch.clone()))),
            Change::CreateTask { body, .. } => call("POST", format!("{p}/tasks"), Some(Value::Object(body.clone()))),
            Change::UpdateTask { id, patch } => call("PATCH", format!("{p}/tasks/{}", seg(id)), Some(Value::Object(patch.clone()))),
            Change::DeleteTask { id } => call("DELETE", format!("{p}/tasks/{}", seg(id)), None),
            Change::CreateDependency { from, to, dep_type, lag_days, .. } => {
                let mut b = Map::new();
                b.insert("from_task_id".into(), json!(from));
                b.insert("to_task_id".into(), json!(to));
                if let Some(t) = dep_type {
                    b.insert("dep_type".into(), json!(t));
                }
                if let Some(l) = lag_days {
                    b.insert("lag_days".into(), json!(l));
                }
                call("POST", format!("{p}/dependencies"), Some(Value::Object(b)))
            }
            Change::DeleteDependency { id } => call("DELETE", format!("{p}/dependencies/{}", seg(id)), None),
            Change::Assign { task, resource, units, .. } => {
                let mut b = Map::new();
                b.insert("resource_id".into(), json!(resource));
                if let Some(u) = units {
                    b.insert("units".into(), json!(u));
                }
                call("POST", format!("{p}/tasks/{}/assign", seg(task)), Some(Value::Object(b)))
            }
            Change::Unassign { task, resource } => call("DELETE", format!("{p}/tasks/{}/assign/{}", seg(task), seg(resource)), None),
            Change::CreateResource { body, .. } => call("POST", format!("{p}/resources"), Some(Value::Object(body.clone()))),
            Change::UpdateResource { id, patch } => call("PATCH", format!("{p}/resources/{}", seg(id)), Some(Value::Object(patch.clone()))),
            Change::DeleteResource { id } => call("DELETE", format!("{p}/resources/{}", seg(id)), None),
            Change::ComputeCpm => call("POST", format!("{p}/cpm"), Some(json!({}))),
            Change::CaptureBaseline { name } => call("POST", format!("{p}/baselines"), Some(match name {
                Some(n) => json!({ "name": n }),
                None => json!({}),
            })),
            Change::UpdateBaseline { id, patch } => call("PATCH", format!("{p}/baselines/{}", seg(id)), Some(Value::Object(patch.clone()))),
            Change::DeleteBaseline { id } => call("DELETE", format!("{p}/baselines/{}", seg(id)), None),
        }
    }

    /// The temporary id of the row this change creates, and where the server's answer carries the real one
    /// (`{ task: { id } }`, `{ dependency: { id } }`…).
    pub fn created_id(&self) -> Option<(&str, &'static str)> {
        match self {
            Change::CreateTask { id, .. } => Some((id, "task")),
            Change::CreateDependency { id, .. } => Some((id, "dependency")),
            Change::Assign { id, .. } => Some((id, "assignment")),
            Change::CreateResource { id, .. } => Some((id, "resource")),
            _ => None,
        }
    }

    /// Renames one id everywhere the change names it (a temporary id once the server minted the real one).
    pub fn rename_id(&mut self, from: &str, to: &str) {
        self.map_ids(&mut |s| (s == from).then(|| to.to_string()));
    }

    /// Passes every id the change names (row ids, link ends, the ids inside a body) through `f`; `None`
    /// keeps the id.
    pub fn map_ids(&mut self, f: &mut dyn FnMut(&str) -> Option<String>) {
        fn fix(s: &mut String, f: &mut dyn FnMut(&str) -> Option<String>) {
            if let Some(n) = f(s) {
                *s = n;
            }
        }
        fn fix_map(m: &mut Map<String, Value>, f: &mut dyn FnMut(&str) -> Option<String>) {
            for k in ["parent_id", "version_id", "resource_id", "task_id"] {
                if let Some(n) = m.get(k).and_then(Value::as_str).and_then(&mut *f) {
                    m.insert(k.into(), json!(n));
                }
            }
        }
        match self {
            Change::UpdateProject { .. } | Change::ComputeCpm | Change::CaptureBaseline { .. } => {}
            Change::CreateTask { id, body } => {
                fix(id, f);
                fix_map(body, f);
            }
            Change::UpdateTask { id, patch } => {
                fix(id, f);
                fix_map(patch, f);
            }
            Change::DeleteTask { id } | Change::DeleteDependency { id } | Change::DeleteResource { id } | Change::DeleteBaseline { id } => fix(id, f),
            Change::UpdateBaseline { id, .. } | Change::UpdateResource { id, .. } | Change::CreateResource { id, .. } => fix(id, f),
            Change::CreateDependency { id, from, to, .. } => {
                fix(id, f);
                fix(from, f);
                fix(to, f);
            }
            Change::Assign { id, task, resource, .. } => {
                fix(id, f);
                fix(task, f);
                fix(resource, f);
            }
            Change::Unassign { task, resource } => {
                fix(task, f);
                fix(resource, f);
            }
        }
    }

    /// Whether the call names a row the server does not know yet (it must wait for the creation's answer).
    pub fn names_temp(&self) -> bool {
        let call = self.request("p");
        call.path.contains(TEMP_PREFIX) || call.body.is_some_and(|b| b.to_string().contains(TEMP_PREFIX))
    }
}

/// Why a change cannot be made (the server would answer 4xx; the desktop refuses before sending).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The row is not in the project (deleted meanwhile).
    NotFound,
    /// « Une tâche ne peut pas dépendre d'elle-même ».
    SelfDependency,
    /// « Cette liaison créerait une boucle : la tâche suivante précède déjà celle-ci. »
    Loop,
    /// « Réparentage invalide : la tâche cible est la tâche elle-même ou l'une de ses sous-tâches ».
    ParentCycle,
    /// « Cette contrainte demande une date. » / « Contrainte inconnue ».
    Constraint(String),
    /// The scheduler refused (a loop, a calendar without a working day).
    Schedule(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::NotFound => f.write_str("Élément introuvable"),
            Refused::SelfDependency => f.write_str("Une tâche ne peut pas dépendre d'elle-même"),
            Refused::Loop => f.write_str("Cette liaison créerait une boucle : la tâche suivante précède déjà celle-ci."),
            Refused::ParentCycle => f.write_str("Réparentage invalide : la tâche cible est la tâche elle-même ou l'une de ses sous-tâches"),
            Refused::Constraint(m) | Refused::Schedule(m) => f.write_str(m),
        }
    }
}

/// What the local mirror of the scheduler needs (the server reads them from its database).
#[derive(Debug, Clone)]
pub struct ScheduleContext {
    pub today: Date,
    /// The project's calendar for the scheduler (`calendar::for_schedule`), anchored on the project start.
    pub calendar: Option<WorkingCalendar>,
    /// The instance default when the project has no calendar (`project_default_include_weekends`).
    pub include_weekends_default: bool,
}

/// `COALESCE($n, column)`: a JSON `null` (or a missing key) keeps the value.
fn coalesce_str(patch: &Map<String, Value>, key: &str, slot: &mut String) {
    if let Some(s) = patch.get(key).and_then(Value::as_str) {
        *slot = s.to_string();
    }
}
fn coalesce_opt_str(patch: &Map<String, Value>, key: &str, slot: &mut Option<String>) {
    if let Some(s) = patch.get(key).and_then(Value::as_str) {
        *slot = Some(s.to_string());
    }
}
fn coalesce_i32(patch: &Map<String, Value>, key: &str, slot: &mut i32) {
    if let Some(n) = patch.get(key).and_then(Value::as_i64) {
        *slot = n as i32;
    }
}
fn coalesce_opt_f64(patch: &Map<String, Value>, key: &str, slot: &mut Option<f64>) {
    if let Some(n) = patch.get(key).and_then(Value::as_f64) {
        *slot = Some(n);
    }
}
/// Tri-state (`double_option`): absent keeps, `null` clears, a value sets.
fn tri_str(patch: &Map<String, Value>, key: &str, slot: &mut Option<String>) {
    match patch.get(key) {
        None => {}
        Some(Value::Null) => *slot = None,
        Some(v) => {
            if let Some(s) = v.as_str() {
                *slot = Some(s.to_string());
            }
        }
    }
}
fn tri_f64(patch: &Map<String, Value>, key: &str, slot: &mut Option<f64>) {
    match patch.get(key) {
        None => {}
        Some(Value::Null) => *slot = None,
        Some(v) => {
            if let Some(n) = v.as_f64() {
                *slot = Some(n);
            }
        }
    }
}

/// The task and all its descendants.
pub fn subtree(data: &ProjectData, id: &str) -> Vec<String> {
    let mut out = vec![id.to_string()];
    let mut i = 0;
    while i < out.len() {
        let cur = out[i].clone();
        for t in &data.tasks {
            if t.parent_id.as_deref() == Some(cur.as_str()) && !out.contains(&t.id) {
                out.push(t.id.clone());
            }
        }
        i += 1;
    }
    out
}

/// `closes_loop`: would `from → to` close a cycle (is `from` reachable from `to`)?
pub fn closes_loop(data: &ProjectData, from: &str, to: &str) -> bool {
    let mut stack = vec![to.to_string()];
    let mut seen = std::collections::HashSet::new();
    while let Some(cur) = stack.pop() {
        for d in data.dependencies.iter().filter(|d| d.from_task_id == cur) {
            if d.to_task_id == from {
                return true;
            }
            if seen.insert(d.to_task_id.clone()) {
                stack.push(d.to_task_id.clone());
            }
        }
    }
    false
}

const DATED: [&str; 6] = ["SNET", "SNLT", "FNET", "FNLT", "MSO", "MFO"];

impl Change {
    /// Checks the change against the rows the way the handler would, without changing anything.
    pub fn check(&self, data: &ProjectData) -> Result<(), Refused> {
        match self {
            Change::UpdateTask { id, patch } => {
                let task = data.task(id).ok_or(Refused::NotFound)?;
                if let Some(Value::String(p)) = patch.get("parent_id") {
                    if subtree(data, id).contains(p) {
                        return Err(Refused::ParentCycle);
                    }
                    if data.task(p).is_none() {
                        return Err(Refused::NotFound);
                    }
                }
                if let Some(ct) = patch.get("constraint_type").and_then(Value::as_str) {
                    if !["ASAP", "ALAP"].contains(&ct) && !DATED.contains(&ct) {
                        return Err(Refused::Constraint(format!("Contrainte inconnue : {ct}")));
                    }
                    if DATED.contains(&ct) {
                        let has = match patch.get("constraint_date") {
                            Some(Value::String(_)) => true,
                            None => task.constraint_date.is_some(),
                            Some(_) => false,
                        };
                        if !has {
                            return Err(Refused::Constraint("Cette contrainte demande une date.".into()));
                        }
                    }
                }
                Ok(())
            }
            Change::CreateTask { body, .. } => match body.get("parent_id").and_then(Value::as_str) {
                Some(p) if data.task(p).is_none() => Err(Refused::NotFound),
                _ => Ok(()),
            },
            Change::DeleteTask { id } => data.task(id).map(|_| ()).ok_or(Refused::NotFound),
            Change::CreateDependency { from, to, .. } => {
                if from == to {
                    return Err(Refused::SelfDependency);
                }
                if data.task(from).is_none() || data.task(to).is_none() {
                    return Err(Refused::NotFound);
                }
                if closes_loop(data, from, to) {
                    return Err(Refused::Loop);
                }
                Ok(())
            }
            Change::Assign { task, resource, .. } => {
                if data.task(task).is_none() || data.resource(resource).is_none() {
                    return Err(Refused::NotFound);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Mirrors the handler on `data`. `now` stamps a created row (`created_at`, the outline's tie-break).
    pub fn apply(&self, data: &mut ProjectData, ctx: &ScheduleContext, now: &str) -> Result<(), Refused> {
        self.check(data)?;
        match self {
            Change::UpdateProject { patch } => {
                let p = &mut data.project;
                coalesce_str(patch, "title", &mut p.title);
                coalesce_str(patch, "description", &mut p.description);
                coalesce_str(patch, "color", &mut p.color);
                coalesce_str(patch, "status", &mut p.status);
                coalesce_opt_str(patch, "start_date", &mut p.start_date);
                coalesce_opt_str(patch, "end_date", &mut p.end_date);
                if let Some(b) = patch.get("is_starred").and_then(Value::as_bool) {
                    p.is_starred = b;
                }
                if let Some(l) = patch.get("labels").filter(|v| !v.is_null()) {
                    p.labels = l.clone();
                }
                tri_str(patch, "parent_id", &mut p.parent_id);
            }
            Change::CreateTask { id, body } => {
                let mut t = Task::new(id.clone(), data.project.id.clone());
                if let Some(n) = body.get("name").and_then(Value::as_str) {
                    t.name = n.to_string();
                }
                t.parent_id = body.get("parent_id").and_then(Value::as_str).map(str::to_string);
                if let Some(tt) = body.get("task_type").and_then(Value::as_str) {
                    t.task_type = tt.to_string();
                }
                t.start_date = body.get("start_date").and_then(Value::as_str).map(str::to_string);
                if let Some(d) = body.get("duration_days").and_then(Value::as_i64) {
                    t.duration_days = d as i32;
                }
                // QUIRK (create_task): a position of 0 means « after the last sibling », like no position.
                let given = body.get("position").and_then(Value::as_i64).filter(|p| *p != 0);
                t.position = match given {
                    Some(p) => p as i32,
                    None => data.tasks.iter().filter(|x| x.parent_id == t.parent_id).map(|x| x.position + 1).max().unwrap_or(0),
                };
                t.created_at = now.to_string();
                t.updated_at = now.to_string();
                data.tasks.push(t);
                data.sort_tasks();
                wbs::renumber(&mut data.tasks);
            }
            Change::UpdateTask { id, patch } => {
                let Some(t) = data.task_mut(id) else { return Err(Refused::NotFound) };
                coalesce_str(patch, "name", &mut t.name);
                coalesce_str(patch, "description", &mut t.description);
                coalesce_str(patch, "status", &mut t.status);
                coalesce_str(patch, "priority", &mut t.priority);
                coalesce_str(patch, "task_type", &mut t.task_type);
                coalesce_opt_str(patch, "start_date", &mut t.start_date);
                coalesce_opt_str(patch, "end_date", &mut t.end_date);
                coalesce_i32(patch, "duration_days", &mut t.duration_days);
                coalesce_i32(patch, "progress", &mut t.progress);
                coalesce_i32(patch, "position", &mut t.position);
                coalesce_str(patch, "wbs", &mut t.wbs);
                coalesce_opt_f64(patch, "estimated_hours", &mut t.estimated_hours);
                coalesce_opt_f64(patch, "spent_hours", &mut t.spent_hours);
                tri_str(patch, "version_id", &mut t.version_id);
                coalesce_str(patch, "constraint_type", &mut t.constraint_type);
                tri_str(patch, "constraint_date", &mut t.constraint_date);
                tri_str(patch, "deadline_date", &mut t.deadline_date);
                tri_f64(patch, "budget_cost", &mut t.budget_cost);
                tri_str(patch, "parent_id", &mut t.parent_id);
                t.updated_at = now.to_string();
                data.sort_tasks();
                wbs::renumber(&mut data.tasks);
            }
            Change::DeleteTask { id } => {
                let gone = subtree(data, id);
                data.tasks.retain(|t| !gone.contains(&t.id));
                data.dependencies.retain(|d| !gone.contains(&d.from_task_id) && !gone.contains(&d.to_task_id));
                data.assignments.retain(|a| !gone.contains(&a.task_id));
                wbs::renumber(&mut data.tasks);
            }
            Change::CreateDependency { id, from, to, dep_type, lag_days } => {
                let ty = dep_type.clone().unwrap_or_else(|| "FS".into());
                let lag = lag_days.unwrap_or(0);
                match data.dependencies.iter_mut().find(|d| d.from_task_id == *from && d.to_task_id == *to) {
                    Some(d) => {
                        d.dep_type = ty;
                        d.lag_days = lag;
                    }
                    None => data.dependencies.push(Dependency { id: id.clone(), project_id: data.project.id.clone(), from_task_id: from.clone(), to_task_id: to.clone(), dep_type: ty, lag_days: lag, rest: Map::new() }),
                }
            }
            Change::DeleteDependency { id } => data.dependencies.retain(|d| d.id != *id),
            Change::Assign { id, task, resource, units } => {
                // `ON CONFLICT (task_id, resource_id) DO UPDATE SET units`.
                let u = units.unwrap_or(1.0);
                match data.assignments.iter_mut().find(|a| a.task_id == *task && a.resource_id == *resource) {
                    Some(a) => a.units = u,
                    None => data.assignments.push(Assignment { id: id.clone(), task_id: task.clone(), resource_id: resource.clone(), units: u, rest: Map::new() }),
                }
            }
            Change::Unassign { task, resource } => data.assignments.retain(|a| !(a.task_id == *task && a.resource_id == *resource)),
            Change::CreateResource { id, body } => {
                let s = |k: &str| body.get(k).and_then(Value::as_str).map(str::to_string);
                let f = |k: &str| body.get(k).and_then(Value::as_f64);
                data.resources.push(Resource {
                    id: id.clone(),
                    project_id: data.project.id.clone(),
                    name: s("name").unwrap_or_default(),
                    role: s("role").unwrap_or_default(),
                    color: s("color").unwrap_or_else(|| "#5f6368".into()),
                    capacity: f("capacity").unwrap_or(1.0),
                    hourly_rate: f("hourly_rate"),
                    user_id: s("user_id"),
                    avatar_url: None,
                    kind: s("kind").unwrap_or_else(|| "person".into()),
                    unit_label: s("unit_label"),
                    overtime_rate: f("overtime_rate"),
                    cost_per_use: f("cost_per_use"),
                    skills: body.get("skills").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default(),
                    created_at: now.to_string(),
                    rest: Map::new(),
                });
            }
            Change::UpdateResource { id, patch } => {
                let Some(r) = data.resources.iter_mut().find(|r| r.id == *id) else { return Err(Refused::NotFound) };
                coalesce_str(patch, "name", &mut r.name);
                coalesce_str(patch, "role", &mut r.role);
                coalesce_str(patch, "color", &mut r.color);
                coalesce_str(patch, "kind", &mut r.kind);
                if let Some(c) = patch.get("capacity").and_then(Value::as_f64) {
                    r.capacity = c;
                }
                tri_f64(patch, "hourly_rate", &mut r.hourly_rate);
                tri_f64(patch, "overtime_rate", &mut r.overtime_rate);
                tri_f64(patch, "cost_per_use", &mut r.cost_per_use);
                tri_str(patch, "unit_label", &mut r.unit_label);
                if let Some(a) = patch.get("skills").and_then(Value::as_array) {
                    r.skills = a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect();
                }
            }
            Change::DeleteResource { id } => {
                data.resources.retain(|r| r.id != *id);
                data.assignments.retain(|a| a.resource_id != *id);
            }
            Change::ComputeCpm => schedule_local(data, ctx)?,
            Change::CaptureBaseline { .. } | Change::UpdateBaseline { .. } | Change::DeleteBaseline { .. } => {}
        }
        Ok(())
    }
}

/// Runs the scheduler on `data` the way `POST /cpm` does (`cpm::schedule` + the `UPDATE`).
pub fn schedule_local(data: &mut ProjectData, ctx: &ScheduleContext) -> Result<(), Refused> {
    let start = data.project_start(ctx.today);
    let cal = ctx.calendar.clone().unwrap_or_else(|| WorkingCalendar::default_week(ctx.include_weekends_default, start));
    let s = cpm::schedule(&data.tasks, &data.dependencies, start, &cal).map_err(|e| Refused::Schedule(e.to_string()))?;
    cpm::apply(&mut data.tasks, &s);
    Ok(())
}

/// `HH:MM:SS` UTC ISO stamp of a Unix time in milliseconds (what a created row carries until the server's).
pub fn iso_from_ms(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    let d = Date::from_days(days);
    format!("{}T{:02}:{:02}:{:02}.{:03}Z", d.iso(), rem / 3_600_000, (rem / 60_000) % 60, (rem / 1000) % 60, rem % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ScheduleContext {
        ScheduleContext { today: Date::new(2026, 9, 7).expect("d"), calendar: None, include_weekends_default: false }
    }

    fn data() -> ProjectData {
        let mut d = ProjectData::default();
        d.project.id = "p".into();
        d.project.start_date = Some("2026-09-07".into());
        for (i, id) in ["a", "b", "c"].iter().enumerate() {
            let mut t = Task::new(*id, "p");
            t.position = i as i32;
            t.created_at = format!("2026-01-01T00:00:0{i}Z");
            d.tasks.push(t);
        }
        wbs::renumber(&mut d.tasks);
        d
    }

    #[test]
    fn requests_name_the_rows() {
        let c = Change::UpdateTask { id: "t 1".into(), patch: Map::new() };
        assert_eq!(c.request("p").path, "/projects/p/tasks/t%201");
        let dep = Change::CreateDependency { id: "local-1".into(), from: "a".into(), to: "b".into(), dep_type: None, lag_days: Some(2) };
        assert_eq!(dep.request("p").body, Some(json!({"from_task_id": "a", "to_task_id": "b", "lag_days": 2})));
        // The dependency's own (temporary) id is not part of its request: it can be sent.
        assert!(!dep.names_temp());
        let mut a = Change::Assign { id: "local-9".into(), task: "local-2".into(), resource: "r".into(), units: None };
        assert!(a.names_temp());
        a.rename_id("local-2", "t2");
        assert_eq!(a.request("p").path, "/projects/p/tasks/t2/assign");
    }

    #[test]
    fn create_update_delete_like_the_server() {
        let mut d = data();
        let mut body = Map::new();
        body.insert("parent_id".into(), json!("a"));
        Change::CreateTask { id: "local-1".into(), body }.apply(&mut d, &ctx(), "2026-01-02T00:00:00Z").expect("created");
        assert_eq!(d.task("local-1").map(|t| (t.wbs.clone(), t.name.clone())), Some(("1.1".into(), "Nouvelle tâche".into())));
        // null keeps a COALESCE field, clears a tri-state one.
        let mut patch = Map::new();
        patch.insert("name".into(), Value::Null);
        patch.insert("parent_id".into(), Value::Null);
        Change::UpdateTask { id: "local-1".into(), patch }.apply(&mut d, &ctx(), "x").expect("updated");
        assert_eq!(d.task("local-1").map(|t| t.name.as_str()), Some("Nouvelle tâche"));
        assert_eq!(d.task("local-1").and_then(|t| t.parent_id.clone()), None);
        // A deletion cascades to the subtree, its links and assignments.
        let mut p = Map::new();
        p.insert("parent_id".into(), json!("a"));
        Change::UpdateTask { id: "b".into(), patch: p }.apply(&mut d, &ctx(), "x").expect("indented");
        Change::CreateDependency { id: "local-2".into(), from: "b".into(), to: "c".into(), dep_type: None, lag_days: None }.apply(&mut d, &ctx(), "x").expect("linked");
        Change::DeleteTask { id: "a".into() }.apply(&mut d, &ctx(), "x").expect("deleted");
        // local-1 was created first under a (position 0 among its siblings), then moved to the root.
        assert_eq!(d.tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["local-1", "c"]);
        assert!(d.dependencies.is_empty());
    }

    #[test]
    fn refusals_match_the_handlers() {
        let mut d = data();
        let self_dep = Change::CreateDependency { id: "x".into(), from: "a".into(), to: "a".into(), dep_type: None, lag_days: None };
        assert_eq!(self_dep.apply(&mut d, &ctx(), "x"), Err(Refused::SelfDependency));
        Change::CreateDependency { id: "d1".into(), from: "a".into(), to: "b".into(), dep_type: None, lag_days: None }.apply(&mut d, &ctx(), "x").expect("ok");
        let back = Change::CreateDependency { id: "d2".into(), from: "b".into(), to: "a".into(), dep_type: None, lag_days: None };
        assert_eq!(back.apply(&mut d, &ctx(), "x"), Err(Refused::Loop));
        // The upsert keeps the existing row's id.
        Change::CreateDependency { id: "d3".into(), from: "a".into(), to: "b".into(), dep_type: Some("SS".into()), lag_days: Some(1) }.apply(&mut d, &ctx(), "x").expect("ok");
        assert_eq!(d.dependencies.len(), 1);
        assert_eq!(d.dependencies[0].id, "d1");
        let mut p = Map::new();
        p.insert("constraint_type".into(), json!("SNET"));
        assert!(matches!(Change::UpdateTask { id: "a".into(), patch: p }.apply(&mut d, &ctx(), "x"), Err(Refused::Constraint(_))));
    }

    #[test]
    fn position_zero_appends() {
        let mut d = data();
        let mut body = Map::new();
        body.insert("position".into(), json!(0));
        Change::CreateTask { id: "local-1".into(), body }.apply(&mut d, &ctx(), "2026-02-01T00:00:00Z").expect("created");
        assert_eq!(d.task("local-1").map(|t| t.position), Some(3));
    }

    #[test]
    fn stamps() {
        assert_eq!(iso_from_ms(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_from_ms(86_400_000 + 3_723_004), "1970-01-02T01:02:03.004Z");
    }
}
