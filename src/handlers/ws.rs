use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use futures_util::{SinkExt, StreamExt};
use tokio::io::AsyncWriteExt;
use crate::{auth, db, docker};
use crate::state::AppState;

pub async fn console_ws(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    ws: WebSocketUpgrade,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };
    ws.on_upgrade(move |socket| handle_console_socket(socket, state, docker_id))
}

async fn handle_console_socket(socket: WebSocket, state: AppState, id: String) {
    let (mut sender, mut receiver) = socket.split();

    match docker::attach_container(&state.docker, &id).await {
        Ok((mut stream, mut sink)) => {
            let mut send_task = tokio::spawn(async move {
                while let Some(msg) = stream.next().await {
                    if let Ok(output) = msg {
                        let text = output.to_string();
                        if sender.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                }
            });

            let mut recv_task = tokio::spawn(async move {
                while let Some(Ok(msg)) = receiver.next().await {
                    match msg {
                        Message::Text(text) => {
                            if sink.write_all(text.as_bytes()).await.is_err() {
                                break;
                            }
                            let _ = sink.flush().await;
                        }
                        Message::Binary(bytes) => {
                            if sink.write_all(&bytes).await.is_err() {
                                break;
                            }
                            let _ = sink.flush().await;
                        }
                        _ => {}
                    }
                }
            });

            tokio::select! {
                _ = (&mut send_task) => recv_task.abort(),
                _ = (&mut recv_task) => send_task.abort(),
            };
        }
        Err(e) => {
            let _ = sender
                .send(Message::Text(format!("Failed to attach: {}", e).into()))
                .await;
        }
    }
}
