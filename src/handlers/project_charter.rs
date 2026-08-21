//! The project charter — what the project is for, who authorises it, and what
//! success will mean.
//!
//! Its one distinguishing rule: an APPROVED charter is read-only. Changing it is
//! a deliberate act that files a dated revision and reopens the document, rather
//! than a silent edit to something people have already signed off.
use axum::{extract::{Path, State}, Extension, Json};
use chrono::NaiveDate;
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    handlers::project_authz::{require_permission, Level},
    middleware::OfficeUser,
    state::AppState,
};

const COLS: &str = "id, project_id, purpose, business_case, objectives, success_criteria, \
     high_level_requirements, assumptions, constraints, risks_summary, budget_summary, \
     sponsor, pm_name, pm_authority, status, approved_by, approved_at, created_at, updated_at";

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Charter {
    id:                      Uuid,
    project_id:              Uuid,
    purpose:                 String,
    business_case:           String,
    objectives:              String,
    success_criteria:        String,
    high_level_requirements: String,
    assumptions:             String,
    constraints:             String,
    risks_summary:           String,
    budget_summary:          String,
    sponsor:                 String,
    pm_name:                 String,
    pm_authority:            String,
    status:                  String,
    approved_by:             Option<Uuid>,
    approved_at:             Option<chrono::DateTime<chrono::Utc>>,
    created_at:              chrono::DateTime<chrono::Utc>,
    updated_at:              chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct CharterMilestone {
    id:          Uuid,
    charter_id:  Uuid,
    name:        String,
    target_date: Option<NaiveDate>,
    position:    i32,
    task_id:     Option<Uuid>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateCharterDto {
    pub purpose:                 Option<String>,
    pub business_case:           Option<String>,
    pub objectives:              Option<String>,
    pub success_criteria:        Option<String>,
    pub high_level_requirements: Option<String>,
    pub assumptions:             Option<String>,
    pub constraints:             Option<String>,
    pub risks_summary:           Option<String>,
    pub budget_summary:          Option<String>,
    pub sponsor:                 Option<String>,
    pub pm_name:                 Option<String>,
    pub pm_authority:            Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReviseDto { pub reason: Option<String> }

/// Tells an absent field apart from one explicitly set to null: without this a
/// target date could be changed but never removed.
fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where T: Deserialize<'de>, D: Deserializer<'de> {
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct MilestoneDto {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub target_date: Option<Option<NaiveDate>>,
    pub position: Option<i32>,
}

/// Fetch the charter, creating an empty one on first read so the interface always
/// has something to edit rather than a special "not created yet" state.
async fn ensure_charter(state: &AppState, project_id: Uuid) -> Result<Charter> {
    if let Some(c) = sqlx::query_as::<_, Charter>(
        &format!("SELECT {COLS} FROM pm_charter WHERE project_id = $1"),
    ).bind(project_id).fetch_optional(&state.db).await? {
        return Ok(c);
    }
    Ok(sqlx::query_as::<_, Charter>(
        &format!("INSERT INTO pm_charter (project_id) VALUES ($1) \
                  ON CONFLICT (project_id) DO UPDATE SET project_id = EXCLUDED.project_id \
                  RETURNING {COLS}"),
    ).bind(project_id).fetch_one(&state.db).await?)
}

async fn milestones_of(state: &AppState, charter_id: Uuid) -> Result<Vec<CharterMilestone>> {
    Ok(sqlx::query_as::<_, CharterMilestone>(
        "SELECT id, charter_id, name, target_date, position, task_id \
         FROM pm_charter_milestone WHERE charter_id = $1 ORDER BY position, target_date NULLS LAST",
    ).bind(charter_id).fetch_all(&state.db).await?)
}


/// Resolve a user's display name, so the interface can say who approved a charter
/// rather than showing an identifier.
async fn name_of(state: &AppState, id: Option<Uuid>) -> Option<String> {
    let id = id?;
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT COALESCE(NULLIF(u.display_name, ''), u.email::text) FROM core.users u WHERE u.id = $1",
    ).bind(id).fetch_optional(&state.db).await.ok().flatten().flatten()
}

/// GET /projects/:id/charter
pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let charter = ensure_charter(&state, project_id).await?;
    let milestones = milestones_of(&state, charter.id).await?;
    let revisions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pm_charter_revision WHERE charter_id = $1",
    ).bind(charter.id).fetch_one(&state.db).await?;
    let approved_by_name = name_of(&state, charter.approved_by).await;
    Ok(Json(json!({
        "charter": charter, "milestones": milestones,
        "revision_count": revisions, "approved_by_name": approved_by_name,
    })))
}

/// PUT /projects/:id/charter — refused while the charter is approved.
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<UpdateCharterDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let current = ensure_charter(&state, project_id).await?;
    refuse_if_approved(&current)?;

    let charter = sqlx::query_as::<_, Charter>(&format!(
        "UPDATE pm_charter SET \
            purpose = COALESCE($2, purpose), \
            business_case = COALESCE($3, business_case), \
            objectives = COALESCE($4, objectives), \
            success_criteria = COALESCE($5, success_criteria), \
            high_level_requirements = COALESCE($6, high_level_requirements), \
            assumptions = COALESCE($7, assumptions), \
            constraints = COALESCE($8, constraints), \
            risks_summary = COALESCE($9, risks_summary), \
            budget_summary = COALESCE($10, budget_summary), \
            sponsor = COALESCE($11, sponsor), \
            pm_name = COALESCE($12, pm_name), \
            pm_authority = COALESCE($13, pm_authority), \
            updated_at = now() \
         WHERE project_id = $1 RETURNING {COLS}"
    ))
    .bind(project_id)
    .bind(dto.purpose.as_deref()).bind(dto.business_case.as_deref())
    .bind(dto.objectives.as_deref()).bind(dto.success_criteria.as_deref())
    .bind(dto.high_level_requirements.as_deref()).bind(dto.assumptions.as_deref())
    .bind(dto.constraints.as_deref()).bind(dto.risks_summary.as_deref())
    .bind(dto.budget_summary.as_deref()).bind(dto.sponsor.as_deref())
    .bind(dto.pm_name.as_deref()).bind(dto.pm_authority.as_deref())
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "charter": charter })))
}

