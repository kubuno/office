use axum::{
    extract::{Path, Query, State},
    Json,
};
use bytes::Bytes;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;
use axum::Extension;

use crate::{
    errors::{OfficeError, Result},
    handlers::project_authz::{
        require_permission, require_tasks_in_project, resource_in_project, task_in_project, Level,
    },
    middleware::OfficeUser,
    models::project::*,
    services::content_files as cf,
    services::working_calendar,
    state::AppState,
};

// ── Cloud-project identifiers (slug) ──────────────────────────────────────────

/// Turns a free-text name into a URL-safe identifier: lowercase, `[a-z0-9-]`,
/// no repeated/edge dashes, starting with a letter, capped at 30 chars. Mirrors
/// the cloud-console rules, adapted to Kubuno (no reserved-word list, no min 6).
fn fold_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in name.to_lowercase().chars().map(fold_accent) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let mut out = out.trim_matches('-').to_string();
    // Must start with a lowercase letter (identifiers are not numeric).
    if !out.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
        out = format!("p-{out}");
    }
    out.truncate(30);
    out.trim_end_matches('-').to_string()
}

/// Resolves a base name to an identifier free across the instance, appending
/// `-2`, `-3`… on collision. The slug column is immutable once set.
async fn resolve_unique_slug(db: &sqlx::PgPool, base: &str) -> Result<String> {
    let root = {
        let s = slugify(base);
        if s.is_empty() { "projet".to_string() } else { s }
    };
    for n in 0..10_000 {
        let cand = if n == 0 {
            root.clone()
        } else {
            let suffix = format!("-{n}");
            let keep = 30usize.saturating_sub(suffix.len());
            format!("{}{}", &root[..root.len().min(keep)], suffix)
        };
        let taken: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE slug = $1)")
                .bind(&cand)
                .fetch_one(db)
                .await?;
        if !taken {
            return Ok(cand);
        }
    }
    Err(OfficeError::Validation(
        "Impossible de générer un identifiant unique".into(),
    ))
}

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
                      last_edited_by, created_at, updated_at, kind, slug, labels, parent_id
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
                      last_edited_by, created_at, updated_at, kind, slug, labels, parent_id
               FROM projects
               WHERE owner_id = $1 AND is_starred = TRUE AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        )
        .bind(user.id).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    } else if q.shared.unwrap_or(false) {
        // Projects shared WITH me: I am a collaborator, someone else owns them.
        sqlx::query_as::<_, Project>(
            r#"SELECT p.id, p.owner_id, p.title, p.file_id, p.description, p.color, p.status,
                      p.start_date, p.end_date, p.is_starred, p.is_trashed, p.trashed_at,
                      p.last_edited_by, p.created_at, p.updated_at, p.kind, p.slug, p.labels, p.parent_id
               FROM projects p
               JOIN project_collaborators c ON c.project_id = p.id
               WHERE c.user_id = $1 AND p.owner_id <> $1 AND p.is_trashed = FALSE
               ORDER BY p.updated_at DESC LIMIT $2 OFFSET $3"#,
        )
        .bind(user.id).bind(limit).bind(offset)
        .fetch_all(&state.db).await?
    } else if q.recent.unwrap_or(false) {
        sqlx::query_as::<_, Project>(
            r#"SELECT id, owner_id, title, file_id, description, color, status,
                      start_date, end_date, is_starred, is_trashed, trashed_at,
                      last_edited_by, created_at, updated_at, kind, slug, labels, parent_id
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
                      last_edited_by, created_at, updated_at, kind, slug, labels, parent_id
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
        kind: None, slug: None, labels: None, parent_id: None,
    });
    let title = dto.title.unwrap_or_else(|| "Nouveau projet".to_string());
    // Instance default project accent colour, used when the request names none.
    let color = dto.color.unwrap_or_else(|| state.instance().project_default_color);
    let kind = if dto.kind.as_deref() == Some("cloud") { "cloud" } else { "management" };
    // Cloud projects carry an immutable, instance-unique identifier derived from
    // the requested slug or the title; management projects are addressed by UUID.
    let slug: Option<String> = if kind == "cloud" {
        let base = dto.slug.clone().unwrap_or_else(|| title.clone());
        Some(resolve_unique_slug(&state.db, &base).await?)
    } else {
        None
    };
    let labels = dto.labels.unwrap_or_else(|| serde_json::json!({}));

    let mut project = sqlx::query_as::<_, Project>(
        r#"INSERT INTO projects (owner_id, title, description, color, start_date, end_date, kind, slug, labels, parent_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
           RETURNING id, owner_id, title, file_id, description, color, status,
                      start_date, end_date, is_starred, is_trashed, trashed_at,
                      last_edited_by, created_at, updated_at, kind, slug, labels, parent_id"#,
    )
    .bind(user.id)
    .bind(&title)
    .bind(dto.description.as_deref().unwrap_or(""))
    .bind(&color)
    .bind(dto.start_date)
    .bind(dto.end_date)
    .bind(kind)
    .bind(&slug)
    .bind(&labels)
    .bind(dto.parent_id)
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
    // Owner OR collaborator may open a project — a project shared with the user
    // (via the access/IAM panel) must be reachable, not only owned ones. This
    // matches the `can_view` rule used by versions/time-entries/modules.
    let project = sqlx::query_as::<_, Project>(
        r#"SELECT id, owner_id, title, file_id, description, color, status,
                      start_date, end_date, is_starred, is_trashed, trashed_at,
                      last_edited_by, created_at, updated_at, kind, slug, labels, parent_id
           FROM projects
           WHERE id = $1 AND (
                 owner_id = $2
                 OR EXISTS (SELECT 1 FROM project_collaborators c
                            WHERE c.project_id = projects.id AND c.user_id = $2)
           )"#,
    )
    .bind(id).bind(user.id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Projet introuvable".into()))?;

    let tasks = sqlx::query_as::<_, Task>(
        r#"SELECT id, project_id, parent_id, position, wbs, name, description,
                  status, priority, task_type, start_date, end_date, duration_days,
                  progress, early_start, early_finish, late_start, late_finish,
                  total_float, free_float, is_critical, cpm_dirty, constraint_type, constraint_date, deadline_date, deadline_missed, estimated_hours, spent_hours, budget_cost, version_id, created_at, updated_at
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
        r#"SELECT r.id, r.project_id, COALESCE(u.display_name, r.name) AS name, r.role, r.color, r.capacity, r.hourly_rate, r.user_id, u.avatar_url::text AS avatar_url, r.kind, r.unit_label, r.overtime_rate, r.cost_per_use, COALESCE((SELECT array_agg(skill ORDER BY skill) FROM resource_skills WHERE resource_id = r.id), '{}'::text[]) AS skills, r.created_at FROM project_resources r LEFT JOIN core.users u ON u.id = r.user_id WHERE r.project_id = $1 ORDER BY r.created_at ASC"#,
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
    // Permission first: the checks below reveal whether projects and tasks exist,
    // so they must not run for someone with no business here.
    require_permission(&state, id, user.id, Level::Edit).await?;

    // Re-parenting (tri-state): present in the body = change; `null` = detach to root.
    // Validate against cycles BEFORE the main update so the RETURNING reflects it.
    if let Some(new_parent) = dto.parent_id {
        if let Some(pid) = new_parent {
            if pid == id {
                return Err(OfficeError::Validation("Un projet ne peut pas être son propre parent.".into()));
            }
            // Owner check on the target parent.
            let owned: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND owner_id = $2)")
                .bind(pid).bind(user.id).fetch_one(&state.db).await?;
            if !owned {
                return Err(OfficeError::NotFound("Projet parent introuvable.".into()));
            }
            // Reject if the target is inside this project's own subtree (would cycle).
            let cycles: bool = sqlx::query_scalar(
                r#"WITH RECURSIVE sub AS (
                       SELECT id FROM projects WHERE id = $1
                       UNION ALL
                       SELECT p.id FROM projects p JOIN sub ON p.parent_id = sub.id
                   ) SELECT EXISTS(SELECT 1 FROM sub WHERE id = $2)"#,
            ).bind(id).bind(pid).fetch_one(&state.db).await?;
            if cycles {
                return Err(OfficeError::Validation("Cible invalide : cela créerait un cycle dans la hiérarchie.".into()));
            }
        }
        sqlx::query("UPDATE projects SET parent_id = $3 WHERE id = $1 AND owner_id = $2")
            .bind(id).bind(user.id).bind(new_parent)
            .execute(&state.db).await?;
    }

    let project = sqlx::query_as::<_, Project>(
        r#"UPDATE projects SET
             title       = COALESCE($3, title),
             description = COALESCE($4, description),
             color       = COALESCE($5, color),
             status      = COALESCE($6, status),
             start_date  = COALESCE($7, start_date),
             end_date    = COALESCE($8, end_date),
             is_starred  = COALESCE($9, is_starred),
             labels      = COALESCE($11, labels),
             last_edited_by = $10
           WHERE id = $1 AND owner_id = $2
           RETURNING id, owner_id, title, file_id, description, color, status,
                      start_date, end_date, is_starred, is_trashed, trashed_at,
                      last_edited_by, created_at, updated_at, kind, slug, labels, parent_id"#,
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
    .bind(&dto.labels)
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
    require_permission(&state, id, user.id, Level::Edit).await?;
    // Report a miss instead of answering "ok" to a request that changed nothing.
    let rows = sqlx::query("UPDATE projects SET is_trashed = TRUE, trashed_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Projet introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}

pub async fn restore(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, id, user.id, Level::Edit).await?;
    let rows = sqlx::query("UPDATE projects SET is_trashed = FALSE, trashed_at = NULL WHERE id = $1")
        .bind(id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Projet introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    // Only the owner may destroy a project — being able to edit it is not enough.
    require_permission(&state, id, user.id, Level::Owner).await?;

    // Read the linked Drive file BEFORE the row disappears: deleting only the row
    // leaves an orphan .kbprj in Drive that answers 404 when opened.
    let file_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT file_id FROM projects WHERE id = $1 AND is_trashed = TRUE",
    )
    .bind(id)
    .fetch_optional(&state.db).await?.flatten();

    let rows = sqlx::query("DELETE FROM projects WHERE id = $1 AND is_trashed = TRUE")
        .bind(id)
        .execute(&state.db).await?.rows_affected();

    // Permanent deletion only applies to a project already in the trash. Answering
    // "ok" when nothing was deleted told the caller a lie.
    if rows == 0 {
        return Err(OfficeError::Validation(
            "Le projet doit d'abord être mis à la corbeille".into(),
        ));
    }
    if let Some(fid) = file_id {
        // Guard: a duplicated project can still point at the same file.
        let still_used: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE file_id = $1)",
        ).bind(fid).fetch_one(&state.db).await?;
        if !still_used {
            cf::delete_entity_files(&state, user.id, [fid]).await;
        }
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn duplicate(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, id, user.id, Level::Edit).await?;

    let source: Project = sqlx::query_as::<_, Project>(
        "SELECT id, owner_id, title, file_id, description, color, status, start_date, end_date,
                is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at, kind, slug, labels, parent_id
         FROM projects WHERE id = $1 AND is_trashed = FALSE",
    )
    .bind(id)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Projet introuvable".into()))?;

    // A cloud copy needs its own immutable identifier; a management copy has none.
    let new_title = format!("{} (copie)", source.title);
    let new_slug: Option<String> = if source.kind == "cloud" {
        Some(resolve_unique_slug(&state.db, &new_title).await?)
    } else {
        None
    };

    // Everything below happens in one transaction: a half-copied project — tasks
    // without their dependencies, assignments pointing at the original's resources —
    // is worse than no copy at all.
    let mut tx = state.db.begin().await?;

    let new_id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (owner_id, title, description, color, status, start_date, end_date, \
                               kind, slug, labels, parent_id, last_edited_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $1) RETURNING id",
    )
    .bind(user.id)
    .bind(&new_title)
    .bind(&source.description)
    .bind(&source.color)
    .bind(&source.status)
    .bind(source.start_date)
    .bind(source.end_date)
    .bind(&source.kind)
    .bind(&new_slug)
    .bind(&source.labels)
    .bind(source.parent_id)
    .fetch_one(&mut *tx).await?;

    // ── Versions, first: tasks reference them ────────────────────────────────
    #[derive(sqlx::FromRow)]
    struct VersionRow {
        id:          Uuid,
        name:        String,
        description: String,
        start_date:  Option<chrono::NaiveDate>,
        due_date:    Option<chrono::NaiveDate>,
        status:      String,
        position:    i32,
    }
    let versions = sqlx::query_as::<_, VersionRow>(
        "SELECT id, name, description, start_date, due_date, status, position \
         FROM project_versions WHERE project_id = $1 ORDER BY position ASC",
    ).bind(id).fetch_all(&mut *tx).await?;

    let mut version_map: HashMap<Uuid, Uuid> = HashMap::with_capacity(versions.len());
    for v in &versions {
        let nid = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO project_versions (id, project_id, name, description, start_date, due_date, status, position) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(nid).bind(new_id).bind(&v.name).bind(&v.description)
        .bind(v.start_date).bind(v.due_date).bind(&v.status).bind(v.position)
        .execute(&mut *tx).await?;
        version_map.insert(v.id, nid);
    }

    // ── Resources, before assignments ────────────────────────────────────────
    let resources = sqlx::query_as::<_, ProjectResource>(
        "SELECT r.id, r.project_id, COALESCE(u.display_name, r.name) AS name, r.role, r.color, r.capacity, r.hourly_rate, r.user_id, u.avatar_url::text AS avatar_url, r.kind, r.unit_label, r.overtime_rate, r.cost_per_use, COALESCE((SELECT array_agg(skill ORDER BY skill) FROM resource_skills WHERE resource_id = r.id), '{}'::text[]) AS skills, r.created_at FROM project_resources r LEFT JOIN core.users u ON u.id = r.user_id WHERE r.project_id = $1 ORDER BY r.created_at ASC",
    ).bind(id).fetch_all(&mut *tx).await?;

    let mut resource_map: HashMap<Uuid, Uuid> = HashMap::with_capacity(resources.len());
    for r in &resources {
        let nid = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO project_resources (id, project_id, name, role, color, capacity, user_id, kind, unit_label, hourly_rate, overtime_rate, cost_per_use) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(nid).bind(new_id).bind(&r.name).bind(&r.role).bind(&r.color).bind(r.capacity).bind(r.user_id)
        .bind(&r.kind).bind(r.unit_label.as_deref()).bind(r.hourly_rate).bind(r.overtime_rate).bind(r.cost_per_use)
        .execute(&mut *tx).await?;
        for sk in &r.skills {
            sqlx::query("INSERT INTO resource_skills (resource_id, skill) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                .bind(nid).bind(sk).execute(&mut *tx).await?;
        }
        resource_map.insert(r.id, nid);
    }

    // ── Tasks ────────────────────────────────────────────────────────────────
    // `tasks.parent_id` points at `tasks` and is NOT deferrable, so PostgreSQL
    // rejects a child inserted before its parent. Ids are generated up front, so
    // the whole old→new map exists before the first write; parents are then
    // reattached in a single statement below.
    let tasks = sqlx::query_as::<_, Task>(
        r#"SELECT id, project_id, parent_id, position, wbs, name, description,
                  status, priority, task_type, start_date, end_date, duration_days,
                  progress, early_start, early_finish, late_start, late_finish,
                  total_float, free_float, is_critical, cpm_dirty, constraint_type, constraint_date, deadline_date, deadline_missed, estimated_hours, spent_hours,
                  version_id, created_at, updated_at
           FROM tasks WHERE project_id = $1 ORDER BY position ASC"#,
    ).bind(id).fetch_all(&mut *tx).await?;

    let task_map: HashMap<Uuid, Uuid> =
        tasks.iter().map(|t| (t.id, Uuid::new_v4())).collect();

    for t in &tasks {
        let Some(&nid) = task_map.get(&t.id) else { continue };
        let new_version = t.version_id.and_then(|v| version_map.get(&v).copied());
        // `spent_hours` stays NULL on purpose: it is the sum of time entries, and
        // those are not copied — hours nobody worked must not appear on a copy.
        sqlx::query(
            r#"INSERT INTO tasks (id, project_id, parent_id, position, wbs, name, description,
                                  status, priority, task_type, start_date, end_date, duration_days,
                                  progress, early_start, early_finish, late_start, late_finish,
                                  total_float, free_float, is_critical, cpm_dirty, constraint_type, constraint_date, deadline_date, deadline_missed, estimated_hours,
                                  spent_hours, budget_cost, version_id)
               VALUES ($1, $2, NULL, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                       $14, $15, $16, $17, $18, $19, TRUE, $20, NULL, $21, $22)"#,
        )
        .bind(nid).bind(new_id)
        .bind(t.position).bind(&t.wbs).bind(&t.name).bind(&t.description)
        .bind(&t.status).bind(&t.priority).bind(&t.task_type)
        .bind(t.start_date).bind(t.end_date).bind(t.duration_days).bind(t.progress)
        .bind(t.early_start).bind(t.early_finish).bind(t.late_start).bind(t.late_finish)
        .bind(t.total_float).bind(t.is_critical)
        // The budget is part of the plan and travels with the copy; the hours
        // and the money actually spent do not — those belong to the original.
        .bind(t.estimated_hours).bind(t.budget_cost).bind(new_version)
        .execute(&mut *tx).await?;
    }

    // Reattach the subtasks now that every row exists.
    let mut children: Vec<Uuid> = Vec::new();
    let mut parents:  Vec<Uuid> = Vec::new();
    for t in &tasks {
        let (Some(old_parent), Some(&child)) = (t.parent_id, task_map.get(&t.id)) else { continue };
        // A parent outside this project would be corrupt data: drop the link, keep the task.
        let Some(&parent) = task_map.get(&old_parent) else {
            tracing::warn!(project_id = %id, task_id = %t.id, "duplicate: parent outside the project — link dropped");
            continue;
        };
        children.push(child);
        parents.push(parent);
    }
    if !children.is_empty() {
        sqlx::query(
            "UPDATE tasks AS t SET parent_id = m.new_parent \
             FROM (SELECT UNNEST($1::uuid[]) AS child, UNNEST($2::uuid[]) AS new_parent) AS m \
             WHERE t.id = m.child",
        )
        .bind(&children).bind(&parents)
        .execute(&mut *tx).await?;
    }

    // ── Dependencies ─────────────────────────────────────────────────────────
    let deps = sqlx::query_as::<_, TaskDependency>(
        "SELECT id, project_id, from_task_id, to_task_id, dep_type, lag_days \
         FROM task_dependencies WHERE project_id = $1",
    ).bind(id).fetch_all(&mut *tx).await?;

    for d in &deps {
        let (Some(&from), Some(&to)) = (task_map.get(&d.from_task_id), task_map.get(&d.to_task_id)) else {
            tracing::warn!(project_id = %id, dep_id = %d.id, "duplicate: dependency endpoint outside the project — skipped");
            continue;
        };
        sqlx::query(
            "INSERT INTO task_dependencies (project_id, from_task_id, to_task_id, dep_type, lag_days) \
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT (from_task_id, to_task_id) DO NOTHING",
        )
        .bind(new_id).bind(from).bind(to).bind(&d.dep_type).bind(d.lag_days)
        .execute(&mut *tx).await?;
    }

    // ── Assignments (both ends remapped) ─────────────────────────────────────
    let assignments = sqlx::query_as::<_, TaskAssignment>(
        "SELECT a.id, a.task_id, a.resource_id, a.units \
         FROM task_assignments a JOIN tasks t ON t.id = a.task_id WHERE t.project_id = $1",
    ).bind(id).fetch_all(&mut *tx).await?;

    for a in &assignments {
        let (Some(&task), Some(&resource)) =
            (task_map.get(&a.task_id), resource_map.get(&a.resource_id)) else { continue };
        sqlx::query(
            "INSERT INTO task_assignments (task_id, resource_id, units) VALUES ($1, $2, $3) \
             ON CONFLICT (task_id, resource_id) DO NOTHING",
        )
        .bind(task).bind(resource).bind(a.units)
        .execute(&mut *tx).await?;
    }

    // ── Attached modules (cloud projects) ────────────────────────────────────
    sqlx::query(
        "INSERT INTO project_modules (project_id, module_id, added_by) \
         SELECT $1, module_id, $2 FROM project_modules WHERE project_id = $3 \
         ON CONFLICT (project_id, module_id) DO NOTHING",
    )
    .bind(new_id).bind(user.id).bind(id)
    .execute(&mut *tx).await?;

    // Baselines, time entries and collaborators are deliberately NOT copied: a
    // baseline is a dated commitment on the original, a time entry records hours a
    // named person actually worked, and copying collaborators would hand people
    // access to a project they never heard of.

    tx.commit().await?;

    // Give the copy its own file in Drive, like a freshly created project would
    // have. Best-effort and after the commit — a call to another module must not
    // hold a transaction open.
    let mut copy = sqlx::query_as::<_, Project>(
        "SELECT id, owner_id, title, file_id, description, color, status, start_date, end_date,
                is_starred, is_trashed, trashed_at, last_edited_by, created_at, updated_at,
                kind, slug, labels, parent_id FROM projects WHERE id = $1",
    ).bind(new_id).fetch_one(&state.db).await?;
    register_project_in_files(&state, &mut copy).await;

    Ok(Json(json!({ "id": new_id })))
}

