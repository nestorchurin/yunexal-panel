use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
    http::HeaderMap,
    response::{IntoResponse, Redirect},
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use std::net::SocketAddr;
use crate::{auth, db, docker, password};
use crate::state::AppState;
use tracing::error;
use super::templates::{
    render, AdminEditTemplate, AdminSetPasswordForm, AdminTemplate,
    ChangePwForm, ContainerEditInfo, CreateUserForm, EditContainerForm, UserInfo,
};

// ── Admin page ───────────────────────────────────────────────────────────────

const VALID_TABS: &[&str] = &[
    "overview", "containers", "users", "images",
    "agents", "dns", "firewall", "backups",
    "insights", "audit",
    "workspaces", "tickets",
    "billing", "plans", "coupons",
    "notifications", "themes", "apikeys", "nodes",
];

async fn build_admin_template(state: &AppState, tab: String, username: String) -> AdminTemplate {
    let containers = match docker::list_containers(&state.docker).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to list containers: {}", e);
            vec![]
        }
    };

    let total_containers = containers.len();
    let running_containers = containers.iter().filter(|c| c.state == "running").count();
    let stopped_containers = total_containers - running_containers;

    let (docker_version, docker_api_version) = match state.docker.version().await {
        Ok(v) => (
            v.version.unwrap_or_else(|| "unknown".to_string()),
            v.api_version.unwrap_or_else(|| "unknown".to_string()),
        ),
        Err(_) => ("unknown".to_string(), "unknown".to_string()),
    };

    let (docker_os, docker_arch, docker_mem_gb, docker_cpus, docker_storage_driver) =
        match state.docker.info().await {
            Ok(info) => (
                info.operating_system.unwrap_or_else(|| "unknown".to_string()),
                info.architecture.unwrap_or_else(|| "unknown".to_string()),
                format!("{:.1}", info.mem_total.unwrap_or(0) as f64 / 1_073_741_824.0),
                info.ncpu.unwrap_or(0),
                info.driver.unwrap_or_else(|| "unknown".to_string()),
            ),
            Err(_) => (
                "unknown".to_string(),
                "unknown".to_string(),
                "?".to_string(),
                0,
                "unknown".to_string(),
            ),
        };

    let panel_memory_mb = tokio::fs::read_to_string("/proc/self/status")
        .await
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
        })
        .map(|kb| format!("{:.1} MB", kb as f64 / 1024.0))
        .unwrap_or_else(|| "N/A".to_string());

    let users = match db::list_users(&state.db).await {
        Ok(u) => u
            .into_iter()
            .map(|u| UserInfo {
                id: u.id,
                username: u.username,
                role: u.role,
                created_at: u.created_at,
            })
            .collect(),
        Err(e) => {
            error!("Failed to list users: {}", e);
            vec![]
        }
    };

    let users_count = users.len();

    let (kernel_version, host_uptime, host_load_avg) = host_proc_info().await;
    let (host_ram_used_gb, host_ram_total_gb, host_swap_used_gb, host_swap_total_gb) = host_mem_info().await;
    let ZramInfo { active: zram_active, devices: zram_devices, disk_mb: zram_disk_mb,
                   orig_mb: zram_orig_mb, compr_mb: zram_compr_mb,
                   ratio: zram_ratio, algorithm: zram_algorithm } = host_zram_info().await;

    // Override display names from SQLite
    let mut containers = containers;
    let info_map = db::get_server_info_map(&state.db).await.unwrap_or_default();
    for c in &mut containers {
        if let Some((id, name, owner)) = info_map.get(&c.id) {
            c.db_id = *id;
            c.name = name.clone();
            c.owner = owner.clone();
        }
    }

    AdminTemplate {
        containers,
        total_containers,
        running_containers,
        stopped_containers,
        docker_version,
        docker_api_version,
        docker_os,
        docker_arch,
        docker_mem_gb,
        docker_cpus,
        docker_storage_driver,
        listen_addr: state.listen_addr.clone(),
        auth_username: username.clone(),
        auth_role: db::find_user_by_username(&state.db, &username)
            .await
            .ok()
            .flatten()
            .map(|u| u.role)
            .unwrap_or_else(|| "user".to_string()),
        panel_memory_mb,
        panel_version: env!("CARGO_PKG_VERSION").to_string(),
        users,
        users_count,
        tab,
        kernel_version,
        host_uptime,
        host_load_avg,
        host_ram_used_gb,
        host_ram_total_gb,
        host_swap_used_gb,
        host_swap_total_gb,
        zram_active,
        zram_devices,
        zram_disk_mb,
        zram_orig_mb,
        zram_compr_mb,
        zram_ratio,
        zram_algorithm,
    }
}

// ── Host system helpers ───────────────────────────────────────────────────────

