use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use uuid::Uuid;
use axum::Extension;

use crate::{middleware::OfficeUser, state::{AppState, SheetMessage}};

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path((_ss_id, sheet_id)): Path<(Uuid, Uuid)>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state, user.id, sheet_id))
}

async fn handle_socket(socket: WebSocket, state: AppState, user_id: Uuid, sheet_id: Uuid) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.sheet_hub.subscribe(sheet_id).await;

    state.sheet_hub.publish(sheet_id, SheetMessage::PresenceChange {
        user_id,
        action: "join".to_string(),
    }).await;

    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if let Ok(payload) = serde_json::to_string(&msg) {
                if sender.send(Message::Text(payload.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if matches!(msg, Message::Close(_)) { break; }
        }
    });

    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }

    state.sheet_hub.publish(sheet_id, SheetMessage::PresenceChange {
        user_id,
        action: "leave".to_string(),
    }).await;
}
