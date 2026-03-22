mod audit;
mod users;
mod servers;
mod ports;
mod dns;
mod images;

pub use audit::*;
pub use users::*;
pub use servers::*;
pub use ports::*;
pub use dns::*;
pub use images::*;

use anyhow::{Context, Result};
use sqlx::{sqlite::SqliteConnectOptions, FromRow, Pool, Sqlite, SqlitePool};
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

    // Migration: add owner_id and status columns to servers table (no-op if already present)
    let _ = sqlx::query("ALTER TABLE servers ADD COLUMN owner_id INTEGER DEFAULT 0")
        .execute(&pool)
        .await;
    let _ = sqlx::query("ALTER TABLE servers ADD COLUMN status TEXT DEFAULT 'stopped'")
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

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS dns_providers (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            name          TEXT NOT NULL,
            provider_type TEXT NOT NULL,
            credentials   TEXT NOT NULL DEFAULT '{}',
            enabled       INTEGER NOT NULL DEFAULT 1,
            created_at    TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create dns_providers table")?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS dns_records (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            provider_id   INTEGER NOT NULL REFERENCES dns_providers(id) ON DELETE CASCADE,
            zone_id       TEXT NOT NULL DEFAULT '',
            zone_name     TEXT NOT NULL DEFAULT '',
            record_type   TEXT NOT NULL DEFAULT 'A',
            name          TEXT NOT NULL DEFAULT '',
            value         TEXT NOT NULL DEFAULT '',
            ttl           INTEGER NOT NULL DEFAULT 300,
            priority      INTEGER NOT NULL DEFAULT 0,
            proxied       INTEGER NOT NULL DEFAULT 0,
            remote_id     TEXT NOT NULL DEFAULT '',
            container_id  INTEGER DEFAULT NULL,
            ddns_enabled  INTEGER NOT NULL DEFAULT 0,
            ddns_interval INTEGER NOT NULL DEFAULT 300,
            last_ip       TEXT NOT NULL DEFAULT '',
            last_synced   TEXT NOT NULL DEFAULT '',
            created_at    TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create dns_records table")?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS audit_log (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            actor      TEXT NOT NULL,
            action     TEXT NOT NULL,
            target     TEXT NOT NULL DEFAULT '',
            detail     TEXT NOT NULL DEFAULT '',
            ip         TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create audit_log table")?;

    // Migration: add ip column to audit_log (no-op if already present)
    let _ = sqlx::query("ALTER TABLE audit_log ADD COLUMN ip TEXT NOT NULL DEFAULT ''")
        .execute(&pool)
        .await;

    info!("Database initialized successfully");
    Ok(pool)
}

// ── Role helpers ─────────────────────────────────────────────────────────────

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
