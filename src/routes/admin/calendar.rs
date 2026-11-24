//! `/admin/kalendar` – the fleet calendar: machines × days (design §6.27).

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;

use crate::errors::AppError;
use crate::routes::admin::{AdminNav, AdminPage, placeholder};
use crate::routes::machines::MonthQuery;
use crate::state::AppState;

/// `GET /admin/kalendar?mesec=2026-10` – `admin/calendar.html`; the month grid
/// alone for htmx (`page::is_htmx`). Data: db::bookings::fleet_spans.
pub async fn show(admin: AdminPage, State(_state): State<AppState>, headers: HeaderMap, Query(_query): Query<MonthQuery>) -> Result<Response, AppError> {
    // TODO(feature: admin-calendar): approved = ink, active = moss, new = hatched yellow, conflicts outlined
    let _ = headers;
    placeholder(admin, "Календар на парка", AdminNav::Calendar)
}
