//! Authentication: password hashing, login/logout, the role-guard extractors
//! and the login rate limiter (architecture §8).
//!
//! Using an extractor as a handler argument makes the check impossible to
//! forget: a handler with a `RequireAdmin` parameter simply cannot run
//! without a logged-in administrator.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Redirect, Response};
use sqlx::SqlitePool;
use tower_sessions::Session;

use crate::db;
use crate::errors::AppError;
use crate::models::{Role, User};
use crate::state::AppState;

/// Session key holding the logged-in user's id.
pub const USER_ID_KEY: &str = "user_id";
/// The only login error message: it never says whether the e-mail exists.
pub const WRONG_CREDENTIALS: &str = "Грешен имейл или парола.";
/// 403 text when an administrator opens a page meant for clients.
pub const CLIENT_ONLY_MESSAGE: &str =
    "Тази страница е за клиенти. Заявките се управляват от администрацията.";

// ── Passwords ───────────────────────────────────────────────────────────────

/// Hashes a password with Argon2id and a random salt; returns a PHC string
/// like `$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>` (safe to store).
pub fn hash_password(password: &str) -> Result<String, AppError> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|e| AppError::Internal(format!("argon2: {e}")))
}

/// Checks a password against a stored PHC string.
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

/// A hash of a throw-away password, used when the e-mail is unknown so that
/// the response takes as long as for a real account.
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash_password("не-съществуващ-профил-1").unwrap_or_default())
}

// ── Login rate limiting ─────────────────────────────────────────────────────

/// Failed logins allowed per e-mail within `LOGIN_WINDOW`.
pub const MAX_FAILED_LOGINS: usize = 5;
pub const LOGIN_WINDOW: Duration = Duration::from_secs(15 * 60);

/// Remembers failed login times per e-mail (in memory, enough for one server).
#[derive(Debug, Default)]
pub struct LoginLimiter {
    failures: Mutex<HashMap<String, Vec<Instant>>>,
}

impl LoginLimiter {
    /// True when the e-mail already has 5 failures in the last 15 minutes.
    pub fn is_blocked(&self, email: &str) -> bool {
        let mut map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        let key = email.trim().to_lowercase();
        match map.get_mut(&key) {
            Some(times) => {
                times.retain(|t| t.elapsed() < LOGIN_WINDOW);
                times.len() >= MAX_FAILED_LOGINS
            }
            None => false,
        }
    }

    pub fn record_failure(&self, email: &str) {
        let mut map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(email.trim().to_lowercase()).or_default().push(Instant::now());
    }

    /// A successful login forgets earlier failures.
    pub fn clear(&self, email: &str) {
        let mut map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&email.trim().to_lowercase());
    }
}

#[derive(Debug)]
pub enum LoginError {
    /// „Грешен имейл или парола.“ (form re-rendered with 422)
    WrongCredentials,
    /// More than 5 failures in 15 minutes → 429.
    TooManyAttempts,
    Db(sqlx::Error),
}

impl LoginError {
    pub fn message(&self) -> &'static str {
        match self {
            LoginError::WrongCredentials => WRONG_CREDENTIALS,
            LoginError::TooManyAttempts => crate::errors::LOGIN_LIMIT_MESSAGE,
            LoginError::Db(_) => "Възникна грешка. Опитайте отново.",
        }
    }
}

/// Checks e-mail and password (used by `POST /vhod`).
pub async fn authenticate(
    db: &SqlitePool,
    limiter: &LoginLimiter,
    email: &str,
    password: &str,
) -> Result<User, LoginError> {
    if limiter.is_blocked(email) {
        return Err(LoginError::TooManyAttempts);
    }
    let user = db::users::find_by_email(db, email.trim()).await.map_err(LoginError::Db)?;
    // Always run one Argon2 verification, even for unknown e-mails (same timing).
    let hash = user.as_ref().map(|u| u.password_hash.as_str()).unwrap_or(dummy_hash());
    let password_ok = verify_password(password, hash);
    match user {
        Some(user) if password_ok => {
            limiter.clear(email);
            Ok(user)
        }
        _ => {
            limiter.record_failure(email);
            Err(LoginError::WrongCredentials)
        }
    }
}

