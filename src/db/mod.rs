//! DATA LAYER: every SQL statement of the application lives in this folder.
//!
//! Queries are plain SQL strings with `?` placeholders; values are always
//! bound with `.bind(…)`, never pasted into the SQL text, so SQL injection is
//! impossible. Rows are turned into structs from `models.rs` with
//! `#[derive(sqlx::FromRow)]`.

// The two SELECT heads below are shared by many queries. They are macros
// (not constants) so that `concat!` can glue them to a WHERE clause into one
// string literal at compile time; SQLx only accepts fixed SQL text.

/// `SELECT … FROM machines m JOIN categories c` → `models::Machine`.
macro_rules! machine_select {
    () => {
        "SELECT m.id, m.slug, m.category_id, c.slug AS category_slug, c.name AS category_name,
                c.undercarriage, m.name, m.brand, m.model, m.year_made, m.weight_kg, m.dig_depth_mm,
                m.width_mm, m.height_mm, m.engine_kw, m.bucket_l, m.price_per_day_cents,
                m.operator_price_per_day_cents, m.engine_hours, m.service_interval_hours,
                m.last_service_hours, m.description, m.illustration, m.active, m.created_at
           FROM machines m
           JOIN categories c ON c.id = m.category_id"
    };
}

/// `SELECT … FROM bookings b JOIN machines m JOIN users u` → `models::Booking`.
macro_rules! booking_select {
    () => {
        "SELECT b.id, b.machine_id, m.name AS machine_name, m.slug AS machine_slug,
                m.illustration AS machine_illustration, m.weight_kg AS machine_weight_kg,
                b.client_id, u.full_name AS client_name, u.email AS client_email,
                u.phone AS client_phone, u.company_name AS client_company, u.company_eik AS client_eik,
                b.start_date, b.end_date, b.with_operator, b.delivery_km, b.delivery_address,
                b.client_note, b.status, b.days, b.weekend_days, b.equipment_cents,
                b.discount_percent, b.discount_cents, b.operator_cents, b.transport_cents,
                b.net_cents, b.vat_cents, b.total_cents, b.admin_note, b.hours_at_start,
                b.hours_at_end, b.created_at, b.updated_at
           FROM bookings b
           JOIN machines m ON m.id = b.machine_id
           JOIN users u ON u.id = b.client_id"
    };
}

pub mod bookings;
pub mod catalog;
pub mod service;
pub mod stats;
pub mod users;

use std::str::FromStr;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

/// Opens (and creates if missing) the SQLite file with the settings from
/// architecture §3: foreign keys on, WAL journal (readers don't block the
/// writer), 5 s wait when the database is busy. Pool of 5 connections.
pub async fn connect(path: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));
    SqlitePoolOptions::new().max_connections(5).connect_with(options).await
}

/// An empty in-memory database for tests. Only ONE connection, because every
/// in-memory connection would be a separate, empty database; it is never
/// closed while the pool lives.
pub async fn connect_memory() -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
}

/// Creates or updates the tables. The SQL files in `migrations/` are compiled
/// into the binary; SQLx remembers which ones already ran.
pub async fn migrate(db: &SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(db).await
}
