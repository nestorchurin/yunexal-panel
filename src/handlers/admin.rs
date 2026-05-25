use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
    http::HeaderMap,
    response::{IntoResponse, Redirect},
    Extension, Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use bollard::query_parameters::ListContainersOptions;
use std::net::SocketAddr;
use crate::{auth, db, docker, host, password};
use crate::state::AppState;
use tracing::error;
use super::CspNonce;
use super::templates::{
    render, AdminEditTemplate, AdminSetPasswordForm, AdminTemplate,
    ChangePwForm, ContainerEditInfo, CreateRoleForm, CreateUserForm, EditContainerForm,
    SetRolePermissionsForm, SetUserRoleForm, UserInfo,
};

// ── Admin page ───────────────────────────────────────────────────────────────

const VALID_TABS: &[&str] = &[
    "overview", "containers", "users", "images",
    "roles", "distpatchers", "firewall", "backups",
    "insights", "audit", "settings", "tickets",
    "notifications", "themes", "apikeys", "nodes",
];

const ROLE_GROUPS: &[(&str, &[&str])] = &[
    (
        "Admin Core",
        &[
            "admin.access",
            "tab.overview",
            "tab.containers",
            "tab.images",
            "tab.users",
            "tab.roles",
            "tab.audit",
            "tab.settings",
        ],
    ),
    (
        "Server Control",
        &[
            "servers.create",
            "servers.edit",
            "servers.delete",
            "servers.global_access",
            "containers.manage",
        ],
    ),
    (
        "Platform Services",
        &[
            "images.manage",
            "users.manage",
            "roles.manage",
            "audit.read",
            "panel.update",
            "panel.settings",
            "storage.manage",
            "security.manage",
            "theme.manage",
        ],
    ),
];

fn default_policy_from_permissions(permissions: &[String]) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for def in db::permission_catalog() {
        let mode = if permissions.iter().any(|p| p == def.key) {
            "write"
        } else {
            "none"
        };
        map.insert(def.key.to_string(), mode.to_string());
    }
    map
}

fn default_role_color(role: &str) -> &'static str {
    match role {
        "root" => "#ef4444",
        "admin" => "#a78bfa",
        "user" => "#60a5fa",
        _ => "#94a3b8",
    }
}

const USER_UID_MIN_LEN: usize = 9;
const USER_UID_MAX_LEN: usize = 16;

fn normalize_user_uid(uid: &str) -> String {
    uid.trim().to_string()
}

fn normalize_user_nickname(nickname: &str) -> String {
    nickname.trim().to_string()
}

fn to_rgba(hex: &str, alpha: f32) -> String {
    let normalized = db::normalize_role_color(hex).unwrap_or_else(|| "#94a3b8".to_string());
    let h = normalized.trim_start_matches('#');
    let expanded = if h.len() == 3 {
        let mut out = String::with_capacity(6);
        for ch in h.chars() {
            out.push(ch);
            out.push(ch);
        }
        out
    } else {
        h.to_string()
    };

    if expanded.len() != 6 || !expanded.chars().all(|c| c.is_ascii_hexdigit()) {
        return "rgba(148,163,184,0.15)".to_string();
    }

    let r = u8::from_str_radix(&expanded[0..2], 16).unwrap_or(148);
    let g = u8::from_str_radix(&expanded[2..4], 16).unwrap_or(163);
    let b = u8::from_str_radix(&expanded[4..6], 16).unwrap_or(184);
    format!("rgba({r},{g},{b},{alpha})")
}

async fn first_allowed_admin_tab(state: &AppState, role: &str) -> String {
    let preferred = [
        "overview",
        "containers",
        "users",
        "roles",
        "images",
        "audit",
        "settings",
    ];

    for tab in preferred {
        let permission = auth::permission_for_admin_tab(tab);
        if auth::role_has_read_permission(state, role, permission).await {
            return tab.to_string();
        }
    }

    "overview".to_string()
}

async fn build_admin_template(state: &AppState, tab: String, username: String, nonce: String) -> AdminTemplate {
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
                uid: u.uid,
                nickname: u.nickname,
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

    let auth_role = db::find_user_by_username(&state.db, &username)
        .await
        .ok()
        .flatten()
        .map(|u| u.role)
        .unwrap_or_else(|| "user".to_string());

    let mut role_colors = std::collections::HashMap::<String, String>::new();
    match db::list_roles(&state.db).await {
        Ok(rows) => {
            for role in rows {
                let color = db::normalize_role_color(&role.color)
                    .unwrap_or_else(|| default_role_color(&role.name).to_string());
                role_colors.insert(role.name, color);
            }
        }
        Err(e) => {
            error!("build_admin_template list_roles: {}", e);
        }
    }

    let auth_role_color = role_colors
        .get(&auth_role)
        .cloned()
        .unwrap_or_else(|| default_role_color(&auth_role).to_string());
    let root_role_color = role_colors
        .get("root")
        .cloned()
        .unwrap_or_else(|| default_role_color("root").to_string());

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
        auth_role: auth_role.clone(),
        auth_role_color: auth_role_color.clone(),
        auth_role_badge_bg: to_rgba(&auth_role_color, 0.15),
        auth_role_badge_border: to_rgba(&auth_role_color, 0.35),
        root_role_color,
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
        nonce,
        settings_ufw_enabled: db::get_panel_setting_bool(&state.db, "ufw_enabled").await,
        settings_bandwidth_enabled: db::get_panel_setting_bool(&state.db, "bandwidth_enabled").await,
        docker_default_quota: {
            let v = db::get_panel_setting(&state.db, "docker_default_quota").await;
            if v.is_empty() { "15".into() } else { v }
        },
        container_storage_path: db::get_panel_setting(&state.db, "container_storage_path").await,
        settings_storage_unsafe_override: db::get_panel_setting_bool(&state.db, "storage_unsafe_override").await,
        panel_accent: {
            let v = db::get_panel_setting(&state.db, "panel_accent").await;
            if v.is_empty() { "#7c3aed".into() } else { v }
        },
        panel_name: {
            let v = db::get_panel_setting(&state.db, "panel_name").await;
            if v.is_empty() { "Yunexal Panel".into() } else { v }
        },
        settings_sftp_enabled: db::get_panel_setting_bool(&state.db, "sftp_enabled").await,
        settings_sftp_port: {
            let v = db::get_panel_setting(&state.db, "sftp_port").await;
            if v.is_empty() { "2022".into() } else { v }
        },
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

pub async fn admin_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> impl IntoResponse {
    let username = auth::session_username(&jar).unwrap_or_default();
    let role = db::find_user_by_username(&state.db, &username)
        .await
        .ok()
        .flatten()
        .map(|u| u.role)
        .unwrap_or_else(|| "user".to_string());
    let next_tab = first_allowed_admin_tab(&state, &role).await;
    Redirect::permanent(&format!("/admin/{}", next_tab))
}

pub async fn admin_tab_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(tab): Path<String>,
    Extension(CspNonce(nonce)): Extension<CspNonce>,
) -> impl IntoResponse {
    let tab = if VALID_TABS.contains(&tab.as_str()) {
        tab
    } else {
        "overview".to_string()
    };
    let username = auth::session_username(&jar).unwrap_or_default();
    let role = db::find_user_by_username(&state.db, &username)
        .await
        .ok()
        .flatten()
        .map(|u| u.role)
        .unwrap_or_else(|| "user".to_string());

    let required = auth::permission_for_admin_tab(&tab);
    if !auth::role_has_read_permission(&state, &role, required).await {
        let fallback = first_allowed_admin_tab(&state, &role).await;
        return Redirect::to(&format!("/admin/{}", fallback)).into_response();
    }

    render(build_admin_template(&state, tab, username, nonce).await).into_response()
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
    let _ = db::audit_log(&state.db, "admin", "admin.stop_all", "", &format!("{} containers", containers.len()), &ip, &auth::user_agent(&headers)).await;
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
                let _ = db::revoke_all_user_sessions(&state.db, user.id).await;
                let _ = db::audit_log(&state.db, &session_user, "user.change_password", &session_user, "", &ip, &auth::user_agent(&headers)).await;
                (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "ok": true,
                        "force_logout": true,
                        "redirect": "/login",
                    })),
                )
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
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<CreateUserForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let uid = normalize_user_uid(&body.uid);
    let nickname = normalize_user_nickname(&body.nickname);
    let username = body.username.trim();

    if username.is_empty() || body.password.trim().is_empty() || uid.is_empty() || nickname.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "uid, nickname, username and password are required"})),
        );
    }
    if nickname.chars().count() > 24 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Nickname must be at most 24 characters"})),
        );
    }
    let uid_len = uid.chars().count();
    if uid_len < USER_UID_MIN_LEN || uid_len > USER_UID_MAX_LEN {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "UID must be between 9 and 16 characters"})),
        );
    }

    let caller = auth::session_username(&jar).unwrap_or_default();
    let caller_role = db::find_user_by_username(&state.db, &caller)
        .await
        .ok()
        .flatten()
        .map(|u| u.role)
        .unwrap_or_default();

    let role = body.role.trim().to_lowercase();
    if role.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Role is required"})),
        );
    }
    match db::role_exists(&state.db, &role).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Selected role does not exist"})),
            );
        }
        Err(e) => {
            error!("api_create_user role check: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to validate role"})),
            );
        }
    }
    if matches!(role.as_str(), "root" | "admin") && caller_role != "root" {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Only root can create admin/root accounts"})),
        );
    }

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
    match db::create_user(&state.db, &uid, &nickname, username, &hash, &role).await {
        Ok(id) => {
            let _ = db::audit_log(
                &state.db,
                &caller,
                "user.create",
                &format!("{} {}", nickname, uid),
                &format!("username={} role={}", username, role),
                &ip,
                &auth::user_agent(&headers),
            )
            .await;
            (
                StatusCode::OK,
                Json(serde_json::json!({"ok": true, "id": id})),
            )
        }
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            let user_msg = if msg.contains("users.username") || msg.contains("idx_users_username") {
                "Username already exists"
            } else if msg.contains("users.uid") || msg.contains("idx_users_uid") {
                "UID already exists"
            } else if msg.contains("uid must be 9-16") {
                "UID must be between 9 and 16 characters"
            } else if msg.contains("nickname") {
                "Invalid nickname value"
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
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let caller = auth::session_username(&jar).unwrap_or_default();
    let caller_role = db::find_user_by_username(&state.db, &caller)
        .await.ok().flatten().map(|u| u.role).unwrap_or_default();
    match db::find_user_by_id(&state.db, id).await {
        Ok(Some(u)) if u.role == "root" => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": "Cannot delete the root account"})),
            );
        }
        Ok(Some(u)) if u.role == "admin" && caller_role != "root" => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": "Only root can delete admin accounts"})),
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
            let _ = db::audit_log(&state.db, &caller, "user.delete", &format!("uid:{}", id), "", &ip, &auth::user_agent(&headers)).await;
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
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<AdminSetPasswordForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let caller = auth::session_username(&jar).unwrap_or_default();
    let caller_user_id = db::find_user_by_username(&state.db, &caller)
        .await
        .ok()
        .flatten()
        .map(|u| u.id);
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
            let _ = db::revoke_all_user_sessions(&state.db, id).await;
            let _ = db::audit_log(&state.db, &caller, "user.set_password", &format!("uid:{}", id), "", &ip, &auth::user_agent(&headers)).await;
            if caller_user_id == Some(id) {
                return (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "ok": true,
                        "force_logout": true,
                        "redirect": "/login",
                    })),
                );
            }
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

