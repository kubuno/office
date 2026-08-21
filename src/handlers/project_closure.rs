//! Closing a project, and what it taught.
//!
//! Almost everything a closure report should say is already recorded elsewhere:
//! which deliverables were accepted, which changes were decided, which risks
//! occurred, which issues are still open, which requirements were never verified.
//! What was missing is the act of **confronting the project with all of it before
//! declaring it over** — so the useful part of this module is not a form, it is a
//! set of checks run against the registers built by every other artifact.
//!
//! A project can still be closed with checks failing. It just cannot be closed
//! *quietly*: the reason is recorded alongside the closure.
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

const CATEGORIES: [&str; 8] = ["process", "technical", "people", "supplier",
                               "estimation", "communication", "risk", "other"];
const OUTCOMES: [&str; 3] = ["positive", "negative", "mixed"];
const LESSON_STATUSES: [&str; 3] = ["draft", "validated", "shared"];

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Closure {
    project_id:      Uuid,
    status:          String,
    objectives_met:  String,
    acceptance_note: String,
    handover_note:   String,
    loose_ends:      String,
    final_note:      String,
    override_reason: String,
    closed_on:       Option<NaiveDate>,
    closed_by:       Option<Uuid>,
    /// Named rather than an identifier, so the closure reads as a decision taken
    /// by somebody.
    closed_by_name:  Option<String>,
    updated_at:      chrono::DateTime<chrono::Utc>,
}

const LESSON_COLS: &str = "l.id, l.project_id, l.code, l.title, l.category, l.outcome, \
     l.situation, l.what_happened, l.recommendation, l.task_id, l.risk_id, l.issue_id, \
     l.change_id, l.status, l.recorded_by, l.recorded_on, l.position, l.created_at, l.updated_at, \
     t.name AS task_name, r.code AS risk_code, i.code AS issue_code, c.code AS change_code";

const LESSON_FROM: &str = "FROM pm_lesson l \
     LEFT JOIN tasks t ON t.id = l.task_id AND t.project_id = l.project_id \
     LEFT JOIN pm_risk r ON r.id = l.risk_id \
     LEFT JOIN pm_issue i ON i.id = l.issue_id \
     LEFT JOIN pm_change_request c ON c.id = l.change_id";

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Lesson {
    id:             Uuid,
    project_id:     Uuid,
    code:           String,
    title:          String,
    category:       String,
    outcome:        String,
    situation:      String,
    what_happened:  String,
    recommendation: String,
    task_id:        Option<Uuid>,
    task_name:      Option<String>,
    risk_id:        Option<Uuid>,
    risk_code:      Option<String>,
    issue_id:       Option<Uuid>,
    issue_code:     Option<String>,
    change_id:      Option<Uuid>,
    change_code:    Option<String>,
    status:         String,
    recorded_by:    Option<Uuid>,
    recorded_on:    NaiveDate,
    position:       i32,
    created_at:     chrono::DateTime<chrono::Utc>,
    updated_at:     chrono::DateTime<chrono::Utc>,
}

fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where T: Deserialize<'de>, D: Deserializer<'de> {
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct ClosureDto {
    pub objectives_met:  Option<String>,
    pub acceptance_note: Option<String>,
    pub handover_note:   Option<String>,
    pub loose_ends:      Option<String>,
    pub final_note:      Option<String>,
    pub status:          Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CloseDto {
    /// Required when checks are still failing. Closing anyway is allowed; closing
    /// anyway *silently* is not.
    pub override_reason: Option<String>,
    pub closed_on:       Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
pub struct LessonDto {
    pub code:           Option<String>,
    pub title:          Option<String>,
    pub category:       Option<String>,
    pub outcome:        Option<String>,
    pub situation:      Option<String>,
    pub what_happened:  Option<String>,
    pub recommendation: Option<String>,
    pub status:         Option<String>,
    pub position:       Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub task_id:   Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub risk_id:   Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub issue_id:  Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub change_id: Option<Option<Uuid>>,
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

async fn ensure_closure(state: &AppState, project_id: Uuid) -> Result<Closure> {
    if let Some(c) = sqlx::query_as::<_, Closure>(
        "SELECT project_id, status, objectives_met, acceptance_note, handover_note, \
                loose_ends, final_note, override_reason, closed_on, closed_by, (SELECT COALESCE(NULLIF(u.display_name, ''), u.email::text) FROM core.users u WHERE u.id = closed_by) AS closed_by_name, updated_at \
         FROM pm_closure WHERE project_id = $1",
    ).bind(project_id).fetch_optional(&state.db).await? {
        return Ok(c);
    }
    Ok(sqlx::query_as::<_, Closure>(
        "INSERT INTO pm_closure (project_id) VALUES ($1) \
         ON CONFLICT (project_id) DO UPDATE SET project_id = EXCLUDED.project_id \
         RETURNING project_id, status, objectives_met, acceptance_note, handover_note, \
                   loose_ends, final_note, override_reason, closed_on, closed_by, (SELECT COALESCE(NULLIF(u.display_name, ''), u.email::text) FROM core.users u WHERE u.id = closed_by) AS closed_by_name, updated_at",
    ).bind(project_id).fetch_one(&state.db).await?)
}

/// One thing to settle before the project can be called over.
struct Check {
    key:      &'static str,
    /// Blocking checks are things left undone. Advisory ones are things left
    /// unwritten — a project can end without them, but it will have taught nothing.
    blocking: bool,
    count:    i64,
}

async fn count(state: &AppState, sql: &str, project_id: Uuid) -> Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(sql).bind(project_id).fetch_one(&state.db).await?)
}

/// Run every check against the registers the other artifacts built.
async fn run_checks(state: &AppState, project_id: Uuid) -> Result<Vec<Check>> {
    Ok(vec![
        Check { key: "tasks_open", blocking: true, count: count(state,
            "SELECT COUNT(*) FROM tasks WHERE project_id = $1 \
             AND status NOT IN ('completed', 'cancelled') \
             AND NOT EXISTS (SELECT 1 FROM tasks c WHERE c.parent_id = tasks.id)", project_id).await? },
        Check { key: "deliverables_unaccepted", blocking: true, count: count(state,
            "SELECT COUNT(*) FROM pm_deliverable WHERE project_id = $1 \
             AND status NOT IN ('accepted', 'rejected')", project_id).await? },
        Check { key: "issues_open", blocking: true, count: count(state,
            "SELECT COUNT(*) FROM pm_issue WHERE project_id = $1 \
             AND status NOT IN ('resolved', 'closed')", project_id).await? },
        Check { key: "changes_undecided", blocking: true, count: count(state,
            "SELECT COUNT(*) FROM pm_change_request WHERE project_id = $1 \
             AND status IN ('submitted', 'assessing')", project_id).await? },
        Check { key: "contracts_open", blocking: true, count: count(state,
            "SELECT COUNT(*) FROM pm_procurement WHERE project_id = $1 \
             AND status IN ('awarded', 'active')", project_id).await? },
        Check { key: "quality_checks_pending", blocking: true, count: count(state,
            "SELECT COUNT(*) FROM pm_quality_check WHERE project_id = $1 \
             AND result IN ('pending', 'fail')", project_id).await? },
        // Advisory: the project can end, but something is left unsaid.
        Check { key: "risks_open", blocking: false, count: count(state,
            "SELECT COUNT(*) FROM pm_risk WHERE project_id = $1 \
             AND status NOT IN ('closed', 'occurred')", project_id).await? },
        Check { key: "requirements_unverified", blocking: false, count: count(state,
            "SELECT COUNT(*) FROM pm_requirement WHERE project_id = $1 \
             AND status NOT IN ('verified', 'deferred', 'rejected')", project_id).await? },
        Check { key: "charter_unapproved", blocking: false, count: count(state,
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM pm_charter \
                                      WHERE project_id = $1 AND status = 'approved') \
                    THEN 0 ELSE 1 END::bigint", project_id).await? },
        Check { key: "no_baseline", blocking: false, count: count(state,
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM project_baselines WHERE project_id = $1) \
                    THEN 0 ELSE 1 END::bigint", project_id).await? },
        // The one that decides whether the next project starts from scratch.
        Check { key: "no_lessons", blocking: false, count: count(state,
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM pm_lesson WHERE project_id = $1) \
                    THEN 0 ELSE 1 END::bigint", project_id).await? },
    ])
}

/// GET /projects/:id/closure
pub async fn overview(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let closure = ensure_closure(&state, project_id).await?;
    let checks = run_checks(&state, project_id).await?;

    let blocking: i64 = checks.iter().filter(|c| c.blocking && c.count > 0).count() as i64;
    let advisory: i64 = checks.iter().filter(|c| !c.blocking && c.count > 0).count() as i64;

    Ok(Json(json!({
        "closure": closure,
        "checks": checks.iter().map(|c| json!({
            "key": c.key, "blocking": c.blocking, "count": c.count, "ok": c.count == 0,
        })).collect::<Vec<_>>(),
        "summary": {
            "blocking": blocking,
            "advisory": advisory,
            "ready": blocking == 0,
            "closed": closure.status == "closed",
        },
    })))
}

/// PUT /projects/:id/closure
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<ClosureDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let current = ensure_closure(&state, project_id).await?;
    if current.status == "closed" {
        return Err(OfficeError::Validation(
            "Le projet est clos. Rouvrez-le pour modifier son dossier de clôture.".into()));
    }
    let status = match dto.status.as_deref() {
        // Closing goes through its own endpoint, which runs the checks.
        Some("closed") => return Err(OfficeError::Validation(
            "Utilisez la clôture du projet : elle vérifie ce qui reste à régler.".into())),
        Some(s) => Some(check_one(s, &["open", "closing"], "Statut")?),
        None => None,
    };

    let closure = sqlx::query_as::<_, Closure>(
        "UPDATE pm_closure SET \
            objectives_met = COALESCE($2, objectives_met), \
            acceptance_note = COALESCE($3, acceptance_note), \
            handover_note = COALESCE($4, handover_note), \
            loose_ends = COALESCE($5, loose_ends), \
            final_note = COALESCE($6, final_note), \
            status = COALESCE($7, status), updated_at = now() \
         WHERE project_id = $1 \
         RETURNING project_id, status, objectives_met, acceptance_note, handover_note, \
                   loose_ends, final_note, override_reason, closed_on, closed_by, (SELECT COALESCE(NULLIF(u.display_name, ''), u.email::text) FROM core.users u WHERE u.id = closed_by) AS closed_by_name, updated_at",
    )
    .bind(project_id)
    .bind(dto.objectives_met.as_deref()).bind(dto.acceptance_note.as_deref())
    .bind(dto.handover_note.as_deref()).bind(dto.loose_ends.as_deref())
    .bind(dto.final_note.as_deref()).bind(status.as_deref())
    .fetch_one(&state.db).await?;
    Ok(Json(json!({ "closure": closure })))
}

/// POST /projects/:id/closure/close — declare it over. Owner only.
pub async fn close(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    body: Option<Json<CloseDto>>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Owner).await?;
    let current = ensure_closure(&state, project_id).await?;
    if current.status == "closed" {
        return Err(OfficeError::Validation("Le projet est déjà clos.".into()));
    }
    let dto = body.map(|Json(d)| d);
    let reason = dto.as_ref().and_then(|d| d.override_reason.as_deref()).map(str::trim).unwrap_or("");

    let checks = run_checks(&state, project_id).await?;
    let blockers: Vec<&Check> = checks.iter().filter(|c| c.blocking && c.count > 0).collect();
    if !blockers.is_empty() && reason.is_empty() {
        let names: Vec<String> = blockers.iter()
            .map(|c| format!("{} ({})", label_of(c.key), c.count)).collect();
        return Err(OfficeError::Validation(format!(
            "Il reste {} point(s) à régler : {}. Clôturer malgré tout est possible, mais il faut \
             dire pourquoi — sinon ce qui reste ouvert disparaît avec le projet.",
            blockers.len(), names.join(", "))));
    }

    // The charter said what the project was for; the closure has to answer it.
    if current.objectives_met.trim().is_empty() {
        return Err(OfficeError::Validation(
            "Dites si les objectifs ont été atteints avant de clore : c'est la seule question \
             à laquelle la charte attend une réponse.".into()));
    }

    let mut tx = state.db.begin().await?;
    let closure = sqlx::query_as::<_, Closure>(
        "UPDATE pm_closure SET status = 'closed', closed_on = COALESCE($2, CURRENT_DATE), \
             closed_by = $3, override_reason = $4, updated_at = now() \
         WHERE project_id = $1 \
         RETURNING project_id, status, objectives_met, acceptance_note, handover_note, \
                   loose_ends, final_note, override_reason, closed_on, closed_by, (SELECT COALESCE(NULLIF(u.display_name, ''), u.email::text) FROM core.users u WHERE u.id = closed_by) AS closed_by_name, updated_at",
    )
    .bind(project_id)
    .bind(dto.as_ref().and_then(|d| d.closed_on))
    .bind(user.id).bind(reason)
    .fetch_one(&mut *tx).await?;
    // The project's own status follows: a closed project should not keep showing
    // as active in every list that reads it.
    sqlx::query("UPDATE projects SET status = 'completed', updated_at = now() WHERE id = $1")
        .bind(project_id).execute(&mut *tx).await?;
    tx.commit().await?;

    Ok(Json(json!({ "closure": closure, "closed_with_open_points": blockers.len() })))
}

/// POST /projects/:id/closure/reopen
pub async fn reopen(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Owner).await?;
    let current = ensure_closure(&state, project_id).await?;
    if current.status != "closed" {
        return Err(OfficeError::Validation("Le projet n'est pas clos.".into()));
    }
    let mut tx = state.db.begin().await?;
    let closure = sqlx::query_as::<_, Closure>(
        "UPDATE pm_closure SET status = 'closing', closed_on = NULL, closed_by = NULL, \
             updated_at = now() WHERE project_id = $1 \
         RETURNING project_id, status, objectives_met, acceptance_note, handover_note, \
                   loose_ends, final_note, override_reason, closed_on, closed_by, (SELECT COALESCE(NULLIF(u.display_name, ''), u.email::text) FROM core.users u WHERE u.id = closed_by) AS closed_by_name, updated_at",
    ).bind(project_id).fetch_one(&mut *tx).await?;
    sqlx::query("UPDATE projects SET status = 'active', updated_at = now() WHERE id = $1")
        .bind(project_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({ "closure": closure })))
}

/// Human name of a check, used in the refusal message so it names what is left.
fn label_of(key: &str) -> &'static str {
    match key {
        "tasks_open"              => "tâches non terminées",
        "deliverables_unaccepted" => "livrables non acceptés",
        "issues_open"             => "incidents ouverts",
        "changes_undecided"       => "demandes de changement non tranchées",
        "quality_checks_pending"  => "contrôles qualité en attente ou en échec",
        "contracts_open"          => "contrats encore ouverts",
        "risks_open"              => "risques encore ouverts",
        "requirements_unverified" => "exigences non vérifiées",
        "charter_unapproved"      => "charte non approuvée",
        "no_baseline"             => "aucun plan de référence",
        "no_lessons"              => "aucun enseignement consigné",
        _ => "point ouvert",
    }
}

