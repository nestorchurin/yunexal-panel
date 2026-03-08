use bollard::Docker;
use serde::Serialize;
use anyhow::Result;

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
