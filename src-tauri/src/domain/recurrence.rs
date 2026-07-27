//! I-21 recurrence math: a minimal RFC 5545 RRULE subset,
//! dependency-free.
//!
//! ```text
//! FREQ=DAILY|WEEKLY|MONTHLY[;INTERVAL=n]     (1 <= n <= 999)
//! ```
//!
//! Times are unix ms UTC. Daily/weekly advance by exact ms offsets.
//! Monthly advances the civil date (Howard Hinnant's public
//! days↔civil algorithms) preserving time-of-day, with short-month
//! day clamping: Jan 31 + 1 month → Feb 28 (Feb 29 in leap years).
//! Note the clamp is per-step from the base date, not cumulative —
//! `next_after` is always applied to the *previous instance's* date,
//! which matches the "next occurrence after completing this one"
//! semantics of I-21 (FUTURE_WORK.md).

const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recurrence {
    pub freq: Freq,
    pub interval: u32,
}

impl Recurrence {
    /// Parse the rule subset. Whitespace-tolerant, case-insensitive,
    /// strict about vocabulary: any unknown key, unknown FREQ value, or
    /// out-of-range INTERVAL rejects the whole rule (None). INTERVAL
    /// defaults to 1.
    pub fn parse(s: &str) -> Option<Self> {
        let mut freq: Option<Freq> = None;
        let mut interval: u32 = 1;
        for part in s.split(';') {
            let (k, v) = part.split_once('=')?;
            match k.trim().to_ascii_uppercase().as_str() {
                "FREQ" => {
                    freq = Some(match v.trim().to_ascii_uppercase().as_str() {
                        "DAILY" => Freq::Daily,
                        "WEEKLY" => Freq::Weekly,
                        "MONTHLY" => Freq::Monthly,
                        _ => return None,
                    })
                }
                "INTERVAL" => {
                    interval = v
                        .trim()
                        .parse()
                        .ok()
                        .filter(|n| (1..=999).contains(n))?;
                }
                _ => return None,
            }
        }
        Some(Recurrence {
            freq: freq?,
            interval,
        })
    }

    /// Canonical serialization (what gets stored): uppercase, INTERVAL
    /// omitted when 1.
    pub fn to_rule(self) -> String {
        let f = match self.freq {
            Freq::Daily => "DAILY",
            Freq::Weekly => "WEEKLY",
            Freq::Monthly => "MONTHLY",
        };
        if self.interval == 1 {
            format!("FREQ={f}")
        } else {
            format!("FREQ={f};INTERVAL={}", self.interval)
        }
    }

    /// The next occurrence strictly after `base_ms`, per the rule.
    pub fn next_after(self, base_ms: i64) -> i64 {
        match self.freq {
            Freq::Daily => base_ms + self.interval as i64 * DAY_MS,
            Freq::Weekly => base_ms + self.interval as i64 * 7 * DAY_MS,
            Freq::Monthly => add_months_ms(base_ms, self.interval as i64),
        }
    }
}

fn add_months_ms(base_ms: i64, months: i64) -> i64 {
    let days = base_ms.div_euclid(DAY_MS);
    let ms_of_day = base_ms.rem_euclid(DAY_MS);
    let (y, m, d) = civil_from_days(days);
    let total = y * 12 + (m as i64 - 1) + months;
    let ny = total.div_euclid(12);
    let nm = (total.rem_euclid(12) + 1) as u32;
    let nd = d.min(last_day_of_month(ny, nm));
    days_from_civil(ny, nm, nd) * DAY_MS + ms_of_day
}

