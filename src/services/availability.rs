//! Availability rules (architecture §3.1): date overlaps, the month calendar
//! and the „Свободна / Заета до …“ badge. Pure functions, no database.

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;

use crate::format;
use crate::models::BookingStatus;

/// Two inclusive date ranges overlap if each starts before (or on the day)
/// the other ends. Same formula as the SQL check:
/// `existing.start_date <= new.end_date AND existing.end_date >= new.start_date`.
pub fn overlaps(a_start: NaiveDate, a_end: NaiveDate, b_start: NaiveDate, b_end: NaiveDate) -> bool {
    a_start <= b_end && a_end >= b_start
}

/// A booking reduced to what matters for availability.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub booking_id: i64,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub status: BookingStatus,
}

/// Bookings that clash with `target`: same dates, not the same booking, and
/// still „alive“ (new, approved or active). The caller decides what to do:
/// a blocking one (approved/active) prevents approval, a `new` one is only a
/// warning for the admin.
pub fn find_conflicts<'a>(target: &Span, others: &'a [Span]) -> Vec<&'a Span> {
    others
        .iter()
        .filter(|o| o.booking_id != target.booking_id)
        .filter(|o| {
            matches!(o.status, BookingStatus::New | BookingStatus::Approved | BookingStatus::Active)
        })
        .filter(|o| overlaps(o.start, o.end, target.start, target.end))
        .collect()
}

/// State of one calendar day as seen by the public (pending requests are not shown).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DayState {
    Free,
    Booked,
    Past,
}

impl DayState {
    pub fn code(self) -> &'static str {
        match self {
            DayState::Free => "free",
            DayState::Booked => "booked",
            DayState::Past => "past",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DayState::Free => "свободен",
            DayState::Booked => "зает",
            DayState::Past => "минал",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalendarDay {
    pub date: NaiveDate,
    pub state: DayState,
    pub is_weekend: bool,
    pub is_today: bool,
}

impl CalendarDay {
    /// Full accessible label: „9 октомври, петък, свободен“.
    pub fn aria_label(&self) -> String {
        format!(
            "{}, {}, {}",
            format::day_month(self.date),
            format::weekday_name(self.date),
            self.state.label()
        )
    }
}

/// Parses `?mesec=2026-10` into the first day of that month.
pub fn parse_month(text: &str) -> Option<NaiveDate> {
    let (year, month) = text.trim().split_once('-')?;
    NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, 1)
}

/// `2026-10` for links and the API.
pub fn month_param(first_day: NaiveDate) -> String {
    format!("{:04}-{:02}", first_day.year(), first_day.month())
}

/// First day of the month containing `day`.
pub fn first_of_month(day: NaiveDate) -> NaiveDate {
    day.with_day(1).expect("day 1 always exists")
}

/// First day of the month `n` months after (or before, if negative) `first_day`'s month.
pub fn add_months(first_day: NaiveDate, n: i32) -> NaiveDate {
    let index = first_day.year() * 12 + first_day.month0() as i32 + n;
    NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1)
        .expect("valid month")
}

/// Last day of the month that starts on `first_day`.
pub fn last_of_month(first_day: NaiveDate) -> NaiveDate {
    add_months(first_of_month(first_day), 1) - Duration::days(1)
}

/// Empty cells before day 1 in a Monday-first grid (Monday → 0, Sunday → 6).
pub fn leading_blanks(first_day: NaiveDate) -> u32 {
    first_day.weekday().num_days_from_monday()
}

/// All days of a month with their public state.
/// `blocking` are the date ranges of approved/active bookings of the machine.
/// Days before today are `Past` (they cannot be chosen, booked or not).
pub fn month_view(first_day: NaiveDate, today: NaiveDate, blocking: &[(NaiveDate, NaiveDate)]) -> Vec<CalendarDay> {
    let first = first_of_month(first_day);
    let last = last_of_month(first);
    first
        .iter_days()
        .take_while(|day| *day <= last)
        .map(|date| {
            let booked = blocking.iter().any(|(s, e)| overlaps(*s, *e, date, date));
            let state = if date < today {
                DayState::Past
            } else if booked {
                DayState::Booked
            } else {
                DayState::Free
            };
            CalendarDay {
                date,
                state,
                is_weekend: crate::services::pricing::is_weekend(date),
                is_today: date == today,
            }
        })
        .collect()
}

