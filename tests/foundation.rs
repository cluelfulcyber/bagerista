//! Integration tests of the foundation: migrations + seed, the data layer on
//! real demo data, login with sessions, role guards and CSRF protection.
//! (The page-level flows live in public_flows.rs / booking_flows.rs / security.rs.)

// Money literals are written as euros_cents: `160_00` = 160,00 €.
#![allow(clippy::inconsistent_digit_grouping)]

mod common;

use axum::body::Body;
use axum::extract::{Form, State};
use axum::http::{Request, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing;
use axum::Router;
use bagerista::auth::{self, CurrentUser, LoginError, RequireAdmin, RequireClient};
use bagerista::db;
use bagerista::errors::AppError;
use bagerista::models::{BookingStatus, Illustration};
use bagerista::services::maintenance::MaintenanceLevel;
use bagerista::state::AppState;
use chrono::Duration;
use common::*;
use serde::Deserialize;
use tower_sessions::Session;

// ── Seed and data layer ─────────────────────────────────────────────────────

#[tokio::test]
async fn migrations_and_seed_create_the_demo() {
    let (_app, pool) = seeded_app().await;
    let count = |sql: &'static str| {
        let pool = pool.clone();
        async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&pool).await.unwrap() }
    };
    assert_eq!(count("SELECT COUNT(*) FROM users WHERE role = 'admin'").await, 1);
    assert_eq!(count("SELECT COUNT(*) FROM users WHERE role = 'client'").await, 12);
    assert_eq!(count("SELECT COUNT(*) FROM users WHERE company_name IS NOT NULL").await, 7);
    assert_eq!(count("SELECT COUNT(*) FROM categories").await, 5);
    assert_eq!(count("SELECT COUNT(*) FROM machines WHERE active = 1").await, 14);
    assert_eq!(count("SELECT COUNT(*) FROM attachments").await, 8);
    assert!(count("SELECT COUNT(*) FROM bookings WHERE status IN ('completed', 'declined', 'cancelled')").await >= 140);
    assert_eq!(count("SELECT COUNT(*) FROM bookings WHERE status = 'active'").await, 2);
    assert_eq!(count("SELECT COUNT(*) FROM bookings WHERE status = 'new'").await, 4);
    assert!(count("SELECT COUNT(*) FROM service_records").await >= 25);
    assert_eq!(db::users::count(&pool).await.unwrap(), 13);

    // no two blocking bookings of one machine overlap
    let clashes = count(
        "SELECT COUNT(*) FROM bookings a JOIN bookings b
           ON a.machine_id = b.machine_id AND a.id < b.id
          WHERE a.status IN ('approved', 'active') AND b.status IN ('approved', 'active')
            AND a.start_date <= b.end_date AND a.end_date >= b.start_date",
    )
    .await;
    assert_eq!(clashes, 0);

    // engine hours = hours at the end of the last completed rental
    let inconsistent = count(
        "SELECT COUNT(*) FROM machines m
          WHERE m.engine_hours <> COALESCE((SELECT MAX(hours_at_end) FROM bookings b
                                             WHERE b.machine_id = m.id AND b.status = 'completed'), m.engine_hours)",
    )
    .await;
    assert_eq!(inconsistent, 0);

    // every machine has a scheduled service that matches last_service_hours
    let missing = count(
        "SELECT COUNT(*) FROM machines m
          WHERE NOT EXISTS (SELECT 1 FROM service_records s
                             WHERE s.machine_id = m.id AND s.kind = 'scheduled'
                               AND s.engine_hours = m.last_service_hours)",
    )
    .await;
    assert_eq!(missing, 0);
}

