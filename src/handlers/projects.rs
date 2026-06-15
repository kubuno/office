use axum::{
    extract::{Path, Query, State},
    Json,
};
use bytes::Bytes;
use serde_json::{json, Value};
use uuid::Uuid;
use axum::Extension;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::project::*,
    state::AppState,
};

// ── Projects CRUD ─────────────────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Query(q): Query<ListProjectsQuery>,
) -> Result<Json<Value>> {
    let limit  = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let trashed = q.trashed.unwrap_or(false);

    let rows: Vec<Project> = if let Some(ref search) = q.search {
        sqlx::query_as::<_, Project>(
            r#"SELECT id, owner_id, title, file_id, description, color, status,
                      start_date, end_date, is_starred, is_trashed, trashed_at,
                      last_edited_by, created_at, updated_at
               FROM projects
               WHERE owner_id = $1 AND is_trashed = $2 AND title ILIKE $3
               ORDER BY updated_at DESC LIMIT $4 OFFSET $5"#,
        )
        .bind(user.id).bind(trashed).bind(format!("%{search}%"))
        .bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    } else if q.starred.unwrap_or(false) {
        sqlx::query_as::<_, Project>(
            r#"SELECT id, owner_id, title, file_id, description, color, status,
                      start_date, end_date, is_starred, is_trashed, trashed_at,
                      last_edited_by, created_at, updated_at
               FROM projects
               WHERE owner_id = $1 AND is_starred = TRUE AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        )
        .bind(user.id).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    } else if q.recent.unwrap_or(false) {
        sqlx::query_as::<_, Project>(
            r#"SELECT id, owner_id, title, file_id, description, color, status,
                      start_date, end_date, is_starred, is_trashed, trashed_at,
                      last_edited_by, created_at, updated_at
               FROM projects
               WHERE owner_id = $1 AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        )
        .bind(user.id).bind(limit.min(20)).bind(offset)
        .fetch_all(&state.db).await?
    } else {
        sqlx::query_as::<_, Project>(
            r#"SELECT id, owner_id, title, file_id, description, color, status,
                      start_date, end_date, is_starred, is_trashed, trashed_at,
                      last_edited_by, created_at, updated_at
               FROM projects
               WHERE owner_id = $1 AND is_trashed = $2
               ORDER BY updated_at DESC LIMIT $3 OFFSET $4"#,
        )
        .bind(user.id).bind(trashed).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    };

    Ok(Json(json!({ "projects": rows, "total": rows.len() })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    body: Option<Json<CreateProjectDto>>,
) -> Result<Json<Value>> {
    let dto = body.map(|Json(d)| d).unwrap_or(CreateProjectDto {
        title: None, description: None, color: None,
        start_date: None, end_date: None,
    });
    let title = dto.title.unwrap_or_else(|| "Nouveau projet".to_string());
    let color = dto.color.unwrap_or_else(|| "#1a73e8".to_string());

    let mut project = sqlx::query_as::<_, Project>(
        r#"INSERT INTO projects (owner_id, title, description, color, start_date, end_date)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, owner_id, title, file_id, description, color, status,
                      start_date, end_date, is_starred, is_trashed, trashed_at,
                      last_edited_by, created_at, updated_at"#,
    )
    .bind(user.id)
    .bind(&title)
    .bind(dto.description.as_deref().unwrap_or(""))
    .bind(&color)
    .bind(dto.start_date)
    .bind(dto.end_date)
    .fetch_one(&state.db)
    .await?;

    // Register in Files (best-effort)
    register_project_in_files(&state, &mut project).await;

    Ok(Json(json!({ "project": project })))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let project = sqlx::query_as::<_, Project>(
        r#"SELECT id, owner_id, title, file_id, description, color, status,
                      start_date, end_date, is_starred, is_trashed, trashed_at,
                      last_edited_by, created_at, updated_at
           FROM projects WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id).bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Projet introuvable".into()))?;

    let tasks = sqlx::query_as::<_, Task>(
        r#"SELECT id, project_id, parent_id, position, wbs, name, description,
                  status, priority, task_type, start_date, end_date, duration_days,
                  progress, early_start, early_finish, late_start, late_finish,
                  total_float, is_critical, cpm_dirty, created_at, updated_at
           FROM tasks WHERE project_id = $1 ORDER BY position ASC"#,
    )
    .bind(id)
    .fetch_all(&state.db).await?;

    let deps = sqlx::query_as::<_, TaskDependency>(
        r#"SELECT id, project_id, from_task_id, to_task_id, dep_type, lag_days
           FROM task_dependencies WHERE project_id = $1"#,
    )
    .bind(id)
    .fetch_all(&state.db).await?;

    let resources = sqlx::query_as::<_, ProjectResource>(
        r#"SELECT id, project_id, name, role, color, capacity, created_at
           FROM project_resources WHERE project_id = $1 ORDER BY created_at ASC"#,
    )
    .bind(id)
    .fetch_all(&state.db).await?;

    let assignments = sqlx::query_as::<_, TaskAssignment>(
        r#"SELECT ta.id, ta.task_id, ta.resource_id, ta.units
           FROM task_assignments ta
           JOIN tasks t ON t.id = ta.task_id
           WHERE t.project_id = $1"#,
    )
    .bind(id)
    .fetch_all(&state.db).await?;

    Ok(Json(json!({
        "project": project,
        "tasks": tasks,
        "dependencies": deps,
        "resources": resources,
        "assignments": assignments,
    })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateProjectDto>,
) -> Result<Json<Value>> {
    let project = sqlx::query_as::<_, Project>(
        r#"UPDATE projects SET
             title       = COALESCE($3, title),
             description = COALESCE($4, description),
             color       = COALESCE($5, color),
             status      = COALESCE($6, status),
             start_date  = COALESCE($7, start_date),
             end_date    = COALESCE($8, end_date),
             is_starred  = COALESCE($9, is_starred),
             last_edited_by = $10
           WHERE id = $1 AND owner_id = $2
           RETURNING id, owner_id, title, file_id, description, color, status,
                      start_date, end_date, is_starred, is_trashed, trashed_at,
                      last_edited_by, created_at, updated_at"#,
    )
    .bind(id).bind(user.id)
    .bind(dto.title.as_deref())
    .bind(dto.description.as_deref())
    .bind(dto.color.as_deref())
    .bind(dto.status.as_deref())
    .bind(dto.start_date)
    .bind(dto.end_date)
    .bind(dto.is_starred)
    .bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Projet introuvable".into()))?;

    // Sync to Files (best-effort)
    sync_project_to_files(&state, &project).await;

    Ok(Json(json!({ "project": project })))
}

pub async fn trash(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("UPDATE projects SET is_trashed = TRUE, trashed_at = NOW() WHERE id = $1 AND owner_id = $2")
        .bind(id).bind(user.id)
        .execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn restore(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("UPDATE projects SET is_trashed = FALSE, trashed_at = NULL WHERE id = $1 AND owner_id = $2")
        .bind(id).bind(user.id)
        .execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query("DELETE FROM projects WHERE id = $1 AND owner_id = $2 AND is_trashed = TRUE")
        .bind(id).bind(user.id)
        .execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn duplicate(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let source: Project = sqlx::query_as::<_, Project>(
        "SELECT id, owner_id, title, file_id, description, color, status, start_date, end_date,
                is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at
         FROM projects WHERE id = $1 AND owner_id = $2 AND is_trashed = FALSE",
    )
    .bind(id).bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Projet introuvable".into()))?;

    let new_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (owner_id, title, description, color, status, start_date, end_date)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(user.id)
    .bind(format!("{} (copie)", source.title))
    .bind(&source.description)
    .bind(&source.color)
    .bind(&source.status)
    .bind(source.start_date)
    .bind(source.end_date)
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "id": new_id })))
}

// ── Tasks ─────────────────────────────────────────────────────────────────────

pub async fn create_task(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<CreateTaskDto>,
) -> Result<Json<Value>> {
    // Verify ownership
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2)")
        .bind(project_id).bind(user.id)
        .fetch_one(&state.db).await?;
    if !exists { return Err(OfficeError::NotFound("Projet introuvable".into())); }

    let position = dto.position.unwrap_or_else(|| {
        // Will be fixed below
        0
    });
    let name = dto.name.unwrap_or_else(|| "Nouvelle tâche".to_string());
    let task_type = dto.task_type.as_deref().unwrap_or("task");
    let duration = dto.duration_days.unwrap_or(1);

    let task = sqlx::query_as::<_, Task>(
        r#"INSERT INTO tasks (project_id, parent_id, position, name, task_type, start_date, duration_days)
           VALUES ($1, $2,
             COALESCE($3, (SELECT COALESCE(MAX(position)+1, 0) FROM tasks WHERE project_id = $1 AND parent_id IS NOT DISTINCT FROM $2)),
             $4, $5, $6, $7)
           RETURNING id, project_id, parent_id, position, wbs, name, description,
                     status, priority, task_type, start_date, end_date, duration_days,
                     progress, early_start, early_finish, late_start, late_finish,
                     total_float, is_critical, cpm_dirty, created_at, updated_at"#,
    )
    .bind(project_id).bind(dto.parent_id)
    .bind(if position == 0 { None::<i32> } else { Some(position) })
    .bind(&name).bind(task_type).bind(dto.start_date).bind(duration)
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "task": task })))
}

