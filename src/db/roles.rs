use anyhow::{Context, Result};
use sqlx::{Pool, Row, Sqlite};
use std::collections::HashMap;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Role {
    pub name: String,
    pub description: String,
    pub color: String,
    pub is_system: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct PermissionDef {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PermissionPolicyRow {
    pub role_name: String,
    pub permission: String,
    pub mode: String,
}

const PERMISSION_CATALOG: &[PermissionDef] = &[
    PermissionDef {
        key: "admin.access",
        label: "Admin Access",
        description: "Allows entering /admin area.",
    },
    PermissionDef {
        key: "tab.overview",
        label: "Tab: Overview",
        description: "Read admin overview dashboard.",
    },
    PermissionDef {
        key: "tab.containers",
        label: "Tab: Containers",
        description: "Access containers tab.",
    },
    PermissionDef {
        key: "tab.images",
        label: "Tab: Images",
        description: "Access images tab.",
    },
    PermissionDef {
        key: "tab.users",
        label: "Tab: Users",
        description: "Access users tab.",
    },
    PermissionDef {
        key: "tab.roles",
        label: "Tab: Roles",
        description: "Access roles tab.",
    },
    PermissionDef {
        key: "tab.audit",
        label: "Tab: Audit",
        description: "Access audit log tab.",
    },
    PermissionDef {
        key: "tab.settings",
        label: "Tab: Settings",
        description: "Access panel settings tab.",
    },
    PermissionDef {
        key: "servers.create",
        label: "Create Servers",
        description: "Create new servers.",
    },
    PermissionDef {
        key: "servers.edit",
        label: "Edit Servers",
        description: "Edit existing servers.",
    },
    PermissionDef {
        key: "servers.delete",
        label: "Delete Servers",
        description: "Delete servers from panel.",
    },
    PermissionDef {
        key: "servers.global_access",
        label: "Global Server Access",
        description: "Access all servers regardless of owner.",
    },
    PermissionDef {
        key: "containers.manage",
        label: "Manage Containers",
        description: "Start/stop/restart/kill and view container list.",
    },
    PermissionDef {
        key: "images.manage",
        label: "Manage Images",
        description: "Pull/delete/duplicate images and edit ENV overrides.",
    },
    PermissionDef {
        key: "users.manage",
        label: "Manage Users",
        description: "Create/delete users and change their passwords/roles.",
    },
    PermissionDef {
        key: "roles.manage",
        label: "Manage Roles",
        description: "Create roles and change permission sets.",
    },
    PermissionDef {
        key: "audit.read",
        label: "Read Audit Log",
        description: "Read audit events.",
    },
    PermissionDef {
        key: "audit.view_ip",
        label: "Audit: View IP Addresses",
        description: "See real IP addresses in audit log entries.",
    },
    PermissionDef {
        key: "panel.update",
        label: "Panel Updates",
        description: "Check/apply panel updates.",
    },
    PermissionDef {
        key: "panel.settings",
        label: "Panel Settings",
        description: "Change panel settings.",
    },
    PermissionDef {
        key: "storage.manage",
        label: "Storage Management",
        description: "Use storage migration and filesystem management APIs.",
    },
    PermissionDef {
        key: "security.manage",
        label: "Security Controls",
        description: "Toggle host firewall controls.",
    },
    PermissionDef {
        key: "theme.manage",
        label: "Theme Management",
        description: "Change panel theme assets.",
    },
];

pub fn permission_catalog() -> &'static [PermissionDef] {
    PERMISSION_CATALOG
}

pub fn is_valid_permission(permission: &str) -> bool {
    PERMISSION_CATALOG.iter().any(|p| p.key == permission)
}

pub fn is_valid_role_name(name: &str) -> bool {
    let n = name.trim();
    if n.len() < 2 || n.len() > 32 {
        return false;
    }
    n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn default_role_color(role: &str) -> &'static str {
    match role {
        "root" => "#ef4444",
        "admin" => "#a78bfa",
        "user" => "#60a5fa",
        _ => "#94a3b8",
    }
}

pub fn normalize_role_color(raw: &str) -> Option<String> {
    let c = raw.trim();
    if c.is_empty() {
        return None;
    }
    if !c.starts_with('#') {
        return None;
    }
    let hex = &c[1..];
    let ok = (hex.len() == 6 || hex.len() == 3) && hex.chars().all(|ch| ch.is_ascii_hexdigit());
    if !ok {
        return None;
    }
    Some(c.to_lowercase())
}

fn default_role_permissions(role: &str) -> &'static [&'static str] {
    match role {
        "root" => &[
            "admin.access",
            "tab.overview",
            "tab.containers",
            "tab.images",
            "tab.users",
            "tab.roles",
            "tab.audit",
            "tab.settings",
            "servers.create",
            "servers.edit",
            "servers.delete",
            "servers.global_access",
            "containers.manage",
            "images.manage",
            "users.manage",
            "roles.manage",
            "audit.read",
            "audit.view_ip",
            "panel.update",
            "storage.manage",
            "security.manage",
            "theme.manage",
        ],
        "admin" => &[
            "admin.access",
            "tab.overview",
            "tab.containers",
            "tab.images",
            "tab.users",
            "tab.roles",
            "tab.audit",
            "servers.create",
            "servers.edit",
            "servers.delete",
            "servers.global_access",
            "containers.manage",
            "images.manage",
            "users.manage",
            "roles.manage",
            "audit.read",
            "panel.update",
        ],
        _ => &[],
    }
}

