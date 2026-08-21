//! Quality: what "good enough" means in numbers, and the evidence it was met.
//!
//! This is deliberately not a second issue log — defects live there — nor a second
//! acceptance criterion, which a deliverable already carries. What was missing is
//! the measurable side: a target with a tolerance, read again and again, so that
//! "the quality is fine" stops being an opinion.
use axum::{extract::{Path, State}, Extension, Json};
use chrono::NaiveDate;
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    handlers::project_authz::{require_permission, task_in_project, Level},
    middleware::OfficeUser,
    state::AppState,
};

const DIRECTIONS: [&str; 3] = ["higher", "lower", "target"];
const FREQUENCIES: [&str; 7] = ["continuous", "daily", "weekly", "sprint", "monthly", "milestone", "once"];
const RESULTS: [&str; 4] = ["pending", "pass", "fail", "waived"];
const COQ: [&str; 4] = ["prevention", "appraisal", "internal_failure", "external_failure"];

const METRIC_COLS: &str = "id, project_id, code, name, description, method, unit, target, \
     tolerance_min, tolerance_max, direction, frequency, owner_id, deliverable_id, task_id, \
     is_active, position, created_at, updated_at";

#[derive(Debug, sqlx::FromRow, serde::Serialize, Clone)]
pub struct Metric {
    id:             Uuid,
    project_id:     Uuid,
    code:           String,
    name:           String,
    description:    String,
    method:         String,
    unit:           String,
    target:         Option<f64>,
    tolerance_min:  Option<f64>,
    tolerance_max:  Option<f64>,
    direction:      String,
    frequency:      String,
    owner_id:       Option<Uuid>,
    deliverable_id: Option<Uuid>,
    task_id:        Option<Uuid>,
    is_active:      bool,
    position:       i32,
    created_at:     chrono::DateTime<chrono::Utc>,
    updated_at:     chrono::DateTime<chrono::Utc>,
}

fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where T: Deserialize<'de>, D: Deserializer<'de> {
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct MetricDto {
    pub code:        Option<String>,
    pub name:        Option<String>,
    pub description: Option<String>,
    pub method:      Option<String>,
    pub unit:        Option<String>,
    pub direction:   Option<String>,
    pub frequency:   Option<String>,
    pub is_active:   Option<bool>,
    pub position:    Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub target:        Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub tolerance_min: Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub tolerance_max: Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub owner_id:       Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub deliverable_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub task_id:        Option<Option<Uuid>>,
}

#[derive(Debug, Deserialize)]
pub struct MeasurementDto {
    pub value:       Option<f64>,
    pub measured_on: Option<NaiveDate>,
    pub notes:       Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CheckDto {
    pub label:    Option<String>,
    pub result:   Option<String>,
    pub evidence: Option<String>,
    pub position: Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub deliverable_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub task_id:        Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub checked_on:     Option<Option<NaiveDate>>,
}

fn check_one(value: &str, allowed: &[&str], field: &str) -> Result<String> {
    let v = value.trim();
    if !allowed.contains(&v) {
        return Err(OfficeError::Validation(format!(
            "{field} : valeur « {v} » refusée. Valeurs admises : {}.", allowed.join(", ")
        )));
    }
    Ok(v.to_string())
}

/// Does a reading conform?
///
/// Returns `None` when the metric says nothing to compare against — a metric with
/// no target and no tolerance is a number being collected, not a standard being
/// held to, and answering "conforming" would be an invention.
fn conforms(m: &Metric, value: f64) -> Option<bool> {
    match (m.tolerance_min, m.tolerance_max) {
        (Some(lo), Some(hi)) => Some(value >= lo && value <= hi),
        (Some(lo), None) => Some(value >= lo),
        (None, Some(hi)) => Some(value <= hi),
        (None, None) => match (m.target, m.direction.as_str()) {
            (Some(t), "higher") => Some(value >= t),
            (Some(t), "lower")  => Some(value <= t),
            // An exact target with no tolerance can only ever be missed.
            _ => None,
        },
    }
}

/// How far a conforming reading sits from the bound it is closest to, as a share
/// of the tolerance band. A metric drifting steadily towards the edge is the
/// interesting case, and the latest value alone cannot show it.
fn margin(m: &Metric, value: f64) -> Option<f64> {
    match (m.tolerance_min, m.tolerance_max) {
        // A two-sided band: the share of it left to the nearer edge.
        (Some(lo), Some(hi)) if hi > lo => Some(((value - lo).min(hi - value)) / (hi - lo)),
        // One bound only. There is no band to take a share of, so the distance is
        // measured against the span from the target to the bound when a target is
        // set, and against the bound itself otherwise — a metric held to "under
        // 300 ms" is plainly close to it at 297.
        (Some(lo), None) => one_sided(value, lo, m.target),
        (None, Some(hi)) => one_sided(value, hi, m.target),
        _ => None,
    }
}

fn one_sided(value: f64, bound: f64, target: Option<f64>) -> Option<f64> {
    let span = match target {
        Some(t) if (t - bound).abs() > f64::EPSILON => (t - bound).abs(),
        _ if bound.abs() > f64::EPSILON => bound.abs(),
        _ => return None,
    };
    Some(((value - bound).abs() / span).min(1.0))
}

async fn fetch_metrics(state: &AppState, project_id: Uuid) -> Result<Vec<Metric>> {
    Ok(sqlx::query_as::<_, Metric>(
        &format!("SELECT {METRIC_COLS} FROM pm_quality_metric WHERE project_id = $1 \
                  ORDER BY position, created_at"),
    ).bind(project_id).fetch_all(&state.db).await?)
}

/// GET /projects/:id/quality
pub async fn overview(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let metrics = fetch_metrics(&state, project_id).await?;

    // Every reading of every metric, in one query.
    let readings = sqlx::query_as::<_, (Uuid, Uuid, NaiveDate, f64, String)>(
        "SELECT r.id, r.metric_id, r.measured_on, r.value, r.notes \
         FROM pm_quality_measurement r JOIN pm_quality_metric m ON m.id = r.metric_id \
         WHERE m.project_id = $1 ORDER BY r.measured_on, r.created_at",
    ).bind(project_id).fetch_all(&state.db).await?;

    let mut series: std::collections::HashMap<Uuid, Vec<Value>> = std::collections::HashMap::new();
    let mut latest: std::collections::HashMap<Uuid, (NaiveDate, f64)> = std::collections::HashMap::new();
    let mut previous: std::collections::HashMap<Uuid, f64> = std::collections::HashMap::new();
    for (id, metric_id, day, value, notes) in &readings {
        series.entry(*metric_id).or_default().push(json!({
            "id": id, "measured_on": day, "value": value, "notes": notes,
        }));
        if let Some((_, last)) = latest.insert(*metric_id, (*day, *value)) {
            previous.insert(*metric_id, last);
        }
    }

    let (mut conforming, mut breaching, mut drifting) = (0i64, 0i64, 0i64);
    // Never measured is not the same as measured against nothing: one needs a
    // reading, the other needs a bound.
    let (mut unmeasured, mut unrated) = (0i64, 0i64);
    let rows: Vec<Value> = metrics.iter().map(|m| {
        let last = latest.get(&m.id).copied();
        let ok = last.and_then(|(_, v)| conforms(m, v));
        let prev = previous.get(&m.id).copied();
        let band = last.and_then(|(_, v)| margin(m, v));
        // Conforming, but close to the edge and heading that way.
        let at_risk = match (ok, band, last, prev) {
            (Some(true), Some(b), Some((_, v)), Some(p)) if b < 0.15 => match (m.tolerance_min, m.tolerance_max) {
                // Two bounds: drifting away from the middle of the band.
                (Some(lo), Some(hi)) => {
                    let mid = (lo + hi) / 2.0;
                    (v - mid).abs() > (p - mid).abs()
                }
                // One bound: simply moving towards it.
                (Some(bound), None) | (None, Some(bound)) => (v - bound).abs() < (p - bound).abs(),
                _ => false,
            },
            _ => false,
        };
        if m.is_active {
            match ok {
                Some(true) if at_risk => { conforming += 1; drifting += 1 }
                Some(true) => conforming += 1,
                Some(false) => breaching += 1,
                None if last.is_none() => unmeasured += 1,
                None => unrated += 1,
            }
        }
        json!({
            "metric": m,
            "latest": last.map(|(d, v)| json!({ "measured_on": d, "value": v })),
            "previous": prev,
            "conforms": ok,
            "at_risk": at_risk,
            "margin": band,
            "series": series.get(&m.id).cloned().unwrap_or_default(),
        })
    }).collect();

    // The audit trail: checks carried out, and what they found.
    let checks = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT COUNT(*) FILTER (WHERE result = 'pending'), \
                COUNT(*) FILTER (WHERE result = 'pass'), \
                COUNT(*) FILTER (WHERE result = 'fail'), \
                COUNT(*) FILTER (WHERE result = 'waived') \
         FROM pm_quality_check WHERE project_id = $1",
    ).bind(project_id).fetch_one(&state.db).await?;

    // Cost of quality, read off the expenses already recorded rather than entered
    // twice. Prevention and appraisal are what is spent to avoid failure; the two
    // failure lines are what is paid when that did not work.
    let coq = sqlx::query_as::<_, (Option<String>, f64)>(
        "SELECT coq_category, COALESCE(SUM(amount), 0) FROM pm_cost_entry \
         WHERE project_id = $1 GROUP BY coq_category",
    ).bind(project_id).fetch_all(&state.db).await?;
    let mut cost = serde_json::Map::new();
    let (mut conformance, mut failure, mut unclassified) = (0.0, 0.0, 0.0);
    for (category, amount) in coq {
        match category.as_deref() {
            Some(c @ ("prevention" | "appraisal")) => { conformance += amount; cost.insert(c.into(), json!(amount)); }
            Some(c @ ("internal_failure" | "external_failure")) => { failure += amount; cost.insert(c.into(), json!(amount)); }
            _ => unclassified += amount,
        }
    }

    Ok(Json(json!({
        "metrics": rows,
        "summary": {
            "total": metrics.len(),
            "active": metrics.iter().filter(|m| m.is_active).count(),
            "conforming": conforming, "breaching": breaching,
            "unmeasured": unmeasured, "unrated": unrated, "drifting": drifting,
        },
        "checks": { "pending": checks.0, "pass": checks.1, "fail": checks.2, "waived": checks.3 },
        "cost_of_quality": {
            "by_category": cost,
            "conformance": (conformance * 100.0).round() / 100.0,
            "failure": (failure * 100.0).round() / 100.0,
            // Expenses nobody classified. Reported rather than folded into one of
            // the four, so the split is never quietly wrong.
            "unclassified": (unclassified * 100.0).round() / 100.0,
        },
    })))
}

/// POST /projects/:id/quality/metrics
pub async fn create_metric(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<MetricDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let name = dto.name.as_deref().map(str::trim).filter(|s| !s.is_empty())
        .ok_or_else(|| OfficeError::Validation("Nommez l'indicateur.".into()))?.to_string();
    let direction = match dto.direction.as_deref() {
        Some(d) => check_one(d, &DIRECTIONS, "Sens")?, None => "higher".into() };
    let frequency = match dto.frequency.as_deref() {
        Some(f) => check_one(f, &FREQUENCIES, "Fréquence")?, None => "sprint".into() };

    let (lo, hi) = (dto.tolerance_min.flatten(), dto.tolerance_max.flatten());
    if let (Some(lo), Some(hi)) = (lo, hi) {
        if lo > hi {
            return Err(OfficeError::Validation(
                "La borne basse de tolérance dépasse la borne haute.".into()));
        }
    }
    if let Some(tid) = dto.task_id.flatten() {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }
    if let Some(did) = dto.deliverable_id.flatten() {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pm_deliverable WHERE id = $1 AND project_id = $2)",
        ).bind(did).bind(project_id).fetch_one(&state.db).await?;
        if !ok { return Err(OfficeError::NotFound(format!("Livrable {did}"))); }
    }

    let code = match dto.code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None => {
            let highest: Option<i32> = sqlx::query_scalar(
                "SELECT MAX(CAST(substring(code FROM '^Q-([0-9]+)$') AS INT)) \
                 FROM pm_quality_metric WHERE project_id = $1 AND code ~ '^Q-[0-9]+$'",
            ).bind(project_id).fetch_one(&state.db).await?;
            format!("Q-{:02}", highest.unwrap_or(0) + 1)
        }
    };
    let position = match dto.position {
        Some(p) => p,
        None => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(position) FROM pm_quality_metric WHERE project_id = $1",
        ).bind(project_id).fetch_one(&state.db).await?.map_or(0, |m| m + 1),
    };

    let metric = sqlx::query_as::<_, Metric>(&format!(
        "INSERT INTO pm_quality_metric (project_id, code, name, description, method, unit, \
             target, tolerance_min, tolerance_max, direction, frequency, owner_id, \
             deliverable_id, task_id, is_active, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, COALESCE($15, TRUE), $16) \
         RETURNING {METRIC_COLS}"
    ))
    .bind(project_id).bind(&code).bind(&name)
    .bind(dto.description.as_deref().unwrap_or_default())
    .bind(dto.method.as_deref().unwrap_or_default())
    .bind(dto.unit.as_deref().map(str::trim).unwrap_or_default())
    .bind(dto.target.flatten()).bind(lo).bind(hi)
    .bind(&direction).bind(&frequency)
    .bind(dto.owner_id.flatten()).bind(dto.deliverable_id.flatten()).bind(dto.task_id.flatten())
    .bind(dto.is_active).bind(position)
    .fetch_one(&state.db).await?;
    Ok(Json(json!({ "metric": metric })))
}

