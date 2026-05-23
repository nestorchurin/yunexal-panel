use anyhow::{Context, Result};
use sqlx::{FromRow, Pool, Sqlite};
use crate::crypto;

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct UserSessionInfo {
    pub session_id: String,
    pub user_id: i64,
    pub username: String,
    pub ip: String,
    pub user_agent: String,
    pub created_at: String,
    pub last_seen_at: String,
    pub revoked: i64,
}

pub async fn create_user_session(
    pool: &Pool<Sqlite>,
    session_id: &str,
    user_id: i64,
    username: &str,
    ip: &str,
    user_agent: &str,
    db_key: &[u8; 32],
) -> Result<()> {
    let encrypted_ip = crypto::encrypt(ip, db_key);
    sqlx::query(
        "INSERT INTO user_sessions (session_id, user_id, username, ip, user_agent, created_at, last_seen_at, revoked) VALUES (?, ?, ?, ?, ?, datetime('now'), datetime('now'), 0)",
    )
    .bind(session_id)
    .bind(user_id)
    .bind(username)
    .bind(&encrypted_ip)
    .bind(user_agent)
    .execute(pool)
    .await
    .context("Failed to create user session")?;
    Ok(())
}

pub async fn touch_user_session(pool: &Pool<Sqlite>, session_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE user_sessions SET last_seen_at = datetime('now') WHERE session_id = ? AND revoked = 0",
    )
    .bind(session_id)
    .execute(pool)
    .await
    .context("Failed to touch user session")?;
    Ok(())
}

pub async fn is_user_session_active(
    pool: &Pool<Sqlite>,
    user_id: i64,
    session_id: &str,
) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_sessions WHERE user_id = ? AND session_id = ? AND revoked = 0",
    )
    .bind(user_id)
    .bind(session_id)
    .fetch_one(pool)
    .await
    .context("Failed to check user session")?;
    Ok(count > 0)
}

pub async fn revoke_session_by_id(pool: &Pool<Sqlite>, session_id: &str) -> Result<u64> {
    let rows = sqlx::query(
        "UPDATE user_sessions SET revoked = 1, last_seen_at = datetime('now') WHERE session_id = ? AND revoked = 0",
    )
    .bind(session_id)
    .execute(pool)
    .await
    .context("Failed to revoke session by id")?
    .rows_affected();
    Ok(rows)
}

pub async fn revoke_user_session(
    pool: &Pool<Sqlite>,
    user_id: i64,
    session_id: &str,
) -> Result<u64> {
    let rows = sqlx::query(
        "UPDATE user_sessions SET revoked = 1, last_seen_at = datetime('now') WHERE user_id = ? AND session_id = ? AND revoked = 0",
    )
    .bind(user_id)
    .bind(session_id)
    .execute(pool)
    .await
    .context("Failed to revoke user session")?
    .rows_affected();
    Ok(rows)
}

pub async fn revoke_all_user_sessions(pool: &Pool<Sqlite>, user_id: i64) -> Result<u64> {
    let rows = sqlx::query(
        "UPDATE user_sessions SET revoked = 1, last_seen_at = datetime('now') WHERE user_id = ? AND revoked = 0",
    )
    .bind(user_id)
    .execute(pool)
    .await
    .context("Failed to revoke all user sessions")?
    .rows_affected();
    Ok(rows)
}

pub async fn list_user_sessions(
    pool: &Pool<Sqlite>,
    user_id: i64,
    db_key: &[u8; 32],
) -> Result<Vec<UserSessionInfo>> {
    let mut rows = sqlx::query_as::<_, UserSessionInfo>(
        "SELECT session_id, user_id, username, ip, user_agent, created_at, last_seen_at, revoked FROM user_sessions WHERE user_id = ? AND revoked = 0 ORDER BY datetime(last_seen_at) DESC, datetime(created_at) DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .context("Failed to list user sessions")?;
    for s in &mut rows {
        s.ip = crypto::decrypt_if_encrypted(&s.ip, db_key).unwrap_or_else(|_| "?".to_string());
    }
    Ok(rows)
}
