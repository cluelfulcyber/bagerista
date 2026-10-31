//! Server-side session storage in our own SQLite table `sessions`.
//!
//! tower-sessions handles the cookie (a random 128-bit id); this store only
//! saves, loads and deletes the session data. Each row holds the whole
//! session record as JSON plus its expiry time in unix seconds.

use async_trait::async_trait;
use sqlx::SqlitePool;
use tower_sessions::cookie::time::OffsetDateTime;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::{self, SessionStore};

#[derive(Debug, Clone)]
pub struct SqliteStore {
    pub db: SqlitePool,
}

impl SqliteStore {
    pub fn new(db: SqlitePool) -> SqliteStore {
        SqliteStore { db }
    }
}

fn backend_error(err: sqlx::Error) -> session_store::Error {
    session_store::Error::Backend(err.to_string())
}

fn encode(record: &Record) -> session_store::Result<String> {
    serde_json::to_string(record).map_err(|e| session_store::Error::Encode(e.to_string()))
}

#[async_trait]
impl SessionStore for SqliteStore {
    /// Inserts a new session; if the random id is already taken (astronomically
    /// unlikely), a new id is drawn and we try again.
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        loop {
            let result = sqlx::query("INSERT INTO sessions (id, data, expires_at) VALUES (?, ?, ?)")
                .bind(record.id.to_string())
                .bind(encode(record)?)
                .bind(record.expiry_date.unix_timestamp())
                .execute(&self.db)
                .await;
            match result {
                Ok(_) => return Ok(()),
                Err(sqlx::Error::Database(e)) if e.is_unique_violation() => record.id = Id::default(),
                Err(e) => return Err(backend_error(e)),
            }
        }
    }

    /// Inserts or replaces the session row.
    async fn save(&self, record: &Record) -> session_store::Result<()> {
        sqlx::query(
            "INSERT INTO sessions (id, data, expires_at) VALUES (?, ?, ?)
             ON CONFLICT (id) DO UPDATE SET data = excluded.data, expires_at = excluded.expires_at",
        )
        .bind(record.id.to_string())
        .bind(encode(record)?)
        .bind(record.expiry_date.unix_timestamp())
        .execute(&self.db)
        .await
        .map_err(backend_error)?;
        Ok(())
    }

    /// Loads a session that has not expired yet.
    async fn load(&self, id: &Id) -> session_store::Result<Option<Record>> {
        let data: Option<String> =
            sqlx::query_scalar("SELECT data FROM sessions WHERE id = ? AND expires_at > ?")
                .bind(id.to_string())
                .bind(OffsetDateTime::now_utc().unix_timestamp())
                .fetch_optional(&self.db)
                .await
                .map_err(backend_error)?;
        match data {
            Some(json) => serde_json::from_str(&json)
                .map(Some)
                .map_err(|e| session_store::Error::Decode(e.to_string())),
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &Id) -> session_store::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.db)
            .await
            .map_err(backend_error)?;
        Ok(())
    }
}

/// Removes expired sessions (called at start-up and on every login).
pub async fn delete_expired(db: &SqlitePool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
        .bind(OffsetDateTime::now_utc().unix_timestamp())
        .execute(db)
        .await?;
    Ok(result.rows_affected())
}
