use axum::{
    extract::{Form, Query, State},
    response::{IntoResponse, Redirect},
    Json,
};
use axum_extra::extract::cookie::PrivateCookieJar;
use bollard::models::{RestartPolicy, RestartPolicyNameEnum};
use rand::{distr::Alphanumeric, RngExt};
use tracing::error;
use crate::compose::ComposeService;
use crate::{auth, db, docker};
use crate::dns as dns_lib;
use crate::state::AppState;
use super::templates::{render, CreateServerForm, NewServerTemplate, UserInfo};
use tracing::warn;

pub async fn create_server(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Form(form): Form<CreateServerForm>,
) -> impl IntoResponse {
    // Load users once — every error render keeps the owner dropdown populated.
    let users: Vec<UserInfo> = db::list_users(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|u| UserInfo { id: u.id, username: u.username, role: u.role, created_at: u.created_at })
        .collect();

    macro_rules! err {
        ($msg:expr) => {{
            return render(NewServerTemplate { users: users.clone(), error: Some($msg) })
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

    if let Err(e) = docker::ensure_image(&state.docker, target_image).await {
        err!(e.to_string());
    }

    // Apply image ENV overrides stored in the panel DB.
    // DB values take precedence over YAML-supplied env (admin-defined defaults win).
    let db_env_str = db::get_image_env(&state.db, target_image).await.unwrap_or_default();
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
    let volume_host_path = cwd.join("volumes").join(&volume_key);

    if let Err(e) = tokio::fs::create_dir_all(&volume_host_path).await {
        return format!("Failed to create volume directory: {}", e).into_response();
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
        // Remove eula.txt — user must accept EULA manually via the Files tab.
        let eula_path = volume_host_path.join("eula.txt");
        if eula_path.exists() {
            let _ = tokio::fs::remove_file(&eula_path).await;
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
            error!("Failed to register server ownership: {}", e);
            0
        }
    };

    if db_id > 0 {
        // ── Auto-create A + SRV DNS records ───────────────────────────────────
        if form.dns_srv_enabled.trim() == "1" {
            let pid: Option<i64> = form.dns_provider_id.trim().parse().ok();
            let port: Option<u64> = form.dns_srv_port.trim().parse().ok();
            let srv_name  = form.dns_srv_name.trim().to_string();
            let zone_id   = form.dns_zone_id.trim().to_string();
            let zone_name = form.dns_zone_name.trim().to_string();
            let priority: i64 = form.dns_srv_priority.trim().parse().unwrap_or(0);
            let weight: u64   = form.dns_srv_weight.trim().parse().unwrap_or(0);
            let a_subdomain   = form.dns_a_subdomain.trim().to_string();
            let a_ip          = form.dns_a_ip.trim().to_string();

            if let (Some(pid), Some(port)) = (pid, port) {
                if !srv_name.is_empty() && !zone_id.is_empty() {
                    if let Ok(Some(provider)) = db::dns_get_provider(&state.db, pid).await {
                        let creds: serde_json::Value = serde_json::from_str(&provider.credentials)
                            .unwrap_or(serde_json::Value::Object(Default::default()));
                        if let Ok(client) = dns_lib::DnsClient::from_type(&provider.provider_type, &creds) {

                            // Step 1 – A record (when subdomain + IP are supplied)
                            let target = if !a_subdomain.is_empty() && !a_ip.is_empty() {
                                let a_remote_id = client.create_record(&zone_id, &dns_lib::DnsRecordInput {
                                    record_type: "A".to_string(),
                                    name:        a_subdomain.clone(),
                                    value:       a_ip.clone(),
                                    ttl:         1,
                                    priority:    0,
                                    proxied:     false,
                                }).await.unwrap_or_default();
                                let _ = db::dns_add_record(
                                    &state.db, pid,
                                    &zone_id, &zone_name,
                                    "A", &a_subdomain, &a_ip,
                                    1, 0, false, &a_remote_id,
                                    Some(db_id), false, 300,
                                ).await;
                                // SRV target = fully-qualified subdomain
                                format!("{}.{}", a_subdomain, zone_name)
                            } else if !form.dns_srv_target.trim().is_empty() {
                                form.dns_srv_target.trim().to_string()
                            } else {
                                zone_name.clone()
                            };

                            // Step 2 – SRV record(s): base name without protocol
                            // If both_protos, create _tcp AND _udp; otherwise use dns_srv_name as-is
                            let base_name = {
                                let n = form.dns_srv_name.trim();
                                // Strip any trailing ._tcp / ._udp to get the bare _service
                                let stripped = n.trim_end_matches("._tcp").trim_end_matches("._udp");
                                stripped.to_string()
                            };
                            let protos: &[&str] = match form.dns_srv_both_protos.trim() {
                                "udp"  => &["udp"],
                                "tcp"  => &["tcp"],
                                _      => &["tcp", "udp"], // "both" or "1" (legacy)
                            };
                            for proto in protos {
                                let full_name = format!("{}._", base_name) + proto;
                                let srv_value = format!("{} {} {}", weight, port, target);
                                let srv_remote_id = client.create_record(&zone_id, &dns_lib::DnsRecordInput {
                                    record_type: "SRV".to_string(),
                                    name:        full_name.clone(),
                                    value:       srv_value.clone(),
                                    ttl:         1,
                                    priority,
                                    proxied:     false,
                                }).await.unwrap_or_default();
                                let _ = db::dns_add_record(
                                    &state.db, pid,
                                    &zone_id, &zone_name,
                                    "SRV", &full_name, &srv_value,
                                    1, priority, false, &srv_remote_id,
                                    Some(db_id), false, 300,
                                ).await;
                            }
                        }
                    }
                }
            }
        }
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
            match db::get_image_env(&state.db, &full_id).await {
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

/// Returns a flat list of all local image tags for use in datalists / autocomplete.
pub async fn api_local_images(State(state): State<AppState>) -> impl IntoResponse {
    let tags: Vec<String> = match docker::list_docker_images(&state.docker).await {
        Ok(images) => images.into_iter().flat_map(|i| i.repo_tags).collect(),
        Err(_) => vec![],
    };
    Json(serde_json::json!({ "tags": tags }))
}
