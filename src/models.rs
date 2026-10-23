//! Plain data types shared by all layers: rows read from the database and
//! the enums behind the status codes.
//!
//! Status codes are stored in English (`approved`); the Bulgarian text the user
//! sees comes from `label()` („Одобрена“), so wording can change without a
//! database migration.

use chrono::NaiveDate;
use serde::Serialize;

use crate::format;
use crate::services::maintenance::{self, MaintenanceStatus};

// ── People ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Client,
    Admin,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::Client => "Клиент",
            Role::Admin => "Администратор",
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub email: String,
    /// Argon2id PHC string; never shown and never serialised.
    pub password_hash: String,
    pub full_name: String,
    pub phone: String,
    pub company_name: Option<String>,
    pub company_eik: Option<String>,
    pub role: Role,
    pub created_at: String,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    /// Company name if there is one, otherwise the person's name.
    pub fn display_name(&self) -> &str {
        self.company_name.as_deref().unwrap_or(&self.full_name)
    }
}

// ── Catalogue ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Undercarriage {
    Tracks,
    Wheels,
}

impl Undercarriage {
    pub fn label(self) -> &'static str {
        match self {
            Undercarriage::Tracks => "Вериги",
            Undercarriage::Wheels => "Гуми",
        }
    }
}

/// The machine class. Its code is also the name of the category illustration
/// (`static/img/machines/{code}.svg`), stored in `machines.illustration`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Illustration {
    Mini,
    Midi,
    Crawler,
    Wheeled,
    Backhoe,
}

