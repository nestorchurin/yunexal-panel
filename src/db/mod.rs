mod audit;
mod users;
mod sessions;
mod roles;
mod servers;
mod ports;
mod images;
mod settings;
pub mod schedules;

pub use audit::*;
pub use users::*;
pub use sessions::*;
pub use roles::*;
pub use servers::*;
pub use ports::*;
pub use images::*;
pub use settings::*;

use anyhow::{Context, Result};
use sqlx::{sqlite::SqliteConnectOptions, FromRow, Pool, Sqlite, SqlitePool};
use std::str::FromStr;
use tracing::info;

// ── User model ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: i64,
    pub uid: String,
    pub nickname: String,
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
            uid         TEXT    NOT NULL UNIQUE CHECK(length(trim(uid)) BETWEEN 9 AND 16),
            nickname    TEXT    NOT NULL CHECK(length(nickname) BETWEEN 1 AND 24),
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
        CREATE TABLE IF NOT EXISTS user_sessions (
            session_id   TEXT PRIMARY KEY,
            user_id      INTEGER NOT NULL,
            username     TEXT NOT NULL,
            ip           TEXT NOT NULL DEFAULT '',
            user_agent   TEXT NOT NULL DEFAULT '',
            created_at   TEXT NOT NULL DEFAULT (datetime('now')),
            last_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
            revoked      INTEGER NOT NULL DEFAULT 0
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create user_sessions table")?;

    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_user_sessions_user_active ON user_sessions (user_id, revoked, last_seen_at)",
    )
    .execute(&pool)
    .await;

    // Migration: add uid/nickname columns for legacy databases.
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN uid TEXT NOT NULL DEFAULT ''")
        .execute(&pool)
        .await;
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN nickname TEXT NOT NULL DEFAULT ''")
        .execute(&pool)
        .await;

    // Migration: keep user_sessions schema aligned for older databases.
    let _ = sqlx::query("ALTER TABLE user_sessions ADD COLUMN last_seen_at TEXT NOT NULL DEFAULT ''")
        .execute(&pool)
        .await;
    let _ = sqlx::query("ALTER TABLE user_sessions ADD COLUMN revoked INTEGER NOT NULL DEFAULT 0")
        .execute(&pool)
        .await;
    let _ = sqlx::query(
        "UPDATE user_sessions SET last_seen_at = COALESCE(NULLIF(last_seen_at, ''), created_at, datetime('now'))",
    )
    .execute(&pool)
    .await;

    // Backfill legacy rows.
    let _ = sqlx::query(
        "UPDATE users SET nickname = substr(trim(username), 1, 24) WHERE nickname IS NULL OR trim(nickname) = ''",
    )
    .execute(&pool)
    .await;
    let _ = sqlx::query(
        "UPDATE users SET uid = '#u' || printf('%014d', id) WHERE uid IS NULL OR trim(uid) = '' OR length(trim(uid)) < 9 OR length(trim(uid)) > 16",
    )
    .execute(&pool)
    .await;

    // Unique uid index for migrated tables where the original table definition lacked it.
    let _ = sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_users_uid ON users(uid)")
        .execute(&pool)
        .await;

    // Validation triggers keep legacy schemas aligned with new constraints.
    let _ = sqlx::query(
        r#"
        CREATE TRIGGER IF NOT EXISTS trg_users_validate_insert
        BEFORE INSERT ON users
        FOR EACH ROW
        BEGIN
            SELECT CASE
                WHEN NEW.uid IS NULL OR trim(NEW.uid) = '' THEN RAISE(ABORT, 'uid is required')
                WHEN length(trim(NEW.uid)) < 9 OR length(trim(NEW.uid)) > 16 THEN RAISE(ABORT, 'uid must be 9-16 characters')
                WHEN NEW.nickname IS NULL OR trim(NEW.nickname) = '' THEN RAISE(ABORT, 'nickname is required')
                WHEN length(NEW.nickname) > 24 THEN RAISE(ABORT, 'nickname too long')
            END;
        END;
        "#,
    )
    .execute(&pool)
    .await;
    let _ = sqlx::query(
        r#"
        CREATE TRIGGER IF NOT EXISTS trg_users_validate_update
        BEFORE UPDATE ON users
        FOR EACH ROW
        BEGIN
            SELECT CASE
                WHEN NEW.uid IS NULL OR trim(NEW.uid) = '' THEN RAISE(ABORT, 'uid is required')
                WHEN length(trim(NEW.uid)) < 9 OR length(trim(NEW.uid)) > 16 THEN RAISE(ABORT, 'uid must be 9-16 characters')
                WHEN NEW.nickname IS NULL OR trim(NEW.nickname) = '' THEN RAISE(ABORT, 'nickname is required')
                WHEN length(NEW.nickname) > 24 THEN RAISE(ABORT, 'nickname too long')
            END;
        END;
        "#,
    )
    .execute(&pool)
    .await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS server_user_permissions (
            server_id   INTEGER NOT NULL,
            user_id     INTEGER NOT NULL,
            permission  TEXT    NOT NULL,
            mode        TEXT    NOT NULL DEFAULT 'none' CHECK(mode IN ('none','read','write')),
            PRIMARY KEY(server_id, user_id, permission)
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create server_user_permissions table")?;

    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_server_user_permissions_user_server ON server_user_permissions (user_id, server_id)",
    )
    .execute(&pool)
    .await;

    roles::ensure_role_schema(&pool)
        .await
        .context("Failed to initialize roles schema")?;

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

    // ── v0.3.2 migrations ─────────────────────────────────────────────────────
    // No new schema changes in v0.3.2.

    // ── v0.4.0 migrations ─────────────────────────────────────────────────────
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS panel_settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL DEFAULT ''
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create panel_settings table")?;

    // Seed default settings (no-op if already present)
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO panel_settings (key, value) VALUES ('ufw_enabled', '0')"
    ).execute(&pool).await;
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO panel_settings (key, value) VALUES ('bandwidth_enabled', '1')"
    ).execute(&pool).await;
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO panel_settings (key, value) VALUES ('docker_default_quota', '15')"
    ).execute(&pool).await;
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO panel_settings (key, value) VALUES ('container_storage_path', '')"
    ).execute(&pool).await;
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO panel_settings (key, value) VALUES ('panel_favicon', '')"
    ).execute(&pool).await;
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO panel_settings (key, value) VALUES ('panel_accent', '#7c3aed')"
    ).execute(&pool).await;
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO panel_settings (key, value) VALUES ('panel_name', 'Yunexal Panel')"
    ).execute(&pool).await;

    // ufw_blocked column for server_ports (no-op if already present)
    let _ = sqlx::query(
        "ALTER TABLE server_ports ADD COLUMN ufw_blocked INTEGER NOT NULL DEFAULT 0"
    ).execute(&pool).await;

    // user_agent column for audit_log (no-op if already present)
    let _ = sqlx::query(
        "ALTER TABLE audit_log ADD COLUMN user_agent TEXT NOT NULL DEFAULT ''"
    ).execute(&pool).await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS key_meta (
            k TEXT PRIMARY KEY,
            v TEXT NOT NULL
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create key_meta table")?;

    info!("Database initialized successfully");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS server_env_permissions (
            server_id     INTEGER NOT NULL,
            key           TEXT    NOT NULL,
            user_editable INTEGER NOT NULL DEFAULT 0,
            user_visible  INTEGER NOT NULL DEFAULT 1,
            PRIMARY KEY (server_id, key)
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create server_env_permissions table")?;

    // ── v0.5.2 migrations ─────────────────────────────────────────────────────
    let _ = sqlx::query(
        "ALTER TABLE servers ADD COLUMN max_schedule_runs INTEGER NOT NULL DEFAULT 20"
    ).execute(&pool).await;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS schedules (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            server_id       INTEGER NOT NULL,
            name            TEXT    NOT NULL,
            cron_expression TEXT    NOT NULL,
            task_type       TEXT    NOT NULL,
            task_payload    TEXT    NOT NULL DEFAULT '{}',
            enabled         INTEGER NOT NULL DEFAULT 1,
            last_run_at     TEXT,
            next_run_at     TEXT,
            created_at      TEXT    NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (server_id) REFERENCES servers(id) ON DELETE CASCADE
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create schedules table")?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS schedule_runs (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            schedule_id INTEGER NOT NULL,
            started_at  TEXT    NOT NULL DEFAULT (datetime('now')),
            status      TEXT    NOT NULL DEFAULT 'running',
            output      TEXT,
            FOREIGN KEY (schedule_id) REFERENCES schedules(id) ON DELETE CASCADE
        );
        "#,
    )
    .execute(&pool)
    .await
    .context("Failed to create schedule_runs table")?;

    Ok(pool)
}

