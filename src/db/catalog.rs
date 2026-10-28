//! Catalogue: categories, machines, attachments, machine ↔ attachment links, photos.

use sqlx::SqlitePool;

use crate::models::{Attachment, AttachmentKind, Category, Illustration, Machine, MachinePhoto, Undercarriage};
use crate::services::wizard::MachineFacts;

// ── Categories ──────────────────────────────────────────────────────────────

/// Footer, catalogue filter, admin machine form.
pub async fn list_categories(db: &SqlitePool) -> Result<Vec<Category>, sqlx::Error> {
    sqlx::query_as::<_, Category>(
        "SELECT id, slug, name, description, undercarriage, sort_order
           FROM categories ORDER BY sort_order, id",
    )
    .fetch_all(db)
    .await
}

/// A category with numbers about its active machines.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CategoryStats {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub undercarriage: Undercarriage,
    pub machine_count: i64,
    pub min_weight_kg: Option<i64>,
    pub max_weight_kg: Option<i64>,
    pub min_price_cents: Option<i64>,
    pub max_depth_mm: Option<i64>,
    pub illustration: Option<Illustration>,
}

/// Home page category ruler („1 120–2 600 кг, 4 машини, от 75,00 €“), the
/// catalogue filter counts and the wizard soil chart (deepest per class).
pub async fn category_stats(db: &SqlitePool) -> Result<Vec<CategoryStats>, sqlx::Error> {
    sqlx::query_as::<_, CategoryStats>(
        "SELECT c.id, c.slug, c.name, c.description, c.undercarriage,
                COUNT(m.id) AS machine_count,
                MIN(m.weight_kg) AS min_weight_kg, MAX(m.weight_kg) AS max_weight_kg,
                MIN(m.price_per_day_cents) AS min_price_cents,
                MAX(m.dig_depth_mm) AS max_depth_mm,
                MAX(m.illustration) AS illustration
           FROM categories c
           LEFT JOIN machines m ON m.category_id = c.id AND m.active = 1
          GROUP BY c.id
          ORDER BY c.sort_order, c.id",
    )
    .fetch_all(db)
    .await
}

/// Numbers for the hero sentence „14 машини от 1 120 до 30 000 кг“.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FleetSummary {
    pub machine_count: i64,
    pub min_weight_kg: Option<i64>,
    pub max_weight_kg: Option<i64>,
}

/// Home page hero.
pub async fn fleet_summary(db: &SqlitePool) -> Result<FleetSummary, sqlx::Error> {
    sqlx::query_as::<_, FleetSummary>(
        "SELECT COUNT(*) AS machine_count, MIN(weight_kg) AS min_weight_kg, MAX(weight_kg) AS max_weight_kg
           FROM machines WHERE active = 1",
    )
    .fetch_one(db)
    .await
}

// ── Machines ────────────────────────────────────────────────────────────────

/// Sort order of the catalogue (`?sort=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CatalogSort {
    /// `cena` – cheapest first (default)
    #[default]
    Price,
    /// `teglo` – lightest first
    Weight,
    /// `dalbochina` – deepest digging first
    Depth,
}

impl CatalogSort {
    pub fn param(self) -> &'static str {
        match self {
            CatalogSort::Price => "cena",
            CatalogSort::Weight => "teglo",
            CatalogSort::Depth => "dalbochina",
        }
    }

    /// Unknown values fall back to the default.
    pub fn parse(param: &str) -> CatalogSort {
        match param {
            "teglo" => CatalogSort::Weight,
            "dalbochina" => CatalogSort::Depth,
            _ => CatalogSort::Price,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CatalogSort::Price => "Първо най-евтините",
            CatalogSort::Weight => "Първо най-леките",
            CatalogSort::Depth => "Първо копаещите най-дълбоко",
        }
    }
}

