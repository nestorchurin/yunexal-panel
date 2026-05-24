use axum::{
    extract::{ConnectInfo, Form, Path, Query, State},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
        HeaderMap, HeaderValue,
    },
    response::IntoResponse,
    Extension, Json,
};
use crate::docker::{self, ContainerInfo};
use crate::{auth, db};
use axum_extra::extract::cookie::PrivateCookieJar;
use crate::state::AppState;
use std::net::SocketAddr;
use tracing::error;
use super::CspNonce;
use super::templates::{
    render, ConsoleTemplate, FilesTemplate, RenameServerForm, ServerAuditTemplate, ServerCardTemplate,
    ServerUsersTemplate, SettingsTemplate,
};

/// Resolves SQLite server id → (Docker container_id, display_name).
async fn resolve_server(state: &crate::state::AppState, db_id: i64) -> Result<(String, String), String> {
    match db::get_server_info_by_db_id(&state.db, db_id).await {
        Ok(Some((cid, name))) => Ok((cid, name)),
        Ok(None) => Err(format!("Server {} not found", db_id)),
        Err(e) => Err(format!("DB error: {}", e)),
    }
}

fn err_container(docker_id: String, db_id: i64) -> ContainerInfo {
    ContainerInfo {
        id: docker_id,
        short_id: "error".into(),
        name: "Error".into(),
        status: "Error".into(),
        state: "unknown".into(),
        cpu_usage: "-".into(),
        ram_usage: "-".into(),
        db_id,
        owner: String::new(),
    }
}

fn sanitize_audit_log_field(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

// ── Page handlers ─────────────────────────────────────────────────────────────

pub async fn console_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Extension(CspNonce(nonce)): Extension<CspNonce>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "console", false).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let can_power = auth::can_access_server_permission(&state, &jar, db_id, "power", true).await;
    let can_members = auth::can_access_server_permission(&state, &jar, db_id, "members", false).await;
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v, Err(e) => return e.into_response(),
    };
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => {
            c.db_id = db_id;
            c.name = db_name;
            render(ConsoleTemplate {
                id: db_id,
                container: c,
                can_power,
                can_members,
                active_tab: "console",
                nonce,
            })
            .into_response()
        }
        Err(e) => format!("Error: {}", e).into_response(),
    }
}

pub async fn server_users_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Extension(CspNonce(nonce)): Extension<CspNonce>,
) -> impl IntoResponse {
    let can_members = auth::can_access_server_permission(&state, &jar, db_id, "members", false).await;
    let can_members_write = auth::can_access_server_permission(&state, &jar, db_id, "members", true).await;
    if !can_members {
        return (
            axum::http::StatusCode::FORBIDDEN,
            "You do not have access to the Users page for this server.",
        )
            .into_response();
    }
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => {
            c.db_id = db_id;
            c.name = db_name;
            render(ServerUsersTemplate {
                id: db_id,
                container: c,
                can_members,
                can_members_write,
                active_tab: "users",
                nonce,
            })
            .into_response()
        }
        Err(e) => format!("Error: {}", e).into_response(),
    }
}

pub async fn files_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Extension(CspNonce(nonce)): Extension<CspNonce>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "files", false).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let can_members = auth::can_access_server_permission(&state, &jar, db_id, "members", false).await;
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v, Err(e) => return e.into_response(),
    };
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => { c.db_id = db_id; c.name = db_name; render(FilesTemplate { id: db_id, container: c, can_members, active_tab: "files", nonce }).into_response() }
        Err(e) => format!("Error: {}", e).into_response(),
    }
}

pub async fn settings_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Extension(CspNonce(nonce)): Extension<CspNonce>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "settings", false).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let is_admin = auth::is_admin_session(&state, &jar).await;
    let can_members = auth::can_access_server_permission(&state, &jar, db_id, "members", false).await;
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v, Err(e) => return e.into_response(),
    };
    let env = docker::inspect_full(&state.docker, &docker_id).await
        .map(|c| c.env)
        .unwrap_or_default();
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => { c.db_id = db_id; c.name = db_name; render(SettingsTemplate { id: db_id, container: c, is_admin, can_members, active_tab: "settings", nonce, env }).into_response() }
        Err(e) => format!("Error: {}", e).into_response(),
    }
}