pub async fn update_task(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, task_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateTaskDto>,
) -> Result<Json<Value>> {
    let task = sqlx::query_as::<_, Task>(
        r#"UPDATE tasks SET
             name          = COALESCE($3, name),
             description   = COALESCE($4, description),
             status        = COALESCE($5, status),
             priority      = COALESCE($6, priority),
             task_type     = COALESCE($7, task_type),
             start_date    = COALESCE($8, start_date),
             end_date      = COALESCE($9, end_date),
             duration_days = COALESCE($10, duration_days),
             progress      = COALESCE($11, progress),
             position      = COALESCE($12, position),
             wbs           = COALESCE($13, wbs)
           WHERE id = $1 AND project_id = $2
             AND EXISTS (SELECT 1 FROM projects WHERE id = $2 AND owner_id = $14)
           RETURNING id, project_id, parent_id, position, wbs, name, description,
                     status, priority, task_type, start_date, end_date, duration_days,
                     progress, early_start, early_finish, late_start, late_finish,
                     total_float, is_critical, cpm_dirty, created_at, updated_at"#,
    )
    .bind(task_id).bind(project_id)
    .bind(dto.name.as_deref())
    .bind(dto.description.as_deref())
    .bind(dto.status.as_deref())
    .bind(dto.priority.as_deref())
    .bind(dto.task_type.as_deref())
    .bind(dto.start_date)
    .bind(dto.end_date)
    .bind(dto.duration_days)
    .bind(dto.progress)
    .bind(dto.position)
    .bind(dto.wbs.as_deref())
    .bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Tâche introuvable".into()))?;

    Ok(Json(json!({ "task": task })))
}