async fn host_proc_info() -> (String, String, String) {
    let kernel = tokio::fs::read_to_string("/proc/version")
        .await
        .ok()
        .and_then(|s| s.split_whitespace().nth(2).map(|v| v.to_string()))
        .unwrap_or_else(|| "unknown".to_string());

    let uptime = tokio::fs::read_to_string("/proc/uptime")
        .await
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
        .map(|secs| {
            let s = secs as u64;
            let d = s / 86400;
            let h = (s % 86400) / 3600;
            let m = (s % 3600) / 60;
            if d > 0 { format!("{}d {}h {}m", d, h, m) }
            else if h > 0 { format!("{}h {}m", h, m) }
            else { format!("{}m", m) }
        })
        .unwrap_or_else(|| "N/A".to_string());

    let load = tokio::fs::read_to_string("/proc/loadavg")
        .await
        .ok()
        .map(|s| s.split_whitespace().take(3).collect::<Vec<_>>().join(" / "))
        .unwrap_or_else(|| "N/A".to_string());

    (kernel, uptime, load)
}

async fn host_mem_info() -> (String, String, String, String) {
    let content = tokio::fs::read_to_string("/proc/meminfo").await.unwrap_or_default();
    let mut mem_total_kb  = 0u64;
    let mut mem_avail_kb  = 0u64;
    let mut swap_total_kb = 0u64;
    let mut swap_free_kb  = 0u64;
    for line in content.lines() {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("MemTotal:")     => { mem_total_kb  = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0); }
            Some("MemAvailable:") => { mem_avail_kb  = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0); }
            Some("SwapTotal:")    => { swap_total_kb = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0); }
            Some("SwapFree:")     => { swap_free_kb  = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0); }
            _ => {}
        }
    }
    let gib = |kb: u64| format!("{:.1}", kb as f64 / (1024.0 * 1024.0));
    (
        gib(mem_total_kb.saturating_sub(mem_avail_kb)),
        gib(mem_total_kb),
        gib(swap_total_kb.saturating_sub(swap_free_kb)),
        gib(swap_total_kb),
    )
}

struct ZramInfo {
    active: bool,
    devices: usize,
    disk_mb: String,
    orig_mb: String,
    compr_mb: String,
    ratio: String,
    algorithm: String,
}

async fn host_zram_info() -> ZramInfo {
    let empty = ZramInfo {
        active: false,
        devices: 0,
        disk_mb: String::new(),
        orig_mb: String::new(),
        compr_mb: String::new(),
        ratio: String::new(),
        algorithm: String::new(),
    };

    // Count active zram devices (zram0, zram1, …)
    let mut devices = 0usize;
    let mut i = 0u32;
    loop {
        if tokio::fs::metadata(format!("/sys/block/zram{}", i)).await.is_ok() {
            devices += 1;
            i += 1;
        } else {
            break;
        }
    }
    if devices == 0 { return empty; }

    // Read mm_stat from zram0 (primary device)
    let mm = tokio::fs::read_to_string("/sys/block/zram0/mm_stat").await.unwrap_or_default();
    let nums: Vec<u64> = mm.split_whitespace()
        .take(3).filter_map(|v| v.parse().ok()).collect();
    if nums.len() < 2 || nums[0] == 0 { return empty; }

    // Disk size (configured capacity)
    let disksize_bytes: u64 = tokio::fs::read_to_string("/sys/block/zram0/disksize")
        .await.unwrap_or_default().trim().parse().unwrap_or(0);
    let disk_mb = if disksize_bytes > 0 {
        format!("{}", disksize_bytes / 1_048_576)
    } else {
        "?".to_string()
    };

    // Compression algorithm — find the bracketed entry: "lzo [lz4] zstd" → "lz4"
    let raw_algo = tokio::fs::read_to_string("/sys/block/zram0/comp_algorithm")
        .await.unwrap_or_default();
    let algorithm = raw_algo.split_whitespace()
        .find(|s| s.starts_with('[') && s.ends_with(']'))
        .map(|s| s.trim_matches(|c| c == '[' || c == ']').to_string())
        .unwrap_or_else(|| raw_algo.split_whitespace().next().unwrap_or("unknown").to_string());

    let orig_mb  = nums[0] as f64 / 1_048_576.0;
    let compr_mb = nums[1] as f64 / 1_048_576.0;
    let ratio = if nums[1] > 0 {
        format!("{:.1}:1", nums[0] as f64 / nums[1] as f64)
    } else {
        "N/A".to_string()
    };

    ZramInfo {
        active: true,
        devices,
        disk_mb,
        orig_mb: format!("{:.0}", orig_mb),
        compr_mb: format!("{:.0}", compr_mb),
        ratio,
        algorithm,
    }
}

pub async fn admin_page() -> impl IntoResponse {
    Redirect::permanent("/admin/overview")
}

