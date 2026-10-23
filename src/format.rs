//! Bulgarian number, money and date formatting (used by templates and messages).
//!
//! Conventions:
//! - thousands are separated by a NO-BREAK SPACE (U+00A0), decimals by a comma:
//!   `1 230,00 €`, `5 500 кг`, `3,80 м`;
//! - the euro sign follows the amount after a no-break space;
//! - negative amounts use the real minus sign „−“ (U+2212);
//! - dates: „9 октомври 2026“ in prose, „09.10.2026“ in tables, „9 окт.“ in badges.

use chrono::{Datelike, NaiveDate, Weekday};

/// The no-break space used between digit groups and before units.
pub const NBSP: char = '\u{a0}';

const MONTHS: [&str; 12] = [
    "януари", "февруари", "март", "април", "май", "юни", "юли", "август", "септември",
    "октомври", "ноември", "декември",
];
/// Short month names with a dot, for badges: „9 окт.“ (май and юни/юли stay whole).
const MONTHS_SHORT: [&str; 12] = [
    "яну.", "фев.", "март", "апр.", "май", "юни", "юли", "авг.", "сеп.", "окт.", "ное.", "дек.",
];
/// Three-letter month names without a dot, for chart axes.
const MONTHS_AXIS: [&str; 12] = [
    "яну", "фев", "мар", "апр", "май", "юни", "юли", "авг", "сеп", "окт", "ное", "дек",
];
const WEEKDAYS: [&str; 7] = ["понеделник", "вторник", "сряда", "четвъртък", "петък", "събота", "неделя"];
const WEEKDAYS_SHORT: [&str; 7] = ["пн", "вт", "ср", "чт", "пт", "сб", "нд"];

/// Groups the digits of a non-negative integer by three: 1234567 → „1 234 567“.
fn group_digits(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(NBSP);
        }
        out.push(ch);
    }
    out
}

/// Whole number with grouped thousands: 5500 → „5 500“, -1200 → „−1 200“.
pub fn number(n: i64) -> String {
    let sign = if n < 0 { "−" } else { "" };
    format!("{sign}{}", group_digits(n.unsigned_abs()))
}

/// Cents as an amount without the currency sign: 277320 → „2 773,20“.
/// Used in the print quote table, where the column header says „Сума, €“.
pub fn amount(cents: i64) -> String {
    let sign = if cents < 0 { "−" } else { "" };
    let abs = cents.unsigned_abs();
    format!("{sign}{},{:02}", group_digits(abs / 100), abs % 100)
}

/// Cents as euros: 277320 → „2 773,20 €“, -12300 → „−123,00 €“.
pub fn money(cents: i64) -> String {
    format!("{}{NBSP}€", amount(cents))
}

/// Rounded to whole euros (half up), for dashboard tiles: 690049 → „6 900 €“.
pub fn money_whole(cents: i64) -> String {
    let euros = (cents.abs() + 50) / 100;
    let sign = if cents < 0 { "−" } else { "" };
    format!("{sign}{}{NBSP}€", group_digits(euros as u64))
}

/// Thousands of euros with one decimal, for chart labels: 1_890_000 cents → „18,9“.
pub fn thousands_one_decimal(cents: i64) -> String {
    // cents → tenths of a thousand euros = cents / 10 000, rounded half up
    let tenths = (cents.abs() + 5_000) / 10_000;
    let sign = if cents < 0 { "−" } else { "" };
    format!("{sign}{},{}", group_digits((tenths / 10) as u64), tenths % 10)
}

/// Percentage: 10 → „10 %“.
pub fn percent(p: i64) -> String {
    format!("{p}{NBSP}%")
}

/// Millimetres as metres with two decimals: 3800 → „3,80 м“.
pub fn meters(mm: i64) -> String {
    let cm = (mm + 5).div_euclid(10); // round to centimetres
    format!("{},{:02}{NBSP}м", cm / 100, cm % 100)
}

