use anyhow::{anyhow, Context, Result};
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;

pub const SERVER_MEMBER_PERMISSIONS: &[&str] = &[
    "console",
    "files",
    "networking",
    "audit",
    "settings",
    "power",
    "members",
    "schedules",
];

#[derive(Debug, Clone)]
pub struct ServerMemberPermissionEntry {
    pub user_id: i64,
    pub username: String,
    pub uid: String,
    pub nickname: String,
    pub permission: String,
    pub mode: String,
}

fn is_valid_member_permission(permission: &str) -> bool {
    SERVER_MEMBER_PERMISSIONS.iter().any(|p| *p == permission)
}

fn is_valid_policy_mode(mode: &str) -> bool {
    matches!(mode, "none" | "read" | "write")
}

fn default_member_mode(permission: &str) -> &'static str {
    match permission {
        "audit" => "read",
        "members" => "none",
        _ => "write",
    }
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

/// Returns container_ids the user can access either as owner or explicit member.
pub async fn list_accessible_container_ids(
    pool: &Pool<Sqlite>,
    user_id: i64,
) -> Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT s.container_id \
         FROM servers s \
         LEFT JOIN server_user_permissions sup \
            ON sup.server_id = s.id \
           AND sup.user_id = ? \
         WHERE s.owner_id = ? OR sup.server_id IS NOT NULL",
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
    .context("Failed to list accessible containers")?;
    Ok(rows)
}

pub async fn add_server_member_with_defaults(
    pool: &Pool<Sqlite>,
    server_id: i64,
    user_id: i64,
) -> Result<()> {
    for permission in SERVER_MEMBER_PERMISSIONS {
        sqlx::query(
            "INSERT OR IGNORE INTO server_user_permissions (server_id, user_id, permission, mode) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(server_id)
        .bind(user_id)
        .bind(*permission)
        .bind(default_member_mode(permission))
        .execute(pool)
        .await
        .context("add_server_member_with_defaults")?;
    }
    Ok(())
}

pub async fn remove_server_member(
    pool: &Pool<Sqlite>,
    server_id: i64,
    user_id: i64,
) -> Result<()> {
    sqlx::query("DELETE FROM server_user_permissions WHERE server_id = ? AND user_id = ?")
        .bind(server_id)
        .bind(user_id)
        .execute(pool)
        .await
        .context("remove_server_member")?;
    Ok(())
}

pub async fn server_member_exists(
    pool: &Pool<Sqlite>,
    server_id: i64,
    user_id: i64,
) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM server_user_permissions WHERE server_id = ? AND user_id = ?",
    )
    .bind(server_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .context("server_member_exists")?;
    Ok(count > 0)
}

pub async fn set_server_member_permission_policy(
    pool: &Pool<Sqlite>,
    server_id: i64,
    user_id: i64,
    permission: &str,
    mode: &str,
) -> Result<()> {
    if !is_valid_member_permission(permission) {
        return Err(anyhow!("invalid permission"));
    }
    if !is_valid_policy_mode(mode) {
        return Err(anyhow!("invalid mode"));
    }
    sqlx::query(
        "INSERT INTO server_user_permissions (server_id, user_id, permission, mode) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT(server_id, user_id, permission) DO UPDATE SET mode = excluded.mode",
    )
    .bind(server_id)
    .bind(user_id)
    .bind(permission)
    .bind(mode)
    .execute(pool)
    .await
    .context("set_server_member_permission_policy")?;
    Ok(())
}

pub async fn server_user_has_read_permission(
    pool: &Pool<Sqlite>,
    server_id: i64,
    user_id: i64,
    permission: &str,
) -> Result<bool> {
    if !is_valid_member_permission(permission) {
        return Ok(false);
    }
    let mode = sqlx::query_scalar::<_, String>(
        "SELECT mode FROM server_user_permissions WHERE server_id = ? AND user_id = ? AND permission = ?",
    )
    .bind(server_id)
    .bind(user_id)
    .bind(permission)
    .fetch_optional(pool)
    .await
    .context("server_user_has_read_permission")?;
    Ok(matches!(mode.as_deref(), Some("read") | Some("write")))
}

