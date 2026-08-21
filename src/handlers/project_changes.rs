//! Change control: what was asked for after the plan was agreed, what it would
//! cost, and who said yes.
//!
//! A change log is not a list of requests. Its purpose is to stop scope from
//! moving without anyone noticing, and two things make that possible: an
//! **assessed impact** — on the schedule, the cost, the scope — and a **decision**
//! with a name and a date on it. Approving a change nobody has assessed is
//! precisely what change control exists to prevent, so it is refused here rather
//! than merely discouraged.
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

const CATEGORIES: [&str; 7] = ["scope", "schedule", "cost", "quality", "resource",
                               "requirement", "other"];
const KINDS: [&str; 4] = ["change", "corrective", "preventive", "defect_repair"];
const URGENCIES: [&str; 4] = ["low", "normal", "high", "critical"];
const STATUSES: [&str; 8] = ["submitted", "assessing", "approved", "partially_approved",
                             "rejected", "deferred", "implemented", "withdrawn"];
/// The states a change reaches only through a decision, never through an edit.
const DECIDED: [&str; 4] = ["approved", "partially_approved", "rejected", "deferred"];

const COLS: &str = "c.id, c.project_id, c.code, c.title, c.description, c.justification, \
     c.category, c.kind, c.urgency, c.requested_by, c.stakeholder_id, c.requested_on, \
     c.impact_days, c.impact_cost, c.impact_scope, c.impact_risk, c.impact_quality, \
     c.assessed_by, c.assessed_on, c.status, c.decision_note, c.decided_by, c.decided_on, \
     c.baseline_id, c.task_id, c.risk_id, c.decision_id, c.position, c.created_at, c.updated_at, \
     s.name AS stakeholder_name, t.name AS task_name, r.code AS risk_code, \
     b.name AS baseline_name, d.title AS decision_title";

const FROM: &str = "FROM pm_change_request c \
     LEFT JOIN pm_stakeholder s ON s.id = c.stakeholder_id \
     LEFT JOIN tasks t ON t.id = c.task_id AND t.project_id = c.project_id \
     LEFT JOIN pm_risk r ON r.id = c.risk_id \
     LEFT JOIN project_baselines b ON b.id = c.baseline_id \
     LEFT JOIN pm_decision d ON d.id = c.decision_id";

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct ChangeRequest {
    id:               Uuid,
    project_id:       Uuid,
    code:             String,
    title:            String,
    description:      String,
    justification:    String,
    category:         String,
    kind:             String,
    urgency:          String,
    requested_by:     Option<Uuid>,
    stakeholder_id:   Option<Uuid>,
    stakeholder_name: Option<String>,
    requested_on:     NaiveDate,
    /// NULL until somebody assessed it — not zero. The difference is the point.
    impact_days:      Option<i32>,
    impact_cost:      Option<f64>,
    impact_scope:     String,
    impact_risk:      String,
    impact_quality:   String,
    assessed_by:      Option<Uuid>,
    assessed_on:      Option<NaiveDate>,
    status:           String,
    decision_note:    String,
    decided_by:       Option<Uuid>,
    decided_on:       Option<NaiveDate>,
    baseline_id:      Option<Uuid>,
    baseline_name:    Option<String>,
    task_id:          Option<Uuid>,
    task_name:        Option<String>,
    risk_id:          Option<Uuid>,
    risk_code:        Option<String>,
    decision_id:      Option<Uuid>,
    decision_title:   Option<String>,
    position:         i32,
    created_at:       chrono::DateTime<chrono::Utc>,
    updated_at:       chrono::DateTime<chrono::Utc>,
}

fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where T: Deserialize<'de>, D: Deserializer<'de> {
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct ChangeDto {
    pub code:          Option<String>,
    pub title:         Option<String>,
    pub description:   Option<String>,
    pub justification: Option<String>,
    pub category:      Option<String>,
    pub kind:          Option<String>,
    pub urgency:       Option<String>,
    pub position:      Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub stakeholder_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub task_id:        Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub risk_id:        Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub decision_id:    Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub requested_on:   Option<Option<NaiveDate>>,
}

#[derive(Debug, Deserialize)]
pub struct AssessDto {
    pub impact_scope:   Option<String>,
    pub impact_risk:    Option<String>,
    pub impact_quality: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub impact_days: Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    pub impact_cost: Option<Option<f64>>,
}

#[derive(Debug, Deserialize)]
pub struct DecideDto {
    pub status:        Option<String>,
    pub decision_note: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub baseline_id:   Option<Option<Uuid>>,
}

fn check(value: &str, allowed: &[&str], field: &str) -> Result<String> {
    let v = value.trim();
    if !allowed.contains(&v) {
        return Err(OfficeError::Validation(format!(
            "{field} : valeur « {v} » refusée. Valeurs admises : {}.", allowed.join(", ")
        )));
    }
    Ok(v.to_string())
}

async fn fetch(state: &AppState, project_id: Uuid, id: Uuid) -> Result<ChangeRequest> {
    sqlx::query_as::<_, ChangeRequest>(
        &format!("SELECT {COLS} {FROM} WHERE c.id = $1 AND c.project_id = $2"),
    ).bind(id).bind(project_id).fetch_optional(&state.db).await?
     .ok_or_else(|| OfficeError::NotFound("Demande de changement introuvable".into()))
}

/// GET /projects/:id/changes
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let changes = sqlx::query_as::<_, ChangeRequest>(&format!(
        "SELECT {COLS} {FROM} WHERE c.project_id = $1 \
         ORDER BY (c.status IN ('submitted', 'assessing')) DESC, c.requested_on DESC, c.position"
    )).bind(project_id).fetch_all(&state.db).await?;

    let mut by_status: std::collections::HashMap<&str, i64> = std::collections::HashMap::new();
    // What the approved changes have already done to the plan. Nobody keeps this
    // number, and it is the one that explains why the project no longer matches
    // the baseline it was judged against.
    let (mut days, mut cost) = (0i64, 0f64);
    let mut costed = 0i64;
    let mut awaiting = Vec::new();
    for c in &changes {
        *by_status.entry(c.status.as_str()).or_insert(0) += 1;
        if c.status == "approved" || c.status == "partially_approved" || c.status == "implemented" {
            if let Some(d) = c.impact_days { days += i64::from(d) }
            if let Some(m) = c.impact_cost { cost += m; costed += 1 }
        }
        if c.status == "submitted" || c.status == "assessing" {
            awaiting.push(json!({
                "id": c.id, "code": c.code, "title": c.title, "urgency": c.urgency,
                "requested_on": c.requested_on, "assessed": c.assessed_on.is_some(),
            }));
        }
    }
    let approved: i64 = ["approved", "partially_approved", "implemented"].iter()
        .map(|s| by_status.get(s).copied().unwrap_or(0)).sum();

    Ok(Json(json!({
        "changes": changes,
        "summary": {
            "total": changes.len(),
            "awaiting_decision": awaiting.len(),
            "awaiting_assessment": changes.iter()
                .filter(|c| c.assessed_on.is_none() && (c.status == "submitted" || c.status == "assessing"))
                .count(),
            "approved": approved,
            "rejected": by_status.get("rejected").copied().unwrap_or(0),
            "deferred": by_status.get("deferred").copied().unwrap_or(0),
            // Cumulative effect of everything that was said yes to.
            "approved_impact": { "days": days, "cost": (cost * 100.0).round() / 100.0, "costed": costed },
            // Approved without anyone naming the baseline the plan moved to. The
            // change is in the plan, but nothing records what the plan became.
            "approved_without_baseline": changes.iter()
                .filter(|c| (c.status == "approved" || c.status == "implemented") && c.baseline_id.is_none())
                .count(),
        },
        "awaiting": awaiting,
    })))
}

/// POST /projects/:id/changes
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<ChangeDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Comment).await?;
    let title = dto.title.as_deref().map(str::trim).filter(|s| !s.is_empty())
        .ok_or_else(|| OfficeError::Validation("Nommez la demande de changement.".into()))?
        .to_string();
    let category = match dto.category.as_deref() { Some(c) => check(c, &CATEGORIES, "Catégorie")?, None => "scope".into() };
    let kind = match dto.kind.as_deref() { Some(k) => check(k, &KINDS, "Nature")?, None => "change".into() };
    let urgency = match dto.urgency.as_deref() { Some(u) => check(u, &URGENCIES, "Urgence")?, None => "normal".into() };
    if let Some(tid) = dto.task_id.flatten() {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }

    let code = match dto.code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None => {
            let highest: Option<i32> = sqlx::query_scalar(
                "SELECT MAX(CAST(substring(code FROM '^CR-([0-9]+)$') AS INT)) \
                 FROM pm_change_request WHERE project_id = $1 AND code ~ '^CR-[0-9]+$'",
            ).bind(project_id).fetch_one(&state.db).await?;
            format!("CR-{:02}", highest.unwrap_or(0) + 1)
        }
    };
    let position = match dto.position {
        Some(p) => p,
        None => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(position) FROM pm_change_request WHERE project_id = $1",
        ).bind(project_id).fetch_one(&state.db).await?.map_or(0, |m| m + 1),
    };

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO pm_change_request (project_id, code, title, description, justification, \
             category, kind, urgency, requested_by, stakeholder_id, requested_on, \
             task_id, risk_id, decision_id, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, COALESCE($11, CURRENT_DATE), \
                 $12, $13, $14, $15) RETURNING id",
    )
    .bind(project_id).bind(&code).bind(&title)
    .bind(dto.description.as_deref().unwrap_or_default())
    .bind(dto.justification.as_deref().unwrap_or_default())
    .bind(&category).bind(&kind).bind(&urgency).bind(user.id)
    .bind(dto.stakeholder_id.flatten()).bind(dto.requested_on.flatten())
    .bind(dto.task_id.flatten()).bind(dto.risk_id.flatten())
    .bind(dto.decision_id.flatten()).bind(position)
    .fetch_one(&state.db).await?;
    Ok(Json(json!({ "change": fetch(&state, project_id, id).await? })))
}

