//! Trash retention — the background cleaner that empties the module's trash.
//!
//! Until now the office trash was permanent: an item stayed `is_trashed = TRUE`
//! for ever unless a user opened the bin and deleted it by hand. An
//! administrator asking "how long do we keep deleted documents?" had no answer
//! to give and no lever to pull, and the storage bill kept growing quietly.
//!
//! The cleaner sweeps every editor of the suite once an hour, deleting the rows
//! whose `trashed_at` is older than the instance's retention. It reads the
//! retention on each pass, so a change in the console takes effect on the next
//! sweep without a restart, and `0` — the shipped default — means the sweep does
//! nothing at all. Nothing is deleted that a user could not already have deleted
//! from the bin; this only stops them having to.

use std::time::Duration;

use crate::state::AppState;

/// One entity type the trash holds: its table (schema-qualified where the
/// editor has its own schema) and the human name used in the log line.
struct TrashedTable {
    table: &'static str,
    label: &'static str,
}

/// Every editor of the suite. Adding one here is all it takes for its bin to be
/// swept — a new editor that forgets this list keeps an eternal trash, which is
/// exactly the bug this module exists to fix.
const TABLES: &[TrashedTable] = &[
    TrashedTable { table: "documents",              label: "documents" },
    TrashedTable { table: "spreadsheets",           label: "classeurs" },
    TrashedTable { table: "presentations",          label: "présentations" },
    TrashedTable { table: "diagrams",               label: "diagrammes" },
    TrashedTable { table: "projects",               label: "projets" },
    TrashedTable { table: "office_wb.boards",       label: "tableaux blancs" },
    TrashedTable { table: "office_data.reports",    label: "rapports" },
    TrashedTable { table: "office_script.scripts",  label: "scripts" },
    TrashedTable { table: "office_maths.formulas",  label: "formules" },
];

/// Deletes, for good, the trashed rows older than `retention_days`.
///
/// Returns the number of rows deleted. A table that fails is logged and skipped
/// rather than aborting the sweep: one editor's broken schema must not keep
/// every other editor's trash alive for ever.
///
/// `LIMIT` per table bounds the work of a single pass — a first sweep on an
/// instance that has been accumulating for a year should not lock the tables for
/// minutes. Whatever is left over is taken on the next pass, an hour later.
pub async fn purge_trash(state: &AppState, retention_days: i64) -> u64 {
    if retention_days <= 0 {
        return 0;
    }

    const BATCH: i64 = 500;
    let mut deleted_total = 0_u64;

    for entry in TABLES {
        // `make_interval(days => $1)` rather than string interpolation: the
        // retention is a number that came from an HTTP payload, and it enters
        // the statement as a bound parameter or not at all.
        let sql = format!(
            "DELETE FROM {t} WHERE ctid IN (
                 SELECT ctid FROM {t}
                 WHERE is_trashed = TRUE
                   AND trashed_at IS NOT NULL
                   AND trashed_at < NOW() - make_interval(days => $1)
                 LIMIT {BATCH}
             )",
            t = entry.table,
        );

        match sqlx::query(&sql)
            .bind(retention_days as i32)
            .execute(&state.db)
            .await
        {
            Ok(res) => {
                let n = res.rows_affected();
                if n > 0 {
                    tracing::info!(
                        kind = entry.label,
                        count = n,
                        retention_days,
                        "Corbeille office purgée"
                    );
                    deleted_total += n;
                }
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    table = entry.table,
                    "Purge de la corbeille office"
                );
            }
        }
    }

    deleted_total
}

/// The hourly loop. Sleeps FIRST so a module restarted in a loop never turns
/// into a deletion loop, and so the very first read of the instance settings has
/// landed before anything is deleted — purging on the compiled default while the
/// administrator's own retention is still in flight would be indefensible.
pub async fn run_trash_cleaner(state: AppState) {
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
        let retention = state.instance().trash_retention_days;
        purge_trash(&state, retention).await;
    }
}
