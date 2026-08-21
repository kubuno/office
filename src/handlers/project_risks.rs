//! The risk register: what might still go wrong — or right — and what is being
//! done about it.
//!
//! Two things distinguish it from an ordinary list. Risks are ranked by
//! probability × impact on a 5×5 scale, and that ranking is aggregated into the
//! probability/impact matrix which shows where a project's exposure actually
//! sits. And opportunities are managed alongside threats: a register that only
//! records bad news teaches people to look one way.
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

const RISK_COLS: &str = "r.id, r.project_id, r.code, r.title, r.description, r.category, r.kind, \
     r.probability, r.impact, r.score, r.probability_pct, r.monetary_impact, r.status, \
     r.owner_id, r.trigger_signs, r.response_strategy, r.response_plan, r.residual_notes, \
     r.parent_risk_id, r.task_id, r.identified_at, r.closed_at, r.position, \
     r.created_at, r.updated_at, \
     (SELECT COALESCE(NULLIF(u.display_name, ''), u.email::text) \
        FROM core.users u WHERE u.id = r.owner_id) AS owner_name, \
     t.name AS task_name, \
     p.code AS parent_code, p.title AS parent_title";

const FROM_RISK: &str = "FROM pm_risk r \
     LEFT JOIN tasks t ON t.id = r.task_id AND t.project_id = r.project_id \
     LEFT JOIN pm_risk p ON p.id = r.parent_risk_id";

const CATEGORIES: [&str; 5] = ["technical", "external", "organizational", "management", "commercial"];
const KINDS: [&str; 2] = ["threat", "opportunity"];
const STATUSES: [&str; 5] = ["identified", "analysing", "responding", "occurred", "closed"];
/// Threats are avoided, mitigated or transferred; opportunities are exploited,
/// enhanced or shared. Both may be accepted, and both may be escalated when the
/// decision sits above the project's authority.
const THREAT_STRATEGIES: [&str; 5] = ["avoid", "mitigate", "transfer", "accept", "escalate"];
const OPPORTUNITY_STRATEGIES: [&str; 5] = ["exploit", "enhance", "share", "accept", "escalate"];

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Risk {
    id:                Uuid,
    project_id:        Uuid,
    code:              String,
    title:             String,
    description:       String,
    category:          String,
    kind:              String,
    probability:       i32,
    impact:            i32,
    /// Kept by the database, so the ranking cannot disagree with its two factors.
    score:             i32,
    probability_pct:   Option<f64>,
    monetary_impact:   Option<f64>,
    status:            String,
    owner_id:          Option<Uuid>,
    owner_name:        Option<String>,
    trigger_signs:     String,
    response_strategy: String,
    response_plan:     String,
    residual_notes:    String,
    parent_risk_id:    Option<Uuid>,
    parent_code:       Option<String>,
    parent_title:      Option<String>,
    task_id:           Option<Uuid>,
    task_name:         Option<String>,
    identified_at:     Option<NaiveDate>,
    closed_at:         Option<chrono::DateTime<chrono::Utc>>,
    position:          i32,
    created_at:        chrono::DateTime<chrono::Utc>,
    updated_at:        chrono::DateTime<chrono::Utc>,
}

