//! Deliverables — what the project actually hands over, followed through to
//! acceptance.
//!
//! A deliverable is the promise; the work package that produces it is only the
//! plan for keeping it. So a deliverable outlives its task (the link is optional
//! and cleared rather than cascaded), and "done" means someone with authority
//! accepted it — not that a bar turned green. Acceptance and rejection are
//! therefore owner-only acts with their own endpoints, recording who and when.
use axum::{
    extract::{Path, State},
    Extension, Json,
};
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

/// Projection shared by every response: the row plus the name of the work package
/// that produces it, resolved in the same query rather than one lookup per row.
const COLS: &str = "d.id, d.project_id, d.task_id, d.code, d.name, d.description, \
     d.acceptance_criteria, d.due_date, d.status, d.accepted_by, d.accepted_at, \
     d.rejection_reason, d.position, d.created_at, d.updated_at, t.name AS task_name, \
     (SELECT COALESCE(NULLIF(u.display_name, ''), u.email::text) \
        FROM core.users u WHERE u.id = d.accepted_by) AS accepted_by_name";

/// The statuses the table's CHECK constraint allows. Kept here so an unknown one
/// is refused with a readable message instead of surfacing as a database error.
const STATUSES: [&str; 5] = ["planned", "in_progress", "delivered", "accepted", "rejected"];

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Deliverable {
    id:                  Uuid,
    project_id:          Uuid,
    task_id:             Option<Uuid>,
    code:                String,
    name:                String,
    description:         String,
    acceptance_criteria: String,
    due_date:            Option<NaiveDate>,
    status:              String,
    accepted_by:         Option<Uuid>,
    accepted_at:         Option<chrono::DateTime<chrono::Utc>>,
    rejection_reason:    String,
    position:            i32,
    created_at:          chrono::DateTime<chrono::Utc>,
    updated_at:          chrono::DateTime<chrono::Utc>,
    /// Name of the linked work package, null when there is none (or none left).
    task_name:           Option<String>,
    /// Who accepted it, so the interface names a person rather than an identifier.
    accepted_by_name:    Option<String>,
}

/// Tells an absent field apart from one explicitly set to null: without this a
/// due date could be changed but never removed.
fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct CreateDto {
    pub name:                Option<String>,
    pub code:                Option<String>,
    pub description:         Option<String>,
    pub acceptance_criteria: Option<String>,
    pub due_date:            Option<NaiveDate>,
    pub task_id:             Option<Uuid>,
    pub status:              Option<String>,
    pub position:            Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDto {
    pub name:                Option<String>,
    pub code:                Option<String>,
    pub description:         Option<String>,
    pub acceptance_criteria: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_date:            Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub task_id:             Option<Option<Uuid>>,
    pub status:              Option<String>,
    pub position:            Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct RejectDto {
    pub reason: Option<String>,
}

/// The `FROM` clause every read shares, over a source relation aliased `d`.
///
/// The join carries `t.project_id = d.project_id` as well: writes already refuse
/// a foreign task, and this makes sure a row that predates that rule can never
/// surface another project's task name either.
fn select_from(source: &str) -> String {
    format!(
        "SELECT {COLS} FROM {source} d \
         LEFT JOIN tasks t ON t.id = d.task_id AND t.project_id = d.project_id"
    )
}

fn validate_status(status: &str) -> Result<String> {
    let s = status.trim();
    if !STATUSES.contains(&s) {
        return Err(OfficeError::Validation(format!(
            "Statut inconnu : « {s} ». Valeurs acceptées : {}.",
            STATUSES.join(", ")
        )));
    }
    Ok(s.to_string())
}

/// A deliverable may only name a work package of the same project: without this
/// check, naming a project you own and a task you do not would reach straight
/// into someone else's schedule.
async fn validate_task(state: &AppState, project_id: Uuid, task_id: Option<Uuid>) -> Result<()> {
    if let Some(tid) = task_id {
        if !task_in_project(state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!(
                "Tâche introuvable dans ce projet : {tid}"
            )));
        }
    }
    Ok(())
}

/// GET /projects/:id/deliverables
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let deliverables = sqlx::query_as::<_, Deliverable>(&format!(
        "{} WHERE d.project_id = $1 ORDER BY d.position, d.created_at",
        select_from("pm_deliverable")
    ))
    .bind(project_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "deliverables": deliverables })))
}

/// POST /projects/:id/deliverables
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<CreateDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;

    // Everything is validated before the first write: a rejected request must
    // leave nothing behind.
    let name = dto
        .name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| OfficeError::Validation("Nommez le livrable.".into()))?;
    let status = match dto.status.as_deref() {
        Some(s) => validate_status(s)?,
        None => "planned".to_string(),
    };
    validate_task(&state, project_id, dto.task_id).await?;

    // Same reasoning as requirements: a deliverable needs a reference to be cited,
    // and the number comes from the highest already issued, never from the count.
    let code = match dto.code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None => {
            let highest: Option<i32> = sqlx::query_scalar(
                "SELECT MAX(CAST(substring(code FROM '^L-([0-9]+)$') AS INT)) \
                 FROM pm_deliverable WHERE project_id = $1 AND code ~ '^L-[0-9]+$'",
            ).bind(project_id).fetch_one(&state.db).await?;
            format!("L-{:02}", highest.unwrap_or(0) + 1)
        }
    };
    let description = dto.description.unwrap_or_default();
    let acceptance_criteria = dto.acceptance_criteria.unwrap_or_default();

    // Appended at the end unless told otherwise, so adding a deliverable does not
    // land it in the middle of the ones already listed.
    let deliverable = sqlx::query_as::<_, Deliverable>(&format!(
        "WITH ins AS ( \
             INSERT INTO pm_deliverable \
                 (project_id, task_id, code, name, description, acceptance_criteria, \
                  due_date, status, position) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, \
                     COALESCE($9::int, \
                              (SELECT MAX(position) + 1 FROM pm_deliverable WHERE project_id = $1), \
                              0)) \
             RETURNING * \
         ) {}",
        select_from("ins")
    ))
    .bind(project_id)
    .bind(dto.task_id)
    .bind(&code)
    .bind(&name)
    .bind(&description)
    .bind(&acceptance_criteria)
    .bind(dto.due_date)
    .bind(&status)
    .bind(dto.position)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "deliverable": deliverable })))
}

