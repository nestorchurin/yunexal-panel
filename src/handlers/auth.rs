use axum::{
    extract::{ConnectInfo, Form, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
    Json,
};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar, SameSite};
use time::Duration as TimeDuration;
use std::net::SocketAddr;
use tracing::{error, warn};
use crate::{auth, db, password};
use crate::state::AppState;
use super::templates::{render, LoginForm, LoginTemplate};

pub async fn login_page() -> impl IntoResponse {
    render(LoginTemplate { error: None })
}

pub async fn login_submit(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);

    // ── Rate-limit check ────────────────────────────────────────────────
    if state.is_login_locked(&ip) {
        warn!("Login rate-limited for IP {}", ip);
        let _ = db::audit_log(&state.db, &form.username, "auth.login_locked", "", "", &ip, &auth::user_agent(&headers)).await;
        return (StatusCode::TOO_MANY_REQUESTS, render(LoginTemplate {
            error: Some("Too many login attempts. Please try again later.".to_string()),
        }))
        .into_response();
    }

    // Look up by username or uid and verify hashed password.
    let found_user = db::find_user_by_username_or_uid(&state.db, form.username.trim())
        .await
        .ok()
        .flatten();
    let ok = found_user
        .as_ref()
        .map(|user| password::verify(&form.password, &user.password_hash))
        .unwrap_or(false);

    if ok {
        let user = match found_user {
            Some(u) => u,
            None => {
                return render(LoginTemplate {
                    error: Some("Invalid username or password.".to_string()),
                })
                .into_response();
            }
        };

        let session_username = user.username.clone();
        let session_id = auth::generate_session_id();

        if let Err(e) = db::create_user_session(
            &state.db,
            &session_id,
            user.id,
            &session_username,
            &ip,
            &auth::user_agent(&headers),
            &state.db_key,
        )
        .await
        {
            error!("login_submit: failed to create session: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                render(LoginTemplate {
                    error: Some("Cannot create login session. Please try again.".to_string()),
                }),
            )
                .into_response();
        }

        let session_cookie_value = auth::make_session_cookie_value(&session_username, &user.password_hash, &session_id);
        state.clear_login_attempts(&ip);
        let _ = db::audit_log(&state.db, &session_username, "auth.login", "", "", &ip, &auth::user_agent(&headers)).await;
        let mut cookie = Cookie::new(auth::SESSION_COOKIE, session_cookie_value);
        cookie.set_http_only(true);
        cookie.set_same_site(SameSite::Strict);
        cookie.set_secure(true);
        cookie.set_path("/");
        cookie.set_max_age(TimeDuration::days(7));
        let updated_jar = jar.add(cookie);
        (updated_jar, Redirect::to("/")).into_response()
    } else {
        let locked = state.record_failed_login(&ip);
        let _ = db::audit_log(&state.db, &form.username, "auth.login_failed", "", "", &ip, &auth::user_agent(&headers)).await;
        if locked {
            warn!("IP {} locked out after repeated failed logins", ip);
        }
        render(LoginTemplate {
            error: Some("Invalid username or password.".to_string()),
        })
        .into_response()
    }
}

pub async fn logout(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);
    let actor = auth::session_username(&jar).unwrap_or_default();
    if let Some(sid) = auth::session_id(&jar) {
        let _ = db::revoke_session_by_id(&state.db, &sid).await;
    }
    let _ = db::audit_log(&state.db, &actor, "auth.logout", "", "", &ip, &auth::user_agent(&headers)).await;
    let updated_jar = jar.remove(Cookie::from(auth::SESSION_COOKIE));
    (updated_jar, Redirect::to("/login")).into_response()
}

#[derive(serde::Deserialize)]
pub struct ServiceLoginBody {
    pub username: Option<String>,
}

/// POST /api/auth/service-login
///
/// Issues a regular panel session cookie for service clients authenticated
/// with API key headers:
/// - Authorization: Bearer <token>
/// - X-API-Key: <token>
pub async fn api_service_login(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    addr: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<ServiceLoginBody>,
) -> impl IntoResponse {
    let ip = auth::client_ip(&headers, addr);

    if !auth::is_service_api_request_authorized(&state, &headers).await {
        let _ = db::audit_log(&state.db, "service", "auth.service_login_denied", "", "invalid_api_key", &ip, &auth::user_agent(&headers)).await;
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Invalid API key"})),
        )
            .into_response();
    }

    let username = body
        .username
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("root")
        .to_string();

    let user = match db::find_user_by_username_or_uid(&state.db, &username).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "User not found"})),
            )
                .into_response();
        }
    };

    if !auth::role_has_permission(&state, &user.role, "admin.access").await {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "User has no API access"})),
        )
            .into_response();
    }

    let session_id = auth::generate_session_id();
    if let Err(e) = db::create_user_session(
        &state.db,
        &session_id,
        user.id,
        &username,
        &ip,
        &auth::user_agent(&headers),
        &state.db_key,
    )
    .await
    {
        error!("api_service_login: failed to create session: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Cannot create session"})),
        )
            .into_response();
    }

    let mut cookie = Cookie::new(
        auth::SESSION_COOKIE,
        auth::make_session_cookie_value(&username, &user.password_hash, &session_id),
    );
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_secure(true);
    cookie.set_path("/");
    cookie.set_max_age(TimeDuration::days(7));

    let updated_jar = jar.add(cookie);
    let _ = db::audit_log(&state.db, &username, "auth.service_login", "", "third_party_api", &ip, &auth::user_agent(&headers)).await;

    (
        updated_jar,
        Json(serde_json::json!({
            "ok": true,
            "username": username,
            "expires_days": 7,
        })),
    )
        .into_response()
}
