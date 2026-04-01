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
    let cf_analytics_token = std::env::var("CF_ANALYTICS_TOKEN").unwrap_or_default();
    let state = AppState::new(pool, docker_client, cookie_key, listen_addr.clone(), cf_analytics_token);

    // Background task: L7 flood detection + auto-disable CF UAM when storm calms
    {
        let monitor = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
            let mut ticks: u32 = 0;
            loop {
                interval.tick().await;
                ticks = ticks.wrapping_add(1);

                // Every tick (10 s): check for L7 HTTP flood and trigger UAM if needed.
                yunexal_panel::handlers::auth::check_l7_and_maybe_trigger_uam(monitor.clone()).await;

                // Every 6 ticks (~60 s): check whether the storm has calmed and auto-disable.
                if ticks % 6 != 0 { continue; }

                let triggered_at = *monitor.cf_uam_triggered_at.lock().await;
                if let Some(triggered) = triggered_at {
                    if !yunexal_panel::db::get_panel_setting_bool(&monitor.db, "cf_uam_enabled").await {
                        continue;
                    }
                    let cooldown_mins: u64 = yunexal_panel::db::get_panel_setting(&monitor.db, "cf_uam_cooldown_mins")
                        .await.parse().unwrap_or(10);
                    let elapsed = std::time::Instant::now().duration_since(triggered).as_secs();
                    if elapsed < cooldown_mins * 60 { continue; }

                    // Verify no recent brute-force login attacks (within last 2 minutes).
                    let now = std::time::Instant::now();
                    let still_brute = monitor.login_attempts.iter().any(|e| {
                        let (count, last) = e.value();
                        *count >= 1 && now.duration_since(*last).as_secs() <= 120
                    });
                    if still_brute { continue; }

                    // Verify L7 flood has also subsided.
                    let l7_threshold: u32 = yunexal_panel::db::get_panel_setting(&monitor.db, "cf_l7_threshold")
                        .await.parse().unwrap_or(200);
                    let l7_ips_min: usize = yunexal_panel::db::get_panel_setting(&monitor.db, "cf_l7_ips_min")
                        .await.parse().unwrap_or(2);
                    let still_l7 = monitor.l7_attacking_ips(l7_threshold) >= l7_ips_min;
                    if still_l7 { continue; }

                    let token = yunexal_panel::db::get_panel_setting(&monitor.db, "cf_api_token").await;
                    let zone_id = yunexal_panel::db::get_panel_setting(&monitor.db, "cf_zone_id").await;
                    if token.is_empty() || zone_id.is_empty() { continue; }

                    match yunexal_panel::cloudflare::disable_under_attack(&zone_id, &token).await {
                        Ok(()) => {
                            *monitor.cf_uam_triggered_at.lock().await = None;
                            tracing::info!("CF Under Attack Mode auto-disabled — storm calmed");
                            let _ = yunexal_panel::db::audit_log(
                                &monitor.db, "system", "panel.cf_uam_disable",
                                "auto", "storm calmed", "127.0.0.1", "system",
                            ).await;
                        }
                        Err(e) => tracing::error!("Failed to auto-disable CF UAM: {}", e),
                    }
                }
            }
        });
    }

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
