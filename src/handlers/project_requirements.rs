//! Requirements and the traceability matrix — what the project was asked for,
//! and what actually fulfils each request.
//!
//! A requirement on its own is a wish list entry. The value is in the links: every
//! requirement should reach a deliverable that satisfies it or a work package that
//! builds it, and every deliverable should answer some requirement. The two
//! questions worth asking are therefore about what is MISSING — requirements
//! nothing realises, deliverables nothing justifies — which is what
//! [`traceability`] reports as `orphans`.
//!
//! Every statement here is scoped to the project in the URL. An identifier that
//! belongs to another project is answered with 404 and never acted upon.
use std::collections::{HashMap, HashSet};

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

const REQ_COLS: &str = "id, project_id, code, title, description, req_type, priority, source, \
     rationale, status, verification_method, verification_notes, verified_at, position, \
     created_at, updated_at";

/// Resolved traceability links, joined to the names their targets carry so the
/// matrix reads as words rather than identifiers.
const LINK_SELECT: &str = "SELECT l.id, l.requirement_id, l.deliverable_id, \
            d.name AS deliverable_name, l.task_id, t.name AS task_name, l.created_at \
     FROM pm_requirement_link l \
     LEFT JOIN pm_deliverable d ON d.id = l.deliverable_id \
     LEFT JOIN tasks t ON t.id = l.task_id";

const REQ_TYPES: &[&str] = &[
    "business", "stakeholder", "functional", "non_functional", "transition", "quality", "project",
];
/// MoSCoW — the vocabulary that forces a real ranking.
const PRIORITIES: &[&str] = &["must", "should", "could", "wont"];
const STATUSES: &[&str] = &[
    "proposed", "approved", "implemented", "verified", "deferred", "rejected",
];
const VERIFICATION_METHODS: &[&str] = &["test", "inspection", "demonstration", "analysis"];

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Requirement {
    id:                  Uuid,
    project_id:          Uuid,
    code:                String,
    title:               String,
    description:         String,
    req_type:            String,
    priority:            String,
    source:              String,
    rationale:           String,
    status:              String,
    verification_method: String,
    verification_notes:  String,
    verified_at:         Option<NaiveDate>,
    position:            i32,
    created_at:          chrono::DateTime<chrono::Utc>,
    updated_at:          chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct RequirementLink {
    id:               Uuid,
    requirement_id:   Uuid,
    deliverable_id:   Option<Uuid>,
    deliverable_name: Option<String>,
    task_id:          Option<Uuid>,
    task_name:        Option<String>,
    created_at:       chrono::DateTime<chrono::Utc>,
}

/// Tells an absent field apart from one explicitly set to null: without this a
/// verification date could be changed but never removed.
fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where T: Deserialize<'de>, D: Deserializer<'de> {
    Deserialize::deserialize(de).map(Some)
}

/// One shape for creation and for editing: the fields are the same, only `title`
/// is compulsory on the way in.
#[derive(Debug, Default, Deserialize)]
pub struct RequirementDto {
    pub code:                Option<String>,
    pub title:               Option<String>,
    pub description:         Option<String>,
    pub req_type:            Option<String>,
    pub priority:            Option<String>,
    pub source:              Option<String>,
    pub rationale:           Option<String>,
    pub status:              Option<String>,
    pub verification_method: Option<String>,
    pub verification_notes:  Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub verified_at:         Option<Option<NaiveDate>>,
    pub position:            Option<i32>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LinkDto {
    pub deliverable_id: Option<Uuid>,
    pub task_id:        Option<Uuid>,
}

/// Refuse an unknown value before the database does: a CHECK violation would come
/// back as a bare 500, whereas the caller needs to read which value was refused
/// and what may be used instead.
fn checked<'a>(field: &str, value: Option<&'a str>, allowed: &[&str]) -> Result<Option<&'a str>> {
    match value {
        None => Ok(None),
        Some(v) if allowed.contains(&v) => Ok(Some(v)),
        Some(v) => Err(OfficeError::Validation(format!(
            "{field} : valeur « {v} » refusée. Valeurs admises : {}.",
            allowed.join(", ")
        ))),
    }
}

/// Validate the four constrained fields in one place, returning them ready to bind.
#[allow(clippy::type_complexity)]
fn checked_enums(dto: &RequirementDto)
    -> Result<(Option<&str>, Option<&str>, Option<&str>, Option<&str>)>
{
    Ok((
        checked("Type d'exigence", dto.req_type.as_deref(), REQ_TYPES)?,
        checked("Priorité", dto.priority.as_deref(), PRIORITIES)?,
        checked("Statut", dto.status.as_deref(), STATUSES)?,
        checked("Méthode de vérification", dto.verification_method.as_deref(), VERIFICATION_METHODS)?,
    ))
}

