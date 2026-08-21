//! Producing documents from a project's data.
//!
//! Twenty-odd registers hold everything a set of PMI documents should say, and
//! nothing could be handed to someone without Kubuno. This turns a register into
//! a real editable document dropped in Drive — the charter, the status report,
//! the closure report, the risk register, the lessons register, the traceability
//! matrix, the WBS dictionary — assembled through `DocBuilder`, so each is the
//! same node tree the editor loads and the DOCX/ODT/PDF exporters read.
//!
//! Read-only against the registers: producing a document never changes the
//! project. The document, once made, is an ordinary Kubuno document and lives its
//! own life.
use axum::{extract::{Path, State}, Extension, Json};
use chrono::NaiveDate;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    handlers::project_authz::{require_permission, Level},
    middleware::OfficeUser,
    services::doc_builder::DocBuilder,
    state::AppState,
};

/// The documents a project can produce. Each maps to a builder below.
const KINDS: [&str; 8] = [
    "charter", "status_report", "closure_report", "risk_register",
    "lessons_register", "traceability_matrix", "wbs_dictionary", "management_plan",
];

fn fmt_date(d: Option<NaiveDate>) -> String {
    d.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default()
}
fn fmt_opt_date(d: &Option<NaiveDate>) -> String { fmt_date(*d) }

async fn project_title(state: &AppState, project_id: Uuid) -> Result<String> {
    sqlx::query_scalar::<_, String>("SELECT title FROM projects WHERE id = $1")
        .bind(project_id).fetch_optional(&state.db).await?
        .ok_or_else(|| OfficeError::NotFound("Projet introuvable".into()))
}

async fn name_of(state: &AppState, id: Option<Uuid>) -> Option<String> {
    let id = id?;
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT COALESCE(NULLIF(u.display_name, ''), u.email::text) FROM core.users u WHERE u.id = $1",
    ).bind(id).fetch_optional(&state.db).await.ok().flatten().flatten()
}

/// GET /projects/:id/documents/available — what can be produced, and whether the
/// data behind each is there. A document offered but empty wastes a click.
pub async fn available(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;

    // One helper closure, borrowing the pool rather than moving it, so it can run
    // for every register in turn.
    let count = |sql: &'static str| sqlx::query_scalar::<_, i64>(sql).bind(project_id).fetch_one(&state.db);
    let has_charter: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pm_charter WHERE project_id = $1)")
        .bind(project_id).fetch_one(&state.db).await.unwrap_or(false);
    let risks = count("SELECT COUNT(*) FROM pm_risk WHERE project_id = $1").await.unwrap_or(0);
    let lessons = count("SELECT COUNT(*) FROM pm_lesson WHERE project_id = $1").await.unwrap_or(0);
    let reqs = count("SELECT COUNT(*) FROM pm_requirement WHERE project_id = $1").await.unwrap_or(0);
    let wbs = count("SELECT COUNT(*) FROM pm_wbs_dictionary d JOIN tasks t ON t.id = d.task_id WHERE t.project_id = $1").await.unwrap_or(0);
    let plans = count("SELECT COUNT(*) FROM pm_management_plan WHERE project_id = $1 AND is_active").await.unwrap_or(0);
    let closed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pm_closure WHERE project_id = $1 AND status <> 'open')")
        .bind(project_id).fetch_one(&state.db).await.unwrap_or(false);

    let docs = vec![
        json!({ "kind": "charter", "has_data": has_charter,
                "hint": if has_charter { "" } else { "La charte n'a pas encore été renseignée." } }),
        json!({ "kind": "status_report", "has_data": true, "hint": "" }),
        json!({ "kind": "closure_report", "has_data": closed,
                "hint": if closed { "" } else { "Le projet n'est pas encore en clôture." } }),
        json!({ "kind": "risk_register", "has_data": risks > 0,
                "hint": if risks > 0 { "" } else { "Aucun risque enregistré." } }),
        json!({ "kind": "lessons_register", "has_data": lessons > 0,
                "hint": if lessons > 0 { "" } else { "Aucun enseignement consigné." } }),
        json!({ "kind": "traceability_matrix", "has_data": reqs > 0,
                "hint": if reqs > 0 { "" } else { "Aucune exigence enregistrée." } }),
        json!({ "kind": "wbs_dictionary", "has_data": wbs > 0,
                "hint": if wbs > 0 { "" } else { "Aucun lot n'a de dictionnaire." } }),
        json!({ "kind": "management_plan", "has_data": plans > 0,
                "hint": if plans > 0 { "" } else { "Aucun plan subsidiaire actif." } }),
    ];
    Ok(Json(json!({ "documents": docs })))
}