pub async fn api_set_user_role(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<SetUserRoleForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let caller = auth::session_username(&jar).unwrap_or_default();
    let caller_role = db::find_user_by_username(&state.db, &caller)
        .await
        .ok()
        .flatten()
        .map(|u| u.role)
        .unwrap_or_default();

    let next_role = body.role.trim().to_lowercase();
    if next_role.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Role is required"})),
        );
    }
    match db::role_exists(&state.db, &next_role).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Selected role does not exist"})),
            );
        }
        Err(e) => {
            error!("api_set_user_role role check: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to validate role"})),
            );
        }
    }

    let target = match db::find_user_by_id(&state.db, id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "User not found"})),
            );
        }
        Err(e) => {
            error!("api_set_user_role target lookup: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            );
        }
    };

    if target.role == "root" {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Cannot change root role"})),
        );
    }
    if target.role == "admin" && caller_role != "root" {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Only root can modify admin accounts"})),
        );
    }
    if matches!(next_role.as_str(), "admin" | "root") && caller_role != "root" {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Only root can assign admin/root roles"})),
        );
    }
    if target.role == next_role {
        return (
            StatusCode::OK,
            Json(serde_json::json!({"ok": true, "role": next_role})),
        );
    }

    match db::update_user_role(&state.db, id, &next_role).await {
        Ok(_) => {
            let _ = db::audit_log(
                &state.db,
                &caller,
                "user.set_role",
                &format!("{} {}", target.nickname, target.uid),
                &format!("{} -> {}", target.role, next_role),
                &ip,
                &auth::user_agent(&headers),
            )
            .await;
            (
                StatusCode::OK,
                Json(serde_json::json!({"ok": true, "role": next_role})),
            )
        }
        Err(e) => {
            error!("api_set_user_role: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to update user role"})),
            )
        }
    }
}

pub async fn api_list_roles(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let roles = match db::list_roles(&state.db).await {
        Ok(v) => v,
        Err(e) => {
            error!("api_list_roles: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to load roles"})),
            )
                .into_response();
        }
    };

    let all_permissions = db::list_all_role_permissions(&state.db)
        .await
        .unwrap_or_default();
    let all_policy = db::list_all_role_permission_policy(&state.db)
        .await
        .unwrap_or_default();

    let mut role_rows = Vec::with_capacity(roles.len());
    for role in roles {
        let users_count = db::count_users_with_role(&state.db, &role.name)
            .await
            .unwrap_or(0);
        let permissions = all_permissions
            .get(&role.name)
            .cloned()
            .unwrap_or_default();
        let policy = all_policy
            .get(&role.name)
            .cloned()
            .unwrap_or_else(|| default_policy_from_permissions(&permissions));
        role_rows.push(serde_json::json!({
            "name": role.name,
            "description": role.description,
            "color": role.color,
            "is_system": role.is_system != 0,
            "users_count": users_count,
            "permissions": permissions,
            "policy": policy,
        }));
    }

    let catalog: Vec<serde_json::Value> = db::permission_catalog()
        .iter()
        .map(|p| {
            serde_json::json!({
                "key": p.key,
                "label": p.label,
                "description": p.description,
            })
        })
        .collect();

    let groups: Vec<serde_json::Value> = ROLE_GROUPS
        .iter()
        .map(|(name, keys)| {
            serde_json::json!({
                "name": name,
                "permissions": keys,
            })
        })
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "roles": role_rows,
            "permissions": catalog,
            "permission_groups": groups,
        })),
    )
        .into_response()
}

pub async fn role_permissions_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(role_name): Path<String>,
) -> impl IntoResponse {
    let username = auth::session_username(&jar).unwrap_or_default();
    let role_name = role_name.trim().to_lowercase();
    if role_name.is_empty() || role_name == "root" {
        return Redirect::to("/admin/roles").into_response();
    }

    let role = match db::find_user_by_username(&state.db, &username).await {
        Ok(Some(u)) => u.role,
        _ => return Redirect::to("/admin/roles").into_response(),
    };

    if !auth::role_has_read_permission(&state, &role, "roles.manage").await {
        return Redirect::to("/admin").into_response();
    }

    let target = match db::role_exists(&state.db, &role_name).await {
        Ok(true) => role_name,
        _ => return Redirect::to("/admin/roles").into_response(),
    };

    Redirect::to(&format!("/admin/roles#edit:{}", target)).into_response()
}

pub async fn api_create_role(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<CreateRoleForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let caller = auth::session_username(&jar).unwrap_or_default();

    let role_name = body.name.trim().to_lowercase();
    if !db::is_valid_role_name(&role_name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Role name must be 2-32 chars and use a-z, 0-9, '_' or '-'"
            })),
        )
            .into_response();
    }
    if matches!(role_name.as_str(), "root" | "admin" | "user") {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "System role names cannot be reused"})),
        )
            .into_response();
    }

    let description = {
        let raw = body.description.trim();
        if raw.chars().count() > 120 {
            raw.chars().take(120).collect::<String>()
        } else {
            raw.to_string()
        }
    };

    let initial_color = if body.color.trim().is_empty() {
        None
    } else {
        match db::normalize_role_color(&body.color) {
            Some(c) => Some(c),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Role color must be a valid hex color (#rgb or #rrggbb)"})),
                )
                    .into_response();
            }
        }
    };

    match db::create_role(&state.db, &role_name, &description).await {
        Ok(_) => {
            if let Some(color) = initial_color {
                if let Err(e) = db::update_role_color(&state.db, &role_name, &color).await {
                    error!("api_create_role color update: {}", e);
                }
            }
            let _ = db::audit_log(
                &state.db,
                &caller,
                "role.create",
                &role_name,
                &description,
                &ip,
                &auth::user_agent(&headers),
            )
            .await;
            (StatusCode::OK, Json(serde_json::json!({"ok": true, "role": role_name}))).into_response()
        }
        Err(e) => {
            let msg = if e.to_string().contains("UNIQUE") {
                "Role already exists"
            } else {
                "Failed to create role"
            };
            error!("api_create_role: {}", e);
            (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": msg}))).into_response()
        }
    }
}

pub async fn api_set_role_permissions(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(role_name): Path<String>,
    Json(body): Json<SetRolePermissionsForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let caller = auth::session_username(&jar).unwrap_or_default();
    let caller_role = db::find_user_by_username(&state.db, &caller)
        .await
        .ok()
        .flatten()
        .map(|u| u.role)
        .unwrap_or_default();

    let role_name = role_name.trim().to_lowercase();
    if role_name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Role is required"})),
        )
            .into_response();
    }

    let exists = match db::role_exists(&state.db, &role_name).await {
        Ok(v) => v,
        Err(e) => {
            error!("api_set_role_permissions role check: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to validate role"})),
            )
                .into_response();
        }
    };
    if !exists {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Role not found"})),
        )
            .into_response();
    }

    if role_name == "root" {
        if caller_role != "root" {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": "Only root can update root color"})),
            )
                .into_response();
        }

        let root_color = match db::normalize_role_color(&body.color) {
            Some(c) => c,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Root role color must be a valid hex color (#rgb or #rrggbb)"})),
                )
                    .into_response();
            }
        };

        match db::update_role_color(&state.db, "root", &root_color).await {
            Ok(_) => {
                let _ = db::audit_log(
                    &state.db,
                    &caller,
                    "role.color",
                    "root",
                    &format!("root color set to {}", root_color),
                    &ip,
                    &auth::user_agent(&headers),
                )
                .await;
                return (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response();
            }
            Err(e) => {
                error!("api_set_role_permissions root color update: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Failed to update root role color"})),
                )
                    .into_response();
            }
        }
    }

    let is_system = db::is_system_role(&state.db, &role_name).await.unwrap_or(false);
    if is_system && caller_role != "root" {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Only root can modify system roles"})),
        )
            .into_response();
    }

    let mut policy = std::collections::HashMap::<String, String>::new();
    for def in db::permission_catalog() {
        let mode = body
            .permissions
            .get(def.key)
            .map(|v| v.as_str())
            .unwrap_or("none");
        let normalized = match mode {
            "read" => "read",
            "write" => "write",
            _ => "none",
        };
        policy.insert(def.key.to_string(), normalized.to_string());
    }

    if policy.values().any(|m| m != "none") {
        if !matches!(policy.get("admin.access").map(|s| s.as_str()), Some("read" | "write")) {
            policy.insert("admin.access".to_string(), "read".to_string());
        }
        let has_tab = policy
            .iter()
            .any(|(k, v)| k.starts_with("tab.") && (v == "read" || v == "write"));
        if !has_tab {
            policy.insert("tab.overview".to_string(), "read".to_string());
        }
    }

    let color_update = if body.color.trim().is_empty() {
        None
    } else {
        match db::normalize_role_color(&body.color) {
            Some(c) => Some(c),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Role color must be a valid hex color (#rgb or #rrggbb)"})),
                )
                    .into_response();
            }
        }
    };

    match db::replace_role_permission_policy(&state.db, &role_name, &policy).await {
        Ok(_) => {
            if let Some(color) = color_update {
                if let Err(e) = db::update_role_color(&state.db, &role_name, &color).await {
                    error!("api_set_role_permissions color update: {}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "Failed to update role color"})),
                    )
                        .into_response();
                }
            }
            let _ = db::audit_log(
                &state.db,
                &caller,
                "role.permissions",
                &role_name,
                "tri-state policy updated",
                &ip,
                &auth::user_agent(&headers),
            )
            .await;
            (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
        }
        Err(e) => {
            error!("api_set_role_permissions: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to update role permissions"})),
            )
                .into_response()
        }
    }
}

