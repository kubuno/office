//! Who the project has to deal with, and who answers for what.
//!
//! The register is not a contact list. Its two useful outputs are the
//! power/interest grid — which says how much attention each person is owed — and
//! the gap between how engaged someone is today and how engaged the project needs
//! them to be. A register that records only the present state describes a
//! situation instead of asking for anything.
//!
//! The RACI beside it carries one rule worth enforcing: exactly one person
//! answers for a task. Two accountable people is nobody accountable.
use axum::{extract::{Path, State}, Extension, Json};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    handlers::project_authz::{require_permission, task_in_project, Level},
    middleware::OfficeUser,
    state::AppState,
};

const CATEGORIES: [&str; 7] = ["internal", "external", "sponsor", "customer",
                               "supplier", "regulator", "team"];
/// PMBOK's five levels, in order. The order matters: it is what makes a gap a
/// direction rather than just a difference.
const ENGAGEMENT: [&str; 5] = ["unaware", "resistant", "neutral", "supportive", "leading"];
const ROLES: [&str; 4] = ["R", "A", "C", "I"];

const COLS: &str = "id, project_id, name, organisation, role_title, contact_email, category, \
     power, interest, engagement_current, engagement_desired, expectations, \
     influence_notes, communication_notes, user_id, position, created_at, updated_at";

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Stakeholder {
    id:                  Uuid,
    project_id:          Uuid,
    name:                String,
    organisation:        String,
    role_title:          String,
    contact_email:       String,
    category:            String,
    power:               i32,
    interest:            i32,
    engagement_current:  String,
    engagement_desired:  String,
    expectations:        String,
    influence_notes:     String,
    communication_notes: String,
    user_id:             Option<Uuid>,
    position:            i32,
    created_at:          chrono::DateTime<chrono::Utc>,
    updated_at:          chrono::DateTime<chrono::Utc>,
}

fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where T: Deserialize<'de>, D: Deserializer<'de> {
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct StakeholderDto {
    pub name:                Option<String>,
    pub organisation:        Option<String>,
    pub role_title:          Option<String>,
    pub contact_email:       Option<String>,
    pub category:            Option<String>,
    pub power:               Option<i32>,
    pub interest:            Option<i32>,
    pub engagement_current:  Option<String>,
    pub engagement_desired:  Option<String>,
    pub expectations:        Option<String>,
    pub influence_notes:     Option<String>,
    pub communication_notes: Option<String>,
    pub position:            Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub user_id:             Option<Option<Uuid>>,
}

#[derive(Debug, Deserialize)]
pub struct RaciDto { pub role: Option<String> }

fn check(value: &str, allowed: &[&str], field: &str) -> Result<String> {
    let v = value.trim();
    if !allowed.contains(&v) {
        return Err(OfficeError::Validation(format!(
            "{field} : valeur « {v} » refusée. Valeurs admises : {}.", allowed.join(", ")
        )));
    }
    Ok(v.to_string())
}

fn check_scale(value: i32, field: &str) -> Result<i32> {
    if !(1..=5).contains(&value) {
        return Err(OfficeError::Validation(format!(
            "{field} : attendu une valeur de 1 à 5, reçu {value}."
        )));
    }
    Ok(value)
}

/// Which of the four quadrants a stakeholder falls in. The grid is only useful
/// because each quadrant prescribes a different amount of attention.
fn quadrant(power: i32, interest: i32) -> &'static str {
    match (power >= 4, interest >= 4) {
        (true,  true)  => "manage_closely",
        (true,  false) => "keep_satisfied",
        (false, true)  => "keep_informed",
        (false, false) => "monitor",
    }
}

fn level_index(level: &str) -> i32 {
    ENGAGEMENT.iter().position(|l| *l == level).unwrap_or(2) as i32
}

