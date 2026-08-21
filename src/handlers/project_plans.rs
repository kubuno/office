//! The subsidiary management plans: how each area will be run.
//!
//! The registers built so far answer *what* — which risks, which changes, which
//! costs. None of them answered *how*: above what score a risk must be escalated,
//! beyond what variance a cost is a problem rather than noise, below what impact
//! the project manager may decide alone. Those thresholds existed nowhere, so
//! every artefact judged against a rule hard-coded in the module and identical
//! for a three-task plan and a construction programme.
//!
//! So this is not a set of text boxes. The prose says how the area is run; the
//! few structured fields beside it are read back by the artefacts themselves,
//! which is what `applied()` exposes.
use axum::{extract::{Path, State}, Extension, Json};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    handlers::project_authz::{require_permission, Level},
    middleware::OfficeUser,
    state::AppState,
};

/// Every area a project can plan. The order is the reading order of the
/// integrated plan document, not alphabetical.
pub const AREAS: [&str; 12] = [
    "scope", "requirements", "schedule", "cost", "quality", "resource",
    "communications", "risk", "procurement", "stakeholder", "change", "configuration",
];
const FREQUENCIES: [&str; 6] = ["weekly", "biweekly", "monthly", "quarterly",
                                "milestone", "on_demand"];

const COLS: &str = "id, project_id, area, is_active, approach, roles, procedures, tools, \
     variance_threshold_pct, risk_appetite_score, change_authority_amount, \
     change_authority_days, review_frequency, updated_at";

#[derive(Debug, sqlx::FromRow, serde::Serialize, Clone)]
pub struct ManagementPlan {
    id:         Uuid,
    project_id: Uuid,
    area:       String,
    is_active:  bool,
    approach:   String,
    roles:      String,
    procedures: String,
    tools:      String,
    variance_threshold_pct:  Option<f64>,
    risk_appetite_score:     Option<i32>,
    change_authority_amount: Option<f64>,
    change_authority_days:   Option<i32>,
    review_frequency:        String,
    updated_at: chrono::DateTime<chrono::Utc>,
}

fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where T: Deserialize<'de>, D: Deserializer<'de> {
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct PlanDto {
    pub is_active:        Option<bool>,
    pub approach:         Option<String>,
    pub roles:            Option<String>,
    pub procedures:       Option<String>,
    pub tools:            Option<String>,
    pub review_frequency: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub variance_threshold_pct:  Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub risk_appetite_score:     Option<Option<i32>>,
    #[serde(default, deserialize_with = "double_option")]
    pub change_authority_amount: Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub change_authority_days:   Option<Option<i32>>,
}

fn check_area(area: &str) -> Result<String> {
    let a = area.trim();
    if !AREAS.contains(&a) {
        return Err(OfficeError::Validation(format!(
            "Domaine « {a} » inconnu. Valeurs admises : {}.", AREAS.join(", ")
        )));
    }
    Ok(a.to_string())
}

/// The thresholds the rest of the module reads, gathered from whichever plans
/// are active. Exposed as one object so a caller never has to know which area
/// holds which figure.
///
/// Every field is optional on purpose: an absent threshold means the project has
/// not set one, and the artefact keeps its own default rather than inventing a
/// rule the project never agreed to.
pub struct AppliedThresholds {
    pub cost_variance_pct:     Option<f64>,
    pub schedule_variance_pct: Option<f64>,
    pub risk_appetite_score:   Option<i32>,
    pub change_amount:         Option<f64>,
    pub change_days:           Option<i32>,
}

pub async fn applied(state: &AppState, project_id: Uuid) -> Result<AppliedThresholds> {
    let rows = sqlx::query_as::<_, (String, Option<f64>, Option<i32>, Option<f64>, Option<i32>)>(
        "SELECT area, variance_threshold_pct, risk_appetite_score, \
                change_authority_amount, change_authority_days \
         FROM pm_management_plan WHERE project_id = $1 AND is_active",
    ).bind(project_id).fetch_all(&state.db).await?;

    let mut out = AppliedThresholds {
        cost_variance_pct: None, schedule_variance_pct: None,
        risk_appetite_score: None, change_amount: None, change_days: None,
    };
    for (area, variance, appetite, amount, days) in rows {
        match area.as_str() {
            "cost"     => out.cost_variance_pct = variance,
            "schedule" => out.schedule_variance_pct = variance,
            "risk"     => out.risk_appetite_score = appetite,
            "change"   => { out.change_amount = amount; out.change_days = days }
            _ => {}
        }
    }
    Ok(out)
}

/// GET /projects/:id/plans
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let stored = sqlx::query_as::<_, ManagementPlan>(
        &format!("SELECT {COLS} FROM pm_management_plan WHERE project_id = $1"),
    ).bind(project_id).fetch_all(&state.db).await?;
    let by_area: std::collections::HashMap<&str, &ManagementPlan> =
        stored.iter().map(|p| (p.area.as_str(), p)).collect();

    // Every area is returned, in reading order, whether or not it has a row. An
    // area nobody has written about is a plan not made, not a plan missing from
    // the interface.
    let plans: Vec<Value> = AREAS.iter().map(|area| match by_area.get(area) {
        Some(p) => json!({ "area": area, "planned": true, "plan": p }),
        None => json!({ "area": area, "planned": false, "plan": Value::Null }),
    }).collect();

    let active = stored.iter().filter(|p| p.is_active).count();
    // A plan switched on but never written is worse than none: it says the area
    // is governed when nothing says how.
    let empty: Vec<&str> = stored.iter()
        .filter(|p| p.is_active && p.approach.trim().is_empty())
        .map(|p| p.area.as_str()).collect();

    Ok(Json(json!({
        "plans": plans,
        "summary": { "areas": AREAS.len(), "active": active, "without_approach": empty.len() },
        "without_approach": empty,
    })))
}