pub async fn server_audit_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Extension(CspNonce(nonce)): Extension<CspNonce>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "audit", false).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let can_members = auth::can_access_server_permission(&state, &jar, db_id, "members", false).await;
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => {
            c.db_id = db_id;
            c.name = db_name;
            render(ServerAuditTemplate {
                id: db_id,
                container: c,
                can_members,
                active_tab: "audit",
                nonce,
            })
            .into_response()
        }
        Err(e) => format!("Error: {}", e).into_response(),
    }
}

#[derive(serde::Deserialize)]
pub struct ServerAuditQuery {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub action: Option<String>,
    pub actor: Option<String>,
    pub search: Option<String>,
}

pub async fn api_server_audit_list(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Query(q): Query<ServerAuditQuery>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "audit", false).await {
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        )
            .into_response();
    }

    match db::get_container_id_by_server_id(&state.db, db_id).await {
        Ok(Some(_)) => {}
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Server not found"})),
            )
                .into_response();
        }
    }

    let limit = q.limit.unwrap_or(50).min(200).max(1);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * limit;
    let action = q.action.as_deref().unwrap_or("");
    let actor = q.actor.as_deref().unwrap_or("");
    let search = q.search.as_deref().unwrap_or("");

    let total = db::audit_count_for_server(&state.db, db_id, action, actor, search)
        .await
        .unwrap_or(0);
    let entries = db::audit_list_for_server(&state.db, db_id, limit, offset, action, actor, search)
        .await
        .unwrap_or_default();
    let show_ip = auth::session_has_permission(&state, &jar, "audit.view_ip").await;
    let entries_json: Vec<serde_json::Value> = entries.into_iter().map(|e| {
        let mut v = serde_json::to_value(&e).unwrap_or_default();
        if !show_ip { v["ip"] = serde_json::Value::Null; }
        v
    }).collect();

    Json(serde_json::json!({
        "ok": true,
        "entries": entries_json,
        "total": total,
        "page": page,
        "pages": (total as f64 / limit as f64).ceil() as i64,
    }))
    .into_response()
}

pub async fn api_server_audit_download(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Query(q): Query<ServerAuditQuery>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "audit", false).await {
        return (
            axum::http::StatusCode::FORBIDDEN,
            "Access denied",
        )
            .into_response();
    }

    match db::get_container_id_by_server_id(&state.db, db_id).await {
        Ok(Some(_)) => {}
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                "Server not found",
            )
                .into_response();
        }
    }

    let action = q.action.as_deref().unwrap_or("");
    let actor = q.actor.as_deref().unwrap_or("");
    let search = q.search.as_deref().unwrap_or("");

    let entries = match db::audit_list_all_for_server(&state.db, db_id, action, actor, search).await {
        Ok(v) => v,
        Err(e) => {
            error!("api_server_audit_download error: {}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to build audit export",
            )
                .into_response();
        }
    };

    let mut body = String::new();
    body.push_str(&format!("# Server audit export\n# server_id={}\n", db_id));

    if entries.is_empty() {
        body.push_str("# No audit entries found\n");
    } else {
        for e in entries {
            body.push_str(&format!(
                "[{created_at}] actor=\"{actor}\" ip=\"{ip}\" action=\"{action}\" target=\"{target}\" detail=\"{detail}\" ua=\"{ua}\"\n",
                created_at = sanitize_audit_log_field(&e.created_at),
                actor = sanitize_audit_log_field(&e.actor),
                ip = sanitize_audit_log_field(&e.ip),
                action = sanitize_audit_log_field(&e.action),
                target = sanitize_audit_log_field(&e.target),
                detail = sanitize_audit_log_field(&e.detail),
                ua = sanitize_audit_log_field(&e.user_agent),
            ));
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    let filename = format!("server-{}-audit.log", db_id);
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename)) {
        headers.insert(CONTENT_DISPOSITION, v);
    }

    (headers, body).into_response()
}

#[derive(serde::Deserialize)]
pub struct AddServerMemberBody {
    #[serde(default)]
    pub uid: String,
}

#[derive(serde::Deserialize)]
pub struct SetServerMemberPermissionBody {
    pub permission: String,
    pub mode: String,
}