/// PATCH /projects/:id/changes/:cid — the request itself, never its decision.
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, change_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<ChangeDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let category = match dto.category.as_deref() { Some(c) => Some(check(c, &CATEGORIES, "Catégorie")?), None => None };
    let kind = match dto.kind.as_deref() { Some(k) => Some(check(k, &KINDS, "Nature")?), None => None };
    let urgency = match dto.urgency.as_deref() { Some(u) => Some(check(u, &URGENCIES, "Urgence")?), None => None };
    if let Some(Some(tid)) = dto.task_id {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }
    // Rewriting a request after it was decided would make the record say the
    // board approved something other than what it read.
    let current = fetch(&state, project_id, change_id).await?;
    if DECIDED.contains(&current.status.as_str()) || current.status == "implemented" {
        return Err(OfficeError::Validation(format!(
            "{} a déjà été tranchée. Modifier la demande après coup ferait dire au registre \
             que la décision portait sur autre chose.", current.code)));
    }

    let rows = sqlx::query(
        "UPDATE pm_change_request SET \
            code = COALESCE($3, code), title = COALESCE($4, title), \
            description = COALESCE($5, description), justification = COALESCE($6, justification), \
            category = COALESCE($7, category), kind = COALESCE($8, kind), \
            urgency = COALESCE($9, urgency), \
            stakeholder_id = CASE WHEN $10::boolean THEN $11::uuid ELSE stakeholder_id END, \
            task_id = CASE WHEN $12::boolean THEN $13::uuid ELSE task_id END, \
            risk_id = CASE WHEN $14::boolean THEN $15::uuid ELSE risk_id END, \
            decision_id = CASE WHEN $16::boolean THEN $17::uuid ELSE decision_id END, \
            requested_on = CASE WHEN $18::boolean THEN $19::date ELSE requested_on END, \
            position = COALESCE($20, position), updated_at = now() \
         WHERE id = $1 AND project_id = $2",
    )
    .bind(change_id).bind(project_id)
    .bind(dto.code.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.title.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.description.as_deref()).bind(dto.justification.as_deref())
    .bind(category.as_deref()).bind(kind.as_deref()).bind(urgency.as_deref())
    .bind(dto.stakeholder_id.is_some()).bind(dto.stakeholder_id.flatten())
    .bind(dto.task_id.is_some()).bind(dto.task_id.flatten())
    .bind(dto.risk_id.is_some()).bind(dto.risk_id.flatten())
    .bind(dto.decision_id.is_some()).bind(dto.decision_id.flatten())
    .bind(dto.requested_on.is_some()).bind(dto.requested_on.flatten())
    .bind(dto.position)
    .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Demande de changement introuvable".into())); }
    Ok(Json(json!({ "change": fetch(&state, project_id, change_id).await? })))
}

