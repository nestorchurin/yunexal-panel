use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use cron::Schedule as CronSchedule;
use sqlx::{FromRow, Pool, Sqlite};
use std::str::FromStr;

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct Schedule {
    pub id: i64,
    pub server_id: i64,
    pub name: String,
    pub cron_expression: String,
    pub task_type: String,
    pub task_payload: String,
    pub enabled: i64,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct ScheduleRun {
    pub id: i64,
    pub schedule_id: i64,
    pub started_at: String,
    pub status: String,
    pub output: Option<String>,
}

pub fn compute_next_run(cron_expression: &str) -> Option<String> {
    // cron crate v0.12 requires 7 fields (sec min hour dom month dow year).
    // Accept the standard 5-field format users enter and prepend sec=0 + append year=*.
    let expr = {
        let parts: Vec<&str> = cron_expression.trim().split_whitespace().collect();
        if parts.len() == 5 {
            format!("0 {} *", parts.join(" "))
        } else {
            cron_expression.to_string()
        }
    };
    let schedule = CronSchedule::from_str(&expr).ok()?;
    let next: DateTime<Utc> = schedule.upcoming(Utc).next()?;
    Some(next.format("%Y-%m-%d %H:%M:%S").to_string())
}

pub async fn list_schedules(pool: &Pool<Sqlite>, server_id: i64) -> Result<Vec<Schedule>> {
    let rows = sqlx::query_as::<_, Schedule>(
        "SELECT id, server_id, name, cron_expression, task_type, task_payload, enabled, \
         last_run_at, next_run_at, created_at FROM schedules WHERE server_id = ? ORDER BY id ASC",
    )
    .bind(server_id)
    .fetch_all(pool)
    .await
    .context("list_schedules")?;
    Ok(rows)
}

pub async fn get_schedule(pool: &Pool<Sqlite>, id: i64) -> Result<Option<Schedule>> {
    let row = sqlx::query_as::<_, Schedule>(
        "SELECT id, server_id, name, cron_expression, task_type, task_payload, enabled, \
         last_run_at, next_run_at, created_at FROM schedules WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("get_schedule")?;
    Ok(row)
}

pub async fn create_schedule(
    pool: &Pool<Sqlite>,
    server_id: i64,
    name: &str,
    cron_expression: &str,
    task_type: &str,
    task_payload: &str,
) -> Result<i64> {
    let next_run = compute_next_run(cron_expression);
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO schedules (server_id, name, cron_expression, task_type, task_payload, next_run_at) \
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(server_id)
    .bind(name)
    .bind(cron_expression)
    .bind(task_type)
    .bind(task_payload)
    .bind(next_run)
    .fetch_one(pool)
    .await
    .context("create_schedule")?;
    Ok(id)
}

pub async fn update_schedule(
    pool: &Pool<Sqlite>,
    id: i64,
    name: &str,
    cron_expression: &str,
    task_type: &str,
    task_payload: &str,
    enabled: bool,
) -> Result<()> {
    let next_run = compute_next_run(cron_expression);
    sqlx::query(
        "UPDATE schedules SET name = ?, cron_expression = ?, task_type = ?, task_payload = ?, \
         enabled = ?, next_run_at = ? WHERE id = ?",
    )
    .bind(name)
    .bind(cron_expression)
    .bind(task_type)
    .bind(task_payload)
    .bind(enabled as i64)
    .bind(next_run)
    .bind(id)
    .execute(pool)
    .await
    .context("update_schedule")?;
    Ok(())
}

pub async fn delete_schedule(pool: &Pool<Sqlite>, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM schedules WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("delete_schedule")?;
    Ok(())
}

pub async fn set_schedule_enabled(pool: &Pool<Sqlite>, id: i64, enabled: bool) -> Result<()> {
    let next_run = if enabled {
        // Recompute next_run when re-enabling
        let expr: Option<String> = sqlx::query_scalar(
            "SELECT cron_expression FROM schedules WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("get cron for enable")?;
        expr.and_then(|e| compute_next_run(&e))
    } else {
        None
    };
    sqlx::query("UPDATE schedules SET enabled = ?, next_run_at = ? WHERE id = ?")
        .bind(enabled as i64)
        .bind(next_run)
        .bind(id)
        .execute(pool)
        .await
        .context("set_schedule_enabled")?;
    Ok(())
}

pub async fn update_schedule_after_run(
    pool: &Pool<Sqlite>,
    id: i64,
    last_run_at: &str,
    next_run_at: Option<String>,
) -> Result<()> {
    sqlx::query(
        "UPDATE schedules SET last_run_at = ?, next_run_at = ? WHERE id = ?",
    )
    .bind(last_run_at)
    .bind(next_run_at)
    .bind(id)
    .execute(pool)
    .await
    .context("update_schedule_after_run")?;
    Ok(())
}

pub async fn list_due_schedules(pool: &Pool<Sqlite>) -> Result<Vec<Schedule>> {
    let rows = sqlx::query_as::<_, Schedule>(
        "SELECT id, server_id, name, cron_expression, task_type, task_payload, enabled, \
         last_run_at, next_run_at, created_at FROM schedules \
         WHERE enabled = 1 AND next_run_at IS NOT NULL AND next_run_at <= datetime('now')",
    )
    .fetch_all(pool)
    .await
    .context("list_due_schedules")?;
    Ok(rows)
}

pub async fn insert_run(
    pool: &Pool<Sqlite>,
    schedule_id: i64,
    status: &str,
    output: Option<&str>,
) -> Result<i64> {
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO schedule_runs (schedule_id, status, output) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(schedule_id)
    .bind(status)
    .bind(output)
    .fetch_one(pool)
    .await
    .context("insert_run")?;
    Ok(id)
}

pub async fn finish_run(
    pool: &Pool<Sqlite>,
    run_id: i64,
    status: &str,
    output: &str,
) -> Result<()> {
    sqlx::query("UPDATE schedule_runs SET status = ?, output = ? WHERE id = ?")
        .bind(status)
        .bind(output)
        .bind(run_id)
        .execute(pool)
        .await
        .context("finish_run")?;
    Ok(())
}

pub async fn list_runs(
    pool: &Pool<Sqlite>,
    schedule_id: i64,
    limit: i64,
) -> Result<Vec<ScheduleRun>> {
    let rows = sqlx::query_as::<_, ScheduleRun>(
        "SELECT id, schedule_id, started_at, status, output FROM schedule_runs \
         WHERE schedule_id = ? ORDER BY id DESC LIMIT ?",
    )
    .bind(schedule_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("list_runs")?;
    Ok(rows)
}

/// Prune old runs for all schedules of the given server, keeping at most `max_runs` per server.
pub async fn prune_runs(pool: &Pool<Sqlite>, server_id: i64, max_runs: i64) -> Result<()> {
    sqlx::query(
        "DELETE FROM schedule_runs WHERE id NOT IN (
            SELECT id FROM schedule_runs
            WHERE schedule_id IN (SELECT id FROM schedules WHERE server_id = ?)
            ORDER BY id DESC LIMIT ?
         ) AND schedule_id IN (SELECT id FROM schedules WHERE server_id = ?)",
    )
    .bind(server_id)
    .bind(max_runs)
    .bind(server_id)
    .execute(pool)
    .await
    .context("prune_runs")?;
    Ok(())
}

pub async fn get_server_max_schedule_runs(pool: &Pool<Sqlite>, server_id: i64) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT max_schedule_runs FROM servers WHERE id = ?",
    )
    .bind(server_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(20)
}
