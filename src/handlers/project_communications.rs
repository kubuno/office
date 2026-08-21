//! Who is told what, by whom, how often — and what was decided.
//!
//! A communication plan listed on its own is a wish list. It becomes useful when
//! it is crossed with the stakeholder register, because the question worth asking
//! is not "what do we send?" but "who receives nothing?" — a stakeholder with
//! power and interest and no line in this plan is the same defect as a requirement
//! nothing realises.
//!
//! The decision log beside it answers a different problem: a project makes choices
//! that outlive the reason for them, and six months on nobody remembers what was
//! ruled out or why.
use axum::{extract::{Path, State}, Extension, Json};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    handlers::project_authz::{require_permission, task_in_project, Level},
    middleware::OfficeUser,
    state::AppState,
};

const CHANNELS: [&str; 7] = ["email", "meeting", "report", "dashboard", "chat", "workshop", "other"];
const FREQUENCIES: [&str; 8] = ["daily", "weekly", "biweekly", "monthly", "quarterly",
                                "milestone", "on_demand", "once"];
const DECISION_STATUSES: [&str; 4] = ["proposed", "decided", "superseded", "rejected"];

const COMM_COLS: &str = "id, project_id, name, purpose, channel, format, frequency, owner_id, \
     next_due, is_active, position, created_at, updated_at";

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Communication {
    id:         Uuid,
    project_id: Uuid,
    name:       String,
    purpose:    String,
    channel:    String,
    format:     String,
    frequency:  String,
    owner_id:   Option<Uuid>,
    next_due:   Option<NaiveDate>,
    is_active:  bool,
    position:   i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

const DECISION_COLS: &str = "d.id, d.project_id, d.code, d.title, d.context, d.decision, \
     d.rationale, d.alternatives, d.consequences, d.status, d.decided_on, d.decided_by, \
     d.stakeholder_id, d.task_id, d.risk_id, d.supersedes_id, d.position, \
     d.created_at, d.updated_at, s.name AS stakeholder_name, t.name AS task_name, \
     r.code AS risk_code, p.title AS supersedes_title";

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Decision {
    id:               Uuid,
    project_id:       Uuid,
    code:             String,
    title:            String,
    context:          String,
    decision:         String,
    rationale:        String,
    alternatives:     String,
    consequences:     String,
    status:           String,
    decided_on:       Option<NaiveDate>,
    decided_by:       Option<Uuid>,
    stakeholder_id:   Option<Uuid>,
    stakeholder_name: Option<String>,
    task_id:          Option<Uuid>,
    task_name:        Option<String>,
    risk_id:          Option<Uuid>,
    risk_code:        Option<String>,
    supersedes_id:    Option<Uuid>,
    supersedes_title: Option<String>,
    position:         i32,
    created_at:       chrono::DateTime<chrono::Utc>,
    updated_at:       chrono::DateTime<chrono::Utc>,
}

fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where T: Deserialize<'de>, D: Deserializer<'de> {
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct CommDto {
    pub name:      Option<String>,
    pub purpose:   Option<String>,
    pub channel:   Option<String>,
    pub format:    Option<String>,
    pub frequency: Option<String>,
    pub is_active: Option<bool>,
    pub position:  Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub owner_id:  Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub next_due:  Option<Option<NaiveDate>>,
    /// The whole audience, replaced as a set. Absent leaves it untouched.
    pub audience:  Option<Vec<Uuid>>,
}

#[derive(Debug, Deserialize)]
pub struct LogDto {
    pub communication_id: Option<Uuid>,
    pub sent_on:          Option<NaiveDate>,
    pub summary:          Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DecisionDto {
    pub code:         Option<String>,
    pub title:        Option<String>,
    pub context:      Option<String>,
    pub decision:     Option<String>,
    pub rationale:    Option<String>,
    pub alternatives: Option<String>,
    pub consequences: Option<String>,
    pub status:       Option<String>,
    pub position:     Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub decided_on:     Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub stakeholder_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub task_id:        Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub risk_id:        Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub supersedes_id:  Option<Option<Uuid>>,
}

fn check(value: &str, allowed: &[&str], field: &str) -> Result<String> {
    let v = value.trim();
    if !allowed.contains(&v) {
        return Err(OfficeError::Validation(format!(
            "{field} : valeur « {v} » refusée. Valeurs admises : {}.", allowed.join(", ")
        )));
    }
    Ok(v.to_string())
}

/// Every stakeholder named must belong to this project — otherwise a plan could
/// address someone else's register.
async fn stakeholders_in_project(state: &AppState, project_id: Uuid, ids: &[Uuid]) -> Result<()> {
    if ids.is_empty() { return Ok(()); }
    let found: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pm_stakeholder WHERE project_id = $1 AND id = ANY($2)",
    ).bind(project_id).bind(ids).fetch_one(&state.db).await?;
    if found as usize != ids.len() {
        return Err(OfficeError::NotFound(
            "Une des parties prenantes désignées n'appartient pas à ce projet.".into()));
    }
    Ok(())
}

/// GET /projects/:id/communications
pub async fn plan(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let comms = sqlx::query_as::<_, Communication>(
        &format!("SELECT {COMM_COLS} FROM pm_communication WHERE project_id = $1 \
                  ORDER BY position, created_at"),
    ).bind(project_id).fetch_all(&state.db).await?;

    let audience = sqlx::query_as::<_, (Uuid, Uuid, String)>(
        "SELECT a.communication_id, a.stakeholder_id, s.name \
         FROM pm_communication_audience a \
         JOIN pm_stakeholder s ON s.id = a.stakeholder_id \
         JOIN pm_communication c ON c.id = a.communication_id \
         WHERE c.project_id = $1 ORDER BY s.name",
    ).bind(project_id).fetch_all(&state.db).await?;
    let mut by_comm: std::collections::HashMap<Uuid, Vec<Value>> = std::collections::HashMap::new();
    let mut covered: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for (comm_id, holder_id, name) in &audience {
        by_comm.entry(*comm_id).or_default().push(json!({ "id": holder_id, "name": name }));
        covered.insert(*holder_id);
    }

    let today = Utc::now().date_naive();
    let mut overdue = 0i64;
    let rows: Vec<Value> = comms.iter().map(|c| {
        let late = c.is_active && c.next_due.is_some_and(|d| d < today);
        if late { overdue += 1 }
        json!({
            "communication": c,
            "audience": by_comm.get(&c.id).cloned().unwrap_or_default(),
            "overdue": late,
        })
    }).collect();

    // The audit: who the plan reaches nobody about. Weighted by how much they
    // matter, because an uncovered regulator is not an uncovered bystander.
    let holders = sqlx::query_as::<_, (Uuid, String, i32, i32, String)>(
        "SELECT id, name, power, interest, engagement_desired FROM pm_stakeholder \
         WHERE project_id = $1 ORDER BY (power * interest) DESC, position",
    ).bind(project_id).fetch_all(&state.db).await?;
    let uncovered: Vec<Value> = holders.iter()
        .filter(|(id, ..)| !covered.contains(id))
        .map(|(id, name, power, interest, desired)| json!({
            "id": id, "name": name, "power": power, "interest": interest,
            "weight": power * interest, "engagement_desired": desired,
        }))
        .collect();

    Ok(Json(json!({
        "communications": rows,
        "summary": {
            "total": comms.len(),
            "active": comms.iter().filter(|c| c.is_active).count(),
            "overdue": overdue,
            "stakeholders": holders.len(),
            "covered": covered.len(),
            "uncovered": uncovered.len(),
        },
        "uncovered": uncovered,
    })))
}

async fn replace_audience(state: &AppState, comm_id: Uuid, ids: &[Uuid]) -> Result<()> {
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM pm_communication_audience WHERE communication_id = $1")
        .bind(comm_id).execute(&mut *tx).await?;
    if !ids.is_empty() {
        sqlx::query(
            "INSERT INTO pm_communication_audience (communication_id, stakeholder_id) \
             SELECT $1, UNNEST($2::uuid[]) ON CONFLICT DO NOTHING",
        ).bind(comm_id).bind(ids).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// POST /projects/:id/communications
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<CommDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let name = dto.name.as_deref().map(str::trim).filter(|s| !s.is_empty())
        .ok_or_else(|| OfficeError::Validation("Nommez la communication.".into()))?.to_string();
    let channel = match dto.channel.as_deref() { Some(c) => check(c, &CHANNELS, "Canal")?, None => "email".into() };
    let frequency = match dto.frequency.as_deref() { Some(f) => check(f, &FREQUENCIES, "Fréquence")?, None => "weekly".into() };
    let audience = dto.audience.unwrap_or_default();
    stakeholders_in_project(&state, project_id, &audience).await?;

    let position = match dto.position {
        Some(p) => p,
        None => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(position) FROM pm_communication WHERE project_id = $1",
        ).bind(project_id).fetch_one(&state.db).await?.map_or(0, |m| m + 1),
    };
    let comm = sqlx::query_as::<_, Communication>(&format!(
        "INSERT INTO pm_communication (project_id, name, purpose, channel, format, frequency, \
             owner_id, next_due, is_active, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, COALESCE($9, TRUE), $10) RETURNING {COMM_COLS}"
    ))
    .bind(project_id).bind(&name)
    .bind(dto.purpose.as_deref().unwrap_or_default())
    .bind(&channel).bind(dto.format.as_deref().unwrap_or_default()).bind(&frequency)
    .bind(dto.owner_id.flatten()).bind(dto.next_due.flatten())
    .bind(dto.is_active).bind(position)
    .fetch_one(&state.db).await?;

    replace_audience(&state, comm.id, &audience).await?;
    Ok(Json(json!({ "communication": comm })))
}

/// PATCH /projects/:id/communications/:cid
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, comm_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<CommDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let channel = match dto.channel.as_deref() { Some(c) => Some(check(c, &CHANNELS, "Canal")?), None => None };
    let frequency = match dto.frequency.as_deref() { Some(f) => Some(check(f, &FREQUENCIES, "Fréquence")?), None => None };
    if let Some(ids) = dto.audience.as_deref() {
        stakeholders_in_project(&state, project_id, ids).await?;
    }

    let comm = sqlx::query_as::<_, Communication>(&format!(
        "UPDATE pm_communication SET \
            name = COALESCE($3, name), purpose = COALESCE($4, purpose), \
            channel = COALESCE($5, channel), format = COALESCE($6, format), \
            frequency = COALESCE($7, frequency), \
            owner_id = CASE WHEN $8::boolean THEN $9::uuid ELSE owner_id END, \
            next_due = CASE WHEN $10::boolean THEN $11::date ELSE next_due END, \
            is_active = COALESCE($12, is_active), position = COALESCE($13, position), \
            updated_at = now() \
         WHERE id = $1 AND project_id = $2 RETURNING {COMM_COLS}"
    ))
    .bind(comm_id).bind(project_id)
    .bind(dto.name.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.purpose.as_deref()).bind(channel.as_deref())
    .bind(dto.format.as_deref()).bind(frequency.as_deref())
    .bind(dto.owner_id.is_some()).bind(dto.owner_id.flatten())
    .bind(dto.next_due.is_some()).bind(dto.next_due.flatten())
    .bind(dto.is_active).bind(dto.position)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Communication introuvable".into()))?;

    if let Some(ids) = dto.audience.as_deref() {
        replace_audience(&state, comm_id, ids).await?;
    }
    Ok(Json(json!({ "communication": comm })))
}

/// DELETE /projects/:id/communications/:cid
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, comm_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query("DELETE FROM pm_communication WHERE id = $1 AND project_id = $2")
        .bind(comm_id).bind(project_id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Communication introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}

/// POST /projects/:id/communications/log — record that something went out.
///
/// Also moves the plan forward: a communication that was sent has a next date,
/// and leaving it in the past would keep it reported as overdue for ever.
pub async fn log_sent(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<LogDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let sent_on = dto.sent_on.unwrap_or_else(|| Utc::now().date_naive());

    let frequency = match dto.communication_id {
        Some(cid) => sqlx::query_scalar::<_, String>(
            "SELECT frequency FROM pm_communication WHERE id = $1 AND project_id = $2",
        ).bind(cid).bind(project_id).fetch_optional(&state.db).await?
         .ok_or_else(|| OfficeError::NotFound("Communication introuvable".into()))?,
        None => String::new(),
    };

    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO pm_communication_log (project_id, communication_id, sent_on, summary, sent_by) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(project_id).bind(dto.communication_id).bind(sent_on)
    .bind(dto.summary.as_deref().map(str::trim).unwrap_or_default()).bind(user.id)
    .fetch_one(&mut *tx).await?;

    let next = match frequency.as_str() {
        "daily"     => Some(sent_on + chrono::Duration::days(1)),
        "weekly"    => Some(sent_on + chrono::Duration::weeks(1)),
        "biweekly"  => Some(sent_on + chrono::Duration::weeks(2)),
        "monthly"   => Some(sent_on + chrono::Duration::days(30)),
        "quarterly" => Some(sent_on + chrono::Duration::days(91)),
        // A one-off, a milestone report or an on-demand note has no next date to
        // compute; leaving the old one would misreport it as late.
        _ => None,
    };
    if let Some(cid) = dto.communication_id {
        sqlx::query("UPDATE pm_communication SET next_due = $2, updated_at = now() WHERE id = $1")
            .bind(cid).bind(next).execute(&mut *tx).await?;
    }
    tx.commit().await?;

    Ok(Json(json!({ "id": id, "sent_on": sent_on, "next_due": next })))
}

/// GET /projects/:id/communications/log
pub async fn list_log(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let rows = sqlx::query_as::<_, (Uuid, Option<Uuid>, Option<String>, NaiveDate, String)>(
        "SELECT l.id, l.communication_id, c.name, l.sent_on, l.summary \
         FROM pm_communication_log l \
         LEFT JOIN pm_communication c ON c.id = l.communication_id \
         WHERE l.project_id = $1 ORDER BY l.sent_on DESC, l.created_at DESC LIMIT 200",
    ).bind(project_id).fetch_all(&state.db).await?;
    let entries: Vec<Value> = rows.into_iter().map(|(id, cid, name, day, summary)| json!({
        "id": id, "communication_id": cid, "communication_name": name,
        "sent_on": day, "summary": summary,
    })).collect();
    Ok(Json(json!({ "entries": entries })))
}

// ── Decision log ─────────────────────────────────────────────────────────────

const DECISION_FROM: &str = "FROM pm_decision d \
     LEFT JOIN pm_stakeholder s ON s.id = d.stakeholder_id \
     LEFT JOIN tasks t ON t.id = d.task_id AND t.project_id = d.project_id \
     LEFT JOIN pm_risk r ON r.id = d.risk_id \
     LEFT JOIN pm_decision p ON p.id = d.supersedes_id";

/// GET /projects/:id/decisions
pub async fn list_decisions(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let decisions = sqlx::query_as::<_, Decision>(&format!(
        "SELECT {DECISION_COLS} {DECISION_FROM} WHERE d.project_id = $1 \
         ORDER BY d.decided_on DESC NULLS FIRST, d.position, d.created_at DESC"
    )).bind(project_id).fetch_all(&state.db).await?;

    let pending = decisions.iter().filter(|d| d.status == "proposed").count();
    // A decision recorded without the reasoning behind it cannot be revisited
    // later — which is the only reason to keep a log.
    let unexplained = decisions.iter()
        .filter(|d| d.status == "decided" && d.rationale.trim().is_empty())
        .map(|d| json!({ "id": d.id, "code": d.code, "title": d.title }))
        .collect::<Vec<_>>();

    Ok(Json(json!({
        "decisions": decisions,
        "summary": {
            "total": decisions.len(),
            "pending": pending,
            "decided": decisions.iter().filter(|d| d.status == "decided").count(),
            "superseded": decisions.iter().filter(|d| d.status == "superseded").count(),
            "unexplained": unexplained.len(),
        },
        "unexplained": unexplained,
    })))
}

async fn fetch_decision(state: &AppState, project_id: Uuid, id: Uuid) -> Result<Decision> {
    sqlx::query_as::<_, Decision>(&format!(
        "SELECT {DECISION_COLS} {DECISION_FROM} WHERE d.id = $1 AND d.project_id = $2"
    )).bind(id).bind(project_id).fetch_optional(&state.db).await?
      .ok_or_else(|| OfficeError::NotFound("Décision introuvable".into()))
}

/// POST /projects/:id/decisions
pub async fn create_decision(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<DecisionDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let title = dto.title.as_deref().map(str::trim).filter(|s| !s.is_empty())
        .ok_or_else(|| OfficeError::Validation("Nommez la décision.".into()))?.to_string();
    let status = match dto.status.as_deref() {
        Some(s) => check(s, &DECISION_STATUSES, "Statut")?, None => "proposed".into() };
    if let Some(tid) = dto.task_id.flatten() {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }
    if let Some(sid) = dto.stakeholder_id.flatten() {
        stakeholders_in_project(&state, project_id, &[sid]).await?;
    }

    let code = match dto.code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None => {
            let highest: Option<i32> = sqlx::query_scalar(
                "SELECT MAX(CAST(substring(code FROM '^D-([0-9]+)$') AS INT)) \
                 FROM pm_decision WHERE project_id = $1 AND code ~ '^D-[0-9]+$'",
            ).bind(project_id).fetch_one(&state.db).await?;
            format!("D-{:02}", highest.unwrap_or(0) + 1)
        }
    };
    let position = match dto.position {
        Some(p) => p,
        None => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(position) FROM pm_decision WHERE project_id = $1",
        ).bind(project_id).fetch_one(&state.db).await?.map_or(0, |m| m + 1),
    };

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO pm_decision (project_id, code, title, context, decision, rationale, \
             alternatives, consequences, status, decided_on, decided_by, stakeholder_id, \
             task_id, risk_id, supersedes_id, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, \
                 CASE WHEN $9 = 'decided' THEN COALESCE($10, CURRENT_DATE) ELSE $10 END, \
                 $11, $12, $13, $14, $15, $16) RETURNING id",
    )
    .bind(project_id).bind(&code).bind(&title)
    .bind(dto.context.as_deref().unwrap_or_default())
    .bind(dto.decision.as_deref().unwrap_or_default())
    .bind(dto.rationale.as_deref().unwrap_or_default())
    .bind(dto.alternatives.as_deref().unwrap_or_default())
    .bind(dto.consequences.as_deref().unwrap_or_default())
    .bind(&status).bind(dto.decided_on.flatten())
    .bind(if status == "decided" { Some(user.id) } else { None })
    .bind(dto.stakeholder_id.flatten()).bind(dto.task_id.flatten())
    .bind(dto.risk_id.flatten()).bind(dto.supersedes_id.flatten()).bind(position)
    .fetch_one(&state.db).await?;

    // A decision that replaces another says so on both sides, or the log reads as
    // a pile of contradictions.
    if let Some(old) = dto.supersedes_id.flatten() {
        sqlx::query(
            "UPDATE pm_decision SET status = 'superseded', updated_at = now() \
             WHERE id = $1 AND project_id = $2 AND status <> 'superseded'",
        ).bind(old).bind(project_id).execute(&state.db).await?;
    }

    Ok(Json(json!({ "decision": fetch_decision(&state, project_id, id).await? })))
}

