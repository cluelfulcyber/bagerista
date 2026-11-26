//! Public information pages: home, prices, about, contacts.
//!
//! `home` is the reference example of how a page is built (see `page.rs`):
//! the handler loads data through `db::*`, shapes it into small view structs
//! and renders `templates/public/home.html`, which extends `base.html`.

use askama::Template;
use axum::extract::State;
use axum::response::Response;

use crate::db;
use crate::db::catalog::{CatalogFilter, CategoryStats, FleetSummary};
use crate::errors::AppError;
use crate::format;
use crate::models::{AttachmentKind, Illustration, Machine};
use crate::routes::page::{HOME_TITLE, Layout, Nav, render};
use crate::routes::placeholder;
use crate::routes::views::{self, MachineCard};
use crate::services::pricing;
use crate::services::wizard::{Access, Answers, Depth, Job, Terrain};
use crate::state::AppState;

/// The machine drawn in the hero (and linked from its caption). The drawing
/// in `partials/hero_drawing.html` is made from this machine's dimensions.
pub const HERO_MACHINE_SLUG: &str = "kubota-kx057-6";
/// Rows in the home page price list (the whole list is in the catalogue).
const PRICE_ROWS: usize = 6;

// ── Home ────────────────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "public/home.html")]
pub struct HomePage {
    pub layout: Layout,
    pub fleet: FleetSummary,
    pub hero: Option<Machine>,
    /// Answers pre-selected on the site board (a trench in a typical yard).
    pub board: Answers,
    pub jobs: Vec<Job>,
    pub depths: Vec<Depth>,
    pub accesses: Vec<Access>,
    pub ruler: Vec<RulerRow>,
    pub lineup: Vec<LineupItem>,
    pub prices: Vec<MachineCard>,
    pub teaser: Vec<TeaserJob>,
    pub soil: Vec<SoilColumn>,
}

/// One class on the category ruler (a range on a log scale of weight).
pub struct RulerRow {
    pub stats: CategoryStats,
    pub class: Illustration,
    /// Position of the range on the axis, 0–1 (`--from` / `--to` in CSS).
    pub from: f64,
    pub to: f64,
    /// Class of the hero machine: drawn yellow, as in the mockup.
    pub current: bool,
}

/// The ruler axis runs from 1 000 to 32 000 кг on a logarithmic scale.
fn ruler_position(weight_kg: i64) -> f64 {
    let x = (weight_kg as f64 / 1000.0).ln() / 32f64.ln();
    x.clamp(0.0, 1.0)
}

impl RulerRow {
    /// `--from:.492;--to:.628` (three decimals are plenty for a bar).
    pub fn css_vars(&self) -> String {
        format!("--from:{:.3};--to:{:.3}", self.from, self.to)
    }

    /// „5 500–8 800 кг“
    pub fn weight_range(&self) -> String {
        let min = self.stats.min_weight_kg.unwrap_or(0);
        let max = self.stats.max_weight_kg.unwrap_or(min);
        if min == max { format::kg(min) } else { format!("{}–{}", format::number(min), format::kg(max)) }
    }

    /// „3 машини, от“
    pub fn count_text(&self) -> String {
        format::machines(self.stats.machine_count)
    }

    pub fn min_price(&self) -> i64 {
        self.stats.min_price_cents.unwrap_or(0)
    }
}

/// One drawing in the same-scale line-up.
pub struct LineupItem {
    pub src: &'static str,
    pub alt: String,
    /// viewBox width in cm (`--w`), so all drawings keep their true size.
    pub width: u32,
    pub height: u32,
    pub name: String,
    pub caption: String,
}

/// Deepest machine of one class on the soil chart.
pub struct SoilColumn {
    pub short_name: &'static str,
    pub depth_mm: i64,
    /// Does it reach the default depth (1,5 м)?
    pub is_short: bool,
}

impl SoilColumn {
    /// CSS number in metres: `2.8` (`--d`).
    pub fn depth_css(&self) -> String {
        format!("{}", self.depth_mm as f64 / 1000.0)
    }