pub async fn api_delete_role(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(role_name): Path<String>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let caller = auth::session_username(&jar).unwrap_or_default();

    let role_name = role_name.trim().to_lowercase();
    if role_name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Role is required"})),
        )
            .into_response();
    }

    if db::is_system_role(&state.db, &role_name).await.unwrap_or(false) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "System roles cannot be deleted"})),
        )
            .into_response();
    }

    let linked_users = db::count_users_with_role(&state.db, &role_name).await.unwrap_or(0);
    if linked_users > 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Reassign users from this role before deleting it"})),
        )
            .into_response();
    }

    match db::delete_role(&state.db, &role_name).await {
        Ok(_) => {
            let _ = db::audit_log(
                &state.db,
                &caller,
                "role.delete",
                &role_name,
                "",
                &ip,
                &auth::user_agent(&headers),
            )
            .await;
            (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
        }
        Err(e) => {
            error!("api_delete_role: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to delete role"})),
            )
                .into_response()
        }
    }
}

// ── Container edit page ───────────────────────────────────────────────────────

pub async fn admin_edit_page(
    State(state): State<AppState>,
    Path(db_id): Path<i64>,
    Extension(CspNonce(nonce)): Extension<CspNonce>,
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

    let current_storage_source = match docker::get_volume_dir(&state.docker, &container.id).await {
        Ok(v) if !v.trim().is_empty() => {
            let raw = v.trim();
            let path = if std::path::Path::new(raw).is_absolute() {
                std::path::PathBuf::from(raw)
            } else {
                docker::volume_dir_to_path(raw)
            };
            path.to_string_lossy().to_string()
        }
        _ => String::new(),
    };
    let current_storage_base = std::path::Path::new(&current_storage_source)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_string();

    let owner_id = db::get_server_owner(&state.db, &container.id)
        .await
        .ok()
        .flatten()
        .unwrap_or(0);

    let disk_limit = full_config
        .labels
        .get("yunexal.disk_limit")
        .map(|v| v.trim().to_string())
        .unwrap_or_default();

    let bandwidth_mbit = docker::get_bandwidth_limit(&state.docker, &container.id)
        .await
        .ok()
        .flatten()
        .map(|v| v.to_string())
        .unwrap_or_default();

    let max_schedule_runs = db::schedules::get_server_max_schedule_runs(&state.db, db_id).await;

    let users: Vec<UserInfo> = db::list_users(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|u| UserInfo {
            id: u.id,
            uid: u.uid,
            nickname: u.nickname,
            username: u.username,
            role: u.role,
            created_at: u.created_at,
        })
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
            disk_limit,
            bandwidth_mbit,
            owner_id,
            max_schedule_runs: max_schedule_runs.to_string(),
        },
        current_storage_source,
        current_storage_base,
        users,
        error: None,
        nonce,
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

    let requested_disk_limit = form.disk_limit.trim().to_lowercase();
    let new_disk_limit = if requested_disk_limit.is_empty() {
        None
    } else if docker::parse_disk_limit(&requested_disk_limit).is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid disk limit format. Use values like 15gb or 500mb."})),
        );
    } else {
        Some(requested_disk_limit)
    };

    let old_disk_limit = old_config
        .labels
        .get("yunexal.disk_limit")
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty());

    let requested_bw = form.bandwidth_mbit.trim();
    let new_bandwidth = if requested_bw.is_empty() {
        None
    } else {
        match requested_bw.parse::<u32>() {
            Ok(v) => Some(v),
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Bandwidth must be a number in Mbit/s."})),
                )
            }
        }
    };

    let old_bandwidth = match docker::get_bandwidth_limit(&state.docker, &full_id).await {
        Ok(v) => v,
        Err(e) => {
            error!("api_admin_edit_container get_bandwidth_limit (non-fatal): {}", e);
            None
        }
    };

    let image_changed = old_config.image.trim() != form.image.trim();
    let ports_changed = sort_lines(&old_config.ports) != sort_lines(&form.ports);
    let env_changed   = sort_lines(&old_config.env)   != sort_lines(&form.env);
    let disk_limit_changed = old_disk_limit.as_deref() != new_disk_limit.as_deref();
    let bandwidth_changed = old_bandwidth != new_bandwidth;
    let needs_recreate = image_changed || ports_changed || env_changed || disk_limit_changed;

    let resources_changed = (old_config.cpu - form.cpu).abs() > 0.001
        || old_config.memory_mb != form.memory_mb;

    let effective_name = if name_changed { new_name.clone() } else { current_db_name.clone() };

    let mut final_container_id = full_id.clone();
    let mut recreated_short: Option<String> = None;

    if needs_recreate {
        let image = form.image.trim().to_string();
        // Pass the existing Docker container name — it's the internal identifier
        let new_id = match docker::recreate_with_updated_config_and_disk_limit(
            &state.docker, &full_id, &image, &form.env,
            &form.ports, form.cpu, form.memory_mb, &docker_name, new_disk_limit.clone(),
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

        final_container_id = new_id.clone();

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
        recreated_short = Some(short.to_string());
    } else {
        // No recreate — update resources + SQLite only (Docker name is internal, not renamed)
        if resources_changed {
            if let Err(e) = docker::update_container_resources(&full_id, form.cpu, form.memory_mb).await {
                error!("api_admin_edit_container update_resources (non-fatal): {}", e);
            }
        }

        if let Err(e) = db::update_server_name_and_owner(&state.db, &full_id, &effective_name, form.owner_id).await {
            error!("api_admin_edit_container update_server_name_and_owner: {}", e);
        }
    }

    if bandwidth_changed {
        if let Err(e) = docker::set_bandwidth_limit(&state.docker, &final_container_id, new_bandwidth).await {
            error!("api_admin_edit_container set_bandwidth_limit (non-fatal): {}", e);
        }
    }

    if disk_limit_changed {
        if let Err(e) = apply_server_disk_limit(&state.docker, db_id, &final_container_id, new_disk_limit.as_deref()).await {
            error!("api_admin_edit_container apply_server_disk_limit (non-fatal): {}", e);
        }
    }

    if form.max_schedule_runs > 0 {
        let _ = db::update_server_max_schedule_runs(&state.db, db_id, form.max_schedule_runs).await;
    }

    let action_detail = if recreated_short.is_some() {
        format!("#{} recreated", db_id)
    } else {
        format!("#{} updated", db_id)
    };
    let _ = db::audit_log(&state.db, "admin", "server.edit", &effective_name, &action_detail, &ip, &auth::user_agent(&headers)).await;

    if let Some(new_short) = recreated_short {
        return (StatusCode::OK, Json(serde_json::json!({"ok": true, "new_id": db_id, "new_short": new_short})));
    }

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

async fn apply_server_disk_limit(
    docker_client: &bollard::Docker,
    server_id: i64,
    container_id: &str,
    disk_limit: Option<&str>,
) -> anyhow::Result<()> {
    let raw_volume = docker::get_volume_dir(docker_client, container_id).await?;
    if raw_volume.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "Could not resolve container volume path for quota update"
        ));
    }

    let volume_path = if std::path::Path::new(raw_volume.trim()).is_absolute() {
        std::path::PathBuf::from(raw_volume.trim())
    } else {
        docker::volume_dir_to_path(raw_volume.trim())
    };

    if let Some(limit_str) = disk_limit {
        let limit_bytes = docker::parse_disk_limit(limit_str)
            .ok_or_else(|| anyhow::anyhow!("Invalid disk_limit value: {}", limit_str))?;

        if docker::ext4_pquota_mount(&volume_path).is_some() {
            docker::apply_ext4_quota(&volume_path, server_id as u32, limit_bytes).await?;
            return Ok(());
        }

        return Err(anyhow::anyhow!(
            "Filesystem for {} is not ext4 with prjquota",
            volume_path.display()
        ));
    }

    if docker::ext4_pquota_mount(&volume_path).is_some() {
        docker::remove_ext4_quota(server_id as u32, &volume_path).await;
    }

    Ok(())
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
            let _ = db::audit_log(&state.db, "admin", "image.delete", &decoded, "", &ip, &auth::user_agent(&headers)).await;
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
            let _ = db::audit_log(&state.db, "admin", "image.pull", &image, "", &ip, &auth::user_agent(&headers)).await;
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
    match db::get_image_env(&state.db, &decoded, &state.db_key).await {
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
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(image_ref): Path<String>,
    Json(body): Json<SetImageEnvForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let actor = auth::session_username(&jar).unwrap_or_default();
    let decoded = urlencoding::decode(&image_ref).unwrap_or(std::borrow::Cow::Borrowed(&image_ref)).into_owned();
    match db::set_image_env(&state.db, &decoded, &body.env, &state.db_key).await {
        Ok(_) => {
            state.cache.remove("images_ts");
            let _ = db::audit_log(&state.db, &actor, "image.env_set", &decoded, "", &ip, &auth::user_agent(&headers)).await;
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
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(image_ref): Path<String>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let actor = auth::session_username(&jar).unwrap_or_default();
    let decoded = urlencoding::decode(&image_ref).unwrap_or(std::borrow::Cow::Borrowed(&image_ref)).into_owned();

    // Collect source tags and env overrides before any mutation
    let src_tags: Vec<String> = docker::get_image_info(&state.docker, &decoded).await
        .ok()
        .and_then(|i| i.repo_tags)
        .unwrap_or_default()
        .into_iter()
        .filter(|t| t != "<none>:<none>")
        .collect();
    let src_env = db::get_image_env(&state.db, &decoded, &state.db_key).await.unwrap_or_default();

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
                if let Err(e) = db::set_image_env(&state.db, &new_id, &src_env, &state.db_key).await {
                    error!("api_duplicate_image: copy env to {}: {}", new_id, e);
                }
            }

            state.cache.remove("images_ts");
            let _ = db::audit_log(&state.db, &actor, "image.duplicate", &decoded, &format!("new_id={}", new_id), &ip, &auth::user_agent(&headers)).await;
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
    jar: PrivateCookieJar,
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

    let _ = db::audit_log(&state.db, &actor, "panel.update", "", &format!("url={download_url}"), &ip, &auth::user_agent(&headers)).await;

    let client = match reqwest::Client::builder()
        .user_agent("yunexal-panel")
        .timeout(std::time::Duration::from_secs(120))
        .build()
    {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    };

    // Download to a temp file
    let mut resp = match client.get(download_url).send().await {
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

    // Stream to disk with a strict size cap to avoid memory spikes / abuse.
    const MAX_UPDATE_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
    if let Some(len) = resp.content_length() {
        if len == 0 {
            return Json(serde_json::json!({"ok": false, "error": "Downloaded archive is empty"}));
        }
        if len > MAX_UPDATE_ARCHIVE_BYTES {
            return Json(serde_json::json!({
                "ok": false,
                "error": format!("Archive is too large ({} bytes > {} bytes)", len, MAX_UPDATE_ARCHIVE_BYTES)
            }));
        }
    }

    let mut archive_file = match tokio::fs::File::create(&tmp_archive).await {
        Ok(f) => f,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("Write failed: {e}")})),
    };

    use tokio::io::AsyncWriteExt;
    let mut downloaded: u64 = 0;
    loop {
        let next = match resp.chunk().await {
            Ok(v) => v,
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp_archive).await;
                return Json(serde_json::json!({"ok": false, "error": format!("Download read failed: {e}")}));
            }
        };
        let Some(chunk) = next else { break; };

        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > MAX_UPDATE_ARCHIVE_BYTES {
            let _ = tokio::fs::remove_file(&tmp_archive).await;
            return Json(serde_json::json!({
                "ok": false,
                "error": format!("Archive exceeds maximum allowed size ({} bytes)", MAX_UPDATE_ARCHIVE_BYTES)
            }));
        }

        if let Err(e) = archive_file.write_all(&chunk).await {
            let _ = tokio::fs::remove_file(&tmp_archive).await;
            return Json(serde_json::json!({"ok": false, "error": format!("Write failed: {e}")}));
        }
    }

    if downloaded == 0 {
        let _ = tokio::fs::remove_file(&tmp_archive).await;
        return Json(serde_json::json!({"ok": false, "error": "Downloaded archive is empty"}));
    }

    if let Err(e) = archive_file.flush().await {
        let _ = tokio::fs::remove_file(&tmp_archive).await;
        return Json(serde_json::json!({"ok": false, "error": format!("Flush failed: {e}")}));
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

    let new_bin_path = match new_binary {
        Some(path) => path,
        None => {
            let _ = tokio::fs::remove_dir_all(&tmp_extract).await;
            return Json(serde_json::json!({"ok": false, "error": "yunexal-panel binary not found in archive"}));
        }
    };

    // Backup current binary (hard-link copy; not the running inode so no ETXTBSY)
    let backup_path = parent_dir.join("yunexal-panel.bak");
    if let Err(e) = tokio::fs::copy(&current_exe, &backup_path).await {
        let _ = tokio::fs::remove_dir_all(&tmp_extract).await;
        return Json(serde_json::json!({"ok": false, "error": format!("Backup failed: {e}")}));
    }

    // Stage the new binary next to the running one, set permissions, then
    // atomically rename it into place.  rename(2) replaces the directory
    // entry without opening the existing inode, so it works on a running binary.
    let staged = parent_dir.join(".yunexal-panel.new");
    if let Err(e) = tokio::fs::copy(&new_bin_path, &staged).await {
        let _ = tokio::fs::remove_dir_all(&tmp_extract).await;
        return Json(serde_json::json!({"ok": false, "error": format!("Stage failed: {e}")}));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)) {
            error!("Failed to set staged binary permissions: {}", e);
        }
    }
    if let Err(e) = tokio::fs::rename(&staged, &current_exe).await {
        // Restore from backup
        let _ = tokio::fs::copy(&backup_path, &current_exe).await;
        let _ = tokio::fs::remove_file(&staged).await;
        let _ = tokio::fs::remove_dir_all(&tmp_extract).await;
        return Json(serde_json::json!({"ok": false, "error": format!("Replace failed: {e}")}));
    }

    // Also update setup binary if present
    if let Some(setup_path) = new_setup {
        let setup_dest = parent_dir.join("yunexal-setup");
        let staged_setup = parent_dir.join(".yunexal-setup.new");
        if tokio::fs::copy(&setup_path, &staged_setup).await.is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&staged_setup, std::fs::Permissions::from_mode(0o755));
            }
            let _ = tokio::fs::rename(&staged_setup, &setup_dest).await;
        }
    }

    let _ = tokio::fs::remove_dir_all(&tmp_extract).await;

    let _ = db::audit_log(&state.db, &actor, "panel.updated", "", "binary replaced, restarting", &ip, &auth::user_agent(&headers)).await;

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
    let root = tokio::fs::canonicalize(dir).await.ok()?;
    let mut stack = vec![root.clone()];
    while let Some(d) = stack.pop() {
        let mut rd = match tokio::fs::read_dir(&d).await {
            Ok(r) => r,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            let path = entry.path();
            let meta = match tokio::fs::symlink_metadata(&path).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            let file_type = meta.file_type();
            if file_type.is_symlink() {
                continue;
            }

            if file_type.is_dir() {
                if let Ok(canon_dir) = tokio::fs::canonicalize(&path).await {
                    if canon_dir.starts_with(&root) {
                        stack.push(canon_dir);
                    }
                }
                continue;
            }

            if file_type.is_file() && path.file_name().and_then(|n| n.to_str()) == Some(name) {
                if let Ok(canon_file) = tokio::fs::canonicalize(&path).await {
                    if canon_file.starts_with(&root) {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

// ── Panel settings API ────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct AdminSetSettingBody {
    pub key: String,
    pub value: String,
}

pub async fn api_admin_set_setting(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<AdminSetSettingBody>,
) -> impl IntoResponse {
    // Only root can change panel settings
    if !auth::is_root_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Root access required"}))).into_response();
    }
    // Allowlist of mutable keys
    const BOOL_KEYS: &[&str] = &[
        "ufw_enabled", "bandwidth_enabled",
        "storage_unsafe_override",
        "sftp_enabled",
    ];
    const STR_KEYS: &[&str] = &[
        "docker_default_quota",
        "container_storage_path",
        "service_api_key",
        "panel_accent",
        "panel_name",
        "sftp_port",
    ];
    let is_bool = BOOL_KEYS.contains(&body.key.as_str());
    let is_str  = STR_KEYS.contains(&body.key.as_str());
    if !is_bool && !is_str {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Unknown setting key"}))).into_response();
    }
    // Boolean keys only accept "0" or "1"
    if is_bool && body.value != "0" && body.value != "1" {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Value must be '0' or '1'"}))).into_response();
    }

    let storage_unsafe_override = db::get_panel_setting_bool(&state.db, "storage_unsafe_override").await;

    if body.key == "container_storage_path" && !storage_unsafe_override {
        let raw = body.value.trim();
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let candidate = if raw.is_empty() {
            cwd.join("volumes")
        } else {
            let p = std::path::PathBuf::from(raw);
            if p.is_absolute() { p } else { cwd.join(p) }
        };
        if let Err(reason) = validate_storage_base_path(&candidate).await {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "ok": false,
                    "error": reason,
                    "path": candidate,
                })),
            )
                .into_response();
        }
    }

    let result = if body.key == "service_api_key" {
        db::set_service_api_key(&state.db, &body.value, &state.db_key).await
    } else {
        db::set_panel_setting(&state.db, &body.key, &body.value).await
    };
    match result {
        Ok(_) => {
            let ip = auth::client_ip(&headers, addr);
            let _ = db::audit_log(&state.db, "admin", "panel.setting", &body.key, &body.value, &ip, &auth::user_agent(&headers)).await;
            Json(serde_json::json!({"ok": true})).into_response()
        }
        Err(e) => {
            error!("api_admin_set_setting: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response()
        }
    }
}

