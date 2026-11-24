//! `/admin` – the dashboard: stat tiles, revenue chart, most rented machines,
//! upcoming rentals and service warnings (`db::stats::dashboard`).

use axum::extract::State;
use axum::response::Response;

use crate::errors::AppError;
use crate::routes::admin::{AdminNav, AdminPage, placeholder};
use crate::state::AppState;

/// `GET /admin` – `admin/dashboard.html` (chart: `views::BarChart::revenue` +
/// `partials/svg_bar_chart.html`).
pub async fn show(admin: AdminPage, State(_state): State<AppState>) -> Result<Response, AppError> {
    // TODO(feature: admin-dashboard)
    placeholder(admin, "Табло", AdminNav::Dashboard)
}