#[tokio::test]
async fn seeded_machines_match_the_mockups() {
    let (_app, pool) = seeded_app().await;
    let kx057 = db::catalog::machine_by_slug(&pool, "kubota-kx057-6").await.unwrap().unwrap();
    assert_eq!(kx057.catalog_no(), "БГР-МД-005");
    assert_eq!((kx057.price_per_day_cents, kx057.operator_price_per_day_cents), (160_00, 150_00));
    assert_eq!((kx057.weight_kg, kx057.dig_depth_mm, kx057.width_mm, kx057.height_mm), (5_500, 3_800, 1_960, 2_550));
    assert_eq!(kx057.engine_hours, 3_412);
    assert_eq!(kx057.week_price_cents(), 144_00);
    assert_eq!(kx057.paragraphs().len(), 3);
    let status = kx057.maintenance(None, today());
    assert_eq!(status.message, "Следващо обслужване след 412 моточаса");

    let attachments = db::catalog::attachments_for_machine(&pool, kx057.id).await.unwrap();
    let ids: Vec<i64> = attachments.iter().map(|a| a.id).collect();
    assert_eq!(ids, vec![1, 3, 4, 5, 6, 7, 8]);

    let a918 = db::catalog::machine_by_id(&pool, 12).await.unwrap().unwrap();
    assert_eq!((a918.name.as_str(), a918.catalog_no()), ("Liebherr A 918 Compact", "БГР-КБ-012".to_string()));

    // catalogue filter and sort
    let filter = db::catalog::CatalogFilter { category_slug: Some("mini-bageri".into()), ..Default::default() };
    let minis = db::catalog::list_machines(&pool, &filter).await.unwrap();
    assert_eq!(minis.len(), 4);
    assert!(minis.iter().all(|m| m.illustration == Illustration::Mini));
    assert_eq!(minis[0].name, "Kubota U10-5"); // cheapest first
    let deep = db::catalog::CatalogFilter {
        min_depth_mm: Some(4_000),
        max_price_cents: Some(250_00),
        sort: db::catalog::CatalogSort::Depth,
        ..Default::default()
    };
    let names: Vec<String> = db::catalog::list_machines(&pool, &deep).await.unwrap().into_iter().map(|m| m.name).collect();
    assert_eq!(names, vec!["JCB 3CX", "CAT 432", "Takeuchi TB290", "Volvo ECR88D"]);

    let stats = db::catalog::category_stats(&pool).await.unwrap();
    let mini = stats.iter().find(|c| c.slug == "mini-bageri").unwrap();
    assert_eq!((mini.machine_count, mini.min_weight_kg, mini.max_weight_kg), (4, Some(1_120), Some(2_600)));
    assert_eq!(mini.min_price_cents, Some(75_00));
    assert_eq!(stats.first().unwrap().slug, "mini-bageri");

    let similar: Vec<String> =
        db::catalog::similar_machines(&pool, &kx057, 3).await.unwrap().into_iter().map(|m| m.name).collect();
    assert_eq!(similar, vec!["Takeuchi TB290", "Volvo ECR88D", "Yanmar ViO26-6"]);
    let narrower: Vec<String> =
        db::catalog::narrower_machines(&pool, kx057.width_mm, 2).await.unwrap().into_iter().map(|m| m.name).collect();
    assert_eq!(narrower, vec!["Yanmar ViO26-6", "Kubota KX019-4"]);
    let deeper = db::catalog::lightest_reaching(&pool, 4_200, kx057.category_id).await.unwrap().unwrap();
    assert_eq!(deeper.name, "Takeuchi TB290");

    let facts = db::catalog::wizard_facts(&pool).await.unwrap();
    assert_eq!(facts.len(), 14);
    assert!(facts.iter().find(|f| f.name == "JCB 3CX").unwrap().has_hammer); // the big hammer fits from 8 000 кг
    assert!(facts.iter().all(|f| f.has_hammer));
}