/// Marks the session as logged in. A new session id is issued first, so an
/// id planted by an attacker before login is worthless (session fixation).
pub async fn log_in(session: &Session, db: &SqlitePool, user_id: i64) -> Result<(), AppError> {
    session.cycle_id().await?;
    session.insert(USER_ID_KEY, user_id).await?;
    crate::session_store::delete_expired(db).await?;
    Ok(())
}

/// Forgets everything in the session and deletes it on the server.
pub async fn log_out(session: &Session) -> Result<(), AppError> {
    session.flush().await?;
    Ok(())
}

// ── Extractors ──────────────────────────────────────────────────────────────

/// The logged-in user, or `None` for a guest. Never fails; every page uses it
/// for the header („Вход“ or the user's name).
#[derive(Debug, Clone)]
pub struct CurrentUser(pub Option<User>);

/// A logged-in client. Guests are redirected to the login page,
/// administrators get 403.
#[derive(Debug, Clone)]
pub struct RequireClient(pub User);

/// A logged-in administrator. Guests are redirected to the login page,
/// clients get 403 „Нямате достъп до тази страница.“
#[derive(Debug, Clone)]
pub struct RequireAdmin(pub User);

/// Loads the user once per request (cached in the request extensions, so
/// using two extractors in one handler costs one query).
async fn load_user(parts: &mut Parts, state: &AppState) -> Result<Option<User>, AppError> {
    if let Some(CurrentUser(cached)) = parts.extensions.get::<CurrentUser>() {
        return Ok(cached.clone());
    }
    let session = Session::from_request_parts(parts, state)
        .await
        .map_err(|_| AppError::Internal("session layer missing".to_string()))?;
    let user = match session.get::<i64>(USER_ID_KEY).await? {
        Some(id) => db::users::find_by_id(&state.db, id).await?,
        None => None,
    };
    parts.extensions.insert(CurrentUser(user.clone()));
    Ok(user)
}

/// `/vhod?sled=/moite-zayavki` – where to go after logging in.
pub fn login_redirect(parts: &Parts) -> Response {
    let target = parts.uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    Redirect::to(&format!("/vhod?sled={}", encode_query_value(target))).into_response()
}

/// Percent-encodes a value for a query string (keeps only unreserved characters).
pub fn encode_query_value(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Accepts only local paths as the `sled` target (no `//evil.example`).
pub fn safe_next_path(next: Option<&str>) -> Option<String> {
    let next = next?.trim();
    if next.starts_with('/') && !next.starts_with("//") && !next.contains('\\') {
        Some(next.to_string())
    } else {
        None
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        Ok(CurrentUser(load_user(parts, state).await?))
    }
}

impl FromRequestParts<AppState> for RequireClient {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        match load_user(parts, state).await.map_err(IntoResponse::into_response)? {
            Some(user) if user.role == Role::Client => Ok(RequireClient(user)),
            Some(_) => Err(crate::errors::error_page(
                axum::http::StatusCode::FORBIDDEN,
                "Нямате достъп до тази страница.",
                CLIENT_ONLY_MESSAGE,
            )),
            None => Err(login_redirect(parts)),
        }
    }
}

impl FromRequestParts<AppState> for RequireAdmin {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        match load_user(parts, state).await.map_err(IntoResponse::into_response)? {
            Some(user) if user.role == Role::Admin => Ok(RequireAdmin(user)),
            Some(_) => Err(AppError::Forbidden.into_response()),
            None => Err(login_redirect(parts)),
        }
    }
}

// ── Account form rules (registration and profile) ───────────────────────────

pub const EMAIL_MESSAGE: &str = "Въведете имейл във вида ime@primer.bg.";
pub const PASSWORD_MESSAGE: &str = "Паролата трябва да е поне 8 символа и да съдържа буква и цифра.";
pub const NAME_MESSAGE: &str = "Въведете име и фамилия (от 2 до 100 символа).";
pub const PHONE_MESSAGE: &str = "Телефонът започва с +359 или 0 и има от 9 до 12 цифри.";
pub const EIK_MESSAGE: &str = "ЕИК трябва да е 9 или 13 цифри.";
pub const EMAIL_TAKEN_MESSAGE: &str = "Вече има профил с този имейл. Влезте или използвайте друг имейл.";

