//! Procurement: what the project buys rather than builds, and who carries the risk.
//!
//! The register's one indispensable field is the **contract type**, because it
//! decides who pays when the estimate turns out to be wrong. A fixed price puts
//! that on the supplier; a cost-reimbursable puts it squarely back on the project;
//! time and material leaves it open unless a cap says otherwise. A register that
//! lists amounts without saying which is which cannot price the exposure it
//! describes — so the totals here are split that way rather than summed into one
//! misleading figure.
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

const TYPES: [&str; 6] = ["fixed_price", "fixed_incentive", "cost_plus_fee",
                          "cost_plus_incentive", "time_material", "other"];
const STATUSES: [&str; 6] = ["planned", "tendering", "awarded", "active", "closed", "cancelled"];
const PAYMENT_STATUSES: [&str; 5] = ["planned", "invoiced", "paid", "disputed", "cancelled"];
/// Contracts under which the supplier absorbs an overrun.
const SUPPLIER_RISK: [&str; 2] = ["fixed_price", "fixed_incentive"];
/// Contracts under which the project absorbs it.
const BUYER_RISK: [&str; 2] = ["cost_plus_fee", "cost_plus_incentive"];

const COLS: &str = "p.id, p.project_id, p.code, p.title, p.statement_of_work, p.make_or_buy_note, \
     p.contract_type, p.supplier_name, p.supplier_contact, p.stakeholder_id, p.value, \
     p.not_to_exceed, p.status, p.awarded_on, p.starts_on, p.ends_on, p.deliverable_id, \
     p.task_id, p.risk_id, p.terms, p.performance_note, p.closed_on, p.closure_note, \
     p.position, p.created_at, p.updated_at, \
     s.name AS stakeholder_name, d.name AS deliverable_name, t.name AS task_name, \
     r.code AS risk_code";

