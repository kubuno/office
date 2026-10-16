//! Collaboration epochs: keeping the web's real-time rooms and REST saves apart.
//!
//! The web editors (documents, spreadsheets, presentations) edit a Yjs room
//! hosted by the core (`/collab/:room/sync`). The room only seeds itself from the
//! stored content when it is empty; once it holds state it is the reference, and
//! the web keeps writing that state back to the stored content (autosave). A REST
//! save made by another client (desktop apps, Android, a version restore) used to
//! be silently overwritten that way: the room still held the previous content.
//!
//! Every such entity now carries an epoch (`collab_epoch`, migration 000065) and
//! its room is named after it ([`room_name`]):
//!
//! * a REST write that replaces the content from outside the room bumps the epoch
//!   **in the same transaction as the content write**, while holding the row lock
//!   ([`begin_content_write`]). The previous room is abandoned: the next web
//!   session joins a new, empty room and seeds it from the new content;
//! * a write coming from the room (the web's autosave) names the epoch of its room
//!   in [`EPOCH_HEADER`]. It goes through only while that epoch is current, under
//!   the same row lock - a session still holding the old state can never write it
//!   back, whatever the timing;
//! * the core is told to close the abandoned room ([`announce_replaced`]): its
//!   connected clients receive `{"type":"replaced"}` and reload, its persisted
//!   state is dropped, and the module's room authorization refuses any further
//!   join to a stale epoch (`collab_authz`).
//!
//! Persisted room state cannot race the new content either: it lives under the
//! old room's name, which nothing reads any more.

use std::sync::OnceLock;
use std::time::Duration;

use axum::http::HeaderMap;
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    errors::{OfficeError, Result},
    state::AppState,
};

/// Request header a web session uses to name the epoch of its room on a save.
pub const EPOCH_HEADER: &str = "x-collab-epoch";

/// Separator between the entity id and the epoch in a room name.
const EPOCH_SEPARATOR: char = '~';

/// Entities whose web editor works through a core collaboration room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollabEntity {
    Document,
    Spreadsheet,
    Presentation,
}

impl CollabEntity {
    /// The entity type as it appears in room names and routes.
    pub fn parse(entity_type: &str) -> Option<Self> {
        match entity_type {
            "document" => Some(Self::Document),
            "spreadsheet" => Some(Self::Spreadsheet),
            "presentation" => Some(Self::Presentation),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Spreadsheet => "spreadsheet",
            Self::Presentation => "presentation",
        }
    }

    // One fully written statement per entity: no SQL text is assembled at run time.
    fn lock_sql(self) -> &'static str {
        match self {
            Self::Document => "SELECT collab_epoch FROM documents WHERE id = $1 FOR UPDATE",
            Self::Spreadsheet => "SELECT collab_epoch FROM spreadsheets WHERE id = $1 FOR UPDATE",
            Self::Presentation => "SELECT collab_epoch FROM presentations WHERE id = $1 FOR UPDATE",
        }
    }

    fn read_sql(self) -> &'static str {
        match self {
            Self::Document => "SELECT collab_epoch FROM documents WHERE id = $1",
            Self::Spreadsheet => "SELECT collab_epoch FROM spreadsheets WHERE id = $1",
            Self::Presentation => "SELECT collab_epoch FROM presentations WHERE id = $1",
        }
    }

    fn bump_sql(self) -> &'static str {
        match self {
            Self::Document => "UPDATE documents SET collab_epoch = $2 WHERE id = $1",
            Self::Spreadsheet => "UPDATE spreadsheets SET collab_epoch = $2 WHERE id = $1",
            Self::Presentation => "UPDATE presentations SET collab_epoch = $2 WHERE id = $1",
        }
    }
}

/// Name of the core room holding `entity`'s collaborative state at `epoch`.
///
/// Epoch 0 keeps the historical name, so rooms opened before epochs existed stay
/// valid until the first REST replacement.
pub fn room_name(entity: CollabEntity, id: Uuid, epoch: i64) -> String {
    if epoch == 0 {
        format!("office-{}:{id}", entity.as_str())
    } else {
        format!("office-{}:{id}{EPOCH_SEPARATOR}{epoch}", entity.as_str())
    }
}

