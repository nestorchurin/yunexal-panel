pub mod admin;
pub mod auth;
pub mod create;
pub mod dashboard;
pub mod files;
pub mod network;
pub mod servers;
pub mod schedules;
pub mod templates;
pub mod ws;

use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    middleware,
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use axum_embed::ServeEmbed;
use rust_embed::Embed;
use crate::auth as auth_middleware;
use crate::db;
use crate::state::AppState;

/// Per-request CSP nonce (128-bit, base64-encoded).
#[derive(Clone)]
pub struct CspNonce(pub String);


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

/// GET /favicon.ico — serves the custom favicon from {cwd}/custom/favicon.{ext}
/// if one has been uploaded, otherwise falls back to the embedded fav.jpg.
async fn serve_favicon(State(state): State<AppState>) -> impl IntoResponse {
    let ext = db::get_panel_setting(&state.db, "panel_favicon").await;
    if !ext.is_empty() {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let path = cwd.join("custom").join(format!("favicon.{ext}"));
        if let Ok(data) = tokio::fs::read(&path).await {
            let ct = match ext.as_str() {
                "png"  => "image/png",
                "gif"  => "image/gif",
                "webp" => "image/webp",
                "ico"  => "image/x-icon",
                _      => "image/jpeg",
            };
            return ([(header::CONTENT_TYPE, ct)], data).into_response();
        }
    }
    // Fallback: embedded fav.jpg
    match StaticAssets::get("fav.jpg") {
        Some(f) => ([(header::CONTENT_TYPE, "image/jpeg")], f.data.into_owned()).into_response(),
        None    => StatusCode::NOT_FOUND.into_response(),
    }
}

/// GET /api/theme/css — returns a tiny CSS file with the panel accent colour.
async fn serve_theme_css(State(state): State<AppState>) -> impl IntoResponse {
    let accent = db::get_panel_setting(&state.db, "panel_accent").await;
    let accent = if accent.is_empty() { "#7c3aed".to_string() } else { accent };
    // Sanitise: only allow valid hex colours (#xxxxxx / #xxx) or CSS named colours (alpha-only)
    let safe_accent = if accent.starts_with('#')
        && accent.len() >= 4
        && accent.len() <= 7
        && accent[1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        accent.clone()
    } else {
        "#7c3aed".to_string()
    };
    let css = format!(":root {{ --accent: {safe_accent}; }}");
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        css,
    ).into_response()
}

// ── Security headers middleware ───────────────────────────────────────────────

async fn security_headers(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl IntoResponse {
    // Generate a per-request 128-bit nonce for CSP.
    use rand::RngExt;
    use base64::Engine;
    let buf: [u8; 16] = rand::rng().random();
    let nonce = base64::engine::general_purpose::STANDARD.encode(buf);
    req.extensions_mut().insert(CspNonce(nonce.clone()));

    let mut resp = next.run(req).await;
    let h = resp.headers_mut();

    h.insert("X-Content-Type-Options",         HeaderValue::from_static("nosniff"));
    h.insert("X-Frame-Options",                HeaderValue::from_static("DENY"));

    let csp = format!(
        "default-src 'self'; \
         script-src 'nonce-{nonce}' https://unpkg.com https://cdn.jsdelivr.net; \
         script-src-attr 'unsafe-inline'; \
         style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://cdn.jsdelivr.net; \
         font-src 'self' https://fonts.gstatic.com https://cdn.jsdelivr.net; \
         img-src 'self' data:; \
         connect-src 'self' wss: https://cdn.jsdelivr.net; \
         frame-ancestors 'none'; \
         base-uri 'self'; \
         form-action 'self'; \
         worker-src 'self' blob:"
    );
    h.insert(
        "Content-Security-Policy",
        HeaderValue::from_str(&csp).unwrap_or_else(|_| HeaderValue::from_static("default-src 'self'")),
    );
    h.insert("Strict-Transport-Security",     HeaderValue::from_static("max-age=63072000; includeSubDomains; preload"));
    h.insert("Referrer-Policy",                HeaderValue::from_static("strict-origin-when-cross-origin"));
    h.insert("Permissions-Policy",             HeaderValue::from_static("camera=(), microphone=(), geolocation=()"));
    h.insert("X-XSS-Protection",              HeaderValue::from_static("0"));
    h.insert("Cross-Origin-Opener-Policy",    HeaderValue::from_static("same-origin"));
    h.insert("Cross-Origin-Embedder-Policy",   HeaderValue::from_static("credentialless"));
    h.insert("Cross-Origin-Resource-Policy",  HeaderValue::from_static("same-origin"));
    h.insert("X-Permitted-Cross-Domain-Policies", HeaderValue::from_static("none"));
    h.insert(
        "Cache-Control",
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );

    resp
}

// ── Sanitise Axum default rejection bodies (415 / 422) ───────────────────────

async fn sanitize_errors(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let resp = next.run(req).await;
    if resp.status() == StatusCode::UNPROCESSABLE_ENTITY
        || resp.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE
    {
        return (resp.status(), "Bad request").into_response();
    }
    resp
}

// ── Custom fallback for unmatched routes / framework rejections ──────────────

async fn fallback() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Html("<h1>404 — Not Found</h1>"))
}