/// GET /projects/:id/stakeholders
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let holders = sqlx::query_as::<_, Stakeholder>(
        &format!("SELECT {COLS} FROM pm_stakeholder WHERE project_id = $1 \
                  ORDER BY (power * interest) DESC, position, created_at"),
    ).bind(project_id).fetch_all(&state.db).await?;

    // The grid: how many people sit at each power/interest cell.
    let mut grid = vec![vec![0i64; 5]; 5];
    let mut by_quadrant: std::collections::HashMap<&str, i64> = std::collections::HashMap::new();
    let mut gaps: Vec<Value> = Vec::new();
    for h in &holders {
        grid[(h.power - 1) as usize][(h.interest - 1) as usize] += 1;
        *by_quadrant.entry(quadrant(h.power, h.interest)).or_insert(0) += 1;
        let (now, want) = (level_index(&h.engagement_current), level_index(&h.engagement_desired));
        if now != want {
            gaps.push(json!({
                "id": h.id, "name": h.name, "power": h.power, "interest": h.interest,
                "current": h.engagement_current, "desired": h.engagement_desired,
                // Positive: they need to be brought along. Negative: the project
                // wants less involvement than it currently has, which is rare and
                // worth noticing rather than hiding.
                "distance": want - now,
                "quadrant": quadrant(h.power, h.interest),
            }));
        }
    }
    // The furthest from where they need to be, among those who matter most.
    gaps.sort_by(|a, b| {
        let key = |v: &Value| (
            v["distance"].as_i64().unwrap_or(0).abs(),
            v["power"].as_i64().unwrap_or(0) * v["interest"].as_i64().unwrap_or(0),
        );
        key(b).cmp(&key(a))
    });

    Ok(Json(json!({
        "stakeholders": holders,
        "grid": grid,
        "summary": {
            "total": holders.len(),
            "by_quadrant": {
                "manage_closely": by_quadrant.get("manage_closely").copied().unwrap_or(0),
                "keep_satisfied": by_quadrant.get("keep_satisfied").copied().unwrap_or(0),
                "keep_informed":  by_quadrant.get("keep_informed").copied().unwrap_or(0),
                "monitor":        by_quadrant.get("monitor").copied().unwrap_or(0),
            },
            "aligned": holders.len() - gaps.len(),
        },
        "gaps": gaps,
    })))
}

/// POST /projects/:id/stakeholders
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<StakeholderDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let name = dto.name.as_deref().map(str::trim).filter(|s| !s.is_empty())
        .ok_or_else(|| OfficeError::Validation("Nommez la partie prenante.".into()))?
        .to_string();
    let category = match dto.category.as_deref() {
        Some(c) => check(c, &CATEGORIES, "Catégorie")?, None => "internal".into() };
    let current = match dto.engagement_current.as_deref() {
        Some(e) => check(e, &ENGAGEMENT, "Engagement actuel")?, None => "neutral".into() };
    let desired = match dto.engagement_desired.as_deref() {
        Some(e) => check(e, &ENGAGEMENT, "Engagement souhaité")?, None => "supportive".into() };
    let power = check_scale(dto.power.unwrap_or(3), "Pouvoir")?;
    let interest = check_scale(dto.interest.unwrap_or(3), "Intérêt")?;
    let position = match dto.position {
        Some(p) => p,
        None => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(position) FROM pm_stakeholder WHERE project_id = $1",
        ).bind(project_id).fetch_one(&state.db).await?.map_or(0, |m| m + 1),
    };

    let holder = sqlx::query_as::<_, Stakeholder>(&format!(
        "INSERT INTO pm_stakeholder (project_id, name, organisation, role_title, contact_email, \
             category, power, interest, engagement_current, engagement_desired, expectations, \
             influence_notes, communication_notes, user_id, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) \
         RETURNING {COLS}"
    ))
    .bind(project_id).bind(&name)
    .bind(dto.organisation.as_deref().unwrap_or_default())
    .bind(dto.role_title.as_deref().unwrap_or_default())
    .bind(dto.contact_email.as_deref().map(str::trim).unwrap_or_default())
    .bind(&category).bind(power).bind(interest).bind(&current).bind(&desired)
    .bind(dto.expectations.as_deref().unwrap_or_default())
    .bind(dto.influence_notes.as_deref().unwrap_or_default())
    .bind(dto.communication_notes.as_deref().unwrap_or_default())
    .bind(dto.user_id.flatten()).bind(position)
    .fetch_one(&state.db).await?;
    Ok(Json(json!({ "stakeholder": holder })))
}

