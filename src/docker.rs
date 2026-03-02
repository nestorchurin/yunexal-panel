use bollard::query_parameters::{ListContainersOptions, StartContainerOptions, StopContainerOptions, AttachContainerOptions};
use bollard::container::LogOutput;
use bollard::Docker;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use futures_util::Stream;
use std::pin::Pin;
use tokio::io::AsyncWrite;
use std::time::SystemTime;

/// Parses an RFC3339 timestamp and returns a human-readable uptime string.
fn parse_uptime(started_at: &str) -> Option<String> {
    // Docker returns timestamps like "2026-02-28T12:00:00.000000000Z"
    // We parse the basic structure without a full datetime crate.
    let started_at = started_at.trim();
    if started_at.is_empty() || started_at == "0001-01-01T00:00:00Z" {
        return None;
    }

    // Parse using the time parts manually
    let parts: Vec<&str> = started_at.split('T').collect();
    if parts.len() != 2 { return None; }

    let date_parts: Vec<u64> = parts[0].split('-').filter_map(|s| s.parse().ok()).collect();
    let time_str = parts[1].trim_end_matches('Z').split('+').next()?;
    let time_parts: Vec<&str> = time_str.split(':').collect();
    if date_parts.len() != 3 || time_parts.len() < 3 { return None; }

    let (year, month, day) = (date_parts[0], date_parts[1], date_parts[2]);
    let hour: u64 = time_parts[0].parse().ok()?;
    let min: u64 = time_parts[1].parse().ok()?;
    let sec: u64 = time_parts[2].split('.').next()?.parse().ok()?;

    // Convert to approximate Unix timestamp (not perfect for leap seconds but fine for uptime)
    fn days_from_civil(y: u64, m: u64, d: u64) -> u64 {
        let y = if m <= 2 { y - 1 } else { y };
        let m = if m <= 2 { m + 9 } else { m - 3 };
        let era = y / 400;
        let yoe = y - era * 400;
        let doy = (153 * m + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }

    let started_secs = days_from_civil(year, month, day) * 86400 + hour * 3600 + min * 60 + sec;
    let now_secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_secs();

    if now_secs < started_secs { return None; }
    let diff = now_secs - started_secs;

    let days = diff / 86400;
    let hours = (diff % 86400) / 3600;
    let mins = (diff % 3600) / 60;

    if days > 0 {
        Some(format!("Up {} day{} {} hr", days, if days != 1 { "s" } else { "" }, hours))
    } else if hours > 0 {
        Some(format!("Up {} hr {} min", hours, mins))
    } else {
        Some(format!("Up {} min", mins.max(1)))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub status: String,
    pub state: String,
    pub cpu_usage: String,
    pub ram_usage: String,
    /// Internal SQLite id. 0 if not yet resolved from DB.
    pub db_id: i64,
    /// Owner username. Empty string if not yet resolved from DB.
    pub owner: String,
}

pub async fn get_docker_client() -> Result<Docker> {
    Docker::connect_with_socket_defaults().context("Failed to connect to Docker socket")
}

pub async fn list_containers(docker: &Docker) -> Result<Vec<ContainerInfo>> {
    let mut filters = std::collections::HashMap::new();
    filters.insert("label".to_string(), vec!["yunexal.managed=true".to_string()]);

    let options = ListContainersOptions {
        all: true,
        filters: Some(filters),
        ..Default::default()
    };

    let containers = docker
        .list_containers(Some(options))
        .await
        .context("Failed to list containers")?;

    let tasks = containers.into_iter().map(|c| {
        let docker = docker.clone();
        async move {
            let name = c.names.as_ref()
                .and_then(|n| n.first())
                .map(|n| n.trim_start_matches('/'))
                .unwrap_or("unknown")
                .to_string();

            let id = c.id.clone().unwrap_or_default();
            let short_id = if id.len() > 12 { &id[..12] } else { &id }.to_string();
            let state = c.state.map(|s| s.to_string()).unwrap_or_default(); // "running", "exited"
            let status = c.status.unwrap_or_default(); // "Up 2 hours"

            let (cpu_usage, ram_usage) = if state == "running" {
                get_container_stats(&docker, &id).await.unwrap_or(("0.0%".into(), "0MB / 0MB".into()))
            } else {
                 ("Offline".into(), "Offline".into())
            };

            ContainerInfo {
                id,
                short_id,
                name,
                status,
                state,
                cpu_usage,
                ram_usage,
                db_id: 0,
                owner: String::new(),
            }
        }
    });

    let info_list = futures_util::future::join_all(tasks).await;

    Ok(info_list)
}

/// Same as list_containers but skips the blocking Docker stats stream.
/// cpu_usage and ram_usage are left empty — fill them separately via get_container_stats.
pub async fn list_containers_fast(docker: &Docker) -> Result<Vec<ContainerInfo>> {
    let mut filters = std::collections::HashMap::new();
    filters.insert("label".to_string(), vec!["yunexal.managed=true".to_string()]);

    let options = ListContainersOptions {
        all: true,
        filters: Some(filters),
        ..Default::default()
    };

    let containers = docker
        .list_containers(Some(options))
        .await
        .context("Failed to list containers")?;

    let info_list = containers.into_iter().map(|c| {
        let name = c.names.as_ref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/'))
            .unwrap_or("unknown")
            .to_string();

        let id = c.id.clone().unwrap_or_default();
        let short_id = if id.len() > 12 { &id[..12] } else { &id }.to_string();
        let state = c.state.map(|s| s.to_string()).unwrap_or_default();
        let status = c.status.unwrap_or_default();

        ContainerInfo {
            id,
            short_id,
            name,
            status,
            state,
            cpu_usage: String::new(),
            ram_usage: String::new(),
            db_id: 0,
            owner: String::new(),
        }
    }).collect();

    Ok(info_list)
}

pub async fn get_container(docker: &Docker, id: &str) -> Result<ContainerInfo> {
    use bollard::query_parameters::InspectContainerOptions;

    // Use inspect instead
    let c = docker.inspect_container(id, None::<InspectContainerOptions>).await
        .context("Container not found")?;

    let name = c.name.as_deref().unwrap_or("unknown").trim_start_matches('/').to_string();
    let id = c.id.clone().unwrap_or_default();
    let short_id = if id.len() > 12 { &id[..12] } else { &id }.to_string();

    let state = c.state.clone().unwrap_or_default();
    let state_str = state.status.map(|s| s.to_string()).unwrap_or_else(|| "unknown".into());
    
    // Build a human-readable status from the state information
    let status = if state_str == "running" {
        let started = state.started_at.as_deref().unwrap_or("");
        // Parse RFC3339 timestamp and compute uptime
        if let Some(uptime) = parse_uptime(started) {
            uptime
        } else {
            "Running".to_string()
        }
    } else {
        state_str.clone()
    };
    
    // We only fetch stats if running
    let (cpu_usage, ram_usage) = if state_str == "running" {
        get_container_stats(docker, &id).await.unwrap_or(("Error".into(), "Error".into()))
    } else {
        ("Offline".into(), "Offline".into())
    };

    Ok(ContainerInfo {
        id,
        short_id,
        name,
        status,
        state: state_str,
        cpu_usage,
        ram_usage,
        db_id: 0,
        owner: String::new(),
    })
}

pub async fn start_container(docker: &Docker, id: &str) -> Result<()> {
    docker
        .start_container(id, None::<StartContainerOptions>)
        .await
        .context(format!("Failed to start container {}", id))?;
    Ok(())
}

pub async fn stop_container(docker: &Docker, id: &str) -> Result<()> {
    let options = StopContainerOptions { t: Some(10), ..Default::default() };
    docker
        .stop_container(id, Some(options))
        .await
        .context(format!("Failed to stop container {}", id))?;
    Ok(())
}

pub async fn create_container(docker: &Docker, name: &str, config: bollard::models::ContainerCreateBody) -> Result<String> {
    use bollard::query_parameters::CreateContainerOptions;

    let options = CreateContainerOptions {
        name: Some(name.to_string()),
        platform: String::new(),
    };

    let response = docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| anyhow::anyhow!("Docker API error: {}", e))?;
    Ok(response.id)
}

/// Copies files from a container path into the host `dest` directory using `docker cp`.
/// The container does NOT need to be running — works on created (stopped) containers too.
/// Silently succeeds if the path doesn't exist in the image.
pub async fn copy_image_files_to_volume(container_id: &str, src_path: &str, dest: &std::path::Path) -> Result<()> {
    // `docker cp container_id:/path/. /host/dir/`  copies the *contents* of /path into /host/dir
    let src = format!("{}:{}/.", container_id, src_path.trim_end_matches('/'));
    let dest_str = dest.to_string_lossy().to_string();

    let output = tokio::process::Command::new("docker")
        .args(["cp", &src, &dest_str])
        .output()
        .await
        .context("Failed to spawn docker cp")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        // These are not real errors — it just means the image has no files there
        if stderr.contains("no such") || stderr.contains("not found") || stderr.contains("could not find") {
            return Ok(());
        }
        tracing::warn!("docker cp: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

pub async fn ensure_image(docker: &Docker, image: &str) -> Result<()> {
    use bollard::errors::Error as BollardError;
    use bollard::query_parameters::CreateImageOptions;
    use futures_util::stream::StreamExt;

    let options = CreateImageOptions {
        from_image: Some(image.to_string()),
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(options), None, None);
    while let Some(msg) = stream.next().await {
        if let Err(err) = msg {
            // Provide clearer context for common pull failures (404/private images)
            return match err {
                BollardError::DockerResponseServerError { status_code, message } => {
                    let reason = if status_code == 404 {
                        "image not found or private; check the name/tag or login first"
                    } else {
                        "docker registry returned an error"
                    };
                    Err(anyhow::anyhow!(
                        "Failed to pull image '{}': {} ({})",
                        image,
                        message,
                        reason
                    ))
                }
                other => Err(anyhow::anyhow!("Failed to pull image '{}': {}", image, other)),
            };
        }
    }
    Ok(())
}

pub async fn get_image_info(docker: &Docker, image: &str) -> Result<bollard::models::ImageInspect> {
    docker.inspect_image(image).await.map_err(|e| anyhow::anyhow!("Failed to inspect image: {}", e))
}

// ── Docker Image Management ───────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DockerImageInfo {
    pub id: String,           // short sha (12 chars)
    pub full_id: String,      // full sha256:... string
    pub repo_tags: Vec<String>,
    pub size_mb: String,
    pub created: String,
    pub in_use: bool,         // true if any yunexal container references this image
}

/// Lists all local Docker images, annotated with whether a managed container uses them.
pub async fn list_docker_images(docker: &Docker) -> Result<Vec<DockerImageInfo>> {
    use bollard::query_parameters::ListImagesOptions;

    let images_fut = docker
        .list_images(Some(ListImagesOptions { all: false, ..Default::default() }));

    let mut filters = std::collections::HashMap::new();
    filters.insert("label".to_string(), vec!["yunexal.managed=true".to_string()]);
    let containers_fut = docker.list_containers(Some(ListContainersOptions {
        all: true,
        filters: Some(filters),
        ..Default::default()
    }));

    let (summaries, containers) = tokio::try_join!(images_fut, containers_fut)
        .context("Failed to list images/containers")?;

    // Collect image IDs referenced by yunexal containers.
    let used_images: std::collections::HashSet<String> = containers
        .into_iter()
        .filter_map(|c| c.image_id)
        .collect();

    let images = summaries
        .into_iter()
        .filter_map(|s| {
            let full_id = s.id.clone();
            let short_id = if full_id.starts_with("sha256:") {
                full_id[7..].chars().take(12).collect()
            } else {
                full_id.chars().take(12).collect()
            };
            let size_mb = if s.size >= 1_073_741_824 {
                format!("{:.1} GB", s.size as f64 / 1_073_741_824.0)
            } else {
                format!("{:.0} MB", s.size as f64 / 1_048_576.0)
            };
            // Format created Unix timestamp as YYYY-MM-DD
            let created = {
                let ts = s.created as u64;
                let days = ts / 86400;
                fn civil(d: u64) -> (u64, u64, u64) {
                    let z = d + 719468;
                    let era = z / 146097;
                    let doe = z - era * 146097;
                    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
                    let y = yoe + era * 400;
                    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                    let mp = (5 * doy + 2) / 153;
                    let d = doy - (153 * mp + 2) / 5 + 1;
                    let m = if mp < 10 { mp + 3 } else { mp - 9 };
                    let y = if m <= 2 { y + 1 } else { y };
                    (y, m, d)
                }
                let (y, m, d) = civil(days);
                format!("{:04}-{:02}-{:02}", y, m, d)
            };
            let in_use = used_images.contains(&full_id);
            let repo_tags: Vec<String> = s.repo_tags.into_iter()
                .filter(|t| t != "<none>:<none>")
                .collect();
            if repo_tags.is_empty() {
                return None; // hide untagged (<none>:<none>-only) images
            }
            Some(DockerImageInfo { id: short_id, full_id, repo_tags, size_mb, created, in_use })
        })
        .collect();

    Ok(images)
}

/// Deletes an image by full ID (sha256:...) or tag, with force=true.
pub async fn delete_docker_image(docker: &Docker, image_ref: &str) -> Result<()> {
    use bollard::query_parameters::RemoveImageOptionsBuilder;
    let opts = RemoveImageOptionsBuilder::default().force(true).build();
    docker
        .remove_image(image_ref, Some(opts), None)
        .await
        .context("Failed to remove image")?;
    Ok(())
}

/// Adds a new tag `new_tag` (format `repo:tag`) to an image identified by `image_ref`.
pub async fn retag_docker_image(docker: &Docker, image_ref: &str, new_repo: &str, new_tag: &str) -> Result<()> {
    use bollard::query_parameters::TagImageOptionsBuilder;
    let opts = TagImageOptionsBuilder::default()
        .repo(new_repo)
        .tag(new_tag)
        .build();
    docker
        .tag_image(image_ref, Some(opts))
        .await
        .context("Failed to tag image")?;
    Ok(())
}

/// Creates a full independent copy of an image via `docker commit` of a temp container.
/// Returns the new image's full ID (sha256:...).
pub async fn duplicate_docker_image(docker: &Docker, image_ref: &str) -> Result<String> {
    use bollard::query_parameters::{CreateContainerOptions, RemoveContainerOptions};

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let temp_name = format!("yunexal-dup-tmp-{}", ts);

    // 1. Create a stopped temporary container from the source image
    let create_opts = CreateContainerOptions {
        name: Some(temp_name.clone()),
        platform: String::new(),
    };
    let create_body = bollard::models::ContainerCreateBody {
        image: Some(image_ref.to_string()),
        ..Default::default()
    };
    let container = docker
        .create_container(Some(create_opts), create_body)
        .await
        .context("Failed to create temp container for image duplication")?;
    let cid = container.id.clone();

    // 2. Commit via docker CLI (most reliable, matches CLI behaviour exactly)
    let commit_out = tokio::process::Command::new("docker")
        .args(["commit", &cid])
        .output()
        .await
        .context("Failed to spawn docker commit")?;

    // 3. Always clean up the temp container
    let _ = docker
        .remove_container(&cid, Some(RemoveContainerOptions { force: true, ..Default::default() }))
        .await;

    if !commit_out.status.success() {
        let err = String::from_utf8_lossy(&commit_out.stderr).trim().to_string();
        return Err(anyhow::anyhow!("docker commit failed: {}", err));
    }

    // stdout is "sha256:<hex>\n"
    let new_id = String::from_utf8_lossy(&commit_out.stdout).trim().to_string();
    if new_id.is_empty() {
        return Err(anyhow::anyhow!("docker commit returned empty ID"));
    }
    Ok(new_id)
}

// Return type alias for complexity
pub type DockerStream = Pin<Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>>;
pub type DockerInput = Pin<Box<dyn AsyncWrite + Send>>;

pub async fn attach_container(docker: &Docker, id: &str) -> Result<(DockerStream, DockerInput)> {
    // If container not running, this might fail or return closed stream.
    // Client must handle disconnection.
    
    let options = AttachContainerOptions {
        stdout: true,
        stderr: true,
        stdin: true,
        stream: true,
        logs: true,
        ..Default::default()
    };
    
    // NOTE: TTY might need to be set on creation for colors to work properly depending on the container image.
    // But we attach with what we have.
    // If the container was created with Tty=true in Config, `attach_container` handles it.
    
    let result = docker.attach_container(id, Some(options)).await?;
    
    // We get .output (Stream) and .input (AsyncWrite)
    Ok((Box::pin(result.output), result.input))
}

pub async fn remove_container(docker: &Docker, id: &str) -> Result<()> {
    use bollard::query_parameters::RemoveContainerOptions;
    docker.remove_container(id, Some(RemoveContainerOptions {
        force: true,
        ..Default::default()
    })).await.context("Failed to remove container")?;
    Ok(())
}

pub async fn kill_container(docker: &Docker, id: &str) -> Result<()> {
    use bollard::query_parameters::KillContainerOptions;
     docker
        .kill_container(id, None::<KillContainerOptions>)
        .await
        .context(format!("Failed to kill container {}", id))?;
    Ok(())
}

#[derive(Serialize, Default)]
pub struct ContainerStatsRaw {
     pub cpu_usage: f64,
     pub ram_usage: u64,
     pub ram_limit: u64,
     pub net_rx: u64,
     pub net_tx: u64,
}

pub async fn get_container_stats_raw(docker: &Docker, id: &str) -> Result<ContainerStatsRaw> {
    use bollard::query_parameters::StatsOptions;
    use futures_util::StreamExt;

    let options = StatsOptions {
        stream: false,
        one_shot: true,
    };
    
    let mut stream = docker.stats(id, Some(options));
    if let Some(Ok(stats)) = stream.next().await {
        // bollard 0.20: cpu_stats / precpu_stats / memory_stats are wrapped in Option
        let cpu_total   = stats.cpu_stats.as_ref().and_then(|c| c.cpu_usage.as_ref()).and_then(|u| u.total_usage).unwrap_or(0);
        let pre_total   = stats.precpu_stats.as_ref().and_then(|c| c.cpu_usage.as_ref()).and_then(|u| u.total_usage).unwrap_or(0);
        let sys_cur     = stats.cpu_stats.as_ref().and_then(|c| c.system_cpu_usage).unwrap_or(0);
        let sys_pre     = stats.precpu_stats.as_ref().and_then(|c| c.system_cpu_usage).unwrap_or(0);
        let num_cpus    = stats.cpu_stats.as_ref().and_then(|c| c.online_cpus).map(|n| n as f64).unwrap_or(1.0);

        let cpu_delta    = cpu_total as f64 - pre_total as f64;
        let system_delta = sys_cur   as f64 - sys_pre   as f64;

        let mut cpu_usage = 0.0;
        if system_delta > 0.0 && cpu_delta > 0.0 {
            cpu_usage = (cpu_delta / system_delta) * num_cpus * 100.0;
        }

        let memory_usage = stats.memory_stats.as_ref().and_then(|m| m.usage).unwrap_or(0);
        let memory_limit = stats.memory_stats.as_ref().and_then(|m| m.limit).unwrap_or(0);

        let mut rx: u64 = 0;
        let mut tx: u64 = 0;
        if let Some(networks) = stats.networks {
            for (_, net) in networks {
                rx += net.rx_bytes.unwrap_or(0);
                tx += net.tx_bytes.unwrap_or(0);
            }
        }

        return Ok(ContainerStatsRaw {
            cpu_usage,
            ram_usage: memory_usage,
            ram_limit: memory_limit,
            net_rx: rx,
            net_tx: tx,
        });
    }
    
    Ok(ContainerStatsRaw::default())
}


pub async fn get_container_stats(docker: &Docker, id: &str) -> Result<(String, String)> {
    let stats = get_container_stats_raw(docker, id).await?;
    let ram_usage = if stats.ram_limit > 0 {
        format!("{:.0}MB / {:.0}MB", stats.ram_usage as f64 / 1024.0 / 1024.0, stats.ram_limit as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.0}MB", stats.ram_usage as f64 / 1024.0 / 1024.0)
    };
    Ok((format!("{:.2}%", stats.cpu_usage), ram_usage))
}

pub async fn list_files(_docker: &Docker, id: &str, path: &str) -> Result<Vec<String>> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    // 'id' is expected to be the container name (server_id)
    let volume_path = cwd.join("volumes").join(id);
    
    // path is relative to the mount point (/app/data), so it should be relative to volume_path
    let rel_path = path.trim_start_matches('/');
    let target_joined = volume_path.join(rel_path);
    // Normalize ".." to prevent path traversal (defense in depth)
    let mut target_path = std::path::PathBuf::new();
    for component in target_joined.components() {
        match component {
            std::path::Component::ParentDir => { target_path.pop(); },
            std::path::Component::CurDir    => {},
            c => target_path.push(c),
        }
    }
    if !target_path.starts_with(&volume_path) {
        anyhow::bail!("Access denied: path traversal");
    }

    if !target_path.exists() {
        return Ok(vec![]);
    }

    let mut entries = tokio::fs::read_dir(target_path).await
        .context(format!("Failed to read directory {:?}", rel_path))?;
    let mut files = Vec::new();
    
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name.ends_with(".example") || name.ends_with(".test") { continue; } // Hide dotfiles and restricted types
        if entry.file_type().await?.is_dir() {
            files.push(format!("{}/", name));
        } else {
            files.push(name);
        }
    }
    
    // Sort files: directories first, then alphabetical
    files.sort_by(|a, b| {
        let a_is_dir = a.ends_with('/');
        let b_is_dir = b.ends_with('/');
        if a_is_dir && !b_is_dir {
            std::cmp::Ordering::Less
        } else if !a_is_dir && b_is_dir {
            std::cmp::Ordering::Greater
        } else {
            a.cmp(b)
        }
    });
    
    Ok(files)
}

