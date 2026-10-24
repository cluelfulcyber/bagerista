//! Rental price calculation (architecture §4).
//!
//! `quote()` is a pure function: no database, no clock. The same function is
//! used by the live calculator API, by the booking request (the server always
//! recomputes the price, it never trusts the browser) and by the demo seed.
//! All amounts are integer euro cents; percentages are rounded half up with
//! `(x * p + 50) / 100`.

use chrono::{Datelike, NaiveDate, Weekday};
use serde::Serialize;

pub const MAX_DAYS: i64 = 90;
pub const MAX_KM: i64 = 150;
/// Saturdays and Sundays: machine and attachments at 50 %.
pub const WEEKEND_PERCENT: i64 = 50;
pub const VAT_PERCENT: i64 = 20;
/// Machines heavier than this need a low-bed truck instead of a trailer.
pub const HEAVY_WEIGHT_KG: i64 = 12_000;
// Transport, per one-way trip; we always deliver AND collect → × 2.
pub const LIGHT_TRIP_FIXED: i64 = 45_00;
pub const LIGHT_PER_KM: i64 = 1_60;
pub const HEAVY_TRIP_FIXED: i64 = 90_00;
pub const HEAVY_PER_KM: i64 = 2_40;

/// Everything the price depends on.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceInput {
    pub start: NaiveDate,
    /// Inclusive: a rental 9–15 October is 7 days.
    pub end: NaiveDate,
    pub machine_day_cents: i64,
    pub operator_day_cents: i64,
    pub with_operator: bool,
    /// One entry per chosen attachment.
    pub attachment_day_cents: Vec<i64>,
    /// 0 = the client collects the machine from our yard.
    pub delivery_km: i64,
    /// Machine weight > 12 000 кг.
    pub heavy_transport: bool,
}

/// The itemised result, exactly the lines of the price slip and the quote.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PriceBreakdown {
    pub days: i64,
    pub weekdays: i64,
    pub weekend_days: i64,
    /// Machine + attachments per day.
    pub equip_rate_cents: i64,
    /// Equipment on weekdays (rate × weekdays).
    pub weekday_equipment_cents: i64,
    /// Equipment on Saturdays and Sundays (at 50 %).
    pub weekend_equipment_cents: i64,
    /// weekday + weekend equipment, before the discount.
    pub equipment_cents: i64,
    pub discount_percent: i64,
    pub discount_cents: i64,
    pub operator_day_cents: i64,
    pub operator_cents: i64,
    pub delivery_km: i64,
    /// Fixed part and per-km price of one trip (for the line „2 × (45,00 € + 20 км × 1,60 €)“).
    pub trip_fixed_cents: i64,
    pub per_km_cents: i64,
    pub transport_cents: i64,
    /// „Общо без ДДС“
    pub net_cents: i64,
    /// „ДДС 20 %“
    pub vat_cents: i64,
    /// „Общо с ДДС“
    pub total_cents: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceError {
    EndBeforeStart,
    TooLong,
    TooFar,
    NegativeKm,
}

impl PriceError {
    /// Bulgarian message for the form / API.
    pub fn message(self) -> &'static str {
        match self {
            PriceError::EndBeforeStart => "Крайната дата е преди началната.",
            PriceError::TooLong => {
                "Максималният срок на наем е 90 дни. За по-дълъг наем се свържете с нас."
            }
            PriceError::TooFar => "Доставяме до 150 км от базата.",
            PriceError::NegativeKm => "Невалидно разстояние.",
        }
    }
}

impl std::fmt::Display for PriceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// Long-rental discount on machine + attachments.
pub fn discount_percent(days: i64) -> i64 {
    match days {
        30.. => 20,
        14..=29 => 15,
        7..=13 => 10,
        _ => 0,
    }
}

/// The next discount tier, for the hint „Още 7 дни до отстъпка от 15 %.“:
/// returns (extra days needed, percent), or `None` at the top tier.
pub fn next_discount(days: i64) -> Option<(i64, i64)> {
    [7, 14, 30]
        .into_iter()
        .find(|&threshold| days < threshold)
        .map(|threshold| (threshold - days, discount_percent(threshold)))
}

/// `x × p %`, rounded half up (all amounts are non-negative).
pub fn percent_of(x: i64, p: i64) -> i64 {
    (x * p + 50) / 100
}

pub fn is_weekend(day: NaiveDate) -> bool {
    matches!(day.weekday(), Weekday::Sat | Weekday::Sun)
}

/// Number of Saturdays and Sundays in [start, end] (inclusive).
pub fn count_weekend_days(start: NaiveDate, end: NaiveDate) -> i64 {
    start
        .iter_days()
        .take_while(|day| *day <= end)
        .filter(|day| is_weekend(*day))
        .count() as i64
}

/// Number of days in an inclusive range: 9–15 October → 7.
pub fn rental_days(start: NaiveDate, end: NaiveDate) -> i64 {
    (end - start).num_days() + 1
}