impl Illustration {
    /// Ordered by weight, like the category ruler on the home page.
    pub const ALL: [Illustration; 5] = [
        Illustration::Mini,
        Illustration::Midi,
        Illustration::Backhoe,
        Illustration::Wheeled,
        Illustration::Crawler,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Illustration::Mini => "mini",
            Illustration::Midi => "midi",
            Illustration::Crawler => "crawler",
            Illustration::Wheeled => "wheeled",
            Illustration::Backhoe => "backhoe",
        }
    }

    pub fn parse(code: &str) -> Option<Illustration> {
        Illustration::ALL.into_iter().find(|i| i.code() == code)
    }

    /// Singular class name: „Мидибагер“.
    pub fn class_label(self) -> &'static str {
        match self {
            Illustration::Mini => "Мини багер",
            Illustration::Midi => "Мидибагер",
            Illustration::Crawler => "Верижен багер",
            Illustration::Wheeled => "Колесен багер",
            Illustration::Backhoe => "Багер-товарач",
        }
    }

    /// Plural with the definite article, for sentences: „Мидибагерите“.
    pub fn plural_definite(self) -> &'static str {
        match self {
            Illustration::Mini => "Мини багерите",
            Illustration::Midi => "Мидибагерите",
            Illustration::Crawler => "Верижните багери",
            Illustration::Wheeled => "Колесните багери",
            Illustration::Backhoe => "Багер-товарачите",
        }
    }

    /// Two-letter code in the catalogue number „БГР-МД-005“ (design §6.7).
    pub fn catalog_code(self) -> &'static str {
        match self {
            Illustration::Mini => "МБ",
            Illustration::Midi => "МД",
            Illustration::Crawler => "ВБ",
            Illustration::Wheeled => "КБ",
            Illustration::Backhoe => "БТ",
        }
    }

    /// URL of the category illustration.
    pub fn svg_path(self) -> &'static str {
        match self {
            Illustration::Mini => "/static/img/machines/mini.svg",
            Illustration::Midi => "/static/img/machines/midi.svg",
            Illustration::Crawler => "/static/img/machines/crawler.svg",
            Illustration::Wheeled => "/static/img/machines/wheeled.svg",
            Illustration::Backhoe => "/static/img/machines/backhoe.svg",
        }
    }

    /// viewBox width/height of the SVG (1 unit = 10 мм, design §8.3); used as
    /// `<img width height>` so all classes keep their true proportions.
    pub fn svg_size(self) -> (u32, u32) {
        match self {
            Illustration::Mini => (347, 244),
            Illustration::Midi => (509, 328),
            Illustration::Backhoe => (741, 447),
            Illustration::Wheeled => (987, 470),
            Illustration::Crawler => (1068, 527),
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Category {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub undercarriage: Undercarriage,
    pub sort_order: i64,
}

/// One machine row joined with its category.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Machine {
    pub id: i64,
    pub slug: String,
    pub category_id: i64,
    pub category_slug: String,
    pub category_name: String,
    pub undercarriage: Undercarriage,
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
    pub created_at: String,
}

impl Machine {
    /// Catalogue number, derived (not stored): „БГР-МД-005“.
    pub fn catalog_no(&self) -> String {
        format!("БГР-{}-{:03}", self.illustration.catalog_code(), self.id)
    }

    /// „Мидибагер, БГР-МД-005“ (card and table subtitle).
    pub fn class_and_no(&self) -> String {
        format!("{}, {}", self.illustration.class_label(), self.catalog_no())
    }

    /// Price per day for rentals of 7+ days (10 % off): 160 € → 144 €.
    pub fn week_price_cents(&self) -> i64 {
        self.price_per_day_cents - (self.price_per_day_cents * 10 + 50) / 100
    }

    /// Saturday/Sunday price (50 %).
    pub fn weekend_price_cents(&self) -> i64 {
        (self.price_per_day_cents * 50 + 50) / 100
    }

    /// Over 12 000 кг the machine travels on a low-bed truck (pricier transport).
    pub fn is_heavy(&self) -> bool {
        self.weight_kg > crate::services::pricing::HEAVY_WEIGHT_KG
    }

    /// Track type for the spec plate: rubber tracks on mini/midi, steel on big crawlers.
    pub fn undercarriage_text(&self) -> &'static str {
        match self.illustration {
            Illustration::Mini | Illustration::Midi => "гумени вериги",
            Illustration::Crawler => "стоманени вериги",
            Illustration::Wheeled | Illustration::Backhoe => "гуми",
        }
    }

    /// Maker in capitals for the data plate („KUBOTA“).
    pub fn brand_caps(&self) -> String {
        self.brand.to_uppercase()
    }

    /// The description split into paragraphs (blank line separated).
    pub fn paragraphs(&self) -> Vec<&str> {
        self.description
            .split("\n\n")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect()
    }

    /// Maintenance state by engine hours (services::maintenance).
    pub fn maintenance(&self, last_inspection: Option<NaiveDate>, today: NaiveDate) -> MaintenanceStatus {
        maintenance::status(
            self.engine_hours,
            self.last_service_hours,
            self.service_interval_hours,
            last_inspection,
            today,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    Hammer,
    Bucket,
    Auger,
    Ripper,
    Grab,
    Other,
}

impl AttachmentKind {
    pub const ALL: [AttachmentKind; 6] = [
        AttachmentKind::Hammer,
        AttachmentKind::Bucket,
        AttachmentKind::Auger,
        AttachmentKind::Ripper,
        AttachmentKind::Grab,
        AttachmentKind::Other,
    ];

    pub fn code(self) -> &'static str {
        match self {
            AttachmentKind::Hammer => "hammer",
            AttachmentKind::Bucket => "bucket",
            AttachmentKind::Auger => "auger",
            AttachmentKind::Ripper => "ripper",
            AttachmentKind::Grab => "grab",
            AttachmentKind::Other => "other",
        }
    }

    pub fn parse(code: &str) -> Option<AttachmentKind> {
        AttachmentKind::ALL.into_iter().find(|k| k.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            AttachmentKind::Hammer => "Хидравличен чук",
            AttachmentKind::Bucket => "Кофа",
            AttachmentKind::Auger => "Свредел",
            AttachmentKind::Ripper => "Рипер",
            AttachmentKind::Grab => "Грайфер",
            AttachmentKind::Other => "Друго",
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Attachment {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub kind: AttachmentKind,
    pub price_per_day_cents: i64,
    pub active: bool,
}

impl Attachment {
    /// Icon file name in `static/img/icons/` (design §8.5).
    pub fn icon(&self) -> &'static str {
        icon_for(self.kind, &self.slug)
    }
}

/// Maps an attachment kind (and for buckets, the slug) to an icon name.
pub fn icon_for(kind: AttachmentKind, slug: &str) -> &'static str {
    match kind {
        AttachmentKind::Hammer => "hammer",
        AttachmentKind::Bucket if slug.contains("tyasna") => "bucket-narrow",
        AttachmentKind::Bucket if slug.contains("60") => "bucket-wide",
        AttachmentKind::Bucket if slug.contains("planirovachna") => "bucket-grading",
        AttachmentKind::Bucket if slug.contains("naklonyaema") => "bucket-tilt",
        AttachmentKind::Bucket => "bucket",
        AttachmentKind::Auger => "auger",
        AttachmentKind::Grab => "grab",
        AttachmentKind::Ripper => "ripper",
        AttachmentKind::Other => "coupler",
    }
}

/// Photo metadata (the image bytes are loaded only by `/media/snimki/{id}`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MachinePhoto {
    pub id: i64,
    pub machine_id: i64,
    pub content_type: String,
    pub alt_text: String,
    pub sort_order: i64,
}

// ── Rentals ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum BookingStatus {
    New,
    Approved,
    Active,
    Completed,
    Declined,
    Cancelled,
}

