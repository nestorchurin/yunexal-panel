use anyhow::{Context, Result};
use sqlx::{Pool, Sqlite};
use crate::crypto;

pub async fn get_panel_setting(pool: &Pool<Sqlite>, key: &str) -> String {
    sqlx::query_as::<_, (String,)>("SELECT value FROM panel_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|(v,)| v)
        .unwrap_or_default()
}

pub async fn set_panel_setting(pool: &Pool<Sqlite>, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO panel_settings (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .context("set_panel_setting")?;
    Ok(())
}

pub async fn get_panel_setting_bool(pool: &Pool<Sqlite>, key: &str) -> bool {
    get_panel_setting(pool, key).await == "1"
}

/// Reads the service API key, decrypting it if stored with enc:v1: prefix.
pub async fn get_service_api_key(pool: &Pool<Sqlite>, db_key: &[u8; 32]) -> String {
    let raw = get_panel_setting(pool, "service_api_key").await;
    if raw.is_empty() {
        return raw;
    }
    crypto::decrypt_if_encrypted(&raw, db_key).unwrap_or_default()
}

/// Encrypts and stores the service API key.
pub async fn set_service_api_key(pool: &Pool<Sqlite>, value: &str, db_key: &[u8; 32]) -> Result<()> {
    let encrypted = crypto::encrypt(value, db_key);
    set_panel_setting(pool, "service_api_key", &encrypted).await
}

