//! The two CRT time formatters the retail client links against, and the zone shift they run in.
//!
//! # Which CRT, and which locale
//!
//! The retail client imports `setlocale`, `localtime`, `asctime`, `strftime` and `wcsftime` from
//! **`MSVCR70.DLL`**, and ships that DLL beside itself
//! (`msvcr70.dll` in the retail install, 344,064 bytes, dated 2013-09-04). Everything below was measured by
//! loading *that* DLL and calling *those* functions, not by reasoning about what a C runtime
//! "would" do.
//!
//! The client calls `setlocale(LC_ALL, "English")` exactly once, during start-up, before
//! anything draws.
//!
//! `setlocale(LC_ALL, "English")` **returns `"English_United States.1252"`** \[measured\]. MSVC
//! resolves a bare language name to that language's `SUBLANG_DEFAULT`, not to the machine's
//! locale — the same call with `"German"` returns `German_Germany.1252` on this en-US box, which
//! is what shows the country comes from the language and not from the host.
//!
//! **So the shipped client does not run in the C locale**, and `%c` is not `asctime` in either
//! locale: MSVCR70's C-locale `%c` is `09/15/26 09:42:58` \[measured\].
//!
//! # `%c` under `English_United States.1252`
//!
//! MSVCR70 imports `GetDateFormatA`, `GetTimeFormatA` and `GetLocaleInfoA` from `KERNEL32`, and
//! `%c` is the locale's **short date**, a space, and the locale's **long time**. For LCID `0x409`
//! those are `LOCALE_SSHORTDATE` = `M/d/yyyy` and `LOCALE_STIMEFORMAT` = `h:mm:ss tt` [measured
//! with `GetLocaleInfoW`, identical with and without `LOCALE_NOUSEROVERRIDE`], which is exactly
//! the shape in a retail screenshot:
//!
//! ```text
//! Bought: 9/14/2026 9:42:58 AM
//! ```
//!
//! Month and day carry **no leading zero**, the year is four digits, the clock is **12-hour with
//! no leading zero**, midnight and noon are `12`, and minutes and seconds are zero-padded to two.
//! `strftime_c` is that, and its unit station pins it against fourteen instants taken straight
//! out of `msvcr70.dll`.
//!
//! # `asctime` is a different shape, and MSVC's is not the textbook one
//!
//! Exactly one call site in the whole client uses `asctime`; the other five use `strftime`. MSVC's
//! `asctime` writes the day of the month as **two digits, zero padded** — `Thu Jan 01 00:00:00
//! 1970` \[measured\] — not the `"%.3s %.3s%3d ..."` width-3 space pad that the C standard's
//! sample implementation and glibc use. Retail draws `Jan 01`, never `Jan  1`; the metalanguage's
//! space-trimming pass is real, but it has no width-3 pad to collapse here, because the shipped
//! client never emits one.
//!
//! # The zone
//!
//! Every one of the six retail call sites formats a `tm` that came from `localtime`, never
//! `gmtime`. MSVCR70's `localtime` goes through `_tzset`, which honours the `TZ` environment
//! variable if it is set and otherwise imports and calls **`GetTimeZoneInformation`** from
//! `KERNEL32` [measured, `dumpbin /IMPORTS msvcr70.dll`]. So the offset comes from the
//! **operating system's time zone**, with its daylight rule applied for the instant being
//! formatted. Nothing in `UserPreferences.ini` or any other client-side configuration takes part.
//!
//! Both formatters here therefore take `utc_offset_secs` as a **parameter rather than reading a
//! clock**, so that a test can pin a zone instead of inheriting the machine's, and so that no
//! call site can forget to say which zone it means. `dereth_desktop::platform` is what supplies the
//! real number; see its note.

/// The shift from UTC that `localtime` would apply, in seconds, for a **particular instant**.
///
/// Named so that the many call sites read the same way, and so that a grep for the concept finds
/// one type rather than a bare `i32`.
pub type UtcOffsetSecs = i32;

/// A local Gregorian calendar value, after applying the supplied UTC offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokenDown {
    /// Full signed year.
    pub year: i64,
    /// Month, 1 through 12.
    pub month: i64,
    /// Day of the month, 1 through 31.
    pub day: i64,
    /// Weekday, 0 for Sunday through 6 for Saturday.
    pub weekday: usize,
    /// Hour on the 24-hour clock.
    pub hour: i64,
    /// Minute within the hour.
    pub minute: i64,
    /// Second within the minute.
    pub second: i64,
}

/// Split a Unix instant into calendar fields using the caller's zone offset.
#[must_use]
pub fn broken_down(t: i64, utc_offset_secs: UtcOffsetSecs) -> BrokenDown {
    let local = t + i64::from(utc_offset_secs);
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    BrokenDown {
        year,
        month,
        day,
        weekday: usize::try_from((days + 4).rem_euclid(7)).unwrap_or(0),
        hour: secs / 3600,
        minute: (secs / 60) % 60,
        second: secs % 60,
    }
}