/// Availability badge on cards and tables (design §6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Free for at least the next `BADGE_HORIZON_DAYS` days.
    Free,
    /// Free today, the next booking starts the day after the given date.
    FreeUntil(NaiveDate),
    /// Booked today, free again the day after the given date.
    BusyUntil(NaiveDate),
}

/// „Свободна до …“ is shown only if the next booking starts within this many days.
pub const BADGE_HORIZON_DAYS: i64 = 30;

impl Availability {
    /// „Свободна“, „Свободна до 18 окт.“, „Заета до 14 окт.“
    pub fn label(self) -> String {
        match self {
            Availability::Free => "Свободна".to_string(),
            Availability::FreeUntil(day) => format!("Свободна до {}", format::date_short(day)),
            Availability::BusyUntil(day) => format!("Заета до {}", format::date_short(day)),
        }
    }

    /// CSS modifier: `badge--free` / `badge--busy`.
    pub fn css_class(self) -> &'static str {
        match self {
            Availability::Free | Availability::FreeUntil(_) => "badge--free",
            Availability::BusyUntil(_) => "badge--busy",
        }
    }

    pub fn is_free_today(self) -> bool {
        !matches!(self, Availability::BusyUntil(_))
    }
}

/// Works out the badge from the machine's blocking bookings.
/// Back-to-back bookings are merged: busy 3–10 and 11–14 → „Заета до 14 окт.“
pub fn availability(today: NaiveDate, blocking: &[(NaiveDate, NaiveDate)]) -> Availability {
    let mut ranges: Vec<(NaiveDate, NaiveDate)> =
        blocking.iter().copied().filter(|(_, end)| *end >= today).collect();
    ranges.sort();

    // Busy today? Then jump to the end of the booking covering `day`, and keep
    // jumping while another booking covers the following day.
    let mut busy_until = None;
    let mut day = today;
    while let Some(end) = ranges.iter().filter(|(s, e)| *s <= day && *e >= day).map(|(_, e)| *e).max() {
        busy_until = Some(end);
        day = end + Duration::days(1);
    }
    if let Some(end) = busy_until {
        return Availability::BusyUntil(end);
    }

    match ranges.iter().map(|(s, _)| *s).filter(|s| *s > today).min() {
        Some(next_start) if (next_start - today).num_days() <= BADGE_HORIZON_DAYS => {
            Availability::FreeUntil(next_start - Duration::days(1))
        }
        _ => Availability::Free,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    fn span(id: i64, s: NaiveDate, e: NaiveDate, status: BookingStatus) -> Span {
        Span { booking_id: id, start: s, end: e, status }
    }

    #[test]
    fn overlap_cases() {
        // identical
        assert!(overlaps(d(10, 10), d(10, 12), d(10, 10), d(10, 12)));
        // touching: one ends on the day the other starts → overlap (inclusive ranges)
        assert!(overlaps(d(10, 10), d(10, 12), d(10, 12), d(10, 15)));
        assert!(overlaps(d(10, 12), d(10, 15), d(10, 10), d(10, 12)));
        // adjacent: ends the day before → no overlap
        assert!(!overlaps(d(10, 10), d(10, 12), d(10, 13), d(10, 15)));
        assert!(!overlaps(d(10, 13), d(10, 15), d(10, 10), d(10, 12)));
        // nested
        assert!(overlaps(d(10, 1), d(10, 31), d(10, 10), d(10, 12)));
        assert!(overlaps(d(10, 10), d(10, 12), d(10, 1), d(10, 31)));
        // disjoint
        assert!(!overlaps(d(10, 1), d(10, 3), d(10, 20), d(10, 25)));
        // single days
        assert!(overlaps(d(10, 5), d(10, 5), d(10, 5), d(10, 5)));
    }

    #[test]
    fn conflicts_ignore_self_and_finished_bookings() {
        let target = span(1, d(10, 12), d(10, 16), BookingStatus::New);
        let others = vec![
            target.clone(),
            span(2, d(10, 12), d(10, 16), BookingStatus::Approved),
            span(3, d(10, 14), d(10, 20), BookingStatus::New),
            span(4, d(10, 12), d(10, 16), BookingStatus::Declined),
            span(5, d(10, 12), d(10, 16), BookingStatus::Cancelled),
            span(6, d(10, 17), d(10, 20), BookingStatus::Approved),
        ];
        let ids: Vec<i64> = find_conflicts(&target, &others).iter().map(|s| s.booking_id).collect();
        assert_eq!(ids, vec![2, 3]);
    }

    #[test]
    fn month_view_marks_past_booked_and_weekends() {
        let today = d(10, 8);
        let days = month_view(d(10, 1), today, &[(d(10, 19), d(10, 23)), (d(9, 28), d(10, 2))]);
        assert_eq!(days.len(), 31);
        assert_eq!(days[0].state, DayState::Past); // 1 Oct, booked but in the past
        assert_eq!(days[6].state, DayState::Past); // 7 Oct
        assert_eq!(days[7].state, DayState::Free); // today
        assert!(days[7].is_today);
        assert_eq!(days[18].state, DayState::Booked); // 19 Oct
        assert_eq!(days[22].state, DayState::Booked); // 23 Oct
        assert_eq!(days[23].state, DayState::Free); // 24 Oct
        assert!(days[9].is_weekend && days[10].is_weekend); // 10, 11 Oct
        assert!(!days[8].is_weekend);
        assert_eq!(days[8].aria_label(), "9 октомври, петък, свободен");
        assert_eq!(leading_blanks(d(10, 1)), 3); // 1 Oct 2026 is a Thursday
    }

    #[test]
    fn month_helpers() {
        assert_eq!(parse_month("2026-10"), Some(d(10, 1)));
        assert_eq!(parse_month("2026-13"), None);
        assert_eq!(parse_month("октомври"), None);
        assert_eq!(month_param(d(10, 1)), "2026-10");
        assert_eq!(add_months(d(10, 1), 3), NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
        assert_eq!(add_months(d(1, 1), -1), NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());
        assert_eq!(last_of_month(d(2, 1)), d(2, 28));
        assert_eq!(last_of_month(d(12, 1)), d(12, 31));
    }

    #[test]
    fn availability_badges() {
        let today = d(10, 8);
        assert_eq!(availability(today, &[]), Availability::Free);
        // the mockup: KX057-6 free until 18 Oct, ViO26-6 busy until 14 Oct
        assert_eq!(availability(today, &[(d(10, 19), d(10, 23))]), Availability::FreeUntil(d(10, 18)));
        assert_eq!(availability(today, &[(d(10, 3), d(10, 14))]), Availability::BusyUntil(d(10, 14)));
        // back-to-back bookings merge
        assert_eq!(
            availability(today, &[(d(10, 3), d(10, 10)), (d(10, 11), d(10, 14)), (d(10, 20), d(10, 22))]),
            Availability::BusyUntil(d(10, 14))
        );
        // a booking starting tomorrow
        assert_eq!(availability(today, &[(d(10, 9), d(10, 9))]), Availability::FreeUntil(d(10, 8)));
        // far away and finished bookings do not matter
        assert_eq!(availability(today, &[(d(12, 1), d(12, 5)), (d(9, 1), d(9, 5))]), Availability::Free);
        assert_eq!(Availability::BusyUntil(d(10, 14)).label(), "Заета до 14\u{a0}окт.");
        assert_eq!(Availability::FreeUntil(d(10, 18)).label(), "Свободна до 18\u{a0}окт.");
    }
}