/// Catalogue filters (`/katalog?kategoria=&max_teglo=&min_dalbochina=&max_cena=&sort=`).
/// `None` means „без значение“.
#[derive(Debug, Clone, Default)]
pub struct CatalogFilter {
    pub category_slug: Option<String>,
    pub max_weight_kg: Option<i64>,
    pub min_depth_mm: Option<i64>,
    pub max_price_cents: Option<i64>,
    pub sort: CatalogSort,
    /// Admin list shows deactivated machines too; the public catalogue never does.
    pub include_inactive: bool,
}

/// `GET /katalog` (and its htmx partial), `GET /admin/mashini` (with `include_inactive`).
///
/// Every filter is written as `(?n IS NULL OR condition)`, so one fixed SQL
/// text serves all combinations: an empty filter simply switches itself off.
pub async fn list_machines(db: &SqlitePool, filter: &CatalogFilter) -> Result<Vec<Machine>, sqlx::Error> {
    sqlx::query_as::<_, Machine>(concat!(
        machine_select!(),
        " WHERE (?1 IS NULL OR c.slug = ?1)
            AND (?2 IS NULL OR m.weight_kg <= ?2)
            AND (?3 IS NULL OR m.dig_depth_mm >= ?3)
            AND (?4 IS NULL OR m.price_per_day_cents <= ?4)
            AND (?5 = 1 OR m.active = 1)
          ORDER BY CASE WHEN ?6 = 'teglo' THEN m.weight_kg END ASC,
                   CASE WHEN ?6 = 'dalbochina' THEN m.dig_depth_mm END DESC,
                   m.price_per_day_cents ASC, m.id ASC"
    ))
    .bind(&filter.category_slug)
    .bind(filter.max_weight_kg)
    .bind(filter.min_depth_mm)
    .bind(filter.max_price_cents)
    .bind(filter.include_inactive)
    .bind(filter.sort.param())
    .fetch_all(db)
    .await
}

/// `GET /mashini/{slug}`, its calendar partial and the two JSON APIs.
/// Only active machines: a deactivated one gives 404 on the public site.
pub async fn machine_by_slug(db: &SqlitePool, slug: &str) -> Result<Option<Machine>, sqlx::Error> {
    sqlx::query_as::<_, Machine>(concat!(machine_select!(), " WHERE m.slug = ? AND m.active = 1"))
        .bind(slug)
        .fetch_optional(db)
        .await
}

/// Admin pages (`/admin/mashini/{id}` …), active or not.
pub async fn machine_by_id(db: &SqlitePool, id: i64) -> Result<Option<Machine>, sqlx::Error> {
    sqlx::query_as::<_, Machine>(concat!(machine_select!(), " WHERE m.id = ?"))
        .bind(id)
        .fetch_optional(db)
        .await
}

/// Machine page „Подобни машини“: the same class first (catalogue order),
/// then machines with the closest price per day.
pub async fn similar_machines(db: &SqlitePool, machine: &Machine, limit: i64) -> Result<Vec<Machine>, sqlx::Error> {
    sqlx::query_as::<_, Machine>(concat!(
        machine_select!(),
        " WHERE m.active = 1 AND m.id <> ?1
          ORDER BY (m.category_id = ?2) DESC,
                   CASE WHEN m.category_id = ?2 THEN m.id ELSE ABS(m.price_per_day_cents - ?3) END ASC,
                   m.id ASC
          LIMIT ?4"
    ))
    .bind(machine.id)
    .bind(machine.category_id)
    .bind(machine.price_per_day_cents)
    .bind(limit)
    .fetch_all(db)
    .await
}

/// Machine page „За какво става“: for a job deeper than this machine digs,
/// „Не стига. Вижте Takeuchi TB290, копае до 4,60 м.“ – the lightest machine
/// that reaches `min_depth_mm`, preferring the same class.
pub async fn lightest_reaching(db: &SqlitePool, min_depth_mm: i64, same_category_id: i64) -> Result<Option<Machine>, sqlx::Error> {
    sqlx::query_as::<_, Machine>(concat!(
        machine_select!(),
        " WHERE m.active = 1 AND m.dig_depth_mm >= ?1
          ORDER BY (m.category_id = ?2) DESC, m.weight_kg ASC, m.id ASC
          LIMIT 1"
    ))
    .bind(min_depth_mm)
    .bind(same_category_id)
    .fetch_optional(db)
    .await
}

