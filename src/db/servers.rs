use anyhow::{Context, Result};
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;

/// Returns true if a server with the given name already exists.
/// Optionally excludes `exclude_container_id` (pass the current container's ID
/// when renaming so a container can keep its own name).
pub async fn server_name_exists(
    pool: &Pool<Sqlite>,
    name: &str,
    exclude_container_id: Option<&str>,
) -> Result<bool> {
    let count: i64 = if let Some(excl) = exclude_container_id {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM servers WHERE name = ? COLLATE NOCASE AND container_id != ?"
        )
        .bind(name)
        .bind(excl)
        .fetch_one(pool)
        .await
        .context("server_name_exists query")?  
    } else {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM servers WHERE name = ? COLLATE NOCASE"
        )
        .bind(name)
        .fetch_one(pool)
        .await
        .context("server_name_exists query")?
    };
    Ok(count > 0)
}

/// Registers or updates a container's owner in the `servers` table.
/// Returns the SQLite row id.
pub async fn register_server(
    pool: &Pool<Sqlite>,
    container_id: &str,
    name: &str,
    owner_id: i64,
) -> Result<i64> {
    let row_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO servers (container_id, name, owner_id)
           VALUES (?, ?, ?)
           ON CONFLICT(container_id) DO UPDATE SET
               name = excluded.name,
               owner_id = excluded.owner_id
           RETURNING id"#,
    )
    .bind(container_id)
    .bind(name)
    .bind(owner_id)
    .fetch_one(pool)
    .await
    .context("Failed to register server")?;
    Ok(row_id)
}

/// Returns all container_ids owned by the given user.
pub async fn list_owned_container_ids(
    pool: &Pool<Sqlite>,
    owner_id: i64,
) -> Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT container_id FROM servers WHERE owner_id = ?",
    )
    .bind(owner_id)
    .fetch_all(pool)
    .await
    .context("Failed to list owned containers")?;
    Ok(rows)
}

/// Returns the Docker container_id for a given SQLite server id.
pub async fn get_container_id_by_server_id(
    pool: &Pool<Sqlite>,
    server_id: i64,
) -> Result<Option<String>> {
    let cid = sqlx::query_scalar::<_, String>(
        "SELECT container_id FROM servers WHERE id = ?",
    )
    .bind(server_id)
    .fetch_optional(pool)
    .await
    .context("get_container_id_by_server_id")?;
    Ok(cid)
}

/// Returns (container_id, display_name) for a given SQLite server id.
pub async fn get_server_info_by_db_id(
    pool: &Pool<Sqlite>,
    server_id: i64,
) -> Result<Option<(String, String)>> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT container_id, name FROM servers WHERE id = ?",
    )
    .bind(server_id)
    .fetch_optional(pool)
    .await
    .context("get_server_info_by_db_id")?;
    Ok(row)
}

/// Returns a map of Docker container_id → (SQLite server id, display name, owner username) for ALL servers.
pub async fn get_server_info_map(pool: &Pool<Sqlite>) -> Result<HashMap<String, (i64, String, String)>> {
    let rows = sqlx::query_as::<_, (String, i64, String, String)>(
        "SELECT s.container_id, s.id, s.name, COALESCE(u.username, '') \
         FROM servers s LEFT JOIN users u ON u.id = s.owner_id",
    )
    .fetch_all(pool)
    .await
    .context("get_server_info_map")?;
    Ok(rows.into_iter().map(|(cid, id, name, owner)| (cid, (id, name, owner))).collect())
}

/// Deletes a server record by Docker container_id.
pub async fn delete_server_by_container_id(
    pool: &Pool<Sqlite>,
    container_id: &str,
) -> Result<()> {
    sqlx::query("DELETE FROM servers WHERE container_id = ?")
        .bind(container_id)
        .execute(pool)
        .await
        .context("delete_server_by_container_id")?;
    Ok(())
}

/// Returns the owner_id for a container, or None if not registered.
pub async fn get_server_owner(pool: &Pool<Sqlite>, container_id: &str) -> Result<Option<i64>> {
    let owner_id = sqlx::query_scalar::<_, i64>(
        "SELECT owner_id FROM servers WHERE container_id = ?",
    )
    .bind(container_id)
    .fetch_optional(pool)
    .await
    .context("Failed to get server owner")?;
    Ok(owner_id)
}

/// Returns the owner_id for a server by its SQLite id, or None if not found.
pub async fn get_server_owner_by_db_id(pool: &Pool<Sqlite>, server_id: i64) -> Result<Option<i64>> {
    let owner_id = sqlx::query_scalar::<_, i64>(
        "SELECT owner_id FROM servers WHERE id = ?",
    )
    .bind(server_id)
    .fetch_optional(pool)
    .await
    .context("get_server_owner_by_db_id")?;
    Ok(owner_id)
}

/// Updates the container_id, name, and owner after recreating a container.
pub async fn update_server(
    pool: &Pool<Sqlite>,
    old_container_id: &str,
    new_container_id: &str,
    name: &str,
    owner_id: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE servers SET container_id = ?, name = ?, owner_id = ? WHERE container_id = ?",
    )
    .bind(new_container_id)
    .bind(name)
    .bind(owner_id)
    .bind(old_container_id)
    .execute(pool)
    .await
    .context("Failed to update server record")?;
    Ok(())
}

/// Updates only name and owner for an existing container (no recreate).
pub async fn update_server_name_and_owner(
    pool: &Pool<Sqlite>,
    container_id: &str,
    name: &str,
    owner_id: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE servers SET name = ?, owner_id = ? WHERE container_id = ?",
    )
    .bind(name)
    .bind(owner_id)
    .bind(container_id)
    .execute(pool)
    .await
    .context("Failed to update server name/owner")?;
    Ok(())
}

/// Updates only the name for an existing container, preserving owner_id.
pub async fn update_server_name_only(
    pool: &Pool<Sqlite>,
    container_id: &str,
    name: &str,
) -> Result<()> {
    sqlx::query("UPDATE servers SET name = ? WHERE container_id = ?")
        .bind(name)
        .bind(container_id)
        .execute(pool)
        .await
        .context("Failed to update server name")?;
    Ok(())
}

/// Returns basic info for every server: (id, display_name, owner_username).
pub async fn list_servers_basic_info(pool: &Pool<Sqlite>) -> Result<Vec<(i64, String, String)>> {
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        r#"SELECT s.id, s.name, COALESCE(u.username, '') as owner
           FROM servers s
           LEFT JOIN users u ON s.owner_id = u.id
           ORDER BY s.name"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
