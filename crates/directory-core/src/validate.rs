//! Shared input validation for user-entered fields.

/// A single-line label: trimmed, non-empty, no control characters, at most `max` chars.
pub fn line(raw: &str, max: usize) -> Result<String, &'static str> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("required");
    }
    if s.chars().count() > max {
        return Err("too long");
    }
    if s.chars().any(char::is_control) {
        return Err("invalid characters");
    }
    Ok(s.to_owned())
}

/// Multi-line text: newlines and tabs allowed, other control characters rejected.
/// Trailing whitespace is trimmed; may be empty.
pub fn text(raw: &str, max: usize) -> Result<String, &'static str> {
    let s = raw.trim_end();
    if s.chars().count() > max {
        return Err("too long");
    }
    if s.chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t' && c != '\r')
    {
        return Err("invalid characters");
    }
    Ok(s.to_owned())
}

/// A calendar date in `YYYY-MM-DD` form (years 1900–9999).
pub fn date(raw: &str) -> Result<String, &'static str> {
    let b = raw.as_bytes();
    let ok_shape = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    if !ok_shape {
        return Err("expected YYYY-MM-DD");
    }
    let num = |r: std::ops::Range<usize>| raw[r].parse::<u32>().unwrap_or(0);
    let (y, m, d) = (num(0..4), num(5..7), num(8..10));
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err("invalid month"),
    };
    if !(1900..=9999).contains(&y) || d == 0 || d > days {
        return Err("invalid date");
    }
    Ok(raw.to_owned())
}

/// Days since 1970-01-01 for a valid `YYYY-MM-DD` (Howard Hinnant's algorithm).
pub fn days_from_civil(date: &str) -> i64 {
    let n = |r: std::ops::Range<usize>| date[r].parse::<i64>().unwrap_or(0);
    let (y, m, d) = (n(0..4), n(5..7), n(8..10));
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
pub fn civil_from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn add_days(date: &str, days: i64) -> String {
    civil_from_days(days_from_civil(date) + days)
}

/// 0 = Monday … 6 = Sunday.
pub fn weekday(date: &str) -> u32 {
    // 1970-01-01 was a Thursday (3).
    (days_from_civil(date) + 3).rem_euclid(7) as u32
}

/// Minutes since midnight for `HH:MM` (00:00–23:59); `24:00` too when
/// `midnight_end` (the end of a block that runs to midnight).
pub fn clock(raw: &str, midnight_end: bool) -> Result<u32, &'static str> {
    let b = raw.as_bytes();
    let ok = b.len() == 5 && b[2] == b':' && b[..2].iter().chain(&b[3..]).all(u8::is_ascii_digit);
    if !ok {
        return Err("expected HH:MM");
    }
    let (h, m) = (
        raw[..2].parse::<u32>().unwrap_or(99),
        raw[3..].parse::<u32>().unwrap_or(99),
    );
    match (h, m) {
        (24, 0) if midnight_end => Ok(1_440),
        (0..=23, 0..=59) => Ok(h * 60 + m),
        _ => Err("expected HH:MM"),
    }
}

/// An IANA time-zone name such as `Europe/London`, `America/Argentina/Salta`,
/// or `UTC` (shape only; the client resolves it).
pub fn timezone(raw: &str) -> Result<String, &'static str> {
    let ok = !raw.is_empty()
        && raw.len() <= 64
        && raw.split('/').all(|part| {
            !part.is_empty()
                && part.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+'))
        });
    if ok {
        Ok(raw.to_owned())
    } else {
        Err("expected an IANA time zone")
    }
}

/// Deserialize helper for PATCH bodies: distinguishes a missing field (`None`)
/// from an explicit `null` (`Some(None)`) so nullable fields can be cleared.
pub fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(d).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines() {
        assert_eq!(line("  Buy milk ", 10), Ok("Buy milk".into()));
        assert_eq!(line("   ", 10), Err("required"));
        assert_eq!(line("12345678901", 10), Err("too long"));
        assert_eq!(line("a\nb", 10), Err("invalid characters"));
        assert_eq!(line("ééé", 3), Ok("ééé".into()), "counts chars, not bytes");
    }

    #[test]
    fn texts() {
        assert_eq!(
            text("line 1\nline 2\t \n", 100),
            Ok("line 1\nline 2".into())
        );
        assert_eq!(text("", 100), Ok(String::new()));
        assert_eq!(text("bell\u{7}", 100), Err("invalid characters"));
    }

    #[test]
    fn dates() {
        assert!(date("2026-10-21").is_ok());
        assert!(date("2024-02-29").is_ok());
        assert!(date("2000-02-29").is_ok());
        for bad in [
            "2025-02-29",
            "1900-02-29",
            "2026-13-01",
            "2026-04-31",
            "2026-1-01",
            "20261021",
            "1899-12-31",
            "2026-10-00",
        ] {
            assert!(date(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn date_arithmetic() {
        assert_eq!(days_from_civil("1970-01-01"), 0);
        assert_eq!(add_days("2026-10-31", 1), "2026-11-01");
        assert_eq!(add_days("2024-02-28", 1), "2024-02-29");
        assert_eq!(add_days("2026-01-01", -1), "2025-12-31");
        assert_eq!(weekday("2026-10-21"), 2, "a Wednesday");
        assert_eq!(weekday("2026-10-25"), 6, "a Sunday");
        for d in ["1900-03-01", "2000-02-29", "2099-12-31"] {
            assert_eq!(civil_from_days(days_from_civil(d)), d);
        }
    }

    #[test]
    fn clocks_and_zones() {
        assert_eq!(clock("00:00", false), Ok(0));
        assert_eq!(clock("09:30", false), Ok(570));
        assert_eq!(clock("23:59", false), Ok(1_439));
        assert_eq!(clock("24:00", true), Ok(1_440));
        for bad in ["24:00", "9:30", "12:60", "ab:cd", "25:00", "12-30"] {
            assert!(clock(bad, false).is_err(), "{bad}");
        }
        assert!(clock("24:01", true).is_err());
        for good in [
            "UTC",
            "Europe/London",
            "America/Argentina/Salta",
            "Etc/GMT+5",
        ] {
            assert!(timezone(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "Europe/",
            "/London",
            "Europe/Lon don",
            "../etc",
            "1Zone",
        ] {
            assert!(timezone(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn double_option_distinguishes_null_from_missing() {
        #[derive(serde::Deserialize)]
        struct P {
            #[serde(default, deserialize_with = "double_option")]
            v: Option<Option<String>>,
        }
        assert_eq!(serde_json::from_str::<P>("{}").unwrap().v, None);
        assert_eq!(
            serde_json::from_str::<P>(r#"{"v":null}"#).unwrap().v,
            Some(None)
        );
        assert_eq!(
            serde_json::from_str::<P>(r#"{"v":"x"}"#).unwrap().v,
            Some(Some("x".into()))
        );
    }
}