/// A room name taken apart: `office-<type>:<uuid>[~<epoch>]` (the `office:`
/// spelling is accepted too). The entity type is returned as written, so the
/// caller decides what an unknown type means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRoom {
    pub entity_type: String,
    pub id: Uuid,
    /// `None` when the name carries no epoch (epoch 0, or an entity without epochs).
    pub epoch: Option<i64>,
}

pub fn parse_room(room: &str) -> Result<ParsedRoom> {
    let rest = room
        .strip_prefix("office-")
        .or_else(|| room.strip_prefix("office:"))
        .unwrap_or(room);
    let (entity_type, tail) = rest
        .split_once(':')
        .ok_or_else(|| OfficeError::Validation(format!("invalid room: {room}")))?;
    let (id_str, epoch) = match tail.split_once(EPOCH_SEPARATOR) {
        Some((id_str, epoch_str)) => {
            let epoch = epoch_str
                .parse::<i64>()
                .ok()
                .filter(|e| *e > 0)
                .ok_or_else(|| OfficeError::Validation(format!("invalid epoch in room: {room}")))?;
            (id_str, Some(epoch))
        }
        None => (tail, None),
    };
    let id = Uuid::parse_str(id_str)
        .map_err(|_| OfficeError::Validation(format!("invalid uuid in room: {room}")))?;
    Ok(ParsedRoom { entity_type: entity_type.to_string(), id, epoch })
}

/// The epoch a web session claims for its save, if any. A malformed value is a
/// client bug and is refused rather than read as "no epoch" - that reading would
/// turn a room save into a replacement and reset everyone's room.
pub fn client_epoch(headers: &HeaderMap) -> Result<Option<i64>> {
    let Some(raw) = headers.get(EPOCH_HEADER) else { return Ok(None) };
    raw.to_str()
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .filter(|e| *e >= 0)
        .map(Some)
        .ok_or_else(|| OfficeError::Validation(format!("{EPOCH_HEADER} must be a non-negative integer")))
}

/// What a content write does to the room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpochChange {
    pub previous: i64,
    pub current: i64,
}

impl EpochChange {
    /// No content write: the epoch stays as it is.
    pub fn unchanged(epoch: i64) -> Self {
        Self { previous: epoch, current: epoch }
    }

    /// The write replaced the content from outside the room.
    pub fn replaced(&self) -> bool {
        self.current != self.previous
    }
}

/// The rule, without I/O: a save naming the current epoch comes from the live
/// room and leaves it alone; a save naming another epoch comes from an abandoned
/// room and is refused; a save naming none replaces the content and opens a new
/// epoch.
pub fn decide(current: i64, client: Option<i64>) -> Result<EpochChange> {
    match client {
        Some(e) if e == current => Ok(EpochChange::unchanged(current)),
        Some(_) => Err(OfficeError::CollabEpochStale { current }),
        None => {
            let next = current
                .checked_add(1)
                .ok_or_else(|| OfficeError::Internal(anyhow::anyhow!("collab epoch overflow")))?;
            Ok(EpochChange { previous: current, current: next })
        }
    }
}

/// Locks the entity row for the rest of `tx` and returns its epoch.
pub async fn lock(tx: &mut Transaction<'_, Postgres>, entity: CollabEntity, id: Uuid) -> Result<i64> {
    let epoch: Option<i64> = sqlx::query_scalar(entity.lock_sql())
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, %id, entity = entity.as_str(), "collab epoch: lock failed");
            OfficeError::Database(e)
        })?;
    epoch.ok_or_else(|| OfficeError::NotFound(format!("{} {id}", entity.as_str())))
}

/// First step of a content write: locks the row, applies [`decide`] and records
/// the new epoch - all inside `tx`, which the caller commits only once the
/// content itself is written. Until then the bump is invisible, and a failed
/// content write rolls it back with everything else.
pub async fn begin_content_write(
    tx: &mut Transaction<'_, Postgres>,
    entity: CollabEntity,
    id: Uuid,
    client: Option<i64>,
) -> Result<EpochChange> {
    let current = lock(tx, entity, id).await?;
    let change = decide(current, client)?;
    if change.replaced() {
        sqlx::query(entity.bump_sql())
            .bind(id)
            .bind(change.current)
            .execute(&mut **tx)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, %id, entity = entity.as_str(), "collab epoch: bump failed");
                OfficeError::Database(e)
            })?;
    }
    Ok(change)
}