/// A trimmed, non-empty title, or the explanation of why there is none.
fn require_title(raw: Option<&str>) -> Result<String> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| OfficeError::Validation("Donnez un intitulé à l'exigence.".into()))
}

/// Every requirement of the project, in the order people arranged them.
async fn requirements_of(state: &AppState, project_id: Uuid) -> Result<Vec<Requirement>> {
    Ok(sqlx::query_as::<_, Requirement>(&format!(
        "SELECT {REQ_COLS} FROM pm_requirement WHERE project_id = $1 \
         ORDER BY position, created_at"
    ))
    .bind(project_id)
    .fetch_all(&state.db)
    .await?)
}

/// Every link of the project in ONE query — joined through `pm_requirement` so a
/// link hanging off another project's requirement can never appear here, and so
/// listing N requirements never costs N queries.
async fn links_of(state: &AppState, project_id: Uuid) -> Result<Vec<RequirementLink>> {
    Ok(sqlx::query_as::<_, RequirementLink>(&format!(
        "{LINK_SELECT} \
         JOIN pm_requirement r ON r.id = l.requirement_id AND r.project_id = $1 \
         ORDER BY l.created_at"
    ))
    .bind(project_id)
    .fetch_all(&state.db)
    .await?)
}

/// Group the flat link list by requirement, so each requirement can carry its own.
fn group_links(links: Vec<RequirementLink>) -> HashMap<Uuid, Vec<RequirementLink>> {
    let mut by_req: HashMap<Uuid, Vec<RequirementLink>> = HashMap::new();
    for link in links {
        by_req.entry(link.requirement_id).or_default().push(link);
    }
    by_req
}

/// A requirement rendered with its links attached.
fn with_links(req: &Requirement, links: Option<&Vec<RequirementLink>>) -> Value {
    let mut value = serde_json::to_value(req).unwrap_or_else(|_| json!({}));
    let rendered = links
        .map(|l| serde_json::to_value(l).unwrap_or_else(|_| json!([])))
        .unwrap_or_else(|| json!([]));
    if let Some(obj) = value.as_object_mut() {
        obj.insert("links".to_string(), rendered);
    }
    value
}

/// Refuse unless the requirement belongs to the project named in the URL.
async fn require_requirement_in_project(
    state: &AppState,
    project_id: Uuid,
    requirement_id: Uuid,
) -> Result<()> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pm_requirement WHERE id = $1 AND project_id = $2)",
    )
    .bind(requirement_id)
    .bind(project_id)
    .fetch_one(&state.db)
    .await?;
    if !exists {
        return Err(OfficeError::NotFound("Exigence introuvable".into()));
    }
    Ok(())
}

/// GET /projects/:id/requirements — the requirements, each carrying its resolved
/// traceability links. Two queries, whatever the number of requirements.
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let requirements = requirements_of(&state, project_id).await?;
    let by_req = group_links(links_of(&state, project_id).await?);
    let rendered: Vec<Value> = requirements
        .iter()
        .map(|r| with_links(r, by_req.get(&r.id)))
        .collect();
    Ok(Json(json!({ "requirements": rendered })))
}

/// POST /projects/:id/requirements
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<RequirementDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let title = require_title(dto.title.as_deref())?;
    let (req_type, priority, status, verification) = checked_enums(&dto)?;

    // Appended at the end unless told otherwise, so a new requirement does not
    // land in the middle of the ones already listed.
    let position = match dto.position {
        Some(p) => p,
        None => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(position) FROM pm_requirement WHERE project_id = $1",
        )
        .bind(project_id)
        .fetch_one(&state.db)
        .await?
        .map_or(0, |m| m + 1),
    };

    // A requirement without a reference cannot be cited in a discussion, so one is
    // issued when none is given. Derived from the highest number already in use
    // rather than from the count, so deleting EX-02 does not hand its number to
    // the next requirement and make two documents disagree about what EX-02 is.
    let code = match dto.code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None => {
            let highest: Option<i32> = sqlx::query_scalar(
                "SELECT MAX(CAST(substring(code FROM '^EX-([0-9]+)$') AS INT)) \
                 FROM pm_requirement WHERE project_id = $1 AND code ~ '^EX-[0-9]+$'",
            ).bind(project_id).fetch_one(&state.db).await?;
            format!("EX-{:02}", highest.unwrap_or(0) + 1)
        }
    };

    let requirement = sqlx::query_as::<_, Requirement>(&format!(
        "INSERT INTO pm_requirement \
             (project_id, code, title, description, req_type, priority, source, rationale, \
              status, verification_method, verification_notes, verified_at, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
         RETURNING {REQ_COLS}"
    ))
    .bind(project_id)
    .bind(&code)
    .bind(&title)
    .bind(dto.description.as_deref().unwrap_or_default())
    .bind(req_type.unwrap_or("functional"))
    .bind(priority.unwrap_or("should"))
    .bind(dto.source.as_deref().map(str::trim).unwrap_or_default())
    .bind(dto.rationale.as_deref().unwrap_or_default())
    .bind(status.unwrap_or("proposed"))
    .bind(verification.unwrap_or("test"))
    .bind(dto.verification_notes.as_deref().unwrap_or_default())
    .bind(dto.verified_at.flatten())
    .bind(position)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "requirement": with_links(&requirement, None) })))
}