/// Tells an absent field apart from one explicitly set to null, so an owner, a
/// date or a link can be cleared and not merely changed.
fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where T: Deserialize<'de>, D: Deserializer<'de> {
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct RiskDto {
    pub code:              Option<String>,
    pub title:             Option<String>,
    pub description:       Option<String>,
    pub category:          Option<String>,
    pub kind:              Option<String>,
    pub probability:       Option<i32>,
    pub impact:            Option<i32>,
    pub status:            Option<String>,
    pub trigger_signs:     Option<String>,
    pub response_strategy: Option<String>,
    pub response_plan:     Option<String>,
    pub residual_notes:    Option<String>,
    pub position:          Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub probability_pct:   Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub monetary_impact:   Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub owner_id:          Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub task_id:           Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub parent_risk_id:    Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub identified_at:     Option<Option<NaiveDate>>,
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

fn check_scale(value: i32, field: &str) -> Result<i32> {
    if !(1..=5).contains(&value) {
        return Err(OfficeError::Validation(format!(
            "{field} : attendu une valeur de 1 à 5, reçu {value}."
        )));
    }
    Ok(value)
}

/// A response only makes sense against the kind of risk it answers: you do not
/// "mitigate" an opportunity, you enhance it.
fn check_strategy(strategy: &str, kind: &str) -> Result<String> {
    let allowed: &[&str] = if kind == "opportunity" { &OPPORTUNITY_STRATEGIES } else { &THREAT_STRATEGIES };
    let s = strategy.trim();
    if !allowed.contains(&s) {
        let label = if kind == "opportunity" { "une opportunité" } else { "une menace" };
        return Err(OfficeError::Validation(format!(
            "Stratégie « {s} » inapplicable à {label}. Valeurs admises : {}.", allowed.join(", ")
        )));
    }
    Ok(s.to_string())
}

/// A risk named in a request must belong to the project in the address.
async fn risk_in_project(state: &AppState, project_id: Uuid, risk_id: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM pm_risk WHERE id = $1 AND project_id = $2)",
    ).bind(risk_id).bind(project_id).fetch_one(&state.db).await?)
}

/// Expected monetary value: the chance of it happening times what it would cost.
/// Negative for a threat, positive for an opportunity — so a register's values
/// can simply be summed into the contingency a project should be holding.
fn emv(risk: &Risk) -> Option<f64> {
    let value = risk.probability_pct? / 100.0 * risk.monetary_impact?;
    Some(if risk.kind == "opportunity" { value } else { -value })
}

fn with_emv(risk: &Risk) -> Value {
    let mut v = serde_json::to_value(risk).unwrap_or_else(|_| json!({}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("emv".into(), json!(emv(risk)));
    }
    v
}

/// GET /projects/:id/risks
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let risks = sqlx::query_as::<_, Risk>(&format!(
        "SELECT {RISK_COLS} {FROM_RISK} WHERE r.project_id = $1 \
         ORDER BY r.score DESC, r.position, r.created_at"
    )).bind(project_id).fetch_all(&state.db).await?;

    // The matrix: how many risks sit at each probability/impact cell. Built here
    // rather than in the browser so every consumer counts the same way.
    let mut matrix = vec![vec![0i64; 5]; 5];
    let (mut threats, mut opportunities, mut open, mut total_emv) = (0i64, 0i64, 0i64, 0f64);
    let mut priced = 0i64;
    let mut occurred = 0i64;
    for r in &risks {
        // A risk that has happened is no longer a probability — it is an issue,
        // and it lives in the issue log now. A closed one is history. Neither
        // belongs in a picture of what the project is still exposed to.
        let still_ahead = r.status != "closed" && r.status != "occurred";
        if still_ahead {
            matrix[(r.probability - 1) as usize][(r.impact - 1) as usize] += 1;
            open += 1;
        }
        if r.status == "occurred" { occurred += 1 }
        if r.kind == "opportunity" { opportunities += 1 } else { threats += 1 }
        // Same reasoning for the provision: money is set aside against what may
        // still happen, not against what already did.
        if still_ahead { if let Some(v) = emv(r) { total_emv += v; priced += 1 } }
    }

    Ok(Json(json!({
        "risks": risks.iter().map(with_emv).collect::<Vec<_>>(),
        "matrix": matrix,
        "summary": {
            "total": risks.len(), "open": open, "occurred": occurred,
            "threats": threats, "opportunities": opportunities,
            // Rounded to the cent: a sum of products of decimals read through f64
            // would otherwise surface its own noise.
            "total_emv": (total_emv * 100.0).round() / 100.0,
            "priced": priced,
        },
    })))
}

