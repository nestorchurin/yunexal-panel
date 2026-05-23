use anyhow::{Context, Result};
use sqlx::{Pool, Sqlite};
use crate::crypto;

pub async fn get_image_env(pool: &Pool<Sqlite>, image_id: &str, db_key: &[u8; 32]) -> Result<String> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT env FROM image_env_overrides WHERE image_id = ?",
    )
    .bind(image_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch image env overrides")?;
    match row {
        None => Ok(String::new()),
        Some((v,)) => crypto::decrypt_if_encrypted(&v, db_key).context("image_env decrypt"),
    }
}

pub async fn set_image_env(pool: &Pool<Sqlite>, image_id: &str, env: &str, db_key: &[u8; 32]) -> Result<()> {
    let encrypted = crypto::encrypt(env, db_key);
    sqlx::query(
        "INSERT INTO image_env_overrides (image_id, env) VALUES (?, ?)
         ON CONFLICT(image_id) DO UPDATE SET env = excluded.env",
    )
    .bind(image_id)
    .bind(&encrypted)
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
