//! Read-only JSON for the calendar and the live price calculator (architecture §7.5).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::routes::machines::MonthQuery;
use crate::state::AppState;

/// `?ot=2026-10-09&do=2026-10-15&operator=1&prikachni=3,5&km=20`
#[derive(Debug, Default, Deserialize)]
pub struct QuoteQuery {
    pub ot: Option<String>,
    #[serde(rename = "do")]
    pub to: Option<String>,
    pub operator: Option<String>,
    /// Attachment ids separated by commas.
    pub prikachni: Option<String>,
    pub km: Option<String>,
}

fn not_built() -> Response {
    (StatusCode::NOT_IMPLEMENTED, Json(json!({ "ok": false, "error": "Страницата се изгражда." }))).into_response()
}

/// `GET /api/mashini/{slug}/zaetost?mesec=2026-10`
/// → `{"machine": …, "month": "2026-10", "days": [{"date": …, "state": "free|booked|past"}]}`.
pub async fn availability(
    State(_state): State<AppState>,
    Path(_slug): Path<String>,
    Query(_query): Query<MonthQuery>,
) -> Response {
    // TODO(feature: machine): db::bookings::blocking_spans + services::availability::month_view
    not_built()
}

/// `GET /api/mashini/{slug}/cena?…` → `{"ok": true, "breakdown": …, "formatted": …, "available": …}`
/// or 422 `{"ok": false, "error": "…"}`.
pub async fn quote(
    State(_state): State<AppState>,
    Path(_slug): Path<String>,
    Query(_query): Query<QuoteQuery>,
) -> Response {
    // TODO(feature: machine): services::pricing::quote with server-side prices
    not_built()
}
