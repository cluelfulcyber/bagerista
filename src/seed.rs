//! Demo data for an empty database (architecture §9.1).
//!
//! - deterministic: the random generator always starts from the same seed
//!   (`StdRng::seed_from_u64(2026)`), so every start produces the same demo;
//! - alive: all dates are relative to `today`, so there is always a rental
//!   running today, requests waiting for approval and 12 months of history;
//! - consistent: prices come from `services::pricing::quote`, the machines'
//!   engine hours are the sum of their completed rentals, no two approved or
//!   active bookings of one machine overlap.
//!
//! Unlike the rest of the application, this file writes its own INSERTs:
//! everything happens in ONE transaction, which the `db::*` functions (they
//! take the pool) cannot join.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use sqlx::{SqliteConnection, SqlitePool};

use crate::auth::hash_password;
use crate::errors::AppError;
use crate::models::{AttachmentKind, BookingStatus, Illustration, ServiceKind};
use crate::services::pricing::{self, HEAVY_WEIGHT_KG, PriceInput};

pub const ADMIN_EMAIL: &str = "admin@bagerista.bg";
pub const ADMIN_PASSWORD: &str = "Admin123!";
pub const CLIENT_EMAIL: &str = "client@bagerista.bg";
pub const CLIENT_PASSWORD: &str = "Client123!";

// ── Fixed content ───────────────────────────────────────────────────────────

struct CategorySeed {
    slug: &'static str,
    name: &'static str,
    description: &'static str,
    undercarriage: &'static str,
    sort_order: i64,
}

/// Inserted in this order (ids 1–5); `sort_order` lists them by weight like the ruler.
const CATEGORIES: [CategorySeed; 5] = [
    CategorySeed {
        slug: "mini-bageri",
        name: "Мини багери",
        description: "Леки машини до 2 600 кг за дворове, градини и тесни места. Минават през врата или тесен вход и се превозват с обикновено ремарке.",
        undercarriage: "tracks",
        sort_order: 1,
    },
    CategorySeed {
        slug: "midibageri",
        name: "Мидибагери",
        description: "Универсалните машини от 5 500 до 8 800 кг за основи, басейни и ВиК. Копаят до 4,6 м и все още влизат в повечето дворове.",
        undercarriage: "tracks",
        sort_order: 2,
    },
    CategorySeed {
        slug: "verizhni-bageri",
        name: "Верижни багери",
        description: "Тежки машини от 15 до 30 тона за големи изкопи, котловани и товарене на камиони. Стабилни са на мек терен и пътуват с платформа.",
        undercarriage: "tracks",
        sort_order: 5,
    },
    CategorySeed {
        slug: "kolesni-bageri",
        name: "Колесни багери",
        description: "Багери на гуми, които се движат сами между обектите в града. Не нараняват асфалта и са бързи при ремонти на улици и мрежи.",
        undercarriage: "wheels",
        sort_order: 4,
    },
    CategorySeed {
        slug: "bager-tovarachi",
        name: "Багер-товарачи",
        description: "Комбинирани машини с товарач отпред и багер отзад. Копаят, разчистват и товарят с една машина.",
        undercarriage: "wheels",
        sort_order: 3,
    },
];

struct AttachmentSeed {
    slug: &'static str,
    name: &'static str,
    description: &'static str,
    kind: AttachmentKind,
    price_eur: i64,
    /// Fits machines with weight in this range (kg, inclusive).
    weight_range: (i64, i64),
}

/// Ids 1–8 (the machine page mockup uses these ids in its checkboxes).
const ATTACHMENTS: [AttachmentSeed; 8] = [
    AttachmentSeed { slug: "hidravlichen-chuk-malak", name: "Хидравличен чук (малък)", description: "Бетон и асфалт до 30 см", kind: AttachmentKind::Hammer, price_eur: 45, weight_range: (0, 7_999) },
    AttachmentSeed { slug: "hidravlichen-chuk-golyam", name: "Хидравличен чук (голям)", description: "Фундаменти, скала и дебел бетон", kind: AttachmentKind::Hammer, price_eur: 85, weight_range: (8_000, 99_999) },
    AttachmentSeed { slug: "tyasna-kofa-30", name: "Тясна кофа 30 см", description: "Канали за ВиК и кабели", kind: AttachmentKind::Bucket, price_eur: 8, weight_range: (0, 9_000) },
    AttachmentSeed { slug: "kofa-60", name: "Кофа 60 см", description: "Изкопи и товарене", kind: AttachmentKind::Bucket, price_eur: 10, weight_range: (0, 99_999) },
    AttachmentSeed { slug: "planirovachna-kofa", name: "Планировъчна кофа", description: "Подравняване и откоси, без зъби", kind: AttachmentKind::Bucket, price_eur: 12, weight_range: (0, 99_999) },
    AttachmentSeed { slug: "naklonyaema-kofa", name: "Наклоняема кофа", description: "Откоси и канавки под ъгъл", kind: AttachmentKind::Bucket, price_eur: 18, weight_range: (0, 23_000) },
    AttachmentSeed { slug: "hidravlichen-svredel", name: "Хидравличен свредел", description: "Дупки за огради и колони", kind: AttachmentKind::Auger, price_eur: 35, weight_range: (0, 9_000) },
    AttachmentSeed { slug: "grayfer", name: "Грайфер", description: "Камъни, корени, отпадъци", kind: AttachmentKind::Grab, price_eur: 40, weight_range: (1_500, 99_999) },
];

