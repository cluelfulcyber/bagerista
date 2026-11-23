//! What every page shares: the layout context for `base.html`, flash
//! messages, form errors and a few small request helpers.
//!
//! ## How a page is built (the pattern every handler follows)
//!
//! ```text
//! #[derive(Template)]
//! #[template(path = "public/prices.html")]   // {% extends "base.html" %}
//! struct PricesPage {
//!     layout: Layout,            // ← always this name: base.html reads `layout.*`
//!     rows: Vec<Machine>,        // ← whatever the page itself needs
//! }
//!
//! pub async fn prices(State(state): State<AppState>, layout: Layout) -> Result<Response, AppError> {
//!     let rows = db::catalog::list_machines(&state.db, &CatalogFilter::default()).await?;
//!     let page = PricesPage { layout: layout.titled("Цени и условия", Nav::Prices), rows };
//!     render(&page)
//! }
//! ```
//!
//! 1. `layout: Layout` is an extractor: before the handler runs it loads the
//!    logged-in user (header links), the session's CSRF token (forms) and
//!    takes the one-time flash message out of the session.
//! 2. `titled(…)` sets the `<title>` („Цени и условия: Багериста“) and which
//!    menu item is marked as current.
//! 3. `render(&page)` turns the template into an HTML response.
//!
//! After a successful POST a handler stores a flash and redirects (303):
//! `set_flash(&session, Flash::ok("Заявката е изпратена.", "…")).await?;`
//! `Ok(Redirect::to("/moite-zayavki/12").into_response())`.
//! A form with mistakes is shown again with `render_status(422, &page)`
//! and a `FormErrors` value (see `partials/field_error.html`).
//!
//! Admin pages use `admin::AdminPage` instead; it wraps a `Layout` the same way.

use askama::Template;
use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use chrono::NaiveDate;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

use crate::auth::{self, CurrentUser};
use crate::errors::AppError;
use crate::models::User;
use crate::services::bookings::FieldError;
use crate::state::AppState;
use crate::{csrf, format};

/// Session key of the one-time message shown after a redirect.
pub const FLASH_KEY: &str = "flash";
/// Title of the home page (all other pages are „{Страница}: Багериста“).
pub const HOME_TITLE: &str = "Багериста: наем на багери в Пловдив";
const DEFAULT_DESCRIPTION: &str = "Мини багери, мидибагери, верижни и колесни багери и багер-товарачи \
под наем в Пловдив и до 150 км около него. Размери, свободни дни и цена с ДДС за всяка машина.";

// ── Layout ──────────────────────────────────────────────────────────────────

/// Item of the main menu marked with `aria-current="page"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    None,
    Catalog,
    Wizard,
    Prices,
    About,
    Contacts,
}

impl Nav {
    /// Short key used in base.html: `layout.nav_is("katalog")`.
    pub fn key(self) -> &'static str {
        match self {
            Nav::None => "",
            Nav::Catalog => "katalog",
            Nav::Wizard => "pomoshtnik",
            Nav::Prices => "ceni",
            Nav::About => "za-nas",
            Nav::Contacts => "kontakti",
        }
    }
}

/// Everything `base.html` needs (header, flash, footer, `<head>`).
#[derive(Debug, Clone)]
pub struct Layout {
    /// Full text of `<title>`.
    pub title: String,
    /// `<meta name="description">`.
    pub description: String,
    pub nav: Nav,
    /// `None` for a guest.
    pub user: Option<User>,
    /// The session's CSRF token: every POST form renders it as a hidden field.
    pub csrf_token: String,
    /// Message left by the previous POST, shown once.
    pub flash: Option<Flash>,
    pub today: NaiveDate,
    /// Path and query of this request (for „Вход“ → back here after login).
    pub path: String,
}

impl Layout {
    /// A layout without a request: guest header, no CSRF token. Used by error
    /// pages produced outside a handler (e.g. the CSRF middleware).
    pub fn guest() -> Layout {
        Layout {
            title: HOME_TITLE.to_string(),
            description: DEFAULT_DESCRIPTION.to_string(),
            nav: Nav::None,
            user: None,
            csrf_token: String::new(),
            flash: None,
            today: crate::today(),
            path: "/".to_string(),
        }
    }