/// PATCH /projects/:id/requirements/:rid
///
/// `verified_at` uses the double-option pattern: absent leaves it alone, an
/// explicit `null` clears it — a verification can be taken back.
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, requirement_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<RequirementDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let (req_type, priority, status, verification) = checked_enums(&dto)?;
    // An explicitly empty title is a mistake, not an instruction to erase it.
    if let Some(t) = dto.title.as_deref() {
        require_title(Some(t))?;
    }

    let requirement = sqlx::query_as::<_, Requirement>(&format!(
        "UPDATE pm_requirement SET \
             code = COALESCE($3, code), \
             title = COALESCE($4, title), \
             description = COALESCE($5, description), \
             req_type = COALESCE($6, req_type), \
             priority = COALESCE($7, priority), \
             source = COALESCE($8, source), \
             rationale = COALESCE($9, rationale), \
             status = COALESCE($10, status), \
             verification_method = COALESCE($11, verification_method), \
             verification_notes = COALESCE($12, verification_notes), \
             verified_at = CASE WHEN $13::boolean THEN $14::date ELSE verified_at END, \
             position = COALESCE($15, position), \
             updated_at = now() \
         WHERE id = $1 AND project_id = $2 \
         RETURNING {REQ_COLS}"
    ))
    .bind(requirement_id)
    .bind(project_id)
    .bind(dto.code.as_deref().map(str::trim))
    .bind(dto.title.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.description.as_deref())
    .bind(req_type)
    .bind(priority)
    .bind(dto.source.as_deref().map(str::trim))
    .bind(dto.rationale.as_deref())
    .bind(status)
    .bind(verification)
    .bind(dto.verification_notes.as_deref())
    .bind(dto.verified_at.is_some())
    .bind(dto.verified_at.flatten())
    .bind(dto.position)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OfficeError::NotFound("Exigence introuvable".into()))?;

    Ok(Json(json!({ "requirement": with_links(&requirement, None) })))
}

/// DELETE /projects/:id/requirements/:rid — its links go with it (ON DELETE CASCADE).
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, requirement_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let rows = sqlx::query("DELETE FROM pm_requirement WHERE id = $1 AND project_id = $2")
        .bind(requirement_id)
        .bind(project_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(OfficeError::NotFound("Exigence introuvable".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

/// Read back one link with its target names resolved.
async fn link_by_id(state: &AppState, link_id: Uuid) -> Result<RequirementLink> {
    sqlx::query_as::<_, RequirementLink>(&format!("{LINK_SELECT} WHERE l.id = $1"))
        .bind(link_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| OfficeError::NotFound("Lien de traçabilité introuvable".into()))
}

/// POST /projects/:id/requirements/:rid/links — trace a requirement to the
/// deliverable that satisfies it, to the work package that builds it, or to both.
pub async fn add_link(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, requirement_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<LinkDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    require_requirement_in_project(&state, project_id, requirement_id).await?;

    if dto.deliverable_id.is_none() && dto.task_id.is_none() {
        return Err(OfficeError::Validation(
            "Indiquez au moins un livrable ou une tâche à tracer.".into(),
        ));
    }
    // Both ends must live in this project: naming a project you may edit and a
    // target you may not would otherwise reach into someone else's data.
    if let Some(did) = dto.deliverable_id {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pm_deliverable WHERE id = $1 AND project_id = $2)",
        )
        .bind(did)
        .bind(project_id)
        .fetch_one(&state.db)
        .await?;
        if !ok {
            return Err(OfficeError::NotFound("Livrable introuvable dans ce projet".into()));
        }
    }
    if let Some(tid) = dto.task_id {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound("Tâche introuvable dans ce projet".into()));
        }
    }

    // Tracing the same pair twice says nothing new. The partial unique indexes
    // enforce it; catching the conflict here turns a raw constraint error into
    // the link that already exists.
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO pm_requirement_link (requirement_id, deliverable_id, task_id) \
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING RETURNING id",
    )
    .bind(requirement_id)
    .bind(dto.deliverable_id)
    .bind(dto.task_id)
    .fetch_optional(&state.db)
    .await?;

    let (link_id, existing) = match inserted {
        Some(id) => (id, false),
        None => {
            let found: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM pm_requirement_link WHERE requirement_id = $1 \
                   AND (($2::uuid IS NOT NULL AND deliverable_id = $2::uuid) \
                     OR ($3::uuid IS NOT NULL AND task_id = $3::uuid)) \
                 ORDER BY created_at LIMIT 1",
            )
            .bind(requirement_id)
            .bind(dto.deliverable_id)
            .bind(dto.task_id)
            .fetch_optional(&state.db)
            .await?;
            match found {
                Some(id) => (id, true),
                None => {
                    return Err(OfficeError::Validation(
                        "Ce lien de traçabilité n'a pas pu être créé : la cible est déjà tracée \
                         pour cette exigence."
                            .into(),
                    ))
                }
            }
        }
    };

    let link = link_by_id(&state, link_id).await?;
    Ok(Json(json!({ "link": link, "existing": existing })))
}

