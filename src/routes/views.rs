//! View models: small structs that prepare data for the reusable partials in
//! `templates/partials/`. Each partial documents which variable it expects
//! (for example `partials/machine_card.html` expects `card: MachineCard`), so
//! a page includes it with `{% for card in cards %}{% include … %}{% endfor %}`
//! or `{% let card = … %}`. Formatting decisions live here, not in templates.

use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate};
use sqlx::SqlitePool;

use crate::format;
use crate::models::{Booking, Machine};
use crate::services::availability::{self, Availability, CalendarDay, DayState};
use crate::services::pricing::{self, PriceBreakdown};
use crate::services::wizard::Recommendation;

// ── Machine card ────────────────────────────────────────────────────────────

/// One machine with its availability badge (`partials/machine_card.html`,
/// also the rows of the home page price list).
#[derive(Debug, Clone)]
pub struct MachineCard {
    pub machine: Machine,
    pub availability: Availability,
}

/// Adds the „Свободна / Заета до …“ badge to each machine with one query.
pub async fn with_availability(db: &SqlitePool, today: NaiveDate, machines: Vec<Machine>) -> Result<Vec<MachineCard>, sqlx::Error> {
    let horizon = today + Duration::days(availability::BADGE_HORIZON_DAYS + 90);
    let mut spans: HashMap<i64, Vec<(NaiveDate, NaiveDate)>> = HashMap::new();
    for (machine_id, start, end) in crate::db::bookings::blocking_spans_all(db, today, horizon).await? {
        spans.entry(machine_id).or_default().push((start, end));
    }
    Ok(machines
        .into_iter()
        .map(|machine| {
            let blocking = spans.get(&machine.id).map(Vec::as_slice).unwrap_or(&[]);
            MachineCard { availability: availability::availability(today, blocking), machine }
        })
        .collect())
}

// ── Calendar month ──────────────────────────────────────────────────────────

/// One month of a machine's availability (`partials/calendar_month.html`, variable `cal`).
#[derive(Debug, Clone)]
pub struct CalendarMonthView {
    pub machine_slug: String,
    /// First day of the shown month.
    pub month: NaiveDate,
    pub days: Vec<CalendarDay>,
    /// Empty cells before day 1 (Monday-first grid).
    pub blanks: Vec<u32>,
    /// The period chosen in the form (`ot`/`do`), if any.
    pub selected: Option<(NaiveDate, NaiveDate)>,
    /// „Предишен месец“ is a link only when that month is not entirely in the past.
    pub has_prev: bool,
}

impl CalendarMonthView {
    pub fn new(
        machine_slug: &str,
        month: NaiveDate,
        today: NaiveDate,
        blocking: &[(NaiveDate, NaiveDate)],
        selected: Option<(NaiveDate, NaiveDate)>,
    ) -> CalendarMonthView {
        let month = availability::first_of_month(month);
        CalendarMonthView {
            machine_slug: machine_slug.to_string(),
            month,
            days: availability::month_view(month, today, blocking),
            blanks: (0..availability::leading_blanks(month)).collect(),
            selected,
            has_prev: month > availability::first_of_month(today),
        }
    }

    /// „октомври 2026“
    pub fn title(&self) -> String {
        format::month_year(self.month)
    }

    pub fn prev_param(&self) -> String {
        availability::month_param(availability::add_months(self.month, -1))
    }

    pub fn next_param(&self) -> String {
        availability::month_param(availability::add_months(self.month, 1))
    }

    /// „Предишен месец, септември“
    pub fn prev_label(&self) -> String {
        format!("Предишен месец, {}", format::month_name(availability::add_months(self.month, -1).month()))
    }

    /// „Следващ месец, ноември“
    pub fn next_label(&self) -> String {
        format!("Следващ месец, {}", format::month_name(availability::add_months(self.month, 1).month()))
    }

    pub fn day_number(&self, day: &CalendarDay) -> u32 {
        day.date.day()
    }

    pub fn is_disabled(&self, day: &CalendarDay) -> bool {
        day.state != DayState::Free
    }

    fn in_selection(&self, day: &CalendarDay) -> bool {
        self.selected.is_some_and(|(s, e)| day.date >= s && day.date <= e)
    }

