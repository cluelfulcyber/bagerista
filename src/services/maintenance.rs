//! Maintenance by engine hours (architecture §6). Pure functions.
//!
//! ```text
//! since_service = engine_hours − last_service_hours
//! remaining     = service_interval_hours − since_service
//! due_soon_at   = max(50, service_interval_hours / 10)
//! Overdue  if remaining ≤ 0, DueSoon if remaining ≤ due_soon_at, otherwise Ok
//! ```

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use crate::format;

/// Hours a machine typically works per rental day (pre-fills „Моточасове при връщане“).
pub const HOURS_PER_DAY_ESTIMATE: i64 = 6;
/// Hours per day used for the approval warning (a full working day).
pub const HOURS_PER_DAY_MAX: i64 = 8;
/// A technical inspection is required once a year.
pub const INSPECTION_EVERY_DAYS: i64 = 365;
pub const INSPECTION_MESSAGE: &str = "Годишен технически преглед";
pub const APPROVAL_WARNING: &str =
    "Внимание: машината ще достигне сервизния интервал по време на наема.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MaintenanceLevel {
    Ok,
    DueSoon,
    Overdue,
}

impl MaintenanceLevel {
    /// CSS modifier from design §6.5: `service--ok|due|overdue`.
    pub fn css_class(self) -> &'static str {
        match self {
            MaintenanceLevel::Ok => "service--ok",
            MaintenanceLevel::DueSoon => "service--due",
            MaintenanceLevel::Overdue => "service--overdue",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaintenanceStatus {
    pub level: MaintenanceLevel,
    /// Hours left until the next service (negative when overdue).
    pub remaining_hours: i64,
    /// Hours worked since the last scheduled service.
    pub since_service_hours: i64,
    /// Share of the interval already used, 0.0–1.0+ (for the gauge ring `--p`).
    pub used_fraction: f64,
    /// „Следващо обслужване след 412 моточаса“ / „Обслужване след 32 моточаса“ /
    /// „Просрочено обслужване (с 37 моточаса)“
    pub message: String,
    /// No inspection in the last 365 days → „Годишен технически преглед“.
    pub inspection_due: bool,
}

/// Warning threshold: 10 % of the interval, at least 50 hours.
pub fn due_soon_at(interval_hours: i64) -> i64 {
    (interval_hours / 10).max(50)
}

pub fn status(
    engine_hours: i64,
    last_service_hours: i64,
    interval_hours: i64,
    last_inspection: Option<NaiveDate>,
    today: NaiveDate,
) -> MaintenanceStatus {
    let since = engine_hours - last_service_hours;
    let remaining = interval_hours - since;

    let (level, message) = if remaining <= 0 {
        (
            MaintenanceLevel::Overdue,
            format!("Просрочено обслужване (с {})", format::engine_hours(-remaining)),
        )
    } else if remaining <= due_soon_at(interval_hours) {
        (MaintenanceLevel::DueSoon, format!("Обслужване след {}", format::engine_hours(remaining)))
    } else {
        (MaintenanceLevel::Ok, format!("Следващо обслужване след {}", format::engine_hours(remaining)))
    };

    let inspection_due = match last_inspection {
        Some(day) => day < today - Duration::days(INSPECTION_EVERY_DAYS),
        None => true,
    };

    MaintenanceStatus {
        level,
        remaining_hours: remaining,
        since_service_hours: since,
        used_fraction: if interval_hours > 0 { since as f64 / interval_hours as f64 } else { 0.0 },
        message,
        inspection_due,
    }
}

/// Approval warning: will the rental push the machine past its service interval?
/// (architecture §6: `remaining < days × 8`)
pub fn will_reach_interval(remaining_hours: i64, rental_days: i64) -> bool {
    remaining_hours < rental_days * HOURS_PER_DAY_MAX
}

/// Pre-filled value for „Моточасове при връщане“.
pub fn suggested_hours_at_end(engine_hours: i64, rental_days: i64) -> i64 {
    engine_hours + rental_days * HOURS_PER_DAY_ESTIMATE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()
    }

    /// Status of a 500 h machine serviced at 1 000 h with `remaining` hours left.
    fn with_remaining(remaining: i64) -> MaintenanceStatus {
        status(1000 + 500 - remaining, 1000, 500, Some(today()), today())
    }

    #[test]
    fn boundaries_for_500_hour_interval() {
        assert_eq!(due_soon_at(500), 50);
        let s = with_remaining(51);
        assert_eq!((s.level, s.remaining_hours), (MaintenanceLevel::Ok, 51));
        assert_eq!(s.message, "Следващо обслужване след 51 моточаса");

        let s = with_remaining(50);
        assert_eq!(s.level, MaintenanceLevel::DueSoon);
        assert_eq!(s.message, "Обслужване след 50 моточаса");

        let s = with_remaining(1);
        assert_eq!(s.level, MaintenanceLevel::DueSoon);
        assert_eq!(s.message, "Обслужване след 1 моточас");

        let s = with_remaining(0);
        assert_eq!(s.level, MaintenanceLevel::Overdue);
        assert_eq!(s.message, "Просрочено обслужване (с 0 моточаса)");

        let s = with_remaining(-1);
        assert_eq!(s.level, MaintenanceLevel::Overdue);
        assert_eq!(s.message, "Просрочено обслужване (с 1 моточас)");

        let s = with_remaining(-37);
        assert_eq!(s.message, "Просрочено обслужване (с 37 моточаса)");
    }

    #[test]
    fn due_soon_threshold_depends_on_interval() {
        assert_eq!(due_soon_at(250), 50); // 25 would be too late, minimum is 50
        assert_eq!(due_soon_at(1000), 100);
        // 250 h interval: 50 h left is already „soon“
        let s = status(200, 0, 250, Some(today()), today());
        assert_eq!(s.level, MaintenanceLevel::DueSoon);
        let s = status(199, 0, 250, Some(today()), today());
        assert_eq!(s.level, MaintenanceLevel::Ok);
        // 1000 h interval: 100 h left is „soon“
        assert_eq!(status(900, 0, 1000, Some(today()), today()).level, MaintenanceLevel::DueSoon);
        assert_eq!(status(899, 0, 1000, Some(today()), today()).level, MaintenanceLevel::Ok);
    }

    #[test]
    fn mockup_example_kx057() {
        // Kubota KX057-6: 3 412 h, serviced at 3 324 h → 88 since, 412 left
        let s = status(3412, 3324, 500, Some(today()), today());
        assert_eq!(s.since_service_hours, 88);
        assert_eq!(s.remaining_hours, 412);
        assert_eq!(s.message, "Следващо обслужване след 412 моточаса");
        assert!((s.used_fraction - 0.176).abs() < 1e-9);
    }

    #[test]
    fn yearly_inspection() {
        let t = today();
        assert!(status(0, 0, 500, None, t).inspection_due);
        assert!(!status(0, 0, 500, Some(t - Duration::days(365)), t).inspection_due);
        assert!(status(0, 0, 500, Some(t - Duration::days(366)), t).inspection_due);
    }

    #[test]
    fn approval_warning_and_prefill() {
        // 40 h left, 7-day rental → up to 56 h of work → warn
        assert!(will_reach_interval(40, 7));
        assert!(!will_reach_interval(56, 7));
        assert!(will_reach_interval(-5, 1));
        assert_eq!(suggested_hours_at_end(3412, 7), 3454);
    }

    #[test]
    fn levels_sort_by_severity() {
        assert!(MaintenanceLevel::Overdue > MaintenanceLevel::DueSoon);
        assert!(MaintenanceLevel::DueSoon > MaintenanceLevel::Ok);
        assert_eq!(MaintenanceLevel::DueSoon.css_class(), "service--due");
    }
}
