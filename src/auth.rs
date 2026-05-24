use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, Method},
    middleware::Next,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar};
use base64::Engine;
use rand::RngExt;
use crate::{db, state::AppState};
use std::net::{IpAddr, SocketAddr};

pub const SESSION_COOKIE: &str = "session";
const SESSION_COOKIE_VERSION: &str = "v2";

#[derive(Debug, Clone)]
struct ParsedSessionCookie {
    username: String,
    stamp: Option<String>,
    session_id: Option<String>,
}

#[derive(Debug, Clone)]
struct SessionContext {
    user: db::User,
    session_id: String,
}

fn parse_session_cookie_value(raw: &str) -> Option<ParsedSessionCookie> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    let prefix = format!("{}.", SESSION_COOKIE_VERSION);
    if let Some(rest) = raw.strip_prefix(&prefix) {
        let mut parts = rest.splitn(3, '.');
        let username_part = parts.next()?;
        let session_part = parts.next()?.trim();
        let stamp_part = parts.next()?.trim();
        if username_part.is_empty() || session_part.is_empty() || stamp_part.is_empty() {
            return None;
        }

        let username_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(username_part)
            .ok()?;
        let username = String::from_utf8(username_bytes).ok()?;
        if username.trim().is_empty() {
            return None;
        }

        return Some(ParsedSessionCookie {
            username,
            stamp: Some(stamp_part.to_string()),
            session_id: Some(session_part.to_string()),
        });
    }

    // Previous stamped format fallback: v1.<b64(username)>.<stamp>
    if let Some(rest) = raw.strip_prefix("v1.") {
        let mut parts = rest.splitn(2, '.');
        let username_part = parts.next()?;
        let stamp_part = parts.next()?.trim();
        if username_part.is_empty() || stamp_part.is_empty() {
            return None;
        }

        let username_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(username_part)
            .ok()?;
        let username = String::from_utf8(username_bytes).ok()?;
        if username.trim().is_empty() {
            return None;
        }

        return Some(ParsedSessionCookie {
            username,
            stamp: Some(stamp_part.to_string()),
            session_id: None,
        });
    }

    // Legacy format fallback: username only.
    Some(ParsedSessionCookie {
        username: raw.to_string(),
        stamp: None,
        session_id: None,
    })
}

fn session_stamp_from_password_hash(password_hash: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(password_hash.as_bytes())
}

pub fn generate_session_id() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 24] = rng.random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn make_session_cookie_value(username: &str, password_hash: &str, session_id: &str) -> String {
    let username_enc = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(username.as_bytes());
    let stamp = session_stamp_from_password_hash(password_hash);
    format!("{}.{}.{}.{}", SESSION_COOKIE_VERSION, username_enc, session_id, stamp)
}

fn login_redirect_with_cookie_clear(jar: PrivateCookieJar) -> axum::response::Response {
    let updated_jar = jar.remove(Cookie::from(SESSION_COOKIE));
    (updated_jar, Redirect::to("/login")).into_response()
}

async fn session_context(state: &AppState, jar: &PrivateCookieJar) -> Option<SessionContext> {
    let raw = jar.get(SESSION_COOKIE)?.value().to_string();
    let parsed = parse_session_cookie_value(&raw)?;
    let user = db::find_user_by_username(&state.db, &parsed.username)
        .await
        .ok()
        .flatten()?;

    let session_id = parsed.session_id?.trim().to_string();
    if session_id.is_empty() {
        return None;
    }

    let expected_stamp = session_stamp_from_password_hash(&user.password_hash);
    if parsed.stamp.as_deref() != Some(expected_stamp.as_str()) {
        return None;
    }

    let active = db::is_user_session_active(&state.db, user.id, &session_id)
        .await
        .ok()
        .unwrap_or(false);
    if !active {
        return None;
    }

    Some(SessionContext { user, session_id })
}

async fn session_user(state: &AppState, jar: &PrivateCookieJar) -> Option<db::User> {
    session_context(state, jar).await.map(|ctx| ctx.user)
}

/// Maps admin tab names to permission keys.
pub fn permission_for_admin_tab(tab: &str) -> &'static str {
    match tab {
        "overview" => "tab.overview",
        "containers" => "tab.containers",
        "images" => "tab.images",
        "users" => "tab.users",
        "roles" => "tab.roles",
        "audit" => "tab.audit",
        "settings" => "tab.settings",
        _ => "admin.access",
    }
}