/// POST /projects/:id/charter/approve — the sign-off. Owner only.
pub async fn approve(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Owner).await?;
    let current = ensure_charter(&state, project_id).await?;
    // A charter with no purpose says nothing; approving it would be a rubber stamp.
    if current.purpose.trim().is_empty() {
        return Err(OfficeError::Validation(
            "Renseignez au moins l'objet de la charte avant de l'approuver.".into(),
        ));
    }
    let charter = sqlx::query_as::<_, Charter>(&format!(
        "UPDATE pm_charter SET status = 'approved', approved_by = $2, approved_at = now(), \
             updated_at = now() WHERE project_id = $1 RETURNING {COLS}"
    )).bind(project_id).bind(user.id).fetch_one(&state.db).await?;
    Ok(Json(json!({ "charter": charter })))
}

/// POST /projects/:id/charter/revise — reopen an approved charter, keeping what
/// was approved as a dated revision.
pub async fn revise(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    body: Option<Json<ReviseDto>>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Owner).await?;
    let current = ensure_charter(&state, project_id).await?;
    if current.status != "approved" {
        return Err(OfficeError::Validation("La charte n'est pas approuvée.".into()));
    }
    let reason = body.and_then(|Json(d)| d.reason).unwrap_or_default();

    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO pm_charter_revision (charter_id, snapshot, reason, revised_by) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(current.id)
    .bind(serde_json::to_value(&current).unwrap_or_else(|_| json!({})))
    .bind(&reason).bind(user.id)
    .execute(&mut *tx).await?;

    let charter = sqlx::query_as::<_, Charter>(&format!(
        "UPDATE pm_charter SET status = 'draft', approved_by = NULL, approved_at = NULL, \
             updated_at = now() WHERE project_id = $1 RETURNING {COLS}"
    )).bind(project_id).fetch_one(&mut *tx).await?;
    tx.commit().await?;

    Ok(Json(json!({ "charter": charter })))
}

/// GET /projects/:id/charter/revisions
pub async fn revisions(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let charter = ensure_charter(&state, project_id).await?;
    let rows = sqlx::query_as::<_, (Uuid, Value, String, Option<Uuid>, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, snapshot, reason, revised_by, revised_at FROM pm_charter_revision \
         WHERE charter_id = $1 ORDER BY revised_at DESC",
    ).bind(charter.id).fetch_all(&state.db).await?;
    let mut revisions: Vec<Value> = Vec::with_capacity(rows.len());
    for (id, snapshot, reason, by, at) in rows {
        let name = name_of(&state, by).await;
        revisions.push(json!({
            "id": id, "snapshot": snapshot, "reason": reason,
            "revised_by": by, "revised_by_name": name, "revised_at": at,
        }));
    }
    Ok(Json(json!({ "revisions": revisions })))
}

