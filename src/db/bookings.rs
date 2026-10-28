//! Bookings (rental requests): creating them without double booking,
//! status changes and every list the client and admin pages show.

use chrono::NaiveDate;
use sqlx::{SqliteConnection, SqlitePool};

use crate::errors::AppError;
use crate::format;
use crate::models::{Booking, BookingAttachment, BookingStatus};
use crate::services::bookings::{ALREADY_PROCESSED, MACHINE_TAKEN};
use crate::services::pricing::PriceBreakdown;

/// Everything needed to store a new request. The price comes from
/// `services::pricing::quote`, computed on the server.
#[derive(Debug, Clone)]
pub struct NewBooking {
    pub machine_id: i64,
    pub client_id: i64,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub with_operator: bool,
    pub delivery_km: i64,
    pub delivery_address: Option<String>,
    pub client_note: Option<String>,
    /// (attachment id, price per day at this moment)
    pub attachments: Vec<(i64, i64)>,
    pub price: PriceBreakdown,
}

/// Id of an approved/active booking of `machine_id` that overlaps the dates,
/// ignoring `except_id`. Same overlap formula as `services::availability::overlaps`.
async fn blocking_clash(
    conn: &mut SqliteConnection,
    machine_id: i64,
    start: NaiveDate,
    end: NaiveDate,
    except_id: i64,
) -> Result<Option<(i64, String)>, sqlx::Error> {
    sqlx::query_as::<_, (i64, String)>(
        "SELECT id, created_at FROM bookings
          WHERE machine_id = ? AND id <> ?
            AND status IN ('approved', 'active')
            AND start_date <= ? AND end_date >= ?
          ORDER BY start_date
          LIMIT 1",
    )
    .bind(machine_id)
    .bind(except_id)
    .bind(end)
    .bind(start)
    .fetch_optional(conn)
    .await
}

fn number_from(id: i64, created_at: &str) -> String {
    let year = created_at.get(0..4).and_then(|y| y.parse().ok()).unwrap_or(2026);
    format::booking_number(year, id)
}

