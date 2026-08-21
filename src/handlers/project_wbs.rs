//! The work breakdown structure: the outline numbering that gives every work
//! package an address, and the dictionary that says what each package covers.
//!
//! The `tasks.wbs` column has existed since the first migration but was never
//! computed — every task carried an empty code. It is derived here from the tree
//! itself (parent, then position), so it cannot drift from the plan it numbers.
use axum::{extract::{Path, State}, Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgExecutor;
use std::collections::HashMap;

/// A task's place in the tree: identifier, sibling order, creation time.
type TreeNode = (Uuid, i32, chrono::DateTime<chrono::Utc>);
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    handlers::project_authz::{require_permission, task_in_project, Level},
    middleware::OfficeUser,
    state::AppState,
};

const DICT_COLS: &str = "id, task_id, code_of_account, statement_of_work, acceptance_criteria, \
     assumptions, exclusions, quality_requirements, risks, responsible, created_at, updated_at";

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct WbsDictionaryEntry {
    id:                   Uuid,
    task_id:              Uuid,
    code_of_account:      String,
    statement_of_work:    String,
    acceptance_criteria:  String,
    assumptions:          String,
    exclusions:           String,
    quality_requirements: String,
    risks:                String,
    responsible:          String,
    created_at:           chrono::DateTime<chrono::Utc>,
    updated_at:           chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDictionaryDto {
    pub code_of_account:      Option<String>,
    pub statement_of_work:    Option<String>,
    pub acceptance_criteria:  Option<String>,
    pub assumptions:          Option<String>,
    pub exclusions:           Option<String>,
    pub quality_requirements: Option<String>,
    pub risks:                Option<String>,
    pub responsible:          Option<String>,
}

/// Recompute every task's outline code from the tree and store it.
///
/// Public because the handlers that reshape the plan call it: a code is only
/// meaningful while it matches the tree, so moving a task renumbers its branch —
/// and every branch after it.
///
/// Takes an executor rather than the pool so it can join a caller's transaction:
/// renumbering must commit with the change that caused it, never on its own.
pub async fn renumber<'e, E>(exec: E, project_id: Uuid) -> Result<()>
where E: PgExecutor<'e> + Copy {
    let rows = sqlx::query_as::<_, (Uuid, Option<Uuid>, i32, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, parent_id, position, created_at FROM tasks WHERE project_id = $1",
    ).bind(project_id).fetch_all(exec).await?;
    if rows.is_empty() { return Ok(()); }

    let mut children: HashMap<Option<Uuid>, Vec<TreeNode>> = HashMap::new();
    for (id, parent, position, created) in &rows {
        children.entry(*parent).or_default().push((*id, *position, *created));
    }
    // Same order the plan is displayed in; creation time breaks ties so the
    // numbering is stable rather than dependent on how the rows came back.
    for list in children.values_mut() {
        list.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));
    }
    // A parent whose own parent is missing from this project would otherwise be
    // unreachable from the roots and keep a stale code.
    let known: std::collections::HashSet<Uuid> = rows.iter().map(|r| r.0).collect();
    let mut roots: Vec<TreeNode> = Vec::new();
    for (id, parent, position, created) in &rows {
        if parent.is_none_or(|p| !known.contains(&p)) {
            roots.push((*id, *position, *created));
        }
    }
    roots.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));

    let mut ids: Vec<Uuid> = Vec::with_capacity(rows.len());
    let mut codes: Vec<String> = Vec::with_capacity(rows.len());
    // Iterative walk: a deep plan must not blow the stack, and a cycle in
    // parent_id (which the database alone does not forbid) must not hang here.
    let mut stack: Vec<(Uuid, String)> = Vec::new();
    for (i, (id, _, _)) in roots.iter().enumerate().rev() {
        stack.push((*id, (i + 1).to_string()));
    }
    let mut seen = std::collections::HashSet::new();
    while let Some((id, code)) = stack.pop() {
        if !seen.insert(id) { continue; }
        ids.push(id);
        codes.push(code.clone());
        if let Some(kids) = children.get(&Some(id)) {
            for (i, (kid, _, _)) in kids.iter().enumerate().rev() {
                stack.push((*kid, format!("{code}.{}", i + 1)));
            }
        }
    }

    sqlx::query(
        "UPDATE tasks AS t SET wbs = v.code FROM UNNEST($1::uuid[], $2::text[]) AS v(id, code) \
         WHERE t.id = v.id AND t.wbs IS DISTINCT FROM v.code",
    ).bind(&ids).bind(&codes).execute(exec).await?;
    Ok(())
}

