//! Static files and uploaded photos.
//!
//! Everything in `static/` (CSS, JS, fonts, SVG) is compiled into the binary
//! by `rust-embed`, so the program is a single file. (In debug builds
//! rust-embed reads the folder from disk, so CSS edits show up without
//! recompiling.) Machine photos live in the database as BLOBs.

use axum::extract::{Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

use crate::db;
use crate::errors::AppError;
use crate::routes::page::PathOr404;
use crate::state::AppState;

#[derive(RustEmbed)]
#[folder = "static/"]
struct StaticFiles;

/// Browsers may keep CSS/JS/fonts for a day.
const CACHE_STATIC: &str = "public, max-age=86400";

/// A file from `static/` with its content type, or our 404 page.
fn embedded(path: &str) -> Response {
    match StaticFiles::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [
                    (header::CONTENT_TYPE, HeaderValue::from_str(mime.as_ref()).unwrap_or(HeaderValue::from_static("application/octet-stream"))),
                    (header::CACHE_CONTROL, HeaderValue::from_static(CACHE_STATIC)),
                ],
                file.data.into_owned(),
            )
                .into_response()
        }
        None => AppError::NotFound.into_response(),
    }
}

/// `GET /static/{*path}` – e.g. `/static/css/app.css`.
pub async fn static_file(Path(path): Path<String>) -> Response {
    embedded(&path)
}

/// `GET /favicon.ico` – for browsers that ignore the SVG icon in `<head>`.
pub async fn favicon() -> Response {
    embedded("img/favicon.ico")
}

/// `GET /robots.txt` – search engines may index the public pages only.
pub async fn robots() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "User-agent: *\nDisallow: /admin\nDisallow: /moite-zayavki\nDisallow: /zayavki\n\
         Disallow: /profil\nDisallow: /vhod\nDisallow: /registratsia\nDisallow: /api\n",
    )
}

/// `GET /media/snimki/{id}` – a machine photo stored in `machine_photos`.
pub async fn photo(State(state): State<AppState>, PathOr404(id): PathOr404<i64>) -> Result<Response, AppError> {
    let (content_type, data) = db::catalog::photo_data(&state.db, id).await?.ok_or(AppError::NotFound)?;
    let content_type = HeaderValue::from_str(&content_type).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((
        [(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600"))],
        data,
    )
        .into_response())
}
