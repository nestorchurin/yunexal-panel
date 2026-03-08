use anyhow::{Context, Result};
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;

/// Returns a map for all allocated ports of a server: (host_port, container_port) → (tag, enabled).
pub async fn get_port_tags(
    pool: &Pool<Sqlite>,
    server_id: i64,
) -> Result<HashMap<(i64, i64), (String, bool)>> {
    let rows: Vec<(i64, i64, String, i64)> = sqlx::query_as(
        "SELECT host_port, container_port, tag, enabled FROM server_ports WHERE server_id = ?",
    )
    .bind(server_id)
    .fetch_all(pool)
    .await
    .context("get_port_tags")?;
    Ok(rows.into_iter().map(|(hp, cp, tag, en)| ((hp, cp), (tag, en != 0))).collect())
}

/// Upserts a port description tag.
pub async fn set_port_tag(
    pool: &Pool<Sqlite>,
    server_id: i64,
    host_port: i64,
    container_port: i64,
    tag: &str,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO server_ports (server_id, host_port, container_port, tag)
           VALUES (?, ?, ?, ?)
           ON CONFLICT(server_id, host_port, container_port) DO UPDATE SET tag = excluded.tag"#,
    )
    .bind(server_id)
    .bind(host_port)
    .bind(container_port)
    .bind(tag)
    .execute(pool)
    .await
    .context("set_port_tag")?;
    Ok(())
}

/// Sets the enabled flag for a port entry (upserts if entry not yet tracked).
pub async fn set_port_enabled(
    pool: &Pool<Sqlite>,
    server_id: i64,
    host_port: i64,
    container_port: i64,
    enabled: bool,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO server_ports (server_id, host_port, container_port, tag, enabled)
           VALUES (?, ?, ?, '', ?)
           ON CONFLICT(server_id, host_port, container_port) DO UPDATE SET enabled = excluded.enabled"#,
    )
    .bind(server_id)
    .bind(host_port)
    .bind(container_port)
    .bind(if enabled { 1i64 } else { 0i64 })
    .execute(pool)
    .await
    .context("set_port_enabled")?;
    Ok(())
}

/// Removes a port entry (called when a port binding is closed).
pub async fn delete_port_entry(
    pool: &Pool<Sqlite>,
    server_id: i64,
    host_port: i64,
    container_port: i64,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM server_ports WHERE server_id = ? AND host_port = ? AND container_port = ?",
    )
    .bind(server_id)
    .bind(host_port)
    .bind(container_port)
    .execute(pool)
    .await
    .context("delete_port_entry")?;
    Ok(())
}
