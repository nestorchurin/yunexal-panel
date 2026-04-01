use axum::{
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use serde::Deserialize;
use crate::{auth, db, docker};
use crate::state::AppState;
use std::net::SocketAddr;
use tracing::error;
use super::templates::{render, NetworkingTemplate, PortRow};

pub async fn networking_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }
    let is_admin = auth::is_admin_session(&state, &jar).await;
    let (docker_id, db_name) = match db::get_server_info_by_db_id(&state.db, db_id).await.ok().flatten() {
        Some(row) => row,
        None => return "Server not found".into_response(),
    };
    match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut container) => {
            container.db_id = db_id;
            container.name = db_name;
            let bandwidth_mbit = docker::get_bandwidth_limit(&state.docker, &docker_id)
                .await
                .unwrap_or(None);
            // Ports: union of Docker bindings (enabled=true) + DB disabled entries
            let docker_ports: std::collections::HashSet<(u16, u16)> = docker::get_port_bindings(&state.docker, &docker_id)
                .await
                .unwrap_or_default()
                .into_iter()
                .collect();
            let db_ports = db::get_port_tags(&state.db, db_id).await.unwrap_or_default();
            let mut all_ports: std::collections::BTreeMap<(u16, u16), (String, bool, bool)> = std::collections::BTreeMap::new();
            // DB entries first (include disabled ones)
            for ((hp, cp), (tag, enabled, ufw_blocked)) in &db_ports {
                all_ports.insert((*hp as u16, *cp as u16), (tag.clone(), *enabled, *ufw_blocked));
            }
            // Docker ports not yet tracked in DB are enabled by definition
            for (hp, cp) in &docker_ports {
                all_ports.entry((*hp, *cp)).or_insert_with(|| (String::new(), true, false));
            }
            let ufw_enabled = db::get_panel_setting_bool(&state.db, "ufw_enabled").await;
            let bandwidth_enabled = db::get_panel_setting_bool(&state.db, "bandwidth_enabled").await;
            let ports = all_ports
                .into_iter()
                .map(|((hp, cp), (tag, enabled, ufw_blocked))| PortRow { host_port: hp, container_port: cp, tag, enabled, ufw_blocked })
                .collect();
            render(NetworkingTemplate { id: db_id, container, bandwidth_mbit, is_admin, ports, active_tab: "networking", cf_token: state.cf_analytics_token.clone(), ufw_enabled, bandwidth_enabled }).into_response()
        }
        Err(e) => format!("Error: {}", e).into_response(),
    }
}

#[derive(Deserialize)]
pub struct SetBandwidthBody {
    /// Mbit/s limit; null or omitted means unlimited.
    pub mbit: Option<u32>,
}

