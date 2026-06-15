use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    Extension,
    response::Response,
};
use futures::{sink::SinkExt, stream::StreamExt};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::{
    middleware::OfficeUser,
    state::AppState,
    whiteboard::services::yjs_service::YjsService,
};

// ── Hub de collaboration (un par board) ───────────────────────────────────────

/// Trame relayée : update Yjs binaire OU message d'awareness texte. Indispensable
/// de préserver le type : un relais texte renvoyé en binaire serait pris pour un
/// update Yjs côté client → corruption du document.
#[derive(Clone)]
enum Frame {
    Bin(Vec<u8>),
    Txt(String),
}

#[derive(Clone)]
pub struct WbHub {
    // board_id → diffuseur de trames (updates binaires + awareness texte)
    rooms: Arc<RwLock<HashMap<Uuid, broadcast::Sender<Frame>>>>,
}

impl WbHub {
    pub fn new() -> Self {
        WbHub { rooms: Arc::new(RwLock::new(HashMap::new())) }
    }

    async fn get_or_create(&self, board_id: Uuid) -> broadcast::Receiver<Frame> {
        {
            let r = self.rooms.read().await;
            if let Some(tx) = r.get(&board_id) {
                return tx.subscribe();
            }
        }
        let mut w = self.rooms.write().await;
        let tx = w.entry(board_id).or_insert_with(|| broadcast::channel(512).0);
        tx.subscribe()
    }

    async fn broadcast(&self, board_id: Uuid, frame: Frame) {
        let r = self.rooms.read().await;
        if let Some(tx) = r.get(&board_id) {
            let _ = tx.send(frame);
        }
    }
}

// Partager le hub dans AppState n'est pas possible sans changer la struct.
// On utilise un lazy static à la place.
use std::sync::OnceLock;

static WB_HUB: OnceLock<WbHub> = OnceLock::new();

pub fn get_hub() -> &'static WbHub {
    WB_HUB.get_or_init(WbHub::new)
}

// ── Handler WebSocket ─────────────────────────────────────────────────────────

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(board_id): Path<Uuid>,
) -> Response {
    ws.on_upgrade(move |socket| handle_ws(socket, state, user, board_id))
}

async fn handle_ws(socket: WebSocket, state: AppState, user: OfficeUser, board_id: Uuid) {
    let hub = get_hub();
    let mut rx = hub.get_or_create(board_id).await;
    let (mut sender, mut receiver) = socket.split();

    // Envoyer les données Yjs existantes (snapshot + updates)
    let parts = match YjsService::load_document(&state, board_id).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "Erreur chargement Yjs pour board {board_id}");
            return;
        }
    };

    for part in parts {
        if sender.send(Message::Binary(part.into())).await.is_err() {
            return;
        }
    }

    // Écouter les messages entrants + broadcaster
    let db   = state.db.clone();
    let hub2 = hub;

    loop {
        tokio::select! {
            // Message entrant du client
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        let data = data.to_vec();
                        // Sauvegarder en base
                        if let Err(e) = YjsService::save_update(
                            &state,
                            board_id,
                            data.clone(),
                            Some(user.id.to_string()),
                        ).await {
                            tracing::error!(error = %e, "Erreur sauvegarde update Yjs");
                        }
                        // Broadcaster aux autres clients
                        hub2.broadcast(board_id, Frame::Bin(data)).await;

                        // Mettre à jour last_edited_at
                        let _ = sqlx::query(
                            "UPDATE office_wb.boards SET last_edited_at = NOW(), last_edited_by = $2 WHERE id = $1",
                        )
                        .bind(board_id)
                        .bind(user.id)
                        .execute(&db)
                        .await;
                    }
                    Some(Ok(Message::Text(txt))) => {
                        // Messages awareness (JSON) — relayés en TEXTE (non persistés).
                        hub2.broadcast(board_id, Frame::Txt(txt.to_string())).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }

            // Trame broadcastée par un autre client (update binaire ou awareness texte)
            Ok(frame) = rx.recv() => {
                let out = match frame {
                    Frame::Bin(d) => Message::Binary(d.into()),
                    Frame::Txt(t) => Message::Text(t.into()),
                };
                if sender.send(out).await.is_err() {
                    break;
                }
            }
        }
    }
}
