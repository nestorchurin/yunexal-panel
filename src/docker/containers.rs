use bollard::query_parameters::{ListContainersOptions, StartContainerOptions, StopContainerOptions, AttachContainerOptions};
use bollard::container::LogOutput;
use bollard::Docker;
use anyhow::{Result, Context};
use futures_util::Stream;
use std::pin::Pin;
use tokio::io::AsyncWrite;
use std::time::SystemTime;

use super::ContainerInfo;

// ── Uptime helper ────────────────────────────────────────────────────────────

/// Parses an RFC3339 timestamp and returns a human-readable uptime string.
fn parse_uptime(started_at: &str) -> Option<String> {
    // Docker returns timestamps like "2026-02-28T12:00:00.000000000Z"
    let started_at = started_at.trim();
    if started_at.is_empty() || started_at == "0001-01-01T00:00:00Z" {
        return None;
    }

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

// ── Container listing ────────────────────────────────────────────────────────

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
            let state = c.state.map(|s| s.to_string()).unwrap_or_default();
            let status = c.status.unwrap_or_default();

            let (cpu_usage, ram_usage) = if state == "running" {
                super::get_container_stats(&docker, &id).await.unwrap_or(("0.0%".into(), "0MB / 0MB".into()))
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

// ── Single container ─────────────────────────────────────────────────────────

pub async fn get_container(docker: &Docker, id: &str) -> Result<ContainerInfo> {
    use bollard::query_parameters::InspectContainerOptions;

    let c = docker.inspect_container(id, None::<InspectContainerOptions>).await
        .context("Container not found")?;

    let name = c.name.as_deref().unwrap_or("unknown").trim_start_matches('/').to_string();
    let id = c.id.clone().unwrap_or_default();
    let short_id = if id.len() > 12 { &id[..12] } else { &id }.to_string();

    let state = c.state.clone().unwrap_or_default();
    let state_str = state.status.map(|s| s.to_string()).unwrap_or_else(|| "unknown".into());
    
    let status = if state_str == "running" {
        let started = state.started_at.as_deref().unwrap_or("");
        if let Some(uptime) = parse_uptime(started) {
            uptime
        } else {
            "Running".to_string()
        }
    } else {
        state_str.clone()
    };
    
    let (cpu_usage, ram_usage) = if state_str == "running" {
        super::get_container_stats(docker, &id).await.unwrap_or(("Error".into(), "Error".into()))
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

// ── Container lifecycle ──────────────────────────────────────────────────────

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

// ── Container inspect helpers ────────────────────────────────────────────────

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

// ── Container attach (WebSocket) ─────────────────────────────────────────────

pub type DockerStream = Pin<Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>>;
pub type DockerInput = Pin<Box<dyn AsyncWrite + Send>>;

pub async fn attach_container(docker: &Docker, id: &str) -> Result<(DockerStream, DockerInput)> {
    let options = AttachContainerOptions {
        stdout: true,
        stderr: true,
        stdin: true,
        stream: true,
        logs: true,
        ..Default::default()
    };
    
    let result = docker.attach_container(id, Some(options)).await?;
    
    Ok((Box::pin(result.output), result.input))
}