use admin::{
    admin_change_password, admin_edit_page, admin_page, admin_tab_page, admin_stop_all,
    api_admin_edit_container, api_create_role, api_create_user, api_delete_role, api_delete_user,
    api_list_roles, api_set_role_permissions, api_set_user_password, api_set_user_role,
    api_list_images, api_delete_image,
    api_get_image_env, api_set_image_env, api_duplicate_image, api_pull_image,
    api_admin_containers, api_admin_overview,
    api_audit_list,
    api_update_check, api_update_apply,
    api_admin_set_setting,
    api_ufw_status, api_ufw_toggle,
    api_admin_storage_stats, api_admin_docker_daemon,
    api_admin_storage_mounts, api_admin_storage_disks,
    api_admin_storage_change_fs, api_admin_storage_migrate,
    api_admin_db_integrity,
    api_admin_theme_favicon,
    role_permissions_page,
};
use auth::{api_service_login, login_page, login_submit, logout};
use create::{api_build_image_from_dockerfile, api_image_env, api_image_env_overrides, api_local_images, api_quota_check, create_server};
use dashboard::{
    api_dashboard_json, api_user_devices, api_user_logout_device, dashboard, new_server_page,
    server_list_fragment,
};
use files::{bulk_delete, copy_file, create_archive, create_new_file, delete_file, download_bulk, download_file, edit_file_page, extract_archive, finalize_file_upload, list_files_api, list_files_json, move_file, rename_file, save_file_content, upload_file_chunk, upload_files};
use network::{api_add_port, api_get_bandwidth, api_remove_port, api_set_bandwidth, api_tag_port, api_toggle_port, api_toggle_port_ufw, api_server_disk, networking_page};
use servers::{
    api_server_member_add, api_server_member_remove, api_server_member_set_permission,
    api_server_members_list,
    console_page, delete_server, files_page, get_server_stats, kill_server, rename_server,
    restart_server, server_audit_page, settings_page, start_server, stop_server, api_get_env,
    api_update_env, api_set_env_permission,
    api_factory_reset, api_server_audit_download, api_server_audit_list, server_users_page,
};
use schedules::{
    schedules_page, api_list_schedules, api_create_schedule, api_update_schedule,
    api_delete_schedule, api_toggle_schedule, api_list_runs, api_run_now,
};
use ws::{console_ws, stats_ws};

