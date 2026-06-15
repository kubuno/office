//! Handler WebSocket de collaboration temps réel GÉNÉRIQUE (Yjs) pour les éditeurs
//! Office. Relaie des updates Yjs binaires (et de l'awareness texte) entre clients,
//! persiste l'état via `CollabService`. Indépendant du type d'éditeur :
//! route `/collab/:entity_type/:entity_id/sync`.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::Response,
    Extension,
};
use futures::{sink::SinkExt, stream::StreamExt};
use std::{collections::HashMap, sync::Arc, sync::OnceLock};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::{middleware::OfficeUser, services::collab::CollabService, state::AppState};

/// Trame relayée : update Yjs binaire ou message d'awareness texte.
#[derive(Clone)]
enum Frame {
    Bin(Vec<u8>),
    Txt(String),
}

#[derive(Clone)]
pub struct CollabHub {
    // clé "entity_type:entity_id" → diffuseur
    rooms: Arc<RwLock<HashMap<String, broadcast::Sender<Frame>>>>,
}

impl CollabHub {
    fn new() -> Self { CollabHub { rooms: Arc::new(RwLock::new(HashMap::new())) } }

    async fn subscribe(&self, key: &str) -> broadcast::Receiver<Frame> {
        {
            let r = self.rooms.read().await;
            if let Some(tx) = r.get(key) { return tx.subscribe(); }
        }
        let mut w = self.rooms.write().await;
        let tx = w.entry(key.to_string()).or_insert_with(|| broadcast::channel(512).0);
        tx.subscribe()
    }

    async fn broadcast(&self, key: &str, frame: Frame) {
        let r = self.rooms.read().await;
        if let Some(tx) = r.get(key) { let _ = tx.send(frame); }
    }
}

static COLLAB_HUB: OnceLock<CollabHub> = OnceLock::new();
fn hub() -> &'static CollabHub { COLLAB_HUB.get_or_init(CollabHub::new) }

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((entity_type, entity_id)): Path<(String, Uuid)>,
) -> Response {
    ws.on_upgrade(move |socket| handle(socket, state, user, entity_type, entity_id))
}

async fn handle(socket: WebSocket, state: AppState, user: OfficeUser, entity_type: String, entity_id: Uuid) {
    // Auth : l'utilisateur doit posséder l'entité (collaborateurs : à venir).
    match CollabService::entity_owner(&state, &entity_type, entity_id).await {
        Ok(Some(owner)) if owner == user.id => {}
        _ => return,
    }

    let key = format!("{entity_type}:{entity_id}");
    let mut rx = hub().subscribe(&key).await;
    let (mut sender, mut receiver) = socket.split();

    // Sync initiale : snapshot + updates existants.
    match CollabService::load_document(&state, &entity_type, entity_id).await {
        Ok(parts) => {
            for part in parts {
                if sender.send(Message::Binary(part.into())).await.is_err() { return; }
            }
        }
        Err(e) => { tracing::error!(error = %e, "collab: chargement {entity_type}/{entity_id}"); return; }
    }

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        let data = data.to_vec();
                        if let Err(e) = CollabService::save_update(&state, &entity_type, entity_id, data.clone(), Some(user.id.to_string())).await {
                            tracing::error!(error = %e, "collab: save_update");
                        }
                        hub().broadcast(&key, Frame::Bin(data)).await;
                    }
                    Some(Ok(Message::Text(txt))) => {
                        // Awareness (curseurs/présence) : relais tel quel, non persisté.
                        hub().broadcast(&key, Frame::Txt(txt.to_string())).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            Ok(frame) = rx.recv() => {
                let out = match frame { Frame::Bin(d) => Message::Binary(d.into()), Frame::Txt(t) => Message::Text(t.into()) };
                if sender.send(out).await.is_err() { break; }
            }
        }
    }
}