impl BookingStatus {
    pub const ALL: [BookingStatus; 6] = [
        BookingStatus::New,
        BookingStatus::Approved,
        BookingStatus::Active,
        BookingStatus::Completed,
        BookingStatus::Declined,
        BookingStatus::Cancelled,
    ];

    /// Code stored in the database and used in URLs (`?status=new`).
    pub fn code(self) -> &'static str {
        match self {
            BookingStatus::New => "new",
            BookingStatus::Approved => "approved",
            BookingStatus::Active => "active",
            BookingStatus::Completed => "completed",
            BookingStatus::Declined => "declined",
            BookingStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(code: &str) -> Option<BookingStatus> {
        BookingStatus::ALL.into_iter().find(|s| s.code() == code)
    }

    /// Bulgarian label shown in badges.
    pub fn label(self) -> &'static str {
        match self {
            BookingStatus::New => "Нова",
            BookingStatus::Approved => "Одобрена",
            BookingStatus::Active => "Активна",
            BookingStatus::Completed => "Приключена",
            BookingStatus::Declined => "Отказана",
            BookingStatus::Cancelled => "Отказана от клиента",
        }
    }

    /// CSS modifier class: `status--approved`.
    pub fn css_class(self) -> String {
        format!("status--{}", self.code())
    }

    /// Approved and active bookings occupy the machine; others don't (architecture §3.1).
    pub fn is_blocking(self) -> bool {
        matches!(self, BookingStatus::Approved | BookingStatus::Active)
    }
}

/// One booking joined with the machine and client it belongs to.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Booking {
    pub id: i64,
    pub machine_id: i64,
    pub machine_name: String,
    pub machine_slug: String,
    pub machine_illustration: Illustration,
    pub machine_weight_kg: i64,
    pub client_id: i64,
    pub client_name: String,
    pub client_email: String,
    pub client_phone: String,
    pub client_company: Option<String>,
    pub client_eik: Option<String>,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub with_operator: bool,
    pub delivery_km: i64,
    pub delivery_address: Option<String>,
    pub client_note: Option<String>,
    pub status: BookingStatus,
    // price snapshot, computed by services::pricing when the request was sent
    pub days: i64,
    pub weekend_days: i64,
    pub equipment_cents: i64,
    pub discount_percent: i64,
    pub discount_cents: i64,
    pub operator_cents: i64,
    pub transport_cents: i64,
    pub net_cents: i64,
    pub vat_cents: i64,
    pub total_cents: i64,
    pub admin_note: Option<String>,
    pub hours_at_start: Option<i64>,
    pub hours_at_end: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

impl Booking {
    /// Number shown to users: „2026-0142“ (year of the request + id).
    pub fn number(&self) -> String {
        let year = self.created_at.get(0..4).and_then(|y| y.parse().ok()).unwrap_or(2026);
        format::booking_number(year, self.id)
    }

