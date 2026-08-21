//! Declaring to the core what **office itself** stores, per account.
//!
//! ## The attribution rule, and why office declares almost nothing
//!
//! Whoever physically holds the byte declares it, and only them.
//!
//! Every piece of user content this module edits — document bodies, sheet data,
//! slide elements, diagram pages, whiteboard scenes, script sources, formulas,
//! datasets, reports — lives in **drive**, written through `FilesClient` over the
//! `/ipc/*` routes. Those bytes sit in `drive.files`, drive weighs them, and drive
//! declares them under category `content`. Office declaring them again would be
//! exactly the double counting this channel exists to prevent, so **office never
//! emits `content`** — a rule the unit tests below enforce mechanically rather
//! than by convention.
//!
//! What office does hold is the machinery around that content, in its own
//! PostgreSQL schemas (`office`, `office_wb`, `office_data`, `office_script`,
//! `office_maths`):
//!
//! * `cache` — the Yjs collaboration state (the consolidated editor snapshots and
//!   the incremental update journals), the idempotency replay store, script run
//!   records and compiled script artefacts. All of it is *regenerable*: the
//!   durable truth is the `.kb***` file at drive, and the snapshot/journal is only
//!   how several people edit it at once. Machinery, not something a person
//!   deposited — hence `cache`, which the core does not bill.
//! * `retention` — the revision history kept in the database
//!   (`document_versions`, `spreadsheet_versions`). Deliberately **not**
//!   `versions`: the billing rule is "what the person can free themselves", and
//!   they cannot free this. `src/router/mod.rs` exposes `GET`/`POST` on
//!   `/documents/:id/versions`, a `restore`, and `GET`/`POST` on
//!   `/spreadsheets/:id/versions` — no `DELETE`, and no switch to turn history
//!   off. The only thing that prunes it is the instance's `max_versions`
//!   setting, i.e. office's retention policy, not the user's choice. If office
//!   ever wants this history billed, the answer is to give people a delete
//!   button, not to relabel the category.
//! * `thumbnails` — the previews office stores inline as base64 `data:` URLs.
//!
//! And one purely informative line:
//!
//! * `delegated` — how many objects office caused drive to create. Weight is
//!   deliberately zero: office does not know it, drive does. The core never adds
//!   `delegated` to any total; it exists so that "office looks like it stores
//!   nothing" reads as *delegation*, not as a broken reporter.
//!
//! ## How bytes are measured
//!
//! `pg_column_size()` throughout, never `octet_length()`. These categories declare
//! what the module occupies **in the database**, and PostgreSQL stores these values
//! TOASTed and compressed: `octet_length()` would report the logical value and
//! over-state a base64 `data:` URL or a JSONB blob by its whole compression ratio.
//! `pg_column_size()` is the figure that matches what the instance actually holds.
//!
//! ## State, never deltas
//!
//! Each declaration carries office's **current** figures for the accounts and
//! categories it names, so re-sending one changes nothing and a message lost in
//! flight costs one stale number until the next declaration repairs it. The core
//! keys rows on `(module_id, user_id, category)`; idempotence is structural.
//!
//! ## One rhythm, on purpose
//!
//! Unlike drive, office has **no incremental `mark_dirty` path**. Everything it
//! declares is second-order — collaboration journals that are consolidated away,
//! version rows capped at a few dozen, thumbnails of a few tens of kilobytes — and
//! none of it moves fast enough for a five-second reaction time to buy anything.
//! Threading a mark through the collab hot path (one call per keystroke batch)
//! would cost more than the freshness is worth. A complete recount every six
//! hours, plus one at startup, is the whole design.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::state::AppState;

/// How often the complete state is recounted and declared. Same period as drive.
const FULL_SYNC_INTERVAL: Duration = Duration::from_secs(6 * 3_600);

