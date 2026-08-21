//! The issue log — what is going wrong right now.
//!
//! The register next door holds risks: things that *might* happen, ranked by
//! probability. This one holds what already has. Keeping the two apart is the
//! whole point — a forecast crowded with events that already occurred stops
//! being a forecast, and a log padded with hypotheses stops being actionable.
//!
//! The bridge between them is [`materialize`]: a risk that came true becomes an
//! issue and the risk is marked `occurred`, in one transaction, so the two
//! registers can never disagree about what happened.
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

/// Projection shared by every response: the row plus the three names the
/// interface needs to be readable — the person on the hook, the work package
/// affected, and the risk this issue is the realisation of. All resolved by the
/// joins below, so a hundred issues still cost one query rather than three
/// hundred and one.
const COLS: &str = "i.id, i.project_id, i.code, i.title, i.description, i.severity, \
     i.status, i.owner_id, i.due_date, i.resolution, i.resolved_at, i.risk_id, \
     i.task_id, i.position, i.created_at, i.updated_at, \
     COALESCE(NULLIF(u.display_name, ''), u.email::text) AS owner_name, \
     t.name AS task_name, r.code AS risk_code, r.title AS risk_title";

/// The statuses the table's CHECK constraint allows. Kept here so an unknown one
/// is refused with a readable message instead of surfacing as a database error.
const STATUSES: [&str; 4] = ["open", "in_progress", "resolved", "closed"];

/// The two statuses that mean "still going on". An issue in one of them is what
/// `open` and `overdue` count, and what the list sorts to the top.
const SQL_LIVE: &str = "('open', 'in_progress')";

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Issue {
    id:          Uuid,
    project_id:  Uuid,
    code:        String,
    title:       String,
    description: String,
    /// 1 (minor) to 5 (critical), the same scale the risk register uses for
    /// impact — which is what lets a materialised risk carry its impact over.
    severity:    i32,
    status:      String,
    owner_id:    Option<Uuid>,
    due_date:    Option<NaiveDate>,
    resolution:  String,
    resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    risk_id:     Option<Uuid>,
    task_id:     Option<Uuid>,
    position:    i32,
    created_at:  chrono::DateTime<chrono::Utc>,
    updated_at:  chrono::DateTime<chrono::Utc>,
    /// Who is accountable, named rather than shown as an identifier.
    owner_name:  Option<String>,
    /// Name of the affected work package, null when there is none (or none left).
    task_name:   Option<String>,
    /// The foreseen risk this issue came from, when it had been foreseen.
    risk_code:   Option<String>,
    risk_title:  Option<String>,
}

/// The aggregate the header reads. Computed by the database in one pass rather
/// than by counting the rows we just fetched, so `overdue` compares against the
/// database's own `CURRENT_DATE` — the same clock that stored the due dates.
#[derive(Debug, sqlx::FromRow)]
struct Summary {
    total:      i64,
    open_count: i64,
    overdue:    i64,
    sev1:       i64,
    sev2:       i64,
    sev3:       i64,
    sev4:       i64,
    sev5:       i64,
}

/// What a materialised risk hands over to the issue it becomes.
#[derive(Debug, sqlx::FromRow)]
struct RiskSource {
    title:       String,
    description: String,
    impact:      i32,
    task_id:     Option<Uuid>,
}