const FROM: &str = "FROM pm_procurement p \
     LEFT JOIN pm_stakeholder s ON s.id = p.stakeholder_id \
     LEFT JOIN pm_deliverable d ON d.id = p.deliverable_id \
     LEFT JOIN tasks t ON t.id = p.task_id AND t.project_id = p.project_id \
     LEFT JOIN pm_risk r ON r.id = p.risk_id";

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Procurement {
    id:                Uuid,
    project_id:        Uuid,
    code:              String,
    title:             String,
    statement_of_work: String,
    make_or_buy_note:  String,
    contract_type:     String,
    supplier_name:     String,
    supplier_contact:  String,
    stakeholder_id:    Option<Uuid>,
    stakeholder_name:  Option<String>,
    value:             Option<f64>,
    /// The cap on a time-and-material contract; without it the exposure is open.
    not_to_exceed:     Option<f64>,
    status:            String,
    awarded_on:        Option<NaiveDate>,
    starts_on:         Option<NaiveDate>,
    ends_on:           Option<NaiveDate>,
    deliverable_id:    Option<Uuid>,
    deliverable_name:  Option<String>,
    task_id:           Option<Uuid>,
    task_name:         Option<String>,
    risk_id:           Option<Uuid>,
    risk_code:         Option<String>,
    terms:             String,
    performance_note:  String,
    closed_on:         Option<NaiveDate>,
    closure_note:      String,
    position:          i32,
    created_at:        chrono::DateTime<chrono::Utc>,
    updated_at:        chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Payment {
    id:             Uuid,
    procurement_id: Uuid,
    label:          String,
    due_on:         Option<NaiveDate>,
    amount:         f64,
    status:         String,
    invoice_ref:    String,
    paid_on:        Option<NaiveDate>,
    cost_entry_id:  Option<Uuid>,
    position:       i32,
}

fn double_option<'de, T, D>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where T: Deserialize<'de>, D: Deserializer<'de> {
    Deserialize::deserialize(de).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct ProcurementDto {
    pub code:              Option<String>,
    pub title:             Option<String>,
    pub statement_of_work: Option<String>,
    pub make_or_buy_note:  Option<String>,
    pub contract_type:     Option<String>,
    pub supplier_name:     Option<String>,
    pub supplier_contact:  Option<String>,
    pub status:            Option<String>,
    pub terms:             Option<String>,
    pub performance_note:  Option<String>,
    pub closure_note:      Option<String>,
    pub position:          Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub value:          Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub not_to_exceed:  Option<Option<f64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub stakeholder_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub deliverable_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub task_id:        Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub risk_id:        Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub awarded_on:     Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub starts_on:      Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub ends_on:        Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub closed_on:      Option<Option<NaiveDate>>,
}

#[derive(Debug, Deserialize)]
pub struct PaymentDto {
    pub label:       Option<String>,
    pub amount:      Option<f64>,
    pub status:      Option<String>,
    pub invoice_ref: Option<String>,
    pub position:    Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_on:  Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub paid_on: Option<Option<NaiveDate>>,
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

fn round2(v: f64) -> f64 { let r = (v * 100.0).round() / 100.0; if r == 0.0 { 0.0 } else { r } }

async fn fetch(state: &AppState, project_id: Uuid, id: Uuid) -> Result<Procurement> {
    sqlx::query_as::<_, Procurement>(
        &format!("SELECT {COLS} {FROM} WHERE p.id = $1 AND p.project_id = $2"),
    ).bind(id).bind(project_id).fetch_optional(&state.db).await?
     .ok_or_else(|| OfficeError::NotFound("Contrat introuvable".into()))
}

/// GET /projects/:id/procurement
pub async fn list(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::View).await?;
    let contracts = sqlx::query_as::<_, Procurement>(&format!(
        "SELECT {COLS} {FROM} WHERE p.project_id = $1 ORDER BY p.position, p.created_at"
    )).bind(project_id).fetch_all(&state.db).await?;

    let payments = sqlx::query_as::<_, Payment>(
        "SELECT y.id, y.procurement_id, y.label, y.due_on, y.amount, y.status, \
                y.invoice_ref, y.paid_on, y.cost_entry_id, y.position \
         FROM pm_procurement_payment y JOIN pm_procurement p ON p.id = y.procurement_id \
         WHERE p.project_id = $1 ORDER BY y.position, y.due_on NULLS LAST",
    ).bind(project_id).fetch_all(&state.db).await?;

    let today = Utc::now().date_naive();
    let mut by_contract: std::collections::HashMap<Uuid, Vec<&Payment>> = std::collections::HashMap::new();
    for p in &payments { by_contract.entry(p.procurement_id).or_default().push(p) }

    // Committed value, split by who absorbs an overrun. Summing the three into one
    // number would describe an exposure the project does not actually carry.
    // A contract typed "other" says nothing about who absorbs an overrun. It is
    // counted apart rather than folded into one of the three, which would state a
    // risk allocation the contract never declared.
    let (mut supplier, mut buyer, mut shared, mut unknown) = (0.0, 0.0, 0.0, 0.0);
    let mut uncapped: Vec<Value> = Vec::new();
    let mut open = 0i64;
    let mut unpriced = 0i64;

    let rows: Vec<Value> = contracts.iter().map(|c| {
        let live = c.status == "awarded" || c.status == "active";
        if live { open += 1 }
        // Only live contracts represent exposure; a plan or a cancelled tender
        // commits nothing.
        if live {
            match c.value {
                Some(v) if SUPPLIER_RISK.contains(&c.contract_type.as_str()) => supplier += v,
                Some(v) if BUYER_RISK.contains(&c.contract_type.as_str()) => buyer += v,
                Some(v) if c.contract_type == "time_material" => shared += v,
                Some(v) => unknown += v,
                None => unpriced += 1,
            }
            // Time and material with no ceiling: the exposure has no upper bound,
            // and that is exactly what a not-to-exceed clause exists to state.
            if c.contract_type == "time_material" && c.not_to_exceed.is_none() {
                uncapped.push(json!({ "id": c.id, "code": c.code, "title": c.title,
                                      "supplier": c.supplier_name }));
            }
        }

        let mine = by_contract.get(&c.id).map(|v| v.as_slice()).unwrap_or(&[]);
        let paid: f64 = mine.iter().filter(|p| p.status == "paid").map(|p| p.amount).sum();
        let invoiced: f64 = mine.iter().filter(|p| p.status == "invoiced").map(|p| p.amount).sum();
        let overdue: Vec<&&Payment> = mine.iter()
            .filter(|p| p.status != "paid" && p.status != "cancelled"
                     && p.due_on.is_some_and(|d| d < today))
            .collect();

        json!({
            "contract": c,
            "payments": mine,
            "paid": round2(paid),
            "invoiced": round2(invoiced),
            // What is left to pay against what was committed. Negative would mean
            // more has been paid than the contract allows, which is worth seeing.
            "remaining": c.value.map(|v| round2(v - paid)),
            "overdue_payments": overdue.len(),
            "risk_side": if SUPPLIER_RISK.contains(&c.contract_type.as_str()) { "supplier" }
                         else if BUYER_RISK.contains(&c.contract_type.as_str()) { "buyer" }
                         else if c.contract_type == "time_material" { "shared" }
                         else { "unknown" },
        })
    }).collect();

    let paid_total: f64 = payments.iter().filter(|p| p.status == "paid").map(|p| p.amount).sum();
    let overdue_total = payments.iter()
        .filter(|p| p.status != "paid" && p.status != "cancelled" && p.due_on.is_some_and(|d| d < today))
        .count();

    Ok(Json(json!({
        "contracts": rows,
        "summary": {
            "total": contracts.len(),
            "open": open,
            "committed": {
                // Who absorbs an overrun on each euro committed.
                "supplier_risk": round2(supplier),
                "buyer_risk": round2(buyer),
                "shared_risk": round2(shared),
                "unknown_risk": round2(unknown),
                "total": round2(supplier + buyer + shared + unknown),
            },
            "paid": round2(paid_total),
            "overdue_payments": overdue_total,
            // Live contracts with no value recorded: they commit something nobody
            // can add up.
            "unpriced": unpriced,
        },
        // Time and material without a ceiling. Reported separately because it is
        // not an amount — it is the absence of one.
        "uncapped": uncapped,
    })))
}

/// POST /projects/:id/procurement
pub async fn create(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(project_id): Path<Uuid>,
    Json(dto): Json<ProcurementDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let title = dto.title.as_deref().map(str::trim).filter(|s| !s.is_empty())
        .ok_or_else(|| OfficeError::Validation("Nommez le contrat.".into()))?.to_string();
    let contract_type = match dto.contract_type.as_deref() {
        Some(t) => check(t, &TYPES, "Type de contrat")?, None => "fixed_price".into() };
    let status = match dto.status.as_deref() { Some(s) => check(s, &STATUSES, "Statut")?, None => "planned".into() };
    if let Some(tid) = dto.task_id.flatten() {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }

    let code = match dto.code.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => c.to_string(),
        None => {
            let highest: Option<i32> = sqlx::query_scalar(
                "SELECT MAX(CAST(substring(code FROM '^CT-([0-9]+)$') AS INT)) \
                 FROM pm_procurement WHERE project_id = $1 AND code ~ '^CT-[0-9]+$'",
            ).bind(project_id).fetch_one(&state.db).await?;
            format!("CT-{:02}", highest.unwrap_or(0) + 1)
        }
    };
    let position = match dto.position {
        Some(p) => p,
        None => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(position) FROM pm_procurement WHERE project_id = $1",
        ).bind(project_id).fetch_one(&state.db).await?.map_or(0, |m| m + 1),
    };

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO pm_procurement (project_id, code, title, statement_of_work, \
             make_or_buy_note, contract_type, supplier_name, supplier_contact, stakeholder_id, \
             value, not_to_exceed, status, awarded_on, starts_on, ends_on, deliverable_id, \
             task_id, risk_id, terms, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, \
                 $17, $18, $19, $20) RETURNING id",
    )
    .bind(project_id).bind(&code).bind(&title)
    .bind(dto.statement_of_work.as_deref().unwrap_or_default())
    .bind(dto.make_or_buy_note.as_deref().unwrap_or_default())
    .bind(&contract_type)
    .bind(dto.supplier_name.as_deref().map(str::trim).unwrap_or_default())
    .bind(dto.supplier_contact.as_deref().map(str::trim).unwrap_or_default())
    .bind(dto.stakeholder_id.flatten())
    .bind(dto.value.flatten()).bind(dto.not_to_exceed.flatten())
    .bind(&status).bind(dto.awarded_on.flatten())
    .bind(dto.starts_on.flatten()).bind(dto.ends_on.flatten())
    .bind(dto.deliverable_id.flatten()).bind(dto.task_id.flatten()).bind(dto.risk_id.flatten())
    .bind(dto.terms.as_deref().unwrap_or_default()).bind(position)
    .fetch_one(&state.db).await?;
    Ok(Json(json!({ "contract": fetch(&state, project_id, id).await? })))
}