/// PATCH /projects/:id/quality/metrics/:mid
pub async fn update_metric(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, metric_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<MetricDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let direction = match dto.direction.as_deref() { Some(d) => Some(check_one(d, &DIRECTIONS, "Sens")?), None => None };
    let frequency = match dto.frequency.as_deref() { Some(f) => Some(check_one(f, &FREQUENCIES, "Fréquence")?), None => None };
    if let (Some(Some(lo)), Some(Some(hi))) = (dto.tolerance_min, dto.tolerance_max) {
        if lo > hi {
            return Err(OfficeError::Validation(
                "La borne basse de tolérance dépasse la borne haute.".into()));
        }
    }
    if let Some(Some(tid)) = dto.task_id {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }

    let metric = sqlx::query_as::<_, Metric>(&format!(
        "UPDATE pm_quality_metric SET \
            code = COALESCE($3, code), name = COALESCE($4, name), \
            description = COALESCE($5, description), method = COALESCE($6, method), \
            unit = COALESCE($7, unit), \
            target = CASE WHEN $8::boolean THEN $9::double precision ELSE target END, \
            tolerance_min = CASE WHEN $10::boolean THEN $11::double precision ELSE tolerance_min END, \
            tolerance_max = CASE WHEN $12::boolean THEN $13::double precision ELSE tolerance_max END, \
            direction = COALESCE($14, direction), frequency = COALESCE($15, frequency), \
            owner_id = CASE WHEN $16::boolean THEN $17::uuid ELSE owner_id END, \
            deliverable_id = CASE WHEN $18::boolean THEN $19::uuid ELSE deliverable_id END, \
            task_id = CASE WHEN $20::boolean THEN $21::uuid ELSE task_id END, \
            is_active = COALESCE($22, is_active), position = COALESCE($23, position), \
            updated_at = now() \
         WHERE id = $1 AND project_id = $2 RETURNING {METRIC_COLS}"
    ))
    .bind(metric_id).bind(project_id)
    .bind(dto.code.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.name.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.description.as_deref()).bind(dto.method.as_deref())
    .bind(dto.unit.as_deref().map(str::trim))
    .bind(dto.target.is_some()).bind(dto.target.flatten())
    .bind(dto.tolerance_min.is_some()).bind(dto.tolerance_min.flatten())
    .bind(dto.tolerance_max.is_some()).bind(dto.tolerance_max.flatten())
    .bind(direction.as_deref()).bind(frequency.as_deref())
    .bind(dto.owner_id.is_some()).bind(dto.owner_id.flatten())
    .bind(dto.deliverable_id.is_some()).bind(dto.deliverable_id.flatten())
    .bind(dto.task_id.is_some()).bind(dto.task_id.flatten())
    .bind(dto.is_active).bind(dto.position)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Indicateur introuvable".into()))?;
    Ok(Json(json!({ "metric": metric })))
}

