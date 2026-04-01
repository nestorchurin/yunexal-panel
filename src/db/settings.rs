use anyhow::{Context, Result};
use sqlx::{Pool, Sqlite};

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