/// PATCH /projects/:id/decisions/:did
pub async fn update_decision(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, decision_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<DecisionDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let status = match dto.status.as_deref() { Some(s) => Some(check(s, &DECISION_STATUSES, "Statut")?), None => None };
    if let Some(Some(tid)) = dto.task_id {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }
    if let Some(Some(sid)) = dto.stakeholder_id {
        stakeholders_in_project(&state, project_id, &[sid]).await?;
    }
    if let Some(Some(old)) = dto.supersedes_id {
        if old == decision_id {
            return Err(OfficeError::Validation("Une décision ne peut pas se remplacer elle-même.".into()));
        }
    }

    let rows = sqlx::query(
        "UPDATE pm_decision SET \
            code = COALESCE($3, code), title = COALESCE($4, title), \
            context = COALESCE($5, context), decision = COALESCE($6, decision), \
            rationale = COALESCE($7, rationale), alternatives = COALESCE($8, alternatives), \
            consequences = COALESCE($9, consequences), status = COALESCE($10, status), \
            decided_on = CASE WHEN $11::boolean THEN $12::date \
                              WHEN $10 = 'decided' THEN COALESCE(decided_on, CURRENT_DATE) \
                              ELSE decided_on END, \
            decided_by = CASE WHEN $10 = 'decided' THEN COALESCE(decided_by, $13::uuid) ELSE decided_by END, \
            stakeholder_id = CASE WHEN $14::boolean THEN $15::uuid ELSE stakeholder_id END, \
            task_id = CASE WHEN $16::boolean THEN $17::uuid ELSE task_id END, \
            risk_id = CASE WHEN $18::boolean THEN $19::uuid ELSE risk_id END, \
            supersedes_id = CASE WHEN $20::boolean THEN $21::uuid ELSE supersedes_id END, \
            position = COALESCE($22, position), updated_at = now() \
         WHERE id = $1 AND project_id = $2",
    )
    .bind(decision_id).bind(project_id)
    .bind(dto.code.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.title.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.context.as_deref()).bind(dto.decision.as_deref())
    .bind(dto.rationale.as_deref()).bind(dto.alternatives.as_deref())
    .bind(dto.consequences.as_deref()).bind(status.as_deref())
    .bind(dto.decided_on.is_some()).bind(dto.decided_on.flatten())
    .bind(user.id)
    .bind(dto.stakeholder_id.is_some()).bind(dto.stakeholder_id.flatten())
    .bind(dto.task_id.is_some()).bind(dto.task_id.flatten())
    .bind(dto.risk_id.is_some()).bind(dto.risk_id.flatten())
    .bind(dto.supersedes_id.is_some()).bind(dto.supersedes_id.flatten())
    .bind(dto.position)
    .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Décision introuvable".into())); }

    if let Some(Some(old)) = dto.supersedes_id {
        sqlx::query(
            "UPDATE pm_decision SET status = 'superseded', updated_at = now() \
             WHERE id = $1 AND project_id = $2 AND status <> 'superseded'",
        ).bind(old).bind(project_id).execute(&state.db).await?;
    }
    Ok(Json(json!({ "decision": fetch_decision(&state, project_id, decision_id).await? })))
}

/// DELETE /projects/:id/decisions/:did
pub async fn delete_decision(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, decision_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query("DELETE FROM pm_decision WHERE id = $1 AND project_id = $2")
        .bind(decision_id).bind(project_id)
        .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Décision introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}
