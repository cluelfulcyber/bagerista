//! Admin dashboard numbers (`GET /admin`).
//!
//! Definitions (say them out loud at the defence):
//! - **utilisation** = booked machine-days (approved, active, completed) inside
//!   the period ÷ (active machines × days in the period);
//! - **revenue** = net amount (без ДДС) of active and completed rentals, counted
//!   in the month the rental starts.

use chrono::{Duration, NaiveDate};
use sqlx::SqlitePool;

use crate::db::service::MachineMaintenance;
use crate::models::{Booking, BookingStatus};
use crate::services::availability::{add_months, first_of_month, last_of_month};
use crate::services::maintenance::MaintenanceLevel;

/// Booked machine-days of all machines inside [from, to] (each booking is
/// clipped to the period: MIN of the ends, MAX of the starts).
async fn booked_days(db: &SqlitePool, from: NaiveDate, to: NaiveDate) -> Result<i64, sqlx::Error> {
    let days: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(julianday(MIN(end_date, ?2)) - julianday(MAX(start_date, ?1)) + 1), 0.0)
           FROM bookings
          WHERE status IN ('approved', 'active', 'completed')
            AND start_date <= ?2 AND end_date >= ?1",
    )
    .bind(from)
    .bind(to)
    .fetch_one(db)
    .await?;
    Ok(days.round() as i64)
}

/// Fleet utilisation in percent for [from, to] (dashboard tile „Заетост на парка“).
pub async fn utilization_percent(db: &SqlitePool, from: NaiveDate, to: NaiveDate) -> Result<i64, sqlx::Error> {
    let machines: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM machines WHERE active = 1")
        .fetch_one(db)
        .await?;
    let period_days = (to - from).num_days() + 1;
    if machines == 0 || period_days <= 0 {
        return Ok(0);
    }
    let booked = booked_days(db, from, to).await?;
    Ok((booked * 100 + machines * period_days / 2) / (machines * period_days))
}

/// One column of the revenue chart.
#[derive(Debug, Clone, PartialEq)]
pub struct MonthRevenue {
    /// First day of the month.
    pub month: NaiveDate,
    pub net_cents: i64,
}

/// Revenue chart: the 12 months ending with the month of `today` (oldest
/// first), months without rentals included as 0.
pub async fn revenue_by_month(db: &SqlitePool, today: NaiveDate) -> Result<Vec<MonthRevenue>, sqlx::Error> {
    let this_month = first_of_month(today);
    let first = add_months(this_month, -11);
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT substr(start_date, 1, 7) AS month, SUM(net_cents)
           FROM bookings
          WHERE status IN ('active', 'completed') AND start_date >= ? AND start_date <= ?
          GROUP BY month
          ORDER BY month",
    )
    .bind(first)
    .bind(last_of_month(this_month))
    .fetch_all(db)
    .await?;
    Ok((0..12)
        .map(|i| {
            let month = add_months(first, i);
            let key = crate::services::availability::month_param(month);
            let net_cents = rows.iter().find(|(m, _)| *m == key).map(|(_, cents)| *cents).unwrap_or(0);
            MonthRevenue { month, net_cents }
        })
        .collect())
}

/// Revenue (без ДДС) of rentals starting in [from, to].
pub async fn revenue_between(db: &SqlitePool, from: NaiveDate, to: NaiveDate) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(net_cents), 0) FROM bookings
          WHERE status IN ('active', 'completed') AND start_date >= ? AND start_date <= ?",
    )
    .bind(from)
    .bind(to)
    .fetch_one(db)
    .await
}

/// A row of „Най-наемани, последните 90 дни“.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MachineUsage {
    pub machine_id: i64,
    pub name: String,
    pub slug: String,
    pub booked_days: i64,
    /// booked_days ÷ period days, in percent.
    pub percent: i64,
}

