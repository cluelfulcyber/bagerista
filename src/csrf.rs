//! CSRF protection with the synchroniser-token pattern (architecture §8).
//!
//! Every session gets one random token. Pages put it in their forms
//! (`<input type="hidden" name="csrf_token" value="…">`) and in the
//! `X-CSRF-Token` header for htmx. The `csrf_guard` middleware runs on every
//! POST and rejects it with 403 unless it carries the same token. A forged
//! form on another site cannot know the token, so its POST fails.

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use tower_sessions::Session;

use crate::errors::{AppError, CSRF_MESSAGE, error_page};

/// Session key under which the token is stored.
pub const SESSION_KEY: &str = "csrf";
/// Name of the hidden form field and of the query parameter (multipart uploads).
pub const FIELD_NAME: &str = "csrf_token";
/// Header sent by htmx / fetch.
pub const HEADER_NAME: &str = "x-csrf-token";
/// Largest url-encoded form body we buffer to look for the token.
const MAX_FORM_BYTES: usize = 64 * 1024;

/// Returns the session's CSRF token, creating it on first use
/// (32 random bytes from the operating system's secure generator, as hex).
pub async fn token(session: &Session) -> Result<String, AppError> {
    if let Some(existing) = session.get::<String>(SESSION_KEY).await? {
        return Ok(existing);
    }
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    session.insert(SESSION_KEY, &token).await?;
    Ok(token)
}

/// Compares two strings in constant time, so the response time does not
/// reveal how many leading characters of a guessed token were right.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let diff = a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    diff == 0
}

/// Finds `csrf_token=…` in an url-encoded string (`a=1&csrf_token=ab12…`).
/// The token is plain hex, so it needs no percent-decoding.
pub fn find_token(urlencoded: &str) -> Option<&str> {
    urlencoded
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == FIELD_NAME)
        .map(|(_, value)| value)
}

fn forbidden() -> Response {
    error_page(StatusCode::FORBIDDEN, "Нямате достъп до тази страница.", CSRF_MESSAGE)
}

/// Defence in depth: a POST whose `Origin` header names another site is rejected.
fn origin_is_foreign(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return false; // no Origin header (older browsers, tests) → rely on the token
    };
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    origin != format!("http://{host}") && origin != format!("https://{host}")
}

fn content_type(headers: &HeaderMap) -> &str {
    headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("")
}

/// Middleware: lets every non-POST request through; a POST must carry the
/// session's token in the `X-CSRF-Token` header, in the `csrf_token` field of
/// an url-encoded form, or – only for `multipart/form-data` (photo upload) –
/// in the `csrf_token` query parameter.
pub async fn csrf_guard(session: Session, request: Request, next: Next) -> Response {
    if request.method() != Method::POST {
        return next.run(request).await;
    }
    if origin_is_foreign(request.headers()) {
        return forbidden();
    }
    let expected = match session.get::<String>(SESSION_KEY).await {
        Ok(Some(token)) => token,
        _ => return forbidden(), // no session token → the form was not served by us
    };

    // 1. header (htmx, fetch)
    if let Some(sent) = request.headers().get(HEADER_NAME).and_then(|v| v.to_str().ok()) {
        return if constant_time_eq(sent, &expected) { next.run(request).await } else { forbidden() };
    }

    let ctype = content_type(request.headers()).to_ascii_lowercase();

    // 2. url-encoded form: read the body, look for the field, put the body back
    if ctype.starts_with("application/x-www-form-urlencoded") {
        let (parts, body) = request.into_parts();
        let Ok(bytes) = axum::body::to_bytes(body, MAX_FORM_BYTES).await else {
            return forbidden();
        };
        let ok = std::str::from_utf8(&bytes)
            .ok()
            .and_then(find_token)
            .is_some_and(|sent| constant_time_eq(sent, &expected));
        if !ok {
            return forbidden();
        }
        return next.run(Request::from_parts(parts, Body::from(bytes))).await;
    }

    // 3. multipart upload: the token travels in the query string
    if ctype.starts_with("multipart/form-data") {
        let ok = request
            .uri()
            .query()
            .and_then(find_token)
            .is_some_and(|sent| constant_time_eq(sent, &expected));
        return if ok { next.run(request).await } else { forbidden() };
    }

    forbidden()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_token_in_form_body() {
        assert_eq!(find_token("email=a%40b.bg&csrf_token=ab12&x=1"), Some("ab12"));
        assert_eq!(find_token("csrf_token="), Some(""));
        assert_eq!(find_token("email=a"), None);
        assert_eq!(find_token("xcsrf_token=ab12"), None);
    }

    #[test]
    fn constant_time_comparison() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn foreign_origin_is_detected() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "127.0.0.1:8080".parse().unwrap());
        assert!(!origin_is_foreign(&headers));
        headers.insert(header::ORIGIN, "http://127.0.0.1:8080".parse().unwrap());
        assert!(!origin_is_foreign(&headers));
        headers.insert(header::ORIGIN, "https://evil.example".parse().unwrap());
        assert!(origin_is_foreign(&headers));
    }
}
