use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Path, State,
    },
    http::HeaderMap,
    response::IntoResponse,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::io::AsyncWriteExt;
use crate::{auth, db, docker};
use crate::state::AppState;

pub async fn console_ws(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    let actor = auth::session_username(&jar).unwrap_or_default();
    let ip = auth::client_ip(&headers, addr);
    let (docker_id, db_name) = match db::get_server_info_by_db_id(&state.db, db_id).await.ok().flatten() {
        Some(v) => v,
        None => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };
    let _ = db::audit_log(&state.db, &actor, "console.connect", &db_name, &format!("#{}", db_id), &ip, &auth::user_agent(&headers)).await;
    ws.on_upgrade(move |socket| handle_console_socket(socket, state, docker_id, actor, db_id, ip))
}

async fn handle_console_socket(socket: WebSocket, state: AppState, id: String, actor: String, db_id: i64, ip: String) {
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

            let db = state.db.clone();
            let mut recv_task = tokio::spawn(async move {
                while let Some(Ok(msg)) = receiver.next().await {
                    match msg {
                        Message::Text(text) => {
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                let short = if trimmed.len() > 200 { &trimmed[..200] } else { trimmed };
                                let _ = db::audit_log(&db, &actor, "console.command", short, &format!("#{}", db_id), &ip, "").await;
                            }
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