#[allow(dead_code)]
pub async fn get_container_inspect(docker: &Docker, id: &str) -> Result<bollard::models::ContainerInspectResponse> {
     docker.inspect_container(id, None).await.map_err(|e| anyhow::anyhow!(e))
}

/// Resolves any ID / short-ID / name to the full 64-char container ID.
#[allow(dead_code)]
pub async fn get_full_id(docker: &Docker, id: &str) -> Result<String> {
    let c = docker.inspect_container(id, None).await
        .context("Container not found")?;
    Ok(c.id.unwrap_or_default())
}

/// Returns the friendly container name (without leading slash).
#[allow(dead_code)]
pub async fn get_container_name(docker: &Docker, id: &str) -> Result<String> {
    let c = docker.inspect_container(id, None).await
        .context("Container not found")?;
    Ok(c.name.unwrap_or_default().trim_start_matches('/').to_string())
}

/// Returns the volume directory key for this container.
/// Resolution order:
///   1. Label `yunexal.volume_dir` — if the directory actually exists on disk.
///   2. Full 64-char container ID — if `./volumes/<full_id>` exists on disk.
///   3. Label value or container name as a last-resort string (directory may be missing).
pub async fn get_volume_dir(docker: &Docker, id: &str) -> Result<String> {
    let c = docker.inspect_container(id, None).await
        .context("Container not found")?;

    let full_id = c.id.clone().unwrap_or_default();
    let name = c.name.clone().unwrap_or_default().trim_start_matches('/').to_string();

    let label_key = c.config.as_ref()
        .and_then(|cfg| cfg.labels.as_ref())
        .and_then(|labels| labels.get("yunexal.volume_dir").cloned());

    // Extract volume dir from the actual bind mount source path
    let bind_dir = c.host_config.as_ref()
        .and_then(|hc| hc.binds.as_ref())
        .and_then(|binds| binds.first())
        .and_then(|b| b.split(':').next())
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|f| f.to_str())
        .map(|s| s.to_string());

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // 1. Label path exists on disk
    if let Some(ref key) = label_key {
        if cwd.join("volumes").join(key).exists() {
            return Ok(key.clone());
        }
    }

    // 2. Bind mount source directory exists on disk
    if let Some(ref dir) = bind_dir {
        if cwd.join("volumes").join(dir).exists() {
            return Ok(dir.clone());
        }
    }

    // 3. Full container ID directory exists on disk
    if !full_id.is_empty() && cwd.join("volumes").join(&full_id).exists() {
        return Ok(full_id);
    }

    // 4. Fallback — return bind dir, label, or name even if missing
    Ok(bind_dir.or(label_key).unwrap_or(name))
}