/// POST /projects/:id/risks
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<RiskDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;

    let title = dto.title.as_deref().map(str::trim).filter(|s| !s.is_empty())
        .ok_or_else(|| OfficeError::Validation("Nommez le risque.".into()))?
        .to_string();
    let kind = match dto.kind.as_deref() { Some(k) => check(k, &KINDS, "Nature")?, None => "threat".into() };
    let category = match dto.category.as_deref() {
        Some(c) => check(c, &CATEGORIES, "Catégorie")?, None => "technical".into() };
    let status = match dto.status.as_deref() {
        Some(s) => check(s, &STATUSES, "Statut")?, None => "identified".into() };
    let strategy = match dto.response_strategy.as_deref() {
        Some(s) => check_strategy(s, &kind)?, None => "accept".into() };
    let probability = check_scale(dto.probability.unwrap_or(3), "Probabilité")?;
    let impact = check_scale(dto.impact.unwrap_or(3), "Impact")?;

    if let Some(tid) = dto.task_id.flatten() {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }
    if let Some(pid) = dto.parent_risk_id.flatten() {
        if !risk_in_project(&state, project_id, pid).await? {
            return Err(OfficeError::NotFound(format!("Risque {pid}")));
        }
    }

    // A risk without a reference cannot be cited in a meeting. Derived from the
    // highest number already issued, never from the count: deleting R-02 must not
    // hand its number to the next risk.
    let code = match dto.code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None => {
            let highest: Option<i32> = sqlx::query_scalar(
                "SELECT MAX(CAST(substring(code FROM '^R-([0-9]+)$') AS INT)) \
                 FROM pm_risk WHERE project_id = $1 AND code ~ '^R-[0-9]+$'",
            ).bind(project_id).fetch_one(&state.db).await?;
            format!("R-{:02}", highest.unwrap_or(0) + 1)
        }
    };
    let position = match dto.position {
        Some(p) => p,
        None => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(position) FROM pm_risk WHERE project_id = $1",
        ).bind(project_id).fetch_one(&state.db).await?.map_or(0, |m| m + 1),
    };

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO pm_risk (project_id, code, title, description, category, kind, \
             probability, impact, probability_pct, monetary_impact, status, owner_id, \
             trigger_signs, response_strategy, response_plan, residual_notes, \
             parent_risk_id, task_id, identified_at, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, \
                 $17, $18, COALESCE($19, CURRENT_DATE), $20) RETURNING id",
    )
    .bind(project_id).bind(&code).bind(&title)
    .bind(dto.description.as_deref().unwrap_or_default())
    .bind(&category).bind(&kind).bind(probability).bind(impact)
    .bind(dto.probability_pct.flatten()).bind(dto.monetary_impact.flatten())
    .bind(&status).bind(dto.owner_id.flatten())
    .bind(dto.trigger_signs.as_deref().unwrap_or_default())
    .bind(&strategy)
    .bind(dto.response_plan.as_deref().unwrap_or_default())
    .bind(dto.residual_notes.as_deref().unwrap_or_default())
    .bind(dto.parent_risk_id.flatten()).bind(dto.task_id.flatten())
    .bind(dto.identified_at.flatten()).bind(position)
    .fetch_one(&state.db).await?;

    let risk = fetch_one(&state, project_id, id).await?;
    Ok(Json(json!({ "risk": with_emv(&risk) })))
}

async fn fetch_one(state: &AppState, project_id: Uuid, risk_id: Uuid) -> Result<Risk> {
    sqlx::query_as::<_, Risk>(&format!(
        "SELECT {RISK_COLS} {FROM_RISK} WHERE r.id = $1 AND r.project_id = $2"
    )).bind(risk_id).bind(project_id).fetch_optional(&state.db).await?
      .ok_or_else(|| OfficeError::NotFound("Risque introuvable".into()))
}