/// Machine page „Достъп до обекта“: „Ако входът ви е по-тесен, вижте
/// Yanmar ViO26-6 (1,50 м) или Kubota KX019-4 (0,99 м).“ – widest first.
pub async fn narrower_machines(db: &SqlitePool, width_mm: i64, limit: i64) -> Result<Vec<Machine>, sqlx::Error> {
    sqlx::query_as::<_, Machine>(concat!(
        machine_select!(),
        " WHERE m.active = 1 AND m.width_mm < ?
          ORDER BY m.width_mm DESC, m.id ASC
          LIMIT ?"
    ))
    .bind(width_mm)
    .bind(limit)
    .fetch_all(db)
    .await
}

/// Every field of the admin machine form.
#[derive(Debug, Clone)]
pub struct MachineInput {
    pub slug: String,
    pub category_id: i64,
    pub name: String,
    pub brand: String,
    pub model: String,
    pub year_made: i64,
    pub weight_kg: i64,
    pub dig_depth_mm: i64,
    pub width_mm: i64,
    pub height_mm: i64,
    pub engine_kw: f64,
    pub bucket_l: i64,
    pub price_per_day_cents: i64,
    pub operator_price_per_day_cents: i64,
    pub engine_hours: i64,
    pub service_interval_hours: i64,
    pub last_service_hours: i64,
    pub description: String,
    pub illustration: Illustration,
    pub active: bool,
}

/// `POST /admin/mashini`, seed, tests. Returns the new id.
pub async fn create_machine(db: &SqlitePool, m: &MachineInput) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO machines (slug, category_id, name, brand, model, year_made, weight_kg,
                               dig_depth_mm, width_mm, height_mm, engine_kw, bucket_l,
                               price_per_day_cents, operator_price_per_day_cents, engine_hours,
                               service_interval_hours, last_service_hours, description,
                               illustration, active)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&m.slug)
    .bind(m.category_id)
    .bind(&m.name)
    .bind(&m.brand)
    .bind(&m.model)
    .bind(m.year_made)
    .bind(m.weight_kg)
    .bind(m.dig_depth_mm)
    .bind(m.width_mm)
    .bind(m.height_mm)
    .bind(m.engine_kw)
    .bind(m.bucket_l)
    .bind(m.price_per_day_cents)
    .bind(m.operator_price_per_day_cents)
    .bind(m.engine_hours)
    .bind(m.service_interval_hours)
    .bind(m.last_service_hours)
    .bind(&m.description)
    .bind(m.illustration)
    .bind(m.active)
    .execute(db)
    .await?;
    Ok(result.last_insert_rowid())
}

/// `POST /admin/mashini/{id}`. Returns false if the machine does not exist.
pub async fn update_machine(db: &SqlitePool, id: i64, m: &MachineInput) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE machines
            SET slug = ?, category_id = ?, name = ?, brand = ?, model = ?, year_made = ?,
                weight_kg = ?, dig_depth_mm = ?, width_mm = ?, height_mm = ?, engine_kw = ?,
                bucket_l = ?, price_per_day_cents = ?, operator_price_per_day_cents = ?,
                engine_hours = ?, service_interval_hours = ?, last_service_hours = ?,
                description = ?, illustration = ?, active = ?
          WHERE id = ?",
    )
    .bind(&m.slug)
    .bind(m.category_id)
    .bind(&m.name)
    .bind(&m.brand)
    .bind(&m.model)
    .bind(m.year_made)
    .bind(m.weight_kg)
    .bind(m.dig_depth_mm)
    .bind(m.width_mm)
    .bind(m.height_mm)
    .bind(m.engine_kw)
    .bind(m.bucket_l)
    .bind(m.price_per_day_cents)
    .bind(m.operator_price_per_day_cents)
    .bind(m.engine_hours)
    .bind(m.service_interval_hours)
    .bind(m.last_service_hours)
    .bind(&m.description)
    .bind(m.illustration)
    .bind(m.active)
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// `POST /admin/mashini/{id}/aktivnost`: soft delete / restore (machines with
/// bookings are never deleted). Returns the new state, `None` if not found.
pub async fn toggle_active(db: &SqlitePool, id: i64) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar("UPDATE machines SET active = 1 - active WHERE id = ? RETURNING active")
        .bind(id)
        .fetch_optional(db)
        .await
}

