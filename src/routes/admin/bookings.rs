//! `/admin/zayavki` – requests: list, detail with conflicts and the
//! maintenance warning, and the status changes (approve, decline, start,
//! complete) plus the admin note. Every change answers 303 → detail with a flash.

use axum::Form;
use axum::extract::{Query, State};
use axum::response::Response;
use serde::Deserialize;
use tower_sessions::Session;

use crate::errors::AppError;
use crate::routes::admin::{AdminNav, AdminPage, placeholder};
use crate::routes::page::PathOr404;
use crate::state::AppState;

/// `?status=new&mashina=5&strana=2`
#[derive(Debug, Default, Deserialize)]
pub struct BookingsQuery {
    pub status: Option<String>,
    pub mashina: Option<String>,
    pub strana: Option<String>,
}

/// `belezhka` (admin note) – used by decline and save_note.
#[derive(Debug, Default, Deserialize)]
pub struct NoteForm {
    pub admin_note: Option<String>,
}

/// `hours_at_end` – „Моточасове при връщане“.
#[derive(Debug, Default, Deserialize)]
pub struct CompleteForm {
    #[serde(default)]
    pub hours_at_end: String,
}

/// `GET /admin/zayavki` – `admin/bookings.html`.
pub async fn index(admin: AdminPage, State(_state): State<AppState>, Query(_query): Query<BookingsQuery>) -> Result<Response, AppError> {
    // TODO(feature: admin-bookings): db::bookings::admin_list with filters, views::Pagination
    placeholder(admin, "Заявки", AdminNav::Bookings)
}

/// `GET /admin/zayavki/{id}` – `admin/booking_detail.html` (conflicts + maintenance warning).
pub async fn show(admin: AdminPage, State(_state): State<AppState>, PathOr404(_id): PathOr404<i64>) -> Result<Response, AppError> {
    // TODO(feature: admin-bookings)
    placeholder(admin, "Заявка", AdminNav::Bookings)
}

/// `POST /admin/zayavki/{id}/odobri` – db::bookings::approve_checked (BEGIN IMMEDIATE).
pub async fn approve(admin: AdminPage, State(_state): State<AppState>, _session: Session, PathOr404(_id): PathOr404<i64>) -> Result<Response, AppError> {
    // TODO(feature: admin-bookings)
    placeholder(admin, "Заявка", AdminNav::Bookings)
}

/// `POST /admin/zayavki/{id}/otkazhi` – new/approved → declined.
pub async fn decline(
    admin: AdminPage,
    State(_state): State<AppState>,
    _session: Session,
    PathOr404(_id): PathOr404<i64>,
    Form(_form): Form<NoteForm>,
) -> Result<Response, AppError> {
    // TODO(feature: admin-bookings)
    placeholder(admin, "Заявка", AdminNav::Bookings)
}

/// `POST /admin/zayavki/{id}/start` – approved → active (hours_at_start saved).
pub async fn start(admin: AdminPage, State(_state): State<AppState>, _session: Session, PathOr404(_id): PathOr404<i64>) -> Result<Response, AppError> {
    // TODO(feature: admin-bookings)
    placeholder(admin, "Заявка", AdminNav::Bookings)
}

/// `POST /admin/zayavki/{id}/priklyuchi` – active → completed with the engine hours.
pub async fn complete(
    admin: AdminPage,
    State(_state): State<AppState>,
    _session: Session,
    PathOr404(_id): PathOr404<i64>,
    Form(_form): Form<CompleteForm>,
) -> Result<Response, AppError> {
    // TODO(feature: admin-bookings): services::bookings::validate_hours_at_end, db::bookings::complete
    placeholder(admin, "Заявка", AdminNav::Bookings)
}

/// `POST /admin/zayavki/{id}/belezhka` – db::bookings::save_admin_note.
pub async fn save_note(
    admin: AdminPage,
    State(_state): State<AppState>,
    _session: Session,
    PathOr404(_id): PathOr404<i64>,
    Form(_form): Form<NoteForm>,
) -> Result<Response, AppError> {
    // TODO(feature: admin-bookings)
    placeholder(admin, "Заявка", AdminNav::Bookings)
}
