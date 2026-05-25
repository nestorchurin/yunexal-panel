use bollard::Docker;
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use std::time::Duration;
use tracing::{error, info, warn};

use crate::db;
use crate::db::schedules::{self, Schedule};
use crate::docker;

pub async fn start(pool: Pool<Sqlite>, docker: Docker) {
    info!("Scheduler started");
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        interval.tick().await;
        tick(&pool, &docker).await;
    }
}

async fn tick(pool: &Pool<Sqlite>, docker: &Docker) {
    let due = match schedules::list_due_schedules(pool).await {
        Ok(v) => v,
        Err(e) => {
            error!("Scheduler: failed to list due schedules: {e}");
            return;
        }
    };

    for s in due {
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let next_run = schedules::compute_next_run(&s.cron_expression);

        if let Err(e) = schedules::update_schedule_after_run(pool, s.id, &now, next_run).await {
            warn!("Scheduler: failed to bump next_run_at for schedule {}: {e}", s.id);
        }

        let pool2 = pool.clone();
        let docker2 = docker.clone();
        tokio::spawn(async move {
            execute(&pool2, &docker2, s).await;
        });
    }
}

pub async fn execute_manual(pool: &Pool<Sqlite>, docker: &Docker, schedule: Schedule) {
    execute(pool, docker, schedule).await;
}

async fn execute(pool: &Pool<Sqlite>, docker: &Docker, schedule: Schedule) {
    let container_id = match db::get_container_id_by_server_id(pool, schedule.server_id).await {
        Ok(Some(cid)) => cid,
        Ok(None) => {
            error!("Scheduler: server {} not found", schedule.server_id);
            return;
        }
        Err(e) => {
            error!("Scheduler: DB error for server {}: {e}", schedule.server_id);
            return;
        }
    };

    let run_id = match schedules::insert_run(pool, schedule.id, "running", None).await {
        Ok(id) => id,
        Err(e) => {
            error!("Scheduler: failed to insert run for schedule {}: {e}", schedule.id);
            return;
        }
    };

    let result: Result<String, String> = match schedule.task_type.as_str() {
        "command" => run_command(docker, &container_id, &schedule.task_payload).await.map_err(|e| e.to_string()),
        "power"   => run_power(docker, &container_id, &schedule.task_payload).await.map_err(|e| e.to_string()),
        "backup"  => run_backup(docker, &container_id, &schedule.task_payload).await.map_err(|e| e.to_string()),
        other     => Err(format!("Unknown task type: {other}")),
    };

    let (status, output) = match result {
        Ok(msg) => ("success", msg),
        Err(e)  => ("error", e),
    };

    if let Err(e) = schedules::finish_run(pool, run_id, status, &output).await {
        error!("Scheduler: failed to finish run {run_id}: {e}");
    }

    let max_runs = schedules::get_server_max_schedule_runs(pool, schedule.server_id).await;
    let _ = schedules::prune_runs(pool, schedule.server_id, max_runs).await;

    let detail = format!("server={} type={} status={}", schedule.server_id, schedule.task_type, status);
    let _ = db::audit_log(pool, "scheduler", "schedule.run", &schedule.name, &detail, "127.0.0.1", "scheduler").await;
}

async fn run_command(
    docker: &Docker,
    container_id: &str,
    task_payload: &str,
) -> anyhow::Result<String> {
    use bollard::exec::{CreateExecOptions, StartExecOptions};
    use futures_util::StreamExt;

    let payload: serde_json::Value = serde_json::from_str(task_payload)
        .unwrap_or(serde_json::json!({}));
    let cmd_str = payload.get("cmd")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if cmd_str.is_empty() {
        return Err(anyhow::anyhow!("Empty command"));
    }

    let exec_id = docker.create_exec(
        container_id,
        CreateExecOptions {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            cmd: Some(vec!["sh", "-c", &cmd_str]),
            ..Default::default()
        },
    ).await?.id;

    let mut output = String::new();
    if let bollard::exec::StartExecResults::Attached { output: mut stream, .. } =
        docker.start_exec(&exec_id, None::<StartExecOptions>).await?
    {
        while let Some(chunk) = stream.next().await {
            match chunk? {
                bollard::container::LogOutput::StdOut { message } |
                bollard::container::LogOutput::StdErr { message } => {
                    output.push_str(&String::from_utf8_lossy(&message));
                }
                _ => {}
            }
        }
    }

    let output = output.trim().to_string();
    let summary = if output.len() > 500 { format!("{}…", &output[..500]) } else { output };
    Ok(if summary.is_empty() { "OK".into() } else { summary })
}

async fn run_power(
    docker: &Docker,
    container_id: &str,
    task_payload: &str,
) -> anyhow::Result<String> {
    let payload: serde_json::Value = serde_json::from_str(task_payload)
        .unwrap_or(serde_json::json!({}));
    let action = payload.get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("restart");

    match action {
        "start"   => docker::start_container(docker, container_id).await?,
        "stop"    => docker::stop_container(docker, container_id).await?,
        "restart" => {
            let _ = docker::stop_container(docker, container_id).await;
            docker::start_container(docker, container_id).await?;
        }
        "kill" => docker::kill_container(docker, container_id).await?,
        other  => return Err(anyhow::anyhow!("Unknown power action: {other}")),
    }

    Ok(format!("{action} completed"))
}

async fn run_backup(
    docker: &Docker,
    container_id: &str,
    task_payload: &str,
) -> anyhow::Result<String> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::fs;

    let volume_dir = docker::get_volume_dir(docker, container_id).await
        .map_err(|e| anyhow::anyhow!("get_volume_dir: {e}"))?;

    let payload: serde_json::Value = serde_json::from_str(task_payload)
        .unwrap_or(serde_json::json!({}));
    let prefix = payload.get("prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("backup");

    let backups_dir = format!("{}/backups", volume_dir);
    fs::create_dir_all(&backups_dir)?;

    let ts = Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{backups_dir}/{prefix}-{ts}.tar.gz");
    let short_name = format!("{prefix}-{ts}.tar.gz");

    let file = fs::File::create(&filename)?;
    let gz = GzEncoder::new(file, Compression::default());
    let mut archive = tar::Builder::new(gz);

    // Archive volume contents, skipping the backups dir itself to avoid recursion
    for entry in fs::read_dir(&volume_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "backups" {
            continue;
        }
        let path = entry.path();
        let rel = name.to_string_lossy().into_owned();
        if path.is_dir() {
            archive.append_dir_all(&rel, &path)?;
        } else {
            archive.append_path_with_name(&path, &rel)?;
        }
    }

    drop(archive);

    let meta = fs::metadata(&filename)?;
    let size_mb = meta.len() as f64 / 1_048_576.0;
    Ok(format!("{} ({:.1} MB)", short_name, size_mb))
}