    /// `aria-pressed` value of a free day: is it inside the chosen period?
    pub fn pressed(&self, day: &CalendarDay) -> &'static str {
        if self.in_selection(day) { "true" } else { "false" }
    }

    /// All CSS classes of one day button (design §6.14).
    pub fn day_class(&self, day: &CalendarDay) -> String {
        let mut class = String::from("cal__day");
        if day.is_weekend {
            class.push_str(" cal__day--weekend");
        }
        match day.state {
            DayState::Past => class.push_str(" cal__day--past"),
            DayState::Booked => class.push_str(" cal__day--booked"),
            DayState::Free => {
                if let Some((start, end)) = self.selected {
                    if day.date == start {
                        class.push_str(" cal__day--start");
                    }
                    if day.date == end {
                        class.push_str(" cal__day--end");
                    }
                    if day.date > start && day.date < end {
                        class.push_str(" cal__day--range");
                    }
                }
                if day.is_today && !self.in_selection(day) {
                    class.push_str(" cal__day--today");
                }
            }
        }
        class
    }

    /// Small tag under the number: „днес“, „зает“, „½ цена“ (weekends cost 50 %).
    pub fn day_tag(&self, day: &CalendarDay) -> &'static str {
        match day.state {
            DayState::Past => "",
            DayState::Booked => "зает",
            DayState::Free if day.is_today => "днес",
            DayState::Free if day.is_weekend => "½ цена",
            DayState::Free => "",
        }
    }

    /// „9 октомври, петък, свободен, половин цена“
    pub fn day_label(&self, day: &CalendarDay) -> String {
        let state = match day.state {
            DayState::Past => "минал ден",
            DayState::Booked => "зает",
            DayState::Free if day.is_weekend => "свободен, половин цена",
            DayState::Free => "свободен",
        };
        format!("{}, {}, {state}", format::day_month(day.date), format::weekday_name(day.date))
    }

    /// Text under the grid: „Избрахте 9 октомври – 15 октомври.“
    pub fn message(&self) -> String {
        match self.selected {
            Some((start, end)) => {
                format!("Избрахте {} – {}.", format::day_month(start), format::day_month(end))
            }
            None => "Изберете първия и последния ден.".to_string(),
        }
    }
}


// ── Price slip ──────────────────────────────────────────────────────────────

/// The itemised price (`partials/price_breakdown.html`, variable `slip`): the
/// same lines on the machine page, in „Моите заявки“ and on the print quote.
#[derive(Debug, Clone)]
pub struct PriceSlip {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub b: PriceBreakdown,
}

impl PriceSlip {
    pub fn new(start: NaiveDate, end: NaiveDate, b: PriceBreakdown) -> PriceSlip {
        PriceSlip { start, end, b }
    }

    /// Rebuilds the slip from the price snapshot stored with a booking.
    pub fn from_booking(booking: &Booking) -> PriceSlip {
        let heavy = booking.machine_weight_kg > pricing::HEAVY_WEIGHT_KG;
        let (trip_fixed_cents, per_km_cents) = if heavy {
            (pricing::HEAVY_TRIP_FIXED, pricing::HEAVY_PER_KM)
        } else {
            (pricing::LIGHT_TRIP_FIXED, pricing::LIGHT_PER_KM)
        };
        let rate = booking.equip_rate_cents();
        let weekday_equipment_cents = rate * booking.weekdays();
        let b = PriceBreakdown {
            days: booking.days,
            weekdays: booking.weekdays(),
            weekend_days: booking.weekend_days,
            equip_rate_cents: rate,
            weekday_equipment_cents,
            weekend_equipment_cents: booking.equipment_cents - weekday_equipment_cents,
            equipment_cents: booking.equipment_cents,
            discount_percent: booking.discount_percent,
            discount_cents: booking.discount_cents,
            operator_day_cents: booking.operator_day_cents(),
            operator_cents: booking.operator_cents,
            delivery_km: booking.delivery_km,
            trip_fixed_cents,
            per_km_cents,
            transport_cents: booking.transport_cents,
            net_cents: booking.net_cents,
            vat_cents: booking.vat_cents,
            total_cents: booking.total_cents,
        };
        PriceSlip { start: booking.start_date, end: booking.end_date, b }
    }

