//! Багериста – website of an excavator rental company (diploma project).
//!
//! Three layers:
//! - `routes/`   – web layer: HTTP handlers and templates;
//! - `services/` – business rules as pure functions (price, availability,
//!   wizard, maintenance, booking statuses);
//! - `db/`       – the only place with SQL.
//!
//! `build_app` returns the complete router; the integration tests use the
//! same function, so they test exactly what runs in production.

// Money literals are written as euros_cents: `160_00` = 160,00 € (16 000 cents).
#![allow(clippy::inconsistent_digit_grouping)]

pub mod auth;
pub mod config;
pub mod csrf;
pub mod db;
pub mod errors;
pub mod format;
pub mod models;
pub mod routes;
pub mod seed;
pub mod services;
pub mod session_store;
pub mod state;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Request};
use axum::http::HeaderValue;
use axum::http::header;
use axum::middleware::{self, Next};
use axum::response::Response;
use chrono::NaiveDate;
use tower_sessions::cookie::SameSite;
use tower_sessions::cookie::time::Duration;
use tower_sessions::{Expiry, SessionManagerLayer};

pub use config::Config;
pub use state::AppState;

/// Name of the session cookie.
pub const SESSION_COOKIE: &str = "bagerista_sid";
/// Largest request body (a 2 MB photo plus the form around it).
pub const MAX_BODY_BYTES: usize = 3 * 1024 * 1024;

/// Content-Security-Policy: everything is served by us, no inline scripts.
/// `style-src-attr 'unsafe-inline'` allows only `style="--need: 1.5"` attributes,
/// which the server uses to pass numbers to the CSS (bars, gauges, ruler).
pub const CSP: &str = "default-src 'self'; img-src 'self' data:; style-src 'self'; \
style-src-attr 'unsafe-inline'; script-src 'self'; frame-ancestors 'none'; \
form-action 'self'; base-uri 'self'";

/// Today's date on the server (local time zone).
pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Starts the whole application: database, demo data, web server.
pub async fn run(cfg: Config) -> Result<(), Box<dyn std::error::Error>> {
    // `try_init` so a second call (e.g. in tests) does not panic
    let _ = tracing_subscriber::fmt::try_init();

    let db = db::connect(&cfg.db_path).await?;
    db::migrate(&db).await?;
    if cfg.seed && db::users::count(&db).await? == 0 {
        tracing::info!("Празна база данни – зареждам демонстрационни данни…");
        seed::run(&db, today()).await?;
    }
    session_store::delete_expired(&db).await?;

    let app = build_app(AppState::new(db, cfg.secure_cookies));
    let listener = tokio::net::TcpListener::bind(&cfg.addr).await?;
    let url = format!("http://{}", listener.local_addr()?);
    tracing::info!("Багериста работи на {url}");
    if cfg.open_browser {
        open_browser(&url);
    }
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}

/// The complete application: all routes plus sessions, CSRF and security headers.
pub fn build_app(state: AppState) -> Router {
    with_layers(routes::router(), state)
}

/// Wraps any router in the middleware stack used in production.
/// (Public so tests can wrap small test-only routers in the same layers.)
///
/// A request passes the layers from the outside in:
/// security headers → session (loads the cookie's session) → CSRF check → handler.
pub fn with_layers(router: Router<AppState>, state: AppState) -> Router {
    let sessions = SessionManagerLayer::new(session_store::SqliteStore::new(state.db.clone()))
        .with_name(SESSION_COOKIE)
        .with_http_only(true)
        .with_same_site(SameSite::Lax)
        .with_secure(state.secure_cookies)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)));

    router
        .layer(middleware::from_fn(csrf::csrf_guard))
        .layer(sessions)
        .layer(middleware::from_fn(security_headers))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

/// Adds the security headers to every response (architecture §8).
async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("same-origin"));
    response
}

/// Waits for Ctrl+C, then lets running requests finish.
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("Спиране на сървъра…");
}

/// Opens the site in the default browser (no extra crate: the OS command).
fn open_browser(url: &str) {
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd").args(["/C", "start", url]).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(e) = result {
        tracing::warn!("Не успях да отворя браузъра: {e}");
    }
}