// ── Lessons learned ──────────────────────────────────────────────────────────

/// GET /projects/:id/lessons
pub async fn list_lessons(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let lessons = sqlx::query_as::<_, Lesson>(&format!(
        "SELECT {LESSON_COLS} {LESSON_FROM} WHERE l.project_id = $1 \
         ORDER BY l.position, l.recorded_on DESC"
    )).bind(project_id).fetch_all(&state.db).await?;

    // A lesson without a recommendation is an anecdote: it says what happened and
    // leaves the next project to work out what to do about it.
    let without_recommendation: Vec<Value> = lessons.iter()
        .filter(|l| l.recommendation.trim().is_empty())
        .map(|l| json!({ "id": l.id, "code": l.code, "title": l.title }))
        .collect();

    Ok(Json(json!({
        "lessons": lessons,
        "summary": {
            "total": lessons.len(),
            "positive": lessons.iter().filter(|l| l.outcome == "positive").count(),
            "negative": lessons.iter().filter(|l| l.outcome == "negative").count(),
            "mixed":    lessons.iter().filter(|l| l.outcome == "mixed").count(),
            "validated": lessons.iter().filter(|l| l.status != "draft").count(),
            "without_recommendation": without_recommendation.len(),
        },
        "without_recommendation": without_recommendation,
    })))
}