// ── Storage stats API ────────────────────────────────────────────────────────

/// GET /api/admin/storage/stats
/// Returns disk usage for the system partition and the Docker container partition.
pub async fn api_admin_storage_stats(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> impl IntoResponse {
    if !auth::is_root_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Root access required"}))).into_response();
    }

    // Parse `df -B1 <path>` to get (total_bytes, used_bytes, free_bytes)
    async fn df_info(path: &str) -> Option<(u64, u64, u64)> {
        let out = tokio::process::Command::new("df")
            .args(["-B1", "--output=size,used,avail", path])
            .output()
            .await
            .ok()?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        // skip header line
        let line = stdout.lines().nth(1)?;
        let mut parts = line.split_whitespace();
        let total: u64 = parts.next()?.parse().ok()?;
        let used:  u64 = parts.next()?.parse().ok()?;
        let free:  u64 = parts.next()?.parse().ok()?;
        Some((total, used, free))
    }

    let gib = |b: u64| format!("{:.1}", b as f64 / 1_073_741_824.0);
    let pct = |used: u64, total: u64| -> u64 {
        if total == 0 { 0 } else { used * 100 / total }
    };

    let sys    = df_info("/").await;
    let docker = df_info("/var/lib/docker").await;

    let sys_json = sys.map(|(tot, used, free)| serde_json::json!({
        "mount": "/", "label": "System (sda1)",
        "total_gib": gib(tot), "used_gib": gib(used), "free_gib": gib(free),
        "pct": pct(used, tot),
    })).unwrap_or(serde_json::json!({"error": "unavailable"}));

    let docker_json = docker.map(|(tot, used, free)| serde_json::json!({
        "mount": "/var/lib/docker", "label": "Containers (NVMe)",
        "total_gib": gib(tot), "used_gib": gib(used), "free_gib": gib(free),
        "pct": pct(used, tot),
    })).unwrap_or(serde_json::json!({"error": "unavailable"}));

    let current_quota = {
        let v = db::get_panel_setting(&state.db, "docker_default_quota").await;
        if v.is_empty() { "15".to_string() } else { v }
    };

    Json(serde_json::json!({
        "ok": true,
        "system": sys_json,
        "docker": docker_json,
        "current_quota_gb": current_quota,
    })).into_response()
}

// ── Docker daemon.json management ────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct DockerDaemonForm {
    pub default_quota_gb: u32,
}