pub async fn admin_tab_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(tab): Path<String>,
) -> impl IntoResponse {
    let tab = if VALID_TABS.contains(&tab.as_str()) {
        tab
    } else {
        "overview".to_string()
    };
    let username = auth::session_username(&jar).unwrap_or_default();
    render(build_admin_template(&state, tab, username).await)
}

// ── Docker helpers ───────────────────────────────────────────────────────────

pub async fn admin_stop_all(State(state): State<AppState>, addr: ConnectInfo<SocketAddr>, headers: HeaderMap) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let containers = match docker::list_containers(&state.docker).await {
        Ok(c) => c,
        Err(e) => {
            error!("admin_stop_all: {}", e);
            return Json(serde_json::json!({"ok": false, "error": "Failed to list containers"}));
        }
    };
    for c in containers.iter().filter(|c| c.state == "running") {
        if let Err(e) = docker::stop_container(&state.docker, &c.id).await {
            error!("admin_stop_all: failed to stop {}: {}", c.id, e);
        }
    }
    let _ = db::audit_log(&state.db, "admin", "admin.stop_all", "", &format!("{} containers", containers.len()), &ip).await;
    Json(serde_json::json!({"ok": true}))
}

// ── Account password change (own account) ───────────────────────────────────

pub async fn admin_change_password(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<ChangePwForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let session_user = match auth::session_username(&jar) {
        Some(u) => u,
        None => return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Not authenticated"})),
        ),
    };
    let user = match db::find_user_by_username(&state.db, &session_user).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Admin user not found in database"})),
            );
        }
        Err(e) => {
            error!("admin_change_password: db error: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            );
        }
    };

    if !password::verify(&body.current, &user.password_hash) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Current password is incorrect"})),
        );
    }

    match password::hash(&body.new_password) {
        Ok(hash) => match db::update_user_password(&state.db, user.id, &hash).await {
            Ok(_) => {
                let _ = db::audit_log(&state.db, &session_user, "user.change_password", &session_user, "", &ip).await;
                (StatusCode::OK, Json(serde_json::json!({"ok": true})))
            }
            Err(e) => {
                error!("admin_change_password: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Failed to update password"})),
                )
            }
        },
        Err(e) => {
            error!("admin_change_password: hash error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to hash password"})),
            )
        }
    }
}

// ── User management API ──────────────────────────────────────────────────────

pub async fn api_create_user(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<CreateUserForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if body.username.trim().is_empty() || body.password.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Username and password are required"})),
        );
    }
    let role = if body.role == "admin" { "admin" } else { "user" };
    let hash = match password::hash(&body.password) {
        Ok(h) => h,
        Err(e) => {
            error!("api_create_user: hash error: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to hash password"})),
            );
        }
    };
    match db::create_user(&state.db, body.username.trim(), &hash, role).await {
        Ok(id) => {
            let _ = db::audit_log(&state.db, "admin", "user.create", body.username.trim(), &format!("role={}", role), &ip).await;
            (
                StatusCode::OK,
                Json(serde_json::json!({"ok": true, "id": id})),
            )
        }
        Err(e) => {
            let msg = e.to_string();
            let user_msg = if msg.contains("UNIQUE") {
                "Username already exists"
            } else {
                "Failed to create user"
            };
            error!("api_create_user: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": user_msg})),
            )
        }
    }
}

pub async fn api_delete_user(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    // Prevent deleting root users or the primary admin login
    match db::find_user_by_id(&state.db, id).await {
        Ok(Some(u)) if u.role == "root" => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": "Cannot delete the root account"})),
            );
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "User not found"})),
            );
        }
        Err(e) => {
            error!("api_delete_user: db error: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            );
        }
        _ => {}
    }
    match db::delete_user(&state.db, id).await {
        Ok(_) => {
            let _ = db::audit_log(&state.db, "admin", "user.delete", &format!("uid:{}", id), "", &ip).await;
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        }
        Err(e) => {
            error!("api_delete_user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to delete user"})),
            )
        }
    }
}

pub async fn api_set_user_password(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<AdminSetPasswordForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    if body.new_password.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Password cannot be empty"})),
        );
    }
    let hash = match password::hash(&body.new_password) {
        Ok(h) => h,
        Err(e) => {
            error!("api_set_user_password: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to hash password"})),
            );
        }
    };
    match db::update_user_password(&state.db, id, &hash).await {
        Ok(_) => {
            let _ = db::audit_log(&state.db, "admin", "user.set_password", &format!("uid:{}", id), "", &ip).await;
            (StatusCode::OK, Json(serde_json::json!({"ok": true})))
        },
        Err(e) => {
            error!("api_set_user_password: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to update password"})),
            )
        }
    }
}

