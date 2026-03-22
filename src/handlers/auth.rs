use axum::{
    extract::{ConnectInfo, Form, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar, SameSite};
use time::Duration as TimeDuration;
use std::net::SocketAddr;
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
    // Look up user in DB and verify hashed password
    let ok = match db::find_user_by_username(&state.db, &form.username).await {
        Ok(Some(user)) => password::verify(&form.password, &user.password_hash),
        _ => false,
    };

    if ok {
        let _ = db::audit_log(&state.db, &form.username, "auth.login", "", "", &ip).await;
        let mut cookie = Cookie::new(auth::SESSION_COOKIE, form.username.clone());
        cookie.set_http_only(true);
        cookie.set_same_site(SameSite::Lax);
        cookie.set_path("/");
        cookie.set_max_age(TimeDuration::days(7));
        let updated_jar = jar.add(cookie);
        (updated_jar, Redirect::to("/")).into_response()
    } else {
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
    let _ = db::audit_log(&state.db, &actor, "auth.logout", "", "", &ip).await;
    let updated_jar = jar.remove(Cookie::from(auth::SESSION_COOKIE));
    (updated_jar, Redirect::to("/login")).into_response()
}