/// POST /projects/:id/changes/:cid/assess — record what it would cost.
pub async fn assess(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, change_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<AssessDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let current = fetch(&state, project_id, change_id).await?;
    if DECIDED.contains(&current.status.as_str()) {
        return Err(OfficeError::Validation(format!(
            "{} a déjà été tranchée : l'évaluation ne peut plus changer.", current.code)));
    }

    // An assessment that says nothing is not an assessment. At least one of the
    // five dimensions has to carry something, or "assessed" would be a rubber stamp.
    let says_something = dto.impact_days.flatten().is_some()
        || dto.impact_cost.flatten().is_some()
        || dto.impact_scope.as_deref().is_some_and(|s| !s.trim().is_empty())
        || dto.impact_risk.as_deref().is_some_and(|s| !s.trim().is_empty())
        || dto.impact_quality.as_deref().is_some_and(|s| !s.trim().is_empty());
    if !says_something {
        return Err(OfficeError::Validation(
            "Une évaluation doit dire quelque chose : délai, coût, périmètre, risque ou qualité. \
             Sans cela, « évaluée » ne veut rien dire.".into()));
    }

    let rows = sqlx::query(
        "UPDATE pm_change_request SET \
            impact_days = CASE WHEN $3::boolean THEN $4::int ELSE impact_days END, \
            impact_cost = CASE WHEN $5::boolean THEN $6::double precision ELSE impact_cost END, \
            impact_scope = COALESCE($7, impact_scope), \
            impact_risk = COALESCE($8, impact_risk), \
            impact_quality = COALESCE($9, impact_quality), \
            assessed_by = $10, assessed_on = CURRENT_DATE, \
            status = CASE WHEN status = 'submitted' THEN 'assessing' ELSE status END, \
            updated_at = now() \
         WHERE id = $1 AND project_id = $2",
    )
    .bind(change_id).bind(project_id)
    .bind(dto.impact_days.is_some()).bind(dto.impact_days.flatten())
    .bind(dto.impact_cost.is_some()).bind(dto.impact_cost.flatten())
    .bind(dto.impact_scope.as_deref()).bind(dto.impact_risk.as_deref())
    .bind(dto.impact_quality.as_deref()).bind(user.id)
    .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Demande de changement introuvable".into())); }

    Ok(Json(json!({ "change": fetch(&state, project_id, change_id).await? })))
}

/// POST /projects/:id/changes/:cid/decide — the board's answer. Owner only.
pub async fn decide(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, change_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<DecideDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Owner).await?;
    let status = dto.status.as_deref()
        .ok_or_else(|| OfficeError::Validation("Indiquez la décision.".into()))?;
    let status = check(status, &STATUSES, "Décision")?;
    if !DECIDED.contains(&status.as_str()) && status != "withdrawn" && status != "implemented" {
        return Err(OfficeError::Validation(format!(
            "« {status} » n'est pas une décision. Valeurs admises : {}.", DECIDED.join(", "))));
    }

    let current = fetch(&state, project_id, change_id).await?;
    // The whole point of change control: nothing is approved before somebody has
    // worked out what it costs.
    if (status == "approved" || status == "partially_approved") && current.assessed_on.is_none() {
        return Err(OfficeError::Validation(format!(
            "{} n'a pas été évaluée. Approuver un changement dont personne n'a chiffré l'effet \
             sur le délai, le coût et le périmètre, c'est exactement ce que la maîtrise des \
             changements sert à empêcher.", current.code)));
    }
    // A partial approval that does not say what was kept is unusable later.
    let note = dto.decision_note.as_deref().map(str::trim).unwrap_or("");
    if status == "partially_approved" && note.is_empty() {
        return Err(OfficeError::Validation(
            "Une approbation partielle doit dire ce qui a été retenu et ce qui ne l'a pas été.".into()));
    }
    if status == "rejected" && note.is_empty() {
        return Err(OfficeError::Validation(
            "Indiquez le motif du refus : le demandeur doit pouvoir le comprendre.".into()));
    }
    if let Some(bid) = dto.baseline_id.flatten() {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM project_baselines WHERE id = $1 AND project_id = $2)",
        ).bind(bid).bind(project_id).fetch_one(&state.db).await?;
        if !ok { return Err(OfficeError::NotFound(format!("Plan de référence {bid}"))); }
    }

    let rows = sqlx::query(
        "UPDATE pm_change_request SET \
            status = $3, decision_note = COALESCE($4, decision_note), \
            decided_by = $5, decided_on = CURRENT_DATE, \
            baseline_id = CASE WHEN $6::boolean THEN $7::uuid ELSE baseline_id END, \
            updated_at = now() \
         WHERE id = $1 AND project_id = $2",
    )
    .bind(change_id).bind(project_id).bind(&status)
    .bind(if note.is_empty() { None } else { Some(note) })
    .bind(user.id)
    .bind(dto.baseline_id.is_some()).bind(dto.baseline_id.flatten())
    .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Demande de changement introuvable".into())); }

    Ok(Json(json!({ "change": fetch(&state, project_id, change_id).await? })))
}

/// DELETE /projects/:id/changes/:cid
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, change_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    // A decided change is the record of what the board answered. Withdrawing a
    // request is a status, not a deletion.
    let current = fetch(&state, project_id, change_id).await?;
    if DECIDED.contains(&current.status.as_str()) || current.status == "implemented" {
        return Err(OfficeError::Validation(format!(
            "{} a été tranchée : le registre garde la trace de ce qui a été décidé. \
             Utilisez « retirée » si la demande n'a plus lieu d'être.", current.code)));
    }
    let rows = sqlx::query("DELETE FROM pm_change_request WHERE id = $1 AND project_id = $2")
        .bind(change_id).bind(project_id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Demande de changement introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}