    /// „9 октомври – 15 октомври 2026, 7 дни (2 почивни)“
    pub fn period(&self) -> String {
        let mut text = format!("{}, {}", format::period_bg(self.start, self.end), format::days(self.b.days));
        if self.b.weekend_days > 0 {
            text.push_str(&format!(" ({} {})", self.b.weekend_days, format::plural(self.b.weekend_days, "почивен", "почивни")));
        }
        text
    }

    /// „Изпрати заявка за 9 октомври – 15 октомври“
    pub fn submit_label(&self) -> String {
        format!("Изпрати заявка за {} – {}", format::day_month(self.start), format::day_month(self.end))
    }

    /// „5 × 205,00 €“
    pub fn weekdays_calc(&self) -> String {
        format!("{} × {}", self.b.weekdays, format::money(self.b.equip_rate_cents))
    }

    /// „2 × 102,50 € (50 %)“
    pub fn weekend_calc(&self) -> String {
        let per_day = pricing::percent_of(self.b.equip_rate_cents, pricing::WEEKEND_PERCENT);
        format!("{} × {} ({})", self.b.weekend_days, format::money(per_day), format::percent(pricing::WEEKEND_PERCENT))
    }

    /// „Отстъпка 10 % за 7 дни“
    pub fn discount_label(&self) -> String {
        format!("Отстъпка {} за {}", format::percent(self.b.discount_percent), format::days(self.b.days))
    }

    /// „7 × 150,00 €“
    pub fn operator_calc(&self) -> String {
        format!("{} × {}", self.b.days, format::money(self.b.operator_day_cents))
    }

    /// „2 × (45,00 € + 20 км × 1,60 €)“
    pub fn transport_calc(&self) -> String {
        format!(
            "2 × ({} + {} км × {})",
            format::money(self.b.trip_fixed_cents),
            self.b.delivery_km,
            format::money(self.b.per_km_cents)
        )
    }

    /// Hint under the total: „Още 7 дни до отстъпка от 15 %.“
    pub fn hint(&self) -> String {
        match pricing::next_discount(self.b.days) {
            Some((more, pct)) => format!("Още {} до отстъпка от {}.", format::days(more), format::percent(pct)),
            None => format!("Получавате най-голямата отстъпка, {}.", format::percent(20)),
        }
    }

    pub fn vat_label(&self) -> String {
        format!("ДДС {}", format::percent(pricing::VAT_PERCENT))
    }
}

// ── Pagination ──────────────────────────────────────────────────────────────

/// Page links under a long list (`partials/pagination.html`, variable `pager`).
#[derive(Debug, Clone)]
pub struct Pagination {
    /// Current page, from 1.
    pub page: i64,
    pub pages: i64,
    /// The list URL without the page parameter, e.g. `/admin/zayavki?status=new`.
    pub base_href: String,
}

/// One numbered link.
#[derive(Debug, Clone)]
pub struct PageLink {
    pub number: i64,
    pub href: String,
    pub current: bool,
}

impl Pagination {
    /// `page` is clamped to the existing pages.
    pub fn new(page: i64, total_items: i64, per_page: i64, base_href: &str) -> Pagination {
        let pages = ((total_items + per_page - 1) / per_page.max(1)).max(1);
        Pagination { page: page.clamp(1, pages), pages, base_href: base_href.to_string() }
    }

    /// Offset for `LIMIT ? OFFSET ?`.
    pub fn offset(&self, per_page: i64) -> i64 {
        (self.page - 1) * per_page
    }

    pub fn href(&self, number: i64) -> String {
        let sep = if self.base_href.contains('?') { '&' } else { '?' };
        format!("{}{sep}strana={number}", self.base_href)
    }

    pub fn links(&self) -> Vec<PageLink> {
        (1..=self.pages)
            .map(|number| PageLink { number, href: self.href(number), current: number == self.page })
            .collect()
    }

    pub fn is_needed(&self) -> bool {
        self.pages > 1
    }
}

// ── Column chart ────────────────────────────────────────────────────────────

/// A CSS column chart with a hidden data table (`partials/svg_bar_chart.html`,
/// variable `chart`). Server-rendered: no chart library, works without JS.
#[derive(Debug, Clone)]
pub struct BarChart {
    /// `<caption>` of the hidden table: „Приходи по месеци, € без ДДС“.
    pub caption: String,
    pub note: String,
    pub bars: Vec<ChartBar>,
    /// Top of the scale (same unit as `ChartBar::value`).
    pub max: i64,
}