/// `POST /mashini/{slug}/zayavka`: saves a new request (status „Нова“).
///
/// Inside one `BEGIN IMMEDIATE` transaction: if an approved or active booking
/// already covers any of the days → `AppError::Conflict` („Машината вече е
/// заета в избрания период…“), nothing is written. Returns the new id.
pub async fn create_checked(db: &SqlitePool, b: &NewBooking) -> Result<i64, AppError> {
    // IMMEDIATE takes SQLite's write lock now, so check + insert are atomic.
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    if blocking_clash(&mut tx, b.machine_id, b.start, b.end, 0).await?.is_some() {
        return Err(AppError::Conflict(MACHINE_TAKEN.to_string()));
    }
    let p = &b.price;
    let id = sqlx::query(
        "INSERT INTO bookings (machine_id, client_id, start_date, end_date, with_operator,
                               delivery_km, delivery_address, client_note, status, days,
                               weekend_days, equipment_cents, discount_percent, discount_cents,
                               operator_cents, transport_cents, net_cents, vat_cents, total_cents)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'new', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(b.machine_id)
    .bind(b.client_id)
    .bind(b.start)
    .bind(b.end)
    .bind(b.with_operator)
    .bind(b.delivery_km)
    .bind(&b.delivery_address)
    .bind(&b.client_note)
    .bind(p.days)
    .bind(p.weekend_days)
    .bind(p.equipment_cents)
    .bind(p.discount_percent)
    .bind(p.discount_cents)
    .bind(p.operator_cents)
    .bind(p.transport_cents)
    .bind(p.net_cents)
    .bind(p.vat_cents)
    .bind(p.total_cents)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    for (attachment_id, price) in &b.attachments {
        sqlx::query(
            "INSERT INTO booking_attachments (booking_id, attachment_id, price_per_day_cents) VALUES (?, ?, ?)",
        )
        .bind(id)
        .bind(attachment_id)
        .bind(price)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(id)
}

/// `POST /admin/zayavki/{id}/odobri`: the decisive moment against double booking.
///
/// In one `BEGIN IMMEDIATE` transaction: the booking must still be „Нова“ and no
/// approved/active booking of the same machine may overlap it; then it becomes
/// „Одобрена“. Two admins clicking at once are serialised by the write lock.
pub async fn approve_checked(db: &SqlitePool, id: i64) -> Result<(), AppError> {
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    let row: Option<(i64, NaiveDate, NaiveDate, BookingStatus)> =
        sqlx::query_as("SELECT machine_id, start_date, end_date, status FROM bookings WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((machine_id, start, end, status)) = row else {
        return Err(AppError::NotFound);
    };
    if status != BookingStatus::New {
        return Err(AppError::Conflict(ALREADY_PROCESSED.to_string()));
    }
    if let Some((other_id, created_at)) = blocking_clash(&mut tx, machine_id, start, end, id).await? {
        return Err(AppError::Conflict(format!(
            "Не може да се одобри: застъпва се със заявка № {}.",
            number_from(other_id, &created_at)
        )));
    }
    sqlx::query(
        "UPDATE bookings SET status = 'approved', updated_at = datetime('now')
          WHERE id = ? AND status = 'new'",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// After a guarded UPDATE changed nothing: 404 if the booking does not exist,
/// otherwise 409 „Заявката вече е обработена.“
async fn nothing_changed(db: &SqlitePool, id: i64) -> AppError {
    match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM bookings WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await
    {
        Ok(0) => AppError::NotFound,
        Ok(_) => AppError::Conflict(ALREADY_PROCESSED.to_string()),
        Err(e) => AppError::Db(e),
    }
}

/// `POST /admin/zayavki/{id}/otkazhi`: „Нова“ or „Одобрена“ → „Отказана“.
/// A non-empty `admin_note` replaces the current note (the reason for the client).
pub async fn decline(db: &SqlitePool, id: i64, admin_note: Option<&str>) -> Result<(), AppError> {
    let result = sqlx::query(
        "UPDATE bookings SET status = 'declined', admin_note = COALESCE(?, admin_note),
                             updated_at = datetime('now')
          WHERE id = ? AND status IN ('new', 'approved')",
    )
    .bind(admin_note.map(str::trim).filter(|n| !n.is_empty()))
    .bind(id)
    .execute(db)
    .await?;
    if result.rows_affected() == 0 {
        return Err(nothing_changed(db, id).await);
    }
    Ok(())
}

/// `POST /moite-zayavki/{id}/otkaz`: the client withdraws their own request,
/// only while it is „Нова“. Someone else's booking → 404.
pub async fn cancel_by_client(db: &SqlitePool, id: i64, client_id: i64) -> Result<(), AppError> {
    let result = sqlx::query(
        "UPDATE bookings SET status = 'cancelled', updated_at = datetime('now')
          WHERE id = ? AND client_id = ? AND status = 'new'",
    )
    .bind(id)
    .bind(client_id)
    .execute(db)
    .await?;
    if result.rows_affected() == 1 {
        return Ok(());
    }
    let owned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE id = ? AND client_id = ?")
        .bind(id)
        .bind(client_id)
        .fetch_one(db)
        .await?;
    if owned == 0 {
        Err(AppError::NotFound)
    } else {
        Err(AppError::Conflict(
            "Може да откажете само заявка със статус „Нова“. За промяна се обадете на 032 64 12 80.".to_string(),
        ))
    }
}

/// `POST /admin/zayavki/{id}/start`: „Одобрена“ → „Активна“ (the machine left
/// the yard); remembers the engine hours at the start.
pub async fn start(db: &SqlitePool, id: i64) -> Result<(), AppError> {
    let result = sqlx::query(
        "UPDATE bookings
            SET status = 'active',
                hours_at_start = (SELECT engine_hours FROM machines WHERE machines.id = bookings.machine_id),
                updated_at = datetime('now')
          WHERE id = ? AND status = 'approved'",
    )
    .bind(id)
    .execute(db)
    .await?;
    if result.rows_affected() == 0 {
        return Err(nothing_changed(db, id).await);
    }
    Ok(())
}

/// `POST /admin/zayavki/{id}/priklyuchi`: „Активна“ → „Приключена“.
/// Saves „Моточасове при връщане“ on the booking and on the machine in the
/// same transaction; they may not be lower than the machine's current hours.
pub async fn complete(db: &SqlitePool, id: i64, hours_at_end: i64) -> Result<(), AppError> {
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    let row: Option<(i64, BookingStatus, i64)> = sqlx::query_as(
        "SELECT b.machine_id, b.status, m.engine_hours
           FROM bookings b JOIN machines m ON m.id = b.machine_id
          WHERE b.id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((machine_id, status, engine_hours)) = row else {
        return Err(AppError::NotFound);
    };
    if status != BookingStatus::Active {
        return Err(AppError::Conflict(ALREADY_PROCESSED.to_string()));
    }
    if hours_at_end < engine_hours {
        return Err(AppError::BadRequest(format!(
            "Моточасовете при връщане не може да са по-малко от сегашните ({}).",
            format::number(engine_hours)
        )));
    }
    sqlx::query(
        "UPDATE bookings SET status = 'completed', hours_at_end = ?, updated_at = datetime('now')
          WHERE id = ? AND status = 'active'",
    )
    .bind(hours_at_end)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE machines SET engine_hours = ? WHERE id = ?")
        .bind(hours_at_end)
        .bind(machine_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// `POST /admin/zayavki/{id}/belezhka`: an empty note removes it.
pub async fn save_admin_note(db: &SqlitePool, id: i64, note: &str) -> Result<(), AppError> {
    let note = Some(note.trim()).filter(|n| !n.is_empty());
    let result = sqlx::query("UPDATE bookings SET admin_note = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(note)
        .bind(id)
        .execute(db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

// ── Reading ─────────────────────────────────────────────────────────────────

/// `GET /admin/zayavki/{id}`, `GET /zayavki/{id}/oferta` for an admin.
pub async fn by_id(db: &SqlitePool, id: i64) -> Result<Option<Booking>, sqlx::Error> {
    sqlx::query_as::<_, Booking>(concat!(booking_select!(), " WHERE b.id = ?"))
        .bind(id)
        .fetch_optional(db)
        .await
}

/// `GET /moite-zayavki/{id}`, `GET /zayavki/{id}/oferta` for a client: only
/// the client's own booking, otherwise `None` (→ 404, existence not revealed).
pub async fn by_id_for_client(db: &SqlitePool, id: i64, client_id: i64) -> Result<Option<Booking>, sqlx::Error> {
    sqlx::query_as::<_, Booking>(concat!(booking_select!(), " WHERE b.id = ? AND b.client_id = ?"))
        .bind(id)
        .bind(client_id)
        .fetch_optional(db)
        .await
}

/// `GET /moite-zayavki` and `GET /admin/klienti/{id}`: newest first.
pub async fn for_client(db: &SqlitePool, client_id: i64) -> Result<Vec<Booking>, sqlx::Error> {
    sqlx::query_as::<_, Booking>(concat!(
        booking_select!(),
        " WHERE b.client_id = ? ORDER BY b.created_at DESC, b.id DESC"
    ))
    .bind(client_id)
    .fetch_all(db)
    .await
}

/// Filters of `GET /admin/zayavki?status=&mashina=`.
#[derive(Debug, Clone, Default)]
pub struct AdminBookingFilter {
    pub status: Option<BookingStatus>,
    pub machine_id: Option<i64>,
    pub client_id: Option<i64>,
    /// `None` = all rows.
    pub limit: Option<i64>,
    pub offset: i64,
}

/// `GET /admin/zayavki`: open requests first (new, approved, active – the
/// soonest start first), then the history (newest first).
pub async fn admin_list(db: &SqlitePool, filter: &AdminBookingFilter) -> Result<Vec<Booking>, sqlx::Error> {
    sqlx::query_as::<_, Booking>(concat!(
        booking_select!(),
        " WHERE (?1 IS NULL OR b.status = ?1)
            AND (?2 IS NULL OR b.machine_id = ?2)
            AND (?3 IS NULL OR b.client_id = ?3)
          ORDER BY CASE WHEN b.status IN ('new', 'approved', 'active') THEN 0 ELSE 1 END,
                   CASE WHEN b.status IN ('new', 'approved', 'active') THEN b.start_date END ASC,
                   b.start_date DESC, b.id DESC
          LIMIT ?4 OFFSET ?5"
    ))
    .bind(filter.status)
    .bind(filter.machine_id)
    .bind(filter.client_id)
    .bind(filter.limit.unwrap_or(-1)) // SQLite: LIMIT -1 = no limit
    .bind(filter.offset)
    .fetch_all(db)
    .await
}

/// Total for the same filter (pagination, „Намерихме 12 заявки“).
pub async fn admin_count(db: &SqlitePool, filter: &AdminBookingFilter) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM bookings b
          WHERE (?1 IS NULL OR b.status = ?1)
            AND (?2 IS NULL OR b.machine_id = ?2)
            AND (?3 IS NULL OR b.client_id = ?3)",
    )
    .bind(filter.status)
    .bind(filter.machine_id)
    .bind(filter.client_id)
    .fetch_one(db)
    .await
}

/// Attachments chosen for a booking, with their price snapshot
/// (booking detail pages, print quote).
pub async fn attachments_of(db: &SqlitePool, booking_id: i64) -> Result<Vec<BookingAttachment>, sqlx::Error> {
    sqlx::query_as::<_, BookingAttachment>(
        "SELECT ba.attachment_id, a.slug, a.name, a.kind, ba.price_per_day_cents
           FROM booking_attachments ba
           JOIN attachments a ON a.id = ba.attachment_id
          WHERE ba.booking_id = ?
          ORDER BY a.id",
    )
    .bind(booking_id)
    .fetch_all(db)
    .await
}

/// `GET /admin/zayavki/{id}`: other new/approved/active bookings of the same
/// machine whose dates overlap (the conflict callout).
pub async fn conflicts_for(db: &SqlitePool, booking: &Booking) -> Result<Vec<Booking>, sqlx::Error> {
    sqlx::query_as::<_, Booking>(concat!(
        booking_select!(),
        " WHERE b.machine_id = ? AND b.id <> ?
            AND b.status IN ('new', 'approved', 'active')
            AND b.start_date <= ? AND b.end_date >= ?
          ORDER BY b.start_date, b.id"
    ))
    .bind(booking.machine_id)
    .bind(booking.id)
    .bind(booking.end_date)
    .bind(booking.start_date)
    .fetch_all(db)
    .await
}

/// Date ranges of approved/active bookings of one machine that touch
/// [from, to]: machine calendar, `/api/mashini/{slug}/zaetost`, availability badge.
pub async fn blocking_spans(db: &SqlitePool, machine_id: i64, from: NaiveDate, to: NaiveDate) -> Result<Vec<(NaiveDate, NaiveDate)>, sqlx::Error> {
    sqlx::query_as::<_, (NaiveDate, NaiveDate)>(
        "SELECT start_date, end_date FROM bookings
          WHERE machine_id = ? AND status IN ('approved', 'active')
            AND start_date <= ? AND end_date >= ?
          ORDER BY start_date",
    )
    .bind(machine_id)
    .bind(to)
    .bind(from)
    .fetch_all(db)
    .await
}

/// Same for all machines at once (catalogue cards, price list badges):
/// rows of (machine id, start, end).
pub async fn blocking_spans_all(db: &SqlitePool, from: NaiveDate, to: NaiveDate) -> Result<Vec<(i64, NaiveDate, NaiveDate)>, sqlx::Error> {
    sqlx::query_as::<_, (i64, NaiveDate, NaiveDate)>(
        "SELECT machine_id, start_date, end_date FROM bookings
          WHERE status IN ('approved', 'active') AND start_date <= ? AND end_date >= ?
          ORDER BY machine_id, start_date",
    )
    .bind(to)
    .bind(from)
    .fetch_all(db)
    .await
}

/// One bar in the admin fleet calendar.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FleetSpan {
    pub booking_id: i64,
    pub machine_id: i64,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub status: BookingStatus,
    pub client_name: String,
    pub client_company: Option<String>,
    pub created_at: String,
}

impl FleetSpan {
    pub fn number(&self) -> String {
        number_from(self.booking_id, &self.created_at)
    }
}

/// `GET /admin/kalendar?mesec=`: new, approved, active and completed bookings
/// touching [from, to] (new ones are drawn hatched, they don't block).
pub async fn fleet_spans(db: &SqlitePool, from: NaiveDate, to: NaiveDate) -> Result<Vec<FleetSpan>, sqlx::Error> {
    sqlx::query_as::<_, FleetSpan>(
        "SELECT b.id AS booking_id, b.machine_id, b.start_date, b.end_date, b.status,
                u.full_name AS client_name, u.company_name AS client_company, b.created_at
           FROM bookings b JOIN users u ON u.id = b.client_id
          WHERE b.status IN ('new', 'approved', 'active', 'completed')
            AND b.start_date <= ? AND b.end_date >= ?
          ORDER BY b.machine_id, b.start_date, b.id",
    )
    .bind(to)
    .bind(from)
    .fetch_all(db)
    .await
}

/// Admin navigation counters („Заявки 4“) and dashboard tile.
pub async fn count_by_status(db: &SqlitePool, status: BookingStatus) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE status = ?")
        .bind(status)
        .fetch_one(db)
        .await
}

/// Dashboard tile „Нови заявки 4 (1 се застъпва с одобрена)“: new requests
/// that overlap an approved/active booking of the same machine.
pub async fn new_conflicting_count(db: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM bookings n
          WHERE n.status = 'new'
            AND EXISTS (SELECT 1 FROM bookings o
                         WHERE o.machine_id = n.machine_id AND o.id <> n.id
                           AND o.status IN ('approved', 'active')
                           AND o.start_date <= n.end_date AND o.end_date >= n.start_date)",
    )
    .fetch_one(db)
    .await
}

/// Dashboard „Предстоящи наеми“: active rentals and approved ones starting
/// from today, soonest first.
pub async fn upcoming(db: &SqlitePool, today: NaiveDate, limit: i64) -> Result<Vec<Booking>, sqlx::Error> {
    sqlx::query_as::<_, Booking>(concat!(
        booking_select!(),
        " WHERE b.status = 'active' OR (b.status = 'approved' AND b.start_date >= ?)
          ORDER BY b.start_date, b.id
          LIMIT ?"
    ))
    .bind(today)
    .bind(limit)
    .fetch_all(db)
    .await
}