/// GET /projects/:id/wbs — the breakdown, numbered, with its dictionary.
pub async fn get_wbs(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    // Read-time renumbering keeps the codes true even for a plan reshaped before
    // this existed; it is a no-op when nothing changed.
    renumber(&state.db, project_id).await?;

    let rows = sqlx::query_as::<_, (Uuid, Option<Uuid>, i32, String, String, String, i32, Option<i64>)>(
        "SELECT t.id, t.parent_id, t.position, t.wbs, t.name, t.task_type, t.progress, \
                (SELECT COUNT(*) FROM pm_deliverable d WHERE d.task_id = t.id) \
         FROM tasks t WHERE t.project_id = $1 ORDER BY t.wbs",
    ).bind(project_id).fetch_all(&state.db).await?;

    let dict = sqlx::query_as::<_, WbsDictionaryEntry>(
        // Every column qualified: `tasks` carries an `id` too, and an unqualified
        // list is ambiguous the moment the join is there.
        &format!("SELECT {} FROM pm_wbs_dictionary d \
                  JOIN tasks t ON t.id = d.task_id WHERE t.project_id = $1",
                 DICT_COLS.split(", ").map(|c| format!("d.{c}"))
                          .collect::<Vec<_>>().join(", ")),
    ).bind(project_id).fetch_all(&state.db).await?;
    let by_task: HashMap<Uuid, &WbsDictionaryEntry> =
        dict.iter().map(|e| (e.task_id, e)).collect();

    let elements: Vec<Value> = rows.iter().map(|(id, parent, position, wbs, name, kind, progress, deliverables)| {
        json!({
            "id": id, "parent_id": parent, "position": position, "wbs": wbs,
            "name": name, "task_type": kind, "progress": progress,
            "deliverable_count": deliverables.unwrap_or(0),
            "has_dictionary": by_task.contains_key(id),
            "dictionary": by_task.get(id),
        })
    }).collect();

    Ok(Json(json!({ "elements": elements })))
}

/// POST /projects/:id/wbs/renumber — force a renumbering.
pub async fn renumber_endpoint(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    renumber(&state.db, project_id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// Create the entry if it is not there yet. Called only when something is being
/// written to it: an entry that exists but says nothing would be indistinguishable
/// from a work package somebody actually described, and "which packages are still
/// undefined" is precisely what the breakdown is meant to show.
async fn create_entry(state: &AppState, task_id: Uuid) -> Result<()> {
    sqlx::query(
        "INSERT INTO pm_wbs_dictionary (task_id) VALUES ($1) ON CONFLICT (task_id) DO NOTHING",
    ).bind(task_id).execute(&state.db).await?;
    Ok(())
}

/// GET /projects/:id/tasks/:tid/dictionary
pub async fn get_entry(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, task_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    if !task_in_project(&state, project_id, task_id).await? {
        return Err(OfficeError::NotFound(format!("Tâche {task_id}")));
    }
    let entry = sqlx::query_as::<_, WbsDictionaryEntry>(
        &format!("SELECT {DICT_COLS} FROM pm_wbs_dictionary WHERE task_id = $1"),
    ).bind(task_id).fetch_optional(&state.db).await?;
    Ok(Json(json!({ "entry": entry })))
}

/// PUT /projects/:id/tasks/:tid/dictionary
pub async fn update_entry(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, task_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateDictionaryDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    if !task_in_project(&state, project_id, task_id).await? {
        return Err(OfficeError::NotFound(format!("Tâche {task_id}")));
    }
    create_entry(&state, task_id).await?;

    let entry = sqlx::query_as::<_, WbsDictionaryEntry>(&format!(
        "UPDATE pm_wbs_dictionary SET \
            code_of_account = COALESCE($2, code_of_account), \
            statement_of_work = COALESCE($3, statement_of_work), \
            acceptance_criteria = COALESCE($4, acceptance_criteria), \
            assumptions = COALESCE($5, assumptions), \
            exclusions = COALESCE($6, exclusions), \
            quality_requirements = COALESCE($7, quality_requirements), \
            risks = COALESCE($8, risks), \
            responsible = COALESCE($9, responsible), \
            updated_at = now() \
         WHERE task_id = $1 RETURNING {DICT_COLS}"
    ))
    .bind(task_id)
    .bind(dto.code_of_account.as_deref()).bind(dto.statement_of_work.as_deref())
    .bind(dto.acceptance_criteria.as_deref()).bind(dto.assumptions.as_deref())
    .bind(dto.exclusions.as_deref()).bind(dto.quality_requirements.as_deref())
    .bind(dto.risks.as_deref()).bind(dto.responsible.as_deref())
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "entry": entry })))
}
