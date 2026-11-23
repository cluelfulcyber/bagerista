//! One error type for the whole application.
//!
//! Handlers return `Result<impl IntoResponse, AppError>` and use `?`; the
//! conversion below turns every error into a Bulgarian error page with the
//! right HTTP status. Technical details (SQL errors…) are only logged.

use askama::Template;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

use crate::routes::page::{Layout, Nav};

#[derive(Debug)]
pub enum AppError {
    /// 404 – the page or record does not exist (or belongs to someone else).
    NotFound,
    /// 403 – logged in, but not allowed (e.g. a client opening /admin).
    Forbidden,
    /// 400 – malformed request; the message is already in Bulgarian.
    BadRequest(String),
    /// 409 – the action clashes with the current state (e.g. double booking).
    Conflict(String),
    /// 429 – too many failed logins.
    TooManyRequests,
    /// 500 – database error.
    Db(sqlx::Error),
    /// 500 – template rendering error.
    Template(askama::Error),
    /// 500 – anything else that is our fault.
    Internal(String),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            // `fetch_one` found nothing → the record does not exist.
            sqlx::Error::RowNotFound => AppError::NotFound,
            other => AppError::Db(other),
        }
    }
}

impl From<askama::Error> for AppError {
    fn from(err: askama::Error) -> Self {
        AppError::Template(err)
    }
}

impl From<tower_sessions::session::Error> for AppError {
    fn from(err: tower_sessions::session::Error) -> Self {
        AppError::Internal(format!("session: {err}"))
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound => write!(f, "не е намерено"),
            AppError::Forbidden => write!(f, "нямате достъп"),
            AppError::BadRequest(msg) | AppError::Conflict(msg) => write!(f, "{msg}"),
            AppError::TooManyRequests => write!(f, "твърде много опити"),
            AppError::Db(e) => write!(f, "база данни: {e}"),
            AppError::Template(e) => write!(f, "шаблон: {e}"),
            AppError::Internal(msg) => write!(f, "вътрешна грешка: {msg}"),
        }
    }
}

impl std::error::Error for AppError {}

/// Texts of the CSRF and role-guard 403 pages (also used by `csrf.rs`).
pub const CSRF_MESSAGE: &str =
    "Сесията е изтекла или формата е невалидна. Презаредете страницата и опитайте отново.";
pub const ADMIN_ONLY_MESSAGE: &str = "Тази страница е достъпна само за администратори.";
pub const LOGIN_LIMIT_MESSAGE: &str =
    "Твърде много неуспешни опита. Опитайте отново след 15 минути.";

impl AppError {
    pub fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Forbidden => StatusCode::FORBIDDEN,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            AppError::Db(_) | AppError::Template(_) | AppError::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    /// Bulgarian title and message shown to the visitor.
    pub fn title_and_message(&self) -> (&'static str, String) {
        match self {
            AppError::NotFound => (
                "Страницата не е намерена",
                "Изкопахме дотук и не я открихме. Адресът може да е сгрешен или машината вече не е в каталога.".to_string(),
            ),
            AppError::Forbidden => ("Нямате достъп до тази страница.", ADMIN_ONLY_MESSAGE.to_string()),
            AppError::BadRequest(msg) => ("Невалидна заявка", msg.clone()),
            AppError::Conflict(msg) => ("Конфликт", msg.clone()),
            AppError::TooManyRequests => ("Твърде много опити", LOGIN_LIMIT_MESSAGE.to_string()),
            AppError::Db(_) | AppError::Template(_) | AppError::Internal(_) => (
                "Грешка в сървъра",
                "Заявката ви не е записана. Опитайте отново след минута или се обадете на 032 64 12 80, приемаме и по телефона.".to_string(),
            ),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        if status.is_server_error() {
            // Details go to the log only, never to the visitor.
            tracing::error!("{self}");
        }
        let (title, message) = self.title_and_message();
        error_page(status, title, &message)
    }
}

/// The error page (`templates/errors/error.html`): the normal site layout with
/// the code as a giant numeral, a title, a message and two links.
#[derive(Template)]
#[template(path = "errors/error.html")]
pub struct ErrorPage {
    pub layout: Layout,
    pub code: u16,
    pub title: String,
    pub message: String,
}

impl ErrorPage {
    /// 5xx pages offer „Опитай отново“ instead of „Към каталога“.
    pub fn is_server_error(&self) -> bool {
        self.code >= 500
    }
}

/// Attached to every error response, so the htmx middleware in `lib.rs` can
/// replace the whole page with a small alert when htmx asked for a fragment.
#[derive(Debug, Clone)]
pub struct ErrorInfo {
    pub title: String,
    pub message: String,
}

/// Renders the Bulgarian error page with a guest header (used where there is
/// no request context, e.g. inside middleware or `AppError::into_response`).
pub fn error_page(status: StatusCode, title: &str, message: &str) -> Response {
    error_page_with(Layout::guest(), status, title, message)
}

/// Renders the error page inside the given layout (user-aware header).
pub fn error_page_with(layout: Layout, status: StatusCode, title: &str, message: &str) -> Response {
    let page = ErrorPage {
        layout: layout.titled(title.trim_end_matches('.'), Nav::None),
        code: status.as_u16(),
        title: title.to_string(),
        message: message.to_string(),
    };
    let mut response = match page.render() {
        Ok(html) => (status, Html(html)).into_response(),
        // The template itself failed: fall back to plain text, never panic.
        Err(err) => {
            tracing::error!("error page template: {err}");
            (status, format!("{title}\n{message}")).into_response()
        }
    };
    response.extensions_mut().insert(ErrorInfo { title: title.to_string(), message: message.to_string() });
    response
}

/// Fallback handler for unknown URLs → Bulgarian 404 page with the visitor's header.
pub async fn not_found(layout: Layout) -> Response {
    let (title, message) = AppError::NotFound.title_and_message();
    error_page_with(layout, StatusCode::NOT_FOUND, title, &message)
}
