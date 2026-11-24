//! Administration (`/admin/*`). Every handler takes `AdminPage` as an
//! argument: the extractor lets only administrators in (guest → login page,
//! client → 403) and carries the layout of `templates/admin/layout.html`.
//!
//! Pattern (same as public pages, see `routes/page.rs`):
//! ```text
//! #[derive(Template)]
//! #[template(path = "admin/clients.html")]   // {% extends "admin/layout.html" %}
//! struct ClientsPage { admin: AdminPage, clients: Vec<ClientSummary> }
//!
//! pub async fn index(admin: AdminPage, State(state): State<AppState>) -> Result<Response, AppError> {
//!     let clients = db::users::list_clients(&state.db).await?;
//!     render(&ClientsPage { admin: admin.titled("Клиенти", AdminNav::Clients), clients })
//! }
//! ```

pub mod attachments;
pub mod bookings;
pub mod calendar;
pub mod clients;
pub mod dashboard;
pub mod machines;
pub mod maintenance;

use askama::Template;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};

use crate::auth::RequireAdmin;
use crate::db;
use crate::errors::AppError;
use crate::format;
use crate::models::{BookingStatus, User};
use crate::routes::page::{Layout, Nav, render};
use crate::services::maintenance::MaintenanceLevel;
use crate::state::AppState;

/// Item of the admin sidebar marked as current.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminNav {
    Dashboard,
    Bookings,
    Calendar,
    Machines,
    Attachments,
    Maintenance,
    Clients,
}

impl AdminNav {
    pub fn key(self) -> &'static str {
        match self {
            AdminNav::Dashboard => "tablo",
            AdminNav::Bookings => "zayavki",
            AdminNav::Calendar => "kalendar",
            AdminNav::Machines => "mashini",
            AdminNav::Attachments => "prikachni",
            AdminNav::Maintenance => "obsluzhvane",
            AdminNav::Clients => "klienti",
        }
    }
}

/// Layout of every admin page: the logged-in admin, the page heading and the
/// two numbers in the sidebar („Заявки 4“, „Обслужване 2“).
#[derive(Debug, Clone)]
pub struct AdminPage {
    /// Title, CSRF token and flash, exactly as on public pages.
    pub layout: Layout,
    pub admin: User,
    /// Big heading of the workspace (`.admin-top__title`).
    pub heading: String,
    pub nav: AdminNav,
    /// Requests with status „Нова“.
    pub new_requests: i64,
    /// Active machines whose service is overdue.
    pub service_alerts: i64,
}

impl AdminPage {
    /// Names the page: heading „Заявки“, `<title>` „Заявки: Багериста“, current menu item.
    pub fn titled(mut self, heading: &str, nav: AdminNav) -> AdminPage {
        self.layout = self.layout.titled(heading, Nav::None);
        self.heading = heading.to_string();
        self.nav = nav;
        self
    }

    pub fn nav_is(&self, key: &str) -> bool {
        self.nav.key() == key
    }

    /// „четвъртък, 8 октомври 2026“ under the heading.
    pub fn today_text(&self) -> String {
        format::date_with_weekday(self.layout.today)
    }
}

impl FromRequestParts<AppState> for AdminPage {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        // The role check comes first: nothing else is loaded for a non-admin.
        let RequireAdmin(admin) = RequireAdmin::from_request_parts(parts, state).await?;
        let layout = Layout::from_request_parts(parts, state).await.map_err(IntoResponse::into_response)?;
        let counts = async {
            let new_requests = db::bookings::count_by_status(&state.db, BookingStatus::New).await?;
            let service_alerts = db::service::fleet_maintenance(&state.db, layout.today)
                .await?
                .iter()
                .filter(|m| m.machine.active && m.status.level == MaintenanceLevel::Overdue)
                .count() as i64;
            Ok::<_, sqlx::Error>((new_requests, service_alerts))
        };
        let (new_requests, service_alerts) = counts.await.map_err(|e| AppError::from(e).into_response())?;
        Ok(AdminPage {
            layout,
            admin,
            heading: String::new(),
            nav: AdminNav::Dashboard,
            new_requests,
            service_alerts,
        })
    }
}

// ── Placeholder ─────────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "admin/placeholder.html")]
struct AdminPlaceholder {
    admin: AdminPage,
}

/// An admin page that only says „Страницата се изгражда“ (until its feature is built).
pub fn placeholder(admin: AdminPage, heading: &str, nav: AdminNav) -> Result<Response, AppError> {
    render(&AdminPlaceholder { admin: admin.titled(heading, nav) })
}
