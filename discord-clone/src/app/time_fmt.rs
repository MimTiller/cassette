//! Timestamp formatting: "Today at …" labels and full dates from the runtime clock.
//! Split out of `app.rs`; `use super::*` keeps app.rs's imports and shared
//! colors/helpers in scope.

use super::*;

pub(super) const DAY_MS: i64 = 86_400_000;

/// Civil local time (day number, hour, minute) for a Unix-seconds instant.
/// Pure math on the wall clock — no date library: all a "Today at …" label
/// needs is *which day* (relative to now) and the clock time.
pub(super) fn local_day_clock(unix_secs: i64) -> (i64, u32, u32) {
    let local_ms = unix_secs * 1_000 + local_offset_minutes() as i64 * 60_000;
    // Floor-divide: local_ms can be negative for pre-1970 instants.
    let day = local_ms.div_euclid(DAY_MS);
    let rem = local_ms.rem_euclid(DAY_MS);
    let h = (rem / 3_600_000) as u32;
    let m = ((rem % 3_600_000) / 60_000) as u32;
    (day, h, m)
}

/// Convert a day number (days since 1970-01-01) to a civil (year, month, day)
/// triple using the standard 4-00-04 leap-year rule. Only exercised by the
/// "older than a day" fallback in `format_when` — mock seed messages never
/// reach it, but the branch shouldn't emit garbage if it ever does.
pub(super) fn civil_ymd_from_day(day: i64) -> (i64, u32, u32) {
    // Howard Hinnant's `days_to_ymd` — pure integer math, valid for all
    // positive day numbers (and most negative ones).
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// Discord-style message time line: "Today at 10:14 AM", "Yesterday at …",
/// or "9/12/2026" once it's older than a day.
pub(crate) fn format_when(unix_secs: i64, now_secs: i64) -> String {
    let (day, h, m) = local_day_clock(unix_secs);
    let (now_day, _, _) = local_day_clock(now_secs);
    let (suffix, h12) = {
        let h12 = h % 12;
        let h12 = if h12 == 0 { 12 } else { h12 };
        (if h < 12 { "AM" } else { "PM" }, h12)
    };
    let time = format!("{h12}:{m:02} {suffix}");
    match now_day - day {
        0 => format!("Today at {time}"),
        1 => format!("Yesterday at {time}"),
        // Older than a day: fall back to a month/day/year built from the day
        // number. (Mock data is always ≤ ~1 day old, so this is the safety
        // branch — kept simple on purpose.)
        _ => {
            let (y, m, d) = civil_ymd_from_day(day);
            format!("{m}/{d}/{y}")
        }
    }
}

/// Full absolute date/time for a message's instant — the hover tooltip's
/// "Saturday, September 12, 2026 at 3:42 PM" form (the counterpart to
/// `format_when`'s short relative label). Pure integer math over the same
/// `local_day_clock` / `civil_ymd_from_day` helpers; the weekday is derived
/// from the day number (1970-01-01 was a Thursday).
pub(crate) fn format_full_when(unix_secs: i64) -> String {
    let (day, h, m) = local_day_clock(unix_secs);
    let (y, mo, d) = civil_ymd_from_day(day);
    // 1970-01-01 (day 0) was a Thursday, so `day.rem_euclid(7)` indexes a
    // Thursday-first weekday table.
    let weekday = [
        "Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday",
    ][day.rem_euclid(7) as usize];
    let month = [
        "January", "February", "March", "April", "May", "June", "July", "August",
        "September", "October", "November", "December",
    ][(mo - 1) as usize];
    let h12 = {
        let h12 = h % 12;
        if h12 == 0 {
            12
        } else {
            h12
        }
    };
    let suffix = if h < 12 { "AM" } else { "PM" };
    format!("{weekday}, {month} {d}, {y} at {h12}:{m:02} {suffix}")
}

/// Discord's date-divider label: the calendar DAY as "October 20, 2026" —
/// the month name, day number, and full year, no weekday or clock. Built
/// from the same `local_day_clock` / `civil_ymd_from_day` pair.
pub(crate) fn format_day_label(unix_secs: i64) -> String {
    let (day, _, _) = local_day_clock(unix_secs);
    let (y, mo, d) = civil_ymd_from_day(day);
    let month = [
        "January", "February", "March", "April", "May", "June", "July", "August",
        "September", "October", "November", "December",
    ][(mo - 1) as usize];
    format!("{month} {d}, {y}")
}

#[cfg(test)]
mod day_label_tests {
    // Noon-ish UTC instants so the label is TZ-stable for any sane local
    // offset: these epochs land mid-day in both UTC and US Central.
    #[test]
    fn october_20_2026() {
        assert_eq!(crate::app::time_fmt::format_day_label(1_792_515_600), "October 20, 2026");
    }

    #[test]
    fn month_boundary() {
        // 2026-12-31 07:00 local (noon UTC), so the label is TZ-stable:
        // Dec -> Jan flips the month name and the year.
        assert_eq!(crate::app::time_fmt::format_day_label(1_798_722_000), "December 31, 2026");
    }

    #[test]
    fn leap_year_edge() {
        // 2026-02-28 local (2026 is not a leap year).
        assert_eq!(crate::app::time_fmt::format_day_label(1_772_301_600), "February 28, 2026");
    }
}

// --- small reusable rows -----------------------------------------------------