#[derive(Debug, serde::Deserialize)]
pub struct ProduceDto { pub kind: Option<String> }

/// POST /projects/:id/documents — build the document and drop it in Drive.
pub async fn produce(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<ProduceDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let kind = dto.kind.as_deref().unwrap_or_default();
    if !KINDS.contains(&kind) {
        return Err(OfficeError::Validation(format!(
            "Type de document inconnu : « {kind} ». Valeurs admises : {}.", KINDS.join(", "))));
    }
    let title = project_title(&state, project_id).await?;

    let (doc_title, body) = match kind {
        "charter"             => build_charter(&state, project_id, &title).await?,
        "status_report"       => build_status_report(&state, project_id, &title).await?,
        "closure_report"      => build_closure_report(&state, project_id, &title).await?,
        "risk_register"       => build_risk_register(&state, project_id, &title).await?,
        "lessons_register"    => build_lessons(&state, project_id, &title).await?,
        "traceability_matrix" => build_traceability(&state, project_id, &title).await?,
        "wbs_dictionary"      => build_wbs_dictionary(&state, project_id, &title).await?,
        "management_plan"     => build_management_plan(&state, project_id, &title).await?,
        _ => unreachable!(),
    };

    // The builder returns {version, content:{doc}}; create_document_content_file
    // wraps again, so hand it the bare doc.
    let pm_doc = body.get("content").cloned().unwrap_or(body);
    let (file_id, _) = crate::services::content_files::create_document_content_file(
        &state, user.id, &doc_title, pm_doc.clone(),
    ).await?;

    // A produced document must be a first-class Kubuno document immediately, not a
    // loose file that open-by-file mistakes for an import. Register the row here,
    // mirroring documents::create, so it opens like any other document.
    let words = crate::handlers::documents::pm_word_count(&pm_doc);
    let etag = crate::handlers::documents::fresh_etag();
    let default_format = state.instance().default_format;
    let doc = sqlx::query_as::<_, crate::models::document::Document>(
        "INSERT INTO documents (owner_id, title, word_count, file_id, etag, content_etag, source_format)          VALUES ($1, $2, $3, $4, $5, $6, $7)          RETURNING id, owner_id, title, icon, cover_url, word_count, is_starred, is_trashed,                    trashed_at, parent_id, position, last_editor_id, file_id, draft_file_id, source_format,                    created_at, updated_at",
    )
    .bind(user.id).bind(&doc_title).bind(words).bind(file_id)
    .bind(&etag).bind(&etag).bind(&default_format)
    .fetch_one(&state.db).await?;

    Ok(Json(json!({ "file_id": file_id, "document_id": doc.id, "title": doc_title })))
}

// ── Builders ─────────────────────────────────────────────────────────────────