/// An approved charter is what people signed off on — its milestones included.
fn refuse_if_approved(charter: &Charter) -> Result<()> {
    if charter.status == "approved" {
        return Err(OfficeError::Validation(
            "La charte est approuvée. Rouvrez-la pour la modifier : une révision datée sera conservée.".into(),
        ));
    }
    Ok(())
}

/// POST /projects/:id/charter/milestones
pub async fn add_milestone(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<MilestoneDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let charter = ensure_charter(&state, project_id).await?;
    refuse_if_approved(&charter)?;
    let name = dto.name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
        .ok_or_else(|| OfficeError::Validation("Nommez le jalon.".into()))?;
    // Appended at the end unless told otherwise, so adding a milestone does not
    // land it in the middle of the ones already listed.
    let position = match dto.position {
        Some(p) => p,
        None => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(position) FROM pm_charter_milestone WHERE charter_id = $1",
        ).bind(charter.id).fetch_one(&state.db).await?.map_or(0, |m| m + 1),
    };
    let ms = sqlx::query_as::<_, CharterMilestone>(
        "INSERT INTO pm_charter_milestone (charter_id, name, target_date, position) \
         VALUES ($1, $2, $3, $4) RETURNING id, charter_id, name, target_date, position, task_id",
    ).bind(charter.id).bind(&name).bind(dto.target_date.flatten()).bind(position)
     .fetch_one(&state.db).await?;
    Ok(Json(json!({ "milestone": ms })))
}

/// PATCH /projects/:id/charter/milestones/:mid
pub async fn update_milestone(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, milestone_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<MilestoneDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let charter = ensure_charter(&state, project_id).await?;
    refuse_if_approved(&charter)?;
    let ms = sqlx::query_as::<_, CharterMilestone>(
        "UPDATE pm_charter_milestone SET \
             name = COALESCE($3, name), \
             target_date = CASE WHEN $4 THEN $5 ELSE target_date END, \
             position = COALESCE($6, position) \
         WHERE id = $1 AND charter_id = $2 \
         RETURNING id, charter_id, name, target_date, position, task_id",
    )
    .bind(milestone_id).bind(charter.id)
    .bind(dto.name.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.target_date.is_some()).bind(dto.target_date.flatten())
    .bind(dto.position)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Jalon introuvable".into()))?;
    Ok(Json(json!({ "milestone": ms })))
}

/// DELETE /projects/:id/charter/milestones/:mid
pub async fn delete_milestone(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, milestone_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let charter = ensure_charter(&state, project_id).await?;
    refuse_if_approved(&charter)?;
    let rows = sqlx::query("DELETE FROM pm_charter_milestone WHERE id = $1 AND charter_id = $2")
        .bind(milestone_id).bind(charter.id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Jalon introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}

/// POST /projects/:id/charter/milestones/generate — turn the charter's milestones
/// into real milestones in the schedule.
///
/// The link is remembered on each charter milestone, so pressing this twice
/// updates what it already created instead of piling up duplicates.
pub async fn generate_milestones(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let charter = ensure_charter(&state, project_id).await?;
    let milestones = milestones_of(&state, charter.id).await?;

    let mut created = 0usize;
    let mut updated = 0usize;
    let mut tx = state.db.begin().await?;
    for (i, ms) in milestones.iter().enumerate() {
        // Still there? A task deleted since generation is created afresh.
        let existing: Option<Uuid> = match ms.task_id {
            Some(tid) => sqlx::query_scalar(
                "SELECT id FROM tasks WHERE id = $1 AND project_id = $2",
            ).bind(tid).bind(project_id).fetch_optional(&mut *tx).await?,
            None => None,
        };
        match existing {
            Some(tid) => {
                sqlx::query(
                    "UPDATE tasks SET name = $2, start_date = COALESCE($3, start_date) WHERE id = $1",
                ).bind(tid).bind(&ms.name).bind(ms.target_date).execute(&mut *tx).await?;
                updated += 1;
            }
            None => {
                let new_id: Uuid = sqlx::query_scalar(
                    "INSERT INTO tasks (project_id, name, task_type, duration_days, position, start_date) \
                     VALUES ($1, $2, 'milestone', 0, $3, $4) RETURNING id",
                )
                .bind(project_id).bind(&ms.name)
                .bind(1000 + i as i32).bind(ms.target_date)
                .fetch_one(&mut *tx).await?;
                sqlx::query("UPDATE pm_charter_milestone SET task_id = $2 WHERE id = $1")
                    .bind(ms.id).bind(new_id).execute(&mut *tx).await?;
                created += 1;
            }
        }
    }
    tx.commit().await?;

    Ok(Json(json!({ "created": created, "updated": updated })))
}
