use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    models::data::*,
    state::AppState,
};

// ── Reports ───────────────────────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Query(q): Query<ListReportsQuery>,
) -> Result<Json<Value>> {
    let trashed = q.trashed.unwrap_or(false);
    let limit   = q.limit.unwrap_or(50).min(200);
    let offset  = q.offset.unwrap_or(0);

    let rows: Vec<Report> = if let Some(ref search) = q.search {
        sqlx::query_as::<_, Report>(
            r#"SELECT id, owner_id, title, description, theme, page_count,
                      dataset_ids, share_token, is_public, is_trashed, is_starred,
                      thumbnail_url, created_at, updated_at
               FROM office_data.reports
               WHERE owner_id = $1 AND is_trashed = $2 AND title ILIKE $3
               ORDER BY updated_at DESC LIMIT $4 OFFSET $5"#,
        ).bind(user.id).bind(trashed).bind(format!("%{search}%")).bind(limit).bind(offset)
         .fetch_all(&state.db).await?
    } else if q.starred.unwrap_or(false) {
        sqlx::query_as::<_, Report>(
            r#"SELECT id, owner_id, title, description, theme, page_count,
                      dataset_ids, share_token, is_public, is_trashed, is_starred,
                      thumbnail_url, created_at, updated_at
               FROM office_data.reports
               WHERE owner_id = $1 AND is_starred = TRUE AND is_trashed = FALSE
               ORDER BY updated_at DESC LIMIT $2 OFFSET $3"#,
        ).bind(user.id).bind(limit).bind(offset).fetch_all(&state.db).await?
    } else {
        sqlx::query_as::<_, Report>(
            r#"SELECT id, owner_id, title, description, theme, page_count,
                      dataset_ids, share_token, is_public, is_trashed, is_starred,
                      thumbnail_url, created_at, updated_at
               FROM office_data.reports
               WHERE owner_id = $1 AND is_trashed = $2
               ORDER BY updated_at DESC LIMIT $3 OFFSET $4"#,
        ).bind(user.id).bind(trashed).bind(limit).bind(offset).fetch_all(&state.db).await?
    };

    Ok(Json(json!({ "reports": rows, "total": rows.len() })))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(dto): Json<CreateReportDto>,
) -> Result<Json<Value>> {
    let title = dto.title.unwrap_or_else(|| "Nouveau rapport".to_string());

    let mut tx = state.db.begin().await?;

    let report: Report = sqlx::query_as::<_, Report>(
        r#"INSERT INTO office_data.reports (owner_id, title, description)
           VALUES ($1, $2, $3)
           RETURNING id, owner_id, title, description, theme, page_count,
                     dataset_ids, share_token, is_public, is_trashed, is_starred,
                     thumbnail_url, created_at, updated_at"#,
    )
    .bind(user.id)
    .bind(&title)
    .bind(&dto.description)
    .fetch_one(&mut *tx)
    .await?;

    // Créer une première page
    sqlx::query(
        r#"INSERT INTO office_data.report_pages (report_id, title, position)
           VALUES ($1, 'Page 1', 0)"#,
    )
    .bind(report.id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(json!({ "report": report })))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let report = fetch_report(&state.db, id, user.id).await?;

    let pages: Vec<ReportPage> = sqlx::query_as::<_, ReportPage>(
        r#"SELECT id, report_id, title, position, width, height, background, created_at
           FROM office_data.report_pages
           WHERE report_id = $1 ORDER BY position"#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    let widgets: Vec<Widget> = sqlx::query_as::<_, Widget>(
        r#"SELECT id, page_id, report_id, widget_type, x, y, width, height, config, z_index, created_at, updated_at
           FROM office_data.widgets
           WHERE report_id = $1 ORDER BY z_index"#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({
        "report": report,
        "pages": pages,
        "widgets": widgets,
    })))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateReportDto>,
) -> Result<Json<Value>> {
    let existing = fetch_report(&state.db, id, user.id).await?;

    let title       = dto.title.as_deref().unwrap_or(&existing.title).to_string();
    let theme       = dto.theme.unwrap_or_else(|| existing.theme.clone());
    let dataset_ids = dto.dataset_ids.unwrap_or_else(|| existing.dataset_ids.clone());
    let is_starred  = dto.is_starred.unwrap_or(existing.is_starred);

    let report: Report = sqlx::query_as::<_, Report>(
        r#"UPDATE office_data.reports
           SET title = $3, description = $4, theme = $5, dataset_ids = $6, is_starred = $7
           WHERE id = $1 AND owner_id = $2
           RETURNING id, owner_id, title, description, theme, page_count,
                     dataset_ids, share_token, is_public, is_trashed, is_starred,
                     thumbnail_url, created_at, updated_at"#,
    )
    .bind(id)
    .bind(user.id)
    .bind(&title)
    .bind(dto.description.as_deref().or(existing.description.as_deref()))
    .bind(&theme)
    .bind(&dataset_ids)
    .bind(is_starred)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "report": report })))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query(
        "DELETE FROM office_data.reports WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Rapport {id} introuvable")));
    }
    Ok(Json(json!({ "deleted": true })))
}

