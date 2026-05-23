use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use serde_json::json;
use crate::{auth, db, docker};
use crate::state::AppState;
use std::net::SocketAddr;
use tracing::error;
use super::CspNonce;
use super::templates::{render, IndexTemplate, NewServerTemplate, ServerListTemplate};

async fn user_is_admin(state: &AppState, jar: &PrivateCookieJar) -> bool {
    auth::is_admin_session(state, jar).await
}

pub async fn dashboard(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Extension(CspNonce(nonce)): Extension<CspNonce>,
) -> impl IntoResponse {
    let is_admin = user_is_admin(&state, &jar).await;
    let mut containers = match docker::list_containers(&state.docker).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to list containers: {}", e);
            vec![]
        }
    };
    if !is_admin {
        if let Some(uid) = auth::session_user_id(&state, &jar).await {
            let allowed = db::list_accessible_container_ids(&state.db, uid).await.unwrap_or_default();
            containers.retain(|c| allowed.iter().any(|oid| oid.starts_with(&c.id) || c.id.starts_with(oid.as_str())));
        } else {
            containers.clear();
        }
    }
    // Populate db_id and SQLite display name for each container
    let info_map = db::get_server_info_map(&state.db).await.unwrap_or_default();
    for c in &mut containers {
        if let Some((id, name, owner)) = info_map.get(&c.id) {
            c.db_id = *id;
            c.name = name.clone();
            c.owner = owner.clone();
        }
    }
    let auth_username = auth::session_username(&jar).unwrap_or_default();
    let auth_owner_label = db::find_user_by_username(&state.db, &auth_username)
        .await
        .ok()
        .flatten()
        .map(|u| {
            if u.nickname.trim().is_empty() {
                u.username
            } else {
                u.nickname
            }
        })
        .unwrap_or_else(|| auth_username.clone());
    render(IndexTemplate {
        containers,
        is_admin,
        auth_username,
        auth_owner_label,
        nonce,
    })
}

pub async fn server_list_fragment(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> impl IntoResponse {
    let is_admin = user_is_admin(&state, &jar).await;
    let mut containers = match docker::list_containers_fast(&state.docker).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to list containers: {}", e);
            vec![]
        }
    };
    if !is_admin {
        if let Some(uid) = auth::session_user_id(&state, &jar).await {
            let allowed = db::list_accessible_container_ids(&state.db, uid).await.unwrap_or_default();
            containers.retain(|c| allowed.iter().any(|oid| oid.starts_with(&c.id) || c.id.starts_with(oid.as_str())));
        } else {
            containers.clear();
        }
    }
    // Populate db_id and SQLite display name for each container
    let info_map = db::get_server_info_map(&state.db).await.unwrap_or_default();
    for c in &mut containers {
        if let Some((id, name, owner)) = info_map.get(&c.id) {
            c.db_id = *id;
            c.name = name.clone();
            c.owner = owner.clone();
        }
    }
    render(ServerListTemplate { containers, is_admin })
}

