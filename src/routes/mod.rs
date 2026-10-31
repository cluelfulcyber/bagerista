//! WEB LAYER: the list of all routes (architecture §7).
//!
//! TEMPORARY: only `GET /` exists for now; the other handlers are added by
//! the web-layer part of the project.

use axum::Router;
use axum::routing::get;

use crate::errors;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(home)).fallback(errors::not_found)
}

async fn home() -> &'static str {
    "ok"
}