pub fn create_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/login", get(login_page).post(login_submit))
        .route("/api/auth/service-login", post(api_service_login))
        .route("/logout", post(logout))
        .route("/favicon.ico", get(serve_favicon))
        .route("/api/theme/css", get(serve_theme_css));

    // Routes accessible by any authenticated user
    let protected = Router::new()
        // Dashboard
        .route("/", get(dashboard))
        .route("/api/servers", get(server_list_fragment))
        .route("/api/dashboard", get(api_dashboard_json))
        // Server pages
        .route("/servers/{id}/console", get(console_page))
        .route("/servers/{id}/files", get(files_page))
        .route("/servers/{id}/audit", get(server_audit_page))
        .route("/servers/{id}/settings", get(settings_page))
        .route("/servers/{id}/networking", get(networking_page))
        .route("/servers/{id}/users", get(server_users_page))
        // Schedules
        .route("/servers/{id}/schedules", get(schedules_page))
        .route("/api/servers/{id}/schedules", get(api_list_schedules).post(api_create_schedule))
        .route("/api/servers/{id}/schedules/{sid}/update", post(api_update_schedule))
        .route("/api/servers/{id}/schedules/{sid}/delete", post(api_delete_schedule))
        .route("/api/servers/{id}/schedules/{sid}/toggle", post(api_toggle_schedule))
        .route("/api/servers/{id}/schedules/{sid}/runs", get(api_list_runs))
        .route("/api/servers/{id}/schedules/{sid}/run-now", post(api_run_now))
        // Server actions
        .route("/api/servers/{id}/start", post(start_server))
        .route("/api/servers/{id}/stop", post(stop_server))
        .route("/api/servers/{id}/restart", post(restart_server))
        .route("/api/servers/{id}/kill", post(kill_server))
        .route("/api/servers/{id}/stats", get(get_server_stats))
        .route("/api/servers/{id}/audit", get(api_server_audit_list))
        .route("/api/servers/{id}/audit/download", get(api_server_audit_download))
        .route("/api/servers/{id}/members", get(api_server_members_list))
        .route("/api/servers/{id}/members/add", post(api_server_member_add))
        .route("/api/servers/{id}/members/{user_id}/permissions", post(api_server_member_set_permission))
        .route("/api/servers/{id}/members/{user_id}/remove", post(api_server_member_remove))
        .route("/api/servers/{id}/rename", post(rename_server))
        // Networking
        .route("/api/servers/{id}/bandwidth", get(api_get_bandwidth).post(api_set_bandwidth))
        .route("/api/servers/{id}/ports/add", post(api_add_port))
        .route("/api/servers/{id}/ports/remove", post(api_remove_port))
        .route("/api/servers/{id}/ports/tag", post(api_tag_port))
        .route("/api/servers/{id}/ports/toggle", post(api_toggle_port))
        .route("/api/servers/{id}/ports/ufw", post(api_toggle_port_ufw))
        .route("/api/servers/{id}/disk", get(api_server_disk))
        .route("/api/servers/{id}/env", get(api_get_env).post(api_update_env))
        .route("/api/servers/{id}/env/permissions", post(api_set_env_permission))
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
        .route("/api/servers/{id}/files/upload-chunk", post(upload_file_chunk)
            .layer(axum::extract::DefaultBodyLimit::disable()))
        .route("/api/servers/{id}/files/upload-complete", post(finalize_file_upload))
        .route("/api/servers/{id}/files/extract", post(extract_archive))
        .route("/api/servers/{id}/files/archive", post(create_archive))
        .route("/api/servers/{id}/files/bulk-delete", post(bulk_delete))
        .route("/api/servers/{id}/files/move", post(move_file))
        .route("/api/servers/{id}/files/download", get(download_file))
        .route("/api/servers/{id}/files/download-bulk", post(download_bulk))
        // WebSocket console + stats
        .route("/api/servers/{id}/ws", get(console_ws))
        .route("/ws/stats", get(stats_ws))
        // PWA manifest (behind auth)
        .route("/manifest.json", get(serve_manifest))
        // Account (own user)
        .route("/api/user/change-password", post(admin_change_password))
        .route("/api/user/devices", get(api_user_devices))
        .route("/api/user/devices/logout", post(api_user_logout_device))
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
        .route("/api/image/build-dockerfile", post(api_build_image_from_dockerfile)
            .layer(axum::extract::DefaultBodyLimit::disable()))
        .route("/api/quota-check", get(api_quota_check))
        .route("/admin", get(admin_page))
        .route("/admin/{tab}", get(admin_tab_page))
        .route("/admin/roles/{name}/edit", get(role_permissions_page))
        .route("/admin/servers/{id}/edit", get(admin_edit_page))
        .route("/api/admin/stop-all", post(admin_stop_all))
        .route("/api/admin/change-password", post(admin_change_password))
        .route("/api/admin/users", post(api_create_user))
        .route("/api/admin/users/{id}/delete", post(api_delete_user))
        .route("/api/admin/users/{id}/set-password", post(api_set_user_password))
        .route("/api/admin/users/{id}/set-role", post(api_set_user_role))
        .route("/api/admin/roles", get(api_list_roles).post(api_create_role))
        .route("/api/admin/roles/{name}/permissions", post(api_set_role_permissions))
        .route("/api/admin/roles/{name}/delete", post(api_delete_role))
        .route("/api/admin/servers/{id}/edit", post(api_admin_edit_container))
        .route("/api/admin/images", get(api_list_images))
        .route("/api/admin/images/{ref}/delete", post(api_delete_image))
        .route("/api/admin/images/{ref}/env", get(api_get_image_env).post(api_set_image_env))
        .route("/api/admin/images/{ref}/duplicate", post(api_duplicate_image))
        .route("/api/admin/images/pull", post(api_pull_image))
        .route("/api/admin/containers", get(api_admin_containers))
        .route("/api/admin/overview", get(api_admin_overview))
        .route("/api/servers/{id}/delete", post(delete_server))
        .route("/api/admin/audit", get(api_audit_list))
        .route("/api/admin/updates/check", get(api_update_check))
        .route("/api/admin/updates/apply", post(api_update_apply))
        .route("/api/admin/settings", post(api_admin_set_setting))
        .route("/api/admin/ufw/status", get(api_ufw_status))
        .route("/api/admin/ufw/toggle", post(api_ufw_toggle))
        .route("/api/admin/storage/stats", get(api_admin_storage_stats))
        .route("/api/admin/storage/daemon", post(api_admin_docker_daemon))
        .route("/api/admin/storage/mounts", get(api_admin_storage_mounts))
        .route("/api/admin/storage/disks", get(api_admin_storage_disks))
        .route("/api/admin/storage/change-fs", post(api_admin_storage_change_fs))
        .route("/api/admin/storage/migrate", post(api_admin_storage_migrate))
        .route("/api/admin/db-integrity", post(api_admin_db_integrity))
        .route("/api/admin/theme/favicon", post(api_admin_theme_favicon))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware::require_admin,
        ));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(admin_only)
        .route("/sw.js", get(serve_sw))
        .nest_service("/static", ServeEmbed::<StaticAssets>::new())
        .fallback(fallback)
        .layer(middleware::from_fn(sanitize_errors))
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}
