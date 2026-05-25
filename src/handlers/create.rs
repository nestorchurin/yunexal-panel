use axum::{
    extract::{ConnectInfo, Form, Multipart, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
    Extension, Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use bollard::models::{RestartPolicy, RestartPolicyNameEnum};
use rand::{distr::Alphanumeric, RngExt};
use tracing::error;
use crate::compose::ComposeService;
use crate::{auth, db, docker};
use crate::state::AppState;
use std::net::SocketAddr;
use super::CspNonce;
use super::templates::{render, CreateServerForm, NewServerTemplate, UserInfo};
use tracing::warn;

fn valid_image_ref(image: &str) -> bool {
    let v = image.trim();
    if v.is_empty() {
        return false;
    }
    if v.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | ':' | '.' | '_' | '-'))
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

async fn root_disk_info_for_policy() -> (String, String) {
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

async fn validate_container_storage_base(path: &std::path::Path) -> Result<(), String> {
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

    let (root_source, root_disk_kname) = root_disk_info_for_policy().await;
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

pub async fn create_server(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Extension(CspNonce(nonce)): Extension<CspNonce>,
    Form(form): Form<CreateServerForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    // Load users once — every error render keeps the owner dropdown populated.
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

    macro_rules! err {
        ($msg:expr) => {{
            return render(NewServerTemplate { users: users.clone(), error: Some($msg), fix_cmd: None, nonce: nonce.clone(), default_quota_gb: "15".to_string() })
                .into_response();
        }};
    }

    let service: ComposeService = if form.config.trim().is_empty() {
        ComposeService {
            image: None,
            container_name: None,
            ports: None,
            environment: None,
            restart: None,
            volumes: None,
            cpus: None,
            mem_limit: None,
            disk_limit: None,
        }
    } else {
        match serde_yaml::from_str(&form.config) {
            Ok(s) => s,
            Err(e) => err!(format!("Could not parse YAML: {}", e)),
        }
    };

    let image_input = if form.image.trim().is_empty() {
        None
    } else {
        Some(form.image.clone())
    };

    let mut config = service.to_container_config(image_input.clone());

    // ── Port conflict check ──────────────────────────────────────────────────
    if let Some(ref hc) = config.host_config {
        if let Some(ref pb) = hc.port_bindings {
            let mut conflicts: Vec<String> = Vec::new();
            for (container_key, bindings_opt) in pb {
                let proto = if container_key.ends_with("/udp") { "udp" } else { "tcp" };
                if let Some(bindings) = bindings_opt {
                    for binding in bindings {
                        if let Some(ref port_str) = binding.host_port {
                            if let Ok(port) = port_str.parse::<u16>() {
                                let in_use = if proto == "udp" {
                                    std::net::UdpSocket::bind(("0.0.0.0", port)).is_err()
                                } else {
                                    std::net::TcpListener::bind(("0.0.0.0", port)).is_err()
                                };
                                let label = format!("{}/{}", port_str, proto);
                                if in_use && !conflicts.contains(&label) {
                                    conflicts.push(label);
                                }
                            }
                        }
                    }
                }
            }
            if !conflicts.is_empty() {
                let list = conflicts.join(", ");
                err!(format!(
                    "Port {} is already in use. Please choose a different port.",
                    list
                ));
            }
        }
    }

    let target_image = config.image.as_deref().unwrap_or("hello-world");
    if target_image.is_empty() {
        err!("Docker image must be provided either in the input field or YAML.".to_string());
    }

    // Local-first image resolution: use existing local/custom image when available,
    // only pull from registry if the image is not present on this host.
    if docker::get_image_info(&state.docker, target_image).await.is_err() {
        if let Err(e) = docker::ensure_image(&state.docker, target_image).await {
            err!(e.to_string());
        }
    }

    // Apply image ENV overrides stored in the panel DB.
    // DB values take precedence over YAML-supplied env (admin-defined defaults win).
    let db_env_str = db::get_image_env(&state.db, target_image, &state.db_key).await.unwrap_or_default();
    if !db_env_str.is_empty() {
        let db_overrides: Vec<String> = db_env_str
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && l.contains('='))
            .map(|l| l.to_string())
            .collect();
        if !db_overrides.is_empty() {
            let db_keys: std::collections::HashSet<&str> = db_overrides
                .iter()
                .filter_map(|l| l.split_once('=').map(|(k, _)| k))
                .collect();
            let mut merged: Vec<String> = config.env.clone().unwrap_or_default();
            // Remove existing YAML entries whose key is overridden by DB
            merged.retain(|e| {
                let key = e.split_once('=').map(|(k, _)| k).unwrap_or(e.as_str());
                !db_keys.contains(key)
            });
            merged.extend(db_overrides);
            config.env = Some(merged);
        }
    }

    // Inspect image to find default volumes
    let image_info = match docker::get_image_info(&state.docker, target_image).await {
        Ok(i) => i,
        Err(e) => err!(format!("Could not inspect image '{}': {}", target_image, e)),
    };

    let mut image_volumes: Vec<String> = Vec::new();
    if let Some(img_config) = image_info.config {
        if let Some(volumes) = img_config.volumes {
            image_volumes.extend(volumes.into_iter());
        }
    }

    // Reject duplicate display names (SQLite uniqueness enforcement).
    let raw_name = form.name.trim().to_string();
    match db::server_name_exists(&state.db, &raw_name, None).await {
        Ok(true) => return render(NewServerTemplate {
            users: users.clone(),
            error: Some(format!("A server named '{}' already exists. Choose a different name.", raw_name)),
            fix_cmd: None,
            nonce: nonce.clone(),
            default_quota_gb: "15".to_string(),
        }).into_response(),
        Err(e) => warn!("server_name_exists check failed: {}", e),
        Ok(false) => {}
    }

    // Sanitize container name for Docker (no spaces, only alphanumeric/-/_/.).
    let sanitized: String = raw_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let container_name = if sanitized.len() < 2 {
        rand::rng()
            .sample_iter(&Alphanumeric)
            .take(8)
            .map(char::from)
            .map(|c| c.to_ascii_lowercase())
            .collect::<String>()
    } else {
        sanitized
    };

    // Generate a stable temp volume key so the bind mount path is known pre-creation.
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let volume_key: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .map(|c| c.to_ascii_lowercase())
        .collect();

    // Determine volumes base directory: form override → panel setting → cwd/volumes
    let volumes_base = {
        let form_path = form.container_storage_path.trim();
        if !form_path.is_empty() {
            std::path::PathBuf::from(form_path)
        } else {
            let db_path = db::get_panel_setting(&state.db, "container_storage_path").await;
            if db_path.trim().is_empty() {
                cwd.join("volumes")
            } else {
                std::path::PathBuf::from(db_path.trim().to_string())
            }
        }
    };

    let volumes_base = if volumes_base.is_absolute() {
        volumes_base
    } else {
        cwd.join(volumes_base)
    };

    let storage_unsafe_override = db::get_panel_setting_bool(&state.db, "storage_unsafe_override").await;

    if !storage_unsafe_override {
        if let Err(e) = validate_container_storage_base(&volumes_base).await {
            err!(e);
        }
    }

    let volume_host_path = volumes_base.join(&volume_key);

    if let Err(e) = tokio::fs::create_dir_all(&volume_host_path).await {
        let fix_cmd = if e.raw_os_error() == Some(13) {
            // EACCES — either the volumes dir doesn't exist, its parent isn't traversable
            // (e.g. /var/lib/docker is 710), or it's not owned by the panel user.
            let path = volumes_base.display();
            let parent = volumes_base.parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            Some(format!(
                "sudo chmod 711 {parent} && sudo mkdir -p {path} && sudo chown $(whoami):$(whoami) {path}",
            ))
        } else {
            None
        };
        return render(NewServerTemplate {
            users: users.clone(),
            error: Some(format!("Failed to create volume directory: {}", e)),
            fix_cmd,
            nonce: nonce.clone(),
            default_quota_gb: "15".to_string(),
        }).into_response();
    }

    let storage_has_quota = docker::ext4_pquota_mount(&volumes_base).is_some();

    if !storage_has_quota {
        if service.disk_limit.is_some() && !storage_unsafe_override {
            err!(format!(
                "Storage path '{}' has no quota support. Configure ext4 with prjquota before creating containers with disk_limit.",
                volumes_base.display()
            ));
        }
        warn!(
            "Volumes directory '{}' is not on ext4+prjquota — disk_limit will have no effect",
            volume_host_path.display()
        );
    }

    let mut host_config = config.host_config.clone().unwrap_or_default();
    let mut binds = host_config.binds.clone().unwrap_or_default();

    let has_user_binds = !binds.is_empty();
    let mount_target = if !has_user_binds {
        let target = image_volumes
            .first()
            .cloned()
            .unwrap_or_else(|| "/data".to_string());
        let bind_mount = format!("{}:{}", volume_host_path.to_string_lossy(), target);
        binds.push(bind_mount);
        Some(target)
    } else {
        None
    };

    host_config.binds = Some(binds);
    host_config.restart_policy = Some(RestartPolicy {
        name: Some(RestartPolicyNameEnum::ON_FAILURE),
        maximum_retry_count: Some(3),
    });
    config.host_config = Some(host_config);

    // Store volume key as a Docker label for future lookup.
    // Also tag as yunexal-managed so the panel filters to only these containers.
    let mut labels = std::collections::HashMap::new();
    labels.insert("yunexal.managed".to_string(), "true".to_string());
    labels.insert("yunexal.volume_dir".to_string(), volume_key.clone());
    // Persist disk_limit so the console page can show used-vs-quota progress.
    if let Some(ref limit_str) = service.disk_limit {
        labels.insert("yunexal.disk_limit".to_string(), limit_str.clone());
    }

    // ── Per-container network isolation ─────────────────────────────────────
    // Each container gets its own bridge so it is invisible to every other
    // container, and iptables rules added at start-time block RFC1918 / loopback
    // destinations so it can only reach the public internet.
    match docker::create_isolated_network(&state.docker, &container_name).await {
        Ok((net_name, _bridge)) => {
            labels.insert("yunexal.network".to_string(), net_name.clone());
            if let Some(ref mut hc) = config.host_config {
                hc.network_mode = Some(net_name);
            }
        }
        Err(e) => warn!("Could not create isolation network for '{}': {}", container_name, e),
    }

    config.labels = Some(labels);

    config.tty = Some(true);
    config.open_stdin = Some(true);
    config.attach_stdin = Some(true);
    config.attach_stdout = Some(true);
    config.attach_stderr = Some(true);

    let docker_id = match docker::create_container(&state.docker, &container_name, config).await {
        Ok(id) => id,
        Err(e) => err!(format!("Failed to create container: {}", e)),
    };

    // Copy image files into the bind-mounted volume (must happen before container starts).
    if let Some(ref target) = mount_target {
        if let Err(e) =
            docker::copy_image_files_to_volume(&docker_id, target, &volume_host_path).await
        {
            error!("Failed to copy image files to volume: {}", e);
        }
    }

    // Persist initial bandwidth limit if provided.
    let bw_mbit: Option<u32> = form.bandwidth_mbit.trim().parse().ok();
    if let Some(mbit) = bw_mbit {
        let bw_dir = cwd.join("bw");
        if tokio::fs::create_dir_all(&bw_dir).await.is_ok() {
            let _ = tokio::fs::write(bw_dir.join(&docker_id), mbit.to_string()).await;
        }
    }

    let short_id = if docker_id.len() >= 12 {
        &docker_id[..12]
    } else {
        &docker_id
    }
    .to_string();

    // Determine owner: use form-selected owner if admin picked one, else session user.
    let owner_id = if form.owner_id != 0 {
        form.owner_id
    } else {
        auth::session_user_id(&state, &jar).await.unwrap_or(0)
    };
    let db_id = match db::register_server(&state.db, &docker_id, &form.name, owner_id).await {
        Ok(id) => id,
        Err(e) => {
            error!("Failed to register server ownership: {:#}", e);
            0
        }
    };

    if db_id > 0 {
        if form.max_schedule_runs > 0 {
            let _ = db::update_server_max_schedule_runs(&state.db, db_id, form.max_schedule_runs).await;
        }
        // ── Disk quota (ext4 + prjquota) ─────────────────────────────────────
        if let Some(ref limit_str) = service.disk_limit {
            if let Some(limit_bytes) = docker::parse_disk_limit(limit_str) {
                if docker::ext4_pquota_mount(&volume_host_path).is_some() {
                    if let Err(e) = docker::apply_ext4_quota(&volume_host_path, db_id as u32, limit_bytes).await {
                        warn!("Server #{}: failed to apply ext4 quota (disk_limit='{}'): {}", db_id, limit_str, e);
                    }
                } else {
                    warn!(
                        "Server #{}: disk_limit='{}' specified but the volumes directory is not on ext4+prjquota — quota will not be enforced",
                        db_id, limit_str
                    );
                }
            }
        }
        let actor = auth::session_username(&jar).unwrap_or_default();
        let _ = db::audit_log(&state.db, &actor, "server.create", &form.name, &format!("#{}", db_id), &ip, &auth::user_agent(&headers)).await;
        Redirect::to(&format!("/servers/{}/console", db_id)).into_response()
    } else {
        Redirect::to(&format!("/servers/{}/console", short_id)).into_response()
    }
}

#[derive(serde::Deserialize)]
pub struct ImageQuery {
    pub image: String,
}

/// Resolves an image tag to its full ID and returns stored DB env overrides.
/// Used by new_server to pre-populate custom env rows without requiring the full SHA.
pub async fn api_image_env_overrides(
    State(state): State<AppState>,
    Query(q): Query<ImageQuery>,
) -> impl IntoResponse {
    // Inspect locally only — no pull, this is just a DB lookup
    match docker::get_image_info(&state.docker, &q.image).await {
        Ok(info) => {
            let full_id = info.id.unwrap_or_default();
            match db::get_image_env(&state.db, &full_id, &state.db_key).await {
                Ok(env) => Json(serde_json::json!({ "ok": true, "env": env })),
                Err(_)  => Json(serde_json::json!({ "ok": true, "env": "" })),
            }
        }
        Err(_) => Json(serde_json::json!({ "ok": true, "env": "" })),
    }
}

pub async fn api_image_env(
    State(state): State<AppState>,
    Query(q): Query<ImageQuery>,
) -> impl IntoResponse {
    // Try local inspect first (fast path for images already on disk).
    // Only pull from registry if the image isn't found locally.
    if docker::get_image_info(&state.docker, &q.image).await.is_err() {
        if let Err(e) = docker::ensure_image(&state.docker, &q.image).await {
            return Json(serde_json::json!({ "ok": false, "error": format!("Failed to pull image: {}", e) }));
        }
    }
    match docker::get_image_info(&state.docker, &q.image).await {
        Ok(info) => {
            let env: Vec<String> = info
                .config
                .and_then(|c| c.env)
                .unwrap_or_default();
            Json(serde_json::json!({ "ok": true, "env": env }))
        }
        Err(e) => {
            Json(serde_json::json!({ "ok": false, "error": e.to_string() }))
        }
    }
}

/// Builds a local Docker image from an uploaded Dockerfile.
/// Multipart fields:
/// - `image`: resulting image tag/reference (required)
/// - `dockerfile`: file bytes (required)
pub async fn api_build_image_from_dockerfile(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let actor = auth::session_username(&jar).unwrap_or_else(|| "admin".to_string());

    let mut image_ref = String::new();
    let mut dockerfile_bytes: Option<Vec<u8>> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "image" => {
                image_ref = field.text().await.unwrap_or_default().trim().to_string();
            }
            "dockerfile" => {
                match field.bytes().await {
                    Ok(bytes) => dockerfile_bytes = Some(bytes.to_vec()),
                    Err(e) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({ "ok": false, "error": format!("Failed to read Dockerfile upload: {}", e) })),
                        ).into_response();
                    }
                }
            }
            _ => {}
        }
    }

    if !valid_image_ref(&image_ref) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": "Invalid image reference" })),
        ).into_response();
    }

    let dockerfile = match dockerfile_bytes {
        Some(bytes) => bytes,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "ok": false, "error": "Dockerfile upload is required" })),
            ).into_response();
        }
    };

    if dockerfile.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": "Dockerfile is empty" })),
        ).into_response();
    }
    if dockerfile.len() > 512 * 1024 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": "Dockerfile is too large (max 512 KiB)" })),
        ).into_response();
    }

    let temp_key: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let build_dir = std::env::temp_dir().join(format!("yunexal-dockerfile-{}", temp_key));
    let dockerfile_path = build_dir.join("Dockerfile");

    if let Err(e) = tokio::fs::create_dir_all(&build_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": format!("Failed to create build context: {}", e) })),
        ).into_response();
    }
    if let Err(e) = tokio::fs::write(&dockerfile_path, &dockerfile).await {
        let _ = tokio::fs::remove_dir_all(&build_dir).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": format!("Failed to write Dockerfile: {}", e) })),
        ).into_response();
    }

    let build_result = tokio::time::timeout(
        std::time::Duration::from_secs(600),
        tokio::process::Command::new("docker")
            .args(["build", "-t", &image_ref, "-f", "Dockerfile", "."])
            .current_dir(&build_dir)
            .output(),
    ).await;

    let _ = tokio::fs::remove_dir_all(&build_dir).await;

    match build_result {
        Ok(Ok(out)) if out.status.success() => {
            state.cache.remove("images_ts");
            let _ = db::audit_log(
                &state.db,
                &actor,
                "image.build_dockerfile",
                &image_ref,
                "",
                &ip,
                &auth::user_agent(&headers),
            ).await;
            (StatusCode::OK, Json(serde_json::json!({ "ok": true, "image": image_ref }))).into_response()
        }
        Ok(Ok(out)) => {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let msg = if err.is_empty() {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            } else {
                err
            };
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "ok": false, "error": if msg.is_empty() { "Docker build failed".to_string() } else { msg } })),
            ).into_response()
        }
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": format!("Failed to run docker build: {}", e) })),
        ).into_response(),
        Err(_) => (
            StatusCode::REQUEST_TIMEOUT,
            Json(serde_json::json!({ "ok": false, "error": "Docker build timed out" })),
        ).into_response(),
    }
}