/// PATCH /projects/:id/deliverables/:did
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, deliverable_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;

    // A name may be left alone, but not emptied: an unnamed promise is unusable.
    let name = match dto.name {
        Some(n) => {
            let n = n.trim().to_string();
            if n.is_empty() {
                return Err(OfficeError::Validation("Nommez le livrable.".into()));
            }
            Some(n)
        }
        None => None,
    };
    let status = match dto.status.as_deref() {
        Some(s) => {
            let s = validate_status(s)?;
            // Accepting or rejecting is the owner's decision and is recorded with
            // who decided and when. Reaching those states through an ordinary
            // field update would sidestep both the check and the record.
            if s == "accepted" || s == "rejected" {
                return Err(OfficeError::Validation(
                    "Un livrable ne devient « accepté » ou « refusé » que par l'acceptation ou le refus du propriétaire.".into(),
                ));
            }
            Some(s)
        }
        None => None,
    };
    validate_task(&state, project_id, dto.task_id.flatten()).await?;

    // `id = $1 AND project_id = $2`: an identifier belonging to another project
    // must fall through to 404 rather than update someone else's row.
    let deliverable = sqlx::query_as::<_, Deliverable>(&format!(
        "WITH upd AS ( \
             UPDATE pm_deliverable SET \
                 code = COALESCE($3::varchar, code), \
                 name = COALESCE($4::varchar, name), \
                 description = COALESCE($5::text, description), \
                 acceptance_criteria = COALESCE($6::text, acceptance_criteria), \
                 due_date = CASE WHEN $7::boolean THEN $8::date ELSE due_date END, \
                 task_id = CASE WHEN $9::boolean THEN $10::uuid ELSE task_id END, \
                 status = COALESCE($11::varchar, status), \
                 position = COALESCE($12::int, position), \
                 accepted_by = CASE WHEN $11::varchar IS NOT NULL \
                                     AND $11::varchar <> 'accepted' \
                                    THEN NULL ELSE accepted_by END, \
                 accepted_at = CASE WHEN $11::varchar IS NOT NULL \
                                     AND $11::varchar <> 'accepted' \
                                    THEN NULL ELSE accepted_at END, \
                 rejection_reason = CASE WHEN $11::varchar IS NOT NULL \
                                          AND $11::varchar <> 'rejected' \
                                         THEN '' ELSE rejection_reason END, \
                 updated_at = now() \
             WHERE id = $1 AND project_id = $2 \
             RETURNING * \
         ) {}",
        select_from("upd")
    ))
    .bind(deliverable_id)
    .bind(project_id)
    .bind(dto.code.as_deref().map(str::trim))
    .bind(name.as_deref())
    .bind(dto.description.as_deref())
    .bind(dto.acceptance_criteria.as_deref())
    .bind(dto.due_date.is_some())
    .bind(dto.due_date.flatten())
    .bind(dto.task_id.is_some())
    .bind(dto.task_id.flatten())
    .bind(status.as_deref())
    .bind(dto.position)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound("Livrable introuvable".into()))?;

    Ok(Json(json!({ "deliverable": deliverable })))
}

/// DELETE /projects/:id/deliverables/:did
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, deliverable_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query("DELETE FROM pm_deliverable WHERE id = $1 AND project_id = $2")
        .bind(deliverable_id)
        .bind(project_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(OfficeError::NotFound("Livrable introuvable".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

/// POST /projects/:id/deliverables/:did/accept — the hand-over is signed off.
/// Owner only: accepting one's own work is not acceptance.
pub async fn accept(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, deliverable_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Owner).await?;
    let deliverable = sqlx::query_as::<_, Deliverable>(&format!(
        "WITH upd AS ( \
             UPDATE pm_deliverable SET \
                 status = 'accepted', accepted_by = $3, accepted_at = now(), \
                 rejection_reason = '', updated_at = now() \
             WHERE id = $1 AND project_id = $2 \
             RETURNING * \
         ) {}",
        select_from("upd")
    ))
    .bind(deliverable_id)
    .bind(project_id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound("Livrable introuvable".into()))?;
    Ok(Json(json!({ "deliverable": deliverable })))
}

/// POST /projects/:id/deliverables/:did/reject — sent back, with the reason it
/// was sent back. A rejection without a reason tells the team nothing.
pub async fn reject(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, deliverable_id)): Path<(Uuid, Uuid)>,
    body: Option<Json<RejectDto>>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Owner).await?;
    let reason = body
        .and_then(|Json(d)| d.reason)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            OfficeError::Validation("Indiquez le motif du refus du livrable.".into())
        })?;

    let deliverable = sqlx::query_as::<_, Deliverable>(&format!(
        "WITH upd AS ( \
             UPDATE pm_deliverable SET \
                 status = 'rejected', rejection_reason = $3, \
                 accepted_by = NULL, accepted_at = NULL, updated_at = now() \
             WHERE id = $1 AND project_id = $2 \
             RETURNING * \
         ) {}",
        select_from("upd")
    ))
    .bind(deliverable_id)
    .bind(project_id)
    .bind(&reason)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound("Livrable introuvable".into()))?;
    Ok(Json(json!({ "deliverable": deliverable })))
}