pub async fn delete_task(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, task_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    sqlx::query(
        r#"DELETE FROM tasks WHERE id = $1 AND project_id = $2
           AND EXISTS (SELECT 1 FROM projects WHERE id = $2 AND owner_id = $3)"#,
    )
    .bind(task_id).bind(project_id).bind(user.id)
    .execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Dependencies ──────────────────────────────────────────────────────────────

pub async fn create_dependency(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<CreateDependencyDto>,
) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2)")
        .bind(project_id).bind(user.id)
        .fetch_one(&state.db).await?;
    if !exists { return Err(OfficeError::NotFound("Projet introuvable".into())); }

    if dto.from_task_id == dto.to_task_id {
        return Err(OfficeError::Validation("Une tâche ne peut pas dépendre d'elle-même".into()));
    }

    let dep = sqlx::query_as::<_, TaskDependency>(
        r#"INSERT INTO task_dependencies (project_id, from_task_id, to_task_id, dep_type, lag_days)
           VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (from_task_id, to_task_id) DO UPDATE SET
             dep_type = EXCLUDED.dep_type,
             lag_days = EXCLUDED.lag_days
           RETURNING id, project_id, from_task_id, to_task_id, dep_type, lag_days"#,
    )
    .bind(project_id).bind(dto.from_task_id).bind(dto.to_task_id)
    .bind(dto.dep_type.as_deref().unwrap_or("FS"))
    .bind(dto.lag_days.unwrap_or(0))
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "dependency": dep })))
}

pub async fn delete_dependency(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, dep_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    sqlx::query(
        r#"DELETE FROM task_dependencies WHERE id = $1 AND project_id = $2
           AND EXISTS (SELECT 1 FROM projects WHERE id = $2 AND owner_id = $3)"#,
    )
    .bind(dep_id).bind(project_id).bind(user.id)
    .execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Resources ─────────────────────────────────────────────────────────────────

pub async fn list_resources(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2)")
        .bind(project_id).bind(user.id)
        .fetch_one(&state.db).await?;
    if !exists { return Err(OfficeError::NotFound("Projet introuvable".into())); }

    let resources = sqlx::query_as::<_, ProjectResource>(
        "SELECT id, project_id, name, role, color, capacity, created_at FROM project_resources WHERE project_id = $1 ORDER BY created_at ASC",
    )
    .bind(project_id)
    .fetch_all(&state.db).await?;

    Ok(Json(json!({ "resources": resources })))
}