/// PATCH /projects/:id/procurement/:pid
pub async fn update(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, contract_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<ProcurementDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let contract_type = match dto.contract_type.as_deref() { Some(t) => Some(check(t, &TYPES, "Type de contrat")?), None => None };
    let status = match dto.status.as_deref() { Some(s) => Some(check(s, &STATUSES, "Statut")?), None => None };
    if let Some(Some(tid)) = dto.task_id {
        if !task_in_project(&state, project_id, tid).await? {
            return Err(OfficeError::NotFound(format!("Tâche {tid}")));
        }
    }
    // Awarding a contract without saying what it commits leaves a figure nobody
    // can add up; the register would report an exposure it cannot total.
    if status.as_deref().is_some_and(|s| s == "awarded" || s == "active") {
        let current = fetch(&state, project_id, contract_id).await?;
        let value = match dto.value { Some(v) => v, None => current.value };
        if value.is_none() {
            return Err(OfficeError::Validation(format!(
                "{} : indiquez le montant engagé avant de l'attribuer. Un contrat actif sans \
                 montant engage quelque chose que personne ne peut totaliser.", current.code)));
        }
    }

    let rows = sqlx::query(
        "UPDATE pm_procurement SET \
            code = COALESCE($3, code), title = COALESCE($4, title), \
            statement_of_work = COALESCE($5, statement_of_work), \
            make_or_buy_note = COALESCE($6, make_or_buy_note), \
            contract_type = COALESCE($7, contract_type), \
            supplier_name = COALESCE($8, supplier_name), \
            supplier_contact = COALESCE($9, supplier_contact), \
            status = COALESCE($10, status), terms = COALESCE($11, terms), \
            performance_note = COALESCE($12, performance_note), \
            closure_note = COALESCE($13, closure_note), \
            value = CASE WHEN $14::boolean THEN $15::double precision ELSE value END, \
            not_to_exceed = CASE WHEN $16::boolean THEN $17::double precision ELSE not_to_exceed END, \
            stakeholder_id = CASE WHEN $18::boolean THEN $19::uuid ELSE stakeholder_id END, \
            deliverable_id = CASE WHEN $20::boolean THEN $21::uuid ELSE deliverable_id END, \
            task_id = CASE WHEN $22::boolean THEN $23::uuid ELSE task_id END, \
            risk_id = CASE WHEN $24::boolean THEN $25::uuid ELSE risk_id END, \
            awarded_on = CASE WHEN $26::boolean THEN $27::date ELSE awarded_on END, \
            starts_on = CASE WHEN $28::boolean THEN $29::date ELSE starts_on END, \
            ends_on = CASE WHEN $30::boolean THEN $31::date ELSE ends_on END, \
            closed_on = CASE WHEN $32::boolean THEN $33::date \
                             WHEN $10 = 'closed' THEN COALESCE(closed_on, CURRENT_DATE) \
                             ELSE closed_on END, \
            position = COALESCE($34, position), updated_at = now() \
         WHERE id = $1 AND project_id = $2",
    )
    .bind(contract_id).bind(project_id)
    .bind(dto.code.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.title.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(dto.statement_of_work.as_deref()).bind(dto.make_or_buy_note.as_deref())
    .bind(contract_type.as_deref())
    .bind(dto.supplier_name.as_deref().map(str::trim))
    .bind(dto.supplier_contact.as_deref().map(str::trim))
    .bind(status.as_deref()).bind(dto.terms.as_deref())
    .bind(dto.performance_note.as_deref()).bind(dto.closure_note.as_deref())
    .bind(dto.value.is_some()).bind(dto.value.flatten())
    .bind(dto.not_to_exceed.is_some()).bind(dto.not_to_exceed.flatten())
    .bind(dto.stakeholder_id.is_some()).bind(dto.stakeholder_id.flatten())
    .bind(dto.deliverable_id.is_some()).bind(dto.deliverable_id.flatten())
    .bind(dto.task_id.is_some()).bind(dto.task_id.flatten())
    .bind(dto.risk_id.is_some()).bind(dto.risk_id.flatten())
    .bind(dto.awarded_on.is_some()).bind(dto.awarded_on.flatten())
    .bind(dto.starts_on.is_some()).bind(dto.starts_on.flatten())
    .bind(dto.ends_on.is_some()).bind(dto.ends_on.flatten())
    .bind(dto.closed_on.is_some()).bind(dto.closed_on.flatten())
    .bind(dto.position)
    .execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Contrat introuvable".into())); }
    Ok(Json(json!({ "contract": fetch(&state, project_id, contract_id).await? })))
}

/// DELETE /projects/:id/procurement/:pid
pub async fn delete(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, contract_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    let current = fetch(&state, project_id, contract_id).await?;
    if current.status == "awarded" || current.status == "active" || current.status == "closed" {
        return Err(OfficeError::Validation(format!(
            "{} a été attribué : le registre garde la trace de ce qui a été engagé. \
             Utilisez « annulé » si le contrat n'a pas été suivi d'effet.", current.code)));
    }
    sqlx::query("DELETE FROM pm_procurement WHERE id = $1 AND project_id = $2")
        .bind(contract_id).bind(project_id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn contract_in_project(state: &AppState, project_id: Uuid, contract_id: Uuid) -> Result<()> {
    let ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pm_procurement WHERE id = $1 AND project_id = $2)",
    ).bind(contract_id).bind(project_id).fetch_one(&state.db).await?;
    if !ok { return Err(OfficeError::NotFound("Contrat introuvable".into())); }
    Ok(())
}

/// POST /projects/:id/procurement/:pid/payments
pub async fn add_payment(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, contract_id)): Path<(Uuid, Uuid)>,
    Json(dto): Json<PaymentDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    contract_in_project(&state, project_id, contract_id).await?;
    let amount = dto.amount.ok_or_else(|| OfficeError::Validation("Indiquez le montant.".into()))?;
    let status = match dto.status.as_deref() { Some(s) => check(s, &PAYMENT_STATUSES, "Statut")?, None => "planned".into() };
    let position = match dto.position {
        Some(p) => p,
        None => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(position) FROM pm_procurement_payment WHERE procurement_id = $1",
        ).bind(contract_id).fetch_one(&state.db).await?.map_or(0, |m| m + 1),
    };
    let payment = sqlx::query_as::<_, Payment>(
        "INSERT INTO pm_procurement_payment (procurement_id, label, due_on, amount, status, \
             invoice_ref, paid_on, position) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING id, procurement_id, label, due_on, amount, status, invoice_ref, paid_on, \
                   cost_entry_id, position",
    )
    .bind(contract_id).bind(dto.label.as_deref().map(str::trim).unwrap_or_default())
    .bind(dto.due_on.flatten()).bind(amount).bind(&status)
    .bind(dto.invoice_ref.as_deref().map(str::trim).unwrap_or_default())
    .bind(dto.paid_on.flatten()).bind(position)
    .fetch_one(&state.db).await?;
    Ok(Json(json!({ "payment": payment })))
}