struct MachineSeed {
    slug: &'static str,
    /// Index into `CATEGORIES`.
    category: usize,
    name: &'static str,
    brand: &'static str,
    model: &'static str,
    year: i64,
    weight_kg: i64,
    dig_depth_mm: i64,
    width_mm: i64,
    height_mm: i64,
    engine_kw: f64,
    bucket_l: i64,
    price_eur: i64,
    operator_eur: i64,
    illustration: Illustration,
    service_interval: i64,
    /// Engine hours today (the history is generated to end exactly here).
    engine_hours: i64,
    /// Hours since the last scheduled service; tuned for the demo:
    /// 2 overdue (TB216, A 918), 2 due soon (TB290, CAT 315), the rest OK.
    since_service: i64,
    /// 100 = average demand; higher = shorter gaps between rentals.
    popularity: i64,
    description: &'static str,
}

/// The 14 machines of the fleet, in catalogue-number order (ids 1–14):
/// Kubota KX057-6 is id 5 → „БГР-МД-005“, as in the mockups.
const MACHINES: [MachineSeed; 14] = [
    MachineSeed {
        slug: "kubota-u10-5", category: 0, name: "Kubota U10-5", brand: "Kubota", model: "U10-5", year: 2018,
        weight_kg: 1_120, dig_depth_mm: 1_720, width_mm: 750, height_mm: 2_290, engine_kw: 7.4, bucket_l: 22,
        price_eur: 75, operator_eur: 140, illustration: Illustration::Mini, service_interval: 250,
        engine_hours: 2_870, since_service: 96, popularity: 90,
        description: "Най-малкият багер в парка. Широк е само 75 см и минава през обикновена врата, по тесни пътеки между къщи и през вътрешни дворове. Подходящ е за изкопи за кабели и дренаж, градински работи и малки основи до 1,7 м.\n\nТежи 1 120 кг и се превозва с обикновено ремарке. Гумените вериги не оставят следи по плочки и тревни площи.",
    },
    MachineSeed {
        slug: "takeuchi-tb216", category: 0, name: "Takeuchi TB216", brand: "Takeuchi", model: "TB216", year: 2018,
        weight_kg: 1_670, dig_depth_mm: 2_350, width_mm: 980, height_mm: 2_380, engine_kw: 11.8, bucket_l: 40,
        price_eur: 90, operator_eur: 140, illustration: Illustration::Mini, service_interval: 250,
        engine_hours: 4_655, since_service: 262, popularity: 80,
        description: "Компактен мини багер с прибиращи се вериги: под 1 м ширина при преминаване и по-широка, стабилна база при копаене. Копае до 2,35 м, което стига за траншеи за ВиК, септични ями и основи на огради.\n\nПриема хидравличен чук и свредел, затова е удобен за ремонтни работи в двора. Превозва се с ремарке заедно с приставките.",
    },
    MachineSeed {
        slug: "kubota-kx019-4", category: 0, name: "Kubota KX019-4", brand: "Kubota", model: "KX019-4", year: 2020,
        weight_kg: 1_800, dig_depth_mm: 2_540, width_mm: 990, height_mm: 2_410, engine_kw: 12.4, bucket_l: 45,
        price_eur: 95, operator_eur: 140, illustration: Illustration::Mini, service_interval: 500,
        engine_hours: 2_384, since_service: 140, popularity: 140,
        description: "Мини багер с ширина 99 см: минава през вход под 1 м и въпреки това копае до 2,54 м. Затова е любимият избор за дворове в града, в които голяма машина не може да влезе.\n\nРаботи тихо и икономично, а с тясната кофа 30 см прави чисти траншеи за тръби и кабели. С малкия хидравличен чук разбива стари бетонни настилки.\n\nПревозва се с ремарке заедно с приставките.",
    },
    MachineSeed {
        slug: "yanmar-vio26-6", category: 0, name: "Yanmar ViO26-6", brand: "Yanmar", model: "ViO26-6", year: 2022,
        weight_kg: 2_600, dig_depth_mm: 2_800, width_mm: 1_500, height_mm: 2_480, engine_kw: 15.2, bucket_l: 70,
        price_eur: 110, operator_eur: 140, illustration: Illustration::Mini, service_interval: 500,
        engine_hours: 1_236, since_service: 210, popularity: 110,
        description: "Мини багер с нулево завъртане на опашката: каросерията не излиза извън веригите при въртене, затова работи плътно до стена или ограда. Копае до 2,80 м и е най-дълбоко копаещият мини багер в парка.\n\nПодходящ е за основи на къщи, басейни и канализационни отклонения. Широк е 1,50 м, така че му трябва вход поне 1,6 м.",
    },
    MachineSeed {
        slug: "kubota-kx057-6", category: 1, name: "Kubota KX057-6", brand: "Kubota", model: "KX057-6", year: 2021,
        weight_kg: 5_500, dig_depth_mm: 3_800, width_mm: 1_960, height_mm: 2_550, engine_kw: 34.7, bucket_l: 160,
        price_eur: 160, operator_eur: 150, illustration: Illustration::Midi, service_interval: 500,
        engine_hours: 3_412, since_service: 88, popularity: 150,
        description: "Мидибагерът е най-универсалната машина в парка. Копае основи на къща, траншеи за ВиК и басейни до 3,8 м, а с хидравличен чук разбива бетон до 30 см.\n\nГумените вериги не нараняват асфалт и плочки. Опашката излиза само 20 см извън веригите при завъртане, затова работи до ограда или стена.\n\nТежи 5 500 кг и пътува на нашата платформа заедно с приставките, без специално разрешение.",
    },
    MachineSeed {
        slug: "takeuchi-tb290", category: 1, name: "Takeuchi TB290", brand: "Takeuchi", model: "TB290", year: 2019,
        weight_kg: 8_500, dig_depth_mm: 4_600, width_mm: 2_300, height_mm: 2_680, engine_kw: 49.0, bucket_l: 280,
        price_eur: 210, operator_eur: 160, illustration: Illustration::Midi, service_interval: 500,
        engine_hours: 5_210, since_service: 460, popularity: 100,
        description: "Мощен мидибагер, който копае до 4,60 м: стига за сутерени с дренаж, дълбоки шахти и изкопи за канализация. Широките вериги го правят стабилен при товарене на камиони.\n\nС големия хидравличен чук се справя с фундаменти и скална основа. Нужни са вход поне 2,4 м и твърд път до обекта.",
    },
    MachineSeed {
        slug: "volvo-ecr88d", category: 1, name: "Volvo ECR88D", brand: "Volvo", model: "ECR88D", year: 2020,
        weight_kg: 8_800, dig_depth_mm: 4_500, width_mm: 2_300, height_mm: 2_700, engine_kw: 42.5, bucket_l: 300,
        price_eur: 220, operator_eur: 160, illustration: Illustration::Midi, service_interval: 500,
        engine_hours: 3_980, since_service: 305, popularity: 95,
        description: "Мидибагер с къса опашка, създаден за работа в града: завърта се почти в габарита на веригите и работи в една лента на улицата, без да спира движението в другата.\n\nКопае до 4,50 м и е подходящ за ВиК мрежи, улични изкопи и основи на жилищни сгради. Кабината е тиха и просторна, удобна за дълги смени с оператор.",
    },
    MachineSeed {
        slug: "cat-315", category: 2, name: "CAT 315", brand: "CAT", model: "315", year: 2022,
        weight_kg: 15_500, dig_depth_mm: 5_900, width_mm: 2_490, height_mm: 2_940, engine_kw: 74.0, bucket_l: 700,
        price_eur: 330, operator_eur: 180, illustration: Illustration::Crawler, service_interval: 500,
        engine_hours: 4_212, since_service: 468, popularity: 120,
        description: "Верижен багер от средния клас, който съчетава голяма сила с разумни размери. Копае до 5,90 м и пълни камион с 4–5 кофи, затова е подходящ за изкопи на сгради, пътни работи и товарене.\n\nПревозва се с платформа за тежък транспорт. На асфалт поставяме гумени подложки под веригите.",
    },
    MachineSeed {
        slug: "komatsu-pc210lc-11", category: 2, name: "Komatsu PC210LC-11", brand: "Komatsu", model: "PC210LC-11", year: 2021,
        weight_kg: 22_500, dig_depth_mm: 6_600, width_mm: 2_980, height_mm: 3_040, engine_kw: 123.0, bucket_l: 1_000,
        price_eur: 420, operator_eur: 190, illustration: Illustration::Crawler, service_interval: 500,
        engine_hours: 5_870, since_service: 190, popularity: 90,
        description: "Класическият верижен багер от 22 500 кг за големи обекти. Дългите вериги му дават стабилност при работа с пълна кофа и на мек терен. Копае до 6,60 м.\n\nПодходящ е за котловани, насипи, укрепване на брегове и събаряне на стари сгради. Пристига с платформа за тежък транспорт, а на обекта е нужна площадка за разтоварване.",
    },
    MachineSeed {
        slug: "volvo-ec300e", category: 2, name: "Volvo EC300E", brand: "Volvo", model: "EC300E", year: 2023,
        weight_kg: 30_000, dig_depth_mm: 7_300, width_mm: 3_190, height_mm: 3_180, engine_kw: 175.0, bucket_l: 1_500,
        price_eur: 560, operator_eur: 200, illustration: Illustration::Crawler, service_interval: 500,
        engine_hours: 1_640, since_service: 352, popularity: 85,
        description: "Най-голямата машина в парка: 30 000 кг и дълбочина на копаене 7,30 м. Предназначена е за дълбоки котловани, кариерни работи и масово товарене на самосвали.\n\nРаботи с кофа от 1 500 л и с големия хидравличен чук. Транспортът е със специална платформа и изисква предварителен оглед на пътя до обекта.",
    },
    MachineSeed {
        slug: "jcb-hydradig-110w", category: 3, name: "JCB Hydradig 110W", brand: "JCB", model: "Hydradig 110W", year: 2019,
        weight_kg: 11_500, dig_depth_mm: 4_600, width_mm: 2_490, height_mm: 3_140, engine_kw: 81.0, bucket_l: 450,
        price_eur: 300, operator_eur: 170, illustration: Illustration::Wheeled, service_interval: 500,
        engine_hours: 6_920, since_service: 233, popularity: 85,
        description: "Колесен багер с двигател в шасито, който дава отлична видимост във всички посоки. Движи се сам по пътя, така че между близки обекти не е нужен транспорт.\n\nКопае до 4,60 м и е подходящ за градски изкопи, ВиК ремонти и поддръжка на пътища. Гумите и стабилизаторите не нараняват асфалта.",
    },
    MachineSeed {
        slug: "liebherr-a-918-compact", category: 3, name: "Liebherr A 918 Compact", brand: "Liebherr", model: "A 918 Compact", year: 2024,
        weight_kg: 18_500, dig_depth_mm: 5_800, width_mm: 2_550, height_mm: 3_130, engine_kw: 115.0, bucket_l: 800,
        price_eur: 400, operator_eur: 180, illustration: Illustration::Wheeled, service_interval: 500,
        engine_hours: 937, since_service: 537, popularity: 75,
        description: "Компактен колесен багер с къса опашка за тесни градски улици. Копае до 5,80 м и вдига тежки тръби и шахти, без да заема цялото платно.\n\nПредната лопата и задните стабилизатори го държат стабилен при работа с пълна кофа. Подходящ е за инфраструктурни обекти, на които машината се мести всеки ден.",
    },
    MachineSeed {
        slug: "jcb-3cx", category: 4, name: "JCB 3CX", brand: "JCB", model: "3CX", year: 2020,
        weight_kg: 8_070, dig_depth_mm: 5_460, width_mm: 2_350, height_mm: 3_600, engine_kw: 81.0, bucket_l: 230,
        price_eur: 230, operator_eur: 160, illustration: Illustration::Backhoe, service_interval: 500,
        engine_hours: 6_480, since_service: 59, popularity: 140,
        description: "Багер-товарачът е две машини в една: отпред товарач с голяма кофа, отзад багер, който копае до 5,46 м. Подходящ е за обекти, на които трябва и да се копае, и да се разчиства и товари.\n\nДвижи се сам по пътя, така че на близки обекти не плащате платформа. Предпочитан е за аварийни ремонти на ВиК и улични мрежи.",
    },
    MachineSeed {
        slug: "cat-432", category: 4, name: "CAT 432", brand: "CAT", model: "432", year: 2023,
        weight_kg: 8_800, dig_depth_mm: 4_800, width_mm: 2_380, height_mm: 3_700, engine_kw: 74.0, bucket_l: 230,
        price_eur: 240, operator_eur: 160, illustration: Illustration::Backhoe, service_interval: 500,
        engine_hours: 1_105, since_service: 395, popularity: 80,
        description: "Багер-товарач с мощен товарач отпред и рамо отзад, което копае до 4,80 м. Маневрен е в тесни дворове и на строителни площадки.\n\nУдобен е за изкопи на улични отклонения, обратен насип и подравняване на терена с една машина.",
    },
];

