//! Helpers shared by the integration tests.
//!
//! Every test gets its own in-memory SQLite database (one connection, see
//! `db::connect_memory`), with the real migrations, and talks to the real
//! router (`bagerista::build_app`) through `tower::ServiceExt::oneshot` –
//! no network, no files.
#![allow(dead_code)]

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use bagerista::db;
use bagerista::models::{AttachmentKind, Illustration, Role};
use bagerista::state::AppState;
use chrono::{Duration, NaiveDate};
use http_body_util::BodyExt;
use sqlx::SqlitePool;
use tower::ServiceExt;

pub const ADMIN_EMAIL: &str = "admin@bagerista.bg";
pub const ADMIN_PASSWORD: &str = "Admin123!";
pub const CLIENT_EMAIL: &str = "client@bagerista.bg";
pub const CLIENT_PASSWORD: &str = "Client123!";
pub const OTHER_CLIENT_EMAIL: &str = "maria.koleva@example.com";

/// An empty, migrated in-memory database.
pub async fn test_db() -> SqlitePool {
    let pool = db::connect_memory().await.expect("in-memory database");
    db::migrate(&pool).await.expect("migrations");
    pool
}

pub fn today() -> NaiveDate {
    bagerista::today()
}

/// Ids of the small fixture.
#[derive(Debug, Clone)]
pub struct Fixture {
    pub admin_id: i64,
    pub client_id: i64,
    pub other_client_id: i64,
    pub mini_id: i64,
    pub midi_id: i64,
    pub hammer_id: i64,
    /// Kubota KX057-6, client, approved, today+10 … today+12.
    pub approved_booking_id: i64,
}

/// Small fixture (architecture §10): 2 machines, 1 admin, 2 clients,
/// 1 approved booking. Built through the real `db::*` functions.
pub async fn fixture(pool: &SqlitePool) -> Fixture {
    let admin_hash = bagerista::auth::hash_password(ADMIN_PASSWORD).unwrap();
    let client_hash = bagerista::auth::hash_password(CLIENT_PASSWORD).unwrap();
    let user = |email: &str, name: &str, hash: &str, role| db::users::NewUser {
        email: email.to_string(),
        password_hash: hash.to_string(),
        full_name: name.to_string(),
        phone: "+359 88 812 3456".to_string(),
        company_name: None,
        company_eik: None,
        role,
    };
    let admin_id = db::users::create(pool, &user(ADMIN_EMAIL, "Георги Димитров", &admin_hash, Role::Admin)).await.unwrap();
    let client_id = db::users::create(pool, &user(CLIENT_EMAIL, "Иван Петров", &client_hash, Role::Client)).await.unwrap();
    let other_client_id =
        db::users::create(pool, &user(OTHER_CLIENT_EMAIL, "Мария Колева", &client_hash, Role::Client)).await.unwrap();

    sqlx::query(
        "INSERT INTO categories (id, slug, name, description, undercarriage, sort_order) VALUES
           (1, 'mini-bageri', 'Мини багери', 'Леки машини.', 'tracks', 1),
           (2, 'midibageri', 'Мидибагери', 'Универсални машини.', 'tracks', 2)",
    )
    .execute(pool)
    .await
    .unwrap();

    let machine = |slug: &str, category_id, name: &str, weight, depth, width, price, illustration| db::catalog::MachineInput {
        slug: slug.to_string(),
        category_id,
        name: name.to_string(),
        brand: "Kubota".to_string(),
        model: name.trim_start_matches("Kubota ").to_string(),
        year_made: 2021,
        weight_kg: weight,
        dig_depth_mm: depth,
        width_mm: width,
        height_mm: 2_500,
        engine_kw: 20.0,
        bucket_l: 60,
        price_per_day_cents: price,
        operator_price_per_day_cents: 150_00,
        engine_hours: 3_412,
        service_interval_hours: 500,
        last_service_hours: 3_324,
        description: "Описание.\n\nВтори абзац.".to_string(),
        illustration,
        active: true,
    };
    let mini_id = db::catalog::create_machine(pool, &machine("kubota-kx019-4", 1, "Kubota KX019-4", 1_800, 2_540, 990, 95_00, Illustration::Mini))
        .await
        .unwrap();
    let midi_id = db::catalog::create_machine(pool, &machine("kubota-kx057-6", 2, "Kubota KX057-6", 5_500, 3_800, 1_960, 160_00, Illustration::Midi))
        .await
        .unwrap();

    let hammer_id = db::catalog::create_attachment(
        pool,
        &db::catalog::AttachmentInput {
            slug: "hidravlichen-chuk-malak".to_string(),
            name: "Хидравличен чук (малък)".to_string(),
            description: "Бетон и асфалт до 30 см".to_string(),
            kind: AttachmentKind::Hammer,
            price_per_day_cents: 45_00,
            active: true,
        },
    )
    .await
    .unwrap();
    db::catalog::set_machine_attachments(pool, mini_id, &[hammer_id]).await.unwrap();
    db::catalog::set_machine_attachments(pool, midi_id, &[hammer_id]).await.unwrap();

    let start = today() + Duration::days(10);
    let end = today() + Duration::days(12);
    let booking = new_booking(midi_id, client_id, start, end, 160_00);
    let approved_booking_id = db::bookings::create_checked(pool, &booking).await.unwrap();
    db::bookings::approve_checked(pool, approved_booking_id).await.unwrap();

    Fixture { admin_id, client_id, other_client_id, mini_id, midi_id, hammer_id, approved_booking_id }
}