async fn fetch_lesson(state: &AppState, project_id: Uuid, id: Uuid) -> Result<Lesson> {
    sqlx::query_as::<_, Lesson>(&format!(
        "SELECT {LESSON_COLS} {LESSON_FROM} WHERE l.id = $1 AND l.project_id = $2"
    )).bind(id).bind(project_id).fetch_optional(&state.db).await?
      .ok_or_else(|| OfficeError::NotFound("Enseignement introuvable".into()))
}

/// POST /projects/:id/lessons
pub async fn create_lesson(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<LessonDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Comment).await?;
    let title = dto.title.as_deref().map(str::trim).filter(|s| !s.is_empty())
        .ok_or_else(|| OfficeError::Validation("Nommez l'enseignement.".into()))?.to_string();
    let category = match dto.category.as_deref() { Some(c) => check_one(c, &CATEGORIES, "Catégorie")?, None => "process".into() };
    let outcome = match dto.outcome.as_deref() { Some(o) => check_one(o, &OUTCOMES, "Nature")?, None => "negative".into() };
    let status = match dto.status.as_deref() { Some(s) => check_one(s, &LESSON_STATUSES, "Statut")?, None => "draft".into() };
    if let Some(tid) = dto.task_id.flatten() {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }

    let code = match dto.code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None => {
            let highest: Option<i32> = sqlx::query_scalar(
                "SELECT MAX(CAST(substring(code FROM '^LE-([0-9]+)$') AS INT)) \
                 FROM pm_lesson WHERE project_id = $1 AND code ~ '^LE-[0-9]+$'",
            ).bind(project_id).fetch_one(&state.db).await?;
            format!("LE-{:02}", highest.unwrap_or(0) + 1)
        }
    };
    let position = match dto.position {
        Some(p) => p,
        None => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(position) FROM pm_lesson WHERE project_id = $1",
        ).bind(project_id).fetch_one(&state.db).await?.map_or(0, |m| m + 1),
    };

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO pm_lesson (project_id, code, title, category, outcome, situation, \
             what_happened, recommendation, task_id, risk_id, issue_id, change_id, \
             status, recorded_by, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) RETURNING id",
    )
    .bind(project_id).bind(&code).bind(&title).bind(&category).bind(&outcome)
    .bind(dto.situation.as_deref().unwrap_or_default())
    .bind(dto.what_happened.as_deref().unwrap_or_default())
    .bind(dto.recommendation.as_deref().unwrap_or_default())
    .bind(dto.task_id.flatten()).bind(dto.risk_id.flatten())
    .bind(dto.issue_id.flatten()).bind(dto.change_id.flatten())
    .bind(&status).bind(user.id).bind(position)
    .fetch_one(&state.db).await?;
    Ok(Json(json!({ "lesson": fetch_lesson(&state, project_id, id).await? })))
}

