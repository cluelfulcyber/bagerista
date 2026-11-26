//! Login, registration, logout and the client's profile (architecture §7.2).

use axum::Form;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use tower_sessions::Session;

use crate::auth::{self, RequireClient};
use crate::errors::AppError;
use crate::routes::page::{Flash, Layout, Nav, set_flash};
use crate::routes::placeholder;
use crate::state::AppState;

/// `/vhod?sled=/mashini/kubota-kx057-6` – where to go after logging in
/// (check it with `auth::safe_next_path`).
#[derive(Debug, Default, Deserialize)]
pub struct NextQuery {
    pub sled: Option<String>,
}

/// Fields of the login form (`id="f-email"`, `id="f-password"`).
#[derive(Debug, Default, Deserialize)]
pub struct LoginForm {
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub password: String,
    pub sled: Option<String>,
}

/// Fields of the registration form.
#[derive(Debug, Default, Deserialize)]
pub struct RegisterForm {
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub phone: String,
    pub company_name: Option<String>,
    pub company_eik: Option<String>,
    #[serde(default)]
    pub password: String,
    pub sled: Option<String>,
}

/// Fields of the profile form (password fields optional: empty = unchanged).
#[derive(Debug, Default, Deserialize)]
pub struct ProfileForm {
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub phone: String,
    pub company_name: Option<String>,
    pub company_eik: Option<String>,
    pub current_password: Option<String>,
    pub new_password: Option<String>,
}

/// `GET /vhod` – `account/login.html`.
pub async fn login_form(layout: Layout, Query(_query): Query<NextQuery>) -> Result<Response, AppError> {
    // TODO(feature: account): login form with hidden csrf_token and sled
    placeholder(layout.titled("Вход", Nav::None), "Вход")
}

/// `POST /vhod` – `auth::authenticate` (rate-limited), `auth::log_in`, then
/// 303 to `sled`, `/moite-zayavki` (client) or `/admin` (admin); 422 with
/// „Грешен имейл или парола.“, 429 when blocked.
pub async fn login(
    State(_state): State<AppState>,
    layout: Layout,
    _session: Session,
    Form(_form): Form<LoginForm>,
) -> Result<Response, AppError> {
    // TODO(feature: account)
    placeholder(layout.titled("Вход", Nav::None), "Вход")
}

/// `GET /registratsia` – `account/register.html`.
pub async fn register_form(layout: Layout, Query(_query): Query<NextQuery>) -> Result<Response, AppError> {
    // TODO(feature: account): registration form (design: components.html, form with errors)
    placeholder(layout.titled("Регистрация", Nav::None), "Регистрация")
}

/// `POST /registratsia` – validate (auth::valid_*), create, log in, 303 → `/moite-zayavki`.
pub async fn register(
    State(_state): State<AppState>,
    layout: Layout,
    _session: Session,
    Form(_form): Form<RegisterForm>,
) -> Result<Response, AppError> {
    // TODO(feature: account)
    placeholder(layout.titled("Регистрация", Nav::None), "Регистрация")
}

/// `POST /izhod` – forget the session, back to the home page with a short message.
pub async fn logout(session: Session) -> Result<Response, AppError> {
    auth::log_out(&session).await?;
    set_flash(&session, Flash::info("", "Излязохте от профила си.")).await?;
    Ok(Redirect::to("/").into_response())
}

/// `GET /profil` – `account/profile.html` (clients only).
pub async fn profile(RequireClient(_user): RequireClient, layout: Layout) -> Result<Response, AppError> {
    // TODO(feature: account): profile form pre-filled with the user's data
    placeholder(layout.titled("Профил", Nav::None), "Профил")
}

/// `POST /profil` – save, flash „Данните са запазени.“, 303 → `/profil`.
pub async fn update_profile(
    RequireClient(_user): RequireClient,
    State(_state): State<AppState>,
    layout: Layout,
    _session: Session,
    Form(_form): Form<ProfileForm>,
) -> Result<Response, AppError> {
    // TODO(feature: account)
    placeholder(layout.titled("Профил", Nav::None), "Профил")
}
