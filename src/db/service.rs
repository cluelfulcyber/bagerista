//! Service records (maintenance history) and the fleet maintenance overview.

use std::collections::HashMap;

use chrono::NaiveDate;
use sqlx::SqlitePool;

use crate::errors::AppError;
use crate::models::{Machine, ServiceKind, ServiceRecord};
use crate::services::maintenance::MaintenanceStatus;

macro_rules! service_select {
    () => {
        "SELECT s.id, s.machine_id, m.name AS machine_name, s.service_date, s.engine_hours, s.kind,
                s.description, s.cost_cents, u.full_name AS created_by_name, s.created_at
           FROM service_records s
           JOIN machines m ON m.id = s.machine_id
           LEFT JOIN users u ON u.id = s.created_by"
    };
}

/// Admin machine page (service history) – newest first.
pub async fn list_for_machine(db: &SqlitePool, machine_id: i64) -> Result<Vec<ServiceRecord>, sqlx::Error> {
    sqlx::query_as::<_, ServiceRecord>(concat!(
        service_select!(),
        " WHERE s.machine_id = ? ORDER BY s.service_date DESC, s.id DESC"
    ))
    .bind(machine_id)
    .fetch_all(db)
    .await
}

/// `GET /admin/obsluzhvane`: latest records of the whole fleet.
pub async fn recent(db: &SqlitePool, limit: i64) -> Result<Vec<ServiceRecord>, sqlx::Error> {
    sqlx::query_as::<_, ServiceRecord>(concat!(
        service_select!(),
        " ORDER BY s.service_date DESC, s.id DESC LIMIT ?"
    ))
    .bind(limit)
    .fetch_all(db)
    .await
}

/// Fields of the „Добави обслужване“ form.
#[derive(Debug, Clone)]
pub struct NewServiceRecord {
    pub machine_id: i64,
    pub service_date: NaiveDate,
    pub engine_hours: i64,
    pub kind: ServiceKind,
    pub description: String,
    pub cost_cents: i64,
    /// The admin who entered it.
    pub created_by: Option<i64>,
}

/// `POST /admin/mashini/{id}/obsluzhvane` (and seed).
///
/// A *scheduled* service resets the counter: `last_service_hours` becomes the
/// record's hours (and `engine_hours` is raised if the record is higher).
/// Repairs and inspections are history only. One transaction.
pub async fn add(db: &SqlitePool, r: &NewServiceRecord) -> Result<i64, AppError> {
    let mut tx = db.begin().await?;
    let id = sqlx::query(
        "INSERT INTO service_records (machine_id, service_date, engine_hours, kind, description, cost_cents, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(r.machine_id)
    .bind(r.service_date)
    .bind(r.engine_hours)
    .bind(r.kind)
    .bind(r.description.trim())
    .bind(r.cost_cents)
    .bind(r.created_by)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    if r.kind == ServiceKind::Scheduled {
        let updated = sqlx::query(
            "UPDATE machines SET engine_hours = MAX(engine_hours, ?1), last_service_hours = ?1 WHERE id = ?2",
        )
        .bind(r.engine_hours)
        .bind(r.machine_id)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
    }
    tx.commit().await?;
    Ok(id)
}

/// Date of the last technical inspection of one machine (admin machine page).
pub async fn last_inspection(db: &SqlitePool, machine_id: i64) -> Result<Option<NaiveDate>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT MAX(service_date) FROM service_records WHERE machine_id = ? AND kind = 'inspection'",
    )
    .bind(machine_id)
    .fetch_one(db)
    .await
}

/// Last inspection date per machine id.
pub async fn last_inspections(db: &SqlitePool) -> Result<HashMap<i64, NaiveDate>, sqlx::Error> {
    let rows: Vec<(i64, NaiveDate)> = sqlx::query_as(
        "SELECT machine_id, MAX(service_date) FROM service_records
          WHERE kind = 'inspection' GROUP BY machine_id ORDER BY machine_id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().collect())
}

/// A machine with its maintenance state.
#[derive(Debug, Clone)]
pub struct MachineMaintenance {
    pub machine: Machine,
    pub last_inspection: Option<NaiveDate>,
    pub status: MaintenanceStatus,
}

/// `GET /admin/obsluzhvane` and the dashboard warnings: every machine
/// (active or not) with its status, most urgent first (fewest hours left).
/// Loads the rows here and lets `services::maintenance` decide.
pub async fn fleet_maintenance(db: &SqlitePool, today: NaiveDate) -> Result<Vec<MachineMaintenance>, sqlx::Error> {
    let machines = sqlx::query_as::<_, Machine>(concat!(machine_select!(), " ORDER BY m.id"))
        .fetch_all(db)
        .await?;
    let inspections = last_inspections(db).await?;
    let mut list: Vec<MachineMaintenance> = machines
        .into_iter()
        .map(|machine| {
            let last_inspection = inspections.get(&machine.id).copied();
            let status = machine.maintenance(last_inspection, today);
            MachineMaintenance { machine, last_inspection, status }
        })
        .collect();
    list.sort_by_key(|m| (m.status.remaining_hours, m.machine.id));
    Ok(list)
}