#[tokio::test]
async fn seeded_demo_tells_the_story_of_the_mockups() {
    let (_app, pool) = seeded_app().await;
    let today = today();

    // the demo client: one completed, one approved, one new
    let client = db::users::find_by_email(&pool, CLIENT_EMAIL).await.unwrap().unwrap();
    let mut statuses: Vec<BookingStatus> =
        db::bookings::for_client(&pool, client.id).await.unwrap().iter().map(|b| b.status).collect();
    statuses.sort_by_key(|s| s.code());
    assert_eq!(statuses, vec![BookingStatus::Approved, BookingStatus::Completed, BookingStatus::New]);

    // availability badges as on the home page price list
    let badge = |slug: &'static str| {
        let pool = pool.clone();
        async move {
            let m = db::catalog::machine_by_slug(&pool, slug).await.unwrap().unwrap();
            let spans = db::bookings::blocking_spans(&pool, m.id, today, today + Duration::days(60)).await.unwrap();
            bagerista::services::availability::availability(today, &spans)
        }
    };
    use bagerista::services::availability::Availability;
    assert_eq!(badge("yanmar-vio26-6").await, Availability::BusyUntil(today + Duration::days(6)));
    assert_eq!(badge("kubota-kx057-6").await, Availability::FreeUntil(today + Duration::days(10)));
    assert_eq!(badge("volvo-ecr88d").await, Availability::FreeUntil(today + Duration::days(3)));

    // maintenance: 2 overdue, 2 due soon
    let fleet = db::service::fleet_maintenance(&pool, today).await.unwrap();
    let level_count = |level| fleet.iter().filter(|m| m.status.level == level).count();
    assert_eq!(level_count(MaintenanceLevel::Overdue), 2);
    assert_eq!(level_count(MaintenanceLevel::DueSoon), 2);
    assert_eq!(fleet[0].machine.name, "Liebherr A 918 Compact"); // most overdue first
    assert!(fleet.iter().any(|m| m.status.inspection_due));

    // dashboard
    let dash = db::stats::dashboard(&pool, today).await.unwrap();
    assert_eq!(dash.new_requests, 4);
    assert_eq!(dash.new_conflicting, 1);
    assert_eq!((dash.overdue_count, dash.due_soon_count), (2, 2));
    assert_eq!(dash.revenue_by_month.len(), 12);
    assert!(dash.revenue_by_month.iter().filter(|m| m.net_cents > 0).count() >= 10);
    assert!(dash.utilization_percent > 0 && dash.utilization_percent <= 100);
    assert_eq!(dash.top_machines.len(), 5);
    assert!(dash.upcoming.len() >= 5);
    assert_eq!(dash.upcoming[0].status, BookingStatus::Active);

    // clients list
    let clients = db::users::list_clients(&pool).await.unwrap();
    assert_eq!(clients.len(), 12);
    assert!(clients.windows(2).all(|w| w[0].total_net_cents >= w[1].total_net_cents));
    let ivan = db::users::client_summary(&pool, client.id).await.unwrap().unwrap();
    assert_eq!(ivan.bookings_count, 3);
    assert!(db::users::client_summary(&pool, 1).await.unwrap().is_none()); // the admin is not a client

    // fleet calendar data for the next 30 days
    let spans = db::bookings::fleet_spans(&pool, today, today + Duration::days(30)).await.unwrap();
    assert!(spans.iter().any(|s| s.status == BookingStatus::New));
    assert!(spans.iter().any(|s| s.status == BookingStatus::Active));
}

#[tokio::test]
async fn double_booking_is_refused_in_both_places() {
    let (_app, pool) = seeded_app().await;

    // the seeded ECR88D request overlaps an approved one → approval refused, status unchanged
    let filter = db::bookings::AdminBookingFilter { status: Some(BookingStatus::New), ..Default::default() };
    let new_ones = db::bookings::admin_list(&pool, &filter).await.unwrap();
    assert_eq!(new_ones.len(), 4);
    let clashing = new_ones.iter().find(|b| b.machine_slug == "volvo-ecr88d").unwrap();
    let conflicts = db::bookings::conflicts_for(&pool, clashing).await.unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].status, BookingStatus::Approved);
    match db::bookings::approve_checked(&pool, clashing.id).await {
        Err(AppError::Conflict(msg)) => assert!(msg.starts_with("Не може да се одобри: застъпва се със заявка №"), "{msg}"),
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert_eq!(db::bookings::by_id(&pool, clashing.id).await.unwrap().unwrap().status, BookingStatus::New);

    // a new request on approved days → refused, nothing stored
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings").fetch_one(&pool).await.unwrap();
    let request = new_booking(clashing.machine_id, clashing.client_id, clashing.start_date, clashing.start_date, 220_00);
    assert!(matches!(db::bookings::create_checked(&pool, &request).await, Err(AppError::Conflict(_))));
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings").fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);

    // the day after the approved booking is free
    let free_day = conflicts[0].end_date + Duration::days(1);
    let request = new_booking(clashing.machine_id, clashing.client_id, free_day, free_day, 220_00);
    let id = db::bookings::create_checked(&pool, &request).await.unwrap();
    assert_eq!(db::bookings::by_id(&pool, id).await.unwrap().unwrap().status, BookingStatus::New);
}