/// Simple e-mail check: one „@“, text before it, a dot in the domain, no spaces.
pub fn valid_email(email: &str) -> bool {
    let email = email.trim();
    match email.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !domain.contains('@')
                && !email.contains(char::is_whitespace)
                && email.len() <= 254
        }
        None => false,
    }
}

/// At least 8 characters, with at least one letter and one digit.
pub fn valid_password(password: &str) -> bool {
    password.chars().count() >= 8
        && password.chars().any(char::is_alphabetic)
        && password.chars().any(|c| c.is_ascii_digit())
}

pub fn valid_full_name(name: &str) -> bool {
    (2..=100).contains(&name.trim().chars().count())
}

/// „+359 88 812 3456“ or „0888 123 456“: starts with +359 or 0, 9–12 digits in total.
pub fn valid_phone(phone: &str) -> bool {
    let phone = phone.trim();
    let allowed = phone.chars().all(|c| c.is_ascii_digit() || " -+()".contains(c));
    let digits = phone.chars().filter(|c| c.is_ascii_digit()).count();
    allowed && (phone.starts_with("+359") || phone.starts_with('0')) && (9..=12).contains(&digits)
}

/// Optional company id: empty → `Ok(None)`; 9 or 13 digits (spaces ignored) → `Ok(Some)`.
pub fn normalize_eik(eik: &str) -> Result<Option<String>, &'static str> {
    let digits: String = eik.chars().filter(|c| !c.is_whitespace()).collect();
    if digits.is_empty() {
        Ok(None)
    } else if digits.chars().all(|c| c.is_ascii_digit()) && (digits.len() == 9 || digits.len() == 13) {
        Ok(Some(digits))
    } else {
        Err(EIK_MESSAGE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify() {
        let hash = hash_password("Client123!").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password("Client123!", &hash));
        assert!(!verify_password("client123!", &hash));
        assert!(!verify_password("Client123!", "not a hash"));
        // same password, different salt → different hash
        assert_ne!(hash, hash_password("Client123!").unwrap());
    }

    #[test]
    fn limiter_blocks_after_five_failures() {
        let limiter = LoginLimiter::default();
        for _ in 0..5 {
            assert!(!limiter.is_blocked("Ivan@Example.com"));
            limiter.record_failure("ivan@example.com");
        }
        assert!(limiter.is_blocked("IVAN@example.com "));
        assert!(!limiter.is_blocked("other@example.com"));
        limiter.clear("ivan@example.com");
        assert!(!limiter.is_blocked("ivan@example.com"));
    }

    #[test]
    fn account_rules() {
        assert!(valid_email("ime@primer.bg"));
        assert!(!valid_email("ime@primer"));
        assert!(!valid_email("ime primer@x.bg"));
        assert!(!valid_email("@primer.bg"));
        assert!(valid_password("Parola123"));
        assert!(!valid_password("parola"));
        assert!(!valid_password("12345678"));
        assert!(!valid_password("абвгдежз"));
        assert!(valid_password("абвгдеж1"));
        assert!(valid_full_name("Иван Петров"));
        assert!(!valid_full_name("И"));
        assert!(valid_phone("+359 88 812 3456"));
        assert!(valid_phone("0888123456"));
        assert!(!valid_phone("888123456"));
        assert!(!valid_phone("+359 88"));
        assert_eq!(normalize_eik(""), Ok(None));
        assert_eq!(normalize_eik("204 556 781"), Ok(Some("204556781".to_string())));
        assert_eq!(normalize_eik("12345"), Err(EIK_MESSAGE));
    }

    #[test]
    fn redirect_targets() {
        assert_eq!(encode_query_value("/moite-zayavki?x=1&y=2"), "/moite-zayavki%3Fx%3D1%26y%3D2");
        assert_eq!(safe_next_path(Some("/admin")), Some("/admin".to_string()));
        assert_eq!(safe_next_path(Some("//evil.example")), None);
        assert_eq!(safe_next_path(Some("https://evil.example")), None);
        assert_eq!(safe_next_path(None), None);
    }
}
