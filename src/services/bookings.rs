//! Booking rules: which status changes are allowed, and validation of a
//! rental request before it is priced and saved. Pure functions.
//!
//! ```text
//!  Нова ──одобри──▶ Одобрена ──започни──▶ Активна ──приключи──▶ Приключена
//!   │  └─клиентът се отказва─▶ Отказана от клиента
//!   └──откажи──▶ Отказана ◀──откажи── Одобрена
//! ```

use chrono::NaiveDate;

use crate::models::BookingStatus;
use crate::services::pricing::{MAX_DAYS, MAX_KM, rental_days};

/// Message when a status-changing UPDATE finds the booking already changed
/// (double click, or two admins at once).
pub const ALREADY_PROCESSED: &str = "Заявката вече е обработена.";
/// Message when a blocking booking overlaps the requested dates.
pub const MACHINE_TAKEN: &str = "Машината вече е заета в избрания период. Изберете други дати.";
/// Longest free-text note we accept.
pub const MAX_NOTE_CHARS: usize = 1000;

/// Allowed status changes (everything else is rejected).
pub fn can_transition(from: BookingStatus, to: BookingStatus) -> bool {
    use BookingStatus::*;
    matches!(
        (from, to),
        (New, Approved)
            | (New, Declined)
            | (New, Cancelled)
            | (Approved, Active)
            | (Approved, Declined)
            | (Active, Completed)
    )
}

/// One validation message attached to a form field (`name` of the input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: &'static str,
    pub message: String,
}

impl FieldError {
    fn new(field: &'static str, message: &str) -> FieldError {
        FieldError { field, message: message.to_string() }
    }
}

/// Raw values of the request form (`ot`, `do`, `km`, `adres`, `belezhka`).
#[derive(Debug, Clone, Default)]
pub struct RequestForm<'a> {
    pub start: &'a str,
    pub end: &'a str,
    pub delivery_km: &'a str,
    pub delivery_address: &'a str,
    pub client_note: &'a str,
}

/// A request that passed validation.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidRequest {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub delivery_km: i64,
    /// `None` when the client collects the machine (km = 0) or left it empty.
    pub delivery_address: Option<String>,
    pub client_note: Option<String>,
}

/// Parses a `YYYY-MM-DD` date from an `<input type="date">`.
pub fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok()
}

fn optional_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

/// Checks a rental request. Returns every problem at once, so the form can
/// show all messages next to their fields.
pub fn validate_request(form: &RequestForm, today: NaiveDate) -> Result<ValidRequest, Vec<FieldError>> {
    let mut errors = Vec::new();

    let start = parse_date(form.start);
    let end = parse_date(form.end);
    if start.is_none() {
        errors.push(FieldError::new("ot", "Изберете първия ден на наема."));
    }
    if end.is_none() {
        errors.push(FieldError::new("do", "Изберете последния ден на наема."));
    }
    if let Some(start) = start
        && start < today
    {
        errors.push(FieldError::new("ot", "Първият ден не може да е в миналото."));
    }
    if let (Some(start), Some(end)) = (start, end) {
        if end < start {
            errors.push(FieldError::new("do", "Крайната дата е преди началната."));
        } else if rental_days(start, end) > MAX_DAYS {
            errors.push(FieldError::new(
                "do",
                "Максималният срок на наем е 90 дни. За по-дълъг наем се свържете с нас.",
            ));
        }
    }

    let km_text = form.delivery_km.trim();
    let km = if km_text.is_empty() { Some(0) } else { km_text.parse::<i64>().ok() };
    match km {
        None => errors.push(FieldError::new("km", "Въведете разстоянието в цели километри.")),
        Some(km) if km < 0 => errors.push(FieldError::new("km", "Невалидно разстояние.")),
        Some(km) if km > MAX_KM => errors.push(FieldError::new("km", "Доставяме до 150 км от базата.")),
        _ => {}
    }
    // an invalid distance already has its message; don't also ask for the address
    let km = km.filter(|k| (0..=MAX_KM).contains(k)).unwrap_or(0);

    let address = optional_text(form.delivery_address);
    if km > 0 && address.is_none() {
        errors.push(FieldError::new("adres", "Въведете адреса на обекта, за да докараме машината."));
    }
    if address.as_ref().is_some_and(|a| a.chars().count() > 300) {
        errors.push(FieldError::new("adres", "Адресът е твърде дълъг (до 300 символа)."));
    }
    let note = optional_text(form.client_note);
    if note.as_ref().is_some_and(|n| n.chars().count() > MAX_NOTE_CHARS) {
        errors.push(FieldError::new("belezhka", "Бележката е твърде дълга (до 1000 символа)."));
    }

    match (start, end) {
        (Some(start), Some(end)) if errors.is_empty() => Ok(ValidRequest {
            start,
            end,
            delivery_km: km,
            // the address only matters when we deliver
            delivery_address: if km > 0 { address } else { None },
            client_note: note,
        }),
        _ => Err(errors),
    }
}

