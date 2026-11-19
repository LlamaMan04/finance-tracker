use crate::application::ApplicationError;
use serde::Serialize;

/// Represents a calendar date without a time zone or time-of-day component.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct LocalDate {
    year: u16,
    month: u8,
    day: u8,
}

impl LocalDate {
    /// Creates a valid proleptic Gregorian calendar date.
    pub fn from_ymd(year: u16, month: u8, day: u8) -> Result<Self, ApplicationError> {
        let is_valid =
            year > 0 && (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month);

        if !is_valid {
            return Err(ApplicationError::validation());
        }

        Ok(Self { year, month, day })
    }

    /// Parses an exact YYYY-MM-DD date without accepting locale-dependent forms.
    pub fn parse(value: &str) -> Result<Self, ApplicationError> {
        let bytes = value.as_bytes();
        let has_expected_shape = bytes.len() == 10 && bytes[4] == b'-' && bytes[7] == b'-';

        if !has_expected_shape {
            return Err(ApplicationError::validation());
        }

        let year = parse_digits(&bytes[0..4])?;
        let month = parse_digits(&bytes[5..7])?;
        let day = parse_digits(&bytes[8..10])?;

        Self::from_ymd(year, month as u8, day as u8)
    }

    /// Returns the calendar year component.
    pub fn year(&self) -> u16 {
        self.year
    }

    /// Returns the one-based month component.
    pub fn month(&self) -> u8 {
        self.month
    }

    /// Returns the one-based day component.
    pub fn day(&self) -> u8 {
        self.day
    }
}

/// Represents an inclusive planning period with a valid chronological range.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PlanningPeriod {
    start: LocalDate,
    end: LocalDate,
}

impl PlanningPeriod {
    /// Creates an inclusive period when its end is not before its start.
    pub fn new(start: LocalDate, end: LocalDate) -> Result<Self, ApplicationError> {
        if end < start {
            return Err(ApplicationError::validation());
        }

        Ok(Self { start, end })
    }

    /// Returns the inclusive start date.
    pub fn start(&self) -> LocalDate {
        self.start
    }

    /// Returns the inclusive end date.
    pub fn end(&self) -> LocalDate {
        self.end
    }
}

/// Parses an ASCII decimal slice into a bounded unsigned integer.
fn parse_digits(value: &[u8]) -> Result<u16, ApplicationError> {
    let mut parsed = 0_u16;

    for character in value {
        if !character.is_ascii_digit() {
            return Err(ApplicationError::validation());
        }

        parsed = parsed
            .checked_mul(10)
            .and_then(|number| number.checked_add(u16::from(character - b'0')))
            .ok_or_else(ApplicationError::validation)?;
    }

    Ok(parsed)
}

/// Returns the number of valid days in a Gregorian month.
fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Determines leap years using the Gregorian calendar rule.
fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

#[cfg(test)]
mod tests {
    use super::{LocalDate, PlanningPeriod};
    use crate::application::ErrorCode;

    #[test]
    fn parses_leap_day_and_rejects_invalid_calendar_days() {
        let leap_day = LocalDate::parse("2024-02-29").expect("2024 is a leap year");
        let invalid_day = LocalDate::parse("2023-02-29").expect_err("2023 is not a leap year");

        assert_eq!((leap_day.year(), leap_day.month(), leap_day.day()), (2024, 2, 29));
        assert_eq!(invalid_day.code, ErrorCode::Validation);
    }

    #[test]
    fn rejects_non_canonical_date_formats() {
        assert!(LocalDate::parse("2024/02/29").is_err());
        assert!(LocalDate::parse("24-02-29").is_err());
    }

    #[test]
    fn rejects_periods_that_end_before_they_start() {
        let start = LocalDate::parse("2026-04-01").expect("fixture date is valid");
        let end = LocalDate::parse("2026-03-31").expect("fixture date is valid");
        let error = PlanningPeriod::new(start, end).expect_err("period order must be valid");

        assert_eq!(error.code, ErrorCode::Validation);
    }
}