// ── Tasks ─────────────────────────────────────────────────────────────────────

pub async fn create_task(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<CreateTaskDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    // A parent named in the request must belong to this project, or a subtask could
    // be grafted onto someone else's plan.
    if let Some(parent) = dto.parent_id {
        require_tasks_in_project(&state, project_id, &[parent]).await?;
    }

    let position = dto.position.unwrap_or({
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
                     total_float, free_float, is_critical, cpm_dirty, constraint_type, constraint_date, deadline_date, deadline_missed, estimated_hours, spent_hours, budget_cost, version_id, created_at, updated_at"#,
    )
    .bind(project_id).bind(dto.parent_id)
    .bind(if position == 0 { None::<i32> } else { Some(position) })
    .bind(&name).bind(task_type).bind(dto.start_date).bind(duration)
    .fetch_one(&state.db).await?;

    // A new task shifts the outline numbers of everything after it.
    let task = renumbered(&state, project_id, task).await?;
    Ok(Json(json!({ "task": task })))
}

pub async fn update_task(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, task_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateTaskDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    // Re-parenting (indent/outdent): `parent_id` is tri-state (see UpdateTaskDto).
    // `set_parent` says whether to touch the column at all; `parent_val` is the new
    // value (NULL = move to root). When moving under another task, validate first
    // that it exists in this project and is neither the task itself nor one of its
    // descendants — otherwise the tree would gain a cycle.
    let (set_parent, parent_val) = match dto.parent_id {
        Some(v) => (true, v),
        None    => (false, None),
    };
    // Version assignment is likewise tri-state (present = change, `null` = detach).
    let (set_version, version_val) = match dto.version_id {
        Some(v) => (true, v),
        None    => (false, None),
    };
    if set_parent {
        if let Some(new_parent) = parent_val {
            // `sub` = the task and all its descendants (within the project). If the
            // requested parent is in that set, the move is a cycle (self-parent
            // included, since the CTE's base row is the task itself).
            let cycle: bool = sqlx::query_scalar(
                r#"WITH RECURSIVE sub AS (
                     SELECT id FROM tasks WHERE id = $1 AND project_id = $3
                     UNION ALL
                     SELECT t.id FROM tasks t JOIN sub ON t.parent_id = sub.id
                     WHERE t.project_id = $3
                   )
                   SELECT EXISTS(SELECT 1 FROM sub WHERE id = $2)"#,
            )
            .bind(task_id).bind(new_parent).bind(project_id)
            .fetch_one(&state.db).await?;
            if cycle {
                return Err(OfficeError::Validation(
                    "Réparentage invalide : la tâche cible est la tâche elle-même ou l'une de ses sous-tâches".into(),
                ));
            }
            let parent_ok: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = $1 AND project_id = $2)",
            )
            .bind(new_parent).bind(project_id)
            .fetch_one(&state.db).await?;
            if !parent_ok {
                return Err(OfficeError::NotFound("Tâche parente introuvable".into()));
            }
        }
    }

    // A dated constraint without its date would silently behave like ASAP.
    const DATED: [&str; 6] = ["SNET", "SNLT", "FNET", "FNLT", "MSO", "MFO"];
    if let Some(ref ct) = dto.constraint_type {
        if !["ASAP", "ALAP"].contains(&ct.as_str()) && !DATED.contains(&ct.as_str()) {
            return Err(OfficeError::Validation(format!("Contrainte inconnue : {ct}")));
        }
        if DATED.contains(&ct.as_str()) {
            // Either supplied now, or already on the task.
            let has_date = matches!(dto.constraint_date, Some(Some(_)))
                || (dto.constraint_date.is_none()
                    && sqlx::query_scalar::<_, Option<chrono::NaiveDate>>(
                        "SELECT constraint_date FROM tasks WHERE id = $1")
                        .bind(task_id).fetch_one(&state.db).await?.is_some());
            if !has_date {
                return Err(OfficeError::Validation(
                    "Cette contrainte demande une date.".into(),
                ));
            }
        }
    }
    let (set_cdate, cdate_val) = match dto.constraint_date {
        Some(v) => (true, v),
        None    => (false, None),
    };
    let (set_ddate, ddate_val) = match dto.deadline_date {
        Some(v) => (true, v),
        None    => (false, None),
    };

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
             wbs           = COALESCE($13, wbs),
             estimated_hours = COALESCE($16, estimated_hours),
             spent_hours     = COALESCE($17, spent_hours),
             version_id    = CASE WHEN $18 THEN $19 ELSE version_id END,
             constraint_type = COALESCE($20, constraint_type),
             constraint_date = CASE WHEN $21 THEN $22 ELSE constraint_date END,
             deadline_date   = CASE WHEN $23 THEN $24 ELSE deadline_date END,
             budget_cost     = CASE WHEN $25::boolean THEN $26::double precision ELSE budget_cost END,
             parent_id     = CASE WHEN $14 THEN $15 ELSE parent_id END
           WHERE id = $1 AND project_id = $2
           RETURNING id, project_id, parent_id, position, wbs, name, description,
                     status, priority, task_type, start_date, end_date, duration_days,
                     progress, early_start, early_finish, late_start, late_finish,
                     total_float, free_float, is_critical, cpm_dirty, constraint_type, constraint_date, deadline_date, deadline_missed, estimated_hours, spent_hours, budget_cost, version_id, created_at, updated_at"#,
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
    .bind(set_parent)
    .bind(parent_val)
    .bind(dto.estimated_hours)
    .bind(dto.spent_hours)
    .bind(set_version)
    .bind(version_val)
    .bind(dto.constraint_type.as_deref())
    .bind(set_cdate).bind(cdate_val)
    .bind(set_ddate).bind(ddate_val)
    .bind(dto.budget_cost.is_some()).bind(dto.budget_cost.flatten())
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Tâche introuvable".into()))?;

    // Moving a task changes its address and everyone's after it.
    let task = renumbered(&state, project_id, task).await?;
    Ok(Json(json!({ "task": task })))
}