struct ClientSeed {
    name: &'static str,
    email: &'static str,
    phone: &'static str,
    company: Option<(&'static str, &'static str)>,
}

/// The demo client first, then 11 more (6 with companies).
const CLIENTS: [ClientSeed; 12] = [
    ClientSeed { name: "Иван Петров", email: CLIENT_EMAIL, phone: "+359 88 812 3456", company: Some(("„Петров Строй“ ЕООД", "204556781")) },
    ClientSeed { name: "Мария Колева", email: "maria.koleva@example.com", phone: "+359 88 734 2210", company: None },
    ClientSeed { name: "Димитър Стоянов", email: "d.stoyanov@example.com", phone: "+359 87 645 1189", company: Some(("„Марица Инфра“ ООД", "115772904")) },
    ClientSeed { name: "Елена Георгиева", email: "elena.georgieva@example.com", phone: "+359 89 512 7730", company: None },
    ClientSeed { name: "Николай Иванов", email: "n.ivanov@example.com", phone: "+359 88 290 4416", company: Some(("„Тракия Билд“ ЕООД", "206114385")) },
    ClientSeed { name: "Стефан Николов", email: "stefan.nikolov@example.com", phone: "+359 87 733 9021", company: None },
    ClientSeed { name: "Петя Ангелова", email: "petya.angelova@example.com", phone: "+359 88 461 3378", company: Some(("„Зелен двор“ ЕООД", "203881547")) },
    ClientSeed { name: "Христо Тодоров", email: "h.todorov@example.com", phone: "+359 89 920 6604", company: Some(("„Родопи Хидро“ ООД", "115903226")) },
    ClientSeed { name: "Виктория Маринова", email: "viktoria.marinova@example.com", phone: "+359 88 155 8842", company: None },
    ClientSeed { name: "Георги Атанасов", email: "g.atanasov@example.com", phone: "+359 87 318 2275", company: Some(("„Атанасов и син“ ООД", "160087412")) },
    ClientSeed { name: "Красимир Велков", email: "krasimir.velkov@example.com", phone: "+359 89 647 0193", company: None },
    ClientSeed { name: "Йордан Костадинов", email: "y.kostadinov@example.com", phone: "+359 88 702 5561", company: Some(("„Пълдин Пътстрой“ ЕООД", "205447830")) },
];

