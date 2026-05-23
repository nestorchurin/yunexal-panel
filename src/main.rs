use anyhow::{Context, Result};
use tracing::{info, warn};
use axum_extra::extract::cookie::Key;
use yunexal_panel::{crypto, db, docker, handlers, password};
use yunexal_panel::state::AppState;

async fn auto_migrate_legacy_labels(pool: &sqlx::Pool<sqlx::Sqlite>, docker_client: &bollard::Docker) {
    let servers = match db::list_servers_with_container_ids(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            warn!("Skipping startup legacy migration: failed to list servers: {}", e);
            return;
        }
    };

    let mut migrated = 0u64;
    let mut failed = 0u64;

    for (db_id, container_id, name) in servers {
        match docker::ensure_management_labels(docker_client, &container_id).await {
            Ok(true) => {
                migrated += 1;
                let _ = db::audit_log(
                    pool,
                    "system",
                    "container.label_autofix",
                    &name,
                    &format!("db_id={} container_id={}", db_id, container_id),
                    "127.0.0.1",
                    "startup",
                )
                .await;
            }
            Ok(false) => {}
            Err(e) => {
                failed += 1;
                warn!(
                    "Legacy label migration failed for db_id={} container={}: {}",
                    db_id,
                    container_id,
                    e
                );
            }
        }
    }

    if migrated > 0 || failed > 0 {
        info!(
            "Startup legacy migration finished: migrated={} failed={}",
            migrated,
            failed
        );
    }
}

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
    let db_key = crypto::derive_db_key(&cookie_secret);

    // 4. Initialize Database
    let pool = db::init_db().await.context("Database initialization failed")?;

    // Verify the DB encryption key matches the canary — fails hard if COOKIE_SECRET was rotated
    // without re-encrypting the DB first.
    db::verify_or_init_canary(&pool, &db_key)
        .await
        .context("DB encryption key check failed — run: yunexal-setup --rekey --old-secret <prev>")?;

    // 5. Initialize Docker Client
    let docker_client = docker::get_docker_client().await.context("Docker client init failed")?;

    let version = docker_client.version().await.context("Failed to ping Docker daemon")?;
    info!("Connected to Docker: {:?}", version.version.unwrap_or_default());

    // Auto-migrate legacy container labels before the router starts using
    // managed-only filters.
    auto_migrate_legacy_labels(&pool, &docker_client).await;

    // 6. Create App State
    let state = AppState::new(pool, docker_client, cookie_key, db_key, listen_addr.clone());

    // 7. Setup Router
    let app = handlers::create_router(state);

    // 8. Run Server
    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    info!("Server running on http://{}", listen_addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
