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
