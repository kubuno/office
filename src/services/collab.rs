//! Service de collaboration temps réel (Yjs) GÉNÉRIQUE, partagé par tous les
//! éditeurs Office. Le backend ne comprend pas la structure Yjs : il stocke et
//! relaie des updates binaires opaques (concaténables) + un snapshot consolidé.
//! L'état durable « utilisateur » reste le fichier .kb*** JSON (écrit par les
//! clients) ; ici on ne garde que l'état CRDT (snapshot + journal) en base.

use anyhow::Result;
use uuid::Uuid;

use crate::state::AppState;

const CONSOLIDATE_THRESHOLD: i64 = 100;

pub struct CollabService;

impl CollabService {
    /// Propriétaire de l'entité (None si introuvable / type inconnu) — pour l'auth.
    pub async fn entity_owner(state: &AppState, entity_type: &str, entity_id: Uuid) -> Result<Option<Uuid>> {
        let table = match entity_type {
            "document"     => "documents",
            "spreadsheet"  => "spreadsheets",
            "presentation" => "presentations",
            "diagram"      => "diagrams",
            _ => return Ok(None),
        };
        // entity_type est validé par le match ci-dessus → pas d'injection.
        let sql = format!("SELECT owner_id FROM {table} WHERE id = $1");
        let owner: Option<Uuid> = sqlx::query_scalar(&sql)
            .bind(entity_id)
            .fetch_optional(&state.db)
            .await?;
        Ok(owner)
    }

    /// Charge l'état Yjs : snapshot consolidé (base) + updates incrémentaux.
    pub async fn load_document(state: &AppState, entity_type: &str, entity_id: Uuid) -> Result<Vec<Vec<u8>>> {
        let mut parts: Vec<Vec<u8>> = Vec::new();

        let snap: Option<(Vec<u8>,)> = sqlx::query_as(
            "SELECT snapshot FROM collab_snapshots WHERE entity_type = $1 AND entity_id = $2",
        )
        .bind(entity_type).bind(entity_id)
        .fetch_optional(&state.db).await?;
        if let Some((s,)) = snap {
            if !s.is_empty() { parts.push(s); }
        }

        let updates: Vec<(Vec<u8>,)> = sqlx::query_as(
            "SELECT update_data FROM collab_updates WHERE entity_type = $1 AND entity_id = $2 ORDER BY created_at ASC",
        )
        .bind(entity_type).bind(entity_id)
        .fetch_all(&state.db).await?;
        parts.extend(updates.into_iter().map(|(d,)| d));

        Ok(parts)
    }

    /// Persiste un update Yjs incrémental ; consolide au-delà du seuil.
    pub async fn save_update(
        state: &AppState, entity_type: &str, entity_id: Uuid, data: Vec<u8>, origin: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO collab_updates (entity_type, entity_id, update_data, origin) VALUES ($1, $2, $3, $4)",
        )
        .bind(entity_type).bind(entity_id).bind(&data).bind(&origin)
        .execute(&state.db).await?;

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM collab_updates WHERE entity_type = $1 AND entity_id = $2",
        )
        .bind(entity_type).bind(entity_id)
        .fetch_one(&state.db).await?;

        if count.0 >= CONSOLIDATE_THRESHOLD {
            Self::consolidate(state, entity_type, entity_id).await?;
        }
        Ok(())
    }

    /// Fusionne snapshot + updates dans le snapshot (concaténation binaire Yjs),
    /// puis vide le journal d'updates.
    pub async fn consolidate(state: &AppState, entity_type: &str, entity_id: Uuid) -> Result<()> {
        let updates: Vec<(Vec<u8>,)> = sqlx::query_as(
            "SELECT update_data FROM collab_updates WHERE entity_type = $1 AND entity_id = $2 ORDER BY created_at ASC",
        )
        .bind(entity_type).bind(entity_id)
        .fetch_all(&state.db).await?;
        if updates.is_empty() { return Ok(()); }

        let snap: Option<(Vec<u8>,)> = sqlx::query_as(
            "SELECT snapshot FROM collab_snapshots WHERE entity_type = $1 AND entity_id = $2",
        )
        .bind(entity_type).bind(entity_id)
        .fetch_optional(&state.db).await?;

        let mut merged = snap.map(|(s,)| s).unwrap_or_default();
        for (u,) in &updates { merged.extend_from_slice(u); }

        sqlx::query(
            "INSERT INTO collab_snapshots (entity_type, entity_id, snapshot, updated_at)
             VALUES ($1, $2, $3, NOW())
             ON CONFLICT (entity_type, entity_id) DO UPDATE SET snapshot = EXCLUDED.snapshot, updated_at = NOW()",
        )
        .bind(entity_type).bind(entity_id).bind(&merged)
        .execute(&state.db).await?;

        sqlx::query("DELETE FROM collab_updates WHERE entity_type = $1 AND entity_id = $2")
            .bind(entity_type).bind(entity_id)
            .execute(&state.db).await?;

        Ok(())
    }
}