pub async fn api_server_members_list(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    let can_write = auth::can_access_server_permission(&state, &jar, db_id, "members", true).await;
    if !auth::can_access_server_permission(&state, &jar, db_id, "members", false).await {
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(serde_json::json!({"ok": false, "error": "Access denied"})),
        )
            .into_response();
    }

    let owner_id = match db::get_server_owner_by_db_id(&state.db, db_id).await {
        Ok(Some(v)) => v,
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"ok": false, "error": "Server not found"})),
            )
                .into_response();
        }
    };

    let owner_user = db::find_user_by_id(&state.db, owner_id)
        .await
        .ok()
        .flatten();
    let owner_username = owner_user
        .as_ref()
        .map(|u| u.username.clone())
        .unwrap_or_else(|| "owner".to_string());
    let owner_uid = owner_user
        .as_ref()
        .map(|u| u.uid.clone())
        .unwrap_or_else(|| format!("#{}", owner_id));
    let owner_nickname = owner_user
        .as_ref()
        .map(|u| u.nickname.clone())
        .unwrap_or_else(|| owner_username.clone());

    let mut members_map: std::collections::HashMap<i64, (String, String, String, serde_json::Map<String, serde_json::Value>)> =
        std::collections::HashMap::new();

    let mut owner_perms = serde_json::Map::new();
    for p in db::SERVER_MEMBER_PERMISSIONS {
        owner_perms.insert((*p).to_string(), serde_json::json!("write"));
    }
    members_map.insert(owner_id, (owner_username, owner_uid, owner_nickname, owner_perms));

    let rows = db::list_server_member_permissions(&state.db, db_id)
        .await
        .unwrap_or_default();

    for r in rows {
        let e = members_map.entry(r.user_id).or_insert_with(|| {
            let mut m = serde_json::Map::new();
            for p in db::SERVER_MEMBER_PERMISSIONS {
                m.insert((*p).to_string(), serde_json::json!("none"));
            }
            (r.username.clone(), r.uid.clone(), r.nickname.clone(), m)
        });
        if e.0.is_empty() {
            e.0 = r.username.clone();
        }
        if e.1.is_empty() {
            e.1 = r.uid.clone();
        }
        if e.2.is_empty() {
            e.2 = r.nickname.clone();
        }
        e.3.insert(r.permission, serde_json::json!(r.mode));
    }

    let mut members: Vec<serde_json::Value> = members_map
        .into_iter()
        .map(|(user_id, (username, uid, nickname, permissions))| {
            let nick = if nickname.trim().is_empty() {
                username.clone()
            } else {
                nickname.clone()
            };
            let display_name = if uid.trim().is_empty() {
                nick.clone()
            } else {
                format!("{} {}", nick, uid)
            };
            serde_json::json!({
                "user_id": user_id,
                "username": username,
                "uid": uid,
                "nickname": nick,
                "display_name": display_name,
                "is_owner": user_id == owner_id,
                "permissions": permissions,
            })
        })
        .collect();

    members.sort_by(|a, b| {
        let an = a.get("display_name").and_then(|v| v.as_str()).unwrap_or("");
        let bn = b.get("display_name").and_then(|v| v.as_str()).unwrap_or("");
        an.cmp(bn)
    });

    let existing_ids: std::collections::HashSet<i64> = members
        .iter()
        .filter_map(|m| m.get("user_id").and_then(|v| v.as_i64()))
        .collect();

    let users = db::list_users(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|u| !existing_ids.contains(&u.id))
        .map(|u| {
            let nick = if u.nickname.trim().is_empty() { u.username.clone() } else { u.nickname.clone() };
            let display_name = if u.uid.trim().is_empty() { nick.clone() } else { format!("{} {}", nick, u.uid) };
            serde_json::json!({
                "id": u.id,
                "username": u.username,
                "uid": u.uid,
                "nickname": nick,
                "display_name": display_name,
            })
        })
        .collect::<Vec<_>>();

    Json(serde_json::json!({
        "ok": true,
        "can_write": can_write,
        "permissions": db::SERVER_MEMBER_PERMISSIONS,
        "members": members,
        "users": users,
    }))
    .into_response()
}

