pub mod admin;
pub mod auth;
pub mod create;
pub mod dashboard;
pub mod files;
pub mod network;
pub mod servers;
pub mod templates;
pub mod ws;

use axum::{middleware, routing::{get, post}, Router};
use tower_http::services::{ServeDir, ServeFile};
use crate::auth as auth_middleware;
use crate::state::AppState;

use admin::{
    admin_change_password, admin_edit_page, admin_page, admin_tab_page, admin_stop_all,
    api_admin_edit_container, api_create_user, api_delete_user, api_set_user_password,
};
use auth::{login_page, login_submit, logout};
use create::{api_image_env, create_server};
use dashboard::{dashboard, new_server_page, server_list_fragment};
use files::{copy_file, create_new_file, delete_file, edit_file_page, list_files_api, rename_file, save_file_content, upload_files};
use network::{api_add_port, api_get_bandwidth, api_remove_port, api_set_bandwidth, api_tag_port, api_toggle_port, networking_page};
use servers::{
    console_page, delete_server, files_page, get_server_stats, kill_server, rename_server,
    restart_server, settings_page, start_server, stop_server,
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
        // File manager
        .route("/servers/{id}/files/edit", get(edit_file_page))
        .route("/api/servers/{id}/files/list", get(list_files_api))
        .route("/api/servers/{id}/files/save", post(save_file_content))
        .route("/api/servers/{id}/files/create", post(create_new_file))
        .route("/api/servers/{id}/files/delete", post(delete_file))
        .route("/api/servers/{id}/files/rename", post(rename_file))
        .route("/api/servers/{id}/files/copy", post(copy_file))
        .route("/api/servers/{id}/files/upload", post(upload_files)
            .layer(axum::extract::DefaultBodyLimit::disable()))
        // WebSocket console
        .route("/api/servers/{id}/ws", get(console_ws))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware::require_auth,
        ));

    // Routes accessible by admin only (create server, admin panel, user management)
    let admin_only = Router::new()
        .route("/servers/new", get(new_server_page).post(create_server))
        .route("/api/image/env", get(api_image_env))
        .route("/admin", get(admin_page))
        .route("/admin/{tab}", get(admin_tab_page))
        .route("/admin/servers/{id}/edit", get(admin_edit_page))
        .route("/api/admin/stop-all", post(admin_stop_all))
        .route("/api/admin/change-password", post(admin_change_password))
        .route("/api/admin/users", post(api_create_user))
        .route("/api/admin/users/{id}/delete", post(api_delete_user))
        .route("/api/admin/users/{id}/set-password", post(api_set_user_password))
        .route("/api/admin/servers/{id}/edit", post(api_admin_edit_container))
        .route("/api/servers/{id}/delete", post(delete_server))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware::require_admin,
        ));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(admin_only)
        .route_service("/manifest.json", ServeFile::new("static/manifest.json"))
        .route_service("/sw.js", ServeFile::new("static/sw.js"))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state)
}