/// DELETE /projects/:id/quality/metrics/:mid
pub async fn delete_metric(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, metric_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query("DELETE FROM pm_quality_metric WHERE id = $1 AND project_id = $2")
        .bind(metric_id).bind(project_id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Indicateur introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}

/// POST /projects/:id/quality/metrics/:mid/measurements
pub async fn add_measurement(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, metric_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<MeasurementDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let value = dto.value.ok_or_else(|| OfficeError::Validation("Indiquez la valeur mesurée.".into()))?;
    let metric = sqlx::query_as::<_, Metric>(
        &format!("SELECT {METRIC_COLS} FROM pm_quality_metric WHERE id = $1 AND project_id = $2"),
    ).bind(metric_id).bind(project_id).fetch_optional(&state.db).await?
     .ok_or_else(|| OfficeError::NotFound("Indicateur introuvable".into()))?;

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO pm_quality_measurement (metric_id, measured_on, value, notes, recorded_by) \
         VALUES ($1, COALESCE($2, CURRENT_DATE), $3, $4, $5) RETURNING id",
    )
    .bind(metric_id).bind(dto.measured_on).bind(value)
    .bind(dto.notes.as_deref().map(str::trim).unwrap_or_default()).bind(user.id)
    .fetch_one(&state.db).await?;

    Ok(Json(json!({
        "id": id, "value": value,
        // Answered here so the caller does not have to re-implement the rule.
        "conforms": conforms(&metric, value),
    })))
}