/// POST /api/admin/storage/daemon
/// Updates overlay2.size in /etc/docker/daemon.json and restarts the Docker daemon.
pub async fn api_admin_docker_daemon(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<DockerDaemonForm>,
) -> impl IntoResponse {
    if !auth::is_root_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Root access required"}))).into_response();
    }

    let quota_gb = body.default_quota_gb;
    if quota_gb < 1 || quota_gb > 900 {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Quota must be between 1 and 900 GB"}))).into_response();
    }

    let daemon_path = "/etc/docker/daemon.json";
    let existing_raw = tokio::fs::read_to_string(daemon_path).await.unwrap_or_else(|_| "{}".to_string());
    let mut daemon_cfg: serde_json::Value = serde_json::from_str(&existing_raw).unwrap_or(serde_json::json!({}));

    let size_str = format!("overlay2.size={}G", quota_gb);
    let opts = daemon_cfg
        .get("storage-opts")
        .and_then(|v| v.as_array())
        .map(|arr| {
            let mut filtered: Vec<serde_json::Value> = arr
                .iter()
                .filter(|v| !v.as_str().map_or(false, |s| s.starts_with("overlay2.size=")))
                .cloned()
                .collect();
            filtered.push(serde_json::Value::String(size_str.clone()));
            filtered
        })
        .unwrap_or_else(|| vec![serde_json::Value::String(size_str.clone())]);

    daemon_cfg["storage-driver"] = serde_json::Value::String("overlay2".to_string());
    daemon_cfg["storage-opts"]   = serde_json::Value::Array(opts);

    let new_json = match serde_json::to_string_pretty(&daemon_cfg) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("JSON error: {e}")}))).into_response(),
    };

    // Write through a privileged tee command so direct-root mode also works on custom ISO.
    let write_ok = {
        let tee_cmd = host::resolve_admin_tool("tee");
        let spawn_result = host::privileged_command(&tee_cmd)
            .arg(daemon_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn();
        match spawn_result {
            Ok(mut child) => {
                use tokio::io::AsyncWriteExt;
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(new_json.as_bytes()).await;
                    // Drop stdin to signal EOF to tee
                }
                child.wait().await.map(|s| s.success()).unwrap_or(false)
            }
            Err(_) => {
                // Fall back: try direct write (works if panel runs as root)
                tokio::fs::write(daemon_path, &new_json).await.is_ok()
            }
        }
    };

    if !write_ok {
        let user = std::env::var("USER").unwrap_or_else(|_| "yunexal".into());
        let tee_entry = format!(
            "{} {}",
            host::resolve_command_path("tee", &["/usr/bin/tee", "/bin/tee"]),
            daemon_path
        );
        let fix = host::sudoers_fix_command(
            &user,
            "/etc/sudoers.d/yunexal-docker",
            &[tee_entry, host::docker_restart_sudoers_entry()],
        );
        return Json(serde_json::json!({"ok": false, "needs_permission": true, "fix_command": fix, "message": "Need sudo permission to write daemon.json"})).into_response();
    }

    let _ = db::set_panel_setting(&state.db, "docker_default_quota", &quota_gb.to_string()).await;

    let (restart_program, restart_args) = host::docker_restart_command_parts();
    let restart_out = host::privileged_command(&restart_program)
        .args(&restart_args)
        .output()
        .await;

    let ip = auth::client_ip(&headers, addr);
    let _ = db::audit_log(&state.db, "admin", "storage.daemon_updated", "docker", &format!("overlay2.size={}G", quota_gb), &ip, &auth::user_agent(&headers)).await;

    match restart_out {
        Ok(out) if out.status.success() => {
            Json(serde_json::json!({"ok": true, "message": format!("Docker daemon updated to {}G quota and restarted.", quota_gb)})).into_response()
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("password is required") || stderr.contains("not allowed") {
                let user = std::env::var("USER").unwrap_or_else(|_| "yunexal".into());
                let fix = host::sudoers_fix_command(
                    &user,
                    "/etc/sudoers.d/yunexal-docker",
                    &[host::docker_restart_sudoers_entry()],
                );
                Json(serde_json::json!({"ok": false, "needs_permission": true, "fix_command": fix, "message": "daemon.json updated but Docker needs sudo permission to restart"})).into_response()
            } else {
                Json(serde_json::json!({"ok": false, "error": format!("Restart failed: {}", stderr.trim())})).into_response()
            }
        }
        Err(e) => {
            Json(serde_json::json!({
                "ok": false,
                "error": format!(
                    "Cannot run '{}': {e}. daemon.json was updated.",
                    host::docker_restart_display_command()
                )
            })).into_response()
        }
    }
}

// ── UFW management (root-only) ───────────────────────────────────────────────

fn ufw_fix_command() -> String {
    let user = std::env::var("USER").unwrap_or_else(|_| "yunexal".into());
    host::sudoers_fix_command(
        &user,
        "/etc/sudoers.d/yunexal-ufw",
        &[host::ufw_sudoers_entry()],
    )
}

/// GET /api/admin/ufw/status — returns whether UFW is active on the host
pub async fn api_ufw_status(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> impl IntoResponse {
    if !auth::is_root_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Root access required"}))).into_response();
    }
    let ufw_cmd = host::resolve_admin_tool("ufw");
    match host::privileged_command(&ufw_cmd).arg("status").output().await {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let active = stdout.contains("Status: active");
            Json(serde_json::json!({"ok": true, "active": active, "output": stdout.trim()})).into_response()
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("password is required") || stderr.contains("Permission denied") || stderr.contains("not allowed") {
                Json(serde_json::json!({"ok": false, "needs_permission": true, "fix_command": ufw_fix_command()})).into_response()
            } else {
                Json(serde_json::json!({"ok": true, "active": false, "output": stderr.trim()})).into_response()
            }
        }
        Err(_) => {
            Json(serde_json::json!({"ok": true, "active": false, "output": "ufw not found"})).into_response()
        }
    }
}

#[derive(serde::Deserialize)]
pub struct UfwToggleBody {
    pub enable: bool,
}

/// POST /api/admin/ufw/toggle — enable or disable UFW on the host
pub async fn api_ufw_toggle(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<UfwToggleBody>,
) -> impl IntoResponse {
    if !auth::is_root_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Root access required"}))).into_response();
    }
    let ufw_cmd = host::resolve_admin_tool("ufw");
    let args: Vec<&str> = if body.enable { vec!["--force", "enable"] } else { vec!["disable"] };
    match host::privileged_command(&ufw_cmd).args(&args).output().await {
        Ok(out) if out.status.success() => {
            let ip = auth::client_ip(&headers, addr);
            let actor = auth::session_username(&jar).unwrap_or_default();
            let action_detail = if body.enable { "enabled" } else { "disabled" };
            let _ = db::audit_log(&state.db, &actor, "panel.ufw_toggle", "ufw", action_detail, &ip, &auth::user_agent(&headers)).await;
            Json(serde_json::json!({"ok": true, "enabled": body.enable})).into_response()
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("password is required") || stderr.contains("Permission denied") || stderr.contains("not allowed") {
                Json(serde_json::json!({"ok": false, "needs_permission": true, "fix_command": ufw_fix_command()})).into_response()
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("ufw failed: {}", stderr.trim())}))).into_response()
            }
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Failed to run ufw: {}", e)}))).into_response()
        }
    }
}

// ── Storage mounts API ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
struct LsblkRoot {
    #[serde(default)]
    blockdevices: Vec<LsblkNode>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct LsblkNode {
    #[serde(default)]
    kname: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    fstype: String,
    #[serde(default)]
    mountpoint: String,
    #[serde(default, rename = "type")]
    dev_type: String,
    #[serde(default)]
    pkname: String,
    #[serde(default)]
    size: String,
    #[serde(default)]
    children: Vec<LsblkNode>,
}

#[derive(Debug, Clone)]
struct DiskCandidate {
    device: String,
    kname: String,
    parent: String,
    fs_type: String,
    mountpoint: String,
    size: String,
}

fn storage_sudo_fix_command() -> String {
    let user = std::env::var("USER").unwrap_or_else(|_| "yunexal".into());
    let entries = vec![
        host::resolve_command_path("umount", &["/usr/bin/umount", "/bin/umount"]),
        host::resolve_command_path("mount", &["/usr/bin/mount", "/bin/mount"]),
        host::resolve_command_path("rsync", &["/usr/bin/rsync", "/bin/rsync"]),
        host::resolve_command_path("cp", &["/usr/bin/cp", "/bin/cp"]),
        host::resolve_command_path("mkdir", &["/usr/bin/mkdir", "/bin/mkdir"]),
        host::resolve_command_path("stat", &["/usr/bin/stat", "/bin/stat"]),
        host::resolve_command_path("chown", &["/usr/bin/chown", "/bin/chown"]),
        host::resolve_command_path("chmod", &["/usr/bin/chmod", "/bin/chmod"]),
        host::resolve_command_path("mkfs.ext4", &["/usr/sbin/mkfs.ext4", "/sbin/mkfs.ext4"]),
    ];
    host::sudoers_fix_command(&user, "/etc/sudoers.d/yunexal-storage", &entries)
}

fn looks_like_permission_issue(stderr: &str) -> bool {
    stderr.contains("password is required")
        || stderr.contains("Permission denied")
        || stderr.contains("not allowed")
}

async fn sync_volume_root_metadata(
    source: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let (uid, gid, mode) = match tokio::fs::metadata(source).await {
        Ok(src_meta) => (
            src_meta.uid(),
            src_meta.gid(),
            src_meta.permissions().mode() & 0o7777,
        ),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            let source_str = source.to_string_lossy().to_string();
            let stat_out = run_sudo_command(vec![
                "stat".to_string(),
                "-c".to_string(),
                "%u:%g:%a".to_string(),
                source_str,
            ])
            .await;

            match stat_out {
                Ok(o) if o.status.success() => {
                    let raw = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    let parts: Vec<&str> = raw.split(':').collect();
                    if parts.len() != 3 {
                        return Err(format!(
                            "Cannot parse source volume metadata '{}': {}",
                            source.display(),
                            raw
                        ));
                    }
                    let uid = parts[0]
                        .parse::<u32>()
                        .map_err(|err| format!("Invalid uid from stat output '{}': {}", raw, err))?;
                    let gid = parts[1]
                        .parse::<u32>()
                        .map_err(|err| format!("Invalid gid from stat output '{}': {}", raw, err))?;
                    let mode = u32::from_str_radix(parts[2], 8)
                        .map_err(|err| format!("Invalid mode from stat output '{}': {}", raw, err))?;
                    (uid, gid, mode & 0o7777)
                }
                Ok(o) => {
                    return Err(format!(
                        "Cannot inspect source volume metadata '{}': {}",
                        source.display(),
                        String::from_utf8_lossy(&o.stderr).trim()
                    ));
                }
                Err(err) => return Err(err),
            }
        }
        Err(e) => {
            return Err(format!(
                "Cannot inspect source volume metadata '{}': {}",
                source.display(),
                e
            ));
        }
    };