pub async fn api_dashboard_json(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> impl IntoResponse {
    if auth::session_username(&jar).is_none() {
        return Json(json!({"ok": false, "error": "unauthorized"})).into_response();
    }
    let is_admin = user_is_admin(&state, &jar).await;
    let mut containers = match docker::list_containers_fast(&state.docker).await {
        Ok(c) => c,
        Err(e) => { error!("Failed to list containers: {}", e); vec![] }
    };
    if !is_admin {
        if let Some(uid) = auth::session_user_id(&state, &jar).await {
            let allowed = db::list_accessible_container_ids(&state.db, uid).await.unwrap_or_default();
            containers.retain(|c| allowed.iter().any(|oid| oid.starts_with(&c.id) || c.id.starts_with(oid.as_str())));
        } else {
            containers.clear();
        }
    }
    let info_map = db::get_server_info_map(&state.db).await.unwrap_or_default();
    for c in &mut containers {
        if let Some((id, name, owner)) = info_map.get(&c.id) {
            c.db_id = *id;
            c.name = name.clone();
            c.owner = owner.clone();
        }
    }
    let items: Vec<_> = containers.iter().map(|c| json!({
        "db_id": c.db_id,
        "name": c.name,
        "owner": c.owner,
        "state": c.state,
        "status": c.status,
    })).collect();
    Json(json!({"ok": true, "is_admin": is_admin, "containers": items})).into_response()
}

#[derive(serde::Deserialize)]
pub struct LogoutDeviceBody {
    pub session_id: String,
}

/// GET /api/user/devices
/// Returns active authenticated device sessions for the current user.
pub async fn api_user_devices(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> impl IntoResponse {
    let user_id = match auth::session_user_id(&state, &jar).await {
        Some(v) => v,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"ok": false, "error": "Not authenticated"})),
            )
                .into_response();
        }
    };

    let current_session_id = auth::session_id(&jar).unwrap_or_default();
    let sessions = match db::list_user_sessions(&state.db, user_id, &state.db_key).await {
        Ok(v) => v,
        Err(e) => {
            error!("api_user_devices: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": "Failed to load devices"})),
            )
                .into_response();
        }
    };

    let devices: Vec<serde_json::Value> = sessions
        .into_iter()
        .map(|s| {
            json!({
                "session_id": s.session_id,
                "ip": s.ip,
                "user_agent": s.user_agent,
                "created_at": s.created_at,
                "last_seen_at": s.last_seen_at,
                "is_current": s.session_id == current_session_id,
            })
        })
        .collect();

    Json(json!({
        "ok": true,
        "current_session_id": current_session_id,
        "devices": devices,
    }))
    .into_response()
}

/// POST /api/user/devices/logout
/// Revokes one active session for the current user, excluding current device.
pub async fn api_user_logout_device(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<LogoutDeviceBody>,
) -> impl IntoResponse {
    let user_id = match auth::session_user_id(&state, &jar).await {
        Some(v) => v,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"ok": false, "error": "Not authenticated"})),
            )
                .into_response();
        }
    };

    let session_id = body.session_id.trim();
    if session_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "session_id is required"})),
        )
            .into_response();
    }

    if auth::session_id(&jar).as_deref() == Some(session_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "Cannot logout current device"})),
        )
            .into_response();
    }

    match db::revoke_user_session(&state.db, user_id, session_id).await {
        Ok(0) => (
            StatusCode::NOT_FOUND,
            Json(json!({"ok": false, "error": "Device session not found"})),
        )
            .into_response(),
        Ok(_) => {
            let actor = auth::session_username(&jar).unwrap_or_default();
            let ip = auth::client_ip(&headers, addr);
            let _ = db::audit_log(
                &state.db,
                &actor,
                "auth.device_logout",
                session_id,
                "settings.devices",
                &ip,
                &auth::user_agent(&headers),
            )
            .await;
            Json(json!({"ok": true})).into_response()
        }
        Err(e) => {
            error!("api_user_logout_device: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": "Failed to revoke device session"})),
            )
                .into_response()
        }
    }
}

pub async fn new_server_page(
    State(state): State<AppState>,
    Extension(CspNonce(nonce)): Extension<CspNonce>,
) -> impl IntoResponse {
    let users = db::list_users(&state.db).await.unwrap_or_default()
        .into_iter()
        .map(|u| super::templates::UserInfo {
            id: u.id,
            uid: u.uid,
            nickname: u.nickname,
            username: u.username,
            role: u.role,
            created_at: u.created_at,
        })
        .collect();
    let default_quota_gb = {
        let v = db::get_panel_setting(&state.db, "docker_default_quota").await;
        if v.is_empty() { "15".to_string() } else { v }
    };
    render(NewServerTemplate { error: None, fix_cmd: None, users, nonce, default_quota_gb })
}
