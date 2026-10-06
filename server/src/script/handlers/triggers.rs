use axum::{
    extract::{Path, State},
    Extension, Json,
};
use rand::RngCore;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    middleware::OfficeUser,
    script::models::trigger::{CreateTriggerDto, Trigger, UpdateTriggerDto},
    state::AppState,
};

/// Valide une expression cron à 5 champs (min heure jour mois jour-semaine).
/// Validation de format uniquement — le dispatcher devra en plus imposer un
/// intervalle minimum lorsqu'il sera implémenté.
fn validate_cron(expr: &str) -> Result<()> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(OfficeError::Validation(
            "L'expression cron doit comporter 5 champs (min heure jour mois jour-semaine)".into(),
        ));
    }
    // Jeu de caractères autorisé par champ : chiffres, * / , - et rien d'autre.
    let allowed = |c: char| c.is_ascii_digit() || matches!(c, '*' | '/' | ',' | '-');
    for f in &fields {
        if f.is_empty() || !f.chars().all(allowed) {
            return Err(OfficeError::Validation(format!("Champ cron invalide : '{f}'")));
        }
    }
    Ok(())
}

/// Génère un jeton de webhook imprévisible (256 bits, hex).
fn generate_webhook_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

// ── List triggers for a script ────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(script_id): Path<Uuid>,
) -> Result<Json<Value>> {
    // Verify script ownership
    let owns: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM office_script.scripts WHERE id = $1 AND owner_id = $2)",
    )
    .bind(script_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    if !owns {
        return Err(OfficeError::NotFound(format!("Script {script_id} introuvable")));
    }

    let triggers: Vec<Trigger> = sqlx::query_as::<_, Trigger>(
        r#"SELECT id, script_id, owner_id, name, trigger_type, cron_expression,
                  event_name, event_module, event_filter, webhook_token, input_vars,
                  is_active, last_fired_at, fire_count, created_at
           FROM office_script.triggers WHERE script_id = $1 ORDER BY created_at"#,
    )
    .bind(script_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(json!({ "triggers": triggers })))
}

// ── Create ────────────────────────────────────────────────────────────────────

pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(script_id): Path<Uuid>,
    Json(dto): Json<CreateTriggerDto>,
) -> Result<Json<Value>> {
    // Verify ownership
    let owns: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM office_script.scripts WHERE id = $1 AND owner_id = $2)",
    )
    .bind(script_id)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    if !owns {
        return Err(OfficeError::NotFound(format!("Script {script_id} introuvable")));
    }

    // Validate trigger type
    if !["cron", "event", "webhook"].contains(&dto.trigger_type.as_str()) {
        return Err(OfficeError::Validation(
            "trigger_type doit être 'cron', 'event' ou 'webhook'".to_string(),
        ));
    }

    // Un trigger cron doit fournir une expression cron valide.
    if dto.trigger_type == "cron" {
        match dto.cron_expression.as_deref() {
            Some(expr) => validate_cron(expr)?,
            None => {
                return Err(OfficeError::Validation(
                    "Un déclencheur cron requiert une expression cron".into(),
                ))
            }
        }
    }

    // Un trigger webhook reçoit un jeton secret généré côté serveur (jamais nul,
    // sinon le futur dispatcher accepterait des appels non authentifiés).
    let webhook_token = if dto.trigger_type == "webhook" {
        Some(generate_webhook_token())
    } else {
        None
    };

    let name        = dto.name.unwrap_or_else(|| "Déclencheur".to_string());
    let event_filter = dto.event_filter.unwrap_or(json!({}));
    let input_vars   = dto.input_vars.unwrap_or(json!({}));

    let trigger: Trigger = sqlx::query_as::<_, Trigger>(
        r#"INSERT INTO office_script.triggers
               (script_id, owner_id, name, trigger_type, cron_expression, event_name,
                event_module, event_filter, input_vars, webhook_token)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
           RETURNING id, script_id, owner_id, name, trigger_type, cron_expression,
                     event_name, event_module, event_filter, webhook_token, input_vars,
                     is_active, last_fired_at, fire_count, created_at"#,
    )
    .bind(script_id)
    .bind(user.id)
    .bind(&name)
    .bind(&dto.trigger_type)
    .bind(&dto.cron_expression)
    .bind(&dto.event_name)
    .bind(&dto.event_module)
    .bind(&event_filter)
    .bind(&input_vars)
    .bind(&webhook_token)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "trigger": trigger })))
}

