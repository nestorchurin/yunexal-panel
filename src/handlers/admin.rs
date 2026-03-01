use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
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
    "settings",
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
        auth_username: username,
        panel_memory_mb,
        panel_version: env!("CARGO_PKG_VERSION").to_string(),
        users,
        users_count,
        tab,
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

pub async fn admin_stop_all(State(state): State<AppState>) -> impl IntoResponse {
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
    Json(serde_json::json!({"ok": true}))
}

// ── Account password change (own account) ───────────────────────────────────

pub async fn admin_change_password(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Json(body): Json<ChangePwForm>,
) -> impl IntoResponse {
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
            Ok(_) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
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
    Json(body): Json<CreateUserForm>,
) -> impl IntoResponse {
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
        Ok(id) => (
            StatusCode::OK,
            Json(serde_json::json!({"ok": true, "id": id})),
        ),
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
    Path(id): Path<i64>,
) -> impl IntoResponse {
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
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
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
    Path(id): Path<i64>,
    Json(body): Json<AdminSetPasswordForm>,
) -> impl IntoResponse {
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
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
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
    Path(db_id): Path<i64>,
    Json(form): Json<EditContainerForm>,
) -> impl IntoResponse {
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
    Path(image_ref): Path<String>,
) -> impl IntoResponse {
    let decoded = urlencoding::decode(&image_ref).unwrap_or(std::borrow::Cow::Borrowed(&image_ref)).into_owned();
    match docker::delete_docker_image(&state.docker, &decoded).await {
        Ok(_) => {
            state.cache.remove("images_ts");
            // Clean up any stored ENV overrides for this image
            let _ = db::delete_image_env(&state.db, &decoded).await;
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
    Json(body): Json<PullImageForm>,
) -> impl IntoResponse {
    let image = body.image.trim().to_string();
    if image.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "image reference is required" }))).into_response();
    }
    match docker::ensure_image(&state.docker, &image).await {
        Ok(_) => {
            state.cache.remove("images_ts");
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