pub async fn server_user_has_write_permission(
    pool: &Pool<Sqlite>,
    server_id: i64,
    user_id: i64,
    permission: &str,
) -> Result<bool> {
    if !is_valid_member_permission(permission) {
        return Ok(false);
    }
    let mode = sqlx::query_scalar::<_, String>(
        "SELECT mode FROM server_user_permissions WHERE server_id = ? AND user_id = ? AND permission = ?",
    )
    .bind(server_id)
    .bind(user_id)
    .bind(permission)
    .fetch_optional(pool)
    .await
    .context("server_user_has_write_permission")?;
    Ok(matches!(mode.as_deref(), Some("write")))
}

pub async fn server_user_has_any_access(
    pool: &Pool<Sqlite>,
    server_id: i64,
    user_id: i64,
) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) \
         FROM server_user_permissions \
         WHERE server_id = ? \
           AND user_id = ? \
           AND permission IN ('console','files','networking','audit','settings') \
           AND mode IN ('read','write')",
    )
    .bind(server_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .context("server_user_has_any_access")?;
    Ok(count > 0)
}

pub async fn list_server_member_permissions(
    pool: &Pool<Sqlite>,
    server_id: i64,
) -> Result<Vec<ServerMemberPermissionEntry>> {
    let rows = sqlx::query_as::<_, (i64, String, String, String, String, String)>(
        "SELECT sup.user_id, u.username, COALESCE(u.uid, ''), COALESCE(u.nickname, ''), sup.permission, sup.mode \
         FROM server_user_permissions sup \
         JOIN users u ON u.id = sup.user_id \
         WHERE sup.server_id = ? \
         ORDER BY COALESCE(NULLIF(u.nickname, ''), u.username) ASC, sup.permission ASC",
    )
    .bind(server_id)
    .fetch_all(pool)
    .await
    .context("list_server_member_permissions")?;

    Ok(rows
        .into_iter()
        .map(|(user_id, username, uid, nickname, permission, mode)| ServerMemberPermissionEntry {
            user_id,
            username,
            uid,
            nickname,
            permission,
            mode,
        })
        .collect())
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
        "SELECT s.container_id, s.id, s.name, COALESCE(NULLIF(u.nickname, ''), u.username, '') \
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
    sqlx::query(
        "DELETE FROM server_user_permissions WHERE server_id IN (SELECT id FROM servers WHERE container_id = ?)",
    )
    .bind(container_id)
    .execute(pool)
    .await
    .context("delete server member permissions by container_id")?;

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

/// Updates max_schedule_runs for a server row (by DB id).
pub async fn update_server_max_schedule_runs(pool: &Pool<Sqlite>, server_id: i64, max_runs: i64) -> Result<()> {
    sqlx::query("UPDATE servers SET max_schedule_runs = ? WHERE id = ?")
        .bind(max_runs)
        .bind(server_id)
        .execute(pool)
        .await
        .context("Failed to update max_schedule_runs")?;
    Ok(())
}

pub async fn get_server_max_backups(pool: &Pool<Sqlite>, server_id: i64) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT max_backups FROM servers WHERE id = ?")
        .bind(server_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(10)
}

pub async fn update_server_max_backups(pool: &Pool<Sqlite>, server_id: i64, max_backups: i64) -> Result<()> {
    sqlx::query("UPDATE servers SET max_backups = ? WHERE id = ?")
        .bind(max_backups)
        .bind(server_id)
        .execute(pool)
        .await
        .context("Failed to update max_backups")?;
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
        r#"SELECT s.id, s.name, COALESCE(NULLIF(u.nickname, ''), u.username, '') as owner
           FROM servers s
           LEFT JOIN users u ON s.owner_id = u.id
           ORDER BY s.name"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── SFTP auth helper ──────────────────────────────────────────────────────────

pub struct SftpAuthInfo {
    pub container_id: String,
    pub password_hash: String,
    pub has_access: bool,
}

/// Returns auth info for an SFTP login attempt: password hash + access flag.
/// Access is granted if the user is the server owner or has files=write permission.
pub async fn get_sftp_auth_info(
    pool: &Pool<Sqlite>,
    server_id: i64,
    username: &str,
) -> Result<Option<SftpAuthInfo>> {
    let row = sqlx::query_as::<_, (String, String, i64)>(
        r#"SELECT s.container_id, u.password_hash,
              CASE WHEN s.owner_id = u.id THEN 1
                   WHEN perm.mode = 'write' THEN 1
                   ELSE 0 END
           FROM servers s
           JOIN users u ON u.username = ?2
           LEFT JOIN server_user_permissions perm
             ON perm.server_id = s.id AND perm.user_id = u.id AND perm.permission = 'files'
           WHERE s.id = ?1
           LIMIT 1"#,
    )
    .bind(server_id)
    .bind(username)
    .fetch_optional(pool)
    .await
    .context("get_sftp_auth_info")?;

    Ok(row.map(|(container_id, password_hash, access)| SftpAuthInfo {
        container_id,
        password_hash,
        has_access: access == 1,
    }))
}

/// Returns server rows needed by startup migration: (db_id, container_id, name).
pub async fn list_servers_with_container_ids(
    pool: &Pool<Sqlite>,
) -> Result<Vec<(i64, String, String)>> {
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, container_id, name FROM servers ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .context("list_servers_with_container_ids")?;
    Ok(rows)
}

// ── ENV variable permissions ──────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnvPermission {
    pub user_editable: bool,
    pub user_visible: bool,
}

impl Default for EnvPermission {
    fn default() -> Self {
        Self { user_editable: false, user_visible: true }
    }
}

pub async fn get_env_permissions(
    pool: &Pool<Sqlite>,
    server_id: i64,
) -> Result<HashMap<String, EnvPermission>> {
    let rows = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT key, user_editable, user_visible FROM server_env_permissions WHERE server_id = ?"
    )
    .bind(server_id)
    .fetch_all(pool)
    .await
    .context("get_env_permissions")?;

    Ok(rows.into_iter().map(|(k, e, v)| (k, EnvPermission {
        user_editable: e != 0,
        user_visible: v != 0,
    })).collect())
}