/// Tells an absent field apart from one explicitly set to null: without this an
/// owner or a due date could be changed but never removed.
fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct CreateDto {
    pub title:       Option<String>,
    pub code:        Option<String>,
    pub description: Option<String>,
    pub severity:    Option<i32>,
    pub status:      Option<String>,
    pub owner_id:    Option<Uuid>,
    pub due_date:    Option<NaiveDate>,
    pub resolution:  Option<String>,
    pub risk_id:     Option<Uuid>,
    pub task_id:     Option<Uuid>,
    pub position:    Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDto {
    pub title:       Option<String>,
    pub code:        Option<String>,
    pub description: Option<String>,
    pub severity:    Option<i32>,
    pub status:      Option<String>,
    pub resolution:  Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_date:    Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub owner_id:    Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub risk_id:     Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub task_id:     Option<Option<Uuid>>,
    pub position:    Option<i32>,
}

/// The `FROM` clause every read shares, over a source relation aliased `i`.
///
/// Both project-scoped joins carry `= i.project_id` as well: writes already
/// refuse a foreign task or risk, and this makes sure a row that predates that
/// rule can never surface another project's names either.
fn select_from(source: &str) -> String {
    format!(
        "SELECT {COLS} FROM {source} i \
         LEFT JOIN core.users u ON u.id = i.owner_id \
         LEFT JOIN tasks t ON t.id = i.task_id AND t.project_id = i.project_id \
         LEFT JOIN pm_risk r ON r.id = i.risk_id AND r.project_id = i.project_id"
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

/// Severity shares the risk register's 1-to-5 scale. Refused here with the value
/// that was sent, rather than as a constraint violation from the database.
fn validate_severity(severity: i32) -> Result<i32> {
    if !(1..=5).contains(&severity) {
        return Err(OfficeError::Validation(format!(
            "Gravité invalide : « {severity} ». Valeurs acceptées : 1, 2, 3, 4, 5 \
             (1 = mineure, 5 = critique)."
        )));
    }
    Ok(severity)
}

fn validate_title(title: &str) -> Result<String> {
    let t = title.trim();
    if t.is_empty() {
        return Err(OfficeError::Validation("Nommez l'incident.".into()));
    }
    Ok(t.to_string())
}

/// An issue may only name a work package of its own project: without this check,
/// naming a project you own and a task you do not would reach straight into
/// someone else's schedule.
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

/// Same guard for the risk an issue claims to be the realisation of: attaching
/// one project's issue to another project's risk would falsify both registers.
async fn validate_risk(state: &AppState, project_id: Uuid, risk_id: Option<Uuid>) -> Result<()> {
    if let Some(rid) = risk_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pm_risk WHERE id = $1 AND project_id = $2)",
        )
        .bind(rid)
        .bind(project_id)
        .fetch_one(&state.db)
        .await?;
        if !exists {
            return Err(OfficeError::NotFound(format!(
                "Risque introuvable dans ce projet : {rid}"
            )));
        }
    }
    Ok(())
}

/// The next free reference, taken from the highest number already issued and
/// never from the count: deleting `I-02` must not hand its number to the next
/// issue, or two documents end up citing different things by the same name.
async fn next_code<'e, E>(executor: E, project_id: Uuid) -> Result<String>
where
    E: sqlx::PgExecutor<'e>,
{
    let highest: Option<i32> = sqlx::query_scalar(
        "SELECT MAX(CAST(substring(code FROM '^I-([0-9]+)$') AS INT)) \
         FROM pm_issue WHERE project_id = $1 AND code ~ '^I-[0-9]+$'",
    )
    .bind(project_id)
    .fetch_one(executor)
    .await?;
    Ok(format!("I-{:02}", highest.unwrap_or(0) + 1))
}

/// Read one issue, scoped to its project so an identifier borrowed from another
/// project reads as "not found" rather than leaking a row.
async fn fetch_issue(state: &AppState, project_id: Uuid, issue_id: Uuid) -> Result<Issue> {
    sqlx::query_as::<_, Issue>(&format!(
        "{} WHERE i.id = $1 AND i.project_id = $2",
        select_from("pm_issue")
    ))
    .bind(issue_id)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound("Incident introuvable".into()))
}

/// GET /projects/:id/issues
///
/// Live issues first, then the worst first, then in the order the team arranged
/// them: a log that opens on last month's closed items buries today's problem.
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;

    let issues = sqlx::query_as::<_, Issue>(&format!(
        "{} WHERE i.project_id = $1 \
         ORDER BY (i.status NOT IN {SQL_LIVE}), i.severity DESC, i.position, i.created_at",
        select_from("pm_issue")
    ))
    .bind(project_id)
    .fetch_all(&state.db)
    .await?;

    // `overdue` is the number worth acting on: still open, and already late.
    // Closed items are never late — they are done.
    let summary = sqlx::query_as::<_, Summary>(&format!(
        "SELECT COUNT(*) AS total, \
                COUNT(*) FILTER (WHERE status IN {SQL_LIVE}) AS open_count, \
                COUNT(*) FILTER (WHERE status IN {SQL_LIVE} \
                                   AND due_date IS NOT NULL \
                                   AND due_date < CURRENT_DATE) AS overdue, \
                COUNT(*) FILTER (WHERE severity = 1) AS sev1, \
                COUNT(*) FILTER (WHERE severity = 2) AS sev2, \
                COUNT(*) FILTER (WHERE severity = 3) AS sev3, \
                COUNT(*) FILTER (WHERE severity = 4) AS sev4, \
                COUNT(*) FILTER (WHERE severity = 5) AS sev5 \
         FROM pm_issue WHERE project_id = $1"
    ))
    .bind(project_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({
        "issues": issues,
        "summary": {
            "total":   summary.total,
            "open":    summary.open_count,
            "overdue": summary.overdue,
            "by_severity": {
                "1": summary.sev1, "2": summary.sev2, "3": summary.sev3,
                "4": summary.sev4, "5": summary.sev5,
            },
        },
    })))
}