/// Dashboard: the most rented machines over the last `days` days (including today).
pub async fn top_machines(db: &SqlitePool, today: NaiveDate, days: i64, limit: i64) -> Result<Vec<MachineUsage>, sqlx::Error> {
    let from = today - Duration::days(days - 1);
    sqlx::query_as::<_, MachineUsage>(
        "SELECT m.id AS machine_id, m.name, m.slug,
                CAST(ROUND(SUM(julianday(MIN(b.end_date, ?2)) - julianday(MAX(b.start_date, ?1)) + 1)) AS INTEGER) AS booked_days,
                CAST(ROUND(SUM(julianday(MIN(b.end_date, ?2)) - julianday(MAX(b.start_date, ?1)) + 1) * 100.0 / ?3) AS INTEGER) AS percent
           FROM bookings b JOIN machines m ON m.id = b.machine_id
          WHERE b.status IN ('approved', 'active', 'completed')
            AND b.start_date <= ?2 AND b.end_date >= ?1
          GROUP BY m.id
          ORDER BY booked_days DESC, m.id ASC
          LIMIT ?4",
    )
    .bind(from)
    .bind(today)
    .bind(days)
    .bind(limit)
    .fetch_all(db)
    .await
}

/// Everything the dashboard shows, in one struct.
#[derive(Debug, Clone)]
pub struct Dashboard {
    pub today: NaiveDate,
    /// „Заетост на парка, октомври 64 %“ (whole current month, incl. approved days ahead)
    pub utilization_percent: i64,
    /// „септември: 71 %“
    pub prev_utilization_percent: i64,
    /// „Приходи без ДДС, октомври 6 900 €“ (rentals started this month so far)
    pub month_revenue_cents: i64,
    /// „Нови заявки 4“
    pub new_requests: i64,
    /// „1 се застъпва с одобрена“
    pub new_conflicting: i64,
    /// „Просрочено обслужване 2“
    pub overdue_count: i64,
    /// „и 2 скоро“
    pub due_soon_count: i64,
    /// 12 columns, oldest first; the last one is the current month.
    pub revenue_by_month: Vec<MonthRevenue>,
    /// „Най-наемани, последните 90 дни“ (top 5)
    pub top_machines: Vec<MachineUsage>,
    /// Active rentals and approved ones still to start (up to 8).
    pub upcoming: Vec<Booking>,
    /// Machines that are overdue or due soon, most urgent first.
    pub service_warnings: Vec<MachineMaintenance>,
}

/// `GET /admin`: collects all dashboard numbers.
pub async fn dashboard(db: &SqlitePool, today: NaiveDate) -> Result<Dashboard, sqlx::Error> {
    let this_month = first_of_month(today);
    let prev_month = add_months(this_month, -1);
    let fleet = crate::db::service::fleet_maintenance(db, today).await?;
    let service_warnings: Vec<MachineMaintenance> = fleet
        .into_iter()
        .filter(|m| m.machine.active && m.status.level != MaintenanceLevel::Ok)
        .collect();
    let count_level = |level| service_warnings.iter().filter(|m| m.status.level == level).count() as i64;

    Ok(Dashboard {
        today,
        utilization_percent: utilization_percent(db, this_month, last_of_month(this_month)).await?,
        prev_utilization_percent: utilization_percent(db, prev_month, last_of_month(prev_month)).await?,
        month_revenue_cents: revenue_between(db, this_month, today).await?,
        new_requests: crate::db::bookings::count_by_status(db, BookingStatus::New).await?,
        new_conflicting: crate::db::bookings::new_conflicting_count(db).await?,
        overdue_count: count_level(MaintenanceLevel::Overdue),
        due_soon_count: count_level(MaintenanceLevel::DueSoon),
        revenue_by_month: revenue_by_month(db, today).await?,
        top_machines: top_machines(db, today, 90, 5).await?,
        upcoming: crate::db::bookings::upcoming(db, today, 8).await?,
        service_warnings,
    })
}