pub async fn set_env_permission(
    pool: &Pool<Sqlite>,
    server_id: i64,
    key: &str,
    editable: bool,
    visible: bool,
) -> Result<()> {
    if !editable && visible {
        // both are defaults → clean up row if it exists
        sqlx::query(
            "DELETE FROM server_env_permissions WHERE server_id = ? AND key = ?"
        )
        .bind(server_id)
        .bind(key)
        .execute(pool)
        .await
        .context("set_env_permission delete")?;
    } else {
        sqlx::query(
            "INSERT INTO server_env_permissions (server_id, key, user_editable, user_visible) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(server_id, key) DO UPDATE SET \
               user_editable = excluded.user_editable, \
               user_visible  = excluded.user_visible"
        )
        .bind(server_id)
        .bind(key)
        .bind(editable as i64)
        .bind(visible as i64)
        .execute(pool)
        .await
        .context("set_env_permission upsert")?;
    }
    Ok(())
}

pub async fn cleanup_env_permissions(
    pool: &Pool<Sqlite>,
    server_id: i64,
    live_keys: &[&str],
) -> Result<()> {
    if live_keys.is_empty() {
        sqlx::query("DELETE FROM server_env_permissions WHERE server_id = ?")
            .bind(server_id)
            .execute(pool)
            .await
            .context("cleanup_env_permissions")?;
        return Ok(());
    }
    let placeholders = live_keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "DELETE FROM server_env_permissions WHERE server_id = ? AND key NOT IN ({})",
        placeholders
    );
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(server_id);
    for k in live_keys { q = q.bind(k); }
    q.execute(pool).await.context("cleanup_env_permissions")?;
    Ok(())
}