pub async fn api_server_member_add(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
    Json(body): Json<AddServerMemberBody>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::can_access_server_permission(&state, &jar, db_id, "members", true).await {
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(serde_json::json!({"ok": false, "error": "Access denied"})),
        )
            .into_response();
    }

    let owner_id = match db::get_server_owner_by_db_id(&state.db, db_id).await {
        Ok(Some(v)) => v,
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"ok": false, "error": "Server not found"})),
            )
                .into_response();
        }
    };

    let uid = body.uid.trim();
    if uid.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "error": "uid is required"})),
        )
            .into_response();
    }

    let user = match db::find_user_by_uid(&state.db, uid).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"ok": false, "error": "User not found"})),
            )
                .into_response();
        }
    };

    if user.id == owner_id {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "error": "Owner already has full access"})),
        )
            .into_response();
    }

    if let Err(e) = db::add_server_member_with_defaults(&state.db, db_id, user.id).await {
        error!("api_server_member_add: {}", e);
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"ok": false, "error": "Failed to add member"})),
        )
            .into_response();
    }

    let actor = auth::session_username(&jar).unwrap_or_default();
    let _ = db::audit_log(
        &state.db,
        &actor,
        "server.member_add",
        &format!("{} {}", user.nickname, user.uid),
        &format!("#{}", db_id),
        &ip,
        &auth::user_agent(&headers),
    )
    .await;

    Json(serde_json::json!({"ok": true})).into_response()
}

pub async fn api_server_member_set_permission(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path((db_id, user_id)): Path<(i64, i64)>,
    Json(body): Json<SetServerMemberPermissionBody>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::can_access_server_permission(&state, &jar, db_id, "members", true).await {
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(serde_json::json!({"ok": false, "error": "Access denied"})),
        )
            .into_response();
    }

    let owner_id = match db::get_server_owner_by_db_id(&state.db, db_id).await {
        Ok(Some(v)) => v,
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"ok": false, "error": "Server not found"})),
            )
                .into_response();
        }
    };
    if user_id == owner_id {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "error": "Owner permissions are immutable"})),
        )
            .into_response();
    }

    if !db::server_member_exists(&state.db, db_id, user_id)
        .await
        .unwrap_or(false)
    {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"ok": false, "error": "Member not found"})),
        )
            .into_response();
    }

    if let Err(e) = db::set_server_member_permission_policy(
        &state.db,
        db_id,
        user_id,
        body.permission.trim(),
        body.mode.trim(),
    )
    .await
    {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "error": e.to_string()})),
        )
            .into_response();
    }

    let actor = auth::session_username(&jar).unwrap_or_default();
    let _ = db::audit_log(
        &state.db,
        &actor,
        "server.member_perm",
        &format!("{}:{}", user_id, body.permission),
        &format!("#{} mode={}", db_id, body.mode),
        &ip,
        &auth::user_agent(&headers),
    )
    .await;

    Json(serde_json::json!({"ok": true})).into_response()
}

pub async fn api_server_member_remove(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path((db_id, user_id)): Path<(i64, i64)>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::can_access_server_permission(&state, &jar, db_id, "members", true).await {
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(serde_json::json!({"ok": false, "error": "Access denied"})),
        )
            .into_response();
    }

    let owner_id = match db::get_server_owner_by_db_id(&state.db, db_id).await {
        Ok(Some(v)) => v,
        _ => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"ok": false, "error": "Server not found"})),
            )
                .into_response();
        }
    };
    if user_id == owner_id {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "error": "Cannot remove owner"})),
        )
            .into_response();
    }

    if let Err(e) = db::remove_server_member(&state.db, db_id, user_id).await {
        error!("api_server_member_remove: {}", e);
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"ok": false, "error": "Failed to remove member"})),
        )
            .into_response();
    }

    let actor = auth::session_username(&jar).unwrap_or_default();
    let _ = db::audit_log(
        &state.db,
        &actor,
        "server.member_remove",
        &format!("{}", user_id),
        &format!("#{}", db_id),
        &ip,
        &auth::user_agent(&headers),
    )
    .await;

    Json(serde_json::json!({"ok": true})).into_response()
}

// ── ENV update (settings page) ────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct UpdateEnvBody {
    pub env: String,
}