/// Days since 1970-01-01 for a civil date (proleptic Gregorian).
/// Howard Hinnant, "chrono-Compatible Low-Level Date Algorithms".
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = ((m + 9) % 12) as i64; // [0, 11], Mar = 0
    let doy = (153 * mp + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Civil date from days since 1970-01-01. Inverse of `days_from_civil`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn is_leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn last_day_of_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => unreachable!("month out of range"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Known unix anchors (UTC): pin the date algorithms to absolute
    // truth, not to themselves.
    const MS_2024_01_31: i64 = 1_706_659_200_000; // 2024-01-31T00:00:00Z
    const MS_2024_02_29: i64 = 1_709_164_800_000; // 2024-02-29T00:00:00Z
    const MS_2026_01_31: i64 = 1_769_817_600_000; // 2026-01-31T00:00:00Z
    const MS_2026_02_28: i64 = 1_772_236_800_000; // 2026-02-28T00:00:00Z

    #[test]
    fn civil_anchors_match_known_unix_days() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2024, 1, 31) * DAY_MS, MS_2024_01_31);
        assert_eq!(days_from_civil(2024, 2, 29) * DAY_MS, MS_2024_02_29);
        assert_eq!(days_from_civil(2026, 1, 31) * DAY_MS, MS_2026_01_31);
        assert_eq!(civil_from_days(MS_2026_02_28 / DAY_MS), (2026, 2, 28));
    }

    #[test]
    fn civil_round_trips_across_a_wide_range() {
        // Every 13 days across ~80 years, plus the epoch neighborhood.
        for z in (-10_000..20_000).step_by(13) {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z, "round trip for day {z} ({y}-{m}-{d})");
        }
    }

    #[test]
    fn leap_rule_handles_the_century_exceptions() {
        // `is_leap` feeds only `last_day_of_month`, and the round-trip
        // test above uses the era-based algorithms rather than this
        // function — so reducing the rule to `y % 4 == 0` left the whole
        // suite green (v0.3 pass 9). The divergence is real but distant:
        // a monthly recurrence crossing 2100-02 would clamp Jan 31 to a
        // February 29th that does not exist.
        for (y, leap) in [
            (2024, true),
            (2025, false),
            (2000, true),  // divisible by 400 — IS a leap year
            (1900, false), // divisible by 100, not 400 — is NOT
            (2100, false), // the next century exception
            (2400, true),
        ] {
            assert_eq!(is_leap(y), leap, "leap rule for {y}");
            assert_eq!(
                last_day_of_month(y, 2),
                if leap { 29 } else { 28 },
                "February {y}"
            );
        }
    }

    #[test]
    fn parse_round_trips_and_normalizes() {
        for (input, canonical) in [
            ("FREQ=DAILY", "FREQ=DAILY"),
            ("freq=weekly", "FREQ=WEEKLY"),
            ("FREQ=MONTHLY;INTERVAL=3", "FREQ=MONTHLY;INTERVAL=3"),
            ("FREQ=DAILY;INTERVAL=1", "FREQ=DAILY"),
            (" FREQ = MONTHLY ; INTERVAL = 2 ", "FREQ=MONTHLY;INTERVAL=2"),
        ] {
            let r = Recurrence::parse(input).unwrap_or_else(|| panic!("{input:?} must parse"));
            assert_eq!(r.to_rule(), canonical);
            assert_eq!(Recurrence::parse(&r.to_rule()), Some(r), "canonical re-parses");
        }
    }

    #[test]
    fn parse_rejects_bad_rules() {
        for bad in [
            "", "FREQ=HOURLY", "FREQ=YEARLY", "INTERVAL=2", "garbage",
            "FREQ=DAILY;INTERVAL=0", "FREQ=DAILY;INTERVAL=1000",
            "FREQ=DAILY;COUNT=3", "FREQ=DAILY;;", "FREQ",
        ] {
            assert_eq!(Recurrence::parse(bad), None, "{bad:?} must be rejected");
        }
    }

    #[test]
    fn daily_and_weekly_are_exact_offsets() {
        let daily3 = Recurrence { freq: Freq::Daily, interval: 3 };
        assert_eq!(daily3.next_after(1_000), 1_000 + 3 * DAY_MS);
        let weekly = Recurrence { freq: Freq::Weekly, interval: 1 };
        assert_eq!(weekly.next_after(MS_2024_01_31), MS_2024_01_31 + 7 * DAY_MS);
    }

    #[test]
    fn monthly_clamps_short_months() {
        let monthly = Recurrence { freq: Freq::Monthly, interval: 1 };
        // Jan 31 2024 (leap) + 1mo → Feb 29 2024.
        assert_eq!(monthly.next_after(MS_2024_01_31), MS_2024_02_29);
        // Jan 31 2026 (non-leap) + 1mo → Feb 28 2026.
        assert_eq!(monthly.next_after(MS_2026_01_31), MS_2026_02_28);
        // Mar 31 + 1mo → Apr 30.
        let mar31 = days_from_civil(2026, 3, 31) * DAY_MS;
        assert_eq!(monthly.next_after(mar31), days_from_civil(2026, 4, 30) * DAY_MS);
    }

    #[test]
    fn monthly_preserves_time_of_day_and_crosses_year() {
        let monthly = Recurrence { freq: Freq::Monthly, interval: 1 };
        let dec15_1030 = days_from_civil(2025, 12, 15) * DAY_MS + 10 * 3_600_000 + 30 * 60_000;
        let jan15_1030 = days_from_civil(2026, 1, 15) * DAY_MS + 10 * 3_600_000 + 30 * 60_000;
        assert_eq!(monthly.next_after(dec15_1030), jan15_1030);
    }

    #[test]
    fn monthly_interval_spans_multiple_months() {
        let q = Recurrence { freq: Freq::Monthly, interval: 3 };
        let jan31 = MS_2026_01_31;
        // Jan 31 + 3mo → Apr 30 (clamped from 31).
        assert_eq!(q.next_after(jan31), days_from_civil(2026, 4, 30) * DAY_MS);
    }
}