/// DELETE /projects/:id/quality/metrics/:mid/measurements/:rid
pub async fn delete_measurement(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, metric_id, reading_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query(
        "DELETE FROM pm_quality_measurement r USING pm_quality_metric m \
         WHERE r.id = $1 AND r.metric_id = $2 AND m.id = r.metric_id AND m.project_id = $3",
    ).bind(reading_id).bind(metric_id).bind(project_id)
     .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Mesure introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}

const CHECK_COLS: &str = "c.id, c.project_id, c.deliverable_id, c.task_id, c.label, c.result, \
     c.evidence, c.checked_on, c.checked_by, c.issue_id, c.position, c.created_at, c.updated_at, \
     d.name AS deliverable_name, t.name AS task_name";

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct QualityCheck {
    id:               Uuid,
    project_id:       Uuid,
    deliverable_id:   Option<Uuid>,
    deliverable_name: Option<String>,
    task_id:          Option<Uuid>,
    task_name:        Option<String>,
    label:            String,
    result:           String,
    evidence:         String,
    checked_on:       Option<NaiveDate>,
    checked_by:       Option<Uuid>,
    issue_id:         Option<Uuid>,
    position:         i32,
    created_at:       chrono::DateTime<chrono::Utc>,
    updated_at:       chrono::DateTime<chrono::Utc>,
}

/// GET /projects/:id/quality/checks
pub async fn list_checks(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let checks = sqlx::query_as::<_, QualityCheck>(&format!(
        "SELECT {CHECK_COLS} FROM pm_quality_check c \
         LEFT JOIN pm_deliverable d ON d.id = c.deliverable_id AND d.project_id = c.project_id \
         LEFT JOIN tasks t ON t.id = c.task_id AND t.project_id = c.project_id \
         WHERE c.project_id = $1 \
         ORDER BY (c.result = 'fail') DESC, (c.result = 'pending') DESC, c.position, c.created_at"
    )).bind(project_id).fetch_all(&state.db).await?;
    Ok(Json(json!({ "checks": checks })))
}

async fn fetch_check(state: &AppState, project_id: Uuid, id: Uuid) -> Result<QualityCheck> {
    sqlx::query_as::<_, QualityCheck>(&format!(
        "SELECT {CHECK_COLS} FROM pm_quality_check c \
         LEFT JOIN pm_deliverable d ON d.id = c.deliverable_id AND d.project_id = c.project_id \
         LEFT JOIN tasks t ON t.id = c.task_id AND t.project_id = c.project_id \
         WHERE c.id = $1 AND c.project_id = $2"
    )).bind(id).bind(project_id).fetch_optional(&state.db).await?
      .ok_or_else(|| OfficeError::NotFound("Contrôle introuvable".into()))
}

