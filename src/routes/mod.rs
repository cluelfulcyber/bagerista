//! WEB LAYER: every URL of the site in one readable list (architecture §7).
//!
//! A handler never writes SQL and never computes a price: it reads the input,
//! calls `db::*` and `services::*`, and renders a template. Role checks are
//! extractors in the handler's arguments (`RequireClient`, `RequireAdmin`,
//! `AdminPage`), so a handler cannot run without the right user.
//! CSRF is checked for every POST by the middleware in `lib.rs`.

pub mod account;
pub mod admin;
pub mod api;
pub mod assets;
pub mod catalog;
pub mod client;
pub mod machines;
pub mod page;
pub mod public;
pub mod views;
pub mod wizard;

use askama::Template;
use axum::Router;
use axum::response::Response;
use axum::routing::{get, post};

use crate::errors::{self, AppError};
use crate::state::AppState;
use page::{Layout, render};

pub fn router() -> Router<AppState> {
    Router::new()
        // ── 7.1 Public ──────────────────────────────────────────────────────
        .route("/", get(public::home))
        .route("/katalog", get(catalog::index))
        .route("/mashini/{slug}", get(machines::show))
        .route("/mashini/{slug}/kalendar", get(machines::calendar))
        .route("/ceni", get(public::prices))
        .route("/pomoshtnik", get(wizard::form))
        .route("/pomoshtnik/rezultat", get(wizard::result))
        .route("/za-nas", get(public::about))
        .route("/kontakti", get(public::contacts))
        // ── 7.2 Account ─────────────────────────────────────────────────────
        .route("/vhod", get(account::login_form).post(account::login))
        .route("/registratsia", get(account::register_form).post(account::register))
        .route("/izhod", post(account::logout))
        .route("/profil", get(account::profile).post(account::update_profile))
        // ── 7.3 Client ──────────────────────────────────────────────────────
        .route("/mashini/{slug}/zayavka", post(machines::request_booking))
        .route("/moite-zayavki", get(client::bookings))
        .route("/moite-zayavki/{id}", get(client::booking))
        .route("/moite-zayavki/{id}/otkaz", post(client::cancel))
        .route("/zayavki/{id}/oferta", get(client::print_quote))
        // ── 7.4 Admin (every handler takes `AdminPage` = admins only) ───────
        .route("/admin", get(admin::dashboard::show))
        .route("/admin/zayavki", get(admin::bookings::index))
        .route("/admin/zayavki/{id}", get(admin::bookings::show))
        .route("/admin/zayavki/{id}/odobri", post(admin::bookings::approve))
        .route("/admin/zayavki/{id}/otkazhi", post(admin::bookings::decline))
        .route("/admin/zayavki/{id}/start", post(admin::bookings::start))
        .route("/admin/zayavki/{id}/priklyuchi", post(admin::bookings::complete))
        .route("/admin/zayavki/{id}/belezhka", post(admin::bookings::save_note))
        .route("/admin/mashini", get(admin::machines::index).post(admin::machines::create))
        .route("/admin/mashini/nova", get(admin::machines::new_form))
        .route("/admin/mashini/{id}", get(admin::machines::edit_form).post(admin::machines::update))
        .route("/admin/mashini/{id}/aktivnost", post(admin::machines::toggle_active))
        .route("/admin/mashini/{id}/snimki", post(admin::machines::upload_photo))
        .route("/admin/mashini/{id}/snimki/{photo_id}/iztriy", post(admin::machines::delete_photo))
        .route("/admin/mashini/{id}/obsluzhvane", post(admin::machines::add_service_record))
        .route("/admin/prikachni", get(admin::attachments::index).post(admin::attachments::create))
        .route("/admin/prikachni/{id}", post(admin::attachments::update))
        .route("/admin/kalendar", get(admin::calendar::show))
        .route("/admin/obsluzhvane", get(admin::maintenance::index))
        .route("/admin/klienti", get(admin::clients::index))
        .route("/admin/klienti/{id}", get(admin::clients::show))
        // ── 7.5 JSON API ────────────────────────────────────────────────────
        .route("/api/mashini/{slug}/zaetost", get(api::availability))
        .route("/api/mashini/{slug}/cena", get(api::quote))
        // ── 7.6 Assets ──────────────────────────────────────────────────────
        .route("/static/{*path}", get(assets::static_file))
        .route("/media/snimki/{id}", get(assets::photo))
        .route("/favicon.ico", get(assets::favicon))
        .route("/robots.txt", get(assets::robots))
        .fallback(errors::not_found)
}

// ── Placeholder for pages that are not built yet ────────────────────────────

#[derive(Template)]
#[template(path = "placeholder.html")]
struct PlaceholderPage {
    layout: Layout,
    heading: String,
}

/// A page in the normal layout that only says „Страницата се изгражда“.
/// Handlers marked `TODO(feature: …)` use it until their template exists.
pub fn placeholder(layout: Layout, heading: &str) -> Result<Response, AppError> {
    render(&PlaceholderPage { layout, heading: heading.to_string() })
}