// ── Update ────────────────────────────────────────────────────────────────────

pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(trigger_id): Path<Uuid>,
    Json(dto): Json<UpdateTriggerDto>,
) -> Result<Json<Value>> {
    let existing = fetch_trigger(&state.db, trigger_id, user.id).await?;

    let name            = dto.name.as_deref().unwrap_or(&existing.name).to_string();
    let cron_expression = dto.cron_expression.or(existing.cron_expression);
    let event_name      = dto.event_name.or(existing.event_name);
    let event_module    = dto.event_module.or(existing.event_module);
    let event_filter    = dto.event_filter.unwrap_or(existing.event_filter);
    let input_vars      = dto.input_vars.unwrap_or(existing.input_vars);
    let is_active       = dto.is_active.unwrap_or(existing.is_active);

    // Revalider l'expression cron d'un trigger cron après modification.
    if existing.trigger_type == "cron" {
        match cron_expression.as_deref() {
            Some(expr) => validate_cron(expr)?,
            None => {
                return Err(OfficeError::Validation(
                    "Un déclencheur cron requiert une expression cron".into(),
                ))
            }
        }
    }

    let trigger: Trigger = sqlx::query_as::<_, Trigger>(
        r#"UPDATE office_script.triggers
           SET name = $2, cron_expression = $3, event_name = $4, event_module = $5,
               event_filter = $6, input_vars = $7, is_active = $8
           WHERE id = $1 AND owner_id = $9
           RETURNING id, script_id, owner_id, name, trigger_type, cron_expression,
                     event_name, event_module, event_filter, webhook_token, input_vars,
                     is_active, last_fired_at, fire_count, created_at"#,
    )
    .bind(trigger_id)
    .bind(&name)
    .bind(&cron_expression)
    .bind(&event_name)
    .bind(&event_module)
    .bind(&event_filter)
    .bind(&input_vars)
    .bind(is_active)
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "trigger": trigger })))
}

// ── Delete ────────────────────────────────────────────────────────────────────

pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(trigger_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let affected = sqlx::query(
        "DELETE FROM office_script.triggers WHERE id = $1 AND owner_id = $2",
    )
    .bind(trigger_id)
    .bind(user.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(OfficeError::NotFound(format!("Déclencheur {trigger_id} introuvable")));
    }
    Ok(Json(json!({ "deleted": true })))
}

// ── Toggle ────────────────────────────────────────────────────────────────────

pub async fn toggle(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(trigger_id): Path<Uuid>,
) -> Result<Json<Value>> {
    let existing = fetch_trigger(&state.db, trigger_id, user.id).await?;
    let new_active = !existing.is_active;

    sqlx::query(
        "UPDATE office_script.triggers SET is_active = $2 WHERE id = $1 AND owner_id = $3",
    )
    .bind(trigger_id)
    .bind(new_active)
    .bind(user.id)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({ "is_active": new_active })))
}

// ── Helper ────────────────────────────────────────────────────────────────────

async fn fetch_trigger(pool: &sqlx::PgPool, id: Uuid, owner_id: Uuid) -> Result<Trigger> {
    sqlx::query_as::<_, Trigger>(
        r#"SELECT id, script_id, owner_id, name, trigger_type, cron_expression,
                  event_name, event_module, event_filter, webhook_token, input_vars,
                  is_active, last_fired_at, fire_count, created_at
           FROM office_script.triggers WHERE id = $1 AND owner_id = $2"#,
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| OfficeError::NotFound(format!("Déclencheur {id} introuvable")))
}