/// PATCH /projects/:id/procurement/:pid/payments/:yid
pub async fn update_payment(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, contract_id, payment_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(dto): Json<PaymentDto>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    contract_in_project(&state, project_id, contract_id).await?;
    let status = match dto.status.as_deref() { Some(s) => Some(check(s, &PAYMENT_STATUSES, "Statut")?), None => None };
    let payment = sqlx::query_as::<_, Payment>(
        "UPDATE pm_procurement_payment SET \
            label = COALESCE($3, label), amount = COALESCE($4, amount), \
            status = COALESCE($5, status), invoice_ref = COALESCE($6, invoice_ref), \
            due_on = CASE WHEN $7::boolean THEN $8::date ELSE due_on END, \
            paid_on = CASE WHEN $9::boolean THEN $10::date \
                           WHEN $5 = 'paid' THEN COALESCE(paid_on, CURRENT_DATE) \
                           WHEN $5 IS NOT NULL AND $5 <> 'paid' THEN NULL ELSE paid_on END, \
            position = COALESCE($11, position) \
         WHERE id = $1 AND procurement_id = $2 \
         RETURNING id, procurement_id, label, due_on, amount, status, invoice_ref, paid_on, \
                   cost_entry_id, position",
    )
    .bind(payment_id).bind(contract_id)
    .bind(dto.label.as_deref().map(str::trim)).bind(dto.amount)
    .bind(status.as_deref()).bind(dto.invoice_ref.as_deref().map(str::trim))
    .bind(dto.due_on.is_some()).bind(dto.due_on.flatten())
    .bind(dto.paid_on.is_some()).bind(dto.paid_on.flatten())
    .bind(dto.position)
    .fetch_optional(&state.db).await?
    .ok_or_else(|| OfficeError::NotFound("Échéance introuvable".into()))?;
    Ok(Json(json!({ "payment": payment })))
}

/// DELETE /projects/:id/procurement/:pid/payments/:yid
pub async fn delete_payment(
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((project_id, contract_id, payment_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<Value>> {
    require_permission(&state, project_id, user.id, Level::Edit).await?;
    contract_in_project(&state, project_id, contract_id).await?;
    let rows = sqlx::query(
        "DELETE FROM pm_procurement_payment WHERE id = $1 AND procurement_id = $2",
    ).bind(payment_id).bind(contract_id).execute(&state.db).await?.rows_affected();
    if rows == 0 { return Err(OfficeError::NotFound("Échéance introuvable".into())); }
    Ok(Json(json!({ "ok": true })))
}