pub async fn api_update_env(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
    Json(body): Json<UpdateEnvBody>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::can_access_server_permission(&state, &jar, db_id, "settings", true).await {
        return (axum::http::StatusCode::FORBIDDEN, axum::Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v, Err(e) => return (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"error": e}))).into_response(),
    };
    let old_cfg = match docker::inspect_full(&state.docker, &docker_id).await {
        Ok(c) => c,
        Err(e) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    let docker_name = match docker::get_container(&state.docker, &docker_id).await {
        Ok(c) => c.name,
        Err(_) => docker_id.clone(),
    };
    let owner_id = db::get_server_owner(&state.db, &docker_id).await.ok().flatten().unwrap_or(0);
    let was_running = old_cfg.state == "running";

    let new_id = match docker::recreate_with_updated_config(
        &state.docker, &docker_id, &old_cfg.image, &body.env,
        &old_cfg.ports, old_cfg.cpu, old_cfg.memory_mb, &docker_name,
    ).await {
        Ok(id) => id,
        Err(e) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };

    if let Err(e) = db::update_server(&state.db, &docker_id, &new_id, &db_name, owner_id).await {
        error!("api_update_env update_server: {}", e);
    }
    if was_running {
        if let Err(e) = docker::start_container(&state.docker, &new_id).await {
            error!("api_update_env start: {}", e);
        } else {
            docker::reapply_bandwidth_limit(&state.docker, &new_id).await;
            docker::reapply_isolation_rules(&state.docker, &new_id).await;
        }
    }
    let actor = auth::session_username(&jar).unwrap_or_default();
    let _ = db::audit_log(&state.db, &actor, "server.env_update", &db_name, &format!("#{}", db_id), &ip, &auth::user_agent(&headers)).await;
    axum::Json(serde_json::json!({"ok": true})).into_response()
}

// ── Action handlers ───────────────────────────────────────────────────────────

pub async fn start_server(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::can_access_server_permission(&state, &jar, db_id, "power", true).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let is_admin = auth::is_admin_session(&state, &jar).await;
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v, Err(e) => return e.into_response(),
    };
    if let Err(e) = docker::start_container(&state.docker, &docker_id).await {
        error!("Failed to start container {}: {}", docker_id, e);
    } else {
        docker::reapply_bandwidth_limit(&state.docker, &docker_id).await;
        docker::reapply_isolation_rules(&state.docker, &docker_id).await;
        let actor = auth::session_username(&jar).unwrap_or_default();
        let _ = db::audit_log(&state.db, &actor, "server.start", &db_name, &format!("#{}", db_id), &ip, &auth::user_agent(&headers)).await;
    }
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => { c.db_id = db_id; c.name = db_name.clone(); render(ServerCardTemplate { container: c, is_admin }).into_response() }
        Err(e) => { error!("Failed to get container info {}: {}", docker_id, e); let mut ec = err_container(docker_id, db_id); ec.name = db_name; render(ServerCardTemplate { container: ec, is_admin }).into_response() }
    }
}

pub async fn stop_server(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::can_access_server_permission(&state, &jar, db_id, "power", true).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let is_admin = auth::is_admin_session(&state, &jar).await;
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v, Err(e) => return e.into_response(),
    };
    if let Err(e) = docker::stop_container(&state.docker, &docker_id).await {
        error!("Failed to stop container {}: {}", docker_id, e);
    } else {
        let actor = auth::session_username(&jar).unwrap_or_default();
        let _ = db::audit_log(&state.db, &actor, "server.stop", &db_name, &format!("#{}", db_id), &ip, &auth::user_agent(&headers)).await;
    }
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => { c.db_id = db_id; c.name = db_name.clone(); render(ServerCardTemplate { container: c, is_admin }).into_response() }
        Err(e) => { error!("Failed to get container info {}: {}", docker_id, e); let mut ec = err_container(docker_id, db_id); ec.name = db_name; render(ServerCardTemplate { container: ec, is_admin }).into_response() }
    }
}

pub async fn restart_server(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::can_access_server_permission(&state, &jar, db_id, "power", true).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let is_admin = auth::is_admin_session(&state, &jar).await;
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v, Err(e) => return e.into_response(),
    };
    let _ = docker::stop_container(&state.docker, &docker_id).await;
    if let Err(e) = docker::start_container(&state.docker, &docker_id).await {
        return format!("Failed to restart: {}", e).into_response();
    }
    docker::reapply_bandwidth_limit(&state.docker, &docker_id).await;
    docker::reapply_isolation_rules(&state.docker, &docker_id).await;
    let actor = auth::session_username(&jar).unwrap_or_default();
    let _ = db::audit_log(&state.db, &actor, "server.restart", &db_name, &format!("#{}", db_id), &ip, &auth::user_agent(&headers)).await;
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => { c.db_id = db_id; c.name = db_name.clone(); render(ServerCardTemplate { container: c, is_admin }).into_response() }
        Err(_) => "Restarted".into_response(),
    }
}

