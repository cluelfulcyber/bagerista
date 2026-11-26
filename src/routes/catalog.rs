//! The catalogue with filters (`/katalog`), full page or htmx fragment.

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;

use crate::errors::AppError;
use crate::routes::page::{Layout, Nav};
use crate::routes::placeholder;
use crate::state::AppState;

/// `/katalog?kategoria=&max_teglo=&min_dalbochina=&max_cena=&sort=&strana=`.
/// Strings, not numbers: an empty field (`max_teglo=`) must mean „без значение“,
/// not a 400 error; the handler parses them.
#[derive(Debug, Default, Deserialize)]
pub struct CatalogQuery {
    pub kategoria: Option<String>,
    pub max_teglo: Option<String>,
    pub min_dalbochina: Option<String>,
    pub max_cena: Option<String>,
    pub sort: Option<String>,
    pub strana: Option<String>,
}

/// `GET /katalog` – `public/catalog.html`; with `HX-Request` only
/// `partials/machine_grid.html` (see `page::is_htmx`).
pub async fn index(
    State(_state): State<AppState>,
    layout: Layout,
    headers: HeaderMap,
    Query(_query): Query<CatalogQuery>,
) -> Result<Response, AppError> {
    // TODO(feature: catalog): filters + cards (db::catalog::list_machines, views::with_availability),
    // fragment for htmx when page::is_htmx(&headers)
    let _ = headers;
    placeholder(layout.titled("Каталог", Nav::Catalog), "Каталог")
}
