//! `/admin/obsluzhvane` – all machines sorted by hours left to service, with history.

use axum::extract::State;
use axum::response::Response;

use crate::errors::AppError;
use crate::routes::admin::{AdminNav, AdminPage, placeholder};
use crate::state::AppState;

/// `GET /admin/obsluzhvane` – `admin/maintenance.html` (db::service::fleet_maintenance,
/// db::service::recent; badges via `partials/service_badge.html`).
pub async fn index(admin: AdminPage, State(_state): State<AppState>) -> Result<Response, AppError> {
    // TODO(feature: admin-maintenance)
    placeholder(admin, "Обслужване", AdminNav::Maintenance)
}