#[tokio::test]
async fn status_changes_and_engine_hours() {
    let pool = test_db().await;
    let fx = fixture(&pool).await;

    // approved → active stores the hours at start
    db::bookings::start(&pool, fx.approved_booking_id).await.unwrap();
    let b = db::bookings::by_id(&pool, fx.approved_booking_id).await.unwrap().unwrap();
    assert_eq!((b.status, b.hours_at_start), (BookingStatus::Active, Some(3_412)));
    // a second click does nothing
    assert!(matches!(db::bookings::start(&pool, fx.approved_booking_id).await, Err(AppError::Conflict(_))));

    // completing with fewer hours than now is refused
    assert!(matches!(db::bookings::complete(&pool, fx.approved_booking_id, 3_000).await, Err(AppError::BadRequest(_))));
    db::bookings::complete(&pool, fx.approved_booking_id, 3_440).await.unwrap();
    let machine = db::catalog::machine_by_id(&pool, fx.midi_id).await.unwrap().unwrap();
    assert_eq!(machine.engine_hours, 3_440);
    assert_eq!(machine.maintenance(None, today()).remaining_hours, 384);

    // a scheduled service resets the counter
    let record = db::service::NewServiceRecord {
        machine_id: fx.midi_id,
        service_date: today(),
        engine_hours: 3_450,
        kind: bagerista::models::ServiceKind::Scheduled,
        description: "Планово обслужване".to_string(),
        cost_cents: 260_00,
        created_by: Some(fx.admin_id),
    };
    db::service::add(&pool, &record).await.unwrap();
    let machine = db::catalog::machine_by_id(&pool, fx.midi_id).await.unwrap().unwrap();
    assert_eq!((machine.engine_hours, machine.last_service_hours), (3_450, 3_450));
    assert_eq!(db::service::list_for_machine(&pool, fx.midi_id).await.unwrap()[0].created_by_name.as_deref(), Some("Георги Димитров"));

    // client cancel: only own, only new
    let start = today() + Duration::days(20);
    let id = db::bookings::create_checked(&pool, &new_booking(fx.mini_id, fx.client_id, start, start, 95_00)).await.unwrap();
    assert!(matches!(db::bookings::cancel_by_client(&pool, id, fx.other_client_id).await, Err(AppError::NotFound)));
    db::bookings::cancel_by_client(&pool, id, fx.client_id).await.unwrap();
    assert_eq!(db::bookings::by_id(&pool, id).await.unwrap().unwrap().status, BookingStatus::Cancelled);
    assert!(matches!(db::bookings::cancel_by_client(&pool, id, fx.client_id).await, Err(AppError::Conflict(_))));
    assert!(db::bookings::by_id_for_client(&pool, id, fx.other_client_id).await.unwrap().is_none());

    // decline keeps the reason
    let id = db::bookings::create_checked(&pool, &new_booking(fx.mini_id, fx.client_id, start, start, 95_00)).await.unwrap();
    db::bookings::decline(&pool, id, Some("Машината е в ремонт в този период.")).await.unwrap();
    let b = db::bookings::by_id(&pool, id).await.unwrap().unwrap();
    assert_eq!((b.status, b.admin_note.as_deref()), (BookingStatus::Declined, Some("Машината е в ремонт в този период.")));
    assert!(matches!(db::bookings::decline(&pool, 9_999, None).await, Err(AppError::NotFound)));
}

// ── Sessions, login, roles, CSRF (through small test-only routes) ───────────

#[derive(Deserialize)]
struct LoginForm {
    email: String,
    password: String,
}

/// A page with a form: the CSRF token as a hidden input.
async fn form_page(session: Session) -> Result<Html<String>, AppError> {
    let token = bagerista::csrf::token(&session).await?;
    Ok(Html(format!(
        "<form method=\"post\"><input type=\"hidden\" name=\"csrf_token\" value=\"{token}\"></form>"
    )))
}