#[derive(Debug, Clone)]
pub struct ChartBar {
    /// Under the column: „окт“.
    pub axis_label: String,
    /// In the hidden table: „октомври 2026“.
    pub full_label: String,
    /// Height, in whole euros.
    pub value: i64,
    /// Above the column: „18,9“ (thousands of euros).
    pub value_text: String,
    /// In the hidden table: „18 900 €“.
    pub value_full: String,
    /// The current month is drawn yellow.
    pub is_current: bool,
}

impl BarChart {
    /// Revenue per month (db::stats::revenue_by_month), the last month is the current one.
    pub fn revenue(months: &[crate::db::stats::MonthRevenue], note: &str) -> BarChart {
        let last = months.len().saturating_sub(1);
        let bars: Vec<ChartBar> = months
            .iter()
            .enumerate()
            .map(|(i, m)| ChartBar {
                axis_label: format::month_axis(m.month.month()).to_string(),
                full_label: format::month_year(m.month),
                value: m.net_cents / 100,
                value_text: format::thousands_one_decimal(m.net_cents),
                value_full: format::money_whole(m.net_cents),
                is_current: i == last,
            })
            .collect();
        // round the scale up to whole 5 000 € so the 25 % gridlines are round numbers
        let top = bars.iter().map(|b| b.value).max().unwrap_or(0);
        let max = ((top + 4_999) / 5_000).max(1) * 5_000;
        BarChart { caption: "Приходи по месеци, € без ДДС".to_string(), note: note.to_string(), bars, max }
    }
}

// ── Wizard results ──────────────────────────────────────────────────────────

/// A recommendation with the machine it points to.
#[derive(Debug, Clone)]
pub struct RecommendationView {
    pub machine: Machine,
    pub rec: Recommendation,
}

/// What `partials/wizard_results.html` shows (variable `results`).
#[derive(Debug, Clone)]
pub struct WizardResults {
    /// Best match first.
    pub recs: Vec<RecommendationView>,
    /// Nothing passed the filters: `recs` are the near misses and the
    /// „Нямаме машина…“ message is shown above them.
    pub no_match: bool,
}

impl WizardResults {
    /// Joins recommendations with their machines (unknown ids are skipped).
    pub fn new(recs: Vec<Recommendation>, machines: &[Machine], no_match: bool) -> WizardResults {
        let recs = recs
            .into_iter()
            .filter_map(|rec| {
                let machine = machines.iter().find(|m| m.id == rec.machine_id)?.clone();
                Some(RecommendationView { machine, rec })
            })
            .collect();
        WizardResults { recs, no_match }
    }

    pub fn no_match_message(&self) -> &'static str {
        crate::services::wizard::NO_MATCH_MESSAGE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::pricing::{PriceInput, quote};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn slip_lines_match_the_reference_example() {
        // architecture §4.4
        let b = quote(&PriceInput {
            start: d(2026, 10, 9),
            end: d(2026, 10, 15),
            machine_day_cents: 160_00,
            operator_day_cents: 150_00,
            with_operator: true,
            attachment_day_cents: vec![45_00],
            delivery_km: 20,
            heavy_transport: false,
        })
        .unwrap();
        let slip = PriceSlip::new(d(2026, 10, 9), d(2026, 10, 15), b);
        assert_eq!(slip.period(), "9 октомври – 15 октомври 2026, 7 дни (2 почивни)");
        assert_eq!(slip.weekdays_calc(), "5 × 205,00\u{a0}€");
        assert_eq!(slip.weekend_calc(), "2 × 102,50\u{a0}€ (50\u{a0}%)");
        assert_eq!(slip.discount_label(), "Отстъпка 10\u{a0}% за 7 дни");
        assert_eq!(slip.transport_calc(), "2 × (45,00\u{a0}€ + 20 км × 1,60\u{a0}€)");
        assert_eq!(slip.hint(), "Още 7 дни до отстъпка от 15\u{a0}%.");
        assert_eq!(slip.submit_label(), "Изпрати заявка за 9 октомври – 15 октомври");
    }