/// PUT /projects/:id/plans/:area
pub async fn upsert(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, area)): Path<(Uuid, String)>,
    Json(dto): Json<PlanDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let area = check_area(&area)?;
    if let Some(f) = dto.review_frequency.as_deref() {
        if !FREQUENCIES.contains(&f.trim()) {
            return Err(OfficeError::Validation(format!(
                "Fréquence de revue « {f} » refusée. Valeurs admises : {}.",
                FREQUENCIES.join(", "))));
        }
    }
    if let Some(Some(score)) = dto.risk_appetite_score {
        if !(1..=25).contains(&score) {
            return Err(OfficeError::Validation(
                "L'appétit au risque se règle sur l'échelle des scores, de 1 à 25 \
                 (probabilité × impact).".into()));
        }
    }
    // A threshold only means something where the artefact reads it. Accepting one
    // silently on the wrong plan would leave someone convinced a rule is in force
    // when nothing consults it.
    let misplaced = match area.as_str() {
        "cost" | "schedule" => dto.risk_appetite_score.flatten().is_some()
            || dto.change_authority_amount.flatten().is_some(),
        "risk" => dto.variance_threshold_pct.flatten().is_some()
            || dto.change_authority_amount.flatten().is_some(),
        "change" => dto.variance_threshold_pct.flatten().is_some()
            || dto.risk_appetite_score.flatten().is_some(),
        _ => dto.variance_threshold_pct.flatten().is_some()
            || dto.risk_appetite_score.flatten().is_some()
            || dto.change_authority_amount.flatten().is_some(),
    };
    if misplaced {
        return Err(OfficeError::Validation(format!(
            "Ce seuil n'est pas lu par le plan « {area} ». L'écart admis se règle sur les \
             coûts et l'échéancier, l'appétit au risque sur les risques, et la délégation \
             de décision sur les changements."
        )));
    }

    let plan = sqlx::query_as::<_, ManagementPlan>(&format!(
        "INSERT INTO pm_management_plan (project_id, area, is_active, approach, roles, \
             procedures, tools, variance_threshold_pct, risk_appetite_score, \
             change_authority_amount, change_authority_days, review_frequency) \
         VALUES ($1, $2, COALESCE($3, TRUE), COALESCE($4, ''), COALESCE($5, ''), \
                 COALESCE($6, ''), COALESCE($7, ''), $8, $9, $10, $11, COALESCE($12, 'monthly')) \
         ON CONFLICT (project_id, area) DO UPDATE SET \
             is_active = COALESCE($3, pm_management_plan.is_active), \
             approach = COALESCE($4, pm_management_plan.approach), \
             roles = COALESCE($5, pm_management_plan.roles), \
             procedures = COALESCE($6, pm_management_plan.procedures), \
             tools = COALESCE($7, pm_management_plan.tools), \
             variance_threshold_pct = CASE WHEN $13::boolean THEN $8 ELSE pm_management_plan.variance_threshold_pct END, \
             risk_appetite_score = CASE WHEN $14::boolean THEN $9 ELSE pm_management_plan.risk_appetite_score END, \
             change_authority_amount = CASE WHEN $15::boolean THEN $10 ELSE pm_management_plan.change_authority_amount END, \
             change_authority_days = CASE WHEN $16::boolean THEN $11 ELSE pm_management_plan.change_authority_days END, \
             review_frequency = COALESCE($12, pm_management_plan.review_frequency), \
             updated_at = now() \
         RETURNING {COLS}"
    ))
    .bind(project_id).bind(&area).bind(dto.is_active)
    .bind(dto.approach.as_deref()).bind(dto.roles.as_deref())
    .bind(dto.procedures.as_deref()).bind(dto.tools.as_deref())
    .bind(dto.variance_threshold_pct.flatten()).bind(dto.risk_appetite_score.flatten())
    .bind(dto.change_authority_amount.flatten()).bind(dto.change_authority_days.flatten())
    .bind(dto.review_frequency.as_deref().map(str::trim))
    .bind(dto.variance_threshold_pct.is_some()).bind(dto.risk_appetite_score.is_some())
    .bind(dto.change_authority_amount.is_some()).bind(dto.change_authority_days.is_some())
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "plan": plan })))
}

/// DELETE /projects/:id/plans/:area — the area stops being planned at all,
/// which is not the same as a plan switched off and left written.
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, area)): Path<(Uuid, String)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let area = check_area(&area)?;
    let rows = sqlx::query("DELETE FROM pm_management_plan WHERE project_id = $1 AND area = $2")
        .bind(project_id).bind(&area)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Plan introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}
