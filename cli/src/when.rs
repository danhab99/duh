//! Times for people.

/// "2026-11-25 14:32 UTC" for a Unix timestamp.
pub fn utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{:04}-{:02}-{:02} {:02}:{:02} UTC", year, month, day, rem / 3_600, rem % 3_600 / 60)
}

#[cfg(test)]
mod tests {
    #[test]
    fn known_dates() {
        assert_eq!(super::utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(super::utc(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(super::utc(1_795_635_225), "2026-11-25 19:33 UTC");
    }
}

/// "Dan <dan@example.com>", or just "Dan" when there's no email.
pub fn person(name: &str, email: &str) -> String {
    if email.is_empty() { name.to_string() } else { format!("{} <{}>", name, email) }
}
