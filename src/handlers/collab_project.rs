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
    Path(project_id): Path<Uuid>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state, user.id, project_id))
}

async fn handle_socket(socket: WebSocket, state: AppState, user_id: Uuid, project_id: Uuid) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.project_hub.subscribe(project_id).await;

    state.project_hub.publish(project_id, crate::state::ProjectMessage::PresenceChange {
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
            match msg {
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }

    state.project_hub.publish(project_id, crate::state::ProjectMessage::PresenceChange {
        user_id,
        action: "leave".to_string(),
    }).await;
}