pub async fn ensure_role_schema(pool: &Pool<Sqlite>) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS roles (
            name        TEXT PRIMARY KEY,
            description TEXT NOT NULL DEFAULT '',
            color       TEXT NOT NULL DEFAULT '#94a3b8',
            is_system   INTEGER NOT NULL DEFAULT 0,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create roles table")?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS role_permissions (
            role_name  TEXT NOT NULL,
            permission TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (role_name, permission),
            FOREIGN KEY (role_name) REFERENCES roles(name) ON DELETE CASCADE
        );
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create role_permissions table")?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS role_permission_policy (
            role_name  TEXT NOT NULL,
            permission TEXT NOT NULL,
            mode       TEXT NOT NULL DEFAULT 'none',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (role_name, permission),
            FOREIGN KEY (role_name) REFERENCES roles(name) ON DELETE CASCADE
        );
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create role_permission_policy table")?;

    // Backward-compatible schema patch for deployments created before role color support.
    let role_columns = sqlx::query("PRAGMA table_info(roles)")
        .fetch_all(pool)
        .await
        .context("Failed to inspect roles table schema")?;
    let has_color_column = role_columns.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == "color")
            .unwrap_or(false)
    });
    if !has_color_column {
        sqlx::query("ALTER TABLE roles ADD COLUMN color TEXT NOT NULL DEFAULT '#94a3b8'")
            .execute(pool)
            .await
            .context("Failed to add roles.color column")?;
    }

    // Keep core roles always available.
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO roles (name, description, color, is_system) VALUES ('root', 'Full system access', '#ef4444', 1)"
    )
    .execute(pool)
    .await;
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO roles (name, description, color, is_system) VALUES ('admin', 'Administrative access', '#a78bfa', 1)"
    )
    .execute(pool)
    .await;
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO roles (name, description, color, is_system) VALUES ('user', 'Basic user access', '#60a5fa', 1)"
    )
    .execute(pool)
    .await;

    // Keep built-in non-root role colors stable after upgrades.
    let _ = sqlx::query("UPDATE roles SET color = '#a78bfa' WHERE name = 'admin'")
        .execute(pool)
        .await;
    let _ = sqlx::query("UPDATE roles SET color = '#60a5fa' WHERE name = 'user'")
        .execute(pool)
        .await;

    // Auto-register any legacy/custom role values already present in users table.
    let existing_roles = sqlx::query("SELECT DISTINCT role FROM users")
        .fetch_all(pool)
        .await
        .context("Failed to read existing user roles")?;
    for row in existing_roles {
        let role_name: String = row.try_get("role").unwrap_or_default();
        if role_name.trim().is_empty() {
            continue;
        }
        let desc = match role_name.as_str() {
            "root" => "Full system access".to_string(),
            "admin" => "Administrative access".to_string(),
            "user" => "Basic user access".to_string(),
            _ => format!("Custom role: {}", role_name),
        };
        let is_system = if matches!(role_name.as_str(), "root" | "admin" | "user") { 1 } else { 0 };
        let _ = sqlx::query("INSERT OR IGNORE INTO roles (name, description, color, is_system) VALUES (?, ?, ?, ?)")
            .bind(&role_name)
            .bind(desc)
            .bind(default_role_color(&role_name))
            .bind(is_system)
            .execute(pool)
            .await;
    }

    // Seed default permission sets for built-in roles.
    for role_name in ["root", "admin"] {
        for key in default_role_permissions(role_name) {
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO role_permissions (role_name, permission) VALUES (?, ?)"
            )
            .bind(role_name)
            .bind(*key)
            .execute(pool)
            .await;

            let _ = sqlx::query(
                "INSERT OR IGNORE INTO role_permission_policy (role_name, permission, mode) VALUES (?, ?, 'write')"
            )
            .bind(role_name)
            .bind(*key)
            .execute(pool)
            .await;
        }
    }

    // Backfill policy rows from existing boolean permission rows.
    let existing_pairs = sqlx::query("SELECT role_name, permission FROM role_permissions")
        .fetch_all(pool)
        .await
        .context("Failed to read existing role permission rows")?;
    for row in existing_pairs {
        let role_name: String = row.try_get("role_name").unwrap_or_default();
        let permission: String = row.try_get("permission").unwrap_or_default();
        if role_name.is_empty() || permission.is_empty() {
            continue;
        }
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO role_permission_policy (role_name, permission, mode) VALUES (?, ?, 'write')"
        )
        .bind(role_name)
        .bind(permission)
        .execute(pool)
        .await;
    }

    Ok(())
}