/// Millimetres as metres with at most one decimal: 3800 → „3,8 м“, 2000 → „2 м“.
/// Used in sentences („Копае до 3,8 м …“).
pub fn meters_short(mm: i64) -> String {
    let dm = (mm + 50).div_euclid(100); // round to decimetres
    if dm % 10 == 0 {
        format!("{}{NBSP}м", dm / 10)
    } else {
        format!("{},{}{NBSP}м", dm / 10, dm % 10)
    }
}

/// Millimetres with the unit: 3800 → „3 800 мм“.
pub fn millimeters(mm: i64) -> String {
    format!("{}{NBSP}мм", number(mm))
}

/// Kilograms with the unit: 5500 → „5 500 кг“ (never „т“, see design §4.3).
pub fn kg(weight: i64) -> String {
    format!("{}{NBSP}кг", number(weight))
}

/// Engine power: 34.7 → „34,7 kW“.
pub fn kw(power: f64) -> String {
    format!("{:.1}{NBSP}kW", power).replace('.', ",")
}

/// Full date for prose: „9 октомври 2026“.
pub fn date_bg(d: NaiveDate) -> String {
    format!("{} {} {}", d.day(), month_name(d.month()), d.year())
}

/// Day and month for prose: „9 октомври“.
pub fn day_month(d: NaiveDate) -> String {
    format!("{} {}", d.day(), month_name(d.month()))
}

/// Short date for badges: „9 окт.“.
pub fn date_short(d: NaiveDate) -> String {
    format!("{}{NBSP}{}", d.day(), MONTHS_SHORT[d.month0() as usize])
}

/// Short date with the year: „9 окт. 2026“.
pub fn date_short_year(d: NaiveDate) -> String {
    format!("{} {}", date_short(d), d.year())
}

/// Numeric date for tables and documents: „09.10.2026“.
pub fn date_num(d: NaiveDate) -> String {
    d.format("%d.%m.%Y").to_string()
}

/// Numeric period for tables: „09.10–15.10.2026“ (year written once when equal).
pub fn period_num(start: NaiveDate, end: NaiveDate) -> String {
    if start.year() == end.year() {
        format!("{}–{}", start.format("%d.%m"), date_num(end))
    } else {
        format!("{}–{}", date_num(start), date_num(end))
    }
}

/// Period in prose: „9 октомври – 15 октомври 2026“.
pub fn period_bg(start: NaiveDate, end: NaiveDate) -> String {
    if start.year() == end.year() {
        format!("{} – {}", day_month(start), date_bg(end))
    } else {
        format!("{} – {}", date_bg(start), date_bg(end))
    }
}

/// „октомври“ for month number 10.
pub fn month_name(month: u32) -> &'static str {
    MONTHS[(month.clamp(1, 12) - 1) as usize]
}

/// „октомври 2026“ for the calendar header.
pub fn month_year(first_day: NaiveDate) -> String {
    format!("{} {}", month_name(first_day.month()), first_day.year())
}

/// „окт“ for chart axes.
pub fn month_axis(month: u32) -> &'static str {
    MONTHS_AXIS[(month.clamp(1, 12) - 1) as usize]
}

/// „петък“.
pub fn weekday_name(d: NaiveDate) -> &'static str {
    WEEKDAYS[d.weekday().num_days_from_monday() as usize]
}

/// „пт“.
pub fn weekday_short(day: Weekday) -> &'static str {
    WEEKDAYS_SHORT[day.num_days_from_monday() as usize]
}

/// Long date with the weekday, for the admin header: „четвъртък, 8 октомври 2026“.
pub fn date_with_weekday(d: NaiveDate) -> String {
    format!("{}, {}", weekday_name(d), date_bg(d))
}

/// Bulgarian plural: 1 → singular form, everything else → plural form.
/// `plural(1, "ден", "дни")` = „ден“, `plural(7, "ден", "дни")` = „дни“.
pub fn plural(n: i64, one: &'static str, many: &'static str) -> &'static str {
    if n.abs() == 1 { one } else { many }
}