/// First retry delay when a declaration could not be delivered, doubling up to
/// [`FULL_SYNC_INTERVAL`].
///
/// The module starts before the core has necessarily finished accepting
/// registrations, so the very first declaration routinely fails. Without a
/// backoff it would be re-attempted six hours later and the breakdown would sit
/// empty for an afternoon after every reboot.
const FULL_RETRY_MIN: Duration = Duration::from_secs(15);

/// Matches the core's own per-request ceiling (`storage::usage::MAX_ENTRIES`).
/// A larger instance is declared in several calls.
const MAX_ENTRIES: usize = 5_000;

/// Identifier this module declares under. Only consulted by the core when the
/// caller could not be identified from its `X-Internal-Secret`: the core prefers
/// the secret's identity and answers 403 when the two disagree, so naming
/// ourselves in the body can never impersonate another module. It is what makes
/// the declaration work while the module spawner still hands out the master
/// secret, and becomes dead weight — not a bypass — once it derives per-module
/// secrets. `/internal/modules/register` already works this way.
const MODULE_ID: &str = "office";

// ── The closed category vocabulary, as office uses it ────────────────────────

/// Regenerable collaboration and execution machinery. Not billed by the core.
const CAT_CACHE: &str = "cache";
/// Copies office keeps by its own policy, which the user has no way to delete.
/// Not billed by the core — you cannot bill somebody for storage they cannot
/// free. (`versions` is the billed sibling: a history the user *can* purge.)
const CAT_RETENTION: &str = "retention";
/// Inline base64 previews. Not billed by the core.
const CAT_THUMBNAILS: &str = "thumbnails";
/// Objects office made drive create. Never added to any total, by contract.
const CAT_DELEGATED: &str = "delegated";