/// Recompute the project's outline numbers and hand back the task carrying its
/// fresh code, so a caller never displays the number the task had a moment ago.
async fn renumbered(state: &AppState, project_id: Uuid, mut task: Task) -> Result<Task> {
    crate::handlers::project_wbs::renumber(&state.db, project_id).await?;
    if let Some(code) = sqlx::query_scalar::<_, String>("SELECT wbs FROM tasks WHERE id = $1")
        .bind(task.id).fetch_optional(&state.db).await?
    {
        task.wbs = code;
    }
    Ok(task)
}

pub async fn delete_task(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, task_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query(
        r#"DELETE FROM tasks WHERE id = $1 AND project_id = $2"#,
    )
    .bind(task_id).bind(project_id)
    .execute(&state.db).await?.rows_affected();
    if rows == 0 {
        return Err(OfficeError::NotFound("Tâche introuvable".into()));
    }
    // Removing a task closes the gap it leaves in the numbering.
    crate::handlers::project_wbs::renumber(&state.db, project_id).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Dependencies ──────────────────────────────────────────────────────────────

pub async fn create_dependency(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<CreateDependencyDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;

    if dto.from_task_id == dto.to_task_id {
        return Err(OfficeError::Validation("Une tâche ne peut pas dépendre d'elle-même".into()));
    }
    // Both ends must live in this project — the table itself does not enforce it.
    require_tasks_in_project(&state, project_id, &[dto.from_task_id, dto.to_task_id]).await?;

    // A link that closes a loop has no schedule: the tasks in the loop would each
    // wait for the other. Refuse it here, where the user can still act on it,
    // rather than let the computation silently skip them.
    let closes_loop: bool = sqlx::query_scalar(
        r#"WITH RECURSIVE reachable AS (
               SELECT to_task_id AS id FROM task_dependencies
               WHERE project_id = $3 AND from_task_id = $1
               UNION
               SELECT d.to_task_id FROM task_dependencies d
               JOIN reachable r ON d.from_task_id = r.id
               WHERE d.project_id = $3
           )
           SELECT EXISTS(SELECT 1 FROM reachable WHERE id = $2)"#,
    )
    .bind(dto.to_task_id).bind(dto.from_task_id).bind(project_id)
    .fetch_one(&state.db).await?;
    if closes_loop {
        return Err(OfficeError::Validation(
            "Cette liaison créerait une boucle : la tâche suivante précède déjà celle-ci.".into(),
        ));
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
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    sqlx::query(
        r#"DELETE FROM task_dependencies WHERE id = $1 AND project_id = $2"#,
    )
    .bind(dep_id).bind(project_id)
    .execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Resources ─────────────────────────────────────────────────────────────────

pub async fn list_resources(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;

    let resources = sqlx::query_as::<_, ProjectResource>(
        "SELECT r.id, r.project_id, COALESCE(u.display_name, r.name) AS name, r.role, r.color, r.capacity, r.hourly_rate, r.user_id, u.avatar_url::text AS avatar_url, r.kind, r.unit_label, r.overtime_rate, r.cost_per_use, COALESCE((SELECT array_agg(skill ORDER BY skill) FROM resource_skills WHERE resource_id = r.id), '{}'::text[]) AS skills, r.created_at FROM project_resources r LEFT JOIN core.users u ON u.id = r.user_id WHERE r.project_id = $1 ORDER BY r.created_at ASC",
    )
    .bind(project_id)
    .fetch_all(&state.db).await?;

    Ok(Json(json!({ "resources": resources })))
}

#[derive(serde::Serialize, sqlx::FromRow)]
struct OrgMember {
    id:           Uuid,
    display_name: String,
    email:        String,
    avatar_url:   Option<String>,
}

/// Directory members of the CURRENT user's organizational unit who are not already
/// resources on this project — the candidates for "assign a task to a member of my
/// unit". Reads core.users directly (same database), like the recipient search does.
pub async fn list_org_members(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let members = sqlx::query_as::<_, OrgMember>(
        r#"SELECT u.id,
                  COALESCE(NULLIF(u.display_name, ''), u.username) AS display_name,
                  COALESCE(u.email::text, '') AS email,
                  u.avatar_url::text AS avatar_url
           FROM core.users u
           WHERE u.is_active
             AND u.org_unit_id = (SELECT org_unit_id FROM core.users WHERE id = $2)
             AND NOT EXISTS (
               SELECT 1 FROM project_resources r WHERE r.project_id = $1 AND r.user_id = u.id
             )
           ORDER BY u.display_name ASC"#,
    )
    .bind(project_id)
    .bind(user.id)
    .fetch_all(&state.db).await?;
    Ok(Json(json!({ "members": members })))
}

/// The full read shape of a resource (live name/avatar from core.users + skills).
const RESOURCE_SELECT: &str = "SELECT r.id, r.project_id, COALESCE(u.display_name, r.name) AS name, r.role, r.color, r.capacity, r.hourly_rate, r.user_id, u.avatar_url::text AS avatar_url, r.kind, r.unit_label, r.overtime_rate, r.cost_per_use, COALESCE((SELECT array_agg(skill ORDER BY skill) FROM resource_skills WHERE resource_id = r.id), '{}'::text[]) AS skills, r.created_at FROM project_resources r LEFT JOIN core.users u ON u.id = r.user_id";

async fn fetch_resource(db: &sqlx::PgPool, id: Uuid) -> Result<Option<ProjectResource>> {
    Ok(sqlx::query_as::<_, ProjectResource>(&format!("{RESOURCE_SELECT} WHERE r.id = $1"))
        .bind(id).fetch_optional(db).await?)
}

/// Replace a resource's whole skill set.
async fn set_skills(db: &sqlx::PgPool, resource_id: Uuid, skills: &[String]) -> Result<()> {
    sqlx::query("DELETE FROM resource_skills WHERE resource_id = $1").bind(resource_id).execute(db).await?;
    for sk in skills {
        let sk = sk.trim();
        if sk.is_empty() { continue; }
        sqlx::query("INSERT INTO resource_skills (resource_id, skill) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(resource_id).bind(sk).execute(db).await?;
    }
    Ok(())
}

const RESOURCE_KINDS: [&str; 10] = ["person", "contractor", "equipment", "facility", "material", "software", "infrastructure", "information", "financial", "cost"];

pub async fn create_resource(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<CreateResourceDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let kind = dto.kind.as_deref().unwrap_or("person");
    if !RESOURCE_KINDS.contains(&kind) {
        return Err(OfficeError::Validation(format!("Type de ressource inconnu : {kind}")));
    }
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO project_resources
             (project_id, name, role, color, capacity, user_id, kind, unit_label, hourly_rate, overtime_rate, cost_per_use)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id"#,
    )
    .bind(project_id)
    .bind(&dto.name)
    .bind(dto.role.as_deref().unwrap_or(""))
    .bind(dto.color.as_deref().unwrap_or("#5f6368"))
    .bind(dto.capacity.unwrap_or(1.0))
    .bind(dto.user_id)
    .bind(kind)
    .bind(dto.unit_label.as_deref())
    .bind(dto.hourly_rate)
    .bind(dto.overtime_rate)
    .bind(dto.cost_per_use)
    .fetch_one(&state.db).await?;

    if let Some(skills) = &dto.skills { set_skills(&state.db, id, skills).await?; }
    let resource = fetch_resource(&state.db, id).await?
        .ok_or_else(|| OfficeError::NotFound("Ressource introuvable".into()))?;
    Ok(Json(json!({ "resource": resource })))
}

pub async fn update_resource(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, resource_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdateResourceDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    if let Some(k) = dto.kind.as_deref() {
        if !RESOURCE_KINDS.contains(&k) {
            return Err(OfficeError::Validation(format!("Type de ressource inconnu : {k}")));
        }
    }
    let done = sqlx::query(
        r#"UPDATE project_resources SET
             name     = COALESCE($3, name),
             role     = COALESCE($4, role),
             color    = COALESCE($5, color),
             capacity = COALESCE($6, capacity),
             kind     = COALESCE($9, kind),
             hourly_rate   = CASE WHEN $7::boolean  THEN $8::double precision  ELSE hourly_rate   END,
             unit_label    = CASE WHEN $10::boolean THEN $11                   ELSE unit_label    END,
             overtime_rate = CASE WHEN $12::boolean THEN $13::double precision ELSE overtime_rate END,
             cost_per_use  = CASE WHEN $14::boolean THEN $15::double precision ELSE cost_per_use  END
           WHERE id = $1 AND project_id = $2"#,
    )
    .bind(resource_id).bind(project_id)
    .bind(dto.name.as_deref())
    .bind(dto.role.as_deref())
    .bind(dto.color.as_deref())
    .bind(dto.capacity)
    .bind(dto.hourly_rate.is_some()).bind(dto.hourly_rate.flatten())
    .bind(dto.kind.as_deref())
    .bind(dto.unit_label.is_some()).bind(dto.unit_label.clone().flatten())
    .bind(dto.overtime_rate.is_some()).bind(dto.overtime_rate.flatten())
    .bind(dto.cost_per_use.is_some()).bind(dto.cost_per_use.flatten())
    .execute(&state.db).await?;
    if done.rows_affected() == 0 {
        return Err(OfficeError::NotFound("Ressource introuvable".into()));
    }
    if let Some(skills) = &dto.skills { set_skills(&state.db, resource_id, skills).await?; }
    let resource = fetch_resource(&state.db, resource_id).await?
        .ok_or_else(|| OfficeError::NotFound("Ressource introuvable".into()))?;

    Ok(Json(json!({ "resource": resource })))
}

pub async fn delete_resource(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, resource_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    sqlx::query(
        r#"DELETE FROM project_resources WHERE id = $1 AND project_id = $2"#,
    )
    .bind(resource_id).bind(project_id)
    .execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Resource availability (time off / leave) ────────────────────────────────────
pub async fn list_time_off(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, resource_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    if !resource_in_project(&state, project_id, resource_id).await? {
        return Err(OfficeError::NotFound("Ressource introuvable".into()));
    }
    let entries = sqlx::query_as::<_, ResourceTimeOff>(
        "SELECT id, resource_id, from_date, to_date, reason FROM resource_time_off WHERE resource_id = $1 ORDER BY from_date",
    ).bind(resource_id).fetch_all(&state.db).await?;
    Ok(Json(json!({ "time_off": entries })))
}

pub async fn create_time_off(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, resource_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<CreateTimeOffDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    if !resource_in_project(&state, project_id, resource_id).await? {
        return Err(OfficeError::NotFound("Ressource introuvable".into()));
    }
    if dto.to_date < dto.from_date {
        return Err(OfficeError::Validation("La date de fin précède la date de début.".into()));
    }
    let entry = sqlx::query_as::<_, ResourceTimeOff>(
        r#"INSERT INTO resource_time_off (resource_id, from_date, to_date, reason)
           VALUES ($1, $2, $3, $4)
           RETURNING id, resource_id, from_date, to_date, reason"#,
    )
    .bind(resource_id).bind(dto.from_date).bind(dto.to_date)
    .bind(dto.reason.as_deref().unwrap_or(""))
    .fetch_one(&state.db).await?;
    Ok(Json(json!({ "entry": entry })))
}

pub async fn delete_time_off(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, resource_id, entry_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    sqlx::query(
        "DELETE FROM resource_time_off t USING project_resources r
          WHERE t.id = $1 AND t.resource_id = r.id AND r.id = $2 AND r.project_id = $3",
    )
    .bind(entry_id).bind(resource_id).bind(project_id)
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
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    // The permission is on the PROJECT, but the row is keyed by (task, resource):
    // without these two guards, naming a project of one's own together with someone
    // else's task reached straight into their plan.
    if !task_in_project(&state, project_id, task_id).await? {
        return Err(OfficeError::NotFound("Tâche introuvable dans ce projet".into()));
    }
    if !resource_in_project(&state, project_id, dto.resource_id).await? {
        return Err(OfficeError::NotFound("Ressource introuvable dans ce projet".into()));
    }

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
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    // The DELETE is keyed by (task, resource) only: it must be tied back to the
    // project, or naming a project of one's own would delete anyone's assignment.
    if !task_in_project(&state, project_id, task_id).await? {
        return Err(OfficeError::NotFound("Tâche introuvable dans ce projet".into()));
    }
    sqlx::query(
        r#"DELETE FROM task_assignments a
           USING tasks t
           WHERE a.task_id = $1 AND a.resource_id = $2
             AND t.id = a.task_id AND t.project_id = $3"#,
    )
    .bind(task_id).bind(resource_id).bind(project_id)
    .execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── CPM Compute ───────────────────────────────────────────────────────────────

pub async fn compute_cpm(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;

    let tasks = sqlx::query_as::<_, Task>(
        r#"SELECT id, project_id, parent_id, position, wbs, name, description,
                  status, priority, task_type, start_date, end_date, duration_days,
                  progress, early_start, early_finish, late_start, late_finish,
                  total_float, free_float, is_critical, cpm_dirty, constraint_type, constraint_date, deadline_date, deadline_missed, estimated_hours, spent_hours, budget_cost, version_id, created_at, updated_at
           FROM tasks WHERE project_id = $1 ORDER BY position"#,
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

    // Constraints are stored as dates because that is what they mean to a person —
    // a permit, a contractual delivery. The schedule works in day offsets, so they
    // are converted against the project start. A project without a start date is
    // anchored on today, exactly like the Gantt already displays it.
    let project_start: chrono::NaiveDate =
        sqlx::query_scalar::<_, Option<chrono::NaiveDate>>("SELECT start_date FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_one(&state.db).await?
            .unwrap_or_else(|| chrono::Utc::now().date_naive());
    let to_offset = |d: chrono::NaiveDate| (d - project_start).num_days() as i32;

    // The project's working calendar. Durations are counted in WORKED days, so a
    // five-day task no longer swallows the weekend. A project without one keeps
    // the Monday-to-Friday default rather than reverting to raw calendar days.
    #[derive(sqlx::FromRow)]
    struct CalRow { mon: bool, tue: bool, wed: bool, thu: bool, fri: bool, sat: bool, sun: bool }
    let cal_row = sqlx::query_as::<_, CalRow>(
        "SELECT c.mon, c.tue, c.wed, c.thu, c.fri, c.sat, c.sun          FROM pm_calendars c JOIN projects p ON p.calendar_id = c.id WHERE p.id = $1",
    ).bind(project_id).fetch_optional(&state.db).await?;

    let exceptions: Vec<(chrono::NaiveDate, bool)> = sqlx::query_as(
        "SELECT e.day, e.is_working FROM pm_calendar_exceptions e          JOIN projects p ON p.calendar_id = e.calendar_id WHERE p.id = $1",
    ).bind(project_id).fetch_all(&state.db).await?;

    let calendar = match cal_row {
        Some(c) => working_calendar::WorkingCalendar::from_parts(
            [c.mon, c.tue, c.wed, c.thu, c.fri, c.sat, c.sun], exceptions, project_start),
        // No project calendar yet → fall back to the administrator's instance default
        // (weekends counted or not) rather than a hard-coded Monday–Friday.
        None => {
            let we = state.instance().project_default_include_weekends;
            working_calendar::WorkingCalendar::from_parts(
                [true, true, true, true, true, we, we], Vec::new(), project_start)
        }
    };
    if !calendar.has_working_day() {
        return Err(OfficeError::Validation(
            "Le calendrier du projet n'a aucun jour travaillé.".into(),
        ));
    }
    // Small helpers so the passes below read the same as before.
    let finish_of = |start: i32, dur: i32| calendar.advance(start, dur, project_start);
    let start_of  = |finish: i32, dur: i32| calendar.retreat(finish, dur, project_start);
    let workable  = |offset: i32| calendar.next_working(offset, project_start);

    // Kahn's topological sort + CPM
    let n = tasks.len();
    let task_idx: std::collections::HashMap<Uuid, usize> = tasks.iter().enumerate()
        .map(|(i, t)| (t.id, i)).collect();

    // Edges carry their relationship type and signed lag: both passes need them.
    let mut in_degree = vec![0usize; n];
    let mut adj: Vec<Vec<(usize, String, i32)>> = vec![vec![]; n];

    for dep in &deps {
        if let (Some(&from), Some(&to)) = (task_idx.get(&dep.from_task_id), task_idx.get(&dep.to_task_id)) {
            adj[from].push((to, dep.dep_type.clone(), dep.lag_days));
            in_degree[to] += 1;
        }
    }

    // Forward pass (ES/EF).
    //
    // Each link is a lower bound on the successor's early start, which is what
    // makes the four relationship types uniform (offsets, so no ±1 day-number
    // adjustment; `lag` is signed — a negative lag is a lead, i.e. an overlap):
    //   FS: ES(B) >= EF(A) + lag
    //   SS: ES(B) >= ES(A) + lag
    //   FF: EF(B) >= EF(A) + lag  =>  ES(B) >= EF(A) + lag - dur(B)
    //   SF: EF(B) >= ES(A) + lag  =>  ES(B) >= ES(A) + lag - dur(B)
    // Until date constraints exist, the project start is the other lower bound,
    // hence the max with 0 (an FF link can otherwise pull a start before day 0).
    let mut early_start  = vec![0i32; n];
    let mut early_finish = vec![0i32; n];

    // A constraint is a lower bound on the early start, just like a link — which is
    // what lets both live in the same pass. `Must` types also pin the late dates
    // (applied in the backward pass below).
    for (i, t) in tasks.iter().enumerate() {
        if let Some(d) = t.constraint_date {
            let at = to_offset(d);
            let floor = match t.constraint_type.as_str() {
                "SNET" => Some(at),
                "FNET" => Some(start_of(at, t.duration_days)),
                "MSO"  => Some(at),
                "MFO"  => Some(start_of(at, t.duration_days)),
                _      => None,     // ASAP, ALAP, SNLT, FNLT bound the LATE dates
            };
            if let Some(f) = floor {
                early_start[i] = early_start[i].max(f);
            }
        }
        early_start[i]  = workable(early_start[i]);
        early_finish[i] = finish_of(early_start[i], t.duration_days);
    }

    let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut topo = Vec::with_capacity(n);
    let mut temp_indegree = in_degree.clone();

    while let Some(u) = queue.pop_front() {
        topo.push(u);
        for &(v, ref dep_type, lag) in &adj[u] {
            let dur_v = tasks[v].duration_days;
            // FF and SF bound the successor's FINISH; converting that to a start
            // must walk back over worked days, not calendar days.
            let bound = match dep_type.as_str() {
                "SS" => early_start[u] + lag,
                "FF" => start_of(early_finish[u] + lag, dur_v),
                "SF" => start_of(early_start[u] + lag, dur_v),
                _    => early_finish[u] + lag,      // FS, the default
            };
            if bound > early_start[v] {
                early_start[v]  = workable(bound);
                early_finish[v] = finish_of(early_start[v], dur_v);
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

    // Constraints that bound the LATE dates. When one of them lands before the
    // early finish the task carries NEGATIVE float — the honest signal that the
    // plan cannot meet its own fixed point. It is left unclamped on purpose.
    for (i, t) in tasks.iter().enumerate() {
        if let Some(d) = t.constraint_date {
            let at = to_offset(d);
            let ceiling = match t.constraint_type.as_str() {
                "SNLT" => Some(finish_of(at, t.duration_days)),   // LS <= at
                "FNLT" => Some(at),
                "MSO"  => Some(finish_of(at, t.duration_days)),   // pinned start
                "MFO"  => Some(at),                              // pinned finish
                _      => None,
            };
            if let Some(c) = ceiling {
                late_finish[i] = late_finish[i].min(c);
            }
        }
    }

    for &u in topo.iter() {
        late_start[u] = start_of(late_finish[u], tasks[u].duration_days);
    }

    // Kahn's algorithm only visits what it can order. Anything left behind sits in a
    // loop — and its dates would keep the defaults, quietly. Say so instead: a plan
    // that cannot be scheduled must not be presented as if it had been.
    if topo.len() < n {
        let stuck: Vec<&str> = (0..n)
            .filter(|i| !topo.contains(i))
            .map(|i| tasks[i].name.as_str())
            .take(5)
            .collect();
        return Err(OfficeError::Validation(format!(
            "Les liaisons forment une boucle : impossible de planifier {}.",
            stuck.join(", ")
        )));
    }

    // Backward pass — the mirror of the forward one: each link is an upper bound
    // on the predecessor's late finish.
    //   FS: LF(A) <= LS(B) - lag
    //   SS: LS(A) <= LS(B) - lag  =>  LF(A) <= LS(B) - lag + dur(A)
    //   FF: LF(A) <= LF(B) - lag
    //   SF: LS(A) <= LF(B) - lag  =>  LF(A) <= LF(B) - lag + dur(A)
    for &u in topo.iter().rev() {
        let dur_u = tasks[u].duration_days;
        for &(v, ref dep_type, lag) in &adj[u] {
            let bound = match dep_type.as_str() {
                "SS" => finish_of(late_start[v]  - lag, dur_u),
                "FF" => late_finish[v] - lag,
                "SF" => finish_of(late_finish[v] - lag, dur_u),
                _    => late_start[v]  - lag,       // FS, the default
            };
            if bound < late_finish[u] {
                late_finish[u] = bound;
                late_start[u]  = start_of(late_finish[u], dur_u);
            }
        }
    }

    // As Late As Possible: pull the task to its latest position that still delays
    // nothing. That is exactly what its float measures, so consuming it is safe —
    // and it must happen after the backward pass, which computes that float.
    for i in 0..n {
        if tasks[i].constraint_type == "ALAP" && late_start[i] > early_start[i] {
            early_start[i]  = late_start[i];
            early_finish[i] = finish_of(early_start[i], tasks[i].duration_days);
        }
    }

    // A deadline does not move anything; it only tells the truth about being late.
    let deadline_missed: Vec<bool> = tasks.iter().enumerate()
        .map(|(i, t)| t.deadline_date.is_some_and(|d| early_finish[i] > to_offset(d)))
        .collect();

    // Free float — how long a task may slip before it pushes a successor's early
    // start. Total float measures slack against the project end; a task can have
    // plenty of one and none of the other. With no successor, the two coincide.
    let mut free_float = vec![0i32; n];
    for u in 0..n {
        let mut slack: Option<i32> = None;
        for &(v, ref dep_type, lag) in &adj[u] {
            let dur_v = tasks[v].duration_days;
            let bound = match dep_type.as_str() {
                "SS" => early_start[u]  + lag,
                "FF" => early_finish[u] + lag - dur_v,
                "SF" => early_start[u]  + lag - dur_v,
                _    => early_finish[u] + lag,
            };
            let gap = early_start[v] - bound;
            slack = Some(slack.map_or(gap, |s: i32| s.min(gap)));
        }
        free_float[u] = slack.unwrap_or(late_start[u] - early_start[u]);
    }

    // ── Summary rollup ─────────────────────────────────────────────────────────
    // A parent task (any task that has sub-tasks) owns no dates of its own: like
    // every Gantt tool (MS Project, Instagantt, GanttProject, ProjectLibre, P6…) it
    // *spans* its children — start = earliest child start, finish = latest child
    // finish, duration = the worked days between. Rolled up in post-order so a nested
    // parent reads children that are already rolled up. Leaves keep their own values.
    let mut children: Vec<Vec<usize>> = vec![vec![]; n];
    let mut roots: Vec<usize> = vec![];
    for (i, t) in tasks.iter().enumerate() {
        match t.parent_id.and_then(|p| task_idx.get(&p)) {
            Some(&pi) => children[pi].push(i),
            None => roots.push(i),
        }
    }
    let mut post: Vec<usize> = Vec::with_capacity(n);
    let mut stack: Vec<(usize, bool)> = roots.iter().rev().map(|&r| (r, false)).collect();
    while let Some((u, done)) = stack.pop() {
        if done {
            post.push(u);
        } else {
            stack.push((u, true));
            for &ch in children[u].iter().rev() {
                stack.push((ch, false));
            }
        }
    }
    let mut duration_out: Vec<i32> = tasks.iter().map(|t| t.duration_days).collect();
    // Roll extents up over EVERY parent into scratch arrays (so a summary that
    // contains a task-with-subtasks still spans everything), then COMMIT only for
    // explicit `summary` tasks. A plain task that happens to have sub-tasks keeps its
    // own dates and duration — it stays a normal, resizable bar. Only a `summary` is
    // a derived bracket, matching the app's three task types.
    let mut roll_es = early_start.clone();
    let mut roll_ef = early_finish.clone();
    let mut roll_ls = late_start.clone();
    let mut roll_lf = late_finish.clone();
    for &u in &post {
        if children[u].is_empty() {
            continue;
        }
        roll_es[u] = children[u].iter().map(|&c| roll_es[c]).min().unwrap_or(roll_es[u]);
        roll_ef[u] = children[u].iter().map(|&c| roll_ef[c]).max().unwrap_or(roll_ef[u]);
        roll_ls[u] = children[u].iter().map(|&c| roll_ls[c]).min().unwrap_or(roll_ls[u]);
        roll_lf[u] = children[u].iter().map(|&c| roll_lf[c]).max().unwrap_or(roll_lf[u]);
    }
    // Any task that HAS sub-tasks is a summary (MS Project convention): its extent
    // is derived from its children, whatever its stored task_type. Leaves keep their
    // own dates and duration.
    for i in 0..n {
        if !children[i].is_empty() {
            early_start[i]  = roll_es[i];
            early_finish[i] = roll_ef[i];
            late_start[i]   = roll_ls[i];
            late_finish[i]  = roll_lf[i];
            duration_out[i] = calendar.working_days_between(roll_es[i], roll_ef[i], project_start);
        }
    }

    // Total float & critical path
    let mut tx = state.db.begin().await?;
    for i in 0..n {
        let tf = late_start[i] - early_start[i];
        // Negative float is worse than zero float: still on the critical path.
        let critical = tf <= 0;
        sqlx::query(
            r#"UPDATE tasks SET
                 early_start   = $1,
                 early_finish  = $2,
                 late_start    = $3,
                 late_finish   = $4,
                 total_float   = $5,
                 free_float    = $6,
                 is_critical   = $7,
                 deadline_missed = $8,
                 duration_days = $10,
                 cpm_dirty     = FALSE
               WHERE id = $9"#,
        )
        .bind(early_start[i]).bind(early_finish[i])
        .bind(late_start[i]).bind(late_finish[i])
        .bind(tf).bind(free_float[i]).bind(critical)
        .bind(deadline_missed[i]).bind(tasks[i].id)
        .bind(duration_out[i])
        .execute(&mut *tx).await?;
    }
    tx.commit().await?;

    let updated = sqlx::query_as::<_, Task>(
        r#"SELECT id, project_id, parent_id, position, wbs, name, description,
                  status, priority, task_type, start_date, end_date, duration_days,
                  progress, early_start, early_finish, late_start, late_finish,
                  total_float, free_float, is_critical, cpm_dirty, constraint_type, constraint_date, deadline_date, deadline_missed, estimated_hours, spent_hours, budget_cost, version_id, created_at, updated_at
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
                  last_edited_by, created_at, updated_at, kind, slug, labels, parent_id
           FROM projects
           WHERE file_id = $1 AND is_trashed = FALSE
             AND (owner_id = $2
                  OR EXISTS (SELECT 1 FROM project_collaborators c
                             WHERE c.project_id = projects.id AND c.user_id = $2))"#,
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
        // `overwrite = false`: the name comes from the title, and two projects can
        // share one ("Nouveau projet"). Overwriting DELETED the older project's
        // file, leaving it with a dangling `file_id` and no file in Drive. Drive
        // now numbers the copy instead ("… (2).kbprj"). Safe here because this
        // runs once, at creation; later saves go through `update_file_content`.
        false,
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

    // Projects created before the name-collision fix can share one file with a
    // same-titled sibling; each save would then overwrite the others' content.
    // Give this project a file of its own instead.
    let shared: bool = sqlx::query_scalar("SELECT COUNT(*) > 1 FROM projects WHERE file_id = $1")
        .bind(file_id).fetch_one(&state.db).await.unwrap_or(false);
    if shared {
        tracing::warn!(project_id = %project.id, file_id = %file_id,
                       "Files: file shared with another project — creating a dedicated one");
        let mut clone = project.clone();
        clone.file_id = None;
        register_project_in_files(state, &mut clone).await;
        return;
    }

    let json_bytes = match build_project_json(state, project).await {
        Some(b) => b,
        None    => return,
    };

    if let Err(e) = state.files_client.update_file_content(project.owner_id, file_id, json_bytes).await {
        // The file is gone while the project still points at it — projects created
        // before the name-collision fix lost their file to a same-titled sibling.
        // Re-create it instead of leaving the project invisible in Drive forever.
        tracing::warn!(project_id = %project.id, file_id = %file_id, error = %e,
                       "Files: update_file_content failed — recreating the project file");
        let mut clone = project.clone();
        clone.file_id = None;
        register_project_in_files(state, &mut clone).await;
    }
}