pub async fn trash(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query(
        "UPDATE office_data.reports SET is_trashed = TRUE WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Rapport {id} introuvable")));
    }
    Ok(Json(json!({ "trashed": true })))
}

pub async fn restore(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    sqlx::query(
        "UPDATE office_data.reports SET is_trashed = FALSE WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({ "restored": true })))
}

pub async fn duplicate(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let existing = fetch_report(&state.db, id, user.id).await?;

    let mut tx = state.db.begin().await?;

    let new_title = format!("{} (copie)", existing.title);
    let new_report: Report = sqlx::query_as::<_, Report>(
        r#"INSERT INTO office_data.reports (owner_id, title, description, theme, dataset_ids)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, owner_id, title, description, theme, page_count,
                     dataset_ids, share_token, is_public, is_trashed, is_starred,
                     thumbnail_url, created_at, updated_at"#,
    )
    .bind(user.id)
    .bind(&new_title)
    .bind(&existing.description)
    .bind(&existing.theme)
    .bind(&existing.dataset_ids)
    .fetch_one(&mut *tx)
    .await?;

    // Copier les pages
    let pages: Vec<ReportPage> = sqlx::query_as::<_, ReportPage>(
        "SELECT id, report_id, title, position, width, height, background, created_at FROM office_data.report_pages WHERE report_id = $1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;

    for page in &pages {
        let new_page_id: (Uuid,) = sqlx::query_as(
            r#"INSERT INTO office_data.report_pages (report_id, title, position, width, height, background)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING id"#,
        )
        .bind(new_report.id)
        .bind(&page.title)
        .bind(page.position)
        .bind(page.width)
        .bind(page.height)
        .bind(&page.background)
        .fetch_one(&mut *tx)
        .await?;

        // Copier les widgets de cette page
        let widgets: Vec<Widget> = sqlx::query_as::<_, Widget>(
            "SELECT id, page_id, report_id, widget_type, x, y, width, height, config, z_index, created_at, updated_at FROM office_data.widgets WHERE page_id = $1",
        )
        .bind(page.id)
        .fetch_all(&mut *tx)
        .await?;

        for w in &widgets {
            sqlx::query(
                r#"INSERT INTO office_data.widgets (page_id, report_id, widget_type, x, y, width, height, config, z_index)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
            )
            .bind(new_page_id.0)
            .bind(new_report.id)
            .bind(&w.widget_type)
            .bind(w.x).bind(w.y).bind(w.width).bind(w.height)
            .bind(&w.config).bind(w.z_index)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    Ok(Json(json!({ "report": new_report })))
}

// ── Pages ─────────────────────────────────────────────────────────────────────

pub async fn create_page(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(report_id): Path<Uuid>,
    Json(dto): Json<CreatePageDto>,
) -> Result<Json<Value>> {
    let _ = fetch_report(&state.db, report_id, user.id).await?;

    let pos: (i32,) = sqlx::query_as(
        "SELECT COALESCE(MAX(position) + 1, 0) FROM office_data.report_pages WHERE report_id = $1",
    )
    .bind(report_id)
    .fetch_one(&state.db)
    .await?;

    let page_count_row: Option<(i32,)> = sqlx::query_as(
        "SELECT COUNT(*)::int FROM office_data.report_pages WHERE report_id = $1",
    )
    .bind(report_id)
    .fetch_optional(&state.db)
    .await?;

    let page_count = page_count_row.map(|(c,)| c).unwrap_or(0);
    let title = dto.title.unwrap_or_else(|| format!("Page {}", page_count + 1));

    let page: ReportPage = sqlx::query_as::<_, ReportPage>(
        r#"INSERT INTO office_data.report_pages
               (report_id, title, position, width, height, background)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, report_id, title, position, width, height, background, created_at"#,
    )
    .bind(report_id)
    .bind(&title)
    .bind(pos.0)
    .bind(dto.width.unwrap_or(1200))
    .bind(dto.height.unwrap_or(800))
    .bind(&dto.background)
    .fetch_one(&state.db)
    .await?;

    // Mettre à jour page_count
    sqlx::query(
        "UPDATE office_data.reports SET page_count = page_count + 1 WHERE id = $1",
    )
    .bind(report_id)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({ "page": page })))
}

pub async fn update_page(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((report_id, page_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<UpdatePageDto>,
) -> Result<Json<Value>> {
    let _ = fetch_report(&state.db, report_id, user.id).await?;

    let existing: ReportPage = sqlx::query_as::<_, ReportPage>(
        "SELECT id, report_id, title, position, width, height, background, created_at FROM office_data.report_pages WHERE id = $1 AND report_id = $2",
    )
    .bind(page_id)
    .bind(report_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Page {page_id} introuvable")))?;

    let title      = dto.title.as_deref().unwrap_or(&existing.title).to_string();
    let position   = dto.position.unwrap_or(existing.position);
    let width      = dto.width.unwrap_or(existing.width);
    let height     = dto.height.unwrap_or(existing.height);
    let background = dto.background.or(existing.background);

    let page: ReportPage = sqlx::query_as::<_, ReportPage>(
        r#"UPDATE office_data.report_pages
           SET title = $3, position = $4, width = $5, height = $6, background = $7
           WHERE id = $1 AND report_id = $2
           RETURNING id, report_id, title, position, width, height, background, created_at"#,
    )
    .bind(page_id)
    .bind(report_id)
    .bind(&title)
    .bind(position)
    .bind(width)
    .bind(height)
    .bind(&background)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "page": page })))
}

pub async fn delete_page(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((report_id, page_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let _ = fetch_report(&state.db, report_id, user.id).await?;

    // Vérifier qu'il reste au moins une page
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM office_data.report_pages WHERE report_id = $1",
    )
    .bind(report_id)
    .fetch_one(&state.db)
    .await?;

    if count.0 <= 1 {
        return Err(OfficeError::Validation("Impossible de supprimer la dernière page".to_string()));
    }

    let affected = sqlx::query(
        "DELETE FROM office_data.report_pages WHERE id = $1 AND report_id = $2",
    )
    .bind(page_id)
    .bind(report_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Page {page_id} introuvable")));
    }

    sqlx::query(
        "UPDATE office_data.reports SET page_count = GREATEST(page_count - 1, 1) WHERE id = $1",
    )
    .bind(report_id)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({ "deleted": true })))
}

