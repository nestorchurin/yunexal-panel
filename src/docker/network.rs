use bollard::Docker;
use anyhow::{Result, Context};

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

/// Inserts iptables `DROP` rules into the `DOCKER-USER` chain.
pub async fn apply_isolation_rules(bridge: &str) {
    for cidr in ISOLATED_CIDRS {
        let _ = tokio::process::Command::new("iptables")
            .args(["-I", "DOCKER-USER", "-i", bridge, "-d", cidr, "-j", "DROP"])
            .output()
            .await;
    }
}

/// Removes iptables `DROP` rules from `DOCKER-USER` for `bridge`.
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
    let inspect = super::get_container_inspect(docker, container_id).await.ok()?;
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
pub async fn reapply_isolation_rules(docker: &Docker, container_id: &str) {
    let Some(net_name) = get_container_network_label(docker, container_id).await else { return; };
    let Some(bridge)   = get_bridge_for_network(docker, &net_name).await           else { return; };
    apply_isolation_rules(&bridge).await;
}

/// Removes the container's dedicated network and its iptables rules.
///
/// **Must be called BEFORE `remove_container`** so that the `yunexal.network`
/// label is still readable from the container inspect.
pub async fn cleanup_isolation(docker: &Docker, container_id: &str) {
    use bollard::models::NetworkDisconnectRequest;

    let Some(net_name) = get_container_network_label(docker, container_id).await else { return; };

    // Remove iptables rules first.
    if let Some(bridge) = get_bridge_for_network(docker, &net_name).await {
        remove_isolation_rules(&bridge).await;
    }

    // Force-disconnect the container so the network has no active endpoints,
    // then remove it.
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