/// Clients used for random history (indexes into `CLIENTS`; companies twice,
/// because they rent more often). The demo client (0) gets only fixed bookings.
const HISTORY_CLIENTS: [usize; 17] = [1, 2, 2, 3, 4, 4, 5, 6, 6, 7, 7, 8, 9, 9, 10, 11, 11];

const ADDRESSES: [&str; 12] = [
    "гр. Пловдив, ул. „Брезовско шосе“ 140",
    "с. Марково, ул. „Липа“ 4",
    "гр. Асеновград, ул. „Изток“ 12",
    "с. Белащица, ул. „Христо Ботев“ 31",
    "гр. Пловдив, бул. „Цариградско шосе“ 95",
    "с. Брестник, ул. „Иван Вазов“ 7",
    "гр. Раковски, ул. „Шипка“ 22",
    "гр. Стамболийски, ул. „Македония“ 3",
    "с. Труд, ул. „Марица“ 18",
    "гр. Пловдив, кв. „Остромила“, ул. „Тинтява“ 9",
    "гр. Пещера, ул. „Дойранска епопея“ 5",
    "гр. Първомай, ул. „Княз Борис I“ 41",
];

const CLIENT_NOTES: [&str; 6] = [
    "Входът към обекта е тесен, моля обадете се преди доставка.",
    "Нужен е оператор с опит в траншеи.",
    "Може ли доставка в 7:30?",
    "Теренът е мек след дъждовете.",
    "Моля за фактура на фирмата.",
    "Ще работим и в събота.",
];

/// Rental lengths in days; repeated values make 2–5 days the most common.
const LENGTHS: [i64; 21] = [1, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 5, 5, 6, 7, 7, 8, 10, 12, 14, 21];

// ── Planned bookings (built in memory first, inserted at the end) ───────────

struct Planned {
    machine: usize,
    client: usize,
    start: NaiveDate,
    end: NaiveDate,
    status: BookingStatus,
    with_operator: bool,
    km: i64,
    address: Option<String>,
    client_note: Option<String>,
    admin_note: Option<String>,
    /// Indexes into `ATTACHMENTS`.
    attachments: Vec<usize>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    hours_at_start: Option<i64>,
    hours_at_end: Option<i64>,
}