    let target_str = target.to_string_lossy().to_string();

    let chown_out = run_sudo_command(vec![
        "chown".to_string(),
        format!("{}:{}", uid, gid),
        target_str.clone(),
    ])
    .await;

    match chown_out {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            return Err(format!(
                "Cannot set owner on migrated volume '{}': {}",
                target.display(),
                String::from_utf8_lossy(&o.stderr).trim()
            ));
        }
        Err(e) => return Err(e),
    }

    let chmod_out = run_sudo_command(vec![
        "chmod".to_string(),
        format!("{:o}", mode),
        target_str,
    ])
    .await;

    match chmod_out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!(
            "Cannot set mode on migrated volume '{}': {}",
            target.display(),
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => Err(e),
    }
}

fn is_critical_system_mount(mount: &str) -> bool {
    if mount == "/" {
        return true;
    }
    ["/boot", "/efi", "/usr", "/var", "/etc", "/bin", "/sbin", "/lib", "/lib64"]
        .iter()
        .any(|p| mount == *p || mount.starts_with(&format!("{}/", p.trim_end_matches('/'))))
}

async fn device_top_parent_kname(device: &str) -> String {
    if !device.starts_with("/dev/") {
        return String::new();
    }

    let mut current = device.to_string();
    let mut top = std::path::Path::new(device)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    for _ in 0..8 {
        let parent = tokio::process::Command::new("lsblk")
            .args(["-no", "PKNAME", &current])
            .output()
            .await
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        if parent.is_empty() {
            break;
        }
        top = parent.clone();
        current = format!("/dev/{}", parent);
    }

    top
}

async fn root_disk_info() -> (String, String) {
    let root_source = tokio::process::Command::new("findmnt")
        .args(["-n", "-o", "SOURCE", "/"])
        .output()
        .await
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    if root_source.is_empty() {
        return (String::new(), String::new());
    }

    let top_kname = device_top_parent_kname(&root_source).await;

    (root_source, top_kname)
}

async fn validate_storage_base_path(path: &std::path::Path) -> Result<(), String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(path)
    };

    let probe = if absolute.exists() {
        absolute.clone()
    } else {
        absolute
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("/"))
    };

    let out = tokio::process::Command::new("findmnt")
        .args(["-n", "-T", &probe.to_string_lossy(), "-o", "SOURCE,FSTYPE,OPTIONS,TARGET"])
        .output()
        .await
        .map_err(|e| format!("Failed to inspect storage mount: {}", e))?;

    if !out.status.success() {
        return Err(format!(
            "Cannot validate storage path '{}': {}",
            absolute.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return Err(format!("Cannot parse mount info for '{}'", absolute.display()));
    }
    let source = parts[0].to_string();
    let fs_type = parts[1].to_string();
    let opts = parts[2].to_string();
    let mount_point = parts[3].to_string();

    if is_critical_system_mount(&mount_point) {
        return Err(format!(
            "Storage path '{}' is on critical system mount '{}' and is forbidden",
            absolute.display(),
            mount_point
        ));
    }

    if fs_type != "ext4" {
        return Err(format!(
            "Only ext4 with prjquota is supported for container storage (mount '{}' uses '{}')",
            mount_point,
            fs_type
        ));
    }

    if !(opts.contains("prjquota") || opts.contains("prjjquota")) {
        return Err(format!(
            "ext4 without prjquota is forbidden for container storage (mount: '{}')",
            mount_point
        ));
    }

    let (root_source, root_disk_kname) = root_disk_info().await;
    if !root_source.is_empty() {
        if source == root_source {
            return Err(format!(
                "Storage path '{}' is on the system disk ('{}') and is forbidden",
                absolute.display(),
                root_source
            ));
        }
        if source.starts_with("/dev/") && !root_disk_kname.is_empty() {
            let src_top = device_top_parent_kname(&source).await;
            if !src_top.is_empty() && src_top == root_disk_kname {
                return Err(format!(
                    "Storage path '{}' is on system root disk '{}' and is forbidden",
                    absolute.display(),
                    root_disk_kname
                ));
            }
        }
    }

    Ok(())
}

fn collect_partitions(nodes: &[LsblkNode], out: &mut Vec<DiskCandidate>) {
    for n in nodes {
        if n.dev_type == "part" && n.path.starts_with("/dev/") {
            out.push(DiskCandidate {
                device: n.path.clone(),
                kname: n.kname.clone(),
                parent: n.pkname.clone(),
                fs_type: n.fstype.clone(),
                mountpoint: n.mountpoint.clone(),
                size: n.size.clone(),
            });
        }
        if !n.children.is_empty() {
            collect_partitions(&n.children, out);
        }
    }
}

async fn list_non_system_disk_candidates() -> Result<Vec<DiskCandidate>, String> {
    let (root_source, root_disk_kname) = root_disk_info().await;

    let out = tokio::process::Command::new("lsblk")
        .args(["-J", "-o", "KNAME,PATH,FSTYPE,MOUNTPOINT,TYPE,PKNAME,SIZE"])
        .output()
        .await
        .map_err(|e| format!("lsblk failed: {}", e))?;

    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }

    let parsed: LsblkRoot = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("lsblk parse error: {}", e))?;

    let mut all_parts = Vec::new();
    collect_partitions(&parsed.blockdevices, &mut all_parts);

    let filtered: Vec<DiskCandidate> = all_parts
        .into_iter()
        .filter(|d| {
            d.device != root_source
                && !d.mountpoint.trim().eq("/")
                && (root_disk_kname.is_empty() || d.parent != root_disk_kname)
        })
        .collect();

    Ok(filtered)
}

/// GET /api/admin/storage/mounts
/// Returns a list of ext4 mount points available on the host.
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct StorageMountsQuery {
    #[serde(default)]
    pub include_all: bool,
}

pub async fn api_admin_storage_mounts(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(query): Query<StorageMountsQuery>,
) -> impl IntoResponse {
    if !auth::is_root_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Root access required"}))).into_response();
    }

    // df helper: returns (free_gib, total_gib, used_pct)
    async fn df_stats(path: &str) -> Option<(f64, f64, u64)> {
        let out = tokio::process::Command::new("df")
            .args(["-B1", "--output=size,used,avail", path])
            .output().await.ok()?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let line = stdout.lines().nth(1)?;
        let mut parts = line.split_whitespace();
        let total: u64 = parts.next()?.parse().ok()?;
        let used:  u64 = parts.next()?.parse().ok()?;
        let free:  u64 = parts.next()?.parse().ok()?;
        let pct = if total > 0 { used * 100 / total } else { 0 };
        Some((free as f64 / 1_073_741_824.0, total as f64 / 1_073_741_824.0, pct))
    }

    let unsafe_override = db::get_panel_setting_bool(&state.db, "storage_unsafe_override").await;
    let include_all = query.include_all;

    let mounts_raw = tokio::fs::read_to_string("/proc/mounts").await.unwrap_or_default();
    let mut mounts: Vec<serde_json::Value> = Vec::new();
    let (root_source, root_disk_kname) = root_disk_info().await;

    // Deduplicate by mount point (proc/mounts can have multiple entries)
    let mut seen: std::collections::HashSet<String> = Default::default();

    for line in mounts_raw.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 { continue; }
        let device_path = parts[0];
        let mount_point = parts[1];
        let fs_type     = parts[2];
        let opts        = parts[3];

        if !device_path.starts_with("/dev/") { continue; }
        if !seen.insert(mount_point.to_string()) { continue; }

        let is_ext4 = fs_type == "ext4";

        let device = std::path::Path::new(device_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(device_path)
            .to_string();

        let has_prjquota = opts.contains("prjquota") || opts.contains("prjjquota") || opts.contains("pquota");
        let is_critical_mount = is_critical_system_mount(mount_point);

        let mut same_root_disk = false;
        if !root_source.is_empty() {
            if device_path == root_source {
                same_root_disk = true;
            } else if !root_disk_kname.is_empty() {
                let top = device_top_parent_kname(device_path).await;
                if !top.is_empty() && top == root_disk_kname {
                    same_root_disk = true;
                }
            }
        }

        let mut blocked_reason = String::new();
        if !is_ext4 {
            blocked_reason = format!(
                "unsupported filesystem '{}' (only ext4 with prjquota is allowed)",
                fs_type
            );
        } else if !has_prjquota {
            blocked_reason = "missing prjquota/prjjquota mount option".to_string();
        } else if is_critical_mount {
            blocked_reason = format!("critical system mount '{}'", mount_point);
        } else if same_root_disk {
            blocked_reason = "mounted on system root disk".to_string();
        }

        let selectable = if unsafe_override {
            is_ext4
        } else {
            blocked_reason.is_empty()
        };

        if !include_all {
            if !is_ext4 {
                continue;
            }
            if !unsafe_override && !selectable {
                continue;
            }
        }

        let (free_gib, total_gib, used_pct) = df_stats(mount_point).await.unwrap_or((0.0, 0.0, 0));

        let suggested_path = if mount_point == "/var/lib/docker" {
            "/var/lib/docker/yunexal-volumes".to_string()
        } else if mount_point == "/" {
            "/var/lib/docker/yunexal-volumes".to_string()
        } else {
            format!("{}/yunexal-volumes", mount_point)
        };

        mounts.push(serde_json::json!({
            "device":         device,
            "mount":          mount_point,
            "fs_type":        fs_type,
            "has_prjquota":   has_prjquota,
            "has_ext4":       is_ext4,
            "free_gib":       format!("{:.1}", free_gib),
            "total_gib":      format!("{:.1}", total_gib),
            "used_pct":       used_pct,
            "suggested_path": suggested_path,
            "prjquota_hint": String::new(),
            "selectable": selectable,
            "blocked_reason": blocked_reason,
        }));
    }

    let current_path = db::get_panel_setting(&state.db, "container_storage_path").await;
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let default_base_raw = if current_path.trim().is_empty() {
        cwd.join("volumes")
    } else {
        let p = std::path::PathBuf::from(current_path.trim());
        if p.is_absolute() { p } else { cwd.join(p) }
    };
    let (default_allowed, default_reason) = if unsafe_override {
        (true, String::new())
    } else {
        let default_validation = validate_storage_base_path(&default_base_raw).await;
        (default_validation.is_ok(), default_validation.err().unwrap_or_default())
    };

    Json(serde_json::json!({
        "ok": true,
        "mounts": mounts,
        "current_path": current_path,
        "default_allowed": default_allowed,
        "default_reason": default_reason,
        "default_path": default_base_raw.to_string_lossy(),
        "unsafe_override": unsafe_override,
    })).into_response()
}