// ── Bandwidth limiting via Linux tc ──────────────────────────────────────────

/// Path to the file that persists the bandwidth limit for a container.
fn bw_file_path(full_id: &str) -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    cwd.join("bw").join(full_id)
}

/// Returns the stored bandwidth limit in Mbit/s, or `None` for unlimited.
pub async fn get_bandwidth_limit(docker: &Docker, id: &str) -> Result<Option<u32>> {
    let c = docker.inspect_container(id, None).await.context("Container not found")?;
    let full_id = c.id.unwrap_or_default();
    match tokio::fs::read_to_string(bw_file_path(&full_id)).await {
        Ok(s) => Ok(s.trim().parse().ok()),
        Err(_) => Ok(None),
    }
}

/// Applies a tc TBF rate limit on the container's network interface and persists it to disk.
/// Pass `limit_mbit = None` to remove the limit entirely.
///
/// If the container is stopped the limit is saved to disk and will be applied automatically
/// by `reapply_bandwidth_limit` when the container next starts.
///
/// Implementation note: spawns a one-shot Alpine container that shares the target container's
/// network namespace (`--network=container:<id>`) with only `CAP_NET_ADMIN`.
/// No `--privileged`, no `--pid=host`, no veth lookup — `tc` operates on `eth0` directly.
pub async fn set_bandwidth_limit(docker: &Docker, id: &str, limit_mbit: Option<u32>) -> Result<()> {
    let c = docker.inspect_container(id, None).await.context("Container not found")?;
    let full_id = c.id.as_deref().unwrap_or("").to_string();
    let running = c.state.and_then(|s| s.running).unwrap_or(false);

    // Always persist / remove the limit record first so reapply works on next start.
    match limit_mbit {
        Some(mbit) => {
            let bw_path = bw_file_path(&full_id);
            tokio::fs::create_dir_all(bw_path.parent().unwrap()).await?;
            tokio::fs::write(&bw_path, mbit.to_string()).await?;
        }
        None => {
            let _ = tokio::fs::remove_file(bw_file_path(&full_id)).await;
        }
    }

    // If stopped, done — the rule will be applied by reapply_bandwidth_limit on next start.
    if !running {
        return Ok(());
    }

    let tc_cmd = match limit_mbit {
        Some(mbit) => format!(
            "apk add -q --no-cache iproute2 && tc qdisc replace dev eth0 root tbf rate {mbit}mbit burst 32kbit latency 400ms",
        ),
        None => "apk add -q --no-cache iproute2 && tc qdisc del dev eth0 root 2>/dev/null || true".to_string(),
    };

    // Run tc inside a helper container that shares the target's network namespace.
    // --cap-add NET_ADMIN  — allows tc to modify qdisc rules
    // --network=container  — operates on target container's eth0, not the host stack
    let network_arg = format!("container:{}", full_id);
    let status = tokio::process::Command::new("docker")
        .args([
            "run", "--rm",
            "--cap-add", "NET_ADMIN",
            "--network", &network_arg,
            "alpine",
            "sh", "-c", &tc_cmd,
        ])
        .status()
        .await
        .context("Failed to spawn docker run for tc — is docker in PATH?")?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "Bandwidth tc command failed (exit {:?})",
            status.code()
        ));
    }

    Ok(())
}