/// Computes the full price breakdown (formula in architecture §4.2).
pub fn quote(input: &PriceInput) -> Result<PriceBreakdown, PriceError> {
    if input.end < input.start {
        return Err(PriceError::EndBeforeStart);
    }
    if input.delivery_km < 0 {
        return Err(PriceError::NegativeKm);
    }
    if input.delivery_km > MAX_KM {
        return Err(PriceError::TooFar);
    }
    let days = rental_days(input.start, input.end);
    if days > MAX_DAYS {
        return Err(PriceError::TooLong);
    }

    let weekend_days = count_weekend_days(input.start, input.end);
    let weekdays = days - weekend_days;

    // Machine and attachments: full price on weekdays, half price on weekends.
    let equip_rate = input.machine_day_cents + input.attachment_day_cents.iter().sum::<i64>();
    let weekday_equipment = equip_rate * weekdays;
    let weekend_equipment = percent_of(equip_rate * weekend_days, WEEKEND_PERCENT);
    let equipment = weekday_equipment + weekend_equipment;

    // The discount applies to equipment only.
    let discount_pct = discount_percent(days);
    let discount = percent_of(equipment, discount_pct);

    // The operator is paid for every day, without discounts.
    let operator_day = if input.with_operator { input.operator_day_cents } else { 0 };
    let operator = operator_day * days;

    // Transport: two trips (delivery and collection).
    let (trip_fixed, per_km) = if input.heavy_transport {
        (HEAVY_TRIP_FIXED, HEAVY_PER_KM)
    } else {
        (LIGHT_TRIP_FIXED, LIGHT_PER_KM)
    };
    let transport = if input.delivery_km == 0 {
        0
    } else {
        2 * (trip_fixed + per_km * input.delivery_km)
    };

    let net = equipment - discount + operator + transport;
    let vat = percent_of(net, VAT_PERCENT);

    Ok(PriceBreakdown {
        days,
        weekdays,
        weekend_days,
        equip_rate_cents: equip_rate,
        weekday_equipment_cents: weekday_equipment,
        weekend_equipment_cents: weekend_equipment,
        equipment_cents: equipment,
        discount_percent: discount_pct,
        discount_cents: discount,
        operator_day_cents: operator_day,
        operator_cents: operator,
        delivery_km: input.delivery_km,
        trip_fixed_cents: trip_fixed,
        per_km_cents: per_km,
        transport_cents: transport,
        net_cents: net,
        vat_cents: vat,
        total_cents: net + vat,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    /// A Kubota KX057-6 rental with no extras, from `start` to `end`.
    fn kx057(start: NaiveDate, end: NaiveDate) -> PriceInput {
        PriceInput {
            start,
            end,
            machine_day_cents: 160_00,
            operator_day_cents: 150_00,
            with_operator: false,
            attachment_day_cents: vec![],
            delivery_km: 0,
            heavy_transport: false,
        }
    }

    #[test]
    fn reference_example_from_architecture() {
        // Fri 09.10.2026 – Thu 15.10.2026, operator, small hammer, 20 km.
        let input = PriceInput {
            with_operator: true,
            attachment_day_cents: vec![45_00],
            delivery_km: 20,
            ..kx057(d(2026, 10, 9), d(2026, 10, 15))
        };
        let p = quote(&input).unwrap();
        assert_eq!((p.days, p.weekdays, p.weekend_days), (7, 5, 2));
        assert_eq!(p.equip_rate_cents, 205_00);
        assert_eq!(p.weekday_equipment_cents, 1_025_00);
        assert_eq!(p.weekend_equipment_cents, 205_00);
        assert_eq!(p.equipment_cents, 1_230_00);
        assert_eq!(p.discount_percent, 10);
        assert_eq!(p.discount_cents, 123_00);
        assert_eq!(p.operator_cents, 1_050_00);
        assert_eq!(p.transport_cents, 154_00);
        assert_eq!(p.net_cents, 2_311_00);
        assert_eq!(p.vat_cents, 462_20);
        assert_eq!(p.total_cents, 2_773_20);
    }

    #[test]
    fn one_weekday_and_one_saturday() {
        let thu = quote(&kx057(d(2026, 10, 8), d(2026, 10, 8))).unwrap();
        assert_eq!((thu.days, thu.weekend_days, thu.equipment_cents), (1, 0, 160_00));
        assert_eq!(thu.net_cents, 160_00);
        assert_eq!(thu.total_cents, 192_00);

        let sat = quote(&kx057(d(2026, 10, 10), d(2026, 10, 10))).unwrap();
        assert_eq!((sat.days, sat.weekend_days, sat.equipment_cents), (1, 1, 80_00));
    }

    #[test]
    fn discount_tier_boundaries() {
        assert_eq!(discount_percent(1), 0);
        assert_eq!(discount_percent(6), 0);
        assert_eq!(discount_percent(7), 10);
        assert_eq!(discount_percent(13), 10);
        assert_eq!(discount_percent(14), 15);
        assert_eq!(discount_percent(29), 15);
        assert_eq!(discount_percent(30), 20);
        assert_eq!(discount_percent(90), 20);

        let start = d(2026, 10, 5); // Monday
        for (len, pct) in [(6, 0), (7, 10), (13, 10), (14, 15), (29, 15), (30, 20)] {
            let end = start + chrono::Duration::days(len - 1);
            let p = quote(&kx057(start, end)).unwrap();
            assert_eq!(p.days, len);
            assert_eq!(p.discount_percent, pct, "{len} days");
            assert_eq!(p.discount_cents, percent_of(p.equipment_cents, pct));
        }
    }

    #[test]
    fn next_discount_hint() {
        assert_eq!(next_discount(1), Some((6, 10)));
        assert_eq!(next_discount(7), Some((7, 15)));
        assert_eq!(next_discount(29), Some((1, 20)));
        assert_eq!(next_discount(30), None);
    }

    #[test]
    fn operator_is_paid_every_day_without_discount() {
        let mon_sun = (d(2026, 10, 5), d(2026, 10, 11)); // 7 days, 2 weekend
        let without = quote(&kx057(mon_sun.0, mon_sun.1)).unwrap();
        let with = quote(&PriceInput { with_operator: true, ..kx057(mon_sun.0, mon_sun.1) }).unwrap();
        assert_eq!(without.operator_cents, 0);
        assert_eq!(with.operator_cents, 7 * 150_00);
        assert_eq!(with.net_cents - without.net_cents, 7 * 150_00);
        assert_eq!(with.discount_cents, without.discount_cents);
    }

    #[test]
    fn attachments_add_to_the_day_rate() {
        let one_day = (d(2026, 10, 6), d(2026, 10, 6));
        let none = quote(&kx057(one_day.0, one_day.1)).unwrap();
        let two = quote(&PriceInput {
            attachment_day_cents: vec![8_00, 35_00],
            ..kx057(one_day.0, one_day.1)
        })
        .unwrap();
        assert_eq!(none.equip_rate_cents, 160_00);
        assert_eq!(two.equip_rate_cents, 203_00);
        assert_eq!(two.equipment_cents, 203_00);
    }

    #[test]
    fn transport_light_and_heavy() {
        let day = d(2026, 10, 6);
        let pickup = quote(&kx057(day, day)).unwrap();
        assert_eq!(pickup.transport_cents, 0);

        let light = quote(&PriceInput { delivery_km: 20, ..kx057(day, day) }).unwrap();
        assert_eq!(light.transport_cents, 2 * (45_00 + 20 * 1_60));

        let heavy = quote(&PriceInput { delivery_km: 20, heavy_transport: true, ..kx057(day, day) }).unwrap();
        assert_eq!(heavy.transport_cents, 2 * (90_00 + 20 * 2_40));
        assert_eq!((heavy.trip_fixed_cents, heavy.per_km_cents), (90_00, 2_40));

        let max = quote(&PriceInput { delivery_km: 150, ..kx057(day, day) }).unwrap();
        assert_eq!(max.transport_cents, 2 * (45_00 + 150 * 1_60));
    }

    #[test]
    fn odd_cents_round_half_up() {
        // 75,25 €/day on one Saturday → 37,625 € → 37,63 €
        let sat = d(2026, 10, 10);
        let p = quote(&PriceInput { machine_day_cents: 75_25, ..kx057(sat, sat) }).unwrap();
        assert_eq!(p.equipment_cents, 37_63);
        // VAT of 37,63 € = 7,526 € → 7,53 €
        assert_eq!(p.vat_cents, 7_53);
        assert_eq!(p.total_cents, 4516); // 45,16 €
    }

    #[test]
    fn validation_errors() {
        let start = d(2026, 10, 9);
        assert_eq!(quote(&kx057(start, d(2026, 10, 8))), Err(PriceError::EndBeforeStart));
        let day_90 = start + chrono::Duration::days(89);
        assert!(quote(&kx057(start, day_90)).is_ok());
        assert_eq!(quote(&kx057(start, day_90 + chrono::Duration::days(1))), Err(PriceError::TooLong));
        assert_eq!(quote(&PriceInput { delivery_km: 151, ..kx057(start, start) }), Err(PriceError::TooFar));
        assert_eq!(quote(&PriceInput { delivery_km: -1, ..kx057(start, start) }), Err(PriceError::NegativeKm));
        assert_eq!(PriceError::EndBeforeStart.message(), "Крайната дата е преди началната.");
    }

    #[test]
    fn weekend_counting() {
        assert_eq!(count_weekend_days(d(2026, 10, 9), d(2026, 10, 15)), 2);
        assert_eq!(count_weekend_days(d(2026, 10, 5), d(2026, 10, 9)), 0);
        assert_eq!(count_weekend_days(d(2026, 10, 10), d(2026, 10, 11)), 2);
        assert_eq!(rental_days(d(2026, 10, 9), d(2026, 10, 9)), 1);
    }
}