/// POST /projects/:id/issues
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<CreateDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;

    // Everything is validated before the first write: a rejected request must
    // leave nothing behind.
    let title = validate_title(dto.title.as_deref().unwrap_or_default())?;
    let severity = match dto.severity {
        Some(s) => validate_severity(s)?,
        None => 3,
    };
    let status = match dto.status.as_deref() {
        Some(s) => validate_status(s)?,
        None => "open".to_string(),
    };
    let resolution = dto.resolution.unwrap_or_default();
    // Same rule as the update path: an issue filed as already closed still has
    // to say how it was closed.
    if status == "closed" && resolution.trim().is_empty() {
        return Err(OfficeError::Validation(
            "Indiquez la résolution avant de clore l'incident.".into(),
        ));
    }
    validate_task(&state, project_id, dto.task_id).await?;
    validate_risk(&state, project_id, dto.risk_id).await?;

    let code = match dto.code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None => next_code(&state.db, project_id).await?,
    };
    let description = dto.description.unwrap_or_default();

    // Appended at the end unless told otherwise, so filing an issue does not
    // land it in the middle of the ones already listed.
    let issue = sqlx::query_as::<_, Issue>(&format!(
        "WITH ins AS ( \
             INSERT INTO pm_issue \
                 (project_id, code, title, description, severity, status, owner_id, \
                  due_date, resolution, risk_id, task_id, resolved_at, position) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, \
                     CASE WHEN $6::varchar IN ('resolved', 'closed') THEN now() END, \
                     COALESCE($12::int, \
                              (SELECT MAX(position) + 1 FROM pm_issue WHERE project_id = $1), \
                              0)) \
             RETURNING * \
         ) {}",
        select_from("ins")
    ))
    .bind(project_id)
    .bind(&code)
    .bind(&title)
    .bind(&description)
    .bind(severity)
    .bind(&status)
    .bind(dto.owner_id)
    .bind(dto.due_date)
    .bind(&resolution)
    .bind(dto.risk_id)
    .bind(dto.task_id)
    .bind(dto.position)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "issue": issue })))
}

/// PATCH /projects/:id/issues/:iid
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, issue_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;

    // Read first, and scoped to the project: it settles the 404 before any
    // validation message can hint that the row exists somewhere else, and the
    // closing rule below needs the resolution already on file.
    let current = fetch_issue(&state, project_id, issue_id).await?;

    // A title may be left alone, but not emptied: an unnamed incident is
    // untraceable in a meeting.
    let title = match dto.title.as_deref() {
        Some(t) => Some(validate_title(t)?),
        None => None,
    };
    let severity = match dto.severity {
        Some(s) => Some(validate_severity(s)?),
        None => None,
    };
    let status = match dto.status.as_deref() {
        Some(s) => Some(validate_status(s)?),
        None => None,
    };
    // Closing without saying how is how an organisation forgets what it learned.
    if status.as_deref() == Some("closed") {
        let effective = dto.resolution.as_deref().unwrap_or(&current.resolution);
        if effective.trim().is_empty() {
            return Err(OfficeError::Validation(
                "Indiquez la résolution avant de clore l'incident.".into(),
            ));
        }
    }
    validate_task(&state, project_id, dto.task_id.flatten()).await?;
    validate_risk(&state, project_id, dto.risk_id.flatten()).await?;

    // `id = $1 AND project_id = $2`: an identifier belonging to another project
    // must fall through to 404 rather than update someone else's row.
    //
    // The four `CASE WHEN $n::boolean` pairs are the "double option": absent
    // leaves the column alone, explicit null clears it. A plain COALESCE would
    // make an owner or a due date impossible to remove once set.
    let issue = sqlx::query_as::<_, Issue>(&format!(
        "WITH upd AS ( \
             UPDATE pm_issue SET \
                 code = COALESCE($3::varchar, code), \
                 title = COALESCE($4::varchar, title), \
                 description = COALESCE($5::text, description), \
                 severity = COALESCE($6::int, severity), \
                 status = COALESCE($7::varchar, status), \
                 resolution = COALESCE($8::text, resolution), \
                 due_date = CASE WHEN $9::boolean THEN $10::date ELSE due_date END, \
                 owner_id = CASE WHEN $11::boolean THEN $12::uuid ELSE owner_id END, \
                 risk_id = CASE WHEN $13::boolean THEN $14::uuid ELSE risk_id END, \
                 task_id = CASE WHEN $15::boolean THEN $16::uuid ELSE task_id END, \
                 resolved_at = CASE \
                     WHEN $7::varchar IN ('resolved', 'closed') THEN COALESCE(resolved_at, now()) \
                     WHEN $7::varchar IS NOT NULL THEN NULL \
                     ELSE resolved_at END, \
                 position = COALESCE($17::int, position), \
                 updated_at = now() \
             WHERE id = $1 AND project_id = $2 \
             RETURNING * \
         ) {}",
        select_from("upd")
    ))
    .bind(issue_id)
    .bind(project_id)
    .bind(dto.code.as_deref().map(str::trim))
    .bind(title.as_deref())
    .bind(dto.description.as_deref())
    .bind(severity)
    .bind(status.as_deref())
    .bind(dto.resolution.as_deref())
    .bind(dto.due_date.is_some())
    .bind(dto.due_date.flatten())
    .bind(dto.owner_id.is_some())
    .bind(dto.owner_id.flatten())
    .bind(dto.risk_id.is_some())
    .bind(dto.risk_id.flatten())
    .bind(dto.task_id.is_some())
    .bind(dto.task_id.flatten())
    .bind(dto.position)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound("Incident introuvable".into()))?;

    Ok(Json(json!({ "issue": issue })))
}