pub async fn list_roles(pool: &Pool<Sqlite>) -> Result<Vec<Role>> {
    let roles = sqlx::query_as::<_, Role>(
        r#"
        SELECT name, description, color, is_system, created_at
        FROM roles
        ORDER BY
            CASE name
                WHEN 'root' THEN 0
                WHEN 'admin' THEN 1
                WHEN 'user' THEN 2
                ELSE 3
            END,
            name ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to list roles")?;
    Ok(roles)
}

pub async fn role_exists(pool: &Pool<Sqlite>, role_name: &str) -> Result<bool> {
    let cnt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM roles WHERE name = ?")
        .bind(role_name)
        .fetch_one(pool)
        .await
        .context("Failed to check role existence")?;
    Ok(cnt > 0)
}

pub async fn is_system_role(pool: &Pool<Sqlite>, role_name: &str) -> Result<bool> {
    let val: Option<i64> = sqlx::query_scalar("SELECT is_system FROM roles WHERE name = ?")
        .bind(role_name)
        .fetch_optional(pool)
        .await
        .context("Failed to check role system flag")?;
    Ok(val.unwrap_or(0) != 0)
}

pub async fn create_role(pool: &Pool<Sqlite>, role_name: &str, description: &str) -> Result<()> {
    sqlx::query("INSERT INTO roles (name, description, color, is_system) VALUES (?, ?, ?, 0)")
        .bind(role_name)
        .bind(description)
        .bind(default_role_color(role_name))
        .execute(pool)
        .await
        .context("Failed to create role")?;
    Ok(())
}

pub async fn update_role_color(pool: &Pool<Sqlite>, role_name: &str, color: &str) -> Result<()> {
    sqlx::query("UPDATE roles SET color = ? WHERE name = ?")
        .bind(color)
        .bind(role_name)
        .execute(pool)
        .await
        .context("Failed to update role color")?;
    Ok(())
}

pub async fn delete_role(pool: &Pool<Sqlite>, role_name: &str) -> Result<()> {
    let mut tx = pool.begin().await.context("Failed to start role delete transaction")?;
    sqlx::query("DELETE FROM role_permissions WHERE role_name = ?")
        .bind(role_name)
        .execute(&mut *tx)
        .await
        .context("Failed to delete role permissions")?;
    sqlx::query("DELETE FROM roles WHERE name = ?")
        .bind(role_name)
        .execute(&mut *tx)
        .await
        .context("Failed to delete role")?;
    tx.commit().await.context("Failed to commit role delete")?;
    Ok(())
}

pub async fn count_users_with_role(pool: &Pool<Sqlite>, role_name: &str) -> Result<i64> {
    let cnt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = ?")
        .bind(role_name)
        .fetch_one(pool)
        .await
        .context("Failed to count users for role")?;
    Ok(cnt)
}

pub async fn list_role_permissions(pool: &Pool<Sqlite>, role_name: &str) -> Result<Vec<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT permission FROM role_permissions WHERE role_name = ? ORDER BY permission ASC"
    )
    .bind(role_name)
    .fetch_all(pool)
    .await
    .context("Failed to list role permissions")?;
    Ok(rows)
}

pub async fn list_all_role_permissions(pool: &Pool<Sqlite>) -> Result<HashMap<String, Vec<String>>> {
    let rows = sqlx::query("SELECT role_name, permission FROM role_permissions ORDER BY role_name, permission")
        .fetch_all(pool)
        .await
        .context("Failed to list all role permissions")?;
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for row in rows {
        let role_name: String = row.try_get("role_name").unwrap_or_default();
        let permission: String = row.try_get("permission").unwrap_or_default();
        map.entry(role_name).or_default().push(permission);
    }
    Ok(map)
}