// ── Container edit page ───────────────────────────────────────────────────────

pub async fn admin_edit_page(
    State(state): State<AppState>,
    Path(db_id): Path<i64>,
) -> impl IntoResponse {
    let (docker_id, db_name) = match db::get_server_info_by_db_id(&state.db, db_id).await {
        Ok(Some(row)) => row,
        Ok(None) => return Redirect::to("/admin").into_response(),
        Err(e) => { error!("admin_edit_page db lookup: {}", e); return Redirect::to("/admin").into_response(); }
    };

    let container = match docker::get_container(&state.docker, &docker_id).await {
        Ok(mut c) => { c.name = db_name; c.db_id = db_id; c },
        Err(e) => {
            error!("admin_edit_page get_container: {}", e);
            return Redirect::to("/admin").into_response();
        }
    };

    let full_config = match docker::inspect_full(&state.docker, &container.id).await {
        Ok(c) => c,
        Err(e) => {
            error!("admin_edit_page inspect_full: {}", e);
            return Redirect::to("/admin").into_response();
        }
    };

    let owner_id = db::get_server_owner(&state.db, &container.id)
        .await
        .ok()
        .flatten()
        .unwrap_or(0);

    let users: Vec<UserInfo> = db::list_users(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|u| UserInfo { id: u.id, username: u.username, role: u.role, created_at: u.created_at })
        .collect();

    render(AdminEditTemplate {
        id: db_id,
        container,
        edit: ContainerEditInfo {
            image: full_config.image,
            env: full_config.env,
            ports: full_config.ports,
            cpu: if full_config.cpu == 0.0 { String::new() } else { format!("{:.2}", full_config.cpu) },
            memory_mb: if full_config.memory_mb == 0 { String::new() } else { full_config.memory_mb.to_string() },
            owner_id,
        },
        users,
        error: None,
    }).into_response()
}

// ── Container edit API ────────────────────────────────────────────────────────

pub async fn api_admin_edit_container(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(db_id): Path<i64>,
    Json(form): Json<EditContainerForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let (docker_id, current_db_name) = match db::get_server_info_by_db_id(&state.db, db_id).await.ok().flatten() {
        Some(row) => row,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Server not found"}))),
    };
    let container = match docker::get_container(&state.docker, &docker_id).await {
        Ok(c) => c,
        Err(e) => {
            error!("api_admin_edit_container get_container: {}", e);
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Container not found"})));
        }
    };
    let full_id = container.id.clone();
    // Docker container name used as stable internal identifier
    let docker_name = container.name.clone();

    let old_config = match docker::inspect_full(&state.docker, &full_id).await {
        Ok(c) => c,
        Err(e) => {
            error!("api_admin_edit_container inspect_full: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()})));
        }
    };

    let was_running = old_config.state == "running";
    let new_name = form.name.trim().to_string();
    // Compare against SQLite name — Docker name is irrelevant for display
    let name_changed = current_db_name != new_name;

    // Check for duplicate name (exclude the current container so it can keep its own name)
    if name_changed {
        match db::server_name_exists(&state.db, &new_name, Some(&full_id)).await {
            Ok(true) => return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": format!("A server named '{}' already exists.", new_name)
                }))
            ),
            Err(e) => error!("server_name_exists check: {}", e),
            Ok(false) => {}
        }
    }

    let image_changed = old_config.image.trim() != form.image.trim();
    let ports_changed = sort_lines(&old_config.ports) != sort_lines(&form.ports);
    let env_changed   = sort_lines(&old_config.env)   != sort_lines(&form.env);
    let needs_recreate = image_changed || ports_changed || env_changed;

    let resources_changed = (old_config.cpu - form.cpu).abs() > 0.001
        || old_config.memory_mb != form.memory_mb;

    let effective_name = if name_changed { new_name.clone() } else { current_db_name.clone() };

    if needs_recreate {
        let image = form.image.trim().to_string();
        // Pass the existing Docker container name — it's the internal identifier
        let new_id = match docker::recreate_with_updated_config(
            &state.docker, &full_id, &image, &form.env,
            &form.ports, form.cpu, form.memory_mb, &docker_name,
        ).await {
            Ok(id) => id,
            Err(e) => {
                error!("api_admin_edit_container recreate: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()})));
            }
        };

        // Move bw file to new container ID
        let cwd = std::env::current_dir().unwrap_or_default();
        let old_bw = cwd.join("bw").join(&full_id);
        let new_bw = cwd.join("bw").join(&new_id);
        if old_bw.exists() { let _ = tokio::fs::rename(&old_bw, &new_bw).await; }

        // Update DB
        if let Err(e) = db::update_server(&state.db, &full_id, &new_id, &effective_name, form.owner_id).await {
            error!("api_admin_edit_container update_server: {}", e);
        }

        if was_running {
            if let Err(e) = docker::start_container(&state.docker, &new_id).await {
                error!("api_admin_edit_container start: {}", e);
            } else {
                docker::reapply_bandwidth_limit(&state.docker, &new_id).await;
                docker::reapply_isolation_rules(&state.docker, &new_id).await;
            }
        }

        let short = if new_id.len() >= 12 { &new_id[..12] } else { &new_id };
        let _ = db::audit_log(&state.db, "admin", "server.edit", &effective_name, &format!("#{} recreated", db_id), &ip).await;
        return (StatusCode::OK, Json(serde_json::json!({"ok": true, "new_id": db_id, "new_short": short})));
    }

    // No recreate — update resources + SQLite only (Docker name is internal, not renamed)
    if resources_changed {
        if let Err(e) = docker::update_container_resources(&full_id, form.cpu, form.memory_mb).await {
            error!("api_admin_edit_container update_resources (non-fatal): {}", e);
        }
    }

    if let Err(e) = db::update_server_name_and_owner(&state.db, &full_id, &effective_name, form.owner_id).await {
        error!("api_admin_edit_container update_server_name_and_owner: {}", e);
    }

    let _ = db::audit_log(&state.db, "admin", "server.edit", &effective_name, &format!("#{} updated", db_id), &ip).await;

    (StatusCode::OK, Json(serde_json::json!({"ok": true, "new_id": null})))
}