pub async fn create_resource(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<CreateResourceDto>,
) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2)")
        .bind(project_id).bind(user.id)
        .fetch_one(&state.db).await?;
    if !exists { return Err(OfficeError::NotFound("Projet introuvable".into())); }

    let resource = sqlx::query_as::<_, ProjectResource>(
        r#"INSERT INTO project_resources (project_id, name, role, color, capacity)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, project_id, name, role, color, capacity, created_at"#,
    )
    .bind(project_id)
    .bind(&dto.name)
    .bind(dto.role.as_deref().unwrap_or(""))
    .bind(dto.color.as_deref().unwrap_or("#5f6368"))
    .bind(dto.capacity.unwrap_or(1.0))
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "resource": resource })))
}

pub async fn update_resource(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, resource_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateResourceDto>,
) -> Result<Json<Value>> {
    let resource = sqlx::query_as::<_, ProjectResource>(
        r#"UPDATE project_resources SET
             name     = COALESCE($3, name),
             role     = COALESCE($4, role),
             color    = COALESCE($5, color),
             capacity = COALESCE($6, capacity)
           WHERE id = $1 AND project_id = $2
             AND EXISTS (SELECT 1 FROM projects WHERE id = $2 AND owner_id = $7)
           RETURNING id, project_id, name, role, color, capacity, created_at"#,
    )
    .bind(resource_id).bind(project_id)
    .bind(dto.name.as_deref())
    .bind(dto.role.as_deref())
    .bind(dto.color.as_deref())
    .bind(dto.capacity)
    .bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Ressource introuvable".into()))?;

    Ok(Json(json!({ "resource": resource })))
}

pub async fn delete_resource(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, resource_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    sqlx::query(
        r#"DELETE FROM project_resources WHERE id = $1 AND project_id = $2
           AND EXISTS (SELECT 1 FROM projects WHERE id = $2 AND owner_id = $3)"#,
    )
    .bind(resource_id).bind(project_id).bind(user.id)
    .execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Assignments ───────────────────────────────────────────────────────────────

pub async fn assign_resource(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, task_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<AssignResourceDto>,
) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2)"
    )
    .bind(project_id).bind(user.id)
    .fetch_one(&state.db).await?;
    if !exists { return Err(OfficeError::NotFound("Projet introuvable".into())); }

    let assignment = sqlx::query_as::<_, TaskAssignment>(
        r#"INSERT INTO task_assignments (task_id, resource_id, units)
           VALUES ($1, $2, $3)
           ON CONFLICT (task_id, resource_id) DO UPDATE SET units = EXCLUDED.units
           RETURNING id, task_id, resource_id, units"#,
    )
    .bind(task_id).bind(dto.resource_id)
    .bind(dto.units.unwrap_or(1.0))
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "assignment": assignment })))
}