/// A booking request priced by the real pricing service (pickup, no extras).
pub fn new_booking(machine_id: i64, client_id: i64, start: NaiveDate, end: NaiveDate, day_cents: i64) -> db::bookings::NewBooking {
    let price = bagerista::services::pricing::quote(&bagerista::services::pricing::PriceInput {
        start,
        end,
        machine_day_cents: day_cents,
        operator_day_cents: 150_00,
        with_operator: false,
        attachment_day_cents: vec![],
        delivery_km: 0,
        heavy_transport: false,
    })
    .unwrap();
    db::bookings::NewBooking {
        machine_id,
        client_id,
        start,
        end,
        with_operator: false,
        delivery_km: 0,
        delivery_address: None,
        client_note: None,
        attachments: vec![],
        price,
    }
}

/// The real application on a fresh database with the small fixture.
pub async fn test_app() -> (Router, SqlitePool, Fixture) {
    let pool = test_db().await;
    let fx = fixture(&pool).await;
    (bagerista::build_app(AppState::new(pool.clone(), false)), pool, fx)
}

/// The real application on a fresh database with the full demo seed.
pub async fn seeded_app() -> (Router, SqlitePool) {
    let pool = test_db().await;
    bagerista::seed::run(&pool, today()).await.expect("seed");
    (bagerista::build_app(AppState::new(pool.clone(), false)), pool)
}

/// Wraps a test-only router in the production middleware (sessions, CSRF, headers).
pub fn wrap(router: Router<AppState>, pool: &SqlitePool) -> Router {
    bagerista::with_layers(router, AppState::new(pool.clone(), false))
}

// ── Requests ────────────────────────────────────────────────────────────────

/// A response with the body already read as text.
pub struct Resp {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: String,
}

impl Resp {
    pub fn location(&self) -> Option<&str> {
        self.headers.get(header::LOCATION).and_then(|v| v.to_str().ok())
    }

    /// `bagerista_sid=…` from `Set-Cookie`, ready for a `Cookie` header.
    pub fn session_cookie(&self) -> Option<String> {
        self.set_cookie_header().map(|c| c.split(';').next().unwrap_or("").to_string())
    }

    /// The whole `Set-Cookie` header of the session cookie (with its flags).
    pub fn set_cookie_header(&self) -> Option<&str> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find(|c| c.starts_with(&format!("{}=", bagerista::SESSION_COOKIE)))
    }
}

pub async fn send(app: &Router, request: Request<Body>) -> Resp {
    let response = app.clone().oneshot(request).await.expect("router never fails");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.expect("body").to_bytes();
    Resp { status, headers, body: String::from_utf8_lossy(&bytes).into_owned() }
}

pub async fn get(app: &Router, path: &str, cookie: Option<&str>) -> Resp {
    let mut request = Request::get(path);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    send(app, request.body(Body::empty()).unwrap()).await
}

/// Same as `get`, as htmx sends it (`HX-Request: true`).
pub async fn get_htmx(app: &Router, path: &str, cookie: Option<&str>) -> Resp {
    let mut request = Request::get(path).header("hx-request", "true");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    send(app, request.body(Body::empty()).unwrap()).await
}

/// Percent-encodes form fields: `[("email", "a@b.bg")]` → `email=a%40b.bg`.
pub fn form_encode(fields: &[(&str, &str)]) -> String {
    fn enc(s: &str) -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                b' ' => "+".to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect()
    }
    fields.iter().map(|(k, v)| format!("{}={}", enc(k), enc(v))).collect::<Vec<_>>().join("&")
}

/// POST an url-encoded form (add `("csrf_token", …)` yourself).
pub async fn post_form(app: &Router, path: &str, cookie: Option<&str>, fields: &[(&str, &str)]) -> Resp {
    let mut request = Request::post(path).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    send(app, request.body(Body::from(form_encode(fields))).unwrap()).await
}

/// The value of the first `name="csrf_token"` hidden input in an HTML page.
pub fn extract_csrf(html: &str) -> Option<String> {
    let at = html.find("name=\"csrf_token\"")?;
    let rest = &html[at..];
    let value_at = rest.find("value=\"")? + "value=\"".len();
    let value = &rest[value_at..];
    Some(value[..value.find('"')?].to_string())
}

/// GETs a page with a form and returns (session cookie, CSRF token).
/// Pass the current cookie to stay in the same session.
pub async fn get_csrf(app: &Router, path: &str, cookie: Option<&str>) -> (String, String) {
    let page = get(app, path, cookie).await;
    assert_eq!(page.status, StatusCode::OK, "GET {path}: {}", page.body);
    let cookie = page.session_cookie().or_else(|| cookie.map(str::to_string)).expect("session cookie");
    let token = extract_csrf(&page.body).unwrap_or_else(|| panic!("no csrf_token on {path}"));
    (cookie, token)
}

/// Logs in through a form page and its POST target; returns the new session
/// cookie (the id changes on login).
pub async fn login_at(app: &Router, form_path: &str, post_path: &str, email: &str, password: &str) -> String {
    let (cookie, token) = get_csrf(app, form_path, None).await;
    let resp = post_form(app, post_path, Some(&cookie), &[("email", email), ("password", password), ("csrf_token", &token)]).await;
    assert_eq!(resp.status, StatusCode::SEE_OTHER, "login of {email} failed: {}", resp.body);
    resp.session_cookie().unwrap_or(cookie)
}

/// Logs in through the real login page (`GET /vhod`, `POST /vhod` with the
/// fields `email`, `password`, `csrf_token`).
pub async fn login(app: &Router, email: &str, password: &str) -> String {
    login_at(app, "/vhod", "/vhod", email, password).await
}