/// Validates „Моточасове при връщане“ when a rental is completed.
pub fn validate_hours_at_end(current_engine_hours: i64, entered: &str) -> Result<i64, String> {
    match entered.trim().parse::<i64>() {
        Ok(hours) if hours >= current_engine_hours => Ok(hours),
        Ok(_) => Err(format!(
            "Моточасовете при връщане не може да са по-малко от сегашните ({}).",
            crate::format::number(current_engine_hours)
        )),
        Err(_) => Err("Въведете моточасовете като цяло число.".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use BookingStatus::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()
    }

    #[test]
    fn allowed_transitions() {
        let allowed = [
            (New, Approved),
            (New, Declined),
            (New, Cancelled),
            (Approved, Active),
            (Approved, Declined),
            (Active, Completed),
        ];
        for from in BookingStatus::ALL {
            for to in BookingStatus::ALL {
                assert_eq!(
                    can_transition(from, to),
                    allowed.contains(&(from, to)),
                    "{from:?} → {to:?}"
                );
            }
        }
    }

    #[test]
    fn valid_request() {
        let form = RequestForm {
            start: "2026-10-09",
            end: "2026-10-15",
            delivery_km: "20",
            delivery_address: "  с. Марково, ул. „Липа“ 4 ",
            client_note: "",
        };
        let ok = validate_request(&form, today()).unwrap();
        assert_eq!(ok.delivery_km, 20);
        assert_eq!(ok.delivery_address.as_deref(), Some("с. Марково, ул. „Липа“ 4"));
        assert_eq!(ok.client_note, None);
    }

    #[test]
    fn pickup_needs_no_address() {
        let form = RequestForm { start: "2026-10-08", end: "2026-10-08", delivery_km: "0", delivery_address: "x", ..Default::default() };
        let ok = validate_request(&form, today()).unwrap();
        assert_eq!(ok.delivery_address, None);
        let form = RequestForm { start: "2026-10-08", end: "2026-10-08", ..Default::default() };
        assert_eq!(validate_request(&form, today()).unwrap().delivery_km, 0);
    }

    #[test]
    fn invalid_requests_list_every_problem() {
        let form = RequestForm { start: "2026-10-01", end: "2026-09-30", delivery_km: "200", ..Default::default() };
        let errors = validate_request(&form, today()).unwrap_err();
        let fields: Vec<&str> = errors.iter().map(|e| e.field).collect();
        assert_eq!(fields, vec!["ot", "do", "km"]);
        assert_eq!(errors[0].message, "Първият ден не може да е в миналото.");

        let form = RequestForm { start: "", end: "утре", delivery_km: "20", ..Default::default() };
        let fields: Vec<&str> = validate_request(&form, today()).unwrap_err().iter().map(|e| e.field).collect();
        assert_eq!(fields, vec!["ot", "do", "adres"]);

        let form = RequestForm { start: "2026-10-09", end: "2027-01-07", ..Default::default() };
        assert_eq!(validate_request(&form, today()).unwrap_err()[0].field, "do"); // 91 days

        let form = RequestForm { start: "2026-10-09", end: "2026-10-10", delivery_km: "-3", ..Default::default() };
        assert_eq!(validate_request(&form, today()).unwrap_err()[0].message, "Невалидно разстояние.");
    }

    #[test]
    fn hours_at_end() {
        assert_eq!(validate_hours_at_end(4212, "4242"), Ok(4242));
        assert_eq!(validate_hours_at_end(4212, " 4212 "), Ok(4212));
        assert!(validate_hours_at_end(4212, "4000").is_err());
        assert!(validate_hours_at_end(4212, "много").is_err());
    }
}