// ── Widgets ───────────────────────────────────────────────────────────────────

pub async fn create_widget(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(page_id): Path<Uuid>,
    Json(dto): Json<CreateWidgetDto>,
) -> Result<Json<Value>> {
    // Retrouver le report_id depuis la page
    let page_info: Option<(Uuid,)> = sqlx::query_as(
        r#"SELECT p.report_id FROM office_data.report_pages p
           JOIN office_data.reports r ON r.id = p.report_id
           WHERE p.id = $1 AND r.owner_id = $2"#,
    )
    .bind(page_id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?;

    let (report_id,) = page_info
        .ok_or_else(|| OfficeError::NotFound(format!("Page {page_id} introuvable")))?;

    let config = dto.config.unwrap_or(json!({}));

    let widget: Widget = sqlx::query_as::<_, Widget>(
        r#"INSERT INTO office_data.widgets
               (page_id, report_id, widget_type, x, y, width, height, config)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING id, page_id, report_id, widget_type, x, y, width, height, config, z_index, created_at, updated_at"#,
    )
    .bind(page_id)
    .bind(report_id)
    .bind(&dto.widget_type)
    .bind(dto.x.unwrap_or(0))
    .bind(dto.y.unwrap_or(0))
    .bind(dto.width.unwrap_or(400))
    .bind(dto.height.unwrap_or(300))
    .bind(&config)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "widget": widget })))
}