    /// „2,80“ (the unit is in the axis).
    pub fn depth_label(&self) -> String {
        let cm = (self.depth_mm + 5) / 10;
        format!("{},{:02}", cm / 100, cm % 100)
    }
}

/// One answer of the wizard teaser: a radio tile and the text it shows.
pub struct TeaserJob {
    pub job: Job,
    /// Typical depth in metres, as a CSS number (`data-need`).
    pub need: &'static str,
    pub meta: &'static str,
    pub lead: &'static str,
    pub text: String,
}

/// The job selected when the page opens (a trench, 1,5 м).
const DEFAULT_JOB: Job = Job::Trasheya;
/// The site board starts with a trench up to 1,5 м behind a gate up to 1,6 м.
const BOARD_DEFAULTS: Answers =
    Answers { job: DEFAULT_JOB, depth: Depth::D1, access: Access::W2, terrain: Terrain::Normalen };
const DEFAULT_NEED_MM: i64 = 1_500;

impl HomePage {
    /// „14 машини от 1 120 до 30 000 кг под наем в Пловдив и до 150 км около него.“
    pub fn hero_lede(&self) -> String {
        let count = format::machines(self.fleet.machine_count);
        let min = format::number(self.fleet.min_weight_kg.unwrap_or(0));
        let max = format::kg(self.fleet.max_weight_kg.unwrap_or(0));
        format!(
            "{count} от {min} до {max} под наем в Пловдив и до {} км около него. \
             За всяка виждате размерите, свободните дни и крайната цена, преди да се обадите.",
            pricing::MAX_KM
        )
    }

    pub fn max_km(&self) -> i64 {
        pricing::MAX_KM
    }

    /// „10 % отстъпка“ (the first discount tier).
    pub fn first_discount(&self) -> String {
        format!("{} отстъпка", format::percent(pricing::discount_percent(7)))
    }

    /// „От 7 дни отстъпката е 10 %, от 14 дни 15 %, от 30 дни 20 %. …“
    pub fn discounts_text(&self) -> String {
        let p = |days| format::percent(pricing::discount_percent(days));
        format!(
            "От 7 дни отстъпката е {}, от 14 дни {}, от 30 дни {}. Съботите и неделите се плащат наполовина.",
            p(7),
            p(14),
            p(30)
        )
    }

    /// „Транспорт: 2 курса по 45,00 € + 1,60 € на км за машини до 12 000 кг.“
    pub fn transport_text(&self) -> String {
        format!(
            "Транспорт: 2 курса по {} + {} на км за машини до {}.",
            format::money(pricing::LIGHT_TRIP_FIXED),
            format::money(pricing::LIGHT_PER_KM),
            format::kg(pricing::HEAVY_WEIGHT_KG)
        )
    }

    /// „Целият каталог, 14 машини“
    pub fn catalog_link_text(&self) -> String {
        format!("Целият каталог, {}", format::machines(self.fleet.machine_count))
    }

    pub fn default_job(&self) -> Job {
        DEFAULT_JOB
    }

    pub fn default_answer(&self) -> Option<&TeaserJob> {
        self.teaser.iter().find(|t| t.job == DEFAULT_JOB)
    }

    /// „нужни 1,5 м“
    pub fn default_need_label(&self) -> String {
        format!("нужни {}", format::meters_short(DEFAULT_NEED_MM))
    }

    /// Accessible description of the soil chart.
    pub fn soil_label(&self) -> String {
        let reach: Vec<String> = self
            .soil
            .iter()
            .map(|c| format!("{} до {}", c.short_name.to_lowercase(), format::meters(c.depth_mm)))
            .collect();
        format!("Разрез на почвата: {}. Най-дълбоко копаещата машина от всеки клас: {}.", self.default_need_label(), reach.join(", "))
    }

    /// Hero caption: „мидибагер 5 500 кг“.
    pub fn hero_class_and_weight(&self) -> String {
        match &self.hero {
            Some(m) => format!("{} {}", m.illustration.class_label().to_lowercase(), format::kg(m.weight_kg)),
            None => String::new(),
        }
    }
}

