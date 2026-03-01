use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use serde_json::json;
use crate::{auth, db, docker};
use crate::state::AppState;
use tracing::error;
use super::templates::{render, IndexTemplate, NewServerTemplate, ServerListTemplate};

async fn user_is_admin(state: &AppState, jar: &PrivateCookieJar) -> bool {
    auth::is_admin_session(state, jar).await
}

pub async fn dashboard(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
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
            let owned = db::list_owned_container_ids(&state.db, uid).await.unwrap_or_default();
            containers.retain(|c| owned.iter().any(|oid| oid.starts_with(&c.id) || c.id.starts_with(oid.as_str())));
        } else {
            containers.clear();
        }
    }
    // Populate db_id and SQLite display name for each container
    let info_map = db::get_server_info_map(&state.db).await.unwrap_or_default();
    for c in &mut containers {
        if let Some((id, name, _owner)) = info_map.get(&c.id) {
            c.db_id = *id;
            c.name = name.clone();
        }
    }
    let auth_username = auth::session_username(&jar).unwrap_or_default();
    render(IndexTemplate { containers, is_admin, auth_username })
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
            let owned = db::list_owned_container_ids(&state.db, uid).await.unwrap_or_default();
            containers.retain(|c| owned.iter().any(|oid| oid.starts_with(&c.id) || c.id.starts_with(oid.as_str())));
        } else {
            containers.clear();
        }
    }
    // Populate db_id and SQLite display name for each container
    let info_map = db::get_server_info_map(&state.db).await.unwrap_or_default();
    for c in &mut containers {
        if let Some((id, name, _owner)) = info_map.get(&c.id) {
            c.db_id = *id;
            c.name = name.clone();
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
            let owned = db::list_owned_container_ids(&state.db, uid).await.unwrap_or_default();
            containers.retain(|c| owned.iter().any(|oid| oid.starts_with(&c.id) || c.id.starts_with(oid.as_str())));
        } else {
            containers.clear();
        }
    }
    let info_map = db::get_server_info_map(&state.db).await.unwrap_or_default();
    for c in &mut containers {
        if let Some((id, name, _owner)) = info_map.get(&c.id) {
            c.db_id = *id;
            c.name = name.clone();
        }
    }
    let items: Vec<_> = containers.iter().map(|c| json!({
        "db_id": c.db_id,
        "name": c.name,
        "state": c.state,
        "status": c.status,
    })).collect();
    Json(json!({"ok": true, "is_admin": is_admin, "containers": items})).into_response()
}

pub async fn new_server_page(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let users = db::list_users(&state.db).await.unwrap_or_default()
        .into_iter()
        .map(|u| super::templates::UserInfo {
            id: u.id,
            username: u.username,
            role: u.role,
            created_at: u.created_at,
        })
        .collect();
    render(NewServerTemplate { error: None, users })
}