/// Re-applies the stored bandwidth limit from disk (call after container start).
/// Does nothing if no limit is stored.
pub async fn reapply_bandwidth_limit(docker: &Docker, id: &str) {
    match get_bandwidth_limit(docker, id).await {
        Ok(Some(mbit)) => {
            if let Err(e) = set_bandwidth_limit(docker, id, Some(mbit)).await {
                tracing::warn!("Could not re-apply bandwidth limit for {}: {}", id, e);
            }
        }
        _ => {}
    }
}

// ── Container edit helpers ──────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug)]
pub struct ContainerFullConfig {
    pub image: String,
    /// Newline-joined "KEY=VALUE" strings.
    pub env: String,
    /// Newline-joined "host:container/proto" strings.
    pub ports: String,
    pub cpu: f64,
    pub memory_mb: i64,
    pub volume_binds: Vec<String>,
    pub labels: std::collections::HashMap<String, String>,
    pub state: String,
}

pub async fn inspect_full(docker: &Docker, id: &str) -> Result<ContainerFullConfig> {
    let c = docker
        .inspect_container(id, None::<bollard::query_parameters::InspectContainerOptions>)
        .await
        .context("Container not found")?;

    let image = c.config.as_ref().and_then(|cfg| cfg.image.clone()).unwrap_or_default();

    let env_vec: Vec<String> = c.config.as_ref()
        .and_then(|cfg| cfg.env.clone())
        .unwrap_or_default();
    let env = env_vec.join("\n");

    let mut port_lines: Vec<String> = Vec::new();
    if let Some(hc) = c.host_config.as_ref() {
        if let Some(pb) = hc.port_bindings.as_ref() {
            let mut pairs: Vec<_> = pb.iter().collect();
            pairs.sort_by_key(|(k, _)| (*k).clone());
            for (container_key, bindings_opt) in pairs {
                if let Some(bindings) = bindings_opt {
                    for binding in bindings {
                        let hp = binding.host_port.as_deref().unwrap_or("0");
                        port_lines.push(format!("{}:{}", hp, container_key));
                    }
                }
            }
        }
    }
    let ports = port_lines.join("\n");

    let cpu = c.host_config.as_ref()
        .and_then(|hc| hc.nano_cpus)
        .filter(|&n| n > 0)
        .map(|n| n as f64 / 1_000_000_000.0)
        .unwrap_or(0.0);

    let memory_mb = c.host_config.as_ref()
        .and_then(|hc| hc.memory)
        .filter(|&m| m > 0)
        .map(|m| m / (1024 * 1024))
        .unwrap_or(0);

    let volume_binds = c.host_config.as_ref()
        .and_then(|hc| hc.binds.clone())
        .unwrap_or_default();

    let labels = c.config.as_ref()
        .and_then(|cfg| cfg.labels.clone())
        .unwrap_or_default();

    let state = c.state.as_ref()
        .and_then(|s| s.status.as_ref())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(ContainerFullConfig { image, env, ports, cpu, memory_mb, volume_binds, labels, state })
}