/// Short class names on the soil chart columns.
fn short_class(class: Illustration) -> &'static str {
    match class {
        Illustration::Mini => "Мини",
        Illustration::Midi => "Миди",
        Illustration::Backhoe => "Товарач",
        Illustration::Wheeled => "Колесен",
        Illustration::Crawler => "Верижен",
    }
}

/// The five teaser answers. Prices in the texts come from the database.
fn teaser_jobs(stats: &[CategoryStats], attachments: &[crate::models::Attachment]) -> Vec<TeaserJob> {
    let class_stat = |class| stats.iter().find(|s| s.illustration == Some(class));
    let from_price = |class| {
        class_stat(class)
            .and_then(|s| s.min_price_cents)
            .map(|c| format!(", от {} на ден", format::money_whole(c)))
            .unwrap_or_default()
    };
    let attachment_price = |pick: &dyn Fn(&crate::models::Attachment) -> bool| {
        attachments.iter().filter(|a| a.active && pick(a)).map(|a| a.price_per_day_cents).min()
    };
    let grading = attachment_price(&|a| a.slug.contains("planirovachna"))
        .map(|c| format!(" Планировъчната кофа е {} на ден.", format::money_whole(c)))
        .unwrap_or_default();
    let hammer = attachment_price(&|a| a.kind == AttachmentKind::Hammer)
        .map(|c| format!(" ({} на ден)", format::money_whole(c)))
        .unwrap_or_default();
    let midi_weight = class_stat(Illustration::Midi)
        .and_then(|s| s.min_weight_kg)
        .map(|kg| format!(" от {}", format::kg(kg)))
        .unwrap_or_default();

    vec![
        TeaserJob {
            job: Job::Osnovi,
            need: "2",
            meta: "обикновено до 2 м",
            lead: "Основи или басейн:",
            text: format!(
                "обикновено до 2\u{a0}м. Мини багерът стига, но товари бавно. Мидибагер{midi_weight} копае с резерв \
                 и изнася пръстта по-бързо{}.",
                from_price(Illustration::Midi)
            ),
        },
        TeaserJob {
            job: Job::Trasheya,
            need: "1.5",
            meta: "до 1,5 м",
            lead: "Канал за ВиК или кабел:",
            text: format!(
                "обикновено до 1,5\u{a0}м дълбочина и 40–60\u{a0}см ширина. Стигат всички класове. \
                 Най-изгоден е мини багер с тясна кофа 30\u{a0}см{}.",
                from_price(Illustration::Mini)
            ),
        },
        TeaserJob {
            job: Job::Gradina,
            need: "0.6",
            meta: "до 0,6 м, често през тесен вход",
            lead: "Градина и подравняване:",
            text: format!(
                "плитко, до 0,6\u{a0}м, но често през тесен вход. Мини багер под 1\u{a0}м ширина минава през врата.{grading}"
            ),
        },
        TeaserJob {
            job: Job::Razbivane,
            need: "0.5",
            meta: "трябва хидравличен чук",
            lead: "Разбиване на бетон:",
            text: format!(
                "тук важи масата, не дълбочината. За плочи до 15\u{a0}см стига мини багер с малък чук{hammer}. \
                 За фундаменти вземете мидибагер с голям чук."
            ),
        },
        TeaserJob {
            job: Job::Izkop,
            need: "3.5",
            meta: "мазе, 3 м и повече",
            lead: "Голям изкоп:",
            text: "за мазе се копае около 3,5\u{a0}м. Мини багерите не стигат. Вземете мидибагер или верижен багер, \
                   който пълни камион с 3–4 кофи."
                .to_string(),
        },
    ]
}