/// POST /projects/:id/quality/checks
pub async fn create_check(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<CheckDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let label = dto.label.as_deref().map(str::trim).filter(|s| !s.is_empty())
        .ok_or_else(|| OfficeError::Validation("Décrivez le contrôle.".into()))?.to_string();
    let result = match dto.result.as_deref() {
        Some(r) => check_one(r, &RESULTS, "Résultat")?, None => "pending".into() };

    let (did, tid) = (dto.deliverable_id.flatten(), dto.task_id.flatten());
    if did.is_none() && tid.is_none() {
        return Err(OfficeError::Validation(
            "Rattachez le contrôle à un livrable ou à un lot de travail : un contrôle qui ne \
             porte sur rien ne prouve rien.".into()));
    }
    if let Some(t) = tid {
        if !task_in_project(&state, project_id, t).await? {
            return Err(OfficeError::NotFound(format!("Tâche {t}")));
        }
    }
    if let Some(d) = did {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pm_deliverable WHERE id = $1 AND project_id = $2)",
        ).bind(d).bind(project_id).fetch_one(&state.db).await?;
        if !ok { return Err(OfficeError::NotFound(format!("Livrable {d}"))); }
    }
    let position = match dto.position {
        Some(p) => p,
        None => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(position) FROM pm_quality_check WHERE project_id = $1",
        ).bind(project_id).fetch_one(&state.db).await?.map_or(0, |m| m + 1),
    };

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO pm_quality_check (project_id, deliverable_id, task_id, label, result, \
             evidence, checked_on, checked_by, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(project_id).bind(did).bind(tid).bind(&label).bind(&result)
    .bind(dto.evidence.as_deref().unwrap_or_default())
    .bind(dto.checked_on.flatten())
    .bind(if result == "pending" { None } else { Some(user.id) })
    .bind(position)
    .fetch_one(&state.db).await?;
    Ok(Json(json!({ "check": fetch_check(&state, project_id, id).await? })))
}

/// PATCH /projects/:id/quality/checks/:cid
pub async fn update_check(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, check_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<CheckDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let result = match dto.result.as_deref() { Some(r) => Some(check_one(r, &RESULTS, "Résultat")?), None => None };
    // A failed check with nothing to show for it is an assertion, not a finding.
    if result.as_deref() == Some("fail") {
        let evidence = match dto.evidence.as_deref() {
            Some(e) => e.trim().to_string(),
            None => sqlx::query_scalar::<_, String>(
                "SELECT evidence FROM pm_quality_check WHERE id = $1 AND project_id = $2",
            ).bind(check_id).bind(project_id).fetch_optional(&state.db).await?.unwrap_or_default(),
        };
        if evidence.is_empty() {
            return Err(OfficeError::Validation(
                "Indiquez ce que le contrôle a relevé avant de le déclarer en échec.".into()));
        }
    }
    if let Some(Some(tid)) = dto.task_id {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }
    // The rule holds for the life of the check, not only at its creation: a plain
    // update could otherwise detach it and leave a check that proves nothing.
    if dto.deliverable_id.is_some() || dto.task_id.is_some() {
        let current: Option<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
            "SELECT deliverable_id, task_id FROM pm_quality_check WHERE id = $1 AND project_id = $2",
        ).bind(check_id).bind(project_id).fetch_optional(&state.db).await?;
        let (cur_d, cur_t) = current.ok_or_else(|| OfficeError::NotFound("Contrôle introuvable".into()))?;
        let next_d = match dto.deliverable_id { Some(v) => v, None => cur_d };
        let next_t = match dto.task_id { Some(v) => v, None => cur_t };
        if next_d.is_none() && next_t.is_none() {
            return Err(OfficeError::Validation(
                "Un contrôle doit rester rattaché à un livrable ou à un lot de travail.".into()));
        }
    }

    let rows = sqlx::query(
        "UPDATE pm_quality_check SET \
            label = COALESCE($3, label), \
            result = COALESCE($4, result), \
            evidence = COALESCE($5, evidence), \
            deliverable_id = CASE WHEN $6::boolean THEN $7::uuid ELSE deliverable_id END, \
            task_id = CASE WHEN $8::boolean THEN $9::uuid ELSE task_id END, \
            checked_on = CASE WHEN $10::boolean THEN $11::date \
                              WHEN $4 IS NOT NULL AND $4 <> 'pending' THEN COALESCE(checked_on, CURRENT_DATE) \
                              WHEN $4 = 'pending' THEN NULL \
                              ELSE checked_on END, \
            checked_by = CASE WHEN $4 IS NOT NULL AND $4 <> 'pending' THEN COALESCE(checked_by, $12::uuid) \
                              WHEN $4 = 'pending' THEN NULL ELSE checked_by END, \
            position = COALESCE($13, position), \
            updated_at = now() \
         WHERE id = $1 AND project_id = $2",
    )
    .bind(check_id).bind(project_id)
    .bind(dto.label.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(result.as_deref())
    .bind(dto.evidence.as_deref())
    .bind(dto.deliverable_id.is_some()).bind(dto.deliverable_id.flatten())
    .bind(dto.task_id.is_some()).bind(dto.task_id.flatten())
    .bind(dto.checked_on.is_some()).bind(dto.checked_on.flatten())
    .bind(user.id).bind(dto.position)
    .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Contrôle introuvable".into())); }
    Ok(Json(json!({ "check": fetch_check(&state, project_id, check_id).await? })))
}