/// Returns deduplicated (host_port, container_port) pairs for a container.
/// TCP and UDP entries for the same pair are collapsed into one entry.
pub async fn get_port_bindings(docker: &Docker, container_id: &str) -> Result<Vec<(u16, u16)>> {
    let cfg = inspect_full(docker, container_id).await?;
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for line in cfg.ports.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Some((hp_str, rest)) = line.split_once(':') {
            let cp_str = rest.split('/').next().unwrap_or(rest);
            if let (Ok(hp), Ok(cp)) = (hp_str.trim().parse::<u16>(), cp_str.trim().parse::<u16>()) {
                if seen.insert((hp, cp)) {
                    result.push((hp, cp));
                }
            }
        }
    }
    result.sort();
    Ok(result)
}

fn parse_ports_to_bindings(
    ports_str: &str,
) -> std::collections::HashMap<String, Option<Vec<bollard::models::PortBinding>>> {
    let mut map = std::collections::HashMap::new();
    for line in ports_str.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let (host_part, rest) = match line.split_once(':') {
            Some(t) => t,
            None => continue,
        };
        let (container_part, proto) = match rest.split_once('/') {
            Some((c, p)) => (c, p),
            None => (rest, "tcp"),
        };
        let key = format!("{}/{}", container_part.trim(), proto.trim());
        map.insert(key, Some(vec![bollard::models::PortBinding {
            host_ip: Some("0.0.0.0".to_string()),
            host_port: Some(host_part.trim().to_string()),
        }]));
    }
    map
}

