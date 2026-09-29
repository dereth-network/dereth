// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Extensions/TimeSpanExtensions.cs
//! `TimeSpanExtensions`.

use crate::dotnet::{CsCast, TimeSpan};

// ACE: TimeSpanExtensions.GetFriendlyString
/// `"1d 10h 17m 36s"`, omitting zero components. Components are absolute values.
#[must_use]
pub fn get_friendly_string(time_span: TimeSpan) -> String {
    let num_days = time_span.format("%d");
    let num_hours = time_span.format("%h");
    let num_minutes = time_span.format("%m");
    let num_seconds = time_span.format("%s");

    let mut sb = String::new();

    if num_days != "0" {
        sb += &(num_days + "d ");
    }
    if num_hours != "0" {
        sb += &(num_hours + "h ");
    }
    if num_minutes != "0" {
        sb += &(num_minutes + "m ");
    }
    if num_seconds != "0" {
        sb += &(num_seconds + "s ");
    }

    sb.trim().to_owned()
}

// ACE: TimeSpanExtensions.GetFriendlyLongString
/// `"1 day , 10 hours , 17 minutes and 36 seconds"` (ACE's spacing, including the space before
/// each comma).
#[must_use]
pub fn get_friendly_long_string(time_span: TimeSpan) -> String {
    let num_days = time_span.format("%d");
    let num_hours = time_span.format("%h");
    let num_minutes = time_span.format("%m");
    let num_seconds = time_span.format("%s");

    let plural = |n: i32| if n > 1 { "s" } else { "" };
    let mut sb = String::new();

    if num_days != "0" {
        sb += &format!("{num_days} day{} ", plural(time_span.days()));
    }
    if num_hours != "0" {
        let sep = if num_days != "0" { ", " } else { "" };
        sb += &format!("{sep}{num_hours} hour{} ", plural(time_span.hours()));
    }
    if num_minutes != "0" {
        let sep = if num_days != "0" || num_hours != "0" {
            ", "
        } else {
            ""
        };
        sb += &format!("{sep}{num_minutes} minute{} ", plural(time_span.minutes()));
    }
    if num_seconds != "0" {
        let sep = if num_days != "0" || num_hours != "0" || num_minutes != "0" {
            "and "
        } else {
            ""
        };
        sb += &format!("{sep}{num_seconds} second{} ", plural(time_span.seconds()));
    }

    sb.trim().to_owned()
}

// ACE: TimeSpanExtensions.SecondsPerMonth
/// A 30-day month. (A mutable static in ACE that nothing assigns.)
pub const SECONDS_PER_MONTH: u32 = 60 * 60 * 24 * 30;

// ACE: TimeSpanExtensions.SecondsPerYear
/// A non-leap year. (A mutable static in ACE that nothing assigns.)
pub const SECONDS_PER_YEAR: u32 = 60 * 60 * 24 * 365;

// ACE: TimeSpanExtensions.GetMonths
/// `(uint)timeSpan.TotalSeconds / SecondsPerMonth`. A negative span saturates to 0 on net10.0.
#[must_use]
pub fn get_months(time_span: TimeSpan) -> u32 {
    let total: u32 = time_span.total_seconds().cs_cast();
    total / SECONDS_PER_MONTH
}

// ACE: TimeSpanExtensions.GetYears
/// `(uint)timeSpan.TotalSeconds / SecondsPerYear`.
#[must_use]
pub fn get_years(time_span: TimeSpan) -> u32 {
    let total: u32 = time_span.total_seconds().cs_cast();
    total / SECONDS_PER_YEAR
}