pub async fn update_widget(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(widget_id): Path<Uuid>,
    Json(dto): Json<UpdateWidgetDto>,
) -> Result<Json<Value>> {
    // Vérifier propriété
    let existing: Widget = sqlx::query_as::<_, Widget>(
        r#"SELECT w.id, w.page_id, w.report_id, w.widget_type, w.x, w.y, w.width, w.height, w.config, w.z_index, w.created_at, w.updated_at
           FROM office_data.widgets w
           JOIN office_data.reports r ON r.id = w.report_id
           WHERE w.id = $1 AND r.owner_id = $2"#,
    )
    .bind(widget_id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Widget {widget_id} introuvable")))?;

    let x           = dto.x.unwrap_or(existing.x);
    let y           = dto.y.unwrap_or(existing.y);
    let width       = dto.width.unwrap_or(existing.width);
    let height      = dto.height.unwrap_or(existing.height);
    let z_index     = dto.z_index.unwrap_or(existing.z_index);
    let config      = dto.config.unwrap_or_else(|| existing.config.clone());
    let widget_type = dto.widget_type.as_deref().unwrap_or(&existing.widget_type).to_string();

    let widget: Widget = sqlx::query_as::<_, Widget>(
        r#"UPDATE office_data.widgets
           SET x = $2, y = $3, width = $4, height = $5, z_index = $6, config = $7, widget_type = $8
           WHERE id = $1
           RETURNING id, page_id, report_id, widget_type, x, y, width, height, config, z_index, created_at, updated_at"#,
    )
    .bind(widget_id)
    .bind(x).bind(y).bind(width).bind(height).bind(z_index).bind(&config).bind(&widget_type)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "widget": widget })))
}

pub async fn delete_widget(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(widget_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query(
        r#"DELETE FROM office_data.widgets w
           USING office_data.reports r
           WHERE w.id = $1 AND w.report_id = r.id AND r.owner_id = $2"#,
    )
    .bind(widget_id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Widget {widget_id} introuvable")));
    }
    Ok(Json(json!({ "deleted": true })))
}

pub async fn batch_update_widgets(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Json(body): Json<Value>,
) -> Result<Json<Value>> {
    let updates: Vec<BatchUpdateWidgetDto> = serde_json::from_value(
        body.get("widgets").cloned().unwrap_or(json!([])),
    )
    .map_err(|e| OfficeError::Validation(e.to_string()))?;

    for u in &updates {
        sqlx::query(
            r#"UPDATE office_data.widgets w
               SET x = COALESCE($2, w.x),
                   y = COALESCE($3, w.y),
                   width = COALESCE($4, w.width),
                   height = COALESCE($5, w.height),
                   z_index = COALESCE($6, w.z_index)
               FROM office_data.reports r
               WHERE w.id = $1 AND w.report_id = r.id AND r.owner_id = $7"#,
        )
        .bind(u.id)
        .bind(u.x).bind(u.y).bind(u.width).bind(u.height).bind(u.z_index)
        .bind(user.id)
        .execute(&state.db)
        .await?;
    }

    Ok(Json(json!({ "updated": updates.len() })))
}

// ── Helper ────────────────────────────────────────────────────────────────────

async fn fetch_report(pool: &sqlx::PgPool, id: Uuid, owner_id: Uuid) -> Result<Report> {
    sqlx::query_as::<_, Report>(
        r#"SELECT id, owner_id, title, description, theme, page_count,
                  dataset_ids, share_token, is_public, is_trashed, is_starred,
                  thumbnail_url, created_at, updated_at
           FROM office_data.reports WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Rapport {id} introuvable")))
}