/// Updates CPU / memory limits in-place via `docker update`. Zero = remove limit.
pub async fn update_container_resources(id: &str, cpu: f64, memory_mb: i64) -> Result<()> {
    let mut args: Vec<String> = vec!["update".to_string()];
    if cpu > 0.0 {
        args.extend_from_slice(&["--cpus".to_string(), format!("{:.4}", cpu)]);
    }
    if memory_mb > 0 {
        args.extend_from_slice(&[
            "--memory".to_string(), format!("{}m", memory_mb),
            "--memory-swap".to_string(), format!("{}m", memory_mb),
        ]);
    }
    args.push(id.to_string());
    let output = tokio::process::Command::new("docker")
        .args(&args)
        .output()
        .await
        .context("Failed to spawn docker update")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker update failed: {}", stderr);
    }
    Ok(())
}

/// Stops, removes, and recreates a container with updated config.
/// Volume bind-mount paths are preserved as-is; the `yunexal.volume_dir` label
/// is updated to the old container ID so `get_volume_dir()` still resolves.
/// Returns the new container ID.
pub async fn recreate_with_updated_config(
    docker: &Docker,
    old_id: &str,
    new_image: &str,
    new_env: &str,
    new_ports: &str,
    new_cpu: f64,
    new_memory_mb: i64,
    new_name: &str,
) -> Result<String> {
    let inspect = docker
        .inspect_container(old_id, None::<bollard::query_parameters::InspectContainerOptions>)
        .await
        .context("Container not found for recreate")?;

    let full_old_id = inspect.id.clone().unwrap_or_else(|| old_id.to_string());

    let mut host_config = inspect.host_config.clone().unwrap_or_default();
    host_config.nano_cpus  = if new_cpu > 0.0 { Some((new_cpu * 1_000_000_000.0) as i64) } else { None };
    host_config.memory     = if new_memory_mb > 0 { Some(new_memory_mb * 1024 * 1024) } else { None };
    host_config.memory_swap = if new_memory_mb > 0 { Some(new_memory_mb * 1024 * 1024) } else { None };
    let port_bindings = parse_ports_to_bindings(new_ports);
    host_config.port_bindings = if port_bindings.is_empty() { None } else { Some(port_bindings) };

    let env_vec: Vec<String> = new_env.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    // Preserve volume_dir label — extract the actual directory name from bind mounts.
    // The existing label may be stale after a previous recreate, so we prefer
    // the bind mount source path which is always correct.
    let mut labels = inspect.config.as_ref()
        .and_then(|cfg| cfg.labels.clone())
        .unwrap_or_default();

    let volume_dir_from_bind = host_config.binds.as_ref()
        .and_then(|binds| binds.first())
        .and_then(|b| b.split(':').next())
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .map(|s| s.to_string());

    if let Some(ref vdir) = volume_dir_from_bind {
        labels.insert("yunexal.volume_dir".to_string(), vdir.clone());
    } else if !labels.contains_key("yunexal.volume_dir") {
        labels.insert("yunexal.volume_dir".to_string(), full_old_id);
    }

    let new_config = bollard::models::ContainerCreateBody {
        image: Some(new_image.to_string()),
        env: if env_vec.is_empty() { None } else { Some(env_vec) },
        host_config: Some(host_config),
        labels: Some(labels),
        tty: Some(true),
        open_stdin: Some(true),
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Default::default()
    };

    ensure_image(docker, new_image).await?;
    let _ = stop_container(docker, old_id).await;
    remove_container(docker, old_id).await?;
    let new_id = create_container(docker, new_name, new_config).await?;
    Ok(new_id)
}