fn required_admin_permission_for_path(path: &str, method: &Method) -> &'static str {
    if path == "/admin" {
        return "admin.access";
    }
    if let Some(tab) = path.strip_prefix("/admin/") {
        // Keep /admin/servers/{id}/edit separate from regular tabs.
        if tab.starts_with("servers/") {
            return "servers.edit";
        }
        let base = tab.split('/').next().unwrap_or(tab);
        return permission_for_admin_tab(base);
    }

    if path == "/servers/new" || path == "/api/quota-check" {
        return "servers.create";
    }
    if path.starts_with("/api/image/") {
        return "servers.create";
    }
    if path == "/api/admin/users" || path.starts_with("/api/admin/users/") {
        return "users.manage";
    }
    if path == "/api/admin/roles" && *method == Method::GET {
        return "users.manage";
    }
    if path == "/api/admin/roles" || path.starts_with("/api/admin/roles/") {
        return "roles.manage";
    }
    if path == "/api/admin/images" || path.starts_with("/api/admin/images/") {
        return "images.manage";
    }
    if path == "/api/admin/audit" {
        return "audit.read";
    }
    if path.starts_with("/api/admin/updates/") {
        return "panel.update";
    }
    if path == "/api/admin/settings" || path == "/api/admin/db-integrity" {
        return "panel.settings";
    }
    if path.starts_with("/api/admin/storage/") {
        return "storage.manage";
    }
    if path.starts_with("/api/admin/ufw/") {
        return "security.manage";
    }
    if path == "/api/admin/theme/favicon" {
        return "theme.manage";
    }
    if path == "/api/admin/overview" || path == "/api/admin/containers" || path == "/api/admin/stop-all" {
        return "containers.manage";
    }
    if path.starts_with("/api/admin/servers/") {
        return "servers.edit";
    }
    if path.starts_with("/api/servers/") && path.ends_with("/delete") {
        return "servers.delete";
    }

    "admin.access"
}

pub async fn role_has_permission(state: &AppState, role: &str, permission: &str) -> bool {
    match db::role_has_write_permission(&state.db, role, permission).await {
        Ok(v) => v,
        Err(_) => false,
    }
}

pub async fn role_has_read_permission(state: &AppState, role: &str, permission: &str) -> bool {
    match db::role_has_read_permission(&state.db, role, permission).await {
        Ok(v) => v,
        Err(_) => false,
    }
}

/// Extract client IP from X-Forwarded-For / X-Real-IP headers, falling back to socket address.
pub fn client_ip(headers: &HeaderMap, addr: ConnectInfo<SocketAddr>) -> String {
    let remote_ip = addr.0.ip();

    // Only honor forwarding headers when the direct peer is a trusted proxy.
    // This prevents clients from spoofing X-Forwarded-For/X-Real-IP directly.
    let trusted_proxy = match remote_ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unique_local(),
    };

    if trusted_proxy {
        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = xff.split(',').next() {
                let ip = first.trim();
                if !ip.is_empty() {
                    return ip.to_string();
                }
            }
        }

        if let Some(real) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            let ip = real.trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
    }

    remote_ip.to_string()
}

/// Extract User-Agent header as a string (truncated to 256 chars).
pub fn user_agent(headers: &HeaderMap) -> String {
    headers.get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| if s.len() > 256 { &s[..256] } else { s })
        .unwrap_or("")
        .to_string()
}

/// Extract service API key from Authorization: Bearer <token> or X-API-Key header.
pub fn service_api_key_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(authz) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        let s = authz.trim();
        if let Some(rest) = s.strip_prefix("Bearer ") {
            let token = rest.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    if let Some(v) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        let token = v.trim();
        if !token.is_empty() {
            return Some(token.to_string());
        }
    }
    None
}

/// Returns true when request provides a valid service API key.
///
/// Key sources (priority):
/// 1. panel setting `service_api_key`
/// 2. env `YUNEXAL_API_KEY`
/// 3. env `PANEL_API_KEY`
pub async fn is_service_api_request_authorized(state: &AppState, headers: &HeaderMap) -> bool {
    let provided = match service_api_key_from_headers(headers) {
        Some(v) => v,
        None => return false,
    };

    let configured_db = db::get_service_api_key(&state.db, &state.db_key).await;
    let configured = if !configured_db.trim().is_empty() {
        configured_db
    } else {
        std::env::var("YUNEXAL_API_KEY")
            .ok()
            .or_else(|| std::env::var("PANEL_API_KEY").ok())
            .unwrap_or_default()
    };

    !configured.trim().is_empty() && provided == configured.trim()
}

