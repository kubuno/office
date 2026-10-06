//! Access control for projects — the single place that answers "may this person
//! do this here?".
//!
//! A project is reachable by its owner and by the people it was shared with, each
//! at a level: `view` < `comment` < `edit`, with the owner above all three. Before
//! this module the levels were stored but never read: every handler wrote its own
//! `owner_id = $2` check, so a "reader" could edit and a collaborator was locked
//! out of perfectly readable pages. Handlers now call [`require_permission`] first
//! and drop their own checks.
use uuid::Uuid;

use crate::{errors::{OfficeError, Result}, state::AppState};

/// What someone may do with a project, from least to most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    View,
    Comment,
    Edit,
    Owner,
}

impl Level {
    fn from_permission(p: &str) -> Level {
        match p {
            "edit"    => Level::Edit,
            "comment" => Level::Comment,
            _         => Level::View,
        }
    }
}

/// The level `user_id` holds on `project_id`, or `None` when the project does not
/// exist or was never shared with them — the two are deliberately indistinguishable
/// to callers, so a stranger cannot probe which project ids exist.
pub async fn level_of(state: &AppState, project_id: Uuid, user_id: Uuid) -> Result<Option<Level>> {
    if sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2)",
    )
    .bind(project_id).bind(user_id)
    .fetch_one(&state.db).await?
    {
        return Ok(Some(Level::Owner));
    }
    let permission: Option<String> = sqlx::query_scalar(
        "SELECT permission FROM project_collaborators WHERE project_id = $1 AND user_id = $2",
    )
    .bind(project_id).bind(user_id)
    .fetch_optional(&state.db).await?;
    Ok(permission.as_deref().map(Level::from_permission))
}

/// Refuse the request unless `user_id` holds at least `min` on `project_id`, and
/// return the level actually held (handlers sometimes branch on `Owner`).
///
/// Call it FIRST in a handler, before any other query: checking late leaks whether
/// a task or a resource exists through validation errors, and does needless work.
pub async fn require_permission(
    state: &AppState,
    project_id: Uuid,
    user_id: Uuid,
    min: Level,
) -> Result<Level> {
    match level_of(state, project_id, user_id).await? {
        // Unknown project, or one never shared with them: say "not found" rather
        // than "forbidden", which would confirm the project exists.
        None => Err(OfficeError::NotFound("Projet introuvable".into())),
        Some(level) if level < min => Err(OfficeError::Forbidden),
        Some(level) => Ok(level),
    }
}

/// True when the task belongs to the project. The project-level check alone is not
/// enough for `/projects/:id/tasks/:tid`-style routes: without this, naming a
/// project you own and a task you do not reaches straight into someone else's data.
pub async fn task_in_project(state: &AppState, project_id: Uuid, task_id: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = $1 AND project_id = $2)",
    )
    .bind(task_id).bind(project_id)
    .fetch_one(&state.db).await?)
}

/// Same guard for resources.
pub async fn resource_in_project(state: &AppState, project_id: Uuid, resource_id: Uuid) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM project_resources WHERE id = $1 AND project_id = $2)",
    )
    .bind(resource_id).bind(project_id)
    .fetch_one(&state.db).await?)
}

/// Refuse unless every listed task belongs to the project.
pub async fn require_tasks_in_project(
    state: &AppState,
    project_id: Uuid,
    task_ids: &[Uuid],
) -> Result<()> {
    for id in task_ids {
        if !task_in_project(state, project_id, *id).await? {
            return Err(OfficeError::NotFound(format!("Tâche introuvable dans ce projet : {id}")));
        }
    }
    Ok(())
}
