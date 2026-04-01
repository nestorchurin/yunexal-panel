use axum::{
    extract::{ConnectInfo, Form, Path, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use crate::docker::{self, ContainerInfo};
use crate::{auth, db};
use crate::dns as dns_lib;
use serde_json::Value as JsonValue;
use axum_extra::extract::cookie::PrivateCookieJar;
use crate::state::AppState;
use std::net::SocketAddr;
use tracing::error;
use super::templates::{
    render, ConsoleTemplate, FilesTemplate, RenameServerForm, ServerCardTemplate, SettingsTemplate,
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

// ── Page handlers ─────────────────────────────────────────────────────────────

pub async fn console_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v, Err(e) => return e.into_response(),
    };
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => { c.db_id = db_id; c.name = db_name; render(ConsoleTemplate { id: db_id, container: c, active_tab: "console", cf_token: state.cf_analytics_token.clone() }).into_response() }
        Err(e) => format!("Error: {}", e).into_response(),
    }
}

pub async fn files_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v, Err(e) => return e.into_response(),
    };
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => { c.db_id = db_id; c.name = db_name; render(FilesTemplate { id: db_id, container: c, active_tab: "files", cf_token: state.cf_analytics_token.clone() }).into_response() }
        Err(e) => format!("Error: {}", e).into_response(),
    }
}

pub async fn settings_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let is_admin = auth::is_admin_session(&state, &jar).await;
    let (docker_id, db_name) = match resolve_server(&state, db_id).await {
        Ok(v) => v, Err(e) => return e.into_response(),
    };
    let env = docker::inspect_full(&state.docker, &docker_id).await
        .map(|c| c.env)
        .unwrap_or_default();
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => { c.db_id = db_id; c.name = db_name; render(SettingsTemplate { id: db_id, container: c, is_admin, active_tab: "settings", cf_token: state.cf_analytics_token.clone(), env }).into_response() }
        Err(e) => format!("Error: {}", e).into_response(),
    }
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
    if !auth::can_access_server(&state, &jar, db_id).await {
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
    if !auth::can_access_server(&state, &jar, db_id).await {
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
    if !auth::can_access_server(&state, &jar, db_id).await {
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
    if !auth::can_access_server(&state, &jar, db_id).await {
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
    if !auth::can_access_server(&state, &jar, db_id).await {
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
    if !auth::can_access_server(&state, &jar, db_id).await {
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
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);
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

    // ── Delete linked DNS records ──────────────────────────────────────────
    // Best-effort: delete from provider API then from local DB
    if let Ok(dns_recs) = db::dns_list_records_by_server_id(&state.db, db_id).await {
        for rec in &dns_recs {
            if rec.remote_id.is_empty() { continue; }
            if let Ok(Some(provider)) = db::dns_get_provider(&state.db, rec.provider_id).await {
                let creds: JsonValue = serde_json::from_str(&provider.credentials)
                    .unwrap_or(JsonValue::Object(Default::default()));
                if let Ok(client) = dns_lib::DnsClient::from_type(&provider.provider_type, &creds) {
                    let _ = client.delete_record(&rec.zone_id, &rec.remote_id).await;
                }
            }
        }
        let _ = db::dns_delete_records_by_server_id(&state.db, db_id).await;
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
    if !auth::can_access_server(&state, &jar, db_id).await {
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
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);
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
    if !auth::can_access_server(&state, &jar, db_id).await {
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

