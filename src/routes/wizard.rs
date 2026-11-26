//! „Кой багер ми трябва?“ – four questions, recommendations with reasons.

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;

use crate::errors::AppError;
use crate::routes::page::{Layout, Nav};
use crate::routes::placeholder;
use crate::state::AppState;

/// `?rabota=&dalbochina=&dostap=&teren=` (param values in services::wizard).
/// Also accepted by `/pomoshtnik` to pre-fill the questions.
#[derive(Debug, Default, Deserialize)]
pub struct WizardQuery {
    pub rabota: Option<String>,
    pub dalbochina: Option<String>,
    pub dostap: Option<String>,
    pub teren: Option<String>,
}

/// `GET /pomoshtnik` – `public/wizard.html`.
pub async fn form(layout: Layout, Query(_query): Query<WizardQuery>) -> Result<Response, AppError> {
    // TODO(feature: wizard): four fieldsets with radio tiles, pre-filled from the query
    placeholder(layout.titled("Кой багер ми трябва?", Nav::Wizard), "Кой багер ми трябва?")
}

/// `GET /pomoshtnik/rezultat` – `public/wizard.html` with results, or only
/// `partials/wizard_results.html` for htmx (variable `results: views::WizardResults`).
pub async fn result(
    State(_state): State<AppState>,
    layout: Layout,
    headers: HeaderMap,
    Query(_query): Query<WizardQuery>,
) -> Result<Response, AppError> {
    // TODO(feature: wizard): services::wizard::recommend / near_misses on db::catalog::wizard_facts
    let _ = headers;
    placeholder(layout.titled("Препоръка", Nav::Wizard), "Кой багер ми трябва?")
}
