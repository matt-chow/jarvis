//! Wall-clock time in an IANA zone (D-049). Core has no time-zone database;
//! the adapter answers "what is the UTC offset of `zone` at this instant"
//! (the Worker asks `Intl`), and core turns local times into instants.

use crate::validate;

pub const MINUTE_MS: i64 = 60_000;
pub const DAY_MS: i64 = 86_400_000;

pub trait ZoneClock {
    /// Minutes east of UTC in `zone` at `utc_ms`, or `None` for an unknown zone.
    fn offset_minutes(&self, zone: &str, utc_ms: i64) -> Option<i32>;
}

/// The local date (`YYYY-MM-DD`) and minute of day at `utc_ms` in `zone`.
pub fn local_at(clock: &impl ZoneClock, zone: &str, utc_ms: i64) -> Option<(String, u32)> {
    let local = utc_ms + i64::from(clock.offset_minutes(zone, utc_ms)?) * MINUTE_MS;
    let day = local.div_euclid(DAY_MS);
    let minute = (local.rem_euclid(DAY_MS) / MINUTE_MS) as u32;
    Some((validate::civil_from_days(day), minute))
}

/// The instant of a local date and minute in `zone`. A time that occurs
/// twice (clocks go back) is the first; one that doesn't exist (clocks go
/// forward) moves forward by the gap, so 02:30 on a spring-forward night is
/// 03:30.
pub fn local_to_utc(clock: &impl ZoneClock, zone: &str, date: &str, minute: u32) -> Option<i64> {
    let naive = validate::days_from_civil(date) * DAY_MS + i64::from(minute) * MINUTE_MS;
    // Transitions are hours apart, so the offsets a day either side cover both readings.
    let before = clock.offset_minutes(zone, naive - DAY_MS)?;
    let after = clock.offset_minutes(zone, naive + DAY_MS)?;
    let candidates = [before, after].map(|o| naive - i64::from(o) * MINUTE_MS);
    let valid = [before, after]
        .iter()
        .zip(candidates)
        .filter(|(o, u)| clock.offset_minutes(zone, *u) == Some(**o))
        .map(|(_, u)| u)
        .min();
    Some(valid.unwrap_or_else(|| candidates[0].max(candidates[1])))
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    /// UTC; `Fixed/+330` style fixed offsets; and `Test/Toronto`, which is
    /// UTC−5, or UTC−4 between 2026-03-08 07:00Z and 2026-11-01 06:00Z (and
    /// the same rule in 2027: 03-14 07:00Z to 11-07 06:00Z).
    pub struct FakeZones;

    impl ZoneClock for FakeZones {
        fn offset_minutes(&self, zone: &str, utc_ms: i64) -> Option<i32> {
            match zone {
                "UTC" => Some(0),
                "Test/Toronto" => {
                    let at = |date: &str, h: i64| {
                        validate::days_from_civil(date) * DAY_MS + h * 3_600_000
                    };
                    let dst = (at("2026-03-08", 7)..at("2026-11-01", 6)).contains(&utc_ms)
                        || (at("2027-03-14", 7)..at("2027-11-07", 6)).contains(&utc_ms);
                    Some(if dst { -240 } else { -300 })
                }
                z => z.strip_prefix("Fixed/").and_then(|o| o.parse().ok()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::FakeZones;
    use super::*;

    fn utc(date: &str, minute: u32) -> i64 {
        validate::days_from_civil(date) * DAY_MS + i64::from(minute) * MINUTE_MS
    }

    #[test]
    fn converts_both_ways_with_dst() {
        let z = FakeZones;
        // Winter: 08:00 in Toronto is 13:00Z; summer: 12:00Z.
        assert_eq!(
            local_to_utc(&z, "Test/Toronto", "2026-01-15", 480),
            Some(utc("2026-01-15", 780))
        );
        assert_eq!(
            local_to_utc(&z, "Test/Toronto", "2026-07-15", 480),
            Some(utc("2026-07-15", 720))
        );
        assert_eq!(
            local_at(&z, "Test/Toronto", utc("2026-07-15", 720)),
            Some(("2026-07-15".into(), 480))
        );
        // Late evening crosses the UTC date line.
        assert_eq!(
            local_at(&z, "Test/Toronto", utc("2026-07-16", 60)),
            Some(("2026-07-15".into(), 21 * 60))
        );
        assert_eq!(
            local_to_utc(&z, "Fixed/330", "2026-01-01", 0),
            Some(utc("2025-12-31", 18 * 60 + 30))
        );
        assert_eq!(local_to_utc(&z, "Nowhere/Zone", "2026-01-01", 0), None);
    }

    #[test]
    fn spring_gap_moves_forward_and_fall_overlap_takes_the_first() {
        let z = FakeZones;
        // 2026-03-08 02:30 doesn't exist in Toronto: it becomes 03:30 EDT (07:30Z).
        assert_eq!(
            local_to_utc(&z, "Test/Toronto", "2026-03-08", 150),
            Some(utc("2026-03-08", 450))
        );
        // 2026-11-01 01:30 happens twice: the first is EDT (05:30Z).
        assert_eq!(
            local_to_utc(&z, "Test/Toronto", "2026-11-01", 90),
            Some(utc("2026-11-01", 330))
        );
        // The same wall time a day apart across the change is 23 or 25 hours.
        let a = local_to_utc(&z, "Test/Toronto", "2026-10-31", 540).unwrap();
        let b = local_to_utc(&z, "Test/Toronto", "2026-11-01", 540).unwrap();
        assert_eq!(b - a, 25 * 3_600_000);
    }
}
