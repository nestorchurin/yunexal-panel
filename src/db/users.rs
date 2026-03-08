use anyhow::{Context, Result};
use sqlx::{Pool, Sqlite};
use super::User;

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