#[derive(serde::Deserialize)]
pub struct StorageFsChangeBody {
    pub device: String,
    pub fs_type: String,
    pub confirm_phrase: String,
}

#[derive(serde::Deserialize)]
pub struct StorageMigrateBody {
    pub server_id: i64,
    pub target_base_path: String,
}

async fn run_sudo_command(args: Vec<String>) -> Result<std::process::Output, String> {
    let program = args
        .first()
        .cloned()
        .ok_or_else(|| "failed to run privileged command: empty args".to_string())?;
    let program = host::resolve_admin_tool(&program);
    let rest: Vec<String> = args.into_iter().skip(1).collect();

    host::privileged_command(&program)
        .args(&rest)
        .output()
        .await
        .map_err(|e| format!("failed to run privileged command: {}", e))
}

/// GET /api/admin/storage/disks
/// Returns non-system block partitions that can be reformatted or selected for storage operations.
pub async fn api_admin_storage_disks(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> impl IntoResponse {
    if !auth::is_root_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Root access required"}))).into_response();
    }

    let unsafe_override = db::get_panel_setting_bool(&state.db, "storage_unsafe_override").await;

    match list_non_system_disk_candidates().await {
        Ok(disks) => {
            let list: Vec<serde_json::Value> = disks
                .into_iter()
                .map(|d| {
                    serde_json::json!({
                        "device": d.device,
                        "kname": d.kname,
                        "parent": d.parent,
                        "fs_type": d.fs_type,
                        "mountpoint": d.mountpoint,
                        "size": d.size,
                    })
                })
                .collect();

            Json(serde_json::json!({
                "ok": true,
                "disks": list,
                "unsafe_override": unsafe_override,
            }))
            .into_response()
        }
        Err(e) => Json(serde_json::json!({"ok": false, "error": e})).into_response(),
    }
}

/// POST /api/admin/storage/change-fs
/// Reformats a non-system partition to ext4.
pub async fn api_admin_storage_change_fs(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<StorageFsChangeBody>,
) -> impl IntoResponse {
    if !auth::is_root_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Root access required"}))).into_response();
    }

    let fs_type = body.fs_type.trim().to_lowercase();
    if fs_type != "ext4" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Unsupported filesystem. Only ext4 is supported."})),
        )
            .into_response();
    }

    let disks = match list_non_system_disk_candidates().await {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Cannot enumerate disks: {}", e)})),
            )
                .into_response();
        }
    };

    let Some(disk) = disks.iter().find(|d| d.device == body.device) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Device is not eligible for filesystem change."})),
        )
            .into_response();
    };

    let expected_confirm = format!("FORMAT {}", disk.device);
    if body.confirm_phrase.trim() != expected_confirm {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Confirmation phrase mismatch",
                "expected": expected_confirm,
            })),
        )
            .into_response();
    }

    if disk.mountpoint.starts_with('/') {
        let unmount = run_sudo_command(vec![
            "umount".to_string(),
            disk.mountpoint.clone(),
        ])
        .await;
        match unmount {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                if looks_like_permission_issue(&stderr) {
                    return Json(serde_json::json!({
                        "ok": false,
                        "needs_permission": true,
                        "fix_command": storage_sudo_fix_command(),
                        "message": "Need sudo permission to unmount/format disks",
                    }))
                    .into_response();
                }
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Cannot unmount {}: {}", disk.mountpoint, stderr),
                    })),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e})),
                )
                    .into_response();
            }
        }
    }

    let format_result = run_sudo_command(vec![
        "mkfs.ext4".to_string(),
        "-F".to_string(),
        disk.device.clone(),
    ])
    .await;

    match format_result {
        Ok(out) if out.status.success() => {
            let ip = auth::client_ip(&headers, addr);
            let actor = auth::session_username(&jar).unwrap_or_else(|| "admin".to_string());
            let detail = format!("{} -> {}", disk.fs_type, fs_type);
            let _ = db::audit_log(
                &state.db,
                &actor,
                "storage.change_fs",
                &disk.device,
                &detail,
                &ip,
                &auth::user_agent(&headers),
            )
            .await;

            let ext4_hint = "Enable prjquota after mounting this partition to allow per-container disk limits (overlay2.size and ext4 project quotas).".to_string();
            let ext4_cmd = "sudo mount -o remount,prjquota <mountpoint>".to_string();
            let msg = format!("{} formatted as {}", disk.device, fs_type);

            Json(serde_json::json!({
                "ok": true,
                "message": msg,
                "ext4_prjquota_hint": ext4_hint,
                "ext4_prjquota_command": ext4_cmd,
            }))
            .into_response()
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if looks_like_permission_issue(&stderr) {
                return Json(serde_json::json!({
                    "ok": false,
                    "needs_permission": true,
                    "fix_command": storage_sudo_fix_command(),
                    "message": "Need sudo permission to format disks",
                }))
                .into_response();
            }
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("mkfs failed: {}", stderr)})),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

