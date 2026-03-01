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
    /// The address the server is listening on, e.g. "0.0.0.0:3000".
    pub listen_addr: String,
}

impl AppState {
    pub fn new(
        db: Pool<Sqlite>,
        docker: Docker,
        cookie_key: Key,
        listen_addr: String,
    ) -> Self {
        Self {
            db,
            docker,
            cache: Arc::new(DashMap::new()),
            cookie_key,
            listen_addr,
        }
    }
}

// Allows PrivateCookieJar to extract Key from AppState.
impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Key {
        state.cookie_key.clone()
    }
}