    /// Names the page: `<title>` „{title}: Багериста“ and the current menu item.
    pub fn titled(mut self, title: &str, nav: Nav) -> Layout {
        self.title = format!("{title}: Багериста");
        self.nav = nav;
        self
    }

    /// Replaces the default meta description.
    pub fn described(mut self, description: &str) -> Layout {
        self.description = description.to_string();
        self
    }

    pub fn nav_is(&self, key: &str) -> bool {
        self.nav.key() == key
    }

    pub fn is_client(&self) -> bool {
        self.user.as_ref().is_some_and(|u| !u.is_admin())
    }

    pub fn is_admin(&self) -> bool {
        self.user.as_ref().is_some_and(|u| u.is_admin())
    }

    /// „Вход“ link that brings the visitor back to this page after logging in.
    pub fn login_href(&self) -> String {
        let plain = ["/", "/vhod", "/registratsia"];
        if plain.iter().any(|p| self.path == *p || self.path.starts_with("/vhod?")) {
            "/vhod".to_string()
        } else {
            format!("/vhod?sled={}", auth::encode_query_value(&self.path))
        }
    }

    /// „08.10.2026“ in the footer title block.
    pub fn today_num(&self) -> String {
        format::date_num(self.today)
    }
}

impl FromRequestParts<AppState> for Layout {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::Internal("session layer missing".to_string()))?;
        let csrf_token = csrf::token(&session).await?;
        // `remove` = read once: the message disappears after this page.
        let flash = session.remove::<Flash>(FLASH_KEY).await?;
        let path = parts.uri.path_and_query().map(|pq| pq.as_str().to_string()).unwrap_or_default();
        Ok(Layout {
            user,
            csrf_token,
            flash,
            path,
            ..Layout::guest()
        })
    }
}

// ── Flash messages ──────────────────────────────────────────────────────────

/// Look of a flash message (design §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlashKind {
    Ok,
    Info,
    Warn,
    Error,
}

/// A one-time message stored in the session: „Заявката е изпратена.“ + what happens next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flash {
    pub kind: FlashKind,
    /// Bold first sentence (may be empty).
    pub title: String,
    pub text: String,
}

impl Flash {
    pub fn new(kind: FlashKind, title: &str, text: &str) -> Flash {
        Flash { kind, title: title.to_string(), text: text.to_string() }
    }
    pub fn ok(title: &str, text: &str) -> Flash {
        Flash::new(FlashKind::Ok, title, text)
    }
    pub fn info(title: &str, text: &str) -> Flash {
        Flash::new(FlashKind::Info, title, text)
    }
    pub fn warn(title: &str, text: &str) -> Flash {
        Flash::new(FlashKind::Warn, title, text)
    }
    pub fn error(title: &str, text: &str) -> Flash {
        Flash::new(FlashKind::Error, title, text)
    }

    /// CSS modifier: `flash--ok`.
    pub fn css_class(&self) -> &'static str {
        match self.kind {
            FlashKind::Ok => "flash--ok",
            FlashKind::Info => "flash--info",
            FlashKind::Warn => "flash--warn",
            FlashKind::Error => "flash--error",
        }
    }

    /// Errors interrupt the screen reader (`alert`), the rest is polite (`status`).
    pub fn role(&self) -> &'static str {
        if self.kind == FlashKind::Error { "alert" } else { "status" }
    }

    pub fn is(&self, kind: &str) -> bool {
        matches!(
            (self.kind, kind),
            (FlashKind::Ok, "ok") | (FlashKind::Info, "info") | (FlashKind::Warn, "warn") | (FlashKind::Error, "error")
        )
    }
}

/// A flash message on its own (htmx error fragments, see `lib.rs`).
#[derive(Template)]
#[template(path = "partials/flash.html")]
pub struct FlashPartial {
    pub flash: Flash,
}

/// Leaves a message for the next page (call it right before a redirect).
pub async fn set_flash(session: &Session, flash: Flash) -> Result<(), AppError> {
    session.insert(FLASH_KEY, flash).await?;
    Ok(())
}

// ── Form errors ─────────────────────────────────────────────────────────────

/// Validation messages of one form, shown next to the fields after a 422.
/// Convention: the input of field `email` has `id="f-email"`, so the error
/// summary can link to it and the message gets the id `f-email-err`.
#[derive(Debug, Clone, Default)]
pub struct FormErrors(pub Vec<FieldError>);

