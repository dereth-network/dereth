//! The one thing the retail client gets from the operating system that no other module here asks
//! for: **the local time zone**.
//!
//! # What retail does
//!
//! Six client functions format a date, and every one of them runs its instant through `localtime`
//! before formatting.
//!
//! `localtime` is imported from **`MSVCR70.DLL`**, which the client ships beside itself. That
//! DLL imports **`GetTimeZoneInformation`** from `KERNEL32`, which is what MSVC's `_tzset` calls when the `TZ`
//! environment variable is not set. So the offset is the **operating system's**, with the
//! daylight rule applied for the instant being converted, and **nothing in the client's own
//! configuration takes part** — there is no time-zone key in `UserPreferences.ini` and no
//! handler reads one.
//!
//! # What this does, and why it looks like this
//!
//! `dereth-client` is `#![forbid(unsafe_code)]` and every Win32 entry point in the `windows` crate
//! is an `unsafe fn`, so `GetTimeZoneInformation` is not reachable from here. The **WinRT**
//! bindings in the same already-locked crate are safe fns, and `Windows.Globalization.Calendar`
//! converts a UTC instant to local civil time with the host's real daylight rule — the same
//! answer `localtime` gives, arrived at without an `unsafe` block and without adding a
//! dependency the lockfile does not already carry.
//!
//! The offset is computed **per instant** rather than cached, because it is not a constant: a
//! house bought in January and one bought in July are an hour apart in the same zone, and
//! `localtime` would give each its own.
//!
//! A fixed offset of zero is wrong: every date would render in UTC, which shows up as an
//! hours-long shift on a house-purchase timestamp.

// The clock implementations, the frame pacer and the timer are `dereth_client_runtime`'s; they
// resolve here at the paths they were written at.
pub use dereth_client_runtime::platform::clock::{
    system_unix_time, Clock, FixedStepClock, Pacer, SystemClock, Timer, EXTERNAL_TIME_EPSILON,
};

/// The shift from UTC that the CRT's `localtime` would apply to `unix_secs`, in seconds.
///
/// Returns `0` — i.e. UTC, unchanged — when the host cannot answer. A failure here must not stop
/// a date from drawing: retail's `localtime` failing means a NULL `tm`, which is a *blank* date
/// on two of the six surfaces, and blanking the character sheet's born line because a WinRT
/// activation failed would be strictly worse than an hour's error.
///
/// # The arithmetic
///
/// `Calendar` is set to the instant as a `Foundation::DateTime` (100-nanosecond ticks since
/// 1601-01-01 UTC) and then read back as *local* civil fields. Rebuilding a Unix instant from
/// those fields as though they were UTC and subtracting the original gives exactly what
/// `localtime` added. Seconds are not read: no zone this side of 1972 has a sub-minute offset,
/// and `Calendar` reports the seconds of the instant unchanged in every zone.
#[must_use]
pub fn local_utc_offset_secs(unix_secs: i64) -> i32 {
    #[cfg(windows)]
    {
        windows_offset(unix_secs).unwrap_or(0)
    }
    #[cfg(not(windows))]
    {
        unix_offset(unix_secs).unwrap_or(0)
    }
}

/// The C library's `localtime` on Linux and macOS reads the same zoneinfo `chrono` reads, with
/// the same daylight rule applied to the instant, so the answer is `localtime`'s.
#[cfg(not(windows))]
fn unix_offset(unix_secs: i64) -> Option<i32> {
    use chrono::{Local, Offset as _, TimeZone as _};
    let local = Local.timestamp_opt(unix_secs, 0).single()?;
    Some(local.offset().fix().local_minus_utc())
}

/// Ticks between 1601-01-01 and 1970-01-01, in seconds — the `FILETIME` epoch shift.
#[cfg(windows)]
const FILETIME_EPOCH_SHIFT_SECS: i64 = 11_644_473_600;

#[cfg(windows)]
fn windows_offset(unix_secs: i64) -> Option<i32> {
    use windows::Globalization::Calendar;

    let cal = Calendar::new().ok()?;
    let ticks = unix_secs
        .checked_add(FILETIME_EPOCH_SHIFT_SECS)?
        .checked_mul(10_000_000)?;
    cal.SetDateTime(windows::Foundation::DateTime {
        UniversalTime: ticks,
    })
    .ok()?;
    // `Calendar` reports a 24-hour clock only if asked; `Period()` is 1 for AM and 2 for PM and
    // `Hour()` is 1..=12 under the default clock, so the hour is reassembled rather than trusted.
    let (y, mo, d) = (cal.Year().ok()?, cal.Month().ok()?, cal.Day().ok()?);
    let (h12, period, mi) = (cal.Hour().ok()?, cal.Period().ok()?, cal.Minute().ok()?);
    let h24 = match (h12 % 12, period) {
        (h, 2) => h + 12,
        (h, _) => h,
    };
    let local_as_utc = days_from_civil(i64::from(y), i64::from(mo), i64::from(d)) * 86_400
        + i64::from(h24) * 3600
        + i64::from(mi) * 60
        + unix_secs.rem_euclid(60);
    i32::try_from(local_as_utc - unix_secs).ok()
}

/// `(year, month 1-12, day 1-31)` → days since 1970-01-01. Howard Hinnant's `days_from_civil`,
/// the inverse of the `civil_from_days` `dereth_ui_screens::ctime` formats with.
#[cfg(windows)]
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::local_utc_offset_secs;

    /// The offset is a plausible zone and is stable for an instant.
    #[test]
    fn the_offset_is_a_plausible_zone_and_is_stable_for_an_instant() {
        for t in [0_i64, 1_000_000_000, 1_789_404_178, 2_000_000_000] {
            let o = local_utc_offset_secs(t);
            assert!(
                (-16 * 3600..=16 * 3600).contains(&o),
                "t={t} gave {o}s, not a real zone"
            );
            assert_eq!(o % 60, 0, "t={t} gave {o}s, not a whole number of minutes");
            assert_eq!(o, local_utc_offset_secs(t), "t={t} is not stable");
        }
    }

    /// Retail's daylight rule is the OS's, so the offset is a function of the instant and not a
    /// constant. On a zone that observes DST these two differ; on one that does not they agree.
    /// Either is a pass — what would be a failure is a January instant and a July instant more
    /// than three hours apart, which no real zone is.
    #[test]
    fn midwinter_and_midsummer_are_at_most_one_daylight_step_apart() {
        // 2026-01-15 12:00 UTC and 2026-07-15 12:00 UTC.
        let jan = local_utc_offset_secs(1_768_478_400);
        let jul = local_utc_offset_secs(1_784_116_800);
        assert!((jan - jul).abs() <= 3 * 3600, "jan={jan}s jul={jul}s");
    }
}
