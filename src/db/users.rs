//! Users: clients and administrators.

use sqlx::SqlitePool;

use crate::models::{Role, User};

/// Data for a new account.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub email: String,
    pub password_hash: String,
    pub full_name: String,
    pub phone: String,
    pub company_name: Option<String>,
    pub company_eik: Option<String>,
    pub role: Role,
}

/// Editable profile fields.
#[derive(Debug, Clone)]
pub struct ProfileUpdate {
    pub full_name: String,
    pub phone: String,
    pub company_name: Option<String>,
    pub company_eik: Option<String>,
}

/// A client with booking totals (admin clients list and detail).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClientSummary {
    pub id: i64,
    pub email: String,
    pub full_name: String,
    pub phone: String,
    pub company_name: Option<String>,
    pub company_eik: Option<String>,
    pub created_at: String,
    /// All requests ever sent.
    pub bookings_count: i64,
    /// Completed + active rentals.
    pub rentals_count: i64,
    /// Sum of `net_cents` (без ДДС) of completed and active rentals.
    pub total_net_cents: i64,
    /// Start date of the latest request, if any (`YYYY-MM-DD`).
    pub last_booking: Option<String>,
}

impl ClientSummary {
    pub fn display_name(&self) -> &str {
        self.company_name.as_deref().unwrap_or(&self.full_name)
    }
}

/// Login (`POST /vhod`, via `auth::authenticate`). E-mail comparison ignores case.
pub async fn find_by_email(db: &SqlitePool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, full_name, phone, company_name, company_eik, role, created_at
           FROM users WHERE email = ?",
    )
    .bind(email.trim())
    .fetch_optional(db)
    .await
}

/// `CurrentUser` / `RequireClient` / `RequireAdmin` extractors (every request).
pub async fn find_by_id(db: &SqlitePool, id: i64) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, full_name, phone, company_name, company_eik, role, created_at
           FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await
}

/// `POST /registratsia`: „Вече има профил с този имейл.“
pub async fn email_exists(db: &SqlitePool, email: &str) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = ?")
        .bind(email.trim())
        .fetch_one(db)
        .await?;
    Ok(count > 0)
}

/// `POST /registratsia`, seed, tests. Returns the new id.
pub async fn create(db: &SqlitePool, user: &NewUser) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO users (email, password_hash, full_name, phone, company_name, company_eik, role)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user.email.trim())
    .bind(&user.password_hash)
    .bind(user.full_name.trim())
    .bind(user.phone.trim())
    .bind(&user.company_name)
    .bind(&user.company_eik)
    .bind(user.role)
    .execute(db)
    .await?;
    Ok(result.last_insert_rowid())
}

/// `POST /profil`.
pub async fn update_profile(db: &SqlitePool, id: i64, profile: &ProfileUpdate) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE users SET full_name = ?, phone = ?, company_name = ?, company_eik = ? WHERE id = ?",
    )
    .bind(profile.full_name.trim())
    .bind(profile.phone.trim())
    .bind(&profile.company_name)
    .bind(&profile.company_eik)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

/// `POST /profil` (optional new password).
pub async fn update_password(db: &SqlitePool, id: i64, password_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(password_hash)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// Start-up: seed demo data only when there are no users yet.
pub async fn count(db: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(db).await
}

// The totals are computed with sub-selects so each client is one row.
macro_rules! client_summary_select {
    () => {
        "SELECT u.id, u.email, u.full_name, u.phone, u.company_name, u.company_eik, u.created_at,
                (SELECT COUNT(*) FROM bookings b WHERE b.client_id = u.id) AS bookings_count,
                (SELECT COUNT(*) FROM bookings b
                  WHERE b.client_id = u.id AND b.status IN ('active', 'completed')) AS rentals_count,
                (SELECT COALESCE(SUM(b.net_cents), 0) FROM bookings b
                  WHERE b.client_id = u.id AND b.status IN ('active', 'completed')) AS total_net_cents,
                (SELECT MAX(b.start_date) FROM bookings b WHERE b.client_id = u.id) AS last_booking
           FROM users u
          WHERE u.role = 'client'"
    };
}

/// `GET /admin/klienti`: all clients, biggest spenders first.
pub async fn list_clients(db: &SqlitePool) -> Result<Vec<ClientSummary>, sqlx::Error> {
    sqlx::query_as::<_, ClientSummary>(concat!(
        client_summary_select!(),
        " ORDER BY total_net_cents DESC, u.full_name ASC, u.id ASC"
    ))
    .fetch_all(db)
    .await
}

/// `GET /admin/klienti/{id}`: one client with totals (`None` for unknown ids and admins).
pub async fn client_summary(db: &SqlitePool, id: i64) -> Result<Option<ClientSummary>, sqlx::Error> {
    sqlx::query_as::<_, ClientSummary>(concat!(client_summary_select!(), " AND u.id = ?"))
        .bind(id)
        .fetch_optional(db)
        .await
}
