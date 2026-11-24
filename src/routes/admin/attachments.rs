//! `/admin/prikachni` – attachments (хидравличен чук, кофи…): list, create, edit.

use axum::Form;
use axum::extract::State;
use axum::response::Response;
use serde::Deserialize;
use tower_sessions::Session;

use crate::errors::AppError;
use crate::routes::admin::{AdminNav, AdminPage, placeholder};
use crate::routes::page::PathOr404;
use crate::state::AppState;

/// Fields of the attachment form.
#[derive(Debug, Default, Deserialize)]
pub struct AttachmentForm {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub description: String,
    /// `AttachmentKind::code()`: hammer, bucket, auger, ripper, grab, other.
    #[serde(default)]
    pub kind: String,
    /// Euros per day, „45“ or „45,00“.
    #[serde(default)]
    pub price: String,
    /// Checkbox: present = active.
    pub active: Option<String>,
}

/// `GET /admin/prikachni` – `admin/attachments.html`.
pub async fn index(admin: AdminPage, State(_state): State<AppState>) -> Result<Response, AppError> {
    // TODO(feature: admin-attachments): db::catalog::list_attachments, icons via Attachment::icon()
    placeholder(admin, "Приставки", AdminNav::Attachments)
}

/// `POST /admin/prikachni` – db::catalog::create_attachment, 303 → list.
pub async fn create(admin: AdminPage, State(_state): State<AppState>, _session: Session, Form(_form): Form<AttachmentForm>) -> Result<Response, AppError> {
    // TODO(feature: admin-attachments)
    placeholder(admin, "Приставки", AdminNav::Attachments)
}

/// `POST /admin/prikachni/{id}` – db::catalog::update_attachment, 303 → list.
pub async fn update(
    admin: AdminPage,
    State(_state): State<AppState>,
    _session: Session,
    PathOr404(_id): PathOr404<i64>,
    Form(_form): Form<AttachmentForm>,
) -> Result<Response, AppError> {
    // TODO(feature: admin-attachments)
    placeholder(admin, "Приставки", AdminNav::Attachments)
}