#[allow(dead_code)]
pub async fn recreate_container_with_cmd(docker: &Docker, id: &str, new_cmd: Option<Vec<String>>) -> Result<()> {
    let inspect = get_container_inspect(docker, id).await?;
    
    let config = inspect.config.ok_or_else(|| anyhow::anyhow!("No config found"))?;
    let host_config = inspect.host_config.clone().unwrap_or_default();
    let name = inspect.name.unwrap_or_default().trim_start_matches('/').to_string();

    let new_config = bollard::models::ContainerCreateBody {
        image: config.image,
        cmd: new_cmd,
        env: config.env,
        host_config: Some(host_config),
        labels: config.labels, // preserve yunexal.volume_dir and other labels
        tty: Some(true),
        open_stdin: Some(true),
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Default::default()
    };
    
    // Stop and Remove
    let _ = stop_container(docker, id).await;
    let _ = remove_container(docker, id).await;
    
    // Create
    create_container(docker, &name, new_config).await?;
    
    // Start
    start_container(docker, &name).await?;
    Ok(())
}

// ── Container Network Isolation ──────────────────────────────────────────────

/// Private CIDRs that managed containers must never reach.
const ISOLATED_CIDRS: &[&str] = &[
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "127.0.0.0/8",
    "169.254.0.0/16",   // link-local
    "100.64.0.0/10",    // CGNAT / shared-address space
];