/// Verifies the DB encryption key against the stored canary, or writes the canary on first run.
/// Returns an error if the key does not match — indicating COOKIE_SECRET was rotated without
/// re-encrypting the database.
pub async fn verify_or_init_canary(pool: &Pool<Sqlite>, key: &[u8; 32]) -> Result<()> {
    use crate::crypto;

    let existing: Option<(String,)> =
        sqlx::query_as("SELECT v FROM key_meta WHERE k = 'canary'")
            .fetch_optional(pool)
            .await
            .context("Failed to read key_meta canary")?;

    match existing {
        None => {
            let canary = crypto::encrypt("yunexal-canary-v1", key);
            sqlx::query("INSERT INTO key_meta (k, v) VALUES ('canary', ?)")
                .bind(&canary)
                .execute(pool)
                .await
                .context("Failed to write key_meta canary")?;
            info!("DB encryption canary initialized.");
            Ok(())
        }
        Some((stored,)) => crypto::decrypt(&stored, key)
            .map(|_| ())
            .map_err(|_| {
                anyhow::anyhow!(
                    "COOKIE_SECRET mismatch: cannot decrypt DB canary. \
                     Run: yunexal-setup --rekey --old-secret <previous_secret>"
                )
            }),
    }
}

/// Encrypts legacy plaintext values in DB columns that now store encrypted data.
/// Safe to run on every startup; already-encrypted rows are skipped.
pub async fn migrate_legacy_encrypted_values(pool: &Pool<Sqlite>, key: &[u8; 32]) -> Result<u64> {
    use crate::crypto;

    let mut migrated = 0u64;

    let session_rows = sqlx::query_as::<_, (String, String)>(
        "SELECT session_id, ip FROM user_sessions WHERE ip IS NOT NULL AND trim(ip) <> ''",
    )
    .fetch_all(pool)
    .await
    .context("migrate encrypted values: list user_sessions")?;

    for (session_id, ip) in session_rows {
        if crypto::is_encrypted(&ip) {
            continue;
        }
        let encrypted = crypto::encrypt(&ip, key);
        sqlx::query("UPDATE user_sessions SET ip = ? WHERE session_id = ?")
            .bind(&encrypted)
            .bind(&session_id)
            .execute(pool)
            .await
            .context("migrate encrypted values: update user_sessions.ip")?;
        migrated += 1;
    }

    let image_rows = sqlx::query_as::<_, (String, String)>(
        "SELECT image_id, env FROM image_env_overrides WHERE env IS NOT NULL AND trim(env) <> ''",
    )
    .fetch_all(pool)
    .await
    .context("migrate encrypted values: list image_env_overrides")?;

    for (image_id, env) in image_rows {
        if crypto::is_encrypted(&env) {
            continue;
        }
        let encrypted = crypto::encrypt(&env, key);
        sqlx::query("UPDATE image_env_overrides SET env = ? WHERE image_id = ?")
            .bind(&encrypted)
            .bind(&image_id)
            .execute(pool)
            .await
            .context("migrate encrypted values: update image_env_overrides.env")?;
        migrated += 1;
    }

    let service_key_row = sqlx::query_as::<_, (String,)>(
        "SELECT value FROM panel_settings WHERE key = 'service_api_key'",
    )
    .fetch_optional(pool)
    .await
    .context("migrate encrypted values: read service_api_key")?;

    if let Some((value,)) = service_key_row {
        let trimmed = value.trim();
        if !trimmed.is_empty() && !crypto::is_encrypted(trimmed) {
            let encrypted = crypto::encrypt(trimmed, key);
            sqlx::query("UPDATE panel_settings SET value = ? WHERE key = 'service_api_key'")
                .bind(&encrypted)
                .execute(pool)
                .await
                .context("migrate encrypted values: update service_api_key")?;
            migrated += 1;
        }
    }

    Ok(migrated)
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
    let username_trimmed = username.trim();
    let default_uid = {
        let candidate = format!("#{}", username_trimmed);
        let candidate_len = candidate.chars().count();
        if (9..=16).contains(&candidate_len) {
            candidate
        } else if candidate_len < 9 {
            format!("{candidate}{:0<width$}", "", width = 9 - candidate_len)
        } else {
            candidate.chars().take(16).collect()
        }
    };
    let default_nickname = {
        let nick: String = username_trimmed.chars().take(24).collect();
        if nick.is_empty() { "root".to_string() } else { nick }
    };

    // Try inserting; on conflict update the hash + role.
    sqlx::query(
        r#"INSERT INTO users (uid, nickname, username, password_hash, role)
           VALUES (?, ?, ?, ?, ?)
           ON CONFLICT(username) DO UPDATE SET
               password_hash = excluded.password_hash,
               role = excluded.role,
               uid = CASE
                    WHEN users.uid IS NULL OR trim(users.uid) = '' THEN excluded.uid
                    ELSE users.uid
               END,
               nickname = CASE
                    WHEN users.nickname IS NULL OR trim(users.nickname) = '' THEN excluded.nickname
                    ELSE users.nickname
               END"#,
    )
    .bind(default_uid)
    .bind(default_nickname)
    .bind(username_trimmed)
    .bind(password_hash)
    .bind(role)
    .execute(pool)
    .await
    .context("Failed to upsert root user")?;
    info!("Root user '{}' ensured with role '{}'.", username, role);
    Ok(())
}