/// DELETE /projects/:id/quality/checks/:cid
pub async fn delete_check(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, check_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query("DELETE FROM pm_quality_check WHERE id = $1 AND project_id = $2")
        .bind(check_id).bind(project_id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Contrôle introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}

/// The cost-of-quality categories, exposed so the expenses screen can offer them
/// without hard-coding the list a second time.
pub fn coq_categories() -> &'static [&'static str; 4] { &COQ }

#[cfg(test)]
mod tests {
    use super::*;

    fn metric(target: Option<f64>, lo: Option<f64>, hi: Option<f64>, dir: &str) -> Metric {
        Metric {
            id: Uuid::nil(), project_id: Uuid::nil(), code: "Q-01".into(), name: "m".into(),
            description: String::new(), method: String::new(), unit: String::new(),
            target, tolerance_min: lo, tolerance_max: hi, direction: dir.into(),
            frequency: "sprint".into(), owner_id: None, deliverable_id: None, task_id: None,
            is_active: true, position: 0,
            created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn a_floor_only_metric_conforms_above_it() {
        let m = metric(None, Some(80.0), None, "higher");
        assert_eq!(conforms(&m, 85.0), Some(true));
        assert_eq!(conforms(&m, 79.9), Some(false));
    }

    #[test]
    fn a_ceiling_only_metric_conforms_below_it() {
        let m = metric(None, None, Some(300.0), "lower");
        assert_eq!(conforms(&m, 250.0), Some(true));
        assert_eq!(conforms(&m, 301.0), Some(false));
    }

    #[test]
    fn a_metric_with_nothing_to_compare_against_answers_nothing() {
        // A number being collected is not a standard being held to.
        let m = metric(None, None, None, "target");
        assert_eq!(conforms(&m, 42.0), None);
        // Same for an exact target with no tolerance: it could only ever be missed.
        let exact = metric(Some(10.0), None, None, "target");
        assert_eq!(conforms(&exact, 10.0), None);
    }

    #[test]
    fn margin_is_the_share_of_the_band_left_to_the_nearest_edge() {
        let m = metric(None, Some(0.0), Some(100.0), "target");
        assert_eq!(margin(&m, 50.0), Some(0.5));
        assert_eq!(margin(&m, 95.0), Some(0.05));
    }

    #[test]
    fn a_one_sided_metric_still_knows_when_it_is_near_its_bound() {
        // "Under 300 ms": 297 is plainly close to the limit, 120 is not.
        let m = metric(None, None, Some(300.0), "lower");
        let near = margin(&m, 297.0).expect("a bound gives a scale");
        let far  = margin(&m, 120.0).expect("a bound gives a scale");
        assert!(near < 0.05, "297 sits within 5 % of 300, got {near}");
        assert!(far > 0.5, "120 is far from 300, got {far}");
    }
}