    #[test]
    fn calendar_marks_the_selected_period() {
        let today = d(2026, 10, 8);
        let cal = CalendarMonthView::new("x", d(2026, 10, 1), today, &[(d(2026, 10, 19), d(2026, 10, 23))], Some((d(2026, 10, 9), d(2026, 10, 15))));
        assert_eq!(cal.blanks.len(), 3); // 1 October 2026 is a Thursday
        assert!(!cal.has_prev);
        let day = |n: usize| cal.days[n - 1].clone();
        assert_eq!(cal.day_class(&day(1)), "cal__day cal__day--past");
        assert_eq!(cal.day_class(&day(8)), "cal__day cal__day--today");
        assert_eq!(cal.day_tag(&day(8)), "днес");
        assert_eq!(cal.day_class(&day(9)), "cal__day cal__day--start");
        assert_eq!(cal.day_class(&day(10)), "cal__day cal__day--weekend cal__day--range");
        assert_eq!(cal.day_tag(&day(10)), "½ цена");
        assert_eq!(cal.day_class(&day(15)), "cal__day cal__day--end");
        assert_eq!(cal.day_class(&day(19)), "cal__day cal__day--booked");
        assert!(cal.is_disabled(&day(19)));
        assert_eq!(cal.day_label(&day(10)), "10 октомври, събота, свободен, половин цена");
        assert_eq!(cal.prev_label(), "Предишен месец, септември");
        assert_eq!(cal.next_param(), "2026-11");
    }

    #[test]
    fn pagination_links() {
        let p = Pagination::new(9, 45, 20, "/admin/zayavki?status=new");
        assert_eq!(p.pages, 3);
        assert_eq!(p.page, 3);
        assert_eq!(p.offset(20), 40);
        assert_eq!(p.links()[0].href, "/admin/zayavki?status=new&strana=1");
        assert_eq!(Pagination::new(1, 0, 20, "/katalog").href(2), "/katalog?strana=2");
        assert!(!Pagination::new(1, 5, 20, "/x").is_needed());
    }

    // ── Every partial compiles and renders (askama checks templates at build time) ──

    use askama::Template;

    use crate::models::{BookingStatus, Illustration, Undercarriage};
    use crate::routes::page::{Flash, FlashPartial, FormErrors};
    use crate::services::maintenance;

    fn sample_machine() -> Machine {
        Machine {
            id: 5,
            slug: "kubota-kx057-6".into(),
            category_id: 2,
            category_slug: "midibageri".into(),
            category_name: "Мидибагери".into(),
            undercarriage: Undercarriage::Tracks,
            name: "Kubota KX057-6".into(),
            brand: "Kubota".into(),
            model: "KX057-6".into(),
            year_made: 2021,
            weight_kg: 5_500,
            dig_depth_mm: 3_800,
            width_mm: 1_960,
            height_mm: 2_550,
            engine_kw: 34.7,
            bucket_l: 160,
            price_per_day_cents: 160_00,
            operator_price_per_day_cents: 150_00,
            engine_hours: 3_412,
            service_interval_hours: 500,
            last_service_hours: 3_324,
            description: "А.\n\nБ.".into(),
            illustration: Illustration::Midi,
            active: true,
            created_at: "2026-01-01 00:00:00".into(),
        }
    }

    #[derive(Template)]
    #[template(source = r#"{% for card in cards %}{% include "partials/machine_card.html" %}{% endfor %}{% include "partials/machine_grid.html" %}"#, ext = "html")]
    struct CardsT {
        cards: Vec<MachineCard>,
    }

    #[derive(Template)]
    #[template(source = r#"{% include "partials/spec_plate.html" %}{% include "partials/status_badge.html" %}{% include "partials/service_badge.html" %}{% let ill = machine.illustration %}{% let ill_class = "table__ill" %}{% include "partials/illustration.html" %}"#, ext = "html")]
    struct PlateT {
        machine: Machine,
        status: BookingStatus,
        service: maintenance::MaintenanceStatus,
    }

    #[derive(Template)]
    #[template(source = r#"{% include "partials/calendar_month.html" %}|{% include "partials/price_breakdown.html" %}|{% include "partials/pagination.html" %}|{% include "partials/svg_bar_chart.html" %}"#, ext = "html")]
    struct ViewsT {
        cal: CalendarMonthView,
        slip: PriceSlip,
        pager: Pagination,
        chart: BarChart,
    }