/// DELETE /projects/:id/requirements/:rid/links/:lid
pub async fn delete_link(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, requirement_id, link_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    // The EXISTS clause keeps the deletion inside the project of the URL even
    // though the link table itself carries no project column.
    let rows = sqlx::query(
        "DELETE FROM pm_requirement_link l \
         WHERE l.id = $1 AND l.requirement_id = $2 \
           AND EXISTS(SELECT 1 FROM pm_requirement r \
                      WHERE r.id = l.requirement_id AND r.project_id = $3)",
    )
    .bind(link_id)
    .bind(requirement_id)
    .bind(project_id)
    .execute(&state.db)
    .await?
    .rows_affected();
    if rows == 0 {
        return Err(OfficeError::NotFound("Lien de traçabilité introuvable".into()));
    }
    Ok(Json(json!({ "ok": true })))
}

/// GET /projects/:id/traceability — the matrix, in three queries.
///
/// Beyond listing what is traced, it names what is not: requirements no
/// deliverable and no work package realises, and deliverables no requirement
/// justifies. Those two lists are the reason the matrix exists.
pub async fn traceability(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;

    let requirements = requirements_of(&state, project_id).await?;
    let links = links_of(&state, project_id).await?;
    let deliverables = sqlx::query_as::<_, (Uuid, String, String, String, Option<Uuid>)>(
        "SELECT id, code, name, status, task_id FROM pm_deliverable \
         WHERE project_id = $1 ORDER BY position, created_at",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await?;

    // Which deliverables are already answering a requirement.
    let traced_deliverables: HashSet<Uuid> =
        links.iter().filter_map(|l| l.deliverable_id).collect();
    let by_req = group_links(links);

    let mut traced = 0usize;
    let mut by_priority: HashMap<&str, usize> = HashMap::new();
    let mut verified = 0usize;
    let mut orphan_requirements: Vec<Value> = Vec::new();
    let mut rendered: Vec<Value> = Vec::with_capacity(requirements.len());

    for req in &requirements {
        let own = by_req.get(&req.id);
        let has_link = own.is_some_and(|l| !l.is_empty());
        if has_link {
            traced += 1;
        } else {
            orphan_requirements.push(json!({
                "id": req.id, "code": req.code, "title": req.title,
                "priority": req.priority, "status": req.status,
            }));
        }
        // "Verified" is the sign-off recorded on the requirement itself, whatever
        // its links: something traced is not thereby proven.
        if req.status == "verified" {
            verified += 1;
        }
        *by_priority.entry(req.priority.as_str()).or_insert(0) += 1;
        rendered.push(with_links(req, own));
    }

    let orphan_deliverables: Vec<Value> = deliverables
        .iter()
        .filter(|(id, ..)| !traced_deliverables.contains(id))
        .map(|(id, code, name, status, task_id)| {
            json!({ "id": id, "code": code, "name": name, "status": status, "task_id": task_id })
        })
        .collect();

    let total = requirements.len();
    Ok(Json(json!({
        "requirements": rendered,
        "orphans": {
            "requirements": orphan_requirements,
            "deliverables": orphan_deliverables,
        },
        "summary": {
            "total": total,
            "traced": traced,
            "untraced": total - traced,
            "by_priority": {
                "must":   by_priority.get("must").copied().unwrap_or(0),
                "should": by_priority.get("should").copied().unwrap_or(0),
                "could":  by_priority.get("could").copied().unwrap_or(0),
                "wont":   by_priority.get("wont").copied().unwrap_or(0),
            },
            "verified": verified,
        },
    })))
}
