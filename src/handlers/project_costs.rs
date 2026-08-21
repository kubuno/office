//! Earned value: what was planned to be done by now, what has actually been done
//! valued at its budget, and what it cost.
//!
//! The reason for keeping the three apart is that spend against budget says
//! nothing on its own — a project that has burnt half its money may have done a
//! tenth of the work. Only the earned value tells the two apart.
//!
//! Everything is measured on **leaf tasks**. A summary task's budget would double
//! count its children, so summaries are rolled up rather than read.
use axum::{extract::{Path, State}, Extension, Json};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    handlers::project_authz::{require_permission, task_in_project, Level},
    middleware::OfficeUser,
    state::AppState,
};

const CATEGORIES: [&str; 6] = ["labour", "subcontract", "licence", "hardware", "travel", "other"];
const EAC_METHODS: [&str; 4] = ["cpi", "budget", "cpi_spi", "manual"];

fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where T: Deserialize<'de>, D: Deserializer<'de> {
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct CostConfig {
    project_id:          Uuid,
    currency:            String,
    default_hourly_rate: Option<f64>,
    status_date:         Option<NaiveDate>,
    eac_method:          String,
    manual_etc:          Option<f64>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct CostEntry {
    id:          Uuid,
    project_id:  Uuid,
    task_id:     Option<Uuid>,
    task_name:   Option<String>,
    incurred_on: NaiveDate,
    amount:      f64,
    category:    String,
    description: String,
    coq_category: Option<String>,
    created_at:  chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ConfigDto {
    pub currency:   Option<String>,
    pub eac_method: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub default_hourly_rate: Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub status_date: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub manual_etc:  Option<Option<f64>>,
}

#[derive(Debug, Deserialize)]
pub struct EntryDto {
    pub amount:      Option<f64>,
    pub category:    Option<String>,
    pub description: Option<String>,
    pub incurred_on: Option<NaiveDate>,
    /// Cost of quality. Optional, and clearable: most expenses are neither spent
    /// to prevent failure nor caused by one, and forcing a choice would make the
    /// split meaningless.
    #[serde(default, deserialize_with = "double_option")]
    pub coq_category: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub task_id:     Option<Option<Uuid>>,
}

/// One work package, measured.
struct Measured {
    id:            Uuid,
    wbs:           String,
    name:          String,
    bac:           Option<f64>,
    progress:      i32,
    /// Planned start and finish, as offsets in days from the project start.
    plan_start:    Option<i32>,
    plan_finish:   Option<i32>,
    labour_cost:   f64,
    direct_cost:   f64,
}

impl Measured {
    fn ev(&self) -> f64 { self.bac.unwrap_or(0.0) * f64::from(self.progress) / 100.0 }
    fn ac(&self) -> f64 { self.labour_cost + self.direct_cost }

    /// The share of this package that was supposed to be done by the status date.
    ///
    /// Spread evenly across the planned duration: without a spend curve per task,
    /// a straight line is the honest assumption, and it is what every tool does
    /// by default. A milestone (no duration) is worth nothing until its date and
    /// everything after it.
    fn planned_fraction(&self, status_offset: i32) -> f64 {
        let (Some(start), Some(finish)) = (self.plan_start, self.plan_finish) else { return 0.0 };
        // A point event — a milestone has no duration, so start and finish are the
        // same offset. It is worth nothing before its date and all of it on the
        // day, which the proportional branch below could never express.
        if finish <= start { return if status_offset >= finish { 1.0 } else { 0.0 }; }
        if status_offset <= start { return 0.0; }
        if status_offset >= finish { return 1.0; }
        f64::from(status_offset - start) / f64::from(finish - start)
    }

    fn pv(&self, status_offset: i32) -> f64 {
        self.bac.unwrap_or(0.0) * self.planned_fraction(status_offset)
    }
}

async fn ensure_config(state: &AppState, project_id: Uuid) -> Result<CostConfig> {
    if let Some(c) = sqlx::query_as::<_, CostConfig>(
        "SELECT project_id, currency, default_hourly_rate, status_date, eac_method, manual_etc \
         FROM pm_cost_config WHERE project_id = $1",
    ).bind(project_id).fetch_optional(&state.db).await? {
        return Ok(c);
    }
    // A new project takes the instance's currency, set in the admin console.
    // Existing ones keep the one they were created with: a change of policy must
    // not silently restate amounts already entered.
    let currency = state.instance().project_default_currency;
    Ok(sqlx::query_as::<_, CostConfig>(
        "INSERT INTO pm_cost_config (project_id, currency) VALUES ($1, $2) \
         ON CONFLICT (project_id) DO UPDATE SET project_id = EXCLUDED.project_id \
         RETURNING project_id, currency, default_hourly_rate, status_date, eac_method, manual_etc",
    ).bind(project_id).bind(&currency).fetch_one(&state.db).await?)
}

/// Gather every number the measurement needs, in four queries.
async fn measure(state: &AppState, project_id: Uuid, cfg: &CostConfig)
    -> Result<(Vec<Measured>, NaiveDate, i32, NaiveDate)>
{
    let project_start: Option<NaiveDate> = sqlx::query_scalar(
        "SELECT start_date FROM projects WHERE id = $1",
    ).bind(project_id).fetch_one(&state.db).await?;
    // A project with no start date is measured from today, which makes every
    // planned value zero — correct, and visibly so.
    let origin = project_start.unwrap_or_else(|| Utc::now().date_naive());
    // Everything below is "as of" this day: the earned value, the cost, the
    // coverage. Measuring one of them at a different moment than the others is
    // how an index ends up flattering a project.
    let status = cfg.status_date.unwrap_or_else(|| Utc::now().date_naive());
    let status_offset = (status - origin).num_days() as i32;

    let rows = sqlx::query_as::<_, (Uuid, Option<Uuid>, String, String, Option<f64>, i32,
                                    Option<i32>, Option<i32>, Option<NaiveDate>, Option<NaiveDate>, i32)>(
        "SELECT id, parent_id, wbs, name, budget_cost, progress, \
                early_start, early_finish, start_date, end_date, duration_days \
         FROM tasks WHERE project_id = $1 ORDER BY wbs",
    ).bind(project_id).fetch_all(&state.db).await?;

    // Only leaves carry work; a summary's budget would count its children twice.
    let parents: HashSet<Uuid> = rows.iter().filter_map(|r| r.1).collect();

    // Labour: hours already logged, valued at the rate of the resources assigned
    // to the task, falling back to the project rate. One query, not one per task.
    let hours = sqlx::query_as::<_, (Uuid, f64)>(
        "SELECT task_id, COALESCE(SUM(hours), 0) FROM time_entries \
         WHERE project_id = $1 AND spent_on <= $2 GROUP BY task_id",
    ).bind(project_id).bind(status).fetch_all(&state.db).await?;
    let hours: HashMap<Uuid, f64> = hours.into_iter().collect();

    let rates = sqlx::query_as::<_, (Uuid, Option<f64>)>(
        "SELECT a.task_id, AVG(r.hourly_rate) \
         FROM task_assignments a JOIN project_resources r ON r.id = a.resource_id \
         WHERE r.project_id = $1 GROUP BY a.task_id",
    ).bind(project_id).fetch_all(&state.db).await?;
    let rates: HashMap<Uuid, Option<f64>> = rates.into_iter().collect();

    let direct = sqlx::query_as::<_, (Option<Uuid>, f64)>(
        "SELECT task_id, COALESCE(SUM(amount), 0) FROM pm_cost_entry \
         WHERE project_id = $1 AND incurred_on <= $2 GROUP BY task_id",
    ).bind(project_id).bind(status).fetch_all(&state.db).await?;
    let mut unassigned_direct = 0.0;
    let mut direct_by_task: HashMap<Uuid, f64> = HashMap::new();
    for (task_id, amount) in direct {
        match task_id {
            Some(t) => { direct_by_task.insert(t, amount); }
            // Costs charged to no package still count against the project.
            None => unassigned_direct += amount,
        }
    }

    let mut measured: Vec<Measured> = rows.iter()
        .filter(|r| !parents.contains(&r.0))
        .map(|(id, _parent, wbs, name, bac, progress, es, ef, sd, ed, duration)| {
            // Prefer the computed schedule; fall back to the dates entered by
            // hand when the plan has never been scheduled.
            let (plan_start, plan_finish) = match (es, ef) {
                (Some(s), Some(f)) => (Some(*s), Some(*f)),
                _ => (
                    sd.map(|d| (d - origin).num_days() as i32),
                    ed.map(|d| (d - origin).num_days() as i32)
                      .or_else(|| sd.map(|d| (d - origin).num_days() as i32 + *duration)),
                ),
            };
            let rate = rates.get(id).copied().flatten().or(cfg.default_hourly_rate);
            let labour = hours.get(id).copied().unwrap_or(0.0) * rate.unwrap_or(0.0);
            Measured {
                id: *id, wbs: wbs.clone(), name: name.clone(), bac: *bac, progress: *progress,
                plan_start, plan_finish,
                labour_cost: labour,
                direct_cost: direct_by_task.get(id).copied().unwrap_or(0.0),
            }
        })
        .collect();

    // Project-level costs are attributed to nothing in particular, but they are
    // spent all the same: carried on a synthetic line so the totals are true.
    if unassigned_direct.abs() > f64::EPSILON {
        measured.push(Measured {
            id: Uuid::nil(), wbs: String::new(), name: String::new(),
            bac: None, progress: 0, plan_start: None, plan_finish: None,
            labour_cost: 0.0, direct_cost: unassigned_direct,
        });
    }
    Ok((measured, status, status_offset, origin))
}

fn round2(v: f64) -> f64 {
    let r = (v * 100.0).round() / 100.0;
    // Summing an empty set of amounts yields -0.0, which serialises as "-0" and
    // reads as a (tiny) negative number where there is simply nothing.
    if r == 0.0 { 0.0 } else { r }
}

/// GET /projects/:id/costs — the measurement.
pub async fn overview(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let cfg = ensure_config(&state, project_id).await?;
    let (tasks, status, status_offset, origin) = measure(&state, project_id, &cfg).await?;

    let bac: f64 = tasks.iter().map(|t| t.bac.unwrap_or(0.0)).sum();
    let ev:  f64 = tasks.iter().map(Measured::ev).sum();
    let ac:  f64 = tasks.iter().map(Measured::ac).sum();
    let pv:  f64 = tasks.iter().map(|t| t.pv(status_offset)).sum();

    // An index divides: with no denominator there is no ratio, and reporting one
    // anyway (1.0, or 0) would read as "on track".
    let cpi = if ac > 0.0 { Some(ev / ac) } else { None };
    let spi = if pv > 0.0 { Some(ev / pv) } else { None };

    let eac = match cfg.eac_method.as_str() {
        // Current cost performance continues to the end.
        "cpi" => cpi.filter(|c| *c > 0.0).map(|c| bac / c),
        // The work left goes exactly to plan.
        "budget" => Some(ac + (bac - ev)),
        // Both the cost and the schedule performance continue.
        "cpi_spi" => match (cpi, spi) {
            (Some(c), Some(s)) if c * s > 0.0 => Some(ac + (bac - ev) / (c * s)),
            _ => None,
        },
        // Somebody re-estimated the remaining work; no index beats that.
        "manual" => cfg.manual_etc.map(|etc| ac + etc),
        _ => None,
    };
    let etc = eac.map(|e| e - ac);
    let vac = eac.map(|e| bac - e);
    // What performance the rest of the work must achieve to still land on budget.
    // Above 1 means it has to go better than it ever has.
    let tcpi = if (bac - ac).abs() > f64::EPSILON { Some((bac - ev) / (bac - ac)) } else { None };

    let costed = tasks.iter().filter(|t| t.bac.is_some()).count();
    let leaves = tasks.iter().filter(|t| !t.wbs.is_empty()).count();
    let labour: f64 = tasks.iter().map(|t| t.labour_cost).sum();
    let logged_hours: f64 = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT SUM(hours) FROM time_entries WHERE project_id = $1 AND spent_on <= $2",
    ).bind(project_id).bind(status).fetch_one(&state.db).await?.unwrap_or(0.0);

    let breakdown: Vec<Value> = tasks.iter().filter(|t| !t.wbs.is_empty()).map(|t| {
        let (ev_t, ac_t, pv_t) = (t.ev(), t.ac(), t.pv(status_offset));
        json!({
            "task_id": t.id, "wbs": t.wbs, "name": t.name,
            "bac": t.bac, "progress": t.progress,
            "pv": round2(pv_t), "ev": round2(ev_t), "ac": round2(ac_t),
            "cv": round2(ev_t - ac_t), "sv": round2(ev_t - pv_t),
            "cpi": if ac_t > 0.0 { Some(round2(ev_t / ac_t)) } else { None },
            "spi": if pv_t > 0.0 { Some(round2(ev_t / pv_t)) } else { None },
        })
    }).collect();

    Ok(Json(json!({
        "config": cfg,
        "status_date": status,
        // The curve counts days from here. Without it the x axis cannot be dated,
        // and a caller would have to guess the origin from the data.
        "origin": origin,
        "status_offset": status_offset,
        "totals": {
            "bac": round2(bac), "pv": round2(pv), "ev": round2(ev), "ac": round2(ac),
            "cv": round2(ev - ac), "sv": round2(ev - pv),
            "cpi": cpi.map(round2), "spi": spi.map(round2),
            "eac": eac.map(round2), "etc": etc.map(round2),
            "vac": vac.map(round2), "tcpi": tcpi.map(round2),
        },
        // How much of the plan can be measured at all. An index computed over a
        // third of the work is not a project-level statement, and saying so is
        // more useful than a confident wrong number.
        "coverage": {
            "costed_tasks": costed, "leaf_tasks": leaves,
            // Without logged hours the actual cost is direct spend only, and every
            // index reads far better than the project is doing. The interface has
            // to be able to say so rather than publish a flattering ratio.
            "logged_hours": round2(logged_hours),
            "labour_cost": round2(labour),
            "direct_cost": round2(ac - labour),
            "has_rate": cfg.default_hourly_rate.is_some(),
        },
        "tasks": breakdown,
        "curve": curve(&tasks, status_offset, &spend_by_day(&state, project_id, &cfg, origin).await?),
    })))
}

/// Every euro the project spent, with the day it was spent on: hours worked
/// valued at the rate that applies to their task, plus the dated expenses. This
/// is what makes the actual-cost line a record rather than a straight line drawn
/// between two points.
async fn spend_by_day(state: &AppState, project_id: Uuid, cfg: &CostConfig, origin: NaiveDate)
    -> Result<Vec<(i32, f64)>>
{
    let mut out: Vec<(i32, f64)> = Vec::new();

    let hours = sqlx::query_as::<_, (NaiveDate, f64, Option<f64>)>(
        "SELECT e.spent_on, e.hours, \
                (SELECT AVG(r.hourly_rate) FROM task_assignments a \
                   JOIN project_resources r ON r.id = a.resource_id \
                  WHERE a.task_id = e.task_id AND r.project_id = $1) \
         FROM time_entries e WHERE e.project_id = $1",
    ).bind(project_id).fetch_all(&state.db).await?;
    for (day, h, rate) in hours {
        let rate = rate.or(cfg.default_hourly_rate).unwrap_or(0.0);
        out.push(((day - origin).num_days() as i32, h * rate));
    }

    let expenses = sqlx::query_as::<_, (NaiveDate, f64)>(
        "SELECT incurred_on, amount FROM pm_cost_entry WHERE project_id = $1",
    ).bind(project_id).fetch_all(&state.db).await?;
    for (day, amount) in expenses {
        out.push(((day - origin).num_days() as i32, amount));
    }
    Ok(out)
}

/// The three lines, and what is honestly known of each.
///
/// Planned value runs to the end of the plan: it is a plan, so it is known
/// throughout. Actual cost is built from the dates money was actually spent —
/// expenses carry the day they were incurred and hours the day they were worked,
/// so its history is real. Earned value has no history at all: progress is a
/// single number per task, recorded now with no trace of what it was last month.
/// It is therefore reported only at the status date, as one point. Drawing a line
/// through it would be inventing a past nobody recorded.
fn curve(tasks: &[Measured], status_offset: i32, spend: &[(i32, f64)]) -> Vec<Value> {
    let horizon = tasks.iter().filter_map(|t| t.plan_finish).max().unwrap_or(status_offset)
        .max(status_offset);
    if horizon < 0 { return Vec::new(); }

    // Weekly, plus the status date itself: sampling only every seventh day would
    // stop the earned-value line up to six days short of the marker that claims
    // to date it.
    let mut offsets: Vec<i32> = (0..=horizon).step_by(7).collect();
    if !offsets.contains(&horizon) { offsets.push(horizon); }
    if status_offset >= 0 && status_offset <= horizon && !offsets.contains(&status_offset) {
        offsets.push(status_offset);
    }
    offsets.sort_unstable();

    let mut points = Vec::new();
    for offset in offsets {
        let pv: f64 = tasks.iter().map(|t| t.pv(offset)).sum();
        // Everything spent on or before this day.
        let ac: Option<f64> = if offset <= status_offset {
            Some(spend.iter().filter(|(d, _)| *d <= offset).map(|(_, a)| a).sum())
        } else {
            None
        };
        let ev = if offset == status_offset {
            Some(round2(tasks.iter().map(Measured::ev).sum::<f64>()))
        } else {
            None
        };
        points.push(json!({
            "offset": offset,
            "pv": round2(pv),
            "ev": ev,
            "ac": ac.map(round2),
        }));
    }
    points
}

/// PUT /projects/:id/costs/config
pub async fn update_config(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<ConfigDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    ensure_config(&state, project_id).await?;

    if let Some(m) = dto.eac_method.as_deref() {
        if !EAC_METHODS.contains(&m.trim()) {
            return Err(OfficeError::Validation(format!(
                "Méthode d'estimation « {m} » inconnue. Valeurs admises : {}.",
                EAC_METHODS.join(", ")
            )));
        }
    }
    let cfg = sqlx::query_as::<_, CostConfig>(
        "UPDATE pm_cost_config SET \
            currency = COALESCE($2, currency), \
            eac_method = COALESCE($3, eac_method), \
            default_hourly_rate = CASE WHEN $4::boolean THEN $5::double precision ELSE default_hourly_rate END, \
            status_date = CASE WHEN $6::boolean THEN $7::date ELSE status_date END, \
            manual_etc = CASE WHEN $8::boolean THEN $9::double precision ELSE manual_etc END, \
            updated_at = now() \
         WHERE project_id = $1 \
         RETURNING project_id, currency, default_hourly_rate, status_date, eac_method, manual_etc",
    )
    .bind(project_id)
    .bind(dto.currency.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.eac_method.as_deref().map(str::trim))
    .bind(dto.default_hourly_rate.is_some()).bind(dto.default_hourly_rate.flatten())
    .bind(dto.status_date.is_some()).bind(dto.status_date.flatten())
    .bind(dto.manual_etc.is_some()).bind(dto.manual_etc.flatten())
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "config": cfg })))
}