/// Every byte-bearing query office runs, paired with the category it feeds.
///
/// Each statement must return exactly `(owner uuid, bytes bigint, objects bigint)`
/// and must only read office's own schemas. Keeping them in one table rather than
/// scattered through functions is what lets the tests below assert, mechanically,
/// that office never declares `content` and never reads drive's tables.
///
/// Rows whose owner cannot be resolved (a collaboration journal left behind by a
/// deleted entity, say) are dropped by the `WHERE owner_id IS NOT NULL` guard
/// rather than attributed to somebody arbitrary: an orphan belongs to nobody, and
/// inventing an owner for it would be worse than not counting it.
const OWNED_QUERIES: &[(&str, &str)] = &[
    // ── Yjs collaboration state ──────────────────────────────────────────────
    // `collab_snapshots` / `collab_updates` carry no owner column; the owner is
    // reached through `(entity_type, entity_id)`, with the same mapping
    // `CollabService::entity_owner` uses. Unknown entity types fall through the
    // CASE to NULL and are dropped.
    (
        CAT_CACHE,
        "SELECT owner_id, COALESCE(SUM(sz), 0)::bigint, COUNT(*)::bigint FROM (
             SELECT CASE s.entity_type
                      WHEN 'document'     THEN (SELECT d.owner_id FROM office.documents d     WHERE d.id = s.entity_id)
                      WHEN 'spreadsheet'  THEN (SELECT x.owner_id FROM office.spreadsheets x  WHERE x.id = s.entity_id)
                      WHEN 'presentation' THEN (SELECT p.owner_id FROM office.presentations p WHERE p.id = s.entity_id)
                      WHEN 'diagram'      THEN (SELECT g.owner_id FROM office.diagrams g      WHERE g.id = s.entity_id)
                    END                          AS owner_id,
                    pg_column_size(s.snapshot)   AS sz
               FROM office.collab_snapshots s
           ) t
          WHERE owner_id IS NOT NULL
          GROUP BY owner_id",
    ),
    (
        CAT_CACHE,
        "SELECT owner_id, COALESCE(SUM(sz), 0)::bigint, COUNT(*)::bigint FROM (
             SELECT CASE u.entity_type
                      WHEN 'document'     THEN (SELECT d.owner_id FROM office.documents d     WHERE d.id = u.entity_id)
                      WHEN 'spreadsheet'  THEN (SELECT x.owner_id FROM office.spreadsheets x  WHERE x.id = u.entity_id)
                      WHEN 'presentation' THEN (SELECT p.owner_id FROM office.presentations p WHERE p.id = u.entity_id)
                      WHEN 'diagram'      THEN (SELECT g.owner_id FROM office.diagrams g      WHERE g.id = u.entity_id)
                    END                           AS owner_id,
                    pg_column_size(u.update_data) AS sz
               FROM office.collab_updates u
           ) t
          WHERE owner_id IS NOT NULL
          GROUP BY owner_id",
    ),
    // Whiteboard keeps its own update journal, and it has a direct parent with an
    // owner (`office_wb.boards`). There is no whiteboard snapshot table to measure:
    // migration 000020 dropped `office_wb.yjs_snapshots` when the consolidated
    // board state moved out to a drive file — the table is still in 000014's text,
    // which is why this was verified against a live schema rather than by reading
    // the first migration that mentions it.
    (
        CAT_CACHE,
        "SELECT b.owner_id,
                COALESCE(SUM(pg_column_size(u.update_data)), 0)::bigint,
                COUNT(*)::bigint
           FROM office_wb.yjs_updates u
           JOIN office_wb.boards b ON b.id = u.board_id
          GROUP BY b.owner_id",
    ),
    // ── Replay / execution machinery ─────────────────────────────────────────
    // Idempotency replay bodies: kept only so a retried write returns the same
    // answer, and expendable the moment the client stops retrying.
    (
        CAT_CACHE,
        "SELECT user_id,
                COALESCE(SUM(pg_column_size(body)), 0)::bigint,
                COUNT(*)::bigint
           FROM office.idempotency_keys
          GROUP BY user_id",
    ),
    // Script run records: console transcript, return value and trigger payload.
    // Logs of past executions — reproducible by re-running, never user content.
    (
        CAT_CACHE,
        "SELECT owner_id,
                COALESCE(SUM(
                    pg_column_size(console_output)
                  + COALESCE(pg_column_size(return_value), 0)
                  + COALESCE(pg_column_size(trigger_data), 0)
                ), 0)::bigint,
                COUNT(*)::bigint
           FROM office_script.runs
          GROUP BY owner_id",
    ),
    // Compiled script artefact. Migration 000018 moved the *source* to drive and
    // describes `compiled_code` as a transient artefact regenerated from it —
    // which is the definition of cache.
    (
        CAT_CACHE,
        "SELECT owner_id,
                COALESCE(SUM(pg_column_size(compiled_code)), 0)::bigint,
                (COUNT(*) FILTER (WHERE compiled_code IS NOT NULL))::bigint
           FROM office_script.scripts
          WHERE compiled_code IS NOT NULL
          GROUP BY owner_id",
    ),
    // No dataset result cache line: migration 000019 moved `office_data.datasets.
    // data_cache` out to a drive file precisely because it could get large, so
    // those bytes are drive's to declare, not office's.
    //
    // ── Revision history, kept by office's policy ────────────────────────────
    // `retention`, not `versions`: no route deletes a version and no setting lets
    // a person turn history off, so these bytes are office's choice to keep, not
    // theirs to free. Attributed to the *entity owner* rather than `author_id`:
    // the history belongs to the document, and charging a collaborator for a
    // revision they wrote in somebody else's file would be indefensible — even
    // for a category that is not charged, the breakdown has to read correctly.
    (
        CAT_RETENTION,
        "SELECT d.owner_id,
                COALESCE(SUM(pg_column_size(v.content_json)), 0)::bigint,
                COUNT(*)::bigint
           FROM office.document_versions v
           JOIN office.documents d ON d.id = v.document_id
          GROUP BY d.owner_id",
    ),
    (
        CAT_RETENTION,
        "SELECT s.owner_id,
                COALESCE(SUM(pg_column_size(v.snapshot)), 0)::bigint,
                COUNT(*)::bigint
           FROM office.spreadsheet_versions v
           JOIN office.spreadsheets s ON s.id = v.spreadsheet_id
          GROUP BY s.owner_id",
    ),
    // ── Inline previews ──────────────────────────────────────────────────────
    // `thumbnail_path` is a misnomer: `whiteboard::handlers::thumbnail` stores a
    // full `data:image/png;base64,…` URL in it, so the column *is* the image. The
    // `LIKE 'data:%'` guard is what makes that explicit and keeps the measure
    // honest if the column ever goes back to holding an actual path — a path's
    // length is not a thumbnail's weight.
    (
        CAT_THUMBNAILS,
        "SELECT owner_id,
                COALESCE(SUM(pg_column_size(thumbnail_path)), 0)::bigint,
                COUNT(*)::bigint
           FROM office_wb.boards
          WHERE thumbnail_path LIKE 'data:%'
          GROUP BY owner_id",
    ),
    (
        CAT_THUMBNAILS,
        "SELECT b.owner_id,
                COALESCE(SUM(pg_column_size(f.thumbnail_path)), 0)::bigint,
                COUNT(*)::bigint
           FROM office_wb.frames f
           JOIN office_wb.boards b ON b.id = f.board_id
          WHERE f.thumbnail_path LIKE 'data:%'
          GROUP BY b.owner_id",
    ),
    // Custom diagram shapes carry a client-supplied preview, same shape.
    (
        CAT_THUMBNAILS,
        "SELECT owner_id,
                COALESCE(SUM(pg_column_size(thumbnail)), 0)::bigint,
                COUNT(*)::bigint
           FROM office.custom_shapes
          WHERE thumbnail LIKE 'data:%'
          GROUP BY owner_id",
    ),
];