async fn login_post(State(state): State<AppState>, session: Session, Form(form): Form<LoginForm>) -> Response {
    match auth::authenticate(&state.db, &state.login_limiter, &form.email, &form.password).await {
        Ok(user) => match auth::log_in(&session, &state.db, user.id).await {
            Ok(()) => Redirect::to("/").into_response(),
            Err(e) => e.into_response(),
        },
        Err(LoginError::TooManyAttempts) => AppError::TooManyRequests.into_response(),
        Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, e.message()).into_response(),
    }
}

async fn me(CurrentUser(user): CurrentUser) -> String {
    user.map(|u| u.full_name).unwrap_or_else(|| "гост".to_string())
}

async fn admin_only(RequireAdmin(user): RequireAdmin) -> String {
    format!("админ {}", user.full_name)
}

async fn client_only(RequireClient(user): RequireClient) -> String {
    format!("клиент {}", user.full_name)
}

async fn logout(session: Session) -> Result<Redirect, AppError> {
    auth::log_out(&session).await?;
    Ok(Redirect::to("/"))
}

fn test_router() -> Router<AppState> {
    Router::new()
        .route("/test/form", routing::get(form_page))
        .route("/test/login", routing::post(login_post))
        .route("/test/me", routing::get(me))
        .route("/test/admin", routing::get(admin_only))
        .route("/test/client", routing::get(client_only))
        .route("/test/logout", routing::post(logout))
        .route("/test/echo", routing::post(|| async { "приет" }))
}

#[tokio::test]
async fn public_router_answers_with_security_headers() {
    let (app, _pool, _fx) = test_app().await;
    let home = get(&app, "/", None).await;
    assert_eq!(home.status, StatusCode::OK);
    assert!(home.headers.get(header::CONTENT_SECURITY_POLICY).unwrap().to_str().unwrap().contains("default-src 'self'"));
    assert_eq!(home.headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(), "nosniff");

    let missing = get(&app, "/nyama-takava-stranitsa", None).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert!(missing.body.contains("Страницата не е намерена"));
    assert!(missing.body.contains("lang=\"bg\""));
}

#[tokio::test]
async fn login_sessions_and_role_guards() {
    let pool = test_db().await;
    fixture(&pool).await;
    let app = wrap(test_router(), &pool);

    // guest
    assert_eq!(get(&app, "/test/me", None).await.body, "гост");
    let guest_admin = get(&app, "/test/admin?x=1", None).await;
    assert_eq!(guest_admin.status, StatusCode::SEE_OTHER);
    assert_eq!(guest_admin.location(), Some("/vhod?sled=/test/admin%3Fx%3D1"));

    // the session cookie is HttpOnly and SameSite=Lax
    let page = get(&app, "/test/form", None).await;
    let set_cookie = page.set_cookie_header().expect("session cookie").to_string();
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(set_cookie.contains("SameSite=Lax"), "{set_cookie}");

    // client
    let client = login_at(&app, "/test/form", "/test/login", CLIENT_EMAIL, CLIENT_PASSWORD).await;
    assert_eq!(get(&app, "/test/me", Some(&client)).await.body, "Иван Петров");
    assert_eq!(get(&app, "/test/client", Some(&client)).await.body, "клиент Иван Петров");
    let forbidden = get(&app, "/test/admin", Some(&client)).await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);
    assert!(forbidden.body.contains("Тази страница е достъпна само за администратори."));

    // admin
    let admin = login_at(&app, "/test/form", "/test/login", ADMIN_EMAIL, ADMIN_PASSWORD).await;
    let ok = get(&app, "/test/admin", Some(&admin)).await;
    assert_eq!((ok.status, ok.body.as_str()), (StatusCode::OK, "админ Георги Димитров"));
    let not_for_admin = get(&app, "/test/client", Some(&admin)).await;
    assert_eq!(not_for_admin.status, StatusCode::FORBIDDEN);
    assert!(not_for_admin.body.contains("Тази страница е за клиенти."));

    // logout forgets the session
    let (_, token) = get_csrf(&app, "/test/form", Some(&admin)).await;
    let out = post_form(&app, "/test/logout", Some(&admin), &[("csrf_token", &token)]).await;
    assert_eq!(out.status, StatusCode::SEE_OTHER);
    assert_eq!(get(&app, "/test/me", Some(&admin)).await.body, "гост");
}

