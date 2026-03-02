use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use serde::Deserialize;
use crate::{auth, db, docker};
use crate::state::AppState;
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
            let mut all_ports: std::collections::BTreeMap<(u16, u16), (String, bool)> = std::collections::BTreeMap::new();
            // DB entries first (include disabled ones)
            for ((hp, cp), (tag, enabled)) in &db_ports {
                all_ports.insert((*hp as u16, *cp as u16), (tag.clone(), *enabled));
            }
            // Docker ports not yet tracked in DB are enabled by definition
            for (hp, cp) in &docker_ports {
                all_ports.entry((*hp, *cp)).or_insert_with(|| (String::new(), true));
            }
            let ports = all_ports
                .into_iter()
                .map(|((hp, cp), (tag, enabled))| PortRow { host_port: hp, container_port: cp, tag, enabled })
                .collect();
            render(NetworkingTemplate { id: db_id, container, bandwidth_mbit, is_admin, ports }).into_response()
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
    Path(db_id): Path<i64>,
    Json(body): Json<SetBandwidthBody>,
) -> impl IntoResponse {
    if !auth::is_admin_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Only admins can change bandwidth limits." }))).into_response();
    }
    let docker_id = match db::get_container_id_by_server_id(&state.db, db_id).await.ok().flatten() {
        Some(cid) => cid,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"not found"}))).into_response(),
    };
    match docker::set_bandwidth_limit(&state.docker, &docker_id, body.mbit).await {
        Ok(_) => Json(serde_json::json!({ "ok": true })).into_response(),
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
    Path(db_id): Path<i64>,
    Json(body): Json<AddPortBody>,
) -> impl IntoResponse {
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

    Json(serde_json::json!({"ok": true})).into_response()
}

pub async fn api_remove_port(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Json(body): Json<RemovePortBody>,
) -> impl IntoResponse {
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

    Json(serde_json::json!({"ok": true})).into_response()
}

pub async fn api_tag_port(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(db_id): Path<i64>,
    Json(body): Json<TagPortBody>,
) -> impl IntoResponse {
    if !auth::is_admin_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Only admins can tag ports."}))).into_response();
    }
    match db::set_port_tag(&state.db, db_id, body.host_port as i64, body.container_port as i64, &body.tag).await {
        Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
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
    Path(db_id): Path<i64>,
    Json(body): Json<TogglePortBody>,
) -> impl IntoResponse {
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
    Json(serde_json::json!({"ok": true, "enabled": body.enabled})).into_response()
}