/// Current epoch of an entity (`None` when it does not exist).
pub async fn current(db: &PgPool, entity: CollabEntity, id: Uuid) -> Result<Option<i64>> {
    sqlx::query_scalar(entity.read_sql())
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, %id, entity = entity.as_str(), "collab epoch: read failed");
            OfficeError::Database(e)
        })
}

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// After the commit of a replacing write: asks the core to close the abandoned
/// room (notify its clients, drop its persisted state). Best effort and off the
/// request path - correctness does not depend on it: the stale room can no longer
/// be joined nor written back. Without it the web only learns at its next save
/// or reconnection.
pub fn announce_replaced(state: &AppState, entity: CollabEntity, id: Uuid, change: EpochChange) {
    if !change.replaced() {
        return;
    }
    let room = room_name(entity, id, change.previous);
    let url = format!("{}/internal/collab/rooms/close", state.settings.core.url.trim_end_matches('/'));
    let secret = state.settings.core.internal_secret.clone();
    let epoch = change.current;
    tokio::spawn(async move {
        let resp = http()
            .post(&url)
            .header("X-Internal-Secret", secret)
            .json(&json!({ "room": room, "epoch": epoch }))
            .timeout(Duration::from_secs(5))
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => {
                tracing::info!(room = %room, epoch, "collab: abandoned room closed after a REST replacement");
            }
            Ok(r) => {
                tracing::warn!(room = %room, status = %r.status(), "collab: the core refused to close the abandoned room");
            }
            Err(e) => {
                tracing::warn!(room = %room, error = %e, "collab: core unreachable to close the abandoned room");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Uuid {
        Uuid::parse_str("0b6f2f2e-6d3c-4c51-9a55-1f3b8f4f7e10").expect("uuid")
    }

    #[test]
    fn epoch_zero_keeps_the_historical_room_name() {
        assert_eq!(room_name(CollabEntity::Document, id(), 0), format!("office-document:{}", id()));
        assert_eq!(room_name(CollabEntity::Spreadsheet, id(), 3), format!("office-spreadsheet:{}~3", id()));
    }

    #[test]
    fn room_names_round_trip() {
        for (entity, epoch) in [(CollabEntity::Document, 0), (CollabEntity::Presentation, 42)] {
            let parsed = parse_room(&room_name(entity, id(), epoch)).expect("parse");
            assert_eq!(parsed.entity_type, entity.as_str());
            assert_eq!(parsed.id, id());
            assert_eq!(parsed.epoch.unwrap_or(0), epoch);
        }
        assert_eq!(parse_room(&format!("office:document:{}", id())).expect("legacy").epoch, None);
    }

    #[test]
    fn malformed_rooms_are_refused() {
        for bad in ["office-document", "office-document:not-a-uuid", &format!("office-document:{}~", id()),
                    &format!("office-document:{}~-1", id()), &format!("office-document:{}~0", id())] {
            assert!(parse_room(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn a_save_from_the_live_room_keeps_the_epoch() {
        assert_eq!(decide(4, Some(4)).expect("ok"), EpochChange::unchanged(4));
    }

    #[test]
    fn a_save_from_an_abandoned_room_is_refused() {
        assert!(matches!(decide(5, Some(4)), Err(OfficeError::CollabEpochStale { current: 5 })));
        // A client cannot jump ahead either.
        assert!(matches!(decide(5, Some(6)), Err(OfficeError::CollabEpochStale { current: 5 })));
    }

    #[test]
    fn a_save_from_outside_the_room_opens_a_new_epoch() {
        let change = decide(7, None).expect("ok");
        assert!(change.replaced());
        assert_eq!((change.previous, change.current), (7, 8));
    }

    #[test]
    fn the_epoch_header_is_validated() {
        let mut h = HeaderMap::new();
        assert_eq!(client_epoch(&h).expect("absent"), None);
        h.insert(EPOCH_HEADER, "12".parse().expect("hv"));
        assert_eq!(client_epoch(&h).expect("valid"), Some(12));
        for bad in ["", "abc", "-1", "1.5"] {
            h.insert(EPOCH_HEADER, bad.parse().expect("hv"));
            assert!(client_epoch(&h).is_err(), "{bad:?} should be refused");
        }
    }
}