async fn build_charter(state: &AppState, project_id: Uuid, title: &str) -> Result<(String, Value)> {
    let row = sqlx::query_as::<_, (String, String, String, String, String, String, String, String, String, String, String, String, String, Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT purpose, business_case, objectives, success_criteria, high_level_requirements, \
                assumptions, constraints, risks_summary, budget_summary, sponsor, pm_name, \
                pm_authority, status, approved_by, approved_at FROM pm_charter WHERE project_id = $1",
    ).bind(project_id).fetch_optional(&state.db).await?
     .ok_or_else(|| OfficeError::Validation("La charte n'a pas encore été renseignée.".into()))?;

    let mut b = DocBuilder::new();
    b.title(&format!("Charte du projet — {title}"));
    if row.12 == "approved" {
        let by = name_of(state, row.13).await.unwrap_or_default();
        let on = row.14.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_default();
        b.para(&format!("Approuvée par {by} le {on}."));
    } else {
        b.note("Brouillon — non approuvée.");
    }
    b.heading(2, "Identité").field("Sponsor", &row.9).field("Chef de projet", &row.10);
    b.section("Objet du projet", &row.0);
    b.section("Justification", &row.1);
    b.section("Objectifs", &row.2);
    b.section("Critères de succès", &row.3);
    b.section("Exigences de haut niveau", &row.4);
    b.section("Hypothèses", &row.5);
    b.section("Contraintes", &row.6);
    b.section("Risques majeurs", &row.7);
    b.section("Budget", &row.8);
    b.section("Niveau d'autorité du chef de projet", &row.11);
    Ok((format!("Charte — {title}"), b.finish()))
}

async fn build_status_report(state: &AppState, project_id: Uuid, title: &str) -> Result<(String, Value)> {
    // A snapshot pulled from the live registers: progress, what is late, what is
    // open, what is waiting. The report a sponsor is handed once a month.
    let (total, done): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE status = 'completed') FROM tasks \
         WHERE project_id = $1 AND NOT EXISTS (SELECT 1 FROM tasks c WHERE c.parent_id = tasks.id)",
    ).bind(project_id).fetch_one(&state.db).await?;
    let progress = if total > 0 { done * 100 / total } else { 0 };
    let late: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks WHERE project_id = $1 AND status NOT IN ('completed','cancelled') \
         AND end_date < CURRENT_DATE AND NOT EXISTS (SELECT 1 FROM tasks c WHERE c.parent_id = tasks.id)",
    ).bind(project_id).fetch_one(&state.db).await.unwrap_or(0);
    let risks_high: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pm_risk WHERE project_id = $1 AND status NOT IN ('closed','occurred') AND score >= 15",
    ).bind(project_id).fetch_one(&state.db).await.unwrap_or(0);
    let issues_open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pm_issue WHERE project_id = $1 AND status NOT IN ('resolved','closed')",
    ).bind(project_id).fetch_one(&state.db).await.unwrap_or(0);
    let changes_waiting: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pm_change_request WHERE project_id = $1 AND status IN ('submitted','assessing')",
    ).bind(project_id).fetch_one(&state.db).await.unwrap_or(0);

    let today = chrono::Utc::now().date_naive().format("%d/%m/%Y").to_string();
    let mut b = DocBuilder::new();
    b.title(&format!("Rapport d'avancement — {title}"));
    b.field("Date du rapport", &today);
    b.heading(2, "Avancement");
    b.field("Tâches terminées", &format!("{done} sur {total} ({progress} %)"));
    b.heading(2, "Points d'attention");
    b.table(&["Indicateur", "Valeur"], &[
        vec!["Tâches en retard".into(), late.to_string()],
        vec!["Risques critiques ouverts".into(), risks_high.to_string()],
        vec!["Incidents ouverts".into(), issues_open.to_string()],
        vec!["Changements en attente de décision".into(), changes_waiting.to_string()],
    ]);
    // The open risks themselves, so the report names them rather than counting.
    let risks = sqlx::query_as::<_, (String, String, i32)>(
        "SELECT code, title, score FROM pm_risk WHERE project_id = $1 \
         AND status NOT IN ('closed','occurred') AND score >= 15 ORDER BY score DESC",
    ).bind(project_id).fetch_all(&state.db).await.unwrap_or_default();
    if !risks.is_empty() {
        b.heading(2, "Risques critiques");
        b.table(&["Code", "Risque", "Score"],
            &risks.iter().map(|(c, t, s)| vec![c.clone(), t.clone(), s.to_string()]).collect::<Vec<_>>());
    }
    Ok((format!("Avancement — {title} — {today}"), b.finish()))
}