fn at(day: NaiveDate, hour: u32, minute: u32) -> NaiveDateTime {
    day.and_hms_opt(hour, minute, 0).expect("valid time")
}

fn timestamp(t: NaiveDateTime) -> String {
    t.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn fits(attachment: &AttachmentSeed, machine: &MachineSeed) -> bool {
    let (min, max) = attachment.weight_range;
    (min..=max).contains(&machine.weight_kg)
}

/// The bookings around today that make the demo tell a story (the same
/// situations as the mockups): two rentals running, approvals ahead, four new
/// requests – one clashing with an approved booking (ECR88D) and one that
/// would take TB290 past its service interval.
fn fixed_bookings(today: NaiveDate) -> Vec<Planned> {
    let d = |offset: i64| today + Duration::days(offset);
    // (machine, client, start, end, status, operator, km, address, attachments, created offset, note)
    #[allow(clippy::type_complexity)]
    let rows: [(usize, usize, i64, i64, BookingStatus, bool, i64, Option<usize>, &[usize], i64, Option<&str>); 12] = [
        // the demo client's three bookings
        (2, 0, -36, -34, BookingStatus::Completed, false, 0, None, &[], -40, None),
        (12, 0, 12, 14, BookingStatus::Approved, false, 12, Some(0), &[], -4, None),
        (4, 0, 1, 7, BookingStatus::New, true, 20, Some(1), &[0], -1, Some("Обектът е зад къщата, има място за разтоварване.")),
        // running today
        (7, 2, -3, 1, BookingStatus::Active, true, 28, Some(6), &[], -7, None),
        (3, 5, -5, 6, BookingStatus::Active, false, 9, Some(3), &[2], -9, None),
        // approved, coming up
        (4, 4, 11, 15, BookingStatus::Approved, true, 35, Some(7), &[], -3, None),
        (4, 3, 20, 22, BookingStatus::Approved, false, 18, Some(2), &[], -2, None),
        (6, 7, 4, 8, BookingStatus::Approved, true, 42, Some(10), &[1], -6, None),
        (0, 1, 2, 3, BookingStatus::Approved, false, 6, Some(9), &[4], -3, None),
        // new requests
        (6, 9, 4, 8, BookingStatus::New, false, 25, Some(8), &[], -1, Some("Спешно е, ВиК аварията трябва да се отстрани до петък.")),
        (5, 11, 5, 11, BookingStatus::New, true, 55, Some(11), &[1], -1, None),
        (2, 8, 16, 18, BookingStatus::New, false, 0, None, &[2], -2, Some("Ще взема машината сам с ремарке.")),
    ];
    rows.iter()
        .enumerate()
        .map(|(i, &(machine, client, start, end, status, with_operator, km, address, atts, created, note))| {
            let created_at = at(d(created), 8 + (i as u32 % 9), (i as u32 * 17) % 60);
            let updated_at = match status {
                BookingStatus::New => created_at,
                BookingStatus::Approved => created_at + Duration::minutes(48),
                BookingStatus::Active => at(d(start), 7, 40),
                _ => at(d(end + 1), 17, 5),
            };
            Planned {
                machine,
                client,
                start: d(start),
                end: d(end),
                status,
                with_operator,
                km,
                address: address.map(|a| ADDRESSES[a].to_string()),
                client_note: note.map(str::to_string),
                admin_note: match status {
                    BookingStatus::Approved | BookingStatus::Active => Some("Доставка в 7:30.".to_string()),
                    _ => None,
                },
                attachments: atts.to_vec(),
                created_at,
                updated_at,
                hours_at_start: None,
                hours_at_end: None,
            }
        })
        .collect()
}

/// Days between two rentals, by season: busy April–October, slow in winter.
fn season_gap(rng: &mut StdRng, day: NaiveDate, popularity: i64) -> i64 {
    let gap = match day.month() {
        4..=10 => rng.random_range(7..=26),
        3 | 11 => rng.random_range(14..=40),
        _ => rng.random_range(28..=65),
    };
    (gap * 100 / popularity).max(1)
}

/// Walks backwards from today over 12 months and fills the gaps between the
/// fixed bookings with random past rentals (never overlapping, always at
/// least one free day in between).
fn history(rng: &mut StdRng, today: NaiveDate, fixed: &[Planned]) -> Vec<Planned> {
    let year_ago = today - Duration::days(365);
    let mut out = Vec::new();
    for (machine, seed) in MACHINES.iter().enumerate() {
        let taken: Vec<(NaiveDate, NaiveDate)> =
            fixed.iter().filter(|p| p.machine == machine).map(|p| (p.start, p.end)).collect();
        let fitting: Vec<usize> = (0..ATTACHMENTS.len()).filter(|&a| fits(&ATTACHMENTS[a], seed)).collect();
        let mut cursor = today - Duration::days(1); // the last day the next (earlier) rental may use
        loop {
            let end = cursor - Duration::days(season_gap(rng, cursor, seed.popularity));
            let length = LENGTHS[rng.random_range(0..LENGTHS.len())];
            let start = end - Duration::days(length - 1);
            if start < year_ago {
                break;
            }
            // keep a free day on both sides of every fixed booking
            if let Some(&(fixed_start, _)) = taken
                .iter()
                .find(|(s, e)| *s <= end + Duration::days(1) && *e >= start - Duration::days(1))
            {
                cursor = fixed_start - Duration::days(2);
                continue;
            }

            let roll = rng.random_range(0..100);
            let status = if roll < 85 {
                BookingStatus::Completed
            } else if roll < 93 {
                BookingStatus::Declined
            } else {
                BookingStatus::Cancelled
            };
            let km = if rng.random_bool(0.25) { 0 } else { rng.random_range(3..=80) };
            let attachments = if !fitting.is_empty() && rng.random_bool(0.3) {
                vec![fitting[rng.random_range(0..fitting.len())]]
            } else {
                vec![]
            };
            let created_at = at(start - Duration::days(rng.random_range(1..=10)), rng.random_range(8..=17), rng.random_range(0..60));
            let updated_at = match status {
                BookingStatus::Completed => at(end + Duration::days(1), 17, rng.random_range(0..60)),
                _ => created_at + Duration::hours(rng.random_range(2..=30)),
            };
            out.push(Planned {
                machine,
                client: HISTORY_CLIENTS[rng.random_range(0..HISTORY_CLIENTS.len())],
                start,
                end,
                status,
                with_operator: rng.random_bool(0.4),
                km,
                address: (km > 0).then(|| ADDRESSES[rng.random_range(0..ADDRESSES.len())].to_string()),
                client_note: rng.random_bool(0.2).then(|| CLIENT_NOTES[rng.random_range(0..CLIENT_NOTES.len())].to_string()),
                admin_note: (status == BookingStatus::Declined).then(|| "Машината е в ремонт в този период.".to_string()),
                attachments,
                created_at,
                updated_at,
                hours_at_start: None,
                hours_at_end: None,
            });
            cursor = start - Duration::days(1);
        }
    }
    out
}

/// Engine-hour timeline of one machine: (start, end, hours at start, hours at end)
/// of its completed rentals in date order.
type Timeline = Vec<(NaiveDate, NaiveDate, i64, i64)>;

/// Fills `hours_at_start/end` so that the completed rentals add up exactly to
/// the machine's current engine hours (≈ 6 hours per day). Returns the
/// timeline used for dating service records.
fn assign_hours(rng: &mut StdRng, machine: usize, bookings: &mut [Planned]) -> (i64, Timeline) {
    let mut completed: Vec<&mut Planned> = bookings
        .iter_mut()
        .filter(|b| b.machine == machine && b.status == BookingStatus::Completed)
        .collect();
    completed.sort_by_key(|b| b.start);
    let worked: Vec<i64> = completed
        .iter()
        .map(|b| ((b.end - b.start).num_days() + 1) * rng.random_range(5..=7))
        .collect();
    let base = MACHINES[machine].engine_hours - worked.iter().sum::<i64>();
    let mut hours = base;
    let mut timeline = Vec::new();
    for (b, used) in completed.into_iter().zip(worked) {
        b.hours_at_start = Some(hours);
        hours += used;
        b.hours_at_end = Some(hours);
        timeline.push((b.start, b.end, hours - used, hours));
    }
    // a rental running today started with today's hours
    for b in bookings.iter_mut().filter(|b| b.machine == machine && b.status == BookingStatus::Active) {
        b.hours_at_start = Some(MACHINES[machine].engine_hours);
    }
    (base, timeline)
}

/// The date on which the machine had `hours` engine hours (it only works
/// during rentals, so: the day after the rental that reached them, or the
/// day before the rental that started above them).
fn date_at_hours(hours: i64, timeline: &Timeline, today: NaiveDate) -> NaiveDate {
    let day = match timeline.iter().find(|(_, _, _, end_hours)| *end_hours >= hours) {
        Some((start, _, start_hours, _)) if *start_hours >= hours => *start - Duration::days(1),
        Some((_, end, _, _)) => *end + Duration::days(1),
        None => timeline.last().map(|(_, end, _, _)| *end + Duration::days(1)).unwrap_or(today - Duration::days(200)),
    };
    day.min(today - Duration::days(1))
}

/// Engine hours on a given date.
fn hours_at_date(day: NaiveDate, base: i64, timeline: &Timeline) -> i64 {
    timeline.iter().filter(|(_, end, _, _)| *end < day).map(|(_, _, _, h)| *h).max().unwrap_or(base)
}

// ── Inserting ───────────────────────────────────────────────────────────────

/// Fills an empty database with the demo. Runs in one transaction: either
/// everything is inserted or nothing.
pub async fn run(db: &SqlitePool, today: NaiveDate) -> Result<(), AppError> {
    let mut rng = StdRng::seed_from_u64(2026);
    // Two Argon2 hashes only: all demo clients share the password „Client123!“.
    let admin_hash = hash_password(ADMIN_PASSWORD)?;
    let client_hash = hash_password(CLIENT_PASSWORD)?;

    // 1. Plan the bookings in memory.
    let mut bookings = fixed_bookings(today);
    let random_history = history(&mut rng, today, &bookings);
    bookings.extend(random_history);
    let mut timelines = Vec::new();
    for machine in 0..MACHINES.len() {
        timelines.push(assign_hours(&mut rng, machine, &mut bookings));
    }
    // ids in the order the requests arrived
    bookings.sort_by_key(|b| (b.created_at, b.machine));

    // 2. Write everything.
    let mut tx = db.begin().await?;
    let conn: &mut SqliteConnection = &mut tx;

    let admin_id = insert_user(conn, "Георги Димитров", ADMIN_EMAIL, "+359 88 900 1122", None, &admin_hash, "admin").await?;
    let mut client_ids = Vec::new();
    for c in &CLIENTS {
        client_ids.push(insert_user(conn, c.name, c.email, c.phone, c.company, &client_hash, "client").await?);
    }

    let mut category_ids = Vec::new();
    for c in &CATEGORIES {
        let id = sqlx::query("INSERT INTO categories (slug, name, description, undercarriage, sort_order) VALUES (?, ?, ?, ?, ?)")
            .bind(c.slug)
            .bind(c.name)
            .bind(c.description)
            .bind(c.undercarriage)
            .bind(c.sort_order)
            .execute(&mut *conn)
            .await?
            .last_insert_rowid();
        category_ids.push(id);
    }

    let mut attachment_ids = Vec::new();
    for a in &ATTACHMENTS {
        let id = sqlx::query("INSERT INTO attachments (slug, name, description, kind, price_per_day_cents) VALUES (?, ?, ?, ?, ?)")
            .bind(a.slug)
            .bind(a.name)
            .bind(a.description)
            .bind(a.kind)
            .bind(a.price_eur * 100)
            .execute(&mut *conn)
            .await?
            .last_insert_rowid();
        attachment_ids.push(id);
    }

    let mut machine_ids = Vec::new();
    for m in &MACHINES {
        let id = sqlx::query(
            "INSERT INTO machines (slug, category_id, name, brand, model, year_made, weight_kg, dig_depth_mm,
                                   width_mm, height_mm, engine_kw, bucket_l, price_per_day_cents,
                                   operator_price_per_day_cents, engine_hours, service_interval_hours,
                                   last_service_hours, description, illustration, active, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)",
        )
        .bind(m.slug)
        .bind(category_ids[m.category])
        .bind(m.name)
        .bind(m.brand)
        .bind(m.model)
        .bind(m.year)
        .bind(m.weight_kg)
        .bind(m.dig_depth_mm)
        .bind(m.width_mm)
        .bind(m.height_mm)
        .bind(m.engine_kw)
        .bind(m.bucket_l)
        .bind(m.price_eur * 100)
        .bind(m.operator_eur * 100)
        .bind(m.engine_hours)
        .bind(m.service_interval)
        .bind(m.engine_hours - m.since_service)
        .bind(m.description)
        .bind(m.illustration)
        .bind(timestamp(at(today - Duration::days(400), 9, 0)))
        .execute(&mut *conn)
        .await?
        .last_insert_rowid();
        machine_ids.push(id);
        for (a, attachment) in ATTACHMENTS.iter().enumerate() {
            if fits(attachment, m) {
                sqlx::query("INSERT INTO machine_attachments (machine_id, attachment_id) VALUES (?, ?)")
                    .bind(id)
                    .bind(attachment_ids[a])
                    .execute(&mut *conn)
                    .await?;
            }
        }
    }

    for b in &bookings {
        insert_booking(conn, b, &machine_ids, &client_ids, &attachment_ids).await?;
    }

    for (machine, (base, timeline)) in timelines.iter().enumerate() {
        insert_service_history(conn, &mut rng, today, machine, machine_ids[machine], *base, timeline, admin_id).await?;
    }

    tx.commit().await?;
    tracing::info!(
        "Демо данни: {} клиента, {} машини, {} заявки.",
        CLIENTS.len(),
        MACHINES.len(),
        bookings.len()
    );
    Ok(())
}