/// Counts the objects office made drive create, per owner.
///
/// One row per entity holding a non-null `file_id`. Draft and imported-source
/// references (`draft_file_id`, `source_file_id`) are deliberately left out: they
/// are transient or secondary, drive weighs them all the same, and inflating a
/// figure that is never summed buys nothing.
const DELEGATED_QUERY: &str = "SELECT owner_id, COUNT(*)::bigint FROM (
        SELECT owner_id FROM office.documents        WHERE file_id IS NOT NULL
        UNION ALL SELECT owner_id FROM office.spreadsheets   WHERE file_id IS NOT NULL
        UNION ALL SELECT owner_id FROM office.presentations  WHERE file_id IS NOT NULL
        UNION ALL SELECT owner_id FROM office.diagrams       WHERE file_id IS NOT NULL
        UNION ALL SELECT owner_id FROM office.projects       WHERE file_id IS NOT NULL
        UNION ALL SELECT owner_id FROM office_wb.boards      WHERE file_id IS NOT NULL
        UNION ALL SELECT owner_id FROM office_script.scripts WHERE file_id IS NOT NULL
        UNION ALL SELECT owner_id FROM office_maths.formulas WHERE file_id IS NOT NULL
        UNION ALL SELECT owner_id FROM office_data.datasets  WHERE file_id IS NOT NULL
        UNION ALL SELECT owner_id FROM office_data.reports   WHERE file_id IS NOT NULL
    ) t
    GROUP BY owner_id";

/// One `(account, category)` figure, as declared.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    user_id: Uuid,
    category: &'static str,
    used_bytes: i64,
    object_count: i64,
}