async fn build_closure_report(state: &AppState, project_id: Uuid, title: &str) -> Result<(String, Value)> {
    let row = sqlx::query_as::<_, (String, String, String, String, String, String, Option<NaiveDate>, Option<Uuid>)>(
        "SELECT status, objectives_met, acceptance_note, handover_note, loose_ends, \
                override_reason, closed_on, closed_by FROM pm_closure WHERE project_id = $1",
    ).bind(project_id).fetch_optional(&state.db).await?
     .ok_or_else(|| OfficeError::Validation("Le projet n'a pas de dossier de clôture.".into()))?;

    let mut b = DocBuilder::new();
    b.title(&format!("Rapport de clôture — {title}"));
    if row.0 == "closed" {
        let by = name_of(state, row.7).await.unwrap_or_default();
        b.field("Clos le", &fmt_opt_date(&row.6));
        b.field("Clos par", &by);
    } else {
        b.note("Projet en cours de clôture — non encore clos.");
    }
    b.section("Objectifs atteints", &row.1);
    b.section("Réception", &row.2);
    b.section("Transfert et exploitation", &row.3);
    b.section("Points en suspens", &row.4);
    if !row.5.trim().is_empty() {
        b.heading(2, "Clôturé malgré des points ouverts");
        b.para(&row.5);
    }
    Ok((format!("Clôture — {title}"), b.finish()))
}

async fn build_risk_register(state: &AppState, project_id: Uuid, title: &str) -> Result<(String, Value)> {
    let risks = sqlx::query_as::<_, (String, String, String, String, i32, i32, i32, String, String)>(
        "SELECT code, title, kind, category, probability, impact, score, status, response_strategy \
         FROM pm_risk WHERE project_id = $1 ORDER BY score DESC, code",
    ).bind(project_id).fetch_all(&state.db).await?;
    if risks.is_empty() {
        return Err(OfficeError::Validation("Aucun risque enregistré.".into()));
    }
    let mut b = DocBuilder::new();
    b.title(&format!("Registre des risques — {title}"));
    b.table(&["Code", "Risque", "Nature", "P×I", "Score", "Statut", "Réponse"],
        &risks.iter().map(|(c, t, k, _cat, p, i, s, st, r)| vec![
            c.clone(), t.clone(),
            if k == "opportunity" { "Opportunité".into() } else { "Menace".into() },
            format!("{p}×{i}"), s.to_string(), st.clone(), r.clone(),
        ]).collect::<Vec<_>>());
    Ok((format!("Risques — {title}"), b.finish()))
}

async fn build_lessons(state: &AppState, project_id: Uuid, title: &str) -> Result<(String, Value)> {
    let lessons = sqlx::query_as::<_, (String, String, String, String, String, String, String)>(
        "SELECT code, title, category, outcome, situation, what_happened, recommendation \
         FROM pm_lesson WHERE project_id = $1 ORDER BY position, code",
    ).bind(project_id).fetch_all(&state.db).await?;
    if lessons.is_empty() {
        return Err(OfficeError::Validation("Aucun enseignement consigné.".into()));
    }
    let mut b = DocBuilder::new();
    b.title(&format!("Enseignements tirés — {title}"));
    for (code, t, _cat, outcome, situation, happened, reco) in &lessons {
        let mark = match outcome.as_str() { "positive" => "✓", "negative" => "✗", _ => "≈" };
        b.heading(2, &format!("{code} · {mark} {t}"));
        b.section("Situation", situation);
        b.section("Ce qui s'est passé", happened);
        b.section("Recommandation", reco);
    }
    Ok((format!("Enseignements — {title}"), b.finish()))
}