fn sort_lines(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    v.sort();
    v
}

// ── Image management API ──────────────────────────────────────────────────────

pub async fn api_list_images(
    State(state): State<AppState>,
) -> impl IntoResponse {
    const CACHE_TTL: u64 = 30;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Serve from cache if fresh
    let cached_ts   = state.cache.get("images_ts").and_then(|v| v.value().parse::<u64>().ok());
    let cached_data = state.cache.get("images_data").map(|v| v.value().clone());
    if let (Some(ts), Some(data)) = (cached_ts, cached_data) {
        if now.saturating_sub(ts) < CACHE_TTL {
            return (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                data,
            ).into_response();
        }
    }

    match docker::list_docker_images(&state.docker).await {
        Ok(images) => {
            let body = serde_json::json!({ "ok": true, "images": images }).to_string();
            state.cache.insert("images_data".to_string(), body.clone());
            state.cache.insert("images_ts".to_string(), now.to_string());
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                body,
            ).into_response()
        }
        Err(e) => {
            error!("api_list_images: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

pub async fn api_delete_image(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(image_ref): Path<String>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let decoded = urlencoding::decode(&image_ref).unwrap_or(std::borrow::Cow::Borrowed(&image_ref)).into_owned();
    match docker::delete_docker_image(&state.docker, &decoded).await {
        Ok(_) => {
            state.cache.remove("images_ts");
            let _ = db::delete_image_env(&state.db, &decoded).await;
            let _ = db::audit_log(&state.db, "admin", "image.delete", &decoded, "", &ip).await;
            (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
        }
        Err(e) => {
            error!("api_delete_image: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct PullImageForm {
    pub image: String,
}

pub async fn api_pull_image(
    State(state): State<AppState>,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PullImageForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let image = body.image.trim().to_string();
    if image.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "image reference is required" }))).into_response();
    }
    match docker::ensure_image(&state.docker, &image).await {
        Ok(_) => {
            state.cache.remove("images_ts");
            let _ = db::audit_log(&state.db, "admin", "image.pull", &image, "", &ip).await;
            (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
        }
        Err(e) => {
            error!("api_pull_image: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

// ── Image ENV overrides API ───────────────────────────────────────────────────

pub async fn api_get_image_env(
    State(state): State<AppState>,
    Path(image_ref): Path<String>,
) -> impl IntoResponse {
    let decoded = urlencoding::decode(&image_ref).unwrap_or(std::borrow::Cow::Borrowed(&image_ref)).into_owned();
    match db::get_image_env(&state.db, &decoded).await {
        Ok(env) => (StatusCode::OK, Json(serde_json::json!({ "ok": true, "env": env }))).into_response(),
        Err(e) => {
            error!("api_get_image_env: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct SetImageEnvForm {
    pub env: String,
}

pub async fn api_set_image_env(
    State(state): State<AppState>,
    Path(image_ref): Path<String>,
    Json(body): Json<SetImageEnvForm>,
) -> impl IntoResponse {
    let decoded = urlencoding::decode(&image_ref).unwrap_or(std::borrow::Cow::Borrowed(&image_ref)).into_owned();
    match db::set_image_env(&state.db, &decoded, &body.env).await {
        Ok(_) => {
            // Invalidate image cache so next list reflects the update
            state.cache.remove("images_ts");
            (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
        }
        Err(e) => {
            error!("api_set_image_env: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

// ── Image full duplicate API ──────────────────────────────────────────────────

pub async fn api_duplicate_image(
    State(state): State<AppState>,
    Path(image_ref): Path<String>,
) -> impl IntoResponse {
    let decoded = urlencoding::decode(&image_ref).unwrap_or(std::borrow::Cow::Borrowed(&image_ref)).into_owned();

    // Collect source tags and env overrides before any mutation
    let src_tags: Vec<String> = docker::get_image_info(&state.docker, &decoded).await
        .ok()
        .and_then(|i| i.repo_tags)
        .unwrap_or_default()
        .into_iter()
        .filter(|t| t != "<none>:<none>")
        .collect();
    let src_env = db::get_image_env(&state.db, &decoded).await.unwrap_or_default();

    match docker::duplicate_docker_image(&state.docker, &decoded).await {
        Ok(new_id) => {
            // Give the duplicate an auto-generated unique tag so:
            // 1. it's visible in the image list (not <none>:<none>)
            // 2. the original keeps its own tags untouched
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            // Derive a base repo name from the first source tag, or fall back to "image"
            let base_repo = src_tags.first()
                .and_then(|t| t.rsplit_once(':').map(|(r, _)| r).or(Some(t.as_str())))
                .unwrap_or("image");
            let dup_repo = format!("{}-dup", base_repo);
            let dup_tag  = ts.to_string();
            if let Err(e) = docker::retag_docker_image(&state.docker, &new_id, &dup_repo, &dup_tag).await {
                error!("api_duplicate_image: auto-tag {}: {}", new_id, e);
            }

            // Copy DB env overrides to the new image ID
            if !src_env.is_empty() {
                if let Err(e) = db::set_image_env(&state.db, &new_id, &src_env).await {
                    error!("api_duplicate_image: copy env to {}: {}", new_id, e);
                }
            }

            state.cache.remove("images_ts");
            (StatusCode::OK, Json(serde_json::json!({ "ok": true, "new_id": new_id }))).into_response()
        }
        Err(e) => {
            error!("api_duplicate_image: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

// ── Real-time polling endpoints ───────────────────────────────────────────────

pub async fn api_admin_containers(State(state): State<AppState>) -> impl IntoResponse {
    let mut containers = match docker::list_containers(&state.docker).await {
        Ok(c) => c,
        Err(e) => {
            error!("api_admin_containers: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to list containers" })),
            ).into_response();
        }
    };

    let info_map = db::get_server_info_map(&state.db).await.unwrap_or_default();
    for c in &mut containers {
        if let Some((id, name, owner)) = info_map.get(&c.id) {
            c.db_id = *id;
            c.name = name.clone();
            c.owner = owner.clone();
        }
    }

    let total = containers.len();
    let running = containers.iter().filter(|c| c.state == "running").count();
    let stopped = total - running;

    let list: Vec<serde_json::Value> = containers.iter().map(|c| {
        serde_json::json!({
            "db_id":     c.db_id,
            "name":      c.name,
            "short_id":  c.short_id,
            "owner":     c.owner,
            "state":     c.state,
            "status":    c.status,
            "cpu_usage": c.cpu_usage,
            "ram_usage": c.ram_usage,
        })
    }).collect();

    Json(serde_json::json!({
        "ok": true,
        "containers": list,
        "total": total,
        "running": running,
        "stopped": stopped,
    })).into_response()
}

pub async fn api_admin_overview(State(state): State<AppState>) -> impl IntoResponse {
    let containers = match docker::list_containers(&state.docker).await {
        Ok(c) => c,
        Err(e) => {
            error!("api_admin_overview: {}", e);
            vec![]
        }
    };

    let total = containers.len();
    let running = containers.iter().filter(|c| c.state == "running").count();
    let stopped = total - running;

    let docker_version = match state.docker.version().await {
        Ok(v) => v.version.unwrap_or_else(|| "unknown".to_string()),
        Err(_) => "unknown".to_string(),
    };

    let panel_memory_mb = tokio::fs::read_to_string("/proc/self/status")
        .await
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
        })
        .map(|kb| format!("{:.1} MB", kb as f64 / 1024.0))
        .unwrap_or_else(|| "N/A".to_string());

    let users_count = db::list_users(&state.db).await.map(|u| u.len()).unwrap_or(0);

    Json(serde_json::json!({
        "ok": true,
        "total_containers": total,
        "running_containers": running,
        "stopped_containers": stopped,
        "docker_version": docker_version,
        "panel_memory_mb": panel_memory_mb,
        "users_count": users_count,
    })).into_response()
}

// ── Audit log API ─────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct AuditQuery {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub action: Option<String>,
    pub actor: Option<String>,
    pub search: Option<String>,
}

pub async fn api_audit_list(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<AuditQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(50).min(200).max(1);
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * limit;
    let action = q.action.as_deref().unwrap_or("");
    let actor = q.actor.as_deref().unwrap_or("");
    let search = q.search.as_deref().unwrap_or("");
    let total = db::audit_count(&state.db, action, actor, search).await.unwrap_or(0);
    let entries = db::audit_list(&state.db, limit, offset, action, actor, search).await.unwrap_or_default();
    Json(serde_json::json!({
        "ok": true,
        "entries": entries,
        "total": total,
        "page": page,
        "pages": (total as f64 / limit as f64).ceil() as i64,
    }))
}

pub async fn api_audit_clear(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let actor = auth::session_username(&jar).unwrap_or_default();
    let _ = db::audit_clear(&state.db).await;
    let _ = db::audit_log(&state.db, &actor, "audit.clear", "", "", &ip).await;
    Json(serde_json::json!({"ok": true}))
}

// ── Update check / apply API ──────────────────────────────────────────────────

const GITHUB_REPO: &str = "nestorchurin/yunexal-panel";

#[derive(serde::Deserialize)]
pub struct UpdateCheckQuery {
    pub channel: Option<String>,
}

/// GET /api/admin/updates/check?channel=stable|unstable
/// Checks the latest version available on GitHub.
pub async fn api_update_check(
    Query(q): Query<UpdateCheckQuery>,
) -> impl IntoResponse {
    let current = env!("CARGO_PKG_VERSION");
    let channel = q.channel.as_deref().unwrap_or("stable");

    let client = match reqwest::Client::builder()
        .user_agent("yunexal-panel")
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    };

    if channel == "unstable" {
        // For unstable, check the latest commit on the unstable branch.
        let url = format!(
            "https://api.github.com/repos/{}/commits/unstable",
            GITHUB_REPO
        );
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await.unwrap_or_default();
                let sha = body["sha"].as_str().unwrap_or("unknown");
                let short_sha = if sha.len() >= 7 { &sha[..7] } else { sha };
                let message = body["commit"]["message"].as_str().unwrap_or("");
                let date = body["commit"]["committer"]["date"].as_str().unwrap_or("");
                Json(serde_json::json!({
                    "ok": true,
                    "channel": "unstable",
                    "current_version": current,
                    "latest_commit": short_sha,
                    "commit_message": message.lines().next().unwrap_or(""),
                    "commit_date": date,
                    "download_url": format!("https://github.com/{}/archive/refs/heads/unstable.zip", GITHUB_REPO),
                }))
            }
            Ok(resp) => {
                let status = resp.status();
                Json(serde_json::json!({"ok": false, "error": format!("GitHub API returned {status}")}))
            }
            Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string()})),
        }
    } else {
        // Stable: check latest GitHub release.
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            GITHUB_REPO
        );
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await.unwrap_or_default();
                let tag = body["tag_name"].as_str().unwrap_or("unknown");
                let latest = tag.trim_start_matches('v');
                let has_update = version_gt(latest, current);
                let published = body["published_at"].as_str().unwrap_or("");
                let changelog = body["body"].as_str().unwrap_or("");
                // Find the linux x86_64 asset download URL
                let download_url = body["assets"]
                    .as_array()
                    .and_then(|assets| {
                        assets.iter().find_map(|a| {
                            let name = a["name"].as_str().unwrap_or("");
                            if name.contains("linux") && name.contains("x86_64") && name.ends_with(".tar.gz") {
                                a["browser_download_url"].as_str().map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                    })
                    .unwrap_or_default();

                Json(serde_json::json!({
                    "ok": true,
                    "channel": "stable",
                    "current_version": current,
                    "latest_version": latest,
                    "has_update": has_update,
                    "published_at": published,
                    "changelog": changelog,
                    "download_url": download_url,
                    "release_url": body["html_url"].as_str().unwrap_or(""),
                }))
            }
            Ok(resp) => {
                let status = resp.status();
                Json(serde_json::json!({"ok": false, "error": format!("GitHub API returned {status}")}))
            }
            Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string()})),
        }
    }
}

/// Simple semver comparison: returns true if `a` > `b` (major.minor.patch).
fn version_gt(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let mut parts = s.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
        (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        )
    };
    parse(a) > parse(b)
}

/// POST /api/admin/updates/apply
/// Downloads the latest release binary and replaces the current one, then signals a restart.
pub async fn api_update_apply(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<UpdateApplyForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let actor = auth::session_username(&jar).unwrap_or_default();
    let download_url = body.download_url.trim();

    // Validate URL belongs to our GitHub repo
    let allowed_prefix = format!("https://github.com/{}/", GITHUB_REPO);
    if !download_url.starts_with(&allowed_prefix) {
        return Json(serde_json::json!({"ok": false, "error": "Invalid download URL"}));
    }

    let _ = db::audit_log(&state.db, &actor, "panel.update", "", &format!("url={download_url}"), &ip).await;

    let client = match reqwest::Client::builder()
        .user_agent("yunexal-panel")
        .timeout(std::time::Duration::from_secs(120))
        .build()
    {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    };

    // Download to a temp file
    let resp = match client.get(download_url).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => return Json(serde_json::json!({"ok": false, "error": format!("Download failed: HTTP {}", r.status())})),
        Err(e) => return Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    };

    let current_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("Cannot determine binary path: {e}")})),
    };

    let parent_dir = current_exe.parent().unwrap_or(std::path::Path::new("."));
    let tmp_archive = parent_dir.join(".yunexal-update.tar.gz");
    let tmp_extract = parent_dir.join(".yunexal-update-extract");

    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("Download read failed: {e}")})),
    };

    if let Err(e) = tokio::fs::write(&tmp_archive, &bytes).await {
        return Json(serde_json::json!({"ok": false, "error": format!("Write failed: {e}")}));
    }

    // Extract tar.gz
    let _ = tokio::fs::remove_dir_all(&tmp_extract).await;
    if let Err(e) = tokio::fs::create_dir_all(&tmp_extract).await {
        let _ = tokio::fs::remove_file(&tmp_archive).await;
        return Json(serde_json::json!({"ok": false, "error": format!("mkdir failed: {e}")}));
    }

    let archive_path = tmp_archive.clone();
    let extract_path = tmp_extract.clone();
    let extract_result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let file = std::fs::File::open(&archive_path).map_err(|e| e.to_string())?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        // Prevent path traversal and limit entry count
        archive.set_overwrite(false);
        let mut count = 0u32;
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            count += 1;
            if count > 500 {
                return Err("Archive has too many entries".to_string());
            }
            entry.unpack_in(&extract_path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }).await;

    let _ = tokio::fs::remove_file(&tmp_archive).await;

    match extract_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            let _ = tokio::fs::remove_dir_all(&tmp_extract).await;
            return Json(serde_json::json!({"ok": false, "error": format!("Extract failed: {e}")}));
        }
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&tmp_extract).await;
            return Json(serde_json::json!({"ok": false, "error": format!("Task failed: {e}")}));
        }
    }

    // Find the yunexal-panel binary inside extracted contents
    let new_binary = find_binary_in_dir(&tmp_extract, "yunexal-panel").await;
    let new_setup = find_binary_in_dir(&tmp_extract, "yunexal-setup").await;

    if new_binary.is_none() {
        let _ = tokio::fs::remove_dir_all(&tmp_extract).await;
        return Json(serde_json::json!({"ok": false, "error": "yunexal-panel binary not found in archive"}));
    }

    let new_bin_path = new_binary.unwrap();

    // Backup current binary
    let backup_path = current_exe.with_extension("bak");
    if let Err(e) = tokio::fs::copy(&current_exe, &backup_path).await {
        let _ = tokio::fs::remove_dir_all(&tmp_extract).await;
        return Json(serde_json::json!({"ok": false, "error": format!("Backup failed: {e}")}));
    }

    // Replace binary
    if let Err(e) = tokio::fs::copy(&new_bin_path, &current_exe).await {
        // Restore backup
        let _ = tokio::fs::copy(&backup_path, &current_exe).await;
        let _ = tokio::fs::remove_dir_all(&tmp_extract).await;
        return Json(serde_json::json!({"ok": false, "error": format!("Replace failed: {e}")}));
    }

    // Ensure the new binary is executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        if let Err(e) = std::fs::set_permissions(&current_exe, perms) {
            error!("Failed to set binary permissions: {}", e);
        }
    }

    // Also update setup binary if present
    if let Some(setup_path) = new_setup {
        let setup_dest = parent_dir.join("yunexal-setup");
        let _ = tokio::fs::copy(&setup_path, &setup_dest).await;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&setup_dest, std::fs::Permissions::from_mode(0o755));
        }
    }

    let _ = tokio::fs::remove_dir_all(&tmp_extract).await;

    let _ = db::audit_log(&state.db, &actor, "panel.updated", "", "binary replaced, restarting", &ip).await;

    // Schedule a graceful restart after responding
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        // If running under systemd, this will trigger a restart
        std::process::exit(0);
    });

    Json(serde_json::json!({"ok": true, "message": "Update applied. Panel is restarting…"}))
}

#[derive(serde::Deserialize)]
pub struct UpdateApplyForm {
    pub download_url: String,
}

/// Walk a directory recursively to find a binary by name.
async fn find_binary_in_dir(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let mut rd = match tokio::fs::read_dir(&d).await {
            Ok(r) => r,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
                return Some(path);
            }
        }
    }
    None
}