const ENTRY_COLS: &str = "e.id, e.project_id, e.task_id, t.name AS task_name, e.incurred_on, \
     e.amount, e.category, e.description, e.coq_category, e.created_at";

/// GET /projects/:id/costs/entries
pub async fn list_entries(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let entries = sqlx::query_as::<_, CostEntry>(&format!(
        "SELECT {ENTRY_COLS} FROM pm_cost_entry e \
         LEFT JOIN tasks t ON t.id = e.task_id AND t.project_id = e.project_id \
         WHERE e.project_id = $1 ORDER BY e.incurred_on DESC, e.created_at DESC"
    )).bind(project_id).fetch_all(&state.db).await?;
    let total: f64 = entries.iter().map(|e| e.amount).sum();
    Ok(Json(json!({ "entries": entries, "total": round2(total) })))
}

/// Cost-of-quality classification, validated against the list the quality handler
/// owns rather than a second copy of it.
fn check_coq(value: Option<&str>) -> Result<Option<String>> {
    let Some(v) = value.map(str::trim) else { return Ok(None) };
    if v.is_empty() { return Ok(None); }
    if !crate::handlers::project_quality::coq_categories().contains(&v) {
        return Err(OfficeError::Validation(format!(
            "Coût de la qualité : valeur « {v} » refusée. Valeurs admises : {}.",
            crate::handlers::project_quality::coq_categories().join(", ")
        )));
    }
    Ok(Some(v.to_string()))
}