pub async fn unassign_resource(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, task_id, resource_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>> {
    sqlx::query(
        r#"DELETE FROM task_assignments
           WHERE task_id = $1 AND resource_id = $2
             AND EXISTS (SELECT 1 FROM projects WHERE id = $3 AND owner_id = $4)"#,
    )
    .bind(task_id).bind(resource_id).bind(project_id).bind(user.id)
    .execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── CPM Compute ───────────────────────────────────────────────────────────────

pub async fn compute_cpm(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2)")
        .bind(project_id).bind(user.id)
        .fetch_one(&state.db).await?;
    if !exists { return Err(OfficeError::NotFound("Projet introuvable".into())); }

    let tasks = sqlx::query_as::<_, Task>(
        r#"SELECT id, project_id, parent_id, position, wbs, name, description,
                  status, priority, task_type, start_date, end_date, duration_days,
                  progress, early_start, early_finish, late_start, late_finish,
                  total_float, is_critical, cpm_dirty, created_at, updated_at
           FROM tasks WHERE project_id = $1 AND task_type != 'summary' ORDER BY position"#,
    )
    .bind(project_id)
    .fetch_all(&state.db).await?;

    let deps = sqlx::query_as::<_, TaskDependency>(
        "SELECT id, project_id, from_task_id, to_task_id, dep_type, lag_days FROM task_dependencies WHERE project_id = $1"
    )
    .bind(project_id)
    .fetch_all(&state.db).await?;

    if tasks.is_empty() {
        return Ok(Json(json!({ "ok": true, "tasks": [] })));
    }

    // Kahn's topological sort + CPM
    let n = tasks.len();
    let task_idx: std::collections::HashMap<Uuid, usize> = tasks.iter().enumerate()
        .map(|(i, t)| (t.id, i)).collect();

    let mut in_degree = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    let mut rev_adj: Vec<Vec<usize>> = vec![vec![]; n];

    for dep in &deps {
        if let (Some(&from), Some(&to)) = (task_idx.get(&dep.from_task_id), task_idx.get(&dep.to_task_id)) {
            adj[from].push(to);
            rev_adj[to].push(from);
            in_degree[to] += 1;
        }
    }

    // Forward pass (ES/EF)
    let mut early_start  = vec![0i32; n];
    let mut early_finish = vec![0i32; n];

    for (i, t) in tasks.iter().enumerate() {
        early_finish[i] = t.duration_days;
    }

    let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut topo = Vec::with_capacity(n);
    let mut temp_indegree = in_degree.clone();

    while let Some(u) = queue.pop_front() {
        topo.push(u);
        for &v in &adj[u] {
            // EF of u is the min ES of v (FS dependency)
            let new_es = early_finish[u];
            if new_es > early_start[v] {
                early_start[v] = new_es;
                early_finish[v] = early_start[v] + tasks[v].duration_days;
            }
            temp_indegree[v] -= 1;
            if temp_indegree[v] == 0 {
                queue.push_back(v);
            }
        }
    }

    // Backward pass (LS/LF)
    let project_end = early_finish.iter().copied().max().unwrap_or(0);
    let mut late_finish = vec![project_end; n];
    let mut late_start  = vec![0i32; n];

    for &u in topo.iter() {
        late_start[u] = late_finish[u] - tasks[u].duration_days;
    }

    for &u in topo.iter().rev() {
        for &v in &adj[u] {
            let new_lf = late_start[v];
            if new_lf < late_finish[u] {
                late_finish[u] = new_lf;
                late_start[u]  = late_finish[u] - tasks[u].duration_days;
            }
        }
    }

    // Total float & critical path
    let mut tx = state.db.begin().await?;
    for i in 0..n {
        let tf = late_start[i] - early_start[i];
        let critical = tf == 0;
        sqlx::query(
            r#"UPDATE tasks SET
                 early_start  = $1,
                 early_finish = $2,
                 late_start   = $3,
                 late_finish  = $4,
                 total_float  = $5,
                 is_critical  = $6,
                 cpm_dirty    = FALSE
               WHERE id = $7"#,
        )
        .bind(early_start[i]).bind(early_finish[i])
        .bind(late_start[i]).bind(late_finish[i])
        .bind(tf).bind(critical).bind(tasks[i].id)
        .execute(&mut *tx).await?;
    }
    tx.commit().await?;

    let updated = sqlx::query_as::<_, Task>(
        r#"SELECT id, project_id, parent_id, position, wbs, name, description,
                  status, priority, task_type, start_date, end_date, duration_days,
                  progress, early_start, early_finish, late_start, late_finish,
                  total_float, is_critical, cpm_dirty, created_at, updated_at
           FROM tasks WHERE project_id = $1 ORDER BY position"#,
    )
    .bind(project_id)
    .fetch_all(&state.db).await?;

    Ok(Json(json!({ "ok": true, "tasks": updated })))
}

// ── open-by-file ──────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct OpenByFileDto {
    pub file_id: Uuid,
}

pub async fn open_by_file(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<OpenByFileDto>,
) -> Result<Json<Value>> {
    let project = sqlx::query_as::<_, Project>(
        r#"SELECT id, owner_id, title, file_id, description, color, status,
                  start_date, end_date, is_starred, is_trashed, trashed_at,
                  last_edited_by, created_at, updated_at
           FROM projects
           WHERE file_id = $1 AND owner_id = $2 AND is_trashed = FALSE"#,
    )
    .bind(dto.file_id).bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound(format!("Aucun projet lié au fichier {}", dto.file_id)))?;

    Ok(Json(json!({ "project": project })))
}