async fn build_traceability(state: &AppState, project_id: Uuid, title: &str) -> Result<(String, Value)> {
    let reqs = sqlx::query_as::<_, (Uuid, String, String, String, String)>(
        "SELECT id, code, title, priority, status FROM pm_requirement WHERE project_id = $1 \
         ORDER BY position, code",
    ).bind(project_id).fetch_all(&state.db).await?;
    if reqs.is_empty() {
        return Err(OfficeError::Validation("Aucune exigence enregistrée.".into()));
    }
    let links = sqlx::query_as::<_, (Uuid, Option<String>, Option<String>)>(
        "SELECT l.requirement_id, d.name, t.name FROM pm_requirement_link l \
         LEFT JOIN pm_deliverable d ON d.id = l.deliverable_id \
         LEFT JOIN tasks t ON t.id = l.task_id \
         JOIN pm_requirement r ON r.id = l.requirement_id WHERE r.project_id = $1",
    ).bind(project_id).fetch_all(&state.db).await.unwrap_or_default();

    let mut by_req: std::collections::HashMap<Uuid, Vec<String>> = std::collections::HashMap::new();
    for (rid, d, t) in &links {
        let target = d.clone().or_else(|| t.clone()).unwrap_or_default();
        if !target.is_empty() { by_req.entry(*rid).or_default().push(target); }
    }
    let mut b = DocBuilder::new();
    b.title(&format!("Matrice de traçabilité — {title}"));
    b.table(&["Code", "Exigence", "Priorité", "Statut", "Réalisé par"],
        &reqs.iter().map(|(id, c, t, p, st)| {
            let realised = by_req.get(id).map(|v| v.join(", "))
                .unwrap_or_else(|| "— rien ne la réalise —".into());
            vec![c.clone(), t.clone(), p.clone(), st.clone(), realised]
        }).collect::<Vec<_>>());
    Ok((format!("Traçabilité — {title}"), b.finish()))
}

async fn build_wbs_dictionary(state: &AppState, project_id: Uuid, title: &str) -> Result<(String, Value)> {
    let entries = sqlx::query_as::<_, (String, String, String, String, String, String)>(
        "SELECT t.wbs, t.name, d.code_of_account, d.statement_of_work, d.acceptance_criteria, d.exclusions \
         FROM pm_wbs_dictionary d JOIN tasks t ON t.id = d.task_id \
         WHERE t.project_id = $1 ORDER BY t.wbs",
    ).bind(project_id).fetch_all(&state.db).await?;
    if entries.is_empty() {
        return Err(OfficeError::Validation("Aucun lot de travail n'a de dictionnaire.".into()));
    }
    let mut b = DocBuilder::new();
    b.title(&format!("Dictionnaire du WBS — {title}"));
    for (wbs, name, cc, sow, accept, excl) in &entries {
        b.heading(2, &format!("{wbs} — {name}"));
        b.field("Code comptable", cc);
        b.section("Énoncé des travaux", sow);
        b.section("Critères d'acceptation", accept);
        b.section("Hors périmètre", excl);
    }
    Ok((format!("Dictionnaire WBS — {title}"), b.finish()))
}

async fn build_management_plan(state: &AppState, project_id: Uuid, title: &str) -> Result<(String, Value)> {
    let plans = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT area, approach, roles, procedures, tools FROM pm_management_plan \
         WHERE project_id = $1 AND is_active ORDER BY area",
    ).bind(project_id).fetch_all(&state.db).await?;
    if plans.is_empty() {
        return Err(OfficeError::Validation("Aucun plan subsidiaire actif.".into()));
    }
    let label = |area: &str| -> &'static str {
        match area {
            "scope" => "Périmètre", "requirements" => "Exigences", "schedule" => "Échéancier",
            "cost" => "Coûts", "quality" => "Qualité", "resource" => "Ressources",
            "communications" => "Communications", "risk" => "Risques",
            "procurement" => "Approvisionnements", "stakeholder" => "Parties prenantes",
            "change" => "Changements", "configuration" => "Configuration", _ => "Domaine",
        }
    };
    let mut b = DocBuilder::new();
    b.title(&format!("Plan de management du projet — {title}"));
    b.note("Document intégrateur : chaque section décrit comment un domaine est piloté.");
    for (area, approach, roles, procedures, tools) in &plans {
        b.heading(2, label(area));
        b.section("Approche", approach);
        b.section("Rôles", roles);
        b.section("Procédures", procedures);
        b.section("Outils", tools);
    }
    Ok((format!("Plan de management — {title}"), b.finish()))
}