/// PATCH /projects/:id/stakeholders/:sid
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, holder_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<StakeholderDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let category = match dto.category.as_deref() { Some(c) => Some(check(c, &CATEGORIES, "Catégorie")?), None => None };
    let current  = match dto.engagement_current.as_deref() { Some(e) => Some(check(e, &ENGAGEMENT, "Engagement actuel")?), None => None };
    let desired  = match dto.engagement_desired.as_deref() { Some(e) => Some(check(e, &ENGAGEMENT, "Engagement souhaité")?), None => None };
    if let Some(p) = dto.power { check_scale(p, "Pouvoir")?; }
    if let Some(i) = dto.interest { check_scale(i, "Intérêt")?; }

    let holder = sqlx::query_as::<_, Stakeholder>(&format!(
        "UPDATE pm_stakeholder SET \
            name = COALESCE($3, name), \
            organisation = COALESCE($4, organisation), \
            role_title = COALESCE($5, role_title), \
            contact_email = COALESCE($6, contact_email), \
            category = COALESCE($7, category), \
            power = COALESCE($8, power), \
            interest = COALESCE($9, interest), \
            engagement_current = COALESCE($10, engagement_current), \
            engagement_desired = COALESCE($11, engagement_desired), \
            expectations = COALESCE($12, expectations), \
            influence_notes = COALESCE($13, influence_notes), \
            communication_notes = COALESCE($14, communication_notes), \
            user_id = CASE WHEN $15::boolean THEN $16::uuid ELSE user_id END, \
            position = COALESCE($17, position), \
            updated_at = now() \
         WHERE id = $1 AND project_id = $2 RETURNING {COLS}"
    ))
    .bind(holder_id).bind(project_id)
    .bind(dto.name.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.organisation.as_deref()).bind(dto.role_title.as_deref())
    .bind(dto.contact_email.as_deref().map(str::trim))
    .bind(category.as_deref()).bind(dto.power).bind(dto.interest)
    .bind(current.as_deref()).bind(desired.as_deref())
    .bind(dto.expectations.as_deref()).bind(dto.influence_notes.as_deref())
    .bind(dto.communication_notes.as_deref())
    .bind(dto.user_id.is_some()).bind(dto.user_id.flatten())
    .bind(dto.position)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Partie prenante introuvable".into()))?;
    Ok(Json(json!({ "stakeholder": holder })))
}

/// DELETE /projects/:id/stakeholders/:sid
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, holder_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query("DELETE FROM pm_stakeholder WHERE id = $1 AND project_id = $2")
        .bind(holder_id).bind(project_id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Partie prenante introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}

