use anyhow::{Context, Result};
use sqlx::{sqlite::SqliteConnectOptions, FromRow, Pool, Sqlite, SqlitePool};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::info;

// ── User model ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
}

// ── DB init ──────────────────────────────────────────────────────────────────

pub async fn init_db() -> Result<Pool<Sqlite>> {
    let db_url = "sqlite://yunexal.db?mode=rwc";

    let options = SqliteConnectOptions::from_str(db_url)?
        .create_if_missing(true);

    let pool = SqlitePool::connect_with(options)
        .await
        .context("Failed to connect to database")?;

    // WAL mode for better async concurrency
    sqlx::query("PRAGMA journal_mode=WAL;")
        .execute(&pool)
        .await
        .context("Failed to enable WAL mode")?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS servers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            container_id TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            owner_id INTEGER DEFAULT 0,
            status TEXT DEFAULT 'stopped'
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create servers table")?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            username    TEXT    NOT NULL UNIQUE,
            password_hash TEXT  NOT NULL,
            role        TEXT    NOT NULL DEFAULT 'user',
            created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create users table")?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS server_ports (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            server_id      INTEGER NOT NULL,
            host_port      INTEGER NOT NULL,
            container_port INTEGER NOT NULL,
            tag            TEXT    NOT NULL DEFAULT '',
            enabled        INTEGER NOT NULL DEFAULT 1,
            UNIQUE(server_id, host_port, container_port)
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create server_ports table")?;

    // Migration: add enabled column for existing databases (no-op if already present)
    let _ = sqlx::query(
        "ALTER TABLE server_ports ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1"
    )
    .execute(&pool)
    .await;

    // Unique name constraint (best-effort — no-op if already exists or if
    // there are pre-existing duplicates)
    let _ = sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_servers_name ON servers (name COLLATE NOCASE)"
    )
    .execute(&pool)
    .await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS image_env_overrides (
            image_id TEXT PRIMARY KEY,
            env      TEXT NOT NULL DEFAULT ''
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create image_env_overrides table")?;

    info!("Database initialized successfully");
    Ok(pool)
}

// ── Image ENV overrides ───────────────────────────────────────────────────────

pub async fn get_image_env(pool: &Pool<Sqlite>, image_id: &str) -> Result<String> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT env FROM image_env_overrides WHERE image_id = ?",
    )
    .bind(image_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch image env overrides")?;
    Ok(row.map(|(e,)| e).unwrap_or_default())
}

pub async fn set_image_env(pool: &Pool<Sqlite>, image_id: &str, env: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO image_env_overrides (image_id, env) VALUES (?, ?)
         ON CONFLICT(image_id) DO UPDATE SET env = excluded.env",
    )
    .bind(image_id)
    .bind(env)
    .execute(pool)
    .await
    .context("Failed to upsert image env overrides")?;
    Ok(())
}

pub async fn delete_image_env(pool: &Pool<Sqlite>, image_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM image_env_overrides WHERE image_id = ?")
        .bind(image_id)
        .execute(pool)
        .await
        .context("delete_image_env")?;
    Ok(())
}

// ── User CRUD ────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub async fn user_count(pool: &Pool<Sqlite>) -> Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .context("Failed to count users")?;
    Ok(count)
}

pub async fn create_user(
    pool: &Pool<Sqlite>,
    username: &str,
    password_hash: &str,
    role: &str,
) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, role) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(username)
    .bind(password_hash)
    .bind(role)
    .fetch_one(pool)
    .await
    .context("Failed to create user")?;
    Ok(id)
}

pub async fn list_users(pool: &Pool<Sqlite>) -> Result<Vec<User>> {
    let users = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, role, created_at FROM users ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .context("Failed to list users")?;
    Ok(users)
}

pub async fn find_user_by_username(
    pool: &Pool<Sqlite>,
    username: &str,
) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, role, created_at FROM users WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .context("Failed to find user")?;
    Ok(user)
}

pub async fn find_user_by_id(pool: &Pool<Sqlite>, id: i64) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, role, created_at FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("Failed to find user by id")?;
    Ok(user)
}

pub async fn delete_user(pool: &Pool<Sqlite>, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to delete user")?;
    Ok(())
}

pub async fn update_user_password(
    pool: &Pool<Sqlite>,
    id: i64,
    password_hash: &str,
) -> Result<()> {
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(password_hash)
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to update password")?;
    Ok(())
}

/// Returns true if the role has admin-level privileges.
pub fn is_admin_role(role: &str) -> bool {
    matches!(role, "root" | "admin")
}

/// Upserts the .env user with role `root` on every startup.
/// If the user doesn't exist, creates them.
/// If they exist, ensures their role stays `root`.
pub async fn seed_root_user(
    pool: &Pool<Sqlite>,
    username: &str,
    password_hash: &str,
    role: &str,
) -> Result<()> {
    // Try inserting; on conflict update the hash + role.
    sqlx::query(
        r#"INSERT INTO users (username, password_hash, role)
           VALUES (?, ?, ?)
           ON CONFLICT(username) DO UPDATE SET
               password_hash = excluded.password_hash,
               role = excluded.role"#,
    )
    .bind(username)
    .bind(password_hash)
    .bind(role)
    .execute(pool)
    .await
    .context("Failed to upsert root user")?;
    info!("Root user '{}' ensured with role '{}'.", username, role);
    Ok(())
}

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