pub async fn list_role_permission_policy(
    pool: &Pool<Sqlite>,
    role_name: &str,
) -> Result<Vec<PermissionPolicyRow>> {
    let rows = sqlx::query_as::<_, PermissionPolicyRow>(
        r#"
        SELECT role_name, permission, mode
        FROM role_permission_policy
        WHERE role_name = ?
        ORDER BY permission ASC
        "#,
    )
    .bind(role_name)
    .fetch_all(pool)
    .await
    .context("Failed to load role permission policy")?;
    Ok(rows)
}

pub async fn list_all_role_permission_policy(
    pool: &Pool<Sqlite>,
) -> Result<HashMap<String, HashMap<String, String>>> {
    let rows = sqlx::query_as::<_, PermissionPolicyRow>(
        r#"
        SELECT role_name, permission, mode
        FROM role_permission_policy
        ORDER BY role_name, permission ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to list role permission policy")?;

    let mut map: HashMap<String, HashMap<String, String>> = HashMap::new();
    for row in rows {
        map.entry(row.role_name)
            .or_default()
            .insert(row.permission, row.mode);
    }
    Ok(map)
}

pub async fn replace_role_permission_policy(
    pool: &Pool<Sqlite>,
    role_name: &str,
    policy: &HashMap<String, String>,
) -> Result<()> {
    let mut tx = pool.begin().await.context("Failed to start role policy update transaction")?;
    sqlx::query("DELETE FROM role_permission_policy WHERE role_name = ?")
        .bind(role_name)
        .execute(&mut *tx)
        .await
        .context("Failed to clear role policy")?;
    sqlx::query("DELETE FROM role_permissions WHERE role_name = ?")
        .bind(role_name)
        .execute(&mut *tx)
        .await
        .context("Failed to clear boolean role permissions")?;

    for def in permission_catalog() {
        let mode = policy
            .get(def.key)
            .map(|m| m.as_str())
            .unwrap_or("none");
        let normalized = match mode {
            "read" => "read",
            "write" => "write",
            _ => "none",
        };

        sqlx::query(
            "INSERT INTO role_permission_policy (role_name, permission, mode) VALUES (?, ?, ?)"
        )
        .bind(role_name)
        .bind(def.key)
        .bind(normalized)
        .execute(&mut *tx)
        .await
        .context("Failed to insert role policy row")?;

        if normalized == "write" {
            sqlx::query("INSERT INTO role_permissions (role_name, permission) VALUES (?, ?)")
                .bind(role_name)
                .bind(def.key)
                .execute(&mut *tx)
                .await
                .context("Failed to insert write permission mirror")?;
        }
    }

    tx.commit().await.context("Failed to commit role policy update")?;
    Ok(())
}

pub async fn role_permission_mode(
    pool: &Pool<Sqlite>,
    role_name: &str,
    permission: &str,
) -> Result<String> {
    if role_name == "root" {
        return Ok("write".to_string());
    }
    let mode = sqlx::query_scalar::<_, String>(
        "SELECT mode FROM role_permission_policy WHERE role_name = ? AND permission = ?"
    )
    .bind(role_name)
    .bind(permission)
    .fetch_optional(pool)
    .await
    .context("Failed to read role permission mode")?;
    Ok(mode.unwrap_or_else(|| "none".to_string()))
}

pub async fn role_has_write_permission(
    pool: &Pool<Sqlite>,
    role_name: &str,
    permission: &str,
) -> Result<bool> {
    Ok(role_permission_mode(pool, role_name, permission).await? == "write")
}

pub async fn role_has_read_permission(
    pool: &Pool<Sqlite>,
    role_name: &str,
    permission: &str,
) -> Result<bool> {
    let mode = role_permission_mode(pool, role_name, permission).await?;
    Ok(mode == "read" || mode == "write")
}

pub async fn replace_role_permissions(
    pool: &Pool<Sqlite>,
    role_name: &str,
    permissions: &[String],
) -> Result<()> {
    let mut unique: Vec<String> = permissions
        .iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty() && is_valid_permission(p))
        .collect();
    unique.sort();
    unique.dedup();

    let mut tx = pool.begin().await.context("Failed to start permission update transaction")?;
    sqlx::query("DELETE FROM role_permissions WHERE role_name = ?")
        .bind(role_name)
        .execute(&mut *tx)
        .await
        .context("Failed to clear previous role permissions")?;

    for permission in unique {
        sqlx::query("INSERT INTO role_permissions (role_name, permission) VALUES (?, ?)")
            .bind(role_name)
            .bind(permission)
            .execute(&mut *tx)
            .await
            .context("Failed to insert role permission")?;
    }

    tx.commit().await.context("Failed to commit role permission update")?;
    Ok(())
}

pub async fn role_has_permission(pool: &Pool<Sqlite>, role_name: &str, permission: &str) -> Result<bool> {
    role_has_write_permission(pool, role_name, permission).await
}