/// Returns a flat list of all local image tags for use in datalists / autocomplete.
pub async fn api_local_images(State(state): State<AppState>) -> impl IntoResponse {
    let tags: Vec<String> = match docker::list_docker_images(&state.docker).await {
        Ok(images) => images.into_iter().flat_map(|i| i.repo_tags).collect(),
        Err(_) => vec![],
    };
    Json(serde_json::json!({ "tags": tags }))
}

/// Returns whether host storage supports ext4 project quotas for containers.
pub async fn api_quota_check(State(state): State<AppState>) -> impl IntoResponse {
    let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    let unsafe_override = db::get_panel_setting_bool(&state.db, "storage_unsafe_override").await;

    let mut has_ext4_prjquota = false;
    let mut ext4_without_prjquota = false;

    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let dev    = parts.next().unwrap_or("");
        let _mount = parts.next().unwrap_or("");
        let fstype = parts.next().unwrap_or("");
        let opts   = parts.next().unwrap_or("");

        if fstype == "ext4" && dev.starts_with("/dev/") {
            let has_prj = opts.split(',').any(|o| o == "prjquota" || o == "prjjquota");
            has_ext4_prjquota |= has_prj;
            ext4_without_prjquota |= !has_prj;
        }
    }

    let has_quota = has_ext4_prjquota;
    let ok = if unsafe_override { true } else { has_quota };
    Json(serde_json::json!({
        "ok": ok,
        "has_quota": has_quota,
        "has_ext4_prjquota": has_ext4_prjquota,
        "ext4_without_prjquota": ext4_without_prjquota,
        "unsafe_override": unsafe_override,
    }))
}