/// DELETE /projects/:id/issues/:iid
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, issue_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query("DELETE FROM pm_issue WHERE id = $1 AND project_id = $2")
        .bind(issue_id)
        .bind(project_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(OfficeError::NotFound("Incident introuvable".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

/// POST /projects/:id/risks/:rid/materialize — the risk came true.
///
/// Filing the issue and marking the risk `occurred` are one act: done in two
/// requests, a crash between them leaves a register claiming the risk is still
/// hypothetical while the log says it happened. Hence the transaction.
///
/// Pressing it twice must not file the same incident twice, so an issue already
/// attached to the risk is returned as it stands, with `existing: true` — and
/// the risk is left untouched, since a risk that occurred and was then closed
/// must not be dragged back to `occurred` by a second click.
pub async fn materialize(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, risk_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;

    // Scoped to the project: another project's risk is "not found", never a
    // usable source for an issue filed here.
    let risk = sqlx::query_as::<_, RiskSource>(
        "SELECT title, description, impact, task_id FROM pm_risk \
         WHERE id = $1 AND project_id = $2",
    )
    .bind(risk_id)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound("Risque introuvable dans ce projet".into()))?;

    if let Some(existing_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM pm_issue WHERE risk_id = $1 AND project_id = $2 \
         ORDER BY created_at LIMIT 1",
    )
    .bind(risk_id)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await?
    {
        let issue = fetch_issue(&state, project_id, existing_id).await?;
        return Ok(Json(json!({ "issue": issue, "existing": true })));
    }

    // The risk's own task link is only carried over if it still points inside
    // this project — a stale link must not smuggle a foreign task into the log.
    let task_id = match risk.task_id {
        Some(tid) if task_in_project(&state, project_id, tid).await? => Some(tid),
        _ => None,
    };
    // The impact that was forecast becomes the severity that is now real, which
    // is why both registers share the 1-to-5 scale.
    let severity = validate_severity(risk.impact)?;

    let mut tx = state.db.begin().await?;
    let code = next_code(&mut *tx, project_id).await?;

    let issue = sqlx::query_as::<_, Issue>(&format!(
        "WITH ins AS ( \
             INSERT INTO pm_issue \
                 (project_id, code, title, description, severity, status, risk_id, task_id, position) \
             VALUES ($1, $2, $3, $4, $5, 'open', $6, $7, \
                     (SELECT COALESCE(MAX(position) + 1, 0) FROM pm_issue WHERE project_id = $1)) \
             RETURNING * \
         ) {}",
        select_from("ins")
    ))
    .bind(project_id)
    .bind(&code)
    .bind(&risk.title)
    .bind(&risk.description)
    .bind(severity)
    .bind(risk_id)
    .bind(task_id)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE pm_risk SET status = 'occurred', updated_at = now() \
         WHERE id = $1 AND project_id = $2",
    )
    .bind(risk_id)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(json!({ "issue": issue, "existing": false })))
}
