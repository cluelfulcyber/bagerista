//! Machine page, its calendar fragment and the booking request.

use askama::Template;
use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use tower_sessions::Session;

use crate::auth;
use crate::db;
use crate::errors::{AppError, error_page};
use crate::routes::page::{Layout, Nav, Params, render};
use crate::routes::placeholder;
use crate::routes::views::CalendarMonthView;
use crate::services::availability;
use crate::state::AppState;

/// `?mesec=2026-10`
#[derive(Debug, Default, Deserialize)]
pub struct MonthQuery {
    pub mesec: Option<String>,
}

/// `GET /mashini/{slug}` (`?ot=&do=&operator=&prikachni=&km=` pre-fill the calculator;
/// `prikachni` repeats, hence `Params`).
pub async fn show(
    State(_state): State<AppState>,
    layout: Layout,
    Path(slug): Path<String>,
    Query(_params): Query<Params>,
) -> Result<Response, AppError> {
    // TODO(feature: machine): public/machine.html – plate, drawings, calendar (views::CalendarMonthView),
    // attachments, slip (views::PriceSlip), fit list, similar machines; body class has-pricebar
    placeholder(layout.titled(&slug, Nav::Catalog), "Машина")
}

/// The month grid alone (htmx swaps it when the visitor changes the month).
#[derive(Template)]
#[template(path = "partials/calendar_month.html")]
pub struct CalendarPartial {
    pub cal: CalendarMonthView,
}

/// `GET /mashini/{slug}/kalendar?mesec=2026-11` – `partials/calendar_month.html`.
pub async fn calendar(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<MonthQuery>,
) -> Result<Response, AppError> {
    let machine = db::catalog::machine_by_slug(&state.db, &slug).await?.ok_or(AppError::NotFound)?;
    let today = crate::today();
    let this_month = availability::first_of_month(today);
    // months before the current one cannot be booked, so they are not shown
    let month = query
        .mesec
        .as_deref()
        .and_then(availability::parse_month)
        .unwrap_or(this_month)
        .max(this_month);
    let blocking =
        db::bookings::blocking_spans(&state.db, machine.id, month, availability::last_of_month(month)).await?;
    let cal = CalendarMonthView::new(&machine.slug, month, today, &blocking, None);
    render(&CalendarPartial { cal })
}

/// `POST /mashini/{slug}/zayavka` – role C. A guest is sent to the login page
/// and comes back to the machine page with the form values in the query.
pub async fn request_booking(
    State(_state): State<AppState>,
    layout: Layout,
    _session: Session,
    Path(slug): Path<String>,
    Form(params): Form<Params>,
) -> Result<Response, AppError> {
    let Some(user) = layout.user.clone() else {
        let keep: Vec<String> = params
            .0
            .iter()
            .filter(|(k, _)| k != "csrf_token")
            .map(|(k, v)| format!("{k}={}", auth::encode_query_value(v)))
            .collect();
        let back = format!("/mashini/{slug}?{}", keep.join("&"));
        return Ok(Redirect::to(&format!("/vhod?sled={}", auth::encode_query_value(&back))).into_response());
    };
    if user.is_admin() {
        return Ok(error_page(StatusCode::FORBIDDEN, "Нямате достъп до тази страница.", auth::CLIENT_ONLY_MESSAGE));
    }
    // TODO(feature: machine): services::bookings::validate_request, pricing::quote with DB prices,
    // db::bookings::create_checked, flash „Заявката е изпратена.“, 303 → /moite-zayavki/{id}
    placeholder(layout.titled("Заявка", Nav::Catalog), "Заявка за наем")
}
