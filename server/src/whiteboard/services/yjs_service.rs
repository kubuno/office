use anyhow::Result;
use uuid::Uuid;

use crate::services::content_files;
use crate::state::AppState;

const CONSOLIDATE_THRESHOLD: i64 = 100;

pub struct YjsService;

impl YjsService {
    /// (owner_id, file_id) du board.
    async fn board_ref(state: &AppState, board_id: Uuid) -> Result<(Uuid, Option<Uuid>)> {
        let row: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
            "SELECT owner_id, file_id FROM office_wb.boards WHERE id = $1",
        )
        .bind(board_id)
        .fetch_optional(&state.db)
        .await?;
        row.ok_or_else(|| anyhow::anyhow!("board {board_id} introuvable"))
    }

    /// Charge le document Yjs : snapshot (fichier .kbwbd) + updates incrémentaux (base).
    pub async fn load_document(state: &AppState, board_id: Uuid) -> Result<Vec<Vec<u8>>> {
        let (owner_id, file_id) = Self::board_ref(state, board_id).await?;
        let mut parts: Vec<Vec<u8>> = Vec::new();

        if let Some(fid) = file_id {
            let snap = content_files::read_whiteboard_snapshot(state, owner_id, fid).await
                .map_err(|e| anyhow::anyhow!("lecture snapshot whiteboard: {e}"))?;
            if !snap.is_empty() {
                parts.push(snap);
            }
        }

        let updates: Vec<(Vec<u8>,)> = sqlx::query_as(
            "SELECT update_data FROM office_wb.yjs_updates WHERE board_id = $1 ORDER BY created_at ASC",
        )
        .bind(board_id)
        .fetch_all(&state.db)
        .await?;

        parts.extend(updates.into_iter().map(|(d,)| d));
        Ok(parts)
    }

    /// Sauvegarde un update incrémental (base) et consolide si nécessaire.
    pub async fn save_update(
        state: &AppState,
        board_id: Uuid,
        data: Vec<u8>,
        origin: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO office_wb.yjs_updates (board_id, update_data, origin) VALUES ($1, $2, $3)",
        )
        .bind(board_id)
        .bind(&data)
        .bind(&origin)
        .execute(&state.db)
        .await?;

        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM office_wb.yjs_updates WHERE board_id = $1",
        )
        .bind(board_id)
        .fetch_one(&state.db)
        .await?;

        if count.0 >= CONSOLIDATE_THRESHOLD {
            Self::consolidate(state, board_id).await?;
        }

        Ok(())
    }

    /// Fusionne tous les updates dans le snapshot du fichier .kbwbd.
    pub async fn consolidate(state: &AppState, board_id: Uuid) -> Result<()> {
        let updates: Vec<(Vec<u8>,)> = sqlx::query_as(
            "SELECT update_data FROM office_wb.yjs_updates WHERE board_id = $1 ORDER BY created_at ASC",
        )
        .bind(board_id)
        .fetch_all(&state.db)
        .await?;

        if updates.is_empty() {
            return Ok(());
        }

        let (owner_id, file_id) = Self::board_ref(state, board_id).await?;

        // Snapshot existant (fichier) puis ajout des updates (le flux Yjs accepte
        // la concaténation des updates binaires).
        let mut merged = match file_id {
            Some(fid) => content_files::read_whiteboard_snapshot(state, owner_id, fid).await
                .map_err(|e| anyhow::anyhow!("lecture snapshot whiteboard: {e}"))?,
            None => Vec::new(),
        };
        for (u,) in &updates {
            merged.extend_from_slice(u);
        }

        // Fichier créé si absent (board pré-migration), sinon réécrit.
        let fid = match file_id {
            Some(fid) => fid,
            None => {
                let (title,): (String,) = sqlx::query_as(
                    "SELECT title FROM office_wb.boards WHERE id = $1",
                )
                .bind(board_id)
                .fetch_one(&state.db)
                .await?;
                let fid = content_files::create_whiteboard_file(state, owner_id, &title).await
                    .map_err(|e| anyhow::anyhow!("création fichier whiteboard: {e}"))?;
                sqlx::query("UPDATE office_wb.boards SET file_id = $1 WHERE id = $2")
                    .bind(fid).bind(board_id)
                    .execute(&state.db).await?;
                fid
            }
        };

        content_files::write_whiteboard_snapshot(state, owner_id, fid, &merged).await
            .map_err(|e| anyhow::anyhow!("écriture snapshot whiteboard: {e}"))?;

        sqlx::query("DELETE FROM office_wb.yjs_updates WHERE board_id = $1")
            .bind(board_id)
            .execute(&state.db)
            .await?;

        Ok(())
    }
}
