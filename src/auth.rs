use axum::{
    extract::{ConnectInfo, Request, State},
    http::HeaderMap,
    middleware::Next,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::cookie::PrivateCookieJar;
use crate::{db, state::AppState};
use std::net::SocketAddr;

pub const SESSION_COOKIE: &str = "session";

/// Extract client IP from X-Forwarded-For / X-Real-IP headers, falling back to socket address.
pub fn client_ip(headers: &HeaderMap, addr: ConnectInfo<SocketAddr>) -> String {
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
    addr.0.ip().to_string()
}

/// Returns the username stored in the session cookie, if any.
pub fn session_username(jar: &PrivateCookieJar) -> Option<String> {
    jar.get(SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .filter(|v| !v.is_empty())
}

/// Returns the DB user id for the current session, or None.
pub async fn session_user_id(state: &AppState, jar: &PrivateCookieJar) -> Option<i64> {
    let username = session_username(jar)?;
    db::find_user_by_username(&state.db, &username)
        .await
        .ok()
        .flatten()
        .map(|u| u.id)
}

/// Returns true if the current session belongs to an admin/root user.
pub async fn is_admin_session(state: &AppState, jar: &PrivateCookieJar) -> bool {
    let username = match session_username(jar) {
        Some(u) => u,
        None => return false,
    };
    matches!(
        db::find_user_by_username(&state.db, &username).await,
        Ok(Some(u)) if db::is_admin_role(&u.role)
    )
}

/// Middleware: redirects to /login if not authenticated or if user was deleted.
pub async fn require_auth(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    request: Request,
    next: Next,
) -> impl IntoResponse {
    let username = match session_username(&jar) {
        Some(u) => u,
        None => return Redirect::to("/login").into_response(),
    };
    // Confirm the user still exists in the DB — auto-logout if deleted.
    match db::find_user_by_username(&state.db, &username).await {
        Ok(Some(_)) => next.run(request).await.into_response(),
        _ => Redirect::to("/login").into_response(),
    }
}

/// Returns true if the session user is admin or owns the server with the given db_id.
pub async fn can_access_server(state: &AppState, jar: &PrivateCookieJar, db_id: i64) -> bool {
    if is_admin_session(state, jar).await {
        return true;
    }
    let uid = match session_user_id(state, jar).await {
        Some(id) => id,
        None => return false,
    };
    matches!(
        db::get_server_owner_by_db_id(&state.db, db_id).await,
        Ok(Some(owner_id)) if owner_id == uid
    )
}

/// Middleware: redirects to / if not admin, or /login if user was deleted.
pub async fn require_admin(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    request: Request,
    next: Next,
) -> impl IntoResponse {
    let username = match session_username(&jar) {
        Some(u) => u,
        None => return Redirect::to("/login").into_response(),
    };
    match db::find_user_by_username(&state.db, &username).await {
        Ok(Some(u)) if db::is_admin_role(&u.role) => next.run(request).await.into_response(),
        Ok(Some(_)) => Redirect::to("/").into_response(),   // logged in but not admin
        _ => Redirect::to("/login").into_response(),        // user deleted or DB error
    }
}