/// `strftime(buf, n, "%c", tm)` under `setlocale(LC_ALL, "English")` — i.e.
/// `English_United States.1252`'s `M/d/yyyy h:mm:ss tt`.
///
/// This is the formatter for **five** of the client's six date call sites — the character-info
/// panel's birth/age/deaths line, the house maintenance clock and the house purchase-time text
/// among them. All five push the same `"%c"` format string, so all five render identically; the
/// sixth call site is the account-banned notice, which uses [`asctime`] instead.
///
/// `t` is a Unix instant and `utc_offset_secs` is what `localtime` would add to it. Pass `0` only
/// when you mean UTC and are prepared to say so.
///
/// # Locale, declared
///
/// The output is **hard-coded to `English_United States`** rather than read from the host's
/// locale. That is a deliberate, declared divergence and it cuts both ways:
///
/// * It matches retail on any machine, because retail hard-codes the *language* too
///   (`setlocale(LC_ALL, "English")`), and MSVC maps that to en-US regardless of the host. A
///   German Windows running the retail client still draws `9/14/2026 9:42:58 AM`.
/// * It diverges from retail for a user who has overridden the **short date** or **long time**
///   format in Control Panel. MSVCR70 asks `GetLocaleInfo` for LCID `0x409` without
///   `LOCALE_NOUSEROVERRIDE`, so on a machine whose user default LCID is also `0x409` those
///   overrides reach retail's `%c` and would not reach this. The machine the formats were measured
///   on has no such override [measured: `M/d/yyyy` and `h:mm:ss tt` both with and without the flag].
///
/// Reading the override would mean a Win32 call, and this crate is `#![forbid(unsafe_code)]`
/// with no dependency that carries a calendar. The choice is recorded rather than hidden.
#[must_use]
pub fn strftime_c(t: i64, utc_offset_secs: UtcOffsetSecs) -> String {
    let BrokenDown {
        year: y,
        month: m,
        day: d,
        hour: h24,
        minute: mi,
        second: s,
        ..
    } = broken_down(t, utc_offset_secs);
    // `h:mm:ss tt` — a 12-hour clock in which both midnight and noon read 12, and the hour has no
    // leading zero. `12:00:00 AM` and `12:00:00 PM` are both measured values, not a guess.
    let h12 = match h24 % 12 {
        0 => 12,
        h => h,
    };
    let ampm = if h24 < 12 { "AM" } else { "PM" };
    format!("{m}/{d}/{y:04} {h12}:{mi:02}:{s:02} {ampm}")
}

/// MSVCR70's `asctime`, minus the trailing newline the account-banned notice strips itself.
///
/// The account-banned notice formats with `asctime(localtime(&t))` and
/// then walks to the terminator and overwrites the character before it. It is the **only** call
/// site in the client, and the only surface in this workspace that should use this shape rather
/// than `strftime_c`.
///
/// The day of the month is **two digits, zero padded** — `Thu Jan 01 00:00:00 1970`. See the
/// module header for why that is worth a paragraph.
#[must_use]
pub fn asctime(t: i64, utc_offset_secs: UtcOffsetSecs) -> String {
    const WDAY: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MON: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let calendar = broken_down(t, utc_offset_secs);
    // The month is in 1..=12, and the weekday is in 0..=6.
    let mon = usize::try_from(calendar.month - 1).unwrap_or(0);
    format!(
        "{} {} {:02} {:02}:{:02}:{:02} {}",
        WDAY[calendar.weekday],
        MON[mon],
        calendar.day,
        calendar.hour,
        calendar.minute,
        calendar.second,
        calendar.year
    )
}