/// GET /projects/:id/raci — the matrix, plus what it is missing.
pub async fn matrix(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;

    // A task with children is a rollup: assigning a role on it duplicates what its
    // children already say. Read from the tree rather than from `task_type`, which
    // a task can carry without ever having been made a parent — and vice versa.
    let tasks = sqlx::query_as::<_, (Uuid, String, String, String, bool)>(
        "SELECT t.id, t.wbs, t.name, t.task_type, \
                EXISTS(SELECT 1 FROM tasks c WHERE c.parent_id = t.id) AS has_children \
         FROM tasks t WHERE t.project_id = $1 ORDER BY t.wbs",
    ).bind(project_id).fetch_all(&state.db).await?;
    let holders = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT id, name, role_title FROM pm_stakeholder WHERE project_id = $1 \
         ORDER BY position, created_at",
    ).bind(project_id).fetch_all(&state.db).await?;
    let cells = sqlx::query_as::<_, (Uuid, Uuid, String)>(
        "SELECT task_id, stakeholder_id, role FROM pm_raci WHERE project_id = $1",
    ).bind(project_id).fetch_all(&state.db).await?;

    let mut by_task: std::collections::HashMap<Uuid, Vec<(Uuid, String)>> = std::collections::HashMap::new();
    for (task_id, holder_id, role) in &cells {
        by_task.entry(*task_id).or_default().push((*holder_id, role.clone()));
    }

    let rows: Vec<Value> = tasks.iter().map(|(id, wbs, name, kind, has_children)| {
        let assigned = by_task.get(id);
        let accountable = assigned.and_then(|v| v.iter().find(|(_, r)| r == "A").map(|(h, _)| *h));
        let responsible: Vec<Uuid> = assigned.map(|v| v.iter().filter(|(_, r)| r == "R").map(|(h, _)| *h).collect()).unwrap_or_default();
        json!({
            "task_id": id, "wbs": wbs, "name": name, "task_type": kind,
            "is_rollup": has_children,
            "cells": assigned.map(|v| v.iter().map(|(h, r)| json!({ "stakeholder_id": h, "role": r })).collect::<Vec<_>>()).unwrap_or_default(),
            "accountable": accountable,
            "responsible_count": responsible.len(),
        })
    }).collect();

    // What the matrix is missing is the point of reading it: a task nobody
    // answers for, and a task nobody is doing.
    let no_accountable: Vec<Value> = rows.iter()
        .filter(|r| r["is_rollup"] == false && r["task_type"] != "summary" && r["accountable"].is_null())
        .map(|r| json!({ "task_id": r["task_id"], "wbs": r["wbs"], "name": r["name"] }))
        .collect();
    let no_responsible: Vec<Value> = rows.iter()
        .filter(|r| r["is_rollup"] == false && r["task_type"] != "summary" && r["responsible_count"] == 0)
        .map(|r| json!({ "task_id": r["task_id"], "wbs": r["wbs"], "name": r["name"] }))
        .collect();

    Ok(Json(json!({
        "stakeholders": holders.iter().map(|(id, name, role)| json!({ "id": id, "name": name, "role_title": role })).collect::<Vec<_>>(),
        "tasks": rows,
        "gaps": { "no_accountable": no_accountable, "no_responsible": no_responsible },
    })))
}

/// PUT /projects/:id/tasks/:tid/raci/:sid — assign a role.
pub async fn set_role(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, task_id, holder_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(dto): Json<RaciDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let role = dto.role.as_deref().map(|r| r.trim().to_uppercase())
        .ok_or_else(|| OfficeError::Validation("Indiquez le rôle (R, A, C ou I).".into()))?;
    let role = check(&role, &ROLES, "Rôle")?;

    if !task_in_project(&state, project_id, task_id).await? {
        return Err(OfficeError::NotFound(format!("Tâche {task_id}")));
    }
    let holder_ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pm_stakeholder WHERE id = $1 AND project_id = $2)",
    ).bind(holder_id).bind(project_id).fetch_one(&state.db).await?;
    if !holder_ok { return Err(OfficeError::NotFound(format!("Partie prenante {holder_id}"))); }

    // Exactly one person answers for a task. Rather than let the unique index
    // surface as a database error, say who currently holds it.
    if role == "A" {
        let holder: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT s.id, s.name FROM pm_raci r JOIN pm_stakeholder s ON s.id = r.stakeholder_id \
             WHERE r.task_id = $1 AND r.role = 'A'",
        ).bind(task_id).fetch_optional(&state.db).await?;
        if let Some((existing, name)) = holder {
            if existing != holder_id {
                return Err(OfficeError::Validation(format!(
                    "{name} répond déjà de cette tâche. Une tâche n'a qu'un seul responsable — \
                     retirez-lui le rôle avant de l'attribuer à quelqu'un d'autre."
                )));
            }
        }
    }

    sqlx::query(
        "INSERT INTO pm_raci (project_id, task_id, stakeholder_id, role) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (task_id, stakeholder_id) DO UPDATE SET role = EXCLUDED.role",
    ).bind(project_id).bind(task_id).bind(holder_id).bind(&role)
     .execute(&state.db).await?;

    Ok(Json(json!({ "task_id": task_id, "stakeholder_id": holder_id, "role": role })))
}

/// DELETE /projects/:id/tasks/:tid/raci/:sid
pub async fn clear_role(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, task_id, holder_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query(
        "DELETE FROM pm_raci WHERE project_id = $1 AND task_id = $2 AND stakeholder_id = $3",
    ).bind(project_id).bind(task_id).bind(holder_id)
     .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Attribution introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}