/// PATCH /projects/:id/risks/:rid
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, risk_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<RiskDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let current = fetch_one(&state, project_id, risk_id).await?;

    let kind = match dto.kind.as_deref() { Some(k) => check(k, &KINDS, "Nature")?, None => current.kind.clone() };
    // Changing the kind can invalidate a response that was valid a moment ago,
    // so the pairing is re-checked against whichever kind now applies.
    let strategy = match dto.response_strategy.as_deref() {
        Some(s) => check_strategy(s, &kind)?,
        None if kind != current.kind => {
            let allowed: &[&str] = if kind == "opportunity" { &OPPORTUNITY_STRATEGIES } else { &THREAT_STRATEGIES };
            if allowed.contains(&current.response_strategy.as_str()) { current.response_strategy.clone() }
            else { "accept".into() }
        }
        None => current.response_strategy.clone(),
    };
    let category = match dto.category.as_deref() { Some(c) => check(c, &CATEGORIES, "Catégorie")?, None => current.category.clone() };
    let status = match dto.status.as_deref() { Some(s) => check(s, &STATUSES, "Statut")?, None => current.status.clone() };
    if let Some(p) = dto.probability { check_scale(p, "Probabilité")?; }
    if let Some(i) = dto.impact { check_scale(i, "Impact")?; }

    if let Some(Some(tid)) = dto.task_id {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }
    if let Some(Some(pid)) = dto.parent_risk_id {
        if pid == risk_id {
            return Err(OfficeError::Validation("Un risque ne peut pas découler de lui-même.".into()));
        }
        if !risk_in_project(&state, project_id, pid).await? {
            return Err(OfficeError::NotFound(format!("Risque {pid}")));
        }
    }

    let rows = sqlx::query(
        "UPDATE pm_risk SET \
            code = COALESCE($3, code), \
            title = COALESCE($4, title), \
            description = COALESCE($5, description), \
            category = $6, kind = $7, \
            probability = COALESCE($8, probability), \
            impact = COALESCE($9, impact), \
            probability_pct = CASE WHEN $10::boolean THEN $11::double precision ELSE probability_pct END, \
            monetary_impact = CASE WHEN $12::boolean THEN $13::double precision ELSE monetary_impact END, \
            status = $14, \
            owner_id = CASE WHEN $15::boolean THEN $16::uuid ELSE owner_id END, \
            trigger_signs = COALESCE($17, trigger_signs), \
            response_strategy = $18, \
            response_plan = COALESCE($19, response_plan), \
            residual_notes = COALESCE($20, residual_notes), \
            parent_risk_id = CASE WHEN $21::boolean THEN $22::uuid ELSE parent_risk_id END, \
            task_id = CASE WHEN $23::boolean THEN $24::uuid ELSE task_id END, \
            identified_at = CASE WHEN $25::boolean THEN $26::date ELSE identified_at END, \
            position = COALESCE($27, position), \
            -- Closing stamps the date; reopening takes it back.
            closed_at = CASE WHEN $14 = 'closed' THEN COALESCE(closed_at, now()) ELSE NULL END, \
            updated_at = now() \
         WHERE id = $1 AND project_id = $2",
    )
    .bind(risk_id).bind(project_id)
    .bind(dto.code.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.title.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.description.as_deref())
    .bind(&category).bind(&kind)
    .bind(dto.probability).bind(dto.impact)
    .bind(dto.probability_pct.is_some()).bind(dto.probability_pct.flatten())
    .bind(dto.monetary_impact.is_some()).bind(dto.monetary_impact.flatten())
    .bind(&status)
    .bind(dto.owner_id.is_some()).bind(dto.owner_id.flatten())
    .bind(dto.trigger_signs.as_deref())
    .bind(&strategy)
    .bind(dto.response_plan.as_deref())
    .bind(dto.residual_notes.as_deref())
    .bind(dto.parent_risk_id.is_some()).bind(dto.parent_risk_id.flatten())
    .bind(dto.task_id.is_some()).bind(dto.task_id.flatten())
    .bind(dto.identified_at.is_some()).bind(dto.identified_at.flatten())
    .bind(dto.position)
    .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Risque introuvable".into())); }

    let risk = fetch_one(&state, project_id, risk_id).await?;
    Ok(Json(json!({ "risk": with_emv(&risk) })))
}

/// DELETE /projects/:id/risks/:rid
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, risk_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query("DELETE FROM pm_risk WHERE id = $1 AND project_id = $2")
        .bind(risk_id).bind(project_id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Risque introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}