async fn insert_user(
    conn: &mut SqliteConnection,
    name: &str,
    email: &str,
    phone: &str,
    company: Option<(&str, &str)>,
    hash: &str,
    role: &str,
) -> Result<i64, sqlx::Error> {
    Ok(sqlx::query(
        "INSERT INTO users (email, password_hash, full_name, phone, company_name, company_eik, role, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now', '-400 days'))",
    )
    .bind(email)
    .bind(hash)
    .bind(name)
    .bind(phone)
    .bind(company.map(|c| c.0))
    .bind(company.map(|c| c.1))
    .bind(role)
    .execute(conn)
    .await?
    .last_insert_rowid())
}

async fn insert_booking(
    conn: &mut SqliteConnection,
    b: &Planned,
    machine_ids: &[i64],
    client_ids: &[i64],
    attachment_ids: &[i64],
) -> Result<(), AppError> {
    let machine = &MACHINES[b.machine];
    let attachment_prices: Vec<i64> = b.attachments.iter().map(|&a| ATTACHMENTS[a].price_eur * 100).collect();
    let price = pricing::quote(&PriceInput {
        start: b.start,
        end: b.end,
        machine_day_cents: machine.price_eur * 100,
        operator_day_cents: machine.operator_eur * 100,
        with_operator: b.with_operator,
        attachment_day_cents: attachment_prices.clone(),
        delivery_km: b.km,
        heavy_transport: machine.weight_kg > HEAVY_WEIGHT_KG,
    })
    .map_err(|e| AppError::Internal(format!("seed price: {e}")))?;

    let id = sqlx::query(
        "INSERT INTO bookings (machine_id, client_id, start_date, end_date, with_operator, delivery_km,
                               delivery_address, client_note, status, days, weekend_days, equipment_cents,
                               discount_percent, discount_cents, operator_cents, transport_cents,
                               net_cents, vat_cents, total_cents, admin_note, hours_at_start,
                               hours_at_end, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(machine_ids[b.machine])
    .bind(client_ids[b.client])
    .bind(b.start)
    .bind(b.end)
    .bind(b.with_operator)
    .bind(b.km)
    .bind(&b.address)
    .bind(&b.client_note)
    .bind(b.status)
    .bind(price.days)
    .bind(price.weekend_days)
    .bind(price.equipment_cents)
    .bind(price.discount_percent)
    .bind(price.discount_cents)
    .bind(price.operator_cents)
    .bind(price.transport_cents)
    .bind(price.net_cents)
    .bind(price.vat_cents)
    .bind(price.total_cents)
    .bind(&b.admin_note)
    .bind(b.hours_at_start)
    .bind(b.hours_at_end)
    .bind(timestamp(b.created_at))
    .bind(timestamp(b.updated_at))
    .execute(&mut *conn)
    .await?
    .last_insert_rowid();

    for (&a, price) in b.attachments.iter().zip(attachment_prices) {
        sqlx::query("INSERT INTO booking_attachments (booking_id, attachment_id, price_per_day_cents) VALUES (?, ?, ?)")
            .bind(id)
            .bind(attachment_ids[a])
            .bind(price)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// Repairs from the spec (machine index, description, cost in euro).
const REPAIRS: [(usize, &str, i64); 4] = [
    (7, "Смяна на хидравличен маркуч на стрелата", 280),
    (2, "Подмяна на гумените вериги", 1_450),
    (12, "Ремонт на стартера", 320),
    (8, "Смяна на зъбите на кофата", 160),
];

#[allow(clippy::too_many_arguments)]
async fn insert_service_history(
    conn: &mut SqliteConnection,
    rng: &mut StdRng,
    today: NaiveDate,
    machine: usize,
    machine_id: i64,
    base: i64,
    timeline: &Timeline,
    admin_id: i64,
) -> Result<(), sqlx::Error> {
    let seed = &MACHINES[machine];
    let last_service = seed.engine_hours - seed.since_service;
    let scheduled_cost = match seed.weight_kg {
        0..=3_000 => 180,
        3_001..=9_000 => 260,
        _ => 420,
    };
    let mut records: Vec<(NaiveDate, i64, ServiceKind, String, i64)> = Vec::new();

    // the last scheduled service (matches last_service_hours) and the one before it
    records.push((
        date_at_hours(last_service, timeline, today),
        last_service,
        ServiceKind::Scheduled,
        format!("Планово обслужване на {} моточаса: масла, филтри, гресиране, проверка на хидравликата.", seed.service_interval),
        scheduled_cost,
    ));
    let previous = last_service - seed.service_interval;
    if previous > 0 && previous > base {
        records.push((
            date_at_hours(previous, timeline, today),
            previous,
            ServiceKind::Scheduled,
            "Планово обслужване: смяна на двигателно масло и филтри.".to_string(),
            scheduled_cost,
        ));
    }

    // yearly inspection; Volvo EC300E's is 372 days old → the admin sees „Годишен технически преглед“
    let inspection_day = if seed.slug == "volvo-ec300e" {
        today - Duration::days(372)
    } else {
        today - Duration::days(rng.random_range(20..=330))
    };
    records.push((
        inspection_day,
        hours_at_date(inspection_day, base, timeline),
        ServiceKind::Inspection,
        "Годишен технически преглед, без забележки.".to_string(),
        60,
    ));

    for (repair_machine, description, cost) in REPAIRS {
        if repair_machine == machine && !timeline.is_empty() {
            let (_, end, _, hours) = timeline[timeline.len() / 2];
            records.push((end + Duration::days(1), hours, ServiceKind::Repair, description.to_string(), cost));
        }
    }

    for (day, hours, kind, description, cost) in records {
        sqlx::query(
            "INSERT INTO service_records (machine_id, service_date, engine_hours, kind, description, cost_cents, created_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(machine_id)
        .bind(day)
        .bind(hours)
        .bind(kind)
        .bind(description)
        .bind(cost * 100)
        .bind(admin_id)
        .bind(timestamp(at(day, 16, 30)))
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::availability::overlaps;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()
    }

    fn plan_for(today: NaiveDate) -> Vec<Planned> {
        let mut rng = StdRng::seed_from_u64(2026);
        let mut all = fixed_bookings(today);
        let h = history(&mut rng, today, &all);
        all.extend(h);
        for m in 0..MACHINES.len() {
            assign_hours(&mut rng, m, &mut all);
        }
        all
    }

    fn plan() -> Vec<Planned> {
        plan_for(today())
    }

    #[test]
    fn blocking_bookings_never_overlap() {
        // the demo must be valid whatever day it is seeded on
        for offset in (0..366).step_by(7) {
            let day = today() + Duration::days(offset);
            let all = plan_for(day);
            for (i, a) in all.iter().enumerate() {
                for b in &all[i + 1..] {
                    if a.machine == b.machine && a.status.is_blocking() && b.status.is_blocking() {
                        assert!(!overlaps(a.start, a.end, b.start, b.end), "{} {}–{}", MACHINES[a.machine].name, a.start, a.end);
                    }
                    if a.machine == b.machine && a.end < day && b.end < day {
                        // past rentals of one machine never touch either
                        assert!(!overlaps(a.start, a.end, b.start, b.end), "{day}: {} {}", MACHINES[a.machine].name, a.start);
                    }
                }
            }
            assert!(all.iter().all(|b| b.hours_at_start.is_none_or(|h| h >= 0)), "{day}");
            assert!(all.iter().all(|b| b.created_at.date() < b.start), "{day}");
        }
    }

    #[test]
    fn history_size_and_hours_are_plausible() {
        let all = plan();
        let past = all.iter().filter(|b| b.end < today()).count();
        assert!((120..=260).contains(&past), "{past} past bookings");
        for (i, m) in MACHINES.iter().enumerate() {
            let last_end = all
                .iter()
                .filter(|b| b.machine == i && b.status == BookingStatus::Completed)
                .filter_map(|b| b.hours_at_end)
                .max();
            if let Some(hours) = last_end {
                assert_eq!(hours, m.engine_hours, "{}", m.name);
            }
            assert!(all.iter().filter(|b| b.machine == i).all(|b| b.hours_at_start.is_none_or(|h| h >= 0)));
            assert!(m.engine_hours >= m.since_service);
        }
        // every request was sent before its rental started
        assert!(all.iter().all(|b| b.created_at.date() < b.start));
    }

    #[test]
    fn kubota_kx057_attachments_match_the_mockup() {
        let kx057 = &MACHINES[4];
        let ids: Vec<usize> = (0..ATTACHMENTS.len()).filter(|&a| fits(&ATTACHMENTS[a], kx057)).map(|a| a + 1).collect();
        assert_eq!(ids, vec![1, 3, 4, 5, 6, 7, 8]);
    }
}
