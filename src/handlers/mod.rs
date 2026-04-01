pub mod admin;
pub mod auth;
pub mod create;
pub mod dashboard;
pub mod dns;
pub mod files;
pub mod network;
pub mod servers;
pub mod templates;
pub mod ws;

use axum::{
    http::{header, HeaderValue, StatusCode},
    middleware,
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use axum_embed::ServeEmbed;
use rust_embed::Embed;
use crate::auth as auth_middleware;
use crate::state::AppState;
use std::net::SocketAddr;
use axum::extract::ConnectInfo;

#[derive(Embed, Clone)]
#[folder = "static/"]
struct StaticAssets;

/// Serve a single embedded static file for root-level assets (manifest.json, sw.js).
async fn serve_embedded(path: &str, content_type: &'static str) -> impl IntoResponse {
    match StaticAssets::get(path) {
        Some(f) => ([(header::CONTENT_TYPE, content_type)], f.data.into_owned()).into_response(),
        None    => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn serve_manifest() -> impl IntoResponse { serve_embedded("manifest.json", "application/json").await }
async fn serve_sw()       -> impl IntoResponse { serve_embedded("sw.js", "application/javascript").await }

// ── L7 request-rate tracking middleware ─────────────────────────────────────

async fn track_requests(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl IntoResponse {
    let ip = req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            req.headers()
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
        })
        .or_else(|| {
            req.extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ci| ci.0.ip().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    state.record_request(&ip);
    next.run(req).await
}

// ── Security headers middleware ───────────────────────────────────────────────

async fn security_headers(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl IntoResponse {
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();

    h.insert("X-Content-Type-Options",         HeaderValue::from_static("nosniff"));
    h.insert("X-Frame-Options",                HeaderValue::from_static("DENY"));
    h.insert(
        "Content-Security-Policy",
        HeaderValue::from_static(
            "default-src 'self'; \
             script-src 'self' 'unsafe-inline' https://unpkg.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net; \
             style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://cdn.jsdelivr.net; \
             font-src 'self' https://fonts.gstatic.com https://cdn.jsdelivr.net; \
             img-src 'self' data:; \
             connect-src 'self' ws: wss:; \
             frame-ancestors 'none'; \
             base-uri 'self'; \
             form-action 'self'"
        ),
    );
    h.insert("Referrer-Policy",                HeaderValue::from_static("strict-origin-when-cross-origin"));
    h.insert("Permissions-Policy",             HeaderValue::from_static("camera=(), microphone=(), geolocation=()"));
    h.insert("X-XSS-Protection",              HeaderValue::from_static("0"));
    h.insert("Cross-Origin-Opener-Policy",    HeaderValue::from_static("same-origin"));
    h.insert(
        "Cache-Control",
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );

    resp
}

// ── Custom fallback for unmatched routes / framework rejections ──────────────

async fn fallback() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Html("<h1>404 — Not Found</h1>"))
}

use admin::{
    admin_change_password, admin_edit_page, admin_page, admin_tab_page, admin_stop_all,
    api_admin_edit_container, api_create_user, api_delete_user, api_set_user_password,
    api_list_images, api_delete_image,
    api_get_image_env, api_set_image_env, api_duplicate_image, api_pull_image,
    api_admin_containers, api_admin_overview,
    api_audit_list,
    api_update_check, api_update_apply,
    api_admin_set_setting,
    api_ufw_status, api_ufw_toggle,
    api_cf_status, api_cf_uam_set,
};
use dns::{
    api_dns_list_providers, api_dns_add_provider, api_dns_update_provider, api_dns_delete_provider,
    api_dns_test_provider, api_dns_list_zones, api_dns_remote_records, api_dns_local_records,
    api_dns_add_record, api_dns_update_record, api_dns_delete_record,
    api_dns_public_ip, api_dns_sync, api_dns_sync_records, api_dns_set_proxy,
    api_dns_container_records,
    api_server_dns_list, api_server_dns_add, api_server_dns_delete,
};
use auth::{login_page, login_submit, logout};
use create::{api_image_env, api_image_env_overrides, api_local_images, create_server};
use dashboard::{api_dashboard_json, dashboard, new_server_page, server_list_fragment};
use files::{bulk_delete, copy_file, create_archive, create_new_file, delete_file, edit_file_page, extract_archive, list_files_api, list_files_json, move_file, rename_file, save_file_content, upload_files};
use network::{api_add_port, api_get_bandwidth, api_remove_port, api_set_bandwidth, api_tag_port, api_toggle_port, api_toggle_port_ufw, api_server_disk, networking_page};
use servers::{
    console_page, delete_server, files_page, get_server_stats, kill_server, rename_server,
    restart_server, settings_page, start_server, stop_server, api_update_env, api_factory_reset,
};
use ws::console_ws;

pub fn create_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", post(logout));

    // Routes accessible by any authenticated user
    let protected = Router::new()
        // Dashboard
        .route("/", get(dashboard))
        .route("/api/servers", get(server_list_fragment))
        .route("/api/dashboard", get(api_dashboard_json))
        // Server pages
        .route("/servers/{id}/console", get(console_page))
        .route("/servers/{id}/files", get(files_page))
        .route("/servers/{id}/settings", get(settings_page))
        .route("/servers/{id}/networking", get(networking_page))
        // Server actions
        .route("/api/servers/{id}/start", post(start_server))
        .route("/api/servers/{id}/stop", post(stop_server))
        .route("/api/servers/{id}/restart", post(restart_server))
        .route("/api/servers/{id}/kill", post(kill_server))
        .route("/api/servers/{id}/stats", get(get_server_stats))
        .route("/api/servers/{id}/rename", post(rename_server))
        // Networking
        .route("/api/servers/{id}/bandwidth", get(api_get_bandwidth).post(api_set_bandwidth))
        .route("/api/servers/{id}/ports/add", post(api_add_port))
        .route("/api/servers/{id}/ports/remove", post(api_remove_port))
        .route("/api/servers/{id}/ports/tag", post(api_tag_port))
        .route("/api/servers/{id}/ports/toggle", post(api_toggle_port))
        .route("/api/servers/{id}/ports/ufw", post(api_toggle_port_ufw))
        .route("/api/servers/{id}/disk", get(api_server_disk))
        .route("/api/servers/{id}/env", post(api_update_env))
        .route("/api/servers/{id}/factory-reset", post(api_factory_reset))
        // File manager
        .route("/servers/{id}/files/edit", get(edit_file_page))
        .route("/api/servers/{id}/files/list", get(list_files_api))
        .route("/api/servers/{id}/files/list-json", get(list_files_json))
        .route("/api/servers/{id}/files/save", post(save_file_content))
        .route("/api/servers/{id}/files/create", post(create_new_file))
        .route("/api/servers/{id}/files/delete", post(delete_file))
        .route("/api/servers/{id}/files/rename", post(rename_file))
        .route("/api/servers/{id}/files/copy", post(copy_file))
        .route("/api/servers/{id}/files/upload", post(upload_files)
            .layer(axum::extract::DefaultBodyLimit::disable()))
        .route("/api/servers/{id}/files/extract", post(extract_archive))
        .route("/api/servers/{id}/files/archive", post(create_archive))
        .route("/api/servers/{id}/files/bulk-delete", post(bulk_delete))
        .route("/api/servers/{id}/files/move", post(move_file))
        // WebSocket console
        .route("/api/servers/{id}/ws", get(console_ws))
        // Server DNS records (owner-accessible)
        .route("/api/servers/{id}/dns", get(api_server_dns_list))
        .route("/api/servers/{id}/dns/add", post(api_server_dns_add))
        .route("/api/servers/{id}/dns/{record_id}/delete", post(api_server_dns_delete))
        // DNS read-only (needed by console add-record form)
        .route("/api/dns/providers", get(api_dns_list_providers))
        .route("/api/dns/providers/{id}/zones", get(api_dns_list_zones))
        .route("/api/dns/public-ip", get(api_dns_public_ip))
        // Account (own user)
        .route("/api/user/change-password", post(admin_change_password))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware::require_auth,
        ));

    // Routes accessible by admin only (create server, admin panel, user management)
    let admin_only = Router::new()
        .route("/servers/new", get(new_server_page).post(create_server))
        .route("/api/image/env", get(api_image_env))
        .route("/api/image/env-overrides", get(api_image_env_overrides))
        .route("/api/image/local", get(api_local_images))
        .route("/admin", get(admin_page))
        .route("/admin/{tab}", get(admin_tab_page))
        .route("/admin/servers/{id}/edit", get(admin_edit_page))
        .route("/api/admin/stop-all", post(admin_stop_all))
        .route("/api/admin/change-password", post(admin_change_password))
        .route("/api/admin/users", post(api_create_user))
        .route("/api/admin/users/{id}/delete", post(api_delete_user))
        .route("/api/admin/users/{id}/set-password", post(api_set_user_password))
        .route("/api/admin/servers/{id}/edit", post(api_admin_edit_container))
        .route("/api/admin/images", get(api_list_images))
        .route("/api/admin/images/{ref}/delete", post(api_delete_image))
        .route("/api/admin/images/{ref}/env", get(api_get_image_env).post(api_set_image_env))
        .route("/api/admin/images/{ref}/duplicate", post(api_duplicate_image))
        .route("/api/admin/images/pull", post(api_pull_image))
        .route("/api/admin/containers", get(api_admin_containers))
        .route("/api/admin/overview", get(api_admin_overview))
        .route("/api/servers/{id}/delete", post(delete_server))
        // DNS management
        .route("/api/admin/dns/providers", get(api_dns_list_providers).post(api_dns_add_provider))
        .route("/api/admin/dns/providers/{id}/update", post(api_dns_update_provider))
        .route("/api/admin/dns/providers/{id}/delete", post(api_dns_delete_provider))
        .route("/api/admin/dns/providers/{id}/test",   post(api_dns_test_provider))
        .route("/api/admin/dns/providers/{id}/zones",  get(api_dns_list_zones))
        .route("/api/admin/dns/providers/{id}/records-remote", get(api_dns_remote_records))
        .route("/api/admin/dns/providers/{id}/records", get(api_dns_local_records))
        .route("/api/admin/dns/records",           post(api_dns_add_record))
        .route("/api/admin/dns/records/{id}/update", post(api_dns_update_record))
        .route("/api/admin/dns/records/{id}/delete", post(api_dns_delete_record))
        .route("/api/admin/dns/records/{id}/set-proxy", post(api_dns_set_proxy))
        .route("/api/admin/dns/public-ip",  get(api_dns_public_ip))
        .route("/api/admin/dns/sync",       post(api_dns_sync))
        .route("/api/admin/dns/providers/{id}/sync-records", post(api_dns_sync_records))
        .route("/api/admin/dns/container-records", get(api_dns_container_records))
        .route("/api/admin/audit", get(api_audit_list))
        .route("/api/admin/updates/check", get(api_update_check))
        .route("/api/admin/updates/apply", post(api_update_apply))
        .route("/api/admin/settings", post(api_admin_set_setting))
        .route("/api/admin/ufw/status", get(api_ufw_status))
        .route("/api/admin/ufw/toggle", post(api_ufw_toggle))
        .route("/api/admin/cf/status", get(api_cf_status))
        .route("/api/admin/cf/uam", post(api_cf_uam_set))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware::require_admin,
        ));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(admin_only)
        .route("/manifest.json", get(serve_manifest))
        .route("/sw.js", get(serve_sw))
        .nest_service("/static", ServeEmbed::<StaticAssets>::new())
        .fallback(fallback)
        .layer(middleware::from_fn_with_state(state.clone(), track_requests))
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}
