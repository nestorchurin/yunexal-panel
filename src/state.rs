use bollard::Docker;
use sqlx::{Pool, Sqlite};
use dashmap::DashMap;
use std::sync::Arc;
use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;

#[derive(Clone)]
pub struct AppState {
    pub db: Pool<Sqlite>,
    pub docker: Docker,
    #[allow(dead_code)]
    pub cache: Arc<DashMap<String, String>>,
    pub cookie_key: Key,
    /// Display name of the primary admin (from env, used in templates only).
    pub auth_username: String,
}

impl AppState {
    pub fn new(
        db: Pool<Sqlite>,
        docker: Docker,
        cookie_key: Key,
        auth_username: String,
    ) -> Self {
        Self {
            db,
            docker,
            cache: Arc::new(DashMap::new()),
            cookie_key,
            auth_username,
        }
    }
}

// Allows PrivateCookieJar to extract Key from AppState.
impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Key {
        state.cookie_key.clone()
    }
}
