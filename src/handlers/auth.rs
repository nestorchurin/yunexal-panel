use axum::{
    extract::{Form, State},
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar, SameSite};
use crate::{auth, db, password};
use crate::state::AppState;
use super::templates::{render, LoginForm, LoginTemplate};

pub async fn login_page() -> impl IntoResponse {
    render(LoginTemplate { error: None })
}

pub async fn login_submit(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    // Look up user in DB and verify hashed password
    let ok = match db::find_user_by_username(&state.db, &form.username).await {
        Ok(Some(user)) => password::verify(&form.password, &user.password_hash),
        _ => false,
    };

    if ok {
        // Store the username (not a fixed string) so role can be looked up later.
        let mut cookie = Cookie::new(auth::SESSION_COOKIE, form.username.clone());
        cookie.set_http_only(true);
        cookie.set_same_site(SameSite::Lax);
        cookie.set_path("/");
        let updated_jar = jar.add(cookie);
        (updated_jar, Redirect::to("/")).into_response()
    } else {
        render(LoginTemplate {
            error: Some("Invalid username or password.".to_string()),
        })
        .into_response()
    }
}

pub async fn logout(jar: PrivateCookieJar) -> impl IntoResponse {
    let updated_jar = jar.remove(Cookie::from(auth::SESSION_COOKIE));
    (updated_jar, Redirect::to("/login")).into_response()
}
