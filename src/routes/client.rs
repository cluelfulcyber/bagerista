//! „Моите заявки“: the client's requests, cancelling, the printable quote.
//! Ownership: load with `db::bookings::by_id_for_client` – someone else's id gives 404.

use axum::extract::State;
use axum::response::Response;
use tower_sessions::Session;

use crate::auth::{RequireClient, RequireUser};
use crate::errors::AppError;
use crate::routes::page::{Layout, Nav, PathOr404};
use crate::routes::placeholder;
use crate::state::AppState;

/// `GET /moite-zayavki` – `client/bookings.html`.
pub async fn bookings(RequireClient(_user): RequireClient, State(_state): State<AppState>, layout: Layout) -> Result<Response, AppError> {
    // TODO(feature: client): table of db::bookings::for_client with partials/status_badge.html,
    // empty state „Още нямате заявки“
    placeholder(layout.titled("Моите заявки", Nav::None), "Моите заявки")
}

/// `GET /moite-zayavki/{id}` – `client/booking_detail.html` (own bookings only).
pub async fn booking(
    RequireClient(_user): RequireClient,
    State(_state): State<AppState>,
    layout: Layout,
    PathOr404(_id): PathOr404<i64>,
) -> Result<Response, AppError> {
    // TODO(feature: client): booking detail with views::PriceSlip::from_booking
    placeholder(layout.titled("Заявка", Nav::None), "Заявка")
}

/// `POST /moite-zayavki/{id}/otkaz` – only own `new` requests
/// (db::bookings::cancel_by_client), flash, 303 → detail.
pub async fn cancel(
    RequireClient(_user): RequireClient,
    State(_state): State<AppState>,
    layout: Layout,
    _session: Session,
    PathOr404(_id): PathOr404<i64>,
) -> Result<Response, AppError> {
    // TODO(feature: client)
    placeholder(layout.titled("Заявка", Nav::None), "Отказ от заявка")
}

/// `GET /zayavki/{id}/oferta` – `print/quote.html`; the client who owns it or an admin.
pub async fn print_quote(
    RequireUser(_user): RequireUser,
    State(_state): State<AppState>,
    layout: Layout,
    PathOr404(_id): PathOr404<i64>,
) -> Result<Response, AppError> {
    // TODO(feature: client): A4 quote (print.css .doc), lines from views::PriceSlip::from_booking
    placeholder(layout.titled("Оферта", Nav::None), "Оферта за печат")
}