/// Creates a per-container isolated Docker bridge network named `yunexal-{container_name}`.
/// - ICC disabled: containers on this bridge cannot peer-communicate.
/// - IP masquerade enabled: outbound NAT to the public internet still works.
///
/// Returns `(network_name, bridge_iface)`.  The bridge interface is always
/// `br-<first-12-hex-chars-of-network-id>`, which is how Docker names bridges.
pub async fn create_isolated_network(docker: &Docker, container_name: &str) -> Result<(String, String)> {
    use bollard::models::NetworkCreateRequest;

    let network_name = format!("yunexal-{}", container_name);

    // Remove any stale network with the same name first (idempotent).
    let _ = docker.remove_network(&network_name).await;

    let mut opts = std::collections::HashMap::new();
    opts.insert("com.docker.network.bridge.enable_icc".to_string(), "false".to_string());
    opts.insert("com.docker.network.bridge.enable_ip_masquerade".to_string(), "true".to_string());

    let resp = docker.create_network(NetworkCreateRequest {
        name: network_name.clone(),
        driver: Some("bridge".to_string()),
        options: Some(opts),
        ..Default::default()
    })
    .await
    .context("Failed to create isolated Docker network")?;

    let network_id = resp.id;
    let bridge = format!("br-{}", &network_id[..network_id.len().min(12)]);

    Ok((network_name, bridge))
}

/// Inserts iptables `DROP` rules into the `DOCKER-USER` chain so that packets
/// leaving `bridge` cannot reach private / loopback subnets.
pub async fn apply_isolation_rules(bridge: &str) {
    for cidr in ISOLATED_CIDRS {
        let _ = tokio::process::Command::new("iptables")
            .args(["-I", "DOCKER-USER", "-i", bridge, "-d", cidr, "-j", "DROP"])
            .output()
            .await;
    }
}

/// Removes iptables `DROP` rules from `DOCKER-USER` for `bridge`.
/// Safe to call even if the rules are already absent.
pub async fn remove_isolation_rules(bridge: &str) {
    for cidr in ISOLATED_CIDRS {
        let _ = tokio::process::Command::new("iptables")
            .args(["-D", "DOCKER-USER", "-i", bridge, "-d", cidr, "-j", "DROP"])
            .output()
            .await;
    }
}

/// Reads the `yunexal.network` label stored on the container at creation time.
pub async fn get_container_network_label(docker: &Docker, container_id: &str) -> Option<String> {
    let inspect = get_container_inspect(docker, container_id).await.ok()?;
    inspect.config?.labels?.get("yunexal.network").cloned()
}

/// Returns the Linux bridge interface name for a Docker network (by name or ID).
pub async fn get_bridge_for_network(docker: &Docker, network_name: &str) -> Option<String> {
    let inspect = docker
        .inspect_network(network_name, None::<bollard::query_parameters::InspectNetworkOptions>)
        .await
        .ok()?;
    let id = inspect.id?;
    Some(format!("br-{}", &id[..id.len().min(12)]))
}

/// Re-applies RFC1918 isolation rules for a container that is being (re)started.
/// Called alongside `reapply_bandwidth_limit` in every container-start path.
pub async fn reapply_isolation_rules(docker: &Docker, container_id: &str) {
    let Some(net_name) = get_container_network_label(docker, container_id).await else { return; };
    let Some(bridge)   = get_bridge_for_network(docker, &net_name).await           else { return; };
    apply_isolation_rules(&bridge).await;
}

/// Removes the container's dedicated network and its iptables rules.
///
/// **Must be called BEFORE `remove_container`** so that the `yunexal.network`
/// label is still readable from the container inspect.  The function force-
/// disconnects the container from the network so that `remove_network` succeeds
/// even while the container still exists.
pub async fn cleanup_isolation(docker: &Docker, container_id: &str) {
    use bollard::models::NetworkDisconnectRequest;

    let Some(net_name) = get_container_network_label(docker, container_id).await else { return; };

    // Remove iptables rules first.
    if let Some(bridge) = get_bridge_for_network(docker, &net_name).await {
        remove_isolation_rules(&bridge).await;
    }

    // Force-disconnect the container so the network has no active endpoints,
    // then remove it.  Both operations are best-effort.
    let _ = docker
        .disconnect_network(
            &net_name,
            NetworkDisconnectRequest {
                container: container_id.to_string(),
                force: Some(true),
            },
        )
        .await;
    let _ = docker.remove_network(&net_name).await;
}
