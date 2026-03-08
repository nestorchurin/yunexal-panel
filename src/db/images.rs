use anyhow::{Context, Result};
use sqlx::{Pool, Sqlite};

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
