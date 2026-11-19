//! The one date format HTTP uses in headers: `Sun, 06 Nov 1994 08:49:37 GMT`.
//!
//! The standard library can count seconds since 1970 but not turn them into
//! a calendar date, so the conversion is done here.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const SECONDS_PER_DAY: u64 = 86_400;

/// Format a time for a `Date` or `Last-Modified` header.
pub fn http_date(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (days, rest) = (seconds / SECONDS_PER_DAY, seconds % SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days as i64);
    // 1 January 1970 was a Thursday.
    let weekday = WEEKDAYS[((days + 4) % 7) as usize];
    format!(
        "{weekday}, {day:02} {} {year} {:02}:{:02}:{:02} GMT",
        MONTHS[month as usize - 1],
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Parse a date in the format `http_date` produces.
pub fn parse_http_date(text: &str) -> Option<SystemTime> {
    let mut parts = text.split_whitespace();
    let (_weekday, day, month, year) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let (clock, zone) = (parts.next()?, parts.next()?);
    if zone != "GMT" || parts.next().is_some() {
        return None;
    }
    let day: u32 = day.parse().ok()?;
    let month = MONTHS.iter().position(|name| *name == month)? as u32 + 1;
    let year: i64 = year.parse().ok()?;
    let mut clock = clock.split(':').map(|part| part.parse::<u64>().ok());
    let (hour, minute, second) = (clock.next()??, clock.next()??, clock.next()??);
    let in_range =
        (1..=31).contains(&day) && year >= 1970 && hour < 24 && minute < 60 && second < 61;
    if !in_range || clock.next().is_some() {
        return None;
    }
    let days = days_from_civil(year, month, day) as u64;
    let seconds = days * SECONDS_PER_DAY + hour * 3600 + minute * 60 + second;
    Some(UNIX_EPOCH + Duration::from_secs(seconds))
}

/// Days since 1 January 1970 to a (year, month, day).
///
/// The trick is to count years from 1 March, so the leap day falls at the
/// very end of the year and every 400-year cycle has the same shape.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = (if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    }) as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The inverse of `civil_from_days`.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let shifted_month = (i64::from(month) + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn formats_the_example_from_the_specification() {
        assert_eq!(http_date(at(784_111_777)), "Sun, 06 Nov 1994 08:49:37 GMT");
        assert_eq!(http_date(UNIX_EPOCH), "Thu, 01 Jan 1970 00:00:00 GMT");
    }

    #[test]
    fn handles_leap_years_and_century_rules() {
        // 2000 was a leap year, because it is divisible by 400.
        assert_eq!(http_date(at(951_782_400)), "Tue, 29 Feb 2000 00:00:00 GMT");
        assert_eq!(http_date(at(951_868_800)), "Wed, 01 Mar 2000 00:00:00 GMT");
        assert_eq!(
            http_date(at(1_709_251_199)),
            "Thu, 29 Feb 2024 23:59:59 GMT"
        );
        // 2100 will not be one.
        assert_eq!(
            http_date(at(4_107_542_400)),
            "Mon, 01 Mar 2100 00:00:00 GMT"
        );
    }

    #[test]
    fn parsing_reverses_formatting() {
        for seconds in [0, 784_111_777, 951_782_400, 1_709_251_199, 4_107_542_400] {
            let text = http_date(at(seconds));
            assert_eq!(parse_http_date(&text), Some(at(seconds)), "{text}");
        }
    }

    #[test]
    fn anything_else_is_not_a_date() {
        let bad_dates = [
            "",
            "yesterday",
            "Sun, 06 Nov 1994 08:49:37",
            "Sun, 06 Nov 1994 08:49:37 PST",
            "Sun, 06 Foo 1994 08:49:37 GMT",
            "Sun, 32 Nov 1994 08:49:37 GMT",
            "Sun, 06 Nov 1994 25:49:37 GMT",
            "Sun, 06 Nov 1994 08:49 GMT",
        ];
        for bad in bad_dates {
            assert_eq!(parse_http_date(bad), None, "{bad:?}");
        }
    }
}