// ── Files integration helpers ─────────────────────────────────────────────────

async fn build_project_json(state: &AppState, project: &Project) -> Option<Bytes> {
    #[derive(serde::Serialize, sqlx::FromRow)]
    struct TaskRow {
        id: Uuid, name: String, status: String, priority: String,
        task_type: String, position: i32, duration_days: i32, progress: i32,
    }
    #[derive(serde::Serialize, sqlx::FromRow)]
    struct DepRow { from_task_id: Uuid, to_task_id: Uuid, dep_type: String, lag_days: i32 }
    #[derive(serde::Serialize, sqlx::FromRow)]
    struct ResRow { id: Uuid, name: String, role: String, color: String, capacity: f64 }

    let tasks: Vec<TaskRow> = sqlx::query_as::<_, TaskRow>(
        "SELECT id, name, status, priority, task_type, position, duration_days, progress FROM tasks WHERE project_id = $1 ORDER BY position"
    ).bind(project.id).fetch_all(&state.db).await
    .map_err(|e| tracing::warn!(project_id = %project.id, error = %e, "Files: fetch tasks failed")).ok()?;

    let deps: Vec<DepRow> = sqlx::query_as::<_, DepRow>(
        "SELECT from_task_id, to_task_id, dep_type, lag_days FROM task_dependencies WHERE project_id = $1"
    ).bind(project.id).fetch_all(&state.db).await
    .map_err(|e| tracing::warn!(project_id = %project.id, error = %e, "Files: fetch deps failed")).ok()?;

    let resources: Vec<ResRow> = sqlx::query_as::<_, ResRow>(
        "SELECT id, name, role, color, capacity FROM project_resources WHERE project_id = $1 ORDER BY created_at"
    ).bind(project.id).fetch_all(&state.db).await
    .map_err(|e| tracing::warn!(project_id = %project.id, error = %e, "Files: fetch resources failed")).ok()?;

    let payload = serde_json::json!({
        "format":    "kubuno-project/v1",
        "title":     project.title,
        "status":    project.status,
        "tasks":     tasks,
        "deps":      deps,
        "resources": resources,
    });

    // Format Kubuno : JSON gzippé (.kbprj).
    let raw = serde_json::to_vec_pretty(&payload).ok()?;
    crate::services::content_files::gzip(&raw).ok().map(Bytes::from)
}

pub async fn register_project_in_files(state: &AppState, project: &mut Project) {
    let folder = match state.files_client.ensure_folder_path(project.owner_id, "Office/Projects", true, Some("SquareKanban")).await {
        Ok(f)  => f,
        Err(e) => { tracing::warn!(project_id = %project.id, error = %e, "Files: ensure_folder_path failed"); return; }
    };

    let json_bytes = match build_project_json(state, project).await {
        Some(b) => b,
        None    => return,
    };

    let file_name = crate::services::content_files::kb_file_name(&project.title, "kbprj");

    let file = match state.files_client.create_file_with_content(
        project.owner_id,
        Some(folder.id),
        &file_name,
        "application/vnd.kubuno.project+json",
        json_bytes,
        Some(serde_json::json!({ "module": "office", "type": "project", "office_project_id": project.id })),
        true,
    ).await {
        Ok(f)  => f,
        Err(e) => { tracing::warn!(project_id = %project.id, error = %e, "Files: create_file_with_content failed"); return; }
    };

    if let Err(e) = sqlx::query("UPDATE projects SET file_id = $1 WHERE id = $2")
        .bind(file.id).bind(project.id).execute(&state.db).await
    {
        tracing::warn!(project_id = %project.id, file_id = %file.id, error = %e, "Files: update file_id failed");
    } else {
        project.file_id = Some(file.id);
    }
}

async fn sync_project_to_files(state: &AppState, project: &Project) {
    let file_id = match project.file_id {
        Some(id) => id,
        None     => {
            let mut clone = project.clone();
            register_project_in_files(state, &mut clone).await;
            return;
        }
    };

    let json_bytes = match build_project_json(state, project).await {
        Some(b) => b,
        None    => return,
    };

    if let Err(e) = state.files_client.update_file_content(project.owner_id, file_id, json_bytes).await {
        tracing::warn!(project_id = %project.id, file_id = %file_id, error = %e, "Files: update_file_content failed");
    }
}