    #[derive(Template)]
    #[template(source = r#"{% include "partials/form_errors.html" %}{% let field = "email" %}{% include "partials/field_error.html" %}{% include "partials/wizard_results.html" %}"#, ext = "html")]
    struct FormT {
        errors: FormErrors,
        results: WizardResults,
    }

    #[test]
    fn partials_render() {
        let today = d(2026, 10, 8);
        let card = MachineCard { machine: sample_machine(), availability: Availability::Free };
        let html = CardsT { cards: vec![card] }.render().unwrap();
        assert!(html.contains("Мидибагер, БГР-МД-005"));
        assert!(html.contains("<data value=\"16000\" data-net>160,00\u{a0}€</data>"));
        assert!(html.contains("<data value=\"14400\" data-net>144,00\u{a0}€</data>"));
        assert!(html.contains("badge badge--free\">Свободна</span>"));
        assert!(CardsT { cards: vec![] }.render().unwrap().contains("Нямаме машина с тези условия"));

        let m = sample_machine();
        let html = PlateT {
            service: m.maintenance(Some(today), today),
            machine: m,
            status: BookingStatus::Approved,
        }
        .render()
        .unwrap();
        assert!(html.contains("plate__maker\">KUBOTA</span><span class=\"plate__code\">БГР-МД-005"));
        assert!(html.contains("34,7<span class=\"plate__unit\">kW</span>"));
        assert!(html.contains("status status--approved\">Одобрена</span>"));
        assert!(html.contains("service service--ok\">Следващо обслужване след 412 моточаса"));

        let b = quote(&PriceInput {
            start: d(2026, 10, 9),
            end: d(2026, 10, 15),
            machine_day_cents: 160_00,
            operator_day_cents: 150_00,
            with_operator: true,
            attachment_day_cents: vec![45_00],
            delivery_km: 20,
            heavy_transport: false,
        })
        .unwrap();
        let months = vec![
            crate::db::stats::MonthRevenue { month: d(2026, 9, 1), net_cents: 2_160_000 },
            crate::db::stats::MonthRevenue { month: d(2026, 10, 1), net_cents: 690_000 },
        ];
        let html = ViewsT {
            cal: CalendarMonthView::new("kubota-kx057-6", d(2026, 10, 1), today, &[], Some((d(2026, 10, 9), d(2026, 10, 15)))),
            slip: PriceSlip::new(d(2026, 10, 9), d(2026, 10, 15), b),
            pager: Pagination::new(2, 45, 20, "/admin/zayavki"),
            chart: BarChart::revenue(&months, "Октомври е до днес."),
        }
        .render()
        .unwrap();
        assert!(html.contains("октомври 2026"));
        assert!(html.contains("aria-label=\"9 октомври, петък, свободен\" aria-pressed=\"true\""));
        assert!(html.contains("data-q=\"total\">2\u{a0}773,20\u{a0}€"));
        assert!(html.contains("data-q=\"discountAmt\">−123,00\u{a0}€"));
        assert!(html.contains("href=\"/admin/zayavki?strana=2\" aria-current=\"page\""));
        assert!(html.contains("chart__bar chart__bar--now\" style=\"--v:6900\""));
        assert!(html.contains("style=\"--max:25000\""));

        let mut errors = FormErrors::new();
        errors.add("email", crate::auth::EMAIL_MESSAGE);
        let rec = Recommendation { machine_id: 5, score: 92, reasons: vec!["Копае до 3,8 м.".into()], caveats: vec![] };
        let html = FormT { errors, results: WizardResults::new(vec![rec], &[sample_machine()], false) }.render().unwrap();
        assert!(html.contains("Проверете 1 поле"));
        assert!(html.contains("<p class=\"field__error\" id=\"f-email-err\">"));
        assert!(html.contains("rec rec--best"));
        assert!(html.contains("92 %"));

        let html = FlashPartial { flash: Flash::ok("Заявката е изпратена.", "Ще ви се обадим.") }.render().unwrap();
        assert!(html.contains("flash flash--ok\" role=\"status\""));
    }
}
