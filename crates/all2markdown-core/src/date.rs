//! Dates, as the four formats happen to write them, turned into the one
//! representation Document Metadata carries.
//!
//! A Unix timestamp rather than a calendar type: the point of a normalised
//! core is that a mixed corpus sorts by date, and every format here writes a
//! fixed-shape date that needs no calendar library to read.

/// Days from 1970-01-01 to a civil date, by Howard Hinnant's `days_from_civil`.
///
/// Correct for every proleptic Gregorian date, which matters because document
/// metadata is full of 1601 epochs and clocks set wrong.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = month as i64;
    let day = day as i64;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// A Unix timestamp from calendar parts, with no validation beyond what the
/// arithmetic needs: a document that declares month 13 gets a date that is
/// wrong in the same way the document is.
pub(crate) fn from_parts(year: i64, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> i64 {
    days_from_civil(year, month.max(1), day.max(1)) * 86_400
        + hour as i64 * 3600
        + min as i64 * 60
        + sec as i64
}

/// `2024-01-15T10:30:00Z`, as OOXML writes it.
///
/// The trailing zone is ignored rather than applied: OOXML dates are UTC in
/// practice, and a wrong offset is worse than none.
pub(crate) fn iso8601(text: &str) -> Option<i64> {
    let text = text.trim();
    let digits: Vec<u32> = text.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() < 8 {
        return None;
    }
    let take = |from: usize, len: usize| -> u32 {
        digits[from..from + len]
            .iter()
            .fold(0, |acc, d| acc * 10 + d)
    };
    let seconds = if digits.len() >= 14 { take(12, 2) } else { 0 };
    Some(from_parts(
        take(0, 4) as i64,
        take(4, 2),
        take(6, 2),
        if digits.len() >= 10 { take(8, 2) } else { 0 },
        if digits.len() >= 12 { take(10, 2) } else { 0 },
        seconds,
    ))
}

/// `D:20240115103000+01'00'`, as PDF writes it.
pub(crate) fn pdf(text: &str) -> Option<i64> {
    iso8601(text.trim().strip_prefix("D:").unwrap_or(text))
}
