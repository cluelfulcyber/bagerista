//! Shared application state, cloned into every request handler.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::auth::LoginLimiter;

#[derive(Clone)]
pub struct AppState {
    /// Connection pool to the SQLite database.
    pub db: SqlitePool,
    /// Counts failed logins per e-mail (brute-force protection).
    pub login_limiter: Arc<LoginLimiter>,
    /// Whether the session cookie gets the `Secure` flag.
    pub secure_cookies: bool,
}

impl AppState {
    pub fn new(db: SqlitePool, secure_cookies: bool) -> AppState {
        AppState {
            db,
            login_limiter: Arc::new(LoginLimiter::default()),
            secure_cookies,
        }
    }
}