/// Admin form validation: „Този адрес (slug) вече се използва.“
/// `except_id` skips the machine being edited.
pub async fn slug_taken(db: &SqlitePool, slug: &str, except_id: Option<i64>) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM machines WHERE slug = ? AND id IS NOT ?")
        .bind(slug)
        .bind(except_id)
        .fetch_one(db)
        .await?;
    Ok(count > 0)
}

#[derive(sqlx::FromRow)]
struct FactsRow {
    id: i64,
    name: String,
    illustration: Illustration,
    undercarriage: Undercarriage,
    dig_depth_mm: i64,
    width_mm: i64,
    weight_kg: i64,
    price_per_day_cents: i64,
    has_hammer: bool,
}

/// `GET /pomoshtnik/rezultat`: the facts the wizard scores, for active machines.
pub async fn wizard_facts(db: &SqlitePool) -> Result<Vec<MachineFacts>, sqlx::Error> {
    let rows = sqlx::query_as::<_, FactsRow>(
        "SELECT m.id, m.name, m.illustration, c.undercarriage, m.dig_depth_mm, m.width_mm,
                m.weight_kg, m.price_per_day_cents,
                EXISTS (SELECT 1 FROM machine_attachments ma
                          JOIN attachments a ON a.id = ma.attachment_id
                         WHERE ma.machine_id = m.id AND a.kind = 'hammer' AND a.active = 1) AS has_hammer
           FROM machines m
           JOIN categories c ON c.id = m.category_id
          WHERE m.active = 1
          ORDER BY m.id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| MachineFacts {
            id: r.id,
            name: r.name,
            class: r.illustration,
            undercarriage: r.undercarriage,
            dig_depth_mm: r.dig_depth_mm,
            width_mm: r.width_mm,
            weight_kg: r.weight_kg,
            price_per_day_cents: r.price_per_day_cents,
            has_hammer: r.has_hammer,
        })
        .collect())
}

// ── Attachments ─────────────────────────────────────────────────────────────

/// Machine page checkboxes, price API (to reject attachments that don't fit),
/// booking POST. Active attachments that fit the machine.
pub async fn attachments_for_machine(db: &SqlitePool, machine_id: i64) -> Result<Vec<Attachment>, sqlx::Error> {
    sqlx::query_as::<_, Attachment>(
        "SELECT a.id, a.slug, a.name, a.description, a.kind, a.price_per_day_cents, a.active
           FROM attachments a
           JOIN machine_attachments ma ON ma.attachment_id = a.id
          WHERE ma.machine_id = ? AND a.active = 1
          ORDER BY a.id",
    )
    .bind(machine_id)
    .fetch_all(db)
    .await
}

/// `GET /admin/prikachni` and the admin machine form (all, active or not).
pub async fn list_attachments(db: &SqlitePool) -> Result<Vec<Attachment>, sqlx::Error> {
    sqlx::query_as::<_, Attachment>(
        "SELECT id, slug, name, description, kind, price_per_day_cents, active
           FROM attachments ORDER BY id",
    )
    .fetch_all(db)
    .await
}

/// Fields of the admin attachment form.
#[derive(Debug, Clone)]
pub struct AttachmentInput {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub kind: AttachmentKind,
    pub price_per_day_cents: i64,
    pub active: bool,
}