/// Recounts everything office holds, plus the delegation line.
///
/// A failing query is logged and skipped rather than aborting the whole
/// declaration: one broken sub-module should cost its own line, not the entire
/// breakdown. The declaration is still marked `full`, which means a category that
/// failed here is *retired* by the core until the next sync repairs it — the
/// honest outcome, since publishing a stale figure as current state would be
/// worse than publishing none.
async fn collect(db: &PgPool) -> Vec<Entry> {
    // Several queries feed the same category (documents and whiteboard both feed
    // `cache`), so figures are folded per `(user, category)` before being sent:
    // the core keys rows on that pair and would keep only the last one otherwise.
    let mut acc: HashMap<(Uuid, &'static str), (i64, i64)> = HashMap::new();

    for (category, sql) in OWNED_QUERIES {
        match sqlx::query_as::<_, (Uuid, i64, i64)>(sql).fetch_all(db).await {
            Ok(rows) => {
                for (user_id, bytes, objects) in rows {
                    let slot = acc.entry((user_id, *category)).or_insert((0, 0));
                    slot.0 += bytes;
                    slot.1 += objects;
                }
            }
            Err(e) => tracing::error!(
                error = %e,
                catégorie = *category,
                "Recomptage de consommation échoué pour une requête — catégorie incomplète"
            ),
        }
    }

    match sqlx::query_as::<_, (Uuid, i64)>(DELEGATED_QUERY)
        .fetch_all(db)
        .await
    {
        Ok(rows) => {
            for (user_id, objects) in rows {
                // `used_bytes: 0` is deliberate and load-bearing. Office knows how
                // many objects it caused, never what they weigh — drive weighs
                // them and declares them as `content`. Putting any figure here
                // would be a guess that the console might one day add up.
                let slot = acc.entry((user_id, CAT_DELEGATED)).or_insert((0, 0));
                slot.1 += objects;
            }
        }
        Err(e) => tracing::error!(
            error = %e,
            "Recomptage des objets délégués à drive échoué"
        ),
    }

    let mut entries: Vec<Entry> = acc
        .into_iter()
        .map(|((user_id, category), (used_bytes, object_count))| Entry {
            user_id,
            category,
            used_bytes,
            object_count,
        })
        .collect();

    // Stable order so consecutive declarations chunk identically — a moving
    // chunk boundary would make partial declarations retire different accounts
    // each time.
    entries.sort_by(|a, b| (a.user_id, a.category).cmp(&(b.user_id, b.category)));
    entries
}

/// How many calls a declaration of `n` entries takes. Zero entries still takes
/// one: an empty `full` declaration is a statement, not a no-op.
fn page_count(n: usize) -> usize {
    if n == 0 {
        1
    } else {
        n.div_ceil(MAX_ENTRIES)
    }
}

/// Whether `full` may actually be claimed on the wire.
///
/// A chunked declaration marked `full` would retire every entry outside whichever
/// chunk happened to be sent last. Beyond the ceiling the declaration is therefore
/// sent as partial: correct, but unable to retire a line until the instance drops
/// back under `MAX_ENTRIES`.
fn claims_full(full: bool, n: usize) -> bool {
    full && n <= MAX_ENTRIES
}

/// Sends one declaration to the core.
async fn send(
    http: &reqwest::Client,
    state: &AppState,
    entries: &[Entry],
    full: bool,
) -> Result<(), String> {
    let url = format!("{}/internal/storage/usage", state.settings.core.url);
    let usage: Vec<_> = entries
        .iter()
        .map(|e| {
            json!({
                "user_id":      e.user_id,
                "category":     e.category,
                "used_bytes":   e.used_bytes,
                "object_count": e.object_count,
            })
        })
        .collect();

    let resp = http
        .post(&url)
        .header(
            "X-Internal-Secret",
            state.settings.core.internal_secret.as_str(),
        )
        .json(&json!({ "module_id": MODULE_ID, "full": full, "usage": usage }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status().is_success() {
        return Ok(());
    }

    // The status alone does not say which of several validations refused the
    // declaration, and this runs unattended — a log line reading "HTTP 422" costs
    // an afternoon the next time the contract shifts.
    let status = resp.status();
    let detail = resp.text().await.unwrap_or_default();
    let detail: String = detail.chars().take(300).collect();
    Err(format!("HTTP {status} {detail}"))
}

/// Declares `entries` in as many calls as the core's ceiling requires.
///
/// Returns `true` when every call landed; the caller reschedules on that.
async fn declare(http: &reqwest::Client, state: &AppState, entries: Vec<Entry>, full: bool) -> bool {
    let full_on_wire = claims_full(full, entries.len());
    if full && !full_on_wire {
        tracing::warn!(
            entrées = entries.len(),
            envois = page_count(entries.len()),
            "Synchronisation complète découpée : déclarée en plusieurs envois partiels"
        );
    }

    // An empty full declaration is meaningful and must still be sent: it is how
    // office says "I hold nothing for anybody", which the core has to be able to
    // tell apart from "office has never declared".
    if entries.is_empty() {
        if !full_on_wire {
            // An empty *partial* declaration says nothing at all; sending it would
            // be a round-trip for no information.
            return true;
        }
        return match send(http, state, &[], true).await {
            Ok(()) => {
                tracing::debug!("Consommation déclarée : aucune entrée");
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "Déclaration de consommation échouée");
                false
            }
        };
    }

    let mut declared_bytes: i64 = 0;
    let mut sent = 0usize;
    let mut all_ok = true;
    for chunk in entries.chunks(MAX_ENTRIES) {
        match send(http, state, chunk, full_on_wire).await {
            Ok(()) => {
                declared_bytes += chunk.iter().map(|e| e.used_bytes).sum::<i64>();
                sent += chunk.len();
            }
            Err(e) => {
                all_ok = false;
                tracing::warn!(error = %e, entrées = chunk.len(), "Déclaration de consommation échouée");
            }
        }
    }

    if sent > 0 {
        tracing::debug!(
            entrées = sent,
            octets = declared_bytes,
            complète = full_on_wire,
            "Consommation déclarée au core"
        );
    }
    all_ok
}

/// The reporter task. Started once at bootstrap.
///
/// Its own `reqwest::Client` rather than one threaded through `AppState`: this is
/// the only outbound HTTP office makes from a background task, and a client is a
/// connection pool, not a resource worth plumbing for a call every six hours.
pub async fn run_reporter(state: AppState) {
    let http = reqwest::Client::new();

    // Absolute deadline rather than an `interval`, so a failed sync can be pulled
    // forward without the retries drifting the normal period.
    let mut next_at = tokio::time::Instant::now(); // the first one is immediate
    let mut backoff = FULL_RETRY_MIN;

    tracing::info!("Rapporteur de consommation démarré (déclaration au core)");

    loop {
        tokio::time::sleep_until(next_at).await;

        let entries = collect(&state.db).await;
        let delivered = declare(&http, &state, entries, true).await;

        let now = tokio::time::Instant::now();
        if delivered {
            next_at = now + FULL_SYNC_INTERVAL;
            backoff = FULL_RETRY_MIN;
        } else {
            next_at = now + backoff;
            backoff = (backoff * 2).min(FULL_SYNC_INTERVAL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule this whole module exists to respect, checked mechanically.
    ///
    /// Office writes every byte of user content into drive, drive declares it as
    /// `content`, and office declaring the same bytes again would double-count
    /// exactly the quantity the core bills. A reviewer adding a query here has to
    /// trip this test before they can invent a `content` line.
    #[test]
    fn office_never_declares_content() {
        for (category, _) in OWNED_QUERIES {
            assert_ne!(
                *category, "content",
                "office must never declare `content`: those bytes belong to drive"
            );
        }
        assert_ne!(CAT_DELEGATED, "content");
    }

    /// Only categories from the core's closed vocabulary, and only the ones that
    /// make sense for a module holding no content of its own.
    #[test]
    fn emitted_categories_are_the_expected_non_billed_set() {
        let vocabulary = [
            "content",
            "trash",
            "versions",
            "retention",
            "thumbnails",
            "index",
            "cache",
            "staging",
            "system",
            "delegated",
        ];
        let allowed = [CAT_CACHE, CAT_RETENTION, CAT_THUMBNAILS];
        for (category, _) in OWNED_QUERIES {
            assert!(vocabulary.contains(category), "category outside the closed vocabulary: {category}");
            assert!(
                allowed.contains(category),
                "unexpected category emitted by office: {category}"
            );
        }
        assert!(vocabulary.contains(&CAT_DELEGATED));

        // The billed set is `content`, `trash` and `versions` — the three things a
        // person can free themselves. Office emits none of them, so nothing office
        // declares can ever move somebody's quota. `retention` rather than
        // `versions` for the revision history is the whole point: no route deletes
        // a version, so those bytes are not the user's to release.
        let billed = ["content", "trash", "versions"];
        for (category, _) in OWNED_QUERIES {
            assert!(
                !billed.contains(category),
                "office must not emit the billed category `{category}`"
            );
        }
    }

    /// Every declared byte must come from a table office owns.
    ///
    /// Reading `drive.files` here would be the same double count arriving through
    /// a join instead of a category name.
    #[test]
    fn queries_only_read_office_schemas() {
        let own = ["office.", "office_wb.", "office_data.", "office_script.", "office_maths."];
        for (category, sql) in OWNED_QUERIES {
            assert!(
                !sql.contains("drive."),
                "query for `{category}` reads drive's tables"
            );
            assert!(
                !sql.contains("core."),
                "query for `{category}` reads the core's tables"
            );
            assert!(
                own.iter().any(|s| sql.contains(*s)),
                "query for `{category}` names no office schema"
            );
        }
        assert!(!DELEGATED_QUERY.contains("drive."));
        assert!(!DELEGATED_QUERY.contains("core."));
    }

    /// Every query must expose the three columns `collect` decodes, or the
    /// `query_as` would fail at runtime in a background task nobody watches.
    #[test]
    fn queries_group_by_an_owner() {
        for (category, sql) in OWNED_QUERIES {
            assert!(
                sql.contains("GROUP BY"),
                "query for `{category}` must aggregate per account"
            );
        }
        assert!(DELEGATED_QUERY.contains("GROUP BY"));
    }

    #[test]
    fn paging_respects_the_core_ceiling() {
        // Zero entries is one call, not none: an empty full declaration says
        // "office holds nothing", which the core must receive.
        assert_eq!(page_count(0), 1);
        assert_eq!(page_count(1), 1);
        assert_eq!(page_count(MAX_ENTRIES), 1);
        assert_eq!(page_count(MAX_ENTRIES + 1), 2);
        assert_eq!(page_count(3 * MAX_ENTRIES), 3);
        assert_eq!(page_count(3 * MAX_ENTRIES + 1), 4);
    }

    #[test]
    fn full_is_only_claimed_on_a_single_call() {
        assert!(claims_full(true, 0));
        assert!(claims_full(true, MAX_ENTRIES));
        // Beyond the ceiling the declaration goes out partial — claiming `full`
        // on the last chunk would retire every account in the earlier ones.
        assert!(!claims_full(true, MAX_ENTRIES + 1));
        assert!(!claims_full(false, 1));
    }

    #[test]
    fn chunking_covers_every_entry_exactly_once() {
        let entries: Vec<Entry> = (0..MAX_ENTRIES + 7)
            .map(|i| Entry {
                user_id: Uuid::from_u128(i as u128 + 1),
                category: CAT_CACHE,
                used_bytes: 1,
                object_count: 1,
            })
            .collect();

        let chunks: Vec<&[Entry]> = entries.chunks(MAX_ENTRIES).collect();
        assert_eq!(chunks.len(), page_count(entries.len()));
        assert_eq!(chunks.iter().map(|c| c.len()).sum::<usize>(), entries.len());
        assert_eq!(chunks[0].len(), MAX_ENTRIES);
        assert_eq!(chunks[1].len(), 7);
    }

    /// The delegation line is informative only: a weight would invite the console
    /// to add it to something.
    #[test]
    fn delegated_entries_carry_no_weight() {
        let e = Entry {
            user_id: Uuid::nil(),
            category: CAT_DELEGATED,
            used_bytes: 0,
            object_count: 42,
        };
        assert_eq!(e.used_bytes, 0);
        assert!(e.object_count > 0);
    }
}