    /// Company name if the client has one, otherwise the person's name.
    pub fn client_display(&self) -> &str {
        self.client_company.as_deref().unwrap_or(&self.client_name)
    }

    pub fn weekdays(&self) -> i64 {
        self.days - self.weekend_days
    }

    /// Equipment rate per day (machine + attachments) recovered from the snapshot:
    /// equipment = rate × weekdays + rate × weekend / 2  ⇒  rate = 2 × equipment / (2 × weekdays + weekend).
    /// (Exact for whole-euro rates, which is how all prices are set.)
    pub fn equip_rate_cents(&self) -> i64 {
        let parts = 2 * self.weekdays() + self.weekend_days;
        if parts == 0 { 0 } else { 2 * self.equipment_cents / parts }
    }

    /// Operator price per day (the operator is paid every day).
    pub fn operator_day_cents(&self) -> i64 {
        if self.days == 0 { 0 } else { self.operator_cents / self.days }
    }

    /// The client may cancel only while the request is still new.
    pub fn can_client_cancel(&self) -> bool {
        self.status == BookingStatus::New
    }
}

/// An attachment chosen for a booking, with the price at the time of the request.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BookingAttachment {
    pub attachment_id: i64,
    pub slug: String,
    pub name: String,
    pub kind: AttachmentKind,
    pub price_per_day_cents: i64,
}

impl BookingAttachment {
    pub fn icon(&self) -> &'static str {
        icon_for(self.kind, &self.slug)
    }
}

// ── Maintenance ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ServiceKind {
    Scheduled,
    Repair,
    Inspection,
}

impl ServiceKind {
    pub const ALL: [ServiceKind; 3] = [ServiceKind::Scheduled, ServiceKind::Repair, ServiceKind::Inspection];

    pub fn code(self) -> &'static str {
        match self {
            ServiceKind::Scheduled => "scheduled",
            ServiceKind::Repair => "repair",
            ServiceKind::Inspection => "inspection",
        }
    }

    pub fn parse(code: &str) -> Option<ServiceKind> {
        ServiceKind::ALL.into_iter().find(|k| k.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            ServiceKind::Scheduled => "Планово обслужване",
            ServiceKind::Repair => "Ремонт",
            ServiceKind::Inspection => "Технически преглед",
        }
    }
}

/// One service record joined with the machine name and the admin who entered it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServiceRecord {
    pub id: i64,
    pub machine_id: i64,
    pub machine_name: String,
    pub service_date: NaiveDate,
    pub engine_hours: i64,
    pub kind: ServiceKind,
    pub description: String,
    pub cost_cents: i64,
    pub created_by_name: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_labels_are_bulgarian() {
        assert_eq!(BookingStatus::New.label(), "Нова");
        assert_eq!(BookingStatus::Cancelled.label(), "Отказана от клиента");
        assert_eq!(BookingStatus::parse("approved"), Some(BookingStatus::Approved));
        assert_eq!(BookingStatus::parse("x"), None);
        assert_eq!(BookingStatus::Active.css_class(), "status--active");
        assert!(BookingStatus::Approved.is_blocking());
        assert!(!BookingStatus::New.is_blocking());
    }

    #[test]
    fn icons_follow_design_mapping() {
        assert_eq!(icon_for(AttachmentKind::Bucket, "tyasna-kofa-30"), "bucket-narrow");
        assert_eq!(icon_for(AttachmentKind::Bucket, "kofa-60"), "bucket-wide");
        assert_eq!(icon_for(AttachmentKind::Bucket, "planirovachna-kofa"), "bucket-grading");
        assert_eq!(icon_for(AttachmentKind::Bucket, "naklonyaema-kofa"), "bucket-tilt");
        assert_eq!(icon_for(AttachmentKind::Other, "barz-prikachvach"), "coupler");
        assert_eq!(icon_for(AttachmentKind::Hammer, "x"), "hammer");
    }

    #[test]
    fn catalogue_codes() {
        assert_eq!(Illustration::Midi.catalog_code(), "МД");
        assert_eq!(Illustration::parse("backhoe"), Some(Illustration::Backhoe));
        assert_eq!(ServiceKind::Repair.label(), "Ремонт");
    }
}
