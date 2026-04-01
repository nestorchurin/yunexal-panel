use bollard::Docker;
use sqlx::{Pool, Sqlite};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;
use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use tokio::sync::Mutex;

/// Per-IP login attempt tracker: (failed_count, last_attempt_time).
pub type LoginAttempts = Arc<DashMap<String, (u32, Instant)>>;
/// Per-IP general request counter for L7 flood detection: (count, window_start).
pub type RequestCounts = Arc<DashMap<String, (u32, Instant)>>;

/// Maximum failed login attempts before lockout.
pub const MAX_LOGIN_ATTEMPTS: u32 = 5;
/// Lockout window in seconds — attempts reset after this.
pub const LOGIN_WINDOW_SECS: u64 = 60;
/// Sliding window (seconds) for L7 HTTP-flood request counting.
pub const L7_WINDOW_SECS: u64 = 60;

#[derive(Clone)]
pub struct AppState {
    pub db: Pool<Sqlite>,
    pub docker: Docker,
    #[allow(dead_code)]
    pub cache: Arc<DashMap<String, String>>,
    pub cookie_key: Key,
    /// The address the server is listening on, e.g. "0.0.0.0:3000".
    pub listen_addr: String,
    /// Per-IP failed-login counter for brute-force protection.
    pub login_attempts: LoginAttempts,
    /// Cloudflare Web Analytics token; empty string means analytics disabled.
    pub cf_analytics_token: String,
    /// When `Some(t)`, the panel auto-triggered CF Under Attack Mode at time `t`.
    pub cf_uam_triggered_at: Arc<Mutex<Option<Instant>>>,
    /// Per-IP request counter for L7 HTTP-flood detection.
    pub request_counts: RequestCounts,
}

impl AppState {
    pub fn new(
        db: Pool<Sqlite>,
        docker: Docker,
        cookie_key: Key,
        listen_addr: String,
        cf_analytics_token: String,
    ) -> Self {
        Self {
            db,
            docker,
            cache: Arc::new(DashMap::new()),
            cookie_key,
            listen_addr,
            login_attempts: Arc::new(DashMap::new()),
            cf_analytics_token,
            cf_uam_triggered_at: Arc::new(Mutex::new(None)),
            request_counts: Arc::new(DashMap::new()),
        }
    }

    /// Record a failed login attempt for the given IP. Returns true if now locked out.
    pub fn record_failed_login(&self, ip: &str) -> bool {
        let now = Instant::now();
        let mut entry = self.login_attempts.entry(ip.to_string()).or_insert((0, now));
        let (count, last) = entry.value_mut();
        if now.duration_since(*last).as_secs() > LOGIN_WINDOW_SECS {
            // Window expired — reset counter.
            *count = 1;
            *last = now;
        } else {
            *count += 1;
            *last = now;
        }
        *count >= MAX_LOGIN_ATTEMPTS
    }

    /// Returns true if the IP is currently rate-limited.
    pub fn is_login_locked(&self, ip: &str) -> bool {
        if let Some(entry) = self.login_attempts.get(ip) {
            let (count, last) = entry.value();
            if *count >= MAX_LOGIN_ATTEMPTS {
                return Instant::now().duration_since(*last).as_secs() <= LOGIN_WINDOW_SECS;
            }
        }
        false
    }

    /// Clear the failed-login counter for the given IP (on successful login).
    pub fn clear_login_attempts(&self, ip: &str) {
        self.login_attempts.remove(ip);
    }

    /// Record a general HTTP request from the given IP for L7 flood detection.
    pub fn record_request(&self, ip: &str) {
        let now = Instant::now();
        let mut entry = self.request_counts.entry(ip.to_string()).or_insert((0, now));
        let (count, window_start) = entry.value_mut();
        if now.duration_since(*window_start).as_secs() >= L7_WINDOW_SECS {
            *count = 1;
            *window_start = now;
        } else {
            *count += 1;
        }
    }

    /// Count distinct IPs whose request rate in the current window exceeds `threshold`.
    pub fn l7_attacking_ips(&self, threshold: u32) -> usize {
        let now = Instant::now();
        self.request_counts.iter()
            .filter(|e| {
                let (count, window_start) = e.value();
                *count >= threshold && now.duration_since(*window_start).as_secs() < L7_WINDOW_SECS
            })
            .count()
    }
}

// Allows PrivateCookieJar to extract Key from AppState.
impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Key {
        state.cookie_key.clone()
    }
}
