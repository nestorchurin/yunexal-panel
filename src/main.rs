mod state;
mod db;
mod docker;
mod handlers;
mod compose;
mod auth;
mod password;

use anyhow::{Context, Result};
use tracing::info;
use axum_extra::extract::cookie::Key;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Load .env
    dotenvy::dotenv().ok();

    // 2. Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("yunexal_panel=debug,tower_http=debug,axum=debug")
        .init();

    info!("Starting Yunexal Panel...");

    // 3. Read config from env
    let auth_username = std::env::var("PANEL_USERNAME")
        .context("PANEL_USERNAME not set in .env")?;
    let auth_password = std::env::var("PANEL_PASSWORD")
        .context("PANEL_PASSWORD not set in .env")?;
    let cookie_secret = std::env::var("COOKIE_SECRET")
        .context("COOKIE_SECRET not set in .env")?;
    // Key::from requires ≥64 bytes; our hex-encoded 64-byte secret is 128 ASCII chars → 128 bytes.
    let cookie_key = Key::from(cookie_secret.as_bytes());

    // 4. Initialize Database
    let pool = db::init_db().await.context("Database initialization failed")?;

    // 5. Seed / update the root user from env each startup
    let hashed = password::hash(&auth_password)
        .context("Failed to hash admin password")?;
    db::seed_root_user(&pool, &auth_username, &hashed).await?;

    // 6. Initialize Docker Client
    let docker_client = docker::get_docker_client().await.context("Docker client init failed")?;
    
    let version = docker_client.version().await.context("Failed to ping Docker daemon")?;
    info!("Connected to Docker: {:?}", version.version.unwrap_or_default());

    // 7. Create App State
    let state = AppState::new(pool, docker_client, cookie_key, auth_username);

    // 8. Setup Router
    let app = handlers::create_router(state);

    // 9. Run Server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("Server running on http://0.0.0.0:3000");
    
    axum::serve(listener, app).await?;

    Ok(())
}