/// PATCH /projects/:id/lessons/:lid
pub async fn update_lesson(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, lesson_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<LessonDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let category = match dto.category.as_deref() { Some(c) => Some(check_one(c, &CATEGORIES, "Catégorie")?), None => None };
    let outcome = match dto.outcome.as_deref() { Some(o) => Some(check_one(o, &OUTCOMES, "Nature")?), None => None };
    let status = match dto.status.as_deref() { Some(s) => Some(check_one(s, &LESSON_STATUSES, "Statut")?), None => None };
    // Validating a lesson that carries no recommendation would publish an anecdote.
    if status.as_deref().is_some_and(|s| s != "draft") {
        let reco = match dto.recommendation.as_deref() {
            Some(r) => r.trim().to_string(),
            None => sqlx::query_scalar::<_, String>(
                "SELECT recommendation FROM pm_lesson WHERE id = $1 AND project_id = $2",
            ).bind(lesson_id).bind(project_id).fetch_optional(&state.db).await?.unwrap_or_default(),
        };
        if reco.is_empty() {
            return Err(OfficeError::Validation(
                "Indiquez la recommandation avant de valider : c'est la seule partie qui serve \
                 au projet suivant.".into()));
        }
    }
    if let Some(Some(tid)) = dto.task_id {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }

    let rows = sqlx::query(
        "UPDATE pm_lesson SET \
            code = COALESCE($3, code), title = COALESCE($4, title), \
            category = COALESCE($5, category), outcome = COALESCE($6, outcome), \
            situation = COALESCE($7, situation), what_happened = COALESCE($8, what_happened), \
            recommendation = COALESCE($9, recommendation), status = COALESCE($10, status), \
            task_id = CASE WHEN $11::boolean THEN $12::uuid ELSE task_id END, \
            risk_id = CASE WHEN $13::boolean THEN $14::uuid ELSE risk_id END, \
            issue_id = CASE WHEN $15::boolean THEN $16::uuid ELSE issue_id END, \
            change_id = CASE WHEN $17::boolean THEN $18::uuid ELSE change_id END, \
            position = COALESCE($19, position), updated_at = now() \
         WHERE id = $1 AND project_id = $2",
    )
    .bind(lesson_id).bind(project_id)
    .bind(dto.code.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.title.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(category.as_deref()).bind(outcome.as_deref())
    .bind(dto.situation.as_deref()).bind(dto.what_happened.as_deref())
    .bind(dto.recommendation.as_deref()).bind(status.as_deref())
    .bind(dto.task_id.is_some()).bind(dto.task_id.flatten())
    .bind(dto.risk_id.is_some()).bind(dto.risk_id.flatten())
    .bind(dto.issue_id.is_some()).bind(dto.issue_id.flatten())
    .bind(dto.change_id.is_some()).bind(dto.change_id.flatten())
    .bind(dto.position)
    .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Enseignement introuvable".into())); }
    Ok(Json(json!({ "lesson": fetch_lesson(&state, project_id, lesson_id).await? })))
}

/// DELETE /projects/:id/lessons/:lid
pub async fn delete_lesson(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, lesson_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query("DELETE FROM pm_lesson WHERE id = $1 AND project_id = $2")
        .bind(lesson_id).bind(project_id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Enseignement introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}
