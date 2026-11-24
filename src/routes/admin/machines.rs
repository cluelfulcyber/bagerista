//! `/admin/mashini` – machines: list, create/edit form (with compatible
//! attachments, photos and service history), activate/deactivate, photos,
//! service records.

use axum::Form;
use axum::extract::{Multipart, State};
use axum::response::Response;
use serde::Deserialize;
use tower_sessions::Session;

use crate::errors::AppError;
use crate::routes::admin::{AdminNav, AdminPage, placeholder};
use crate::routes::page::{Params, PathOr404};
use crate::state::AppState;

/// Fields of the service-record form on the machine edit page.
#[derive(Debug, Default, Deserialize)]
pub struct ServiceRecordForm {
    #[serde(default)]
    pub service_date: String,
    #[serde(default)]
    pub engine_hours: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    /// In euros, „280,00“ or „280“.
    #[serde(default)]
    pub cost: String,
}

/// `GET /admin/mashini` – `admin/machines.html` (active and inactive).
pub async fn index(admin: AdminPage, State(_state): State<AppState>) -> Result<Response, AppError> {
    // TODO(feature: admin-machines): db::catalog::list_machines with include_inactive
    placeholder(admin, "Машини", AdminNav::Machines)
}

/// `GET /admin/mashini/nova` – empty `admin/machine_form.html`.
pub async fn new_form(admin: AdminPage, State(_state): State<AppState>) -> Result<Response, AppError> {
    // TODO(feature: admin-machines)
    placeholder(admin, "Нова машина", AdminNav::Machines)
}

/// `POST /admin/mashini` – create (the form has attachment checkboxes → `Params`).
pub async fn create(admin: AdminPage, State(_state): State<AppState>, _session: Session, Form(_form): Form<Params>) -> Result<Response, AppError> {
    // TODO(feature: admin-machines): validate, db::catalog::create_machine, 303 → edit page
    placeholder(admin, "Нова машина", AdminNav::Machines)
}

/// `GET /admin/mashini/{id}` – `admin/machine_form.html` with photos and service history.
pub async fn edit_form(admin: AdminPage, State(_state): State<AppState>, PathOr404(_id): PathOr404<i64>) -> Result<Response, AppError> {
    // TODO(feature: admin-machines)
    placeholder(admin, "Машина", AdminNav::Machines)
}

/// `POST /admin/mashini/{id}` – update incl. compatible attachments.
pub async fn update(
    admin: AdminPage,
    State(_state): State<AppState>,
    _session: Session,
    PathOr404(_id): PathOr404<i64>,
    Form(_form): Form<Params>,
) -> Result<Response, AppError> {
    // TODO(feature: admin-machines): db::catalog::update_machine + set_machine_attachments
    placeholder(admin, "Машина", AdminNav::Machines)
}

/// `POST /admin/mashini/{id}/aktivnost` – db::catalog::toggle_active, 303 → list.
pub async fn toggle_active(admin: AdminPage, State(_state): State<AppState>, _session: Session, PathOr404(_id): PathOr404<i64>) -> Result<Response, AppError> {
    // TODO(feature: admin-machines)
    placeholder(admin, "Машини", AdminNav::Machines)
}

/// `POST /admin/mashini/{id}/snimki?csrf_token=…` – multipart upload, ≤ 2 MB,
/// jpeg/png/webp checked by magic bytes (CSRF token in the query, see csrf.rs).
pub async fn upload_photo(
    admin: AdminPage,
    State(_state): State<AppState>,
    _session: Session,
    PathOr404(_id): PathOr404<i64>,
    _multipart: Multipart,
) -> Result<Response, AppError> {
    // TODO(feature: admin-machines): db::catalog::add_photo
    placeholder(admin, "Машина", AdminNav::Machines)
}

/// `POST /admin/mashini/{id}/snimki/{photo_id}/iztriy`
pub async fn delete_photo(
    admin: AdminPage,
    State(_state): State<AppState>,
    _session: Session,
    PathOr404((_id, _photo_id)): PathOr404<(i64, i64)>,
) -> Result<Response, AppError> {
    // TODO(feature: admin-machines): db::catalog::delete_photo
    placeholder(admin, "Машина", AdminNav::Machines)
}

/// `POST /admin/mashini/{id}/obsluzhvane` – db::service::add, 303 → edit page.
pub async fn add_service_record(
    admin: AdminPage,
    State(_state): State<AppState>,
    _session: Session,
    PathOr404(_id): PathOr404<i64>,
    Form(_form): Form<ServiceRecordForm>,
) -> Result<Response, AppError> {
    // TODO(feature: admin-machines)
    placeholder(admin, "Машина", AdminNav::Machines)
}