/// `POST /admin/prikachni`, seed. Returns the new id.
pub async fn create_attachment(db: &SqlitePool, a: &AttachmentInput) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO attachments (slug, name, description, kind, price_per_day_cents, active)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&a.slug)
    .bind(&a.name)
    .bind(&a.description)
    .bind(a.kind)
    .bind(a.price_per_day_cents)
    .bind(a.active)
    .execute(db)
    .await?;
    Ok(result.last_insert_rowid())
}

/// `POST /admin/prikachni/{id}`. Returns false if not found.
pub async fn update_attachment(db: &SqlitePool, id: i64, a: &AttachmentInput) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE attachments SET slug = ?, name = ?, description = ?, kind = ?,
                                price_per_day_cents = ?, active = ?
          WHERE id = ?",
    )
    .bind(&a.slug)
    .bind(&a.name)
    .bind(&a.description)
    .bind(a.kind)
    .bind(a.price_per_day_cents)
    .bind(a.active)
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Admin machine form: which attachment checkboxes are ticked.
pub async fn attachment_ids_for_machine(db: &SqlitePool, machine_id: i64) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT attachment_id FROM machine_attachments WHERE machine_id = ? ORDER BY attachment_id",
    )
    .bind(machine_id)
    .fetch_all(db)
    .await
}

/// `POST /admin/mashini/{id}` (and seed): replaces the list of compatible
/// attachments in one transaction.
pub async fn set_machine_attachments(db: &SqlitePool, machine_id: i64, attachment_ids: &[i64]) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM machine_attachments WHERE machine_id = ?")
        .bind(machine_id)
        .execute(&mut *tx)
        .await?;
    for attachment_id in attachment_ids {
        sqlx::query("INSERT OR IGNORE INTO machine_attachments (machine_id, attachment_id) VALUES (?, ?)")
            .bind(machine_id)
            .bind(attachment_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

// ── Photos ──────────────────────────────────────────────────────────────────

/// Machine page gallery and admin machine form (metadata only, no bytes).
pub async fn photos_for_machine(db: &SqlitePool, machine_id: i64) -> Result<Vec<MachinePhoto>, sqlx::Error> {
    sqlx::query_as::<_, MachinePhoto>(
        "SELECT id, machine_id, content_type, alt_text, sort_order
           FROM machine_photos WHERE machine_id = ? ORDER BY sort_order, id",
    )
    .bind(machine_id)
    .fetch_all(db)
    .await
}

/// First photo id per machine (catalogue cards may show it instead of the illustration).
pub async fn first_photo_ids(db: &SqlitePool) -> Result<Vec<(i64, i64)>, sqlx::Error> {
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT machine_id, MIN(id) FROM machine_photos GROUP BY machine_id ORDER BY machine_id",
    )
    .fetch_all(db)
    .await
}

/// `GET /media/snimki/{id}`: content type and image bytes.
pub async fn photo_data(db: &SqlitePool, photo_id: i64) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    sqlx::query_as::<_, (String, Vec<u8>)>("SELECT content_type, data FROM machine_photos WHERE id = ?")
        .bind(photo_id)
        .fetch_optional(db)
        .await
}

/// `POST /admin/mashini/{id}/snimki`: stores an uploaded photo (already checked:
/// ≤ 2 MB, jpeg/png/webp by magic bytes). New photos go to the end.
pub async fn add_photo(db: &SqlitePool, machine_id: i64, content_type: &str, data: &[u8], alt_text: &str) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO machine_photos (machine_id, content_type, data, alt_text, sort_order)
         VALUES (?, ?, ?, ?, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM machine_photos WHERE machine_id = ?))",
    )
    .bind(machine_id)
    .bind(content_type)
    .bind(data)
    .bind(alt_text)
    .bind(machine_id)
    .execute(db)
    .await?;
    Ok(result.last_insert_rowid())
}

/// `POST /admin/mashini/{id}/snimki/{photo_id}/iztriy`. Returns false if the
/// photo does not belong to that machine.
pub async fn delete_photo(db: &SqlitePool, machine_id: i64, photo_id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM machine_photos WHERE id = ? AND machine_id = ?")
        .bind(photo_id)
        .bind(machine_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() == 1)
}