pub async fn api_get_bandwidth(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"not found"}))).into_response(),
    };
    match docker::get_bandwidth_limit(&state.docker, &docker_id).await {
        Ok(limit) => Json(serde_json::json!({ "mbit": limit })).into_response(),
        Err(e) => {
            error!("get_bandwidth {}: {}", docker_id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

pub async fn api_set_bandwidth(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
    Json(body): Json<SetBandwidthBody>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::is_admin_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Only admins can change bandwidth limits." }))).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"not found"}))).into_response(),
    };
    match docker::set_bandwidth_limit(&state.docker, &docker_id, body.mbit).await {
        Ok(_) => {
            let actor = auth::session_username(&jar).unwrap_or_default();
            let detail = body.mbit.map(|m| format!("{}Mbit", m)).unwrap_or_else(|| "unlimited".into());
            let _ = db::audit_log(&state.db, &actor, "net.bandwidth", &format!("#{}", db_id), &detail, &ip, &auth::user_agent(&headers)).await;
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(e) => {
            error!("set_bandwidth {}: {}", docker_id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

// ── Port management ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AddPortBody {
    pub host_port: u16,
    pub container_port: u16,
    #[serde(default)]
    pub tag: String,
}

#[derive(Deserialize)]
pub struct RemovePortBody {
    pub host_port: u16,
    pub container_port: u16,
}

#[derive(Deserialize)]
pub struct TagPortBody {
    pub host_port: u16,
    pub container_port: u16,
    pub tag: String,
}

fn ports_with_added(existing: &str, hp: u16, cp: u16) -> String {
    let mut lines: Vec<String> = existing.lines()
        .filter(|l| !matches_port(l, hp, cp))
        .map(|l| l.to_string())
        .collect();
    lines.push(format!("{}:{}/tcp", hp, cp));
    lines.push(format!("{}:{}/udp", hp, cp));
    lines.join("\n")
}

fn ports_without(existing: &str, hp: u16, cp: u16) -> String {
    existing.lines()
        .filter(|l| !matches_port(l, hp, cp))
        .collect::<Vec<_>>()
        .join("\n")
}

fn matches_port(line: &str, hp: u16, cp: u16) -> bool {
    if let Some((hp_str, rest)) = line.split_once(':') {
        let cp_str = rest.split('/').next().unwrap_or(rest);
        if let (Ok(h), Ok(c)) = (hp_str.trim().parse::<u16>(), cp_str.trim().parse::<u16>()) {
            return h == hp && c == cp;
        }
    }
    false
}

async fn recreate_for_ports(
    state: &AppState,
    db_id: i64,
    new_ports: String,
) -> Result<(), (StatusCode, String)> {
    let (docker_id, db_name) = db::get_server_info_by_db_id(&state.db, db_id)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Server not found".to_string()))?;

    let owner_id = db::get_server_owner(&state.db, &docker_id)
        .await
        .ok()
        .flatten()
        .unwrap_or(0);

    let old_cfg = docker::inspect_full(&state.docker, &docker_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let docker_name = docker::get_container(&state.docker, &docker_id)
        .await
        .map(|c| c.name)
        .unwrap_or_else(|_| docker_id.clone());

    let was_running = old_cfg.state == "running";

    let new_id = docker::recreate_with_updated_config(
        &state.docker, &docker_id, &old_cfg.image, &old_cfg.env,
        &new_ports, old_cfg.cpu, old_cfg.memory_mb, &docker_name,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Migrate bw file
    let cwd = std::env::current_dir().unwrap_or_default();
    let old_bw = cwd.join("bw").join(&docker_id);
    let new_bw = cwd.join("bw").join(&new_id);
    if old_bw.exists() { let _ = tokio::fs::rename(&old_bw, &new_bw).await; }

    if let Err(e) = db::update_server(&state.db, &docker_id, &new_id, &db_name, owner_id).await {
        error!("recreate_for_ports update_server: {}", e);
    }

    if was_running {
        if let Err(e) = docker::start_container(&state.docker, &new_id).await {
            error!("recreate_for_ports start: {}", e);
        } else {
            docker::reapply_bandwidth_limit(&state.docker, &new_id).await;
            docker::reapply_isolation_rules(&state.docker, &new_id).await;
        }
    }

    Ok(())
}

pub async fn api_add_port(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
    Json(body): Json<AddPortBody>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::is_admin_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Only admins can open ports."}))).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"not found"}))).into_response(),
    };
    let old_cfg = match docker::inspect_full(&state.docker, &docker_id).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    let new_ports = ports_with_added(&old_cfg.ports, body.host_port, body.container_port);

    if let Err((code, msg)) = recreate_for_ports(&state, db_id, new_ports).await {
        return (code, Json(serde_json::json!({"error": msg}))).into_response();
    }

    // Always upsert entry; enabled=1 because we just opened it
    if let Err(e) = db::set_port_tag(&state.db, db_id, body.host_port as i64, body.container_port as i64, &body.tag).await {
        error!("api_add_port set_port_tag: {}", e);
    }
    if let Err(e) = db::set_port_enabled(&state.db, db_id, body.host_port as i64, body.container_port as i64, true).await {
        error!("api_add_port set_port_enabled: {}", e);
    }

    let actor = auth::session_username(&jar).unwrap_or_default();
    let _ = db::audit_log(&state.db, &actor, "net.port_add", &format!("#{}", db_id), &format!("{}:{}", body.host_port, body.container_port), &ip, &auth::user_agent(&headers)).await;

    Json(serde_json::json!({"ok": true})).into_response()
}

pub async fn api_remove_port(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
    Json(body): Json<RemovePortBody>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::is_admin_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Only admins can close ports."}))).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"not found"}))).into_response(),
    };
    let old_cfg = match docker::inspect_full(&state.docker, &docker_id).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    let new_ports = ports_without(&old_cfg.ports, body.host_port, body.container_port);

    if let Err((code, msg)) = recreate_for_ports(&state, db_id, new_ports).await {
        return (code, Json(serde_json::json!({"error": msg}))).into_response();
    }

    if let Err(e) = db::delete_port_entry(&state.db, db_id, body.host_port as i64, body.container_port as i64).await {
        error!("api_remove_port delete_port_entry: {}", e);
    }

    let actor = auth::session_username(&jar).unwrap_or_default();
    let _ = db::audit_log(&state.db, &actor, "net.port_remove", &format!("#{}", db_id), &format!("{}:{}", body.host_port, body.container_port), &ip, &auth::user_agent(&headers)).await;

    Json(serde_json::json!({"ok": true})).into_response()
}

pub async fn api_tag_port(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
    Json(body): Json<TagPortBody>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::is_admin_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Only admins can tag ports."}))).into_response();
    }
    match db::set_port_tag(&state.db, db_id, body.host_port as i64, body.container_port as i64, &body.tag).await {
        Ok(_) => {
            let actor = auth::session_username(&jar).unwrap_or_default();
            let _ = db::audit_log(&state.db, &actor, "net.port_tag", &format!("#{}", db_id), &format!("{}:{} tag={}", body.host_port, body.container_port, body.tag), &ip, &auth::user_agent(&headers)).await;
            Json(serde_json::json!({"ok": true})).into_response()
        }
        Err(e) => {
            error!("api_tag_port: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct TogglePortBody {
    pub host_port: u16,
    pub container_port: u16,
    pub enabled: bool,
}

pub async fn api_toggle_port(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
    Json(body): Json<TogglePortBody>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::is_admin_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Only admins can toggle ports."}))).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"not found"}))).into_response(),
    };
    let old_cfg = match docker::inspect_full(&state.docker, &docker_id).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    let new_ports = if body.enabled {
        ports_with_added(&old_cfg.ports, body.host_port, body.container_port)
    } else {
        ports_without(&old_cfg.ports, body.host_port, body.container_port)
    };
    if let Err((code, msg)) = recreate_for_ports(&state, db_id, new_ports).await {
        return (code, Json(serde_json::json!({"error": msg}))).into_response();
    }
    if let Err(e) = db::set_port_enabled(&state.db, db_id, body.host_port as i64, body.container_port as i64, body.enabled).await {
        error!("api_toggle_port set_port_enabled: {}", e);
    }
    let actor = auth::session_username(&jar).unwrap_or_default();
    let _ = db::audit_log(&state.db, &actor, "net.port_toggle", &format!("#{}", db_id), &format!("{}:{} enabled={}", body.host_port, body.container_port, body.enabled), &ip, &auth::user_agent(&headers)).await;
    Json(serde_json::json!({"ok": true, "enabled": body.enabled})).into_response()
}

// ── UFW firewall management per port ─────────────────────────────────────────

#[derive(Deserialize)]
pub struct UfwPortBody {
    pub host_port: u16,
    pub container_port: u16,
    /// true = block in UFW (deny), false = remove block (allow/delete deny rule)
    pub block: bool,
}

pub async fn api_toggle_port_ufw(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
    Json(body): Json<UfwPortBody>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if !auth::is_root_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Only root can manage UFW rules."}))).into_response();
    }
    if !db::get_panel_setting_bool(&state.db, "ufw_enabled").await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "UFW management is disabled in panel settings."}))).into_response();
    }

    let port_str = body.host_port.to_string();
    let ufw_result = if body.block {
        tokio::process::Command::new("sudo")
            .args(["-n", "ufw", "deny", &port_str])
            .output().await
    } else {
        tokio::process::Command::new("sudo")
            .args(["-n", "ufw", "delete", "deny", &port_str])
            .output().await
    };

    match ufw_result {
        Ok(out) if out.status.success() || !body.block => {
            if let Err(e) = db::set_port_ufw_blocked(&state.db, db_id, body.host_port as i64, body.container_port as i64, body.block).await {
                error!("api_toggle_port_ufw set_port_ufw_blocked: {}", e);
            }
            let actor = auth::session_username(&jar).unwrap_or_default();
            let _ = db::audit_log(&state.db, &actor, "net.ufw_toggle", &format!("#{}", db_id), &format!("port {} blocked={}", body.host_port, body.block), &ip, &auth::user_agent(&headers)).await;
            Json(serde_json::json!({"ok": true, "blocked": body.block})).into_response()
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("password is required") || stderr.contains("Permission denied") || stderr.contains("not allowed") {
                let user = std::env::var("USER").unwrap_or_else(|_| "yunexal".into());
                let fix = format!("echo '{user} ALL=(ALL) NOPASSWD: /usr/sbin/ufw' | sudo tee /etc/sudoers.d/yunexal-ufw && sudo chmod 440 /etc/sudoers.d/yunexal-ufw");
                Json(serde_json::json!({"ok": false, "needs_permission": true, "fix_command": fix})).into_response()
            } else {
                error!("ufw command failed: {}", stderr);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("ufw failed: {}", stderr.trim())}))).into_response()
            }
        }
        Err(e) => {
            error!("ufw command error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to run ufw: {}", e)}))).into_response()
        }
    }
}