/// POST /api/admin/storage/migrate
/// Migrates one container volume from its current disk to a new base path, then recreates the container.
pub async fn api_admin_storage_migrate(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<StorageMigrateBody>,
) -> impl IntoResponse {
    if !auth::is_root_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Root access required"}))).into_response();
    }

    if body.server_id <= 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid server id"})),
        )
            .into_response();
    }

    let target_base_raw = body.target_base_path.trim();
    if target_base_raw.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "target_base_path is required"})),
        )
            .into_response();
    }

    let target_base = std::path::PathBuf::from(target_base_raw);
    if !target_base.is_absolute() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "target_base_path must be an absolute path"})),
        )
            .into_response();
    }

    let unsafe_override = db::get_panel_setting_bool(&state.db, "storage_unsafe_override").await;

    if !unsafe_override {
        if let Err(reason) = validate_storage_base_path(&target_base).await {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "ok": false,
                    "error": reason,
                    "path": target_base,
                })),
            )
                .into_response();
        }
    }

    if unsafe_override {
        let _ = db::audit_log(
            &state.db,
            &auth::session_username(&jar).unwrap_or_else(|| "admin".to_string()),
            "storage.unsafe_migrate",
            &body.server_id.to_string(),
            &format!("target_base_path={}", target_base.display()),
            &auth::client_ip(&headers, addr),
            &auth::user_agent(&headers),
        ).await;
    }

    let (old_container_id, display_name) = match db::get_server_info_by_db_id(&state.db, body.server_id).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Server not found"})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("DB lookup failed: {}", e)})),
            )
                .into_response();
        }
    };

    let full_cfg = match docker::inspect_full(&state.docker, &old_container_id).await {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Container inspect failed: {}", e)})),
            )
                .into_response();
        }
    };

    let inspect = match state
        .docker
        .inspect_container(
            &old_container_id,
            None::<bollard::query_parameters::InspectContainerOptions>,
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Container raw inspect failed: {}", e)})),
            )
                .into_response();
        }
    };

    let labels = inspect
        .config
        .as_ref()
        .and_then(|cfg| cfg.labels.clone())
        .unwrap_or_default();

    let old_bind_source = inspect
        .host_config
        .as_ref()
        .and_then(|hc| hc.binds.as_ref())
        .and_then(|binds| binds.first())
        .and_then(|b| b.split(':').next())
        .unwrap_or("")
        .trim()
        .to_string();

    if old_bind_source.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Container has no bind mount source to migrate"})),
        )
            .into_response();
    }

    let old_volume_path = if std::path::Path::new(&old_bind_source).is_absolute() {
        std::path::PathBuf::from(&old_bind_source)
    } else {
        docker::volume_dir_to_path(&old_bind_source)
    };

    let mut volume_key = labels
        .get("yunexal.volume_dir")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| {
            let p = std::path::Path::new(&v);
            if p.is_absolute() {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&v)
                    .to_string()
            } else {
                v
            }
        })
        .or_else(|| {
            old_volume_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| {
            old_container_id
                .chars()
                .take(12)
                .collect::<String>()
        });

    if volume_key.is_empty() {
        volume_key = old_container_id
            .chars()
            .take(12)
            .collect::<String>();
    }

    if let Err(e) = tokio::fs::create_dir_all(&target_base).await {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            let out = run_sudo_command(vec![
                "mkdir".to_string(),
                "-p".to_string(),
                target_base.to_string_lossy().to_string(),
            ])
            .await;
            match out {
                Ok(o) if o.status.success() => {}
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                    if looks_like_permission_issue(&stderr) {
                        return Json(serde_json::json!({
                            "ok": false,
                            "needs_permission": true,
                            "fix_command": storage_sudo_fix_command(),
                            "message": "Need sudo permission to prepare target storage directory",
                        }))
                        .into_response();
                    }
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({"error": format!("Cannot create target path: {}", stderr)})),
                    )
                        .into_response();
                }
                Err(err) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": err})),
                    )
                        .into_response();
                }
            }
        } else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Cannot create target path: {}", e)})),
            )
                .into_response();
        }
    }

    let target_volume_path = target_base.join(&volume_key);

    let old_cmp = tokio::fs::canonicalize(&old_volume_path)
        .await
        .unwrap_or_else(|_| old_volume_path.clone());
    let new_cmp = tokio::fs::canonicalize(&target_volume_path)
        .await
        .unwrap_or_else(|_| target_volume_path.clone());
    if old_cmp == new_cmp {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Source and target volume paths are identical"})),
        )
            .into_response();
    }

    if let Err(e) = tokio::fs::create_dir_all(&target_volume_path).await {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            let out = run_sudo_command(vec![
                "mkdir".to_string(),
                "-p".to_string(),
                target_volume_path.to_string_lossy().to_string(),
            ])
            .await;
            match out {
                Ok(o) if o.status.success() => {}
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                    if looks_like_permission_issue(&stderr) {
                        return Json(serde_json::json!({
                            "ok": false,
                            "needs_permission": true,
                            "fix_command": storage_sudo_fix_command(),
                            "message": "Need sudo permission to create target volume path",
                        }))
                        .into_response();
                    }
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({"error": format!("Cannot create target volume path: {}", stderr)})),
                    )
                        .into_response();
                }
                Err(err) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": err})),
                    )
                        .into_response();
                }
            }
        } else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Cannot create target volume path: {}", e)})),
            )
                .into_response();
        }
    }

    let was_running = full_cfg.state == "running";
    if was_running {
        if let Err(e) = docker::stop_container(&state.docker, &old_container_id).await {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Failed to stop container before migration: {}", e)})),
            )
                .into_response();
        }
    }

    let src_rsync = format!("{}/", old_volume_path.to_string_lossy().trim_end_matches('/'));
    let dst_rsync = format!("{}/", target_volume_path.to_string_lossy().trim_end_matches('/'));
    let rsync_out = run_sudo_command(vec![
        "rsync".to_string(),
        "-aHAX".to_string(),
        "--numeric-ids".to_string(),
        "--delete".to_string(),
        src_rsync,
        dst_rsync,
    ])
    .await;

    let copy_ok = match rsync_out {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if looks_like_permission_issue(&stderr) {
                if was_running {
                    let _ = docker::start_container(&state.docker, &old_container_id).await;
                }
                return Json(serde_json::json!({
                    "ok": false,
                    "needs_permission": true,
                    "fix_command": storage_sudo_fix_command(),
                    "message": "Need sudo permission to copy volume data",
                }))
                .into_response();
            }

            // Fallback for hosts without rsync.
            let cp_out = run_sudo_command(vec![
                "cp".to_string(),
                "-a".to_string(),
                format!("{}/.", old_volume_path.to_string_lossy().trim_end_matches('/')),
                target_volume_path.to_string_lossy().to_string(),
            ])
            .await;

            match cp_out {
                Ok(o) if o.status.success() => true,
                Ok(o) => {
                    let cp_err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                    if was_running {
                        let _ = docker::start_container(&state.docker, &old_container_id).await;
                    }
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": format!("Data copy failed (rsync/cp): {} / {}", stderr, cp_err),
                        })),
                    )
                        .into_response();
                }
                Err(err) => {
                    if was_running {
                        let _ = docker::start_container(&state.docker, &old_container_id).await;
                    }
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": err})),
                    )
                        .into_response();
                }
            }
        }
        Err(err) => {
            if was_running {
                let _ = docker::start_container(&state.docker, &old_container_id).await;
            }
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": err})),
            )
                .into_response();
        }
    };

    if !copy_ok {
        if was_running {
            let _ = docker::start_container(&state.docker, &old_container_id).await;
        }
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Unexpected copy pipeline error"})),
        )
            .into_response();
    }

    if let Err(e) = sync_volume_root_metadata(&old_volume_path, &target_volume_path).await {
        if was_running {
            let _ = docker::start_container(&state.docker, &old_container_id).await;
        }

        if looks_like_permission_issue(&e) {
            return Json(serde_json::json!({
                "ok": false,
                "needs_permission": true,
                "fix_command": storage_sudo_fix_command(),
                "message": "Need sudo permission to normalize migrated volume ownership/permissions",
            }))
            .into_response();
        }

        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Failed to preserve source volume permissions: {}", e),
            })),
        )
            .into_response();
    }

    let docker_name = inspect
        .name
        .as_deref()
        .unwrap_or("")
        .trim_start_matches('/')
        .to_string();
    let recreate_name = if docker_name.is_empty() {
        old_container_id.chars().take(12).collect::<String>()
    } else {
        docker_name.clone()
    };

    let new_container_id = match docker::recreate_with_updated_config_and_volume_source(
        &state.docker,
        &old_container_id,
        &full_cfg.image,
        &full_cfg.env,
        &full_cfg.ports,
        full_cfg.cpu,
        full_cfg.memory_mb,
        &recreate_name,
        &target_volume_path.to_string_lossy(),
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Recreate failed after data copy: {}", e)})),
            )
                .into_response();
        }
    };

    // The old container is gone after recreation; clear any quota assignment left
    // on the previous source path before applying quota to the new location.
    docker::remove_ext4_quota(body.server_id as u32, &old_volume_path).await;

    let owner_id = db::get_server_owner_by_db_id(&state.db, body.server_id)
        .await
        .ok()
        .flatten()
        .unwrap_or(0);

    if let Err(e) = db::update_server(
        &state.db,
        &old_container_id,
        &new_container_id,
        &display_name,
        owner_id,
    )
    .await
    {
        error!(
            "storage migrate: update_server failed for #{}: {}",
            body.server_id, e
        );
        let _ = db::register_server(&state.db, &new_container_id, &display_name, owner_id).await;
        let _ = db::delete_server_by_container_id(&state.db, &old_container_id).await;
    }

    let cwd = std::env::current_dir().unwrap_or_default();
    let old_bw = cwd.join("bw").join(&old_container_id);
    let new_bw = cwd.join("bw").join(&new_container_id);
    if old_bw.exists() {
        let _ = tokio::fs::rename(&old_bw, &new_bw).await;
    }

    if was_running {
        if let Err(e) = docker::start_container(&state.docker, &new_container_id).await {
            error!("storage migrate: failed to start new container {}: {}", new_container_id, e);
        } else {
            docker::reapply_bandwidth_limit(&state.docker, &new_container_id).await;
            docker::reapply_isolation_rules(&state.docker, &new_container_id).await;
        }
    }

    let mut quota_note = String::new();
    if let Some(limit_str) = labels
        .get("yunexal.disk_limit")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
    {
        if let Some(limit_bytes) = docker::parse_disk_limit(limit_str) {
            if docker::ext4_pquota_mount(&target_volume_path).is_some() {
                match docker::apply_ext4_quota(&target_volume_path, body.server_id as u32, limit_bytes).await {
                    Ok(_) => quota_note = format!("Reapplied ext4 project quota ({})", limit_str),
                    Err(e) => quota_note = format!("Failed to reapply ext4 quota: {}", e),
                }
            } else {
                quota_note = "Target filesystem has no active quota support (ext4 with prjquota required)".to_string();
            }
        }
    }

    let ip = auth::client_ip(&headers, addr);
    let actor = auth::session_username(&jar).unwrap_or_else(|| "admin".to_string());
    let _ = db::audit_log(
        &state.db,
        &actor,
        "storage.migrate_container",
        &display_name,
        &format!(
            "server_id={} from={} to={} old_container={} new_container={}",
            body.server_id,
            old_volume_path.display(),
            target_volume_path.display(),
            old_container_id,
            new_container_id,
        ),
        &ip,
        &auth::user_agent(&headers),
    )
    .await;

    Json(serde_json::json!({
        "ok": true,
        "message": "Container migrated to new storage path",
        "server_id": body.server_id,
        "old_container_id": old_container_id,
        "new_container_id": new_container_id,
        "source_path": old_volume_path.to_string_lossy(),
        "target_path": target_volume_path.to_string_lossy(),
        "quota_note": quota_note,
    }))
    .into_response()
}

// ── DB integrity check ───────────────────────────────────────────────────────

/// POST /api/admin/db-integrity
/// Scans the database and removes records that are no longer linked to real
/// Docker containers or existing server rows. Returns counts of removed rows.
pub async fn api_admin_db_integrity(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !auth::is_root_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Root access required"}))).into_response();
    }

    // 1. Collect all live Docker container IDs (not only yunexal-managed labels).
    let live_ids: std::collections::HashSet<String> = state
        .docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            ..Default::default()
        }))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| c.id)
        .collect();

    // 2. Find servers in DB whose container_id is not in Docker.
    let db_servers: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, container_id FROM servers"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut removed_servers: u64 = 0;
    for (server_id, cid) in &db_servers {
        if !live_ids.contains(cid) {
            let _ = sqlx::query("DELETE FROM servers WHERE id = ?")
                .bind(server_id)
                .execute(&state.db)
                .await;
            removed_servers += 1;
        }
    }

    // 3. Remove orphaned server_ports (server_id not in servers).
    let orphan_ports = sqlx::query(
        "DELETE FROM server_ports WHERE server_id NOT IN (SELECT id FROM servers)"
    )
    .execute(&state.db)
    .await
    .map(|r| r.rows_affected())
    .unwrap_or(0);

    let ip = auth::client_ip(&headers, addr);
    let _ = db::audit_log(
        &state.db, "admin", "panel.db_integrity",
        "check",
        &format!("removed_servers={} orphan_ports={}", removed_servers, orphan_ports),
        &ip,
        &auth::user_agent(&headers),
    ).await;

    Json(serde_json::json!({
        "ok": true,
        "removed_servers": removed_servers,
        "removed_orphan_ports": orphan_ports,
        "total_fixed": removed_servers + orphan_ports,
    })).into_response()
}

// ── Favicon upload ────────────────────────────────────────────────────────────

/// POST /api/admin/theme/favicon
/// Accepts a multipart `file` field containing an image (jpeg/png/gif/webp/ico).
/// Saves it to `{cwd}/custom/favicon.{ext}` and records the extension in panel_settings.
pub async fn api_admin_theme_favicon(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    mut multipart: axum::extract::Multipart,
) -> impl IntoResponse {
    if !auth::is_root_session(&state, &jar).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Root access required"}))).into_response();
    }

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name != "file" { continue; }

        let content_type = field.content_type().unwrap_or("").to_string();
        let ext = match content_type.as_str() {
            "image/jpeg" | "image/jpg" => "jpg",
            "image/png"  => "png",
            "image/gif"  => "gif",
            "image/webp" => "webp",
            "image/x-icon" | "image/vnd.microsoft.icon" => "ico",
            _ => {
                // Try to infer from filename
                let fname = field.file_name().unwrap_or("").to_lowercase();
                if fname.ends_with(".png") { "png" }
                else if fname.ends_with(".gif") { "gif" }
                else if fname.ends_with(".webp") { "webp" }
                else if fname.ends_with(".ico") { "ico" }
                else { "jpg" }
            }
        };

        let data = match field.bytes().await {
            Ok(b) => b,
            Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("Read failed: {e}")}))).into_response(),
        };

        if data.len() > 5 * 1024 * 1024 {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "File too large (max 5 MB)"}))).into_response();
        }

        // Save to {cwd}/custom/favicon.{ext}
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let custom_dir = cwd.join("custom");
        if let Err(e) = tokio::fs::create_dir_all(&custom_dir).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Dir create failed: {e}")}))).into_response();
        }
        let dest = custom_dir.join(format!("favicon.{ext}"));
        if let Err(e) = tokio::fs::write(&dest, &data).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("Write failed: {e}")}))).into_response();
        }

        let _ = db::set_panel_setting(&state.db, "panel_favicon", ext).await;
        let ip = auth::client_ip(&headers, addr);
        let _ = db::audit_log(&state.db, "admin", "panel.theme.favicon", "upload", ext, &ip, &auth::user_agent(&headers)).await;

        return Json(serde_json::json!({"ok": true, "ext": ext})).into_response();
    }
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "No file field in request"}))).into_response()
}