pub async fn kill_server(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::can_access_server_permission(&state, &jar, db_id, "power", true).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let is_admin = auth::is_admin_session(&state, &jar).await;
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v, Err(e) => return e.into_response(),
    };
    if let Err(e) = docker::kill_container(&state.docker, &docker_id).await {
        return format!("Failed to kill: {}", e).into_response();
    }
    let actor = auth::session_username(&jar).unwrap_or_default();
    let _ = db::audit_log(&state.db, &actor, "server.kill", &db_name, &format!("#{}", db_id), &ip, &auth::user_agent(&headers)).await;
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => { c.db_id = db_id; c.name = db_name.clone(); render(ServerCardTemplate { container: c, is_admin }).into_response() }
        Err(_) => "Killed".into_response(),
    }
}

pub async fn rename_server(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
    Form(form): Form<RenameServerForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::can_access_server_permission(&state, &jar, db_id, "settings", true).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let is_admin = auth::is_admin_session(&state, &jar).await;
    let new_name = form.name.trim().to_string();
    if new_name.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "Name cannot be empty").into_response();
    }
    let (docker_id, _) = match resolve_server(&state, db_id).await {
        Ok(v) => v, Err(e) => return e.into_response(),
    };
    // Check name uniqueness (exclude current container)
    match db::server_name_exists(&state.db, &new_name, Some(&docker_id)).await {
        Ok(true) => return (axum::http::StatusCode::CONFLICT, "Name already taken").into_response(),
        Err(e) => error!("server_name_exists: {}", e),
        _ => {}
    }
    // Update name in SQLite only — Docker container name stays as internal identifier
    if let Err(e) = db::update_server_name_only(&state.db, &docker_id, &new_name).await {
        error!("rename_server db update: {}", e);
    } else {
        let actor = auth::session_username(&jar).unwrap_or_default();
        let _ = db::audit_log(&state.db, &actor, "server.rename", &new_name, &format!("#{}", db_id), &ip, &auth::user_agent(&headers)).await;
    }
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => { c.db_id = db_id; c.name = new_name; render(ServerCardTemplate { container: c, is_admin }).into_response() }
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e)).into_response(),
    }
}

pub async fn delete_server(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let hx_redir = [(axum::http::header::HeaderName::from_static("hx-redirect"), "/")];
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v,
        Err(_) => return hx_redir,
    };
    // Resolve volume dir before removing the container
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id)
        .await
        .unwrap_or_else(|_| docker_id.clone());
    // Stop and remove container first, then DB
    let _ = docker::stop_container(&state.docker, &docker_id).await;

    // Delete volume directory
    let volume_path = docker::volume_dir_to_path(&volume_dir);
    // Remove ext4 project quota entries before deleting the directory.
    docker::remove_ext4_quota(db_id as u32, &volume_path).await;
    if volume_path.exists() {
        let abs = volume_path.canonicalize().unwrap_or(volume_path.clone());
        let mount_arg = format!("{}:/target", abs.display());
        let status = tokio::process::Command::new("docker")
            .args(["run", "--rm", "-v", &mount_arg, "alpine", "sh", "-c", "rm -rf /target/*  /target/.[!.]* 2>/dev/null || true"])
            .status().await;
        if let Err(e) = status { error!("Failed to spawn docker cleanup for {}: {}", volume_dir, e); }
        if let Err(e) = tokio::fs::remove_dir_all(&volume_path).await {
            error!("Failed to delete volume directory {}: {}", volume_dir, e);
        }
    }

    // Clean up dedicated isolation network and iptables rules BEFORE removing
    // the container so that the `yunexal.network` label is still readable.
    docker::cleanup_isolation(&state.docker, &docker_id).await;

    if let Err(e) = docker::remove_container(&state.docker, &docker_id).await {
        error!("Failed to delete container {}: {}", docker_id, e);
    }

    // Remove DB record last
    if let Err(e) = db::delete_server_by_container_id(&state.db, &docker_id).await {
        error!("delete_server db: {}", e);
    }
    let actor = auth::session_username(&jar).unwrap_or_default();
    let _ = db::audit_log(&state.db, &actor, "server.delete", &db_name, &format!("#{}", db_id), &ip, &auth::user_agent(&headers)).await;
    hx_redir
}