// ── Disk info endpoint ────────────────────────────────────────────────────────

pub async fn api_server_disk(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    if !auth::can_access_server(&state, &jar, db_id).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Access denied"}))).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"not found"}))).into_response(),
    };
    let volume_dir = docker::get_volume_dir(&state.docker, &docker_id)
        .await
        .unwrap_or_else(|_| docker_id.clone());
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_path = cwd.join("volumes").join(&volume_dir);

    let path_str = volume_path.to_string_lossy().to_string();

    // Volume used bytes via du -sb (sum of all file sizes in subtree)
    let volume_used = if volume_path.exists() {
        tokio::process::Command::new("du")
            .args(["-sb", &path_str])
            .output().await
            .map(|o| String::from_utf8_lossy(&o.stdout)
                .split_whitespace().next()
                .and_then(|v| v.parse::<u64>().ok()).unwrap_or(0))
            .unwrap_or(0)
    } else { 0 };

    // Filesystem total and used bytes via df -B1
    let (disk_total, disk_used) = tokio::process::Command::new("df")
        .args(["-B1", "--output=size,used", &path_str])
        .output().await
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            if let Some(line) = s.lines().nth(1) {
                let mut parts = line.split_whitespace();
                let total = parts.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                let used  = parts.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                (total, used)
            } else {
                (0, 0)
            }
        })
        .unwrap_or((0, 0));

    Json(serde_json::json!({
        "volume_used": volume_used,
        "disk_total": disk_total,
        "disk_used": disk_used,
    })).into_response()
}