/// Days since the epoch → `(year, month 1-12, day 1-31)`.
///
/// Howard Hinnant's `civil_from_days`, the standard proleptic-Gregorian inverse. It is here rather
/// than in a dependency because this crate has none that carries a calendar.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::{asctime, strftime_c};

    /// **The oracle, and where every one of these strings came from.**
    ///
    /// the retail install's `msvcr70.dll` was loaded into a 32-bit process, `setlocale(LC_ALL, "English")` was
    /// called exactly as the client calls it, and then for each instant below
    /// `strftime(buf, 1024, "%c", gmtime(&t))` and `asctime(gmtime(&t))` were called and their
    /// bytes copied here verbatim. `gmtime` rather than `localtime` so that the table is a
    /// property of the CRT and not of the machine's time zone — the offset is pinned at `0` on
    /// this side, which is what makes this table reproducible in Reykjavik and in Sydney.
    const GOLDEN: &[(i64, &str, &str)] = &[
        (0, "1/1/1970 12:00:00 AM", "Thu Jan 01 00:00:00 1970"),
        (43_200, "1/1/1970 12:00:00 PM", "Thu Jan 01 12:00:00 1970"),
        (43_199, "1/1/1970 11:59:59 AM", "Thu Jan 01 11:59:59 1970"),
        (46_800, "1/1/1970 1:00:00 PM", "Thu Jan 01 13:00:00 1970"),
        (86_399, "1/1/1970 11:59:59 PM", "Thu Jan 01 23:59:59 1970"),
        (
            951_782_400,
            "2/29/2000 12:00:00 AM",
            "Tue Feb 29 00:00:00 2000",
        ),
        (
            1_709_164_800,
            "2/29/2024 12:00:00 AM",
            "Thu Feb 29 00:00:00 2024",
        ),
        (
            1_789_404_178,
            "9/14/2026 4:42:58 PM",
            "Mon Sep 14 16:42:58 2026",
        ),
        (
            1_789_490_578,
            "9/15/2026 4:42:58 PM",
            "Tue Sep 15 16:42:58 2026",
        ),
        (
            1_788_565_581,
            "9/4/2026 11:46:21 PM",
            "Fri Sep 04 23:46:21 2026",
        ),
        (
            1_000_000_000,
            "9/9/2001 1:46:40 AM",
            "Sun Sep 09 01:46:40 2001",
        ),
        (
            2_000_000_000,
            "5/18/2033 3:33:20 AM",
            "Wed May 18 03:33:20 2033",
        ),
        (
            1_073_741_823,
            "1/10/2004 1:37:03 PM",
            "Sat Jan 10 13:37:03 2004",
        ),
        (
            2_147_483_647,
            "1/19/2038 3:14:07 AM",
            "Tue Jan 19 03:14:07 2038",
        ),
    ];

    #[test]
    fn strftime_c_matches_msvcr70_at_every_golden_instant() {
        for &(t, want, _) in GOLDEN {
            assert_eq!(strftime_c(t, 0), want, "strftime(\"%c\") at t={t}");
        }
    }

    #[test]
    fn asctime_matches_msvcr70_at_every_golden_instant() {
        for &(t, _, want) in GOLDEN {
            assert_eq!(asctime(t, 0), want, "asctime at t={t}");
        }
    }

    /// A fixed timestamp renders in the supplied timezone.
    #[test]
    fn the_owners_screenshot_instant_renders_the_way_retail_rendered_it() {
        const OWNER_INSTANT: i64 = 1_789_404_178;
        const PDT: i32 = -7 * 3600;
        assert_eq!(strftime_c(OWNER_INSTANT, PDT), "9/14/2026 9:42:58 AM");
        // ...and the old rendering, so that the diff between the two is in the test file rather
        // than only in the report.
        assert_eq!(asctime(OWNER_INSTANT, 0), "Mon Sep 14 16:42:58 2026");
    }

    /// The offset is a shift, not a relabelling: it moves the date across midnight in both
    /// directions. Pinned rather than derived, so that a sign error cannot survive.
    #[test]
    fn the_offset_moves_the_instant_across_midnight_both_ways() {
        // 1970-01-01 00:00:00 UTC, one hour west -> the previous year.
        assert_eq!(strftime_c(0, -3600), "12/31/1969 11:00:00 PM");
        // ...and 14 hours east -> the next afternoon.
        assert_eq!(strftime_c(0, 14 * 3600), "1/1/1970 2:00:00 PM");
        assert_eq!(asctime(0, -3600), "Wed Dec 31 23:00:00 1969");
        assert_eq!(asctime(0, 14 * 3600), "Thu Jan 01 14:00:00 1970");
    }

    /// MSVC's `asctime` zero-pads the day; the textbook `"%.3s %.3s%3d"` space-pads it. This
    /// workspace had the textbook one until this unit, so the distinction gets its own station.
    #[test]
    fn asctime_zero_pads_the_day_the_way_msvc_does_and_not_the_way_the_standard_sample_does() {
        assert!(asctime(0, 0).contains("Jan 01"), "{}", asctime(0, 0));
        assert!(
            !asctime(0, 0).contains("Jan  1"),
            "the width-3 space pad is glibc's, not MSVC's"
        );
        // A two-digit day is unaffected, which is why the bug hid: it only shows on days 1-9.
        assert!(asctime(1_756_000_000, 0).contains("Aug 24"));
    }

    /// `%c` pads minutes and seconds but not the month, the day or the hour.
    #[test]
    fn strftime_c_pads_only_the_minute_and_the_second() {
        // 2001-01-02 03:04:05 UTC.
        let t = 978_404_645;
        assert_eq!(strftime_c(t, 0), "1/2/2001 3:04:05 AM");
    }
}
