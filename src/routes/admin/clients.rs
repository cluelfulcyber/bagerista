//! `/admin/klienti` – registered clients and one client's bookings.

use axum::extract::State;
use axum::response::Response;

use crate::errors::AppError;
use crate::routes::admin::{AdminNav, AdminPage, placeholder};
use crate::routes::page::PathOr404;
use crate::state::AppState;

/// `GET /admin/klienti` – `admin/clients.html` (db::users::list_clients).
pub async fn index(admin: AdminPage, State(_state): State<AppState>) -> Result<Response, AppError> {
    // TODO(feature: admin-clients)
    placeholder(admin, "Клиенти", AdminNav::Clients)
}

/// `GET /admin/klienti/{id}` – `admin/client_detail.html` (bookings + total spent).
pub async fn show(admin: AdminPage, State(_state): State<AppState>, PathOr404(_id): PathOr404<i64>) -> Result<Response, AppError> {
    // TODO(feature: admin-clients): db::users::client_summary + db::bookings::for_client
    placeholder(admin, "Клиент", AdminNav::Clients)
}
