use bollard::Docker;
use anyhow::{Result, Context};

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

    // Preserve volume_dir label
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

    super::ensure_image(docker, new_image).await?;
    let _ = super::stop_container(docker, old_id).await;
    super::remove_container(docker, old_id).await?;
    let new_id = super::create_container(docker, new_name, new_config).await?;
    Ok(new_id)
}

#[allow(dead_code)]
pub async fn recreate_container_with_cmd(docker: &Docker, id: &str, new_cmd: Option<Vec<String>>) -> Result<()> {
    let inspect = super::get_container_inspect(docker, id).await?;
    
    let config = inspect.config.ok_or_else(|| anyhow::anyhow!("No config found"))?;
    let host_config = inspect.host_config.clone().unwrap_or_default();
    let name = inspect.name.unwrap_or_default().trim_start_matches('/').to_string();

    let new_config = bollard::models::ContainerCreateBody {
        image: config.image,
        cmd: new_cmd,
        env: config.env,
        host_config: Some(host_config),
        labels: config.labels,
        tty: Some(true),
        open_stdin: Some(true),
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Default::default()
    };
    
    // Stop and Remove
    let _ = super::stop_container(docker, id).await;
    let _ = super::remove_container(docker, id).await;
    
    // Create
    super::create_container(docker, &name, new_config).await?;
    
    // Start
    super::start_container(docker, &name).await?;
    Ok(())
}