/// „1 ден“, „7 дни“.
pub fn days(n: i64) -> String {
    format!("{} {}", number(n), plural(n, "ден", "дни"))
}

/// „1 машина“, „14 машини“.
pub fn machines(n: i64) -> String {
    format!("{} {}", number(n), plural(n, "машина", "машини"))
}

/// „1 моточас“, „412 моточаса“.
pub fn engine_hours(n: i64) -> String {
    format!("{} {}", number(n), plural(n, "моточас", "моточаса"))
}

/// „1 заявка“, „4 заявки“.
pub fn requests(n: i64) -> String {
    format!("{} {}", number(n), plural(n, "заявка", "заявки"))
}

/// Booking number shown to users: year of creation + id → „2026-0142“.
pub fn booking_number(year: i32, id: i64) -> String {
    format!("{year}-{id:04}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn money_uses_bulgarian_format() {
        assert_eq!(money(277_320), "2\u{a0}773,20\u{a0}€");
        assert_eq!(money(0), "0,00\u{a0}€");
        assert_eq!(money(5), "0,05\u{a0}€");
        assert_eq!(money(-12_300), "−123,00\u{a0}€");
        assert_eq!(money(123_456_789), "1\u{a0}234\u{a0}567,89\u{a0}€");
        assert_eq!(amount(102_500), "1\u{a0}025,00");
    }

    #[test]
    fn whole_and_chart_amounts() {
        assert_eq!(money_whole(690_049), "6\u{a0}900\u{a0}€");
        assert_eq!(money_whole(690_050), "6\u{a0}901\u{a0}€");
        assert_eq!(thousands_one_decimal(1_890_000), "18,9");
        assert_eq!(thousands_one_decimal(694_999), "6,9");
        assert_eq!(thousands_one_decimal(0), "0,0");
    }

    #[test]
    fn units() {
        assert_eq!(number(5500), "5\u{a0}500");
        assert_eq!(number(999), "999");
        assert_eq!(kg(30_000), "30\u{a0}000\u{a0}кг");
        assert_eq!(meters(3800), "3,80\u{a0}м");
        assert_eq!(meters(990), "0,99\u{a0}м");
        assert_eq!(meters_short(3800), "3,8\u{a0}м");
        assert_eq!(meters_short(2000), "2\u{a0}м");
        assert_eq!(millimeters(3800), "3\u{a0}800\u{a0}мм");
        assert_eq!(kw(34.7), "34,7\u{a0}kW");
        assert_eq!(kw(123.0), "123,0\u{a0}kW");
        assert_eq!(percent(10), "10\u{a0}%");
    }

    #[test]
    fn dates() {
        let day = d(2026, 10, 9);
        assert_eq!(date_bg(day), "9 октомври 2026");
        assert_eq!(date_short(day), "9\u{a0}окт.");
        assert_eq!(date_short_year(day), "9\u{a0}окт. 2026");
        assert_eq!(date_num(day), "09.10.2026");
        assert_eq!(weekday_name(day), "петък");
        assert_eq!(period_num(day, d(2026, 10, 15)), "09.10–15.10.2026");
        assert_eq!(period_bg(day, d(2026, 10, 15)), "9 октомври – 15 октомври 2026");
        assert_eq!(period_num(d(2026, 12, 30), d(2027, 1, 2)), "30.12.2026–02.01.2027");
        assert_eq!(month_year(d(2026, 10, 1)), "октомври 2026");
        assert_eq!(date_with_weekday(d(2026, 10, 8)), "четвъртък, 8 октомври 2026");
    }

    #[test]
    fn plurals() {
        assert_eq!(days(1), "1 ден");
        assert_eq!(days(7), "7 дни");
        assert_eq!(machines(1), "1 машина");
        assert_eq!(machines(14), "14 машини");
        assert_eq!(engine_hours(1), "1 моточас");
        assert_eq!(engine_hours(412), "412 моточаса");
        assert_eq!(requests(4), "4 заявки");
        assert_eq!(booking_number(2026, 142), "2026-0142");
    }
}