impl FormErrors {
    pub fn new() -> FormErrors {
        FormErrors(Vec::new())
    }

    pub fn add(&mut self, field: &'static str, message: &str) {
        self.0.push(FieldError { field, message: message.to_string() });
    }

    /// The message for one field (the first one if there are several).
    pub fn get(&self, field: &str) -> Option<&str> {
        self.0.iter().find(|e| e.field == field).map(|e| e.message.as_str())
    }

    pub fn has(&self, field: &str) -> bool {
        self.get(field).is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Heading of the error summary: „Проверете 1 поле“ / „Проверете 2 полета“.
    pub fn summary_title(&self) -> String {
        let n = self.0.len() as i64;
        format!("Проверете {n} {}", format::plural(n, "поле", "полета"))
    }
}

// ── Rendering and request helpers ───────────────────────────────────────────

/// Renders a template as a `200 OK` HTML page.
pub fn render<T: Template>(page: &T) -> Result<Response, AppError> {
    Ok(Html(page.render()?).into_response())
}

/// Renders a template with another status, e.g. 422 for a form with errors.
pub fn render_status<T: Template>(status: StatusCode, page: &T) -> Result<Response, AppError> {
    Ok((status, Html(page.render()?)).into_response())
}

/// True when htmx sent the request: answer with a fragment, not a whole page.
pub fn is_htmx(headers: &HeaderMap) -> bool {
    headers.get("hx-request").is_some_and(|v| v == "true")
}

/// Query string or form as a list of pairs. Needed where one name repeats
/// (checkboxes: `prikachni=1&prikachni=3`), which a struct cannot hold.
/// Use as `Query(params): Query<Params>` or `Form(params): Form<Params>`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(transparent)]
pub struct Params(pub Vec<(String, String)>);

impl Params {
    /// First value of `key`, trimmed; empty values count as missing.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.trim()).filter(|v| !v.is_empty())
    }

    /// All values of `key` (checkbox groups).
    pub fn all(&self, key: &str) -> Vec<&str> {
        self.0.iter().filter(|(k, _)| k == key).map(|(_, v)| v.trim()).collect()
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
}

/// Like axum's `Path`, but a malformed id (`/admin/zayavki/abc`) gives our
/// Bulgarian 404 page instead of axum's English „400 Invalid URL“.
#[derive(Debug, Clone)]
pub struct PathOr404<T>(pub T);

impl<T, S> FromRequestParts<S> for PathOr404<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Path::<T>::from_request_parts(parts, state).await {
            Ok(Path(value)) => Ok(PathOr404(value)),
            Err(_) => Err(AppError::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_follow_the_pattern() {
        let layout = Layout::guest().titled("Каталог", Nav::Catalog);
        assert_eq!(layout.title, "Каталог: Багериста");
        assert!(layout.nav_is("katalog"));
        assert!(!layout.nav_is("ceni"));
        assert_eq!(Layout::guest().title, HOME_TITLE);
    }

    #[test]
    fn login_link_returns_to_the_page() {
        let mut layout = Layout::guest();
        assert_eq!(layout.login_href(), "/vhod");
        layout.path = "/mashini/kubota-kx057-6?ot=2026-10-09".to_string();
        assert_eq!(layout.login_href(), "/vhod?sled=/mashini/kubota-kx057-6%3Fot%3D2026-10-09");
    }

    #[test]
    fn form_errors_summary() {
        let mut errors = FormErrors::new();
        assert!(errors.is_empty());
        errors.add("email", auth::EMAIL_MESSAGE);
        assert_eq!(errors.summary_title(), "Проверете 1 поле");
        errors.add("phone", auth::PHONE_MESSAGE);
        assert_eq!(errors.summary_title(), "Проверете 2 полета");
        assert_eq!(errors.get("email"), Some(auth::EMAIL_MESSAGE));
        assert!(!errors.has("password"));
    }

    #[test]
    fn params_keep_repeated_names() {
        let p = Params(vec![
            ("prikachni".into(), "1".into()),
            ("km".into(), " 20 ".into()),
            ("prikachni".into(), "3".into()),
            ("adres".into(), "".into()),
        ]);
        assert_eq!(p.all("prikachni"), vec!["1", "3"]);
        assert_eq!(p.get("km"), Some("20"));
        assert_eq!(p.get("adres"), None);
    }
}
