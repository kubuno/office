//! The portfolio: every project the user can see, judged by the same measures.
//!
//! Nothing is stored here. Each figure is read from the registers the projects
//! already keep — the plan, the risks, the issues, the changes, the deliverables,
//! the closure checks. The value is not in the numbers themselves, which each
//! project already shows, but in putting them side by side: a project quietly
//! carrying six open critical risks looks exactly like a healthy one until it is
//! placed next to the others.
//!
//! Every query is grouped across the whole set, so the cost of this endpoint does
//! not grow with the number of projects — the mistake that makes portfolio views
//! unusable at the moment they become useful.
use axum::{extract::State, Extension, Json};
use chrono::{NaiveDate, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{errors::Result, middleware::OfficeUser, state::AppState};

/// One number per project, keyed by project.
type ByProject = HashMap<Uuid, i64>;
type MoneyByProject = HashMap<Uuid, f64>;

async fn counts(state: &AppState, sql: &str, ids: &[Uuid]) -> Result<ByProject> {
    Ok(sqlx::query_as::<_, (Uuid, i64)>(sql).bind(ids).fetch_all(&state.db).await?
        .into_iter().collect())
}

async fn money(state: &AppState, sql: &str, ids: &[Uuid]) -> Result<MoneyByProject> {
    Ok(sqlx::query_as::<_, (Uuid, f64)>(sql).bind(ids).fetch_all(&state.db).await?
        .into_iter().collect())
}

fn get(map: &ByProject, id: &Uuid) -> i64 { map.get(id).copied().unwrap_or(0) }
fn get_money(map: &MoneyByProject, id: &Uuid) -> f64 { map.get(id).copied().unwrap_or(0.0) }
fn round2(v: f64) -> f64 { let r = (v * 100.0).round() / 100.0; if r == 0.0 { 0.0 } else { r } }

/// GET /portfolio
pub async fn overview(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
) -> Result<Json<Value>> {
    // Only management projects: a cloud project has no plan to judge.
    let projects = sqlx::query_as::<_, (Uuid, String, String, Option<NaiveDate>, Option<NaiveDate>, Option<Uuid>)>(
        "SELECT p.id, p.title, p.status, p.start_date, p.end_date, p.parent_id \
         FROM projects p \
         WHERE p.is_trashed = FALSE AND COALESCE(p.kind, 'management') = 'management' \
           AND (p.owner_id = $1 OR EXISTS ( \
                 SELECT 1 FROM project_collaborators c \
                 WHERE c.project_id = p.id AND c.user_id = $1)) \
         ORDER BY p.updated_at DESC",
    ).bind(user.id).fetch_all(&state.db).await?;

    if projects.is_empty() {
        return Ok(Json(json!({ "projects": [], "summary": { "total": 0 } })));
    }
    let ids: Vec<Uuid> = projects.iter().map(|p| p.0).collect();
    let today = Utc::now().date_naive();

    // One grouped query per measure, over the whole set.
    let tasks_total = counts(&state,
        "SELECT project_id, COUNT(*) FROM tasks WHERE project_id = ANY($1) \
         AND NOT EXISTS (SELECT 1 FROM tasks c WHERE c.parent_id = tasks.id) \
         GROUP BY project_id", &ids).await?;
    let tasks_done = counts(&state,
        "SELECT project_id, COUNT(*) FROM tasks WHERE project_id = ANY($1) \
         AND status = 'completed' \
         AND NOT EXISTS (SELECT 1 FROM tasks c WHERE c.parent_id = tasks.id) \
         GROUP BY project_id", &ids).await?;
    // Work that should have finished and has not. The plainest measure of late.
    let tasks_late = counts(&state,
        "SELECT project_id, COUNT(*) FROM tasks WHERE project_id = ANY($1) \
         AND status NOT IN ('completed', 'cancelled') AND end_date < CURRENT_DATE \
         AND NOT EXISTS (SELECT 1 FROM tasks c WHERE c.parent_id = tasks.id) \
         GROUP BY project_id", &ids).await?;
    let critical = counts(&state,
        "SELECT project_id, COUNT(*) FROM tasks WHERE project_id = ANY($1) AND is_critical \
         GROUP BY project_id", &ids).await?;

    let risks_high = counts(&state,
        "SELECT project_id, COUNT(*) FROM pm_risk WHERE project_id = ANY($1) \
         AND status NOT IN ('closed', 'occurred') AND score >= 15 GROUP BY project_id", &ids).await?;
    let risks_occurred = counts(&state,
        "SELECT project_id, COUNT(*) FROM pm_risk WHERE project_id = ANY($1) \
         AND status = 'occurred' GROUP BY project_id", &ids).await?;
    let issues_open = counts(&state,
        "SELECT project_id, COUNT(*) FROM pm_issue WHERE project_id = ANY($1) \
         AND status NOT IN ('resolved', 'closed') GROUP BY project_id", &ids).await?;
    let issues_overdue = counts(&state,
        "SELECT project_id, COUNT(*) FROM pm_issue WHERE project_id = ANY($1) \
         AND status NOT IN ('resolved', 'closed') AND due_date < CURRENT_DATE \
         GROUP BY project_id", &ids).await?;
    let changes_waiting = counts(&state,
        "SELECT project_id, COUNT(*) FROM pm_change_request WHERE project_id = ANY($1) \
         AND status IN ('submitted', 'assessing') GROUP BY project_id", &ids).await?;
    let deliv_total = counts(&state,
        "SELECT project_id, COUNT(*) FROM pm_deliverable WHERE project_id = ANY($1) \
         GROUP BY project_id", &ids).await?;
    let deliv_accepted = counts(&state,
        "SELECT project_id, COUNT(*) FROM pm_deliverable WHERE project_id = ANY($1) \
         AND status = 'accepted' GROUP BY project_id", &ids).await?;

    // Money, as far as it can be read without running the full earned-value
    // engine per project: what was budgeted and what was spent directly.
    let budget = money(&state,
        "SELECT project_id, COALESCE(SUM(budget_cost), 0) FROM tasks \
         WHERE project_id = ANY($1) \
         AND NOT EXISTS (SELECT 1 FROM tasks c WHERE c.parent_id = tasks.id) \
         GROUP BY project_id", &ids).await?;
    let spent_direct = money(&state,
        "SELECT project_id, COALESCE(SUM(amount), 0) FROM pm_cost_entry \
         WHERE project_id = ANY($1) GROUP BY project_id", &ids).await?;
    // Committed on live contracts, split by who absorbs an overrun.
    let committed_buyer = money(&state,
        "SELECT project_id, COALESCE(SUM(value), 0) FROM pm_procurement \
         WHERE project_id = ANY($1) AND status IN ('awarded', 'active') \
         AND contract_type IN ('cost_plus_fee', 'cost_plus_incentive', 'time_material') \
         GROUP BY project_id", &ids).await?;

    let closures = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT project_id, status FROM pm_closure WHERE project_id = ANY($1)",
    ).bind(&ids).fetch_all(&state.db).await?
     .into_iter().collect::<HashMap<Uuid, String>>();

    let mut needs_attention = 0i64;
    let rows: Vec<Value> = projects.iter().map(|(id, title, status, start, end, parent)| {
        let (total, done) = (get(&tasks_total, id), get(&tasks_done, id));
        let progress = if total > 0 { (done as f64 / total as f64 * 100.0).round() as i64 } else { 0 };
        let late = get(&tasks_late, id);
        let (high, occurred) = (get(&risks_high, id), get(&risks_occurred, id));
        let (open_issues, overdue) = (get(&issues_open, id), get(&issues_overdue, id));
        let waiting = get(&changes_waiting, id);
        let (dt, da) = (get(&deliv_total, id), get(&deliv_accepted, id));
        let bac = get_money(&budget, id);
        let spent = get_money(&spent_direct, id);
        let exposure = get_money(&committed_buyer, id);

        // What actually asks for someone's attention, named rather than scored.
        // A single number would hide which of them it is.
        let mut flags: Vec<&str> = Vec::new();
        if late > 0 { flags.push("late_work") }
        if overdue > 0 { flags.push("overdue_issues") }
        if high > 0 { flags.push("high_risks") }
        if waiting > 0 { flags.push("changes_waiting") }
        if bac > 0.0 && spent > bac { flags.push("over_budget") }
        if end.is_some_and(|d| d < today) && progress < 100 { flags.push("past_end_date") }
        if !flags.is_empty() { needs_attention += 1 }

        json!({
            "id": id, "title": title, "status": status,
            "start_date": start, "end_date": end, "parent_id": parent,
            "closure_status": closures.get(id).cloned().unwrap_or_else(|| "open".into()),
            "progress": progress,
            "tasks": { "total": total, "done": done, "late": late, "critical": get(&critical, id) },
            "risks": { "high": high, "occurred": occurred },
            "issues": { "open": open_issues, "overdue": overdue },
            "changes": { "awaiting": waiting },
            "deliverables": { "total": dt, "accepted": da },
            "money": {
                "budget": round2(bac),
                "spent_direct": round2(spent),
                // Committed on contracts where the project absorbs an overrun.
                "exposure": round2(exposure),
            },
            "flags": flags,
        })
    }).collect();

    Ok(Json(json!({
        "projects": rows,
        "summary": {
            "total": projects.len(),
            "needs_attention": needs_attention,
            "closed": closures.values().filter(|s| *s == "closed").count(),
            "budget": round2(ids.iter().map(|i| get_money(&budget, i)).sum::<f64>()),
            "spent_direct": round2(ids.iter().map(|i| get_money(&spent_direct, i)).sum::<f64>()),
            "exposure": round2(ids.iter().map(|i| get_money(&committed_buyer, i)).sum::<f64>()),
        },
    })))
}
