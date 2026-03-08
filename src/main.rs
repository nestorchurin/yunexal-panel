use anyhow::{Context, Result};
use tracing::info;
use axum_extra::extract::cookie::Key;
use yunexal_panel::{db, docker, handlers, password};
use yunexal_panel::state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Load .env
    dotenvy::dotenv().ok();

    // 2. Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("yunexal_panel=debug,tower_http=debug,axum=debug")
        .init();

    // Check for --seed flag: seed DB then exit (used by setup.sh)
    let seed_only = std::env::args().any(|a| a == "--seed");

    if seed_only {
        // Credentials are only needed here, during initial setup.
        let auth_username = std::env::var("PANEL_USERNAME")
            .context("PANEL_USERNAME not set")?;
        let auth_password = std::env::var("PANEL_PASSWORD")
            .context("PANEL_PASSWORD not set")?;
        let auth_role = std::env::var("PANEL_ROLE")
            .unwrap_or_else(|_| "admin".to_string());

        let pool = db::init_db().await.context("Database initialization failed")?;
        let hashed = password::hash(&auth_password)
            .context("Failed to hash password")?;
        db::seed_root_user(&pool, &auth_username, &hashed, &auth_role).await?;
        println!("✓ Database seeded: user '{}' with role '{}'.", auth_username, auth_role);
        return Ok(());
    }

    info!("Starting Yunexal Panel...");

    // 3. Read config from env
    let panel_port: u16 = std::env::var("PANEL_PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .context("PANEL_PORT must be a valid port number")?;
    let listen_addr = format!("0.0.0.0:{}", panel_port);

    let cookie_secret = std::env::var("COOKIE_SECRET")
        .context("COOKIE_SECRET not set in .env")?;
    // Key::from requires ≥64 bytes; our hex-encoded 64-byte secret is 128 ASCII chars → 128 bytes.
    let cookie_key = Key::from(cookie_secret.as_bytes());

    // 4. Initialize Database
    let pool = db::init_db().await.context("Database initialization failed")?;

    // 5. Initialize Docker Client
    let docker_client = docker::get_docker_client().await.context("Docker client init failed")?;

    let version = docker_client.version().await.context("Failed to ping Docker daemon")?;
    info!("Connected to Docker: {:?}", version.version.unwrap_or_default());

    // 6. Create App State
    let state = AppState::new(pool, docker_client, cookie_key, listen_addr.clone());

    // 7. Setup Router
    let app = handlers::create_router(state);

    // 8. Run Server
    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    info!("Server running on http://{}", listen_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