fn check_category(category: &str) -> Result<String> {
    let c = category.trim();
    if !CATEGORIES.contains(&c) {
        return Err(OfficeError::Validation(format!(
            "Catégorie « {c} » refusée. Valeurs admises : {}.", CATEGORIES.join(", ")
        )));
    }
    Ok(c.to_string())
}

/// POST /projects/:id/costs/entries
pub async fn create_entry(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<EntryDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let amount = dto.amount
        .ok_or_else(|| OfficeError::Validation("Indiquez le montant de la dépense.".into()))?;
    let category = match dto.category.as_deref() { Some(c) => check_category(c)?, None => "other".into() };
    if let Some(tid) = dto.task_id.flatten() {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }
    let coq = check_coq(dto.coq_category.as_ref().and_then(|c| c.as_deref()))?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO pm_cost_entry (project_id, task_id, incurred_on, amount, category, \
             description, coq_category, created_by) \
         VALUES ($1, $2, COALESCE($3, CURRENT_DATE), $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(project_id).bind(dto.task_id.flatten()).bind(dto.incurred_on)
    .bind(amount).bind(&category)
    .bind(dto.description.as_deref().map(str::trim).unwrap_or_default())
    .bind(coq.as_deref()).bind(user.id)
    .fetch_one(&state.db).await?;
    Ok(Json(json!({ "entry": fetch_entry(&state, project_id, id).await? })))
}

async fn fetch_entry(state: &AppState, project_id: Uuid, id: Uuid) -> Result<CostEntry> {
    sqlx::query_as::<_, CostEntry>(&format!(
        "SELECT {ENTRY_COLS} FROM pm_cost_entry e \
         LEFT JOIN tasks t ON t.id = e.task_id AND t.project_id = e.project_id \
         WHERE e.id = $1 AND e.project_id = $2"
    )).bind(id).bind(project_id).fetch_optional(&state.db).await?
      .ok_or_else(|| OfficeError::NotFound("Dépense introuvable".into()))
}

/// PATCH /projects/:id/costs/entries/:eid
pub async fn update_entry(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, entry_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<EntryDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let category = match dto.category.as_deref() { Some(c) => Some(check_category(c)?), None => None };
    let coq = check_coq(dto.coq_category.as_ref().and_then(|c| c.as_deref()))?;
    if let Some(Some(tid)) = dto.task_id {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }
    let rows = sqlx::query(
        "UPDATE pm_cost_entry SET \
            amount = COALESCE($3, amount), \
            category = COALESCE($4, category), \
            description = COALESCE($5, description), \
            incurred_on = COALESCE($6, incurred_on), \
            task_id = CASE WHEN $7::boolean THEN $8::uuid ELSE task_id END, \
            coq_category = CASE WHEN $9::boolean THEN $10::varchar ELSE coq_category END, \
            updated_at = now() \
         WHERE id = $1 AND project_id = $2",
    )
    .bind(entry_id).bind(project_id)
    .bind(dto.amount).bind(category.as_deref())
    .bind(dto.description.as_deref().map(str::trim))
    .bind(dto.incurred_on)
    .bind(dto.task_id.is_some()).bind(dto.task_id.flatten())
    .bind(dto.coq_category.is_some()).bind(coq.as_deref())
    .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Dépense introuvable".into())); }
    Ok(Json(json!({ "entry": fetch_entry(&state, project_id, entry_id).await? })))
}

/// DELETE /projects/:id/costs/entries/:eid
pub async fn delete_entry(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, entry_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query("DELETE FROM pm_cost_entry WHERE id = $1 AND project_id = $2")
        .bind(entry_id).bind(project_id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Dépense introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(bac: f64, progress: i32, start: i32, finish: i32) -> Measured {
        Measured {
            id: Uuid::nil(), wbs: "1".into(), name: "t".into(),
            bac: Some(bac), progress, plan_start: Some(start), plan_finish: Some(finish),
            labour_cost: 0.0, direct_cost: 0.0,
        }
    }

    #[test]
    fn planned_value_is_spread_across_the_planned_span() {
        let t = pkg(1000.0, 0, 0, 10);
        assert_eq!(t.pv(0), 0.0);
        assert_eq!(t.pv(5), 500.0);
        assert_eq!(t.pv(10), 1000.0);
        // Past the end it stays whole rather than growing.
        assert_eq!(t.pv(40), 1000.0);
    }

    #[test]
    fn a_milestone_is_worth_nothing_until_its_date() {
        let m = pkg(500.0, 0, 4, 4);
        assert_eq!(m.pv(3), 0.0);
        assert_eq!(m.pv(4), 500.0);
    }

    #[test]
    fn earned_value_follows_progress_not_spend() {
        let mut t = pkg(1000.0, 25, 0, 10);
        t.direct_cost = 800.0;
        assert_eq!(t.ev(), 250.0);
        assert_eq!(t.ac(), 800.0);
        // Three quarters of the money for a quarter of the work.
        assert!(t.ev() / t.ac() < 0.5);
    }

    #[test]
    fn an_uncosted_package_contributes_nothing() {
        let mut t = pkg(0.0, 100, 0, 10);
        t.bac = None;
        assert_eq!(t.ev(), 0.0);
        assert_eq!(t.pv(20), 0.0);
    }
}