/// `GET /` – the home page.
pub async fn home(State(state): State<AppState>, layout: Layout) -> Result<Response, AppError> {
    let db = &state.db;
    let today = layout.today;
    let fleet = db::catalog::fleet_summary(db).await?;
    let hero = db::catalog::machine_by_slug(db, HERO_MACHINE_SLUG).await?;
    let mut stats = db::catalog::category_stats(db).await?;
    stats.retain(|s| s.machine_count > 0);
    stats.sort_by_key(|s| s.min_weight_kg);
    let attachments = db::catalog::list_attachments(db).await?;

    // Category ruler: one row per class, lightest first.
    let hero_category = hero.as_ref().map(|m| m.category_id);
    let ruler: Vec<RulerRow> = stats
        .iter()
        .filter_map(|s| {
            Some(RulerRow {
                class: s.illustration?,
                from: ruler_position(s.min_weight_kg?),
                to: ruler_position(s.max_weight_kg?),
                current: Some(s.id) == hero_category,
                stats: s.clone(),
            })
        })
        .collect();

    // Same-scale line-up: the person, then each class with its deepest machine.
    let mut lineup = vec![LineupItem {
        src: "/static/img/person.svg",
        alt: "Човек с ръст 1,80 м".to_string(),
        width: 60,
        height: 191,
        name: "1,80 м".to_string(),
        caption: "за мащаб".to_string(),
    }];
    let mut soil = Vec::new();
    for class in Illustration::ALL {
        let Some(depth) = stats.iter().find(|s| s.illustration == Some(class)).and_then(|s| s.max_depth_mm) else {
            continue;
        };
        let (width, height) = class.svg_size();
        lineup.push(LineupItem {
            src: class.svg_path(),
            alt: class.class_label().to_string(),
            width,
            height,
            name: class.class_label().to_string(),
            caption: format!("копае до {}", format::meters(depth)),
        });
        soil.push(SoilColumn { short_name: short_class(class), depth_mm: depth, is_short: depth < DEFAULT_NEED_MM });
    }
    soil.sort_by_key(|c| c.depth_mm);

    // Price list: the most rented machines of the last year, lightest first.
    let machines = db::catalog::list_machines(db, &CatalogFilter::default()).await?;
    let top = db::stats::top_machines(db, today, 365, PRICE_ROWS as i64).await?;
    let mut picked: Vec<Machine> = top.iter().filter_map(|t| machines.iter().find(|m| m.id == t.machine_id).cloned()).collect();
    for m in &machines {
        if picked.len() >= PRICE_ROWS {
            break;
        }
        if !picked.iter().any(|p| p.id == m.id) {
            picked.push(m.clone());
        }
    }
    picked.sort_by_key(|m| (m.weight_kg, m.id));
    let prices = views::with_availability(db, today, picked).await?;

    let mut layout = layout;
    layout.title = HOME_TITLE.to_string();
    let page = HomePage {
        layout,
        fleet,
        hero,
        board: BOARD_DEFAULTS,
        jobs: Job::ALL.to_vec(),
        depths: Depth::ALL.to_vec(),
        accesses: Access::ALL.to_vec(),
        ruler,
        lineup,
        prices,
        teaser: teaser_jobs(&stats, &attachments),
        soil,
    };
    render(&page)
}

// ── Other public pages (stubs) ──────────────────────────────────────────────

/// `GET /ceni` – the pricing rules of architecture §4 in plain Bulgarian.
pub async fn prices(layout: Layout) -> Result<Response, AppError> {
    // TODO(feature: public-pages): price list table + rules (template public/prices.html)
    placeholder(layout.titled("Цени и условия", Nav::Prices), "Цени и условия")
}

/// `GET /za-nas`
pub async fn about(layout: Layout) -> Result<Response, AppError> {
    // TODO(feature: public-pages): „За нас“ (template public/about.html)
    placeholder(layout.titled("За нас", Nav::About), "За нас")
}

/// `GET /kontakti`
pub async fn contacts(layout: Layout) -> Result<Response, AppError> {
    // TODO(feature: public-pages): contacts (template public/contacts.html)
    placeholder(layout.titled("Контакти", Nav::Contacts), "Контакти")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruler_uses_a_log_scale() {
        assert_eq!(format!("{:.3}", ruler_position(5_500)), "0.492");
        assert_eq!(format!("{:.3}", ruler_position(8_800)), "0.628");
        assert_eq!(ruler_position(500), 0.0);
        assert_eq!(ruler_position(40_000), 1.0);
    }

    #[test]
    fn soil_labels() {
        let c = SoilColumn { short_name: "Мини", depth_mm: 2_800, is_short: false };
        assert_eq!(c.depth_css(), "2.8");
        assert_eq!(c.depth_label(), "2,80");
    }
}