/// Returns the username stored in the session cookie, if any.
pub fn session_username(jar: &PrivateCookieJar) -> Option<String> {
    jar.get(SESSION_COOKIE)
        .and_then(|c| parse_session_cookie_value(c.value()))
        .map(|v| v.username)
        .filter(|v| !v.is_empty())
}

/// Returns the current session id from cookie (if present).
pub fn session_id(jar: &PrivateCookieJar) -> Option<String> {
    jar.get(SESSION_COOKIE)
        .and_then(|c| parse_session_cookie_value(c.value()))
        .and_then(|v| v.session_id)
        .filter(|v| !v.trim().is_empty())
}

/// Returns the DB user id for the current session, or None.
pub async fn session_user_id(state: &AppState, jar: &PrivateCookieJar) -> Option<i64> {
    session_user(state, jar).await.map(|u| u.id)
}

/// Returns true if the current session belongs to an admin/root user.
pub async fn is_admin_session(state: &AppState, jar: &PrivateCookieJar) -> bool {
    let user = match session_user(state, jar).await {
        Some(u) => u,
        _ => return false,
    };
    role_has_permission(state, &user.role, "admin.access").await
}

/// Returns true if the current session belongs to the root user.
pub async fn is_root_session(state: &AppState, jar: &PrivateCookieJar) -> bool {
    matches!(session_user(state, jar).await, Some(u) if u.role == "root")
}

pub async fn session_has_permission(state: &AppState, jar: &PrivateCookieJar, permission: &str) -> bool {
    match session_user(state, jar).await {
        Some(u) => role_has_permission(state, &u.role, permission).await,
        None => false,
    }
}

/// Middleware: redirects to /login if not authenticated or if user was deleted.
pub async fn require_auth(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    request: Request,
    next: Next,
) -> impl IntoResponse {
    match session_context(&state, &jar).await {
        Some(ctx) => {
            let _ = db::touch_user_session(&state.db, &ctx.session_id).await;
            next.run(request).await.into_response()
        }
        None => login_redirect_with_cookie_clear(jar),
    }
}

/// Returns true if the session user is admin or owns the server with the given db_id.
pub async fn can_access_server(state: &AppState, jar: &PrivateCookieJar, db_id: i64) -> bool {
    let user = match session_user(state, jar).await {
        Some(u) => u,
        _ => return false,
    };

    if role_has_permission(state, &user.role, "servers.global_access").await {
        return true;
    }

    if matches!(
        db::get_server_owner_by_db_id(&state.db, db_id).await,
        Ok(Some(owner_id)) if owner_id == user.id
    ) {
        return true;
    }

    db::server_user_has_any_access(&state.db, db_id, user.id)
        .await
        .unwrap_or(false)
}

/// Checks access to a specific server capability for the current session user.
///
/// Owners and users with global server access bypass member-level capability checks.
pub async fn can_access_server_permission(
    state: &AppState,
    jar: &PrivateCookieJar,
    db_id: i64,
    permission: &str,
    require_write: bool,
) -> bool {
    let user = match session_user(state, jar).await {
        Some(u) => u,
        _ => return false,
    };

    if role_has_permission(state, &user.role, "servers.global_access").await {
        return true;
    }

    if matches!(
        db::get_server_owner_by_db_id(&state.db, db_id).await,
        Ok(Some(owner_id)) if owner_id == user.id
    ) {
        return true;
    }

    if require_write {
        db::server_user_has_write_permission(&state.db, db_id, user.id, permission)
            .await
            .unwrap_or(false)
    } else {
        db::server_user_has_read_permission(&state.db, db_id, user.id, permission)
            .await
            .unwrap_or(false)
    }
}

/// Middleware: redirects to / if not admin, or /login if user was deleted.
pub async fn require_admin(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    request: Request,
    next: Next,
) -> impl IntoResponse {
    match session_context(&state, &jar).await {
        Some(ctx) => {
            let _ = db::touch_user_session(&state.db, &ctx.session_id).await;
            let u = ctx.user;
            if !role_has_permission(&state, &u.role, "admin.access").await {
                return Redirect::to("/").into_response();
            }

            let req_perm = required_admin_permission_for_path(request.uri().path(), request.method());
            let can_read = role_has_read_permission(&state, &u.role, req_perm).await;
            if req_perm != "admin.access" && !can_read {
                return Redirect::to("/admin").into_response();
            }

            next.run(request).await.into_response()
        }
        None => login_redirect_with_cookie_clear(jar), // invalid/legacy cookie or deleted user
    }
}