// ── Factory Reset ────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct FactoryResetBody {
    pub password: String,
}

pub async fn api_factory_reset(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
    Json(body): Json<FactoryResetBody>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::can_access_server_permission(&state, &jar, db_id, "settings", true).await {
        return (axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Access denied"}))).into_response();
    }
    // Verify password
    let username = match auth::session_username(&jar) {
        Some(u) => u,
        None => return (axum::http::StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Not authenticated"}))).into_response(),
    };
    let ok = match db::find_user_by_username(&state.db, &username).await {
        Ok(Some(user)) => crate::password::verify(&body.password, &user.password_hash),
        _ => false,
    };
    if !ok {
        let _ = db::audit_log(&state.db, &username, "server.factory_reset_failed", &format!("#{}", db_id), "wrong password", &ip, &auth::user_agent(&headers)).await;
        return (axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Incorrect password"}))).into_response();
    }

    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v,
        Err(e) => return (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    };

    // Get volume before stopping
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id)
        .await
        .unwrap_or_else(|_| docker_id.clone());

    // Stop container
    let _ = docker::stop_container(&state.docker, &docker_id).await;

    // Wipe volume contents (keep directory)
    let volume_path = docker::volume_dir_to_path(&volume_dir);
    if volume_path.exists() {
        let abs = volume_path.canonicalize().unwrap_or(volume_path.clone());
        let mount_arg = format!("{}:/target", abs.display());
        let status = tokio::process::Command::new("docker")
            .args(["run", "--rm", "-v", &mount_arg, "alpine", "sh", "-c", "rm -rf /target/* /target/.[!.]* 2>/dev/null || true"])
            .status().await;
        if let Err(e) = status { error!("factory_reset cleanup for {}: {}", volume_dir, e); }
    }

    // Start container again
    let _ = docker::start_container(&state.docker, &docker_id).await;

    let _ = db::audit_log(&state.db, &username, "server.factory_reset", &db_name, &format!("#{}", db_id), &ip, &auth::user_agent(&headers)).await;
    Json(serde_json::json!({"ok": true, "message": "Server reset to factory defaults"})).into_response()
}

// ── Stats ────────────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct ServerStatsResponse {
    pub state: String,
    pub status: String,
    pub cpu: f64,
    pub ram: u64,
    pub ram_limit: u64,
    pub rx: u64,
    pub tx: u64,
    pub blk_read: u64,
    pub blk_write: u64,
}

macro_rules! err_stats {
    ($state:expr, $status:expr) => {
        Json(ServerStatsResponse { state: $state.into(), status: $status.into(), cpu: 0.0, ram: 0, ram_limit: 0, rx: 0, tx: 0, blk_read: 0, blk_write: 0 }).into_response()
    };
}

pub async fn get_server_stats(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    if !auth::can_access_server_permission(&state, &jar, db_id, "console", false).await {
        return err_stats!("error", "Access denied");
    }
    let (docker_id, _) = match resolve_server(&state, db_id).await {
        Ok(v) => v,
        Err(_) => return err_stats!("error", "Error"),
    };
    let container = match docker::get_container(&state.docker, &docker_id).await {
        Ok(c) => c,
        Err(_) => return err_stats!("error", "Error"),
    };

    if container.state == "running" {
        match docker::get_container_stats_raw(&state.docker, &docker_id).await {
            Ok(s) => Json(ServerStatsResponse {
                state: container.state, status: container.status,
                cpu: s.cpu_usage, ram: s.ram_usage, ram_limit: s.ram_limit,
                rx: s.net_rx, tx: s.net_tx,
                blk_read: s.blk_read, blk_write: s.blk_write,
            }).into_response(),
            Err(_) => err_stats!(container.state, container.status),
        }
    } else {
        err_stats!(container.state, container.status)
    }
}