#[tokio::test]
async fn login_changes_the_session_id_and_limits_failures() {
    let pool = test_db().await;
    fixture(&pool).await;
    let app = wrap(test_router(), &pool);

    let (guest_cookie, token) = get_csrf(&app, "/test/form", None).await;
    let fields = |pw: &'static str| [("email", CLIENT_EMAIL), ("password", pw), ("csrf_token", "")];
    // session fixation: the id after login differs from the id before
    let mut ok = fields(CLIENT_PASSWORD);
    ok[2].1 = &token;
    let resp = post_form(&app, "/test/login", Some(&guest_cookie), &ok).await;
    assert_eq!(resp.status, StatusCode::SEE_OTHER);
    assert_ne!(resp.session_cookie().unwrap(), guest_cookie);

    // 5 wrong passwords → the 6th attempt is refused with 429, even with the right password
    let (cookie, token) = get_csrf(&app, "/test/form", None).await;
    for _ in 0..5 {
        let mut wrong = fields("Greshna123");
        wrong[2].1 = &token;
        let resp = post_form(&app, "/test/login", Some(&cookie), &wrong).await;
        assert_eq!(resp.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(resp.body, "Грешен имейл или парола.");
    }
    let mut right = fields(CLIENT_PASSWORD);
    right[2].1 = &token;
    let blocked = post_form(&app, "/test/login", Some(&cookie), &right).await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(blocked.body.contains("Опитайте отново след 15 минути."));

    // unknown e-mail: the same message
    let mut unknown = [("email", "nyama@example.com"), ("password", "Parola123"), ("csrf_token", "")];
    unknown[2].1 = &token;
    assert_eq!(post_form(&app, "/test/login", Some(&cookie), &unknown).await.body, "Грешен имейл или парола.");
}

#[tokio::test]
async fn csrf_guard_checks_every_post() {
    let pool = test_db().await;
    let app = wrap(test_router(), &pool);
    let (cookie, token) = get_csrf(&app, "/test/form", None).await;
    assert_eq!(token.len(), 64);

    // no session at all
    assert_eq!(post_form(&app, "/test/echo", None, &[("a", "1")]).await.status, StatusCode::FORBIDDEN);
    // session but no token
    let no_token = post_form(&app, "/test/echo", Some(&cookie), &[("a", "1")]).await;
    assert_eq!(no_token.status, StatusCode::FORBIDDEN);
    assert!(no_token.body.contains("Сесията е изтекла или формата е невалидна."));
    // wrong token
    let wrong = "0".repeat(64);
    assert_eq!(post_form(&app, "/test/echo", Some(&cookie), &[("csrf_token", &wrong)]).await.status, StatusCode::FORBIDDEN);
    // right token in the form
    let ok = post_form(&app, "/test/echo", Some(&cookie), &[("a", "1"), ("csrf_token", &token)]).await;
    assert_eq!((ok.status, ok.body.as_str()), (StatusCode::OK, "приет"));

    // right token in the X-CSRF-Token header (htmx)
    let request = Request::post("/test/echo")
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &token)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, request).await.status, StatusCode::OK);

    // multipart: the token must be in the query string
    let multipart = |uri: String| {
        Request::post(uri)
            .header(header::COOKIE, &cookie)
            .header(header::CONTENT_TYPE, "multipart/form-data; boundary=X")
            .body(Body::from("--X\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\n1\r\n--X--\r\n"))
            .unwrap()
    };
    assert_eq!(send(&app, multipart("/test/echo".into())).await.status, StatusCode::FORBIDDEN);
    assert_eq!(send(&app, multipart(format!("/test/echo?csrf_token={token}"))).await.status, StatusCode::OK);

    // a POST from another site is refused even with the token
    let foreign = Request::post("/test/echo")
        .header(header::COOKIE, &cookie)
        .header(header::HOST, "127.0.0.1:8080")
        .header(header::ORIGIN, "https://evil.example")
        .header("x-csrf-token", &token)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, foreign).await.status, StatusCode::FORBIDDEN);

    // GET requests are never blocked
    assert_eq!(get(&app, "/test/me", Some(&cookie)).await.status, StatusCode::OK);
}
