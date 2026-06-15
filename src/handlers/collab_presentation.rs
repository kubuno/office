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
use crate::{middleware::OfficeUser, state::AppState};

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Extension(user): Extension<OfficeUser>,
    Path(pres_id): Path<Uuid>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state, user.id, pres_id))
}

async fn handle_socket(socket: WebSocket, state: AppState, user_id: Uuid, pres_id: Uuid) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.pres_hub.subscribe(pres_id).await;

    // Notify others of join
    state.pres_hub.publish(pres_id, crate::state::PresentationMessage::PresenceChange {
        user_id,
        action: "join".to_string(),
    }).await;

    // Task: forward hub messages to this WS client
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if let Ok(payload) = serde_json::to_string(&msg) {
                if sender.send(Message::Text(payload.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    // Task: receive from client (ping/keepalive only — content updates go via REST)
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Close(_) => break,
                Message::Ping(d) => {
                    let _ = d;
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }

    // Notify others of leave
    state.pres_hub.publish(pres_id, crate::state::PresentationMessage::PresenceChange {
        user_id,
        action: "leave".to_string(),
    }).await;
}
