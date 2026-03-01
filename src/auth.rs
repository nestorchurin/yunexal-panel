use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::cookie::PrivateCookieJar;
use crate::{db, state::AppState};

pub const SESSION_COOKIE: &str = "session";

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
