//! Not ACE: each era's calendar, the start of its game time (`Timers.PortalYearTicks`).
//!
//! The client shows the date from the game time the server sends, through the region record's
//! game-time table. Every era keeps AC1's table: day 0 of year 10 P.Y. begins 3,600 s before game
//! time 0, a day is 7,620 s, a year 360 days, and its seasons begin every 30 days from Morningthaw
//! (Leafcull at day 210).
//!
//! An era starts its game time one of two ways ([`Calendar`]): ACE's, the seconds since the retail
//! servers closed, or at a day of its own chosen for the world's launch, the moment `[era]
//! launched_at` gives, game time running on with real time from there.

use crate::dotnet::datetime::DotNetDateTime;

/// The seconds of a day.
pub const DAY_SECONDS: f64 = 7620.0;

/// The days of a year.
pub const DAYS_IN_A_YEAR: i32 = 360;

/// The year of game time 0.
pub const ZERO_YEAR: i32 = 10;

/// The seconds into its year that game time 0 is.
pub const ZERO_TIME_OF_YEAR: f64 = 3600.0;

/// The day of the year Leafcull begins, counted from 0.
pub const LEAFCULL: i32 = 210;

/// How an era's game time starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Calendar {
    /// ACE's: the seconds since the retail servers closed (`DerethDateTime.UtcNowToEMUTime`).
    SinceRetailClosed,
    /// At the world's launch (`[era] launched_at`), game time is `at_launch`; it runs on with real
    /// time from there.
    FromLaunch { at_launch: f64 },
}

/// The game time at which day `day` (from 0) of year `year` begins.
#[must_use]
pub const fn day_begins(year: i32, day: i32) -> f64 {
    (year - ZERO_YEAR) as f64 * DAYS_IN_A_YEAR as f64 * DAY_SECONDS + day as f64 * DAY_SECONDS
        - ZERO_TIME_OF_YEAR
}

/// The year, the day of the year (from 0) and the seconds into the day that game time `t` is, as
/// the client reads it.
#[must_use]
pub fn date_of(t: f64) -> (i32, i32, f64) {
    let year_seconds = f64::from(DAYS_IN_A_YEAR) * DAY_SECONDS;
    let y = t + ZERO_TIME_OF_YEAR;
    let years = (y / year_seconds).floor();
    let into_year = y - years * year_seconds;
    let day = (into_year / DAY_SECONDS).floor();
    #[allow(clippy::cast_possible_truncation)]
    (
        years as i32 + ZERO_YEAR,
        day as i32,
        into_year - day * DAY_SECONDS,
    )
}

impl Calendar {
    /// Whether the era's game time starts at the world's launch, which its configuration must
    /// then give.
    #[must_use]
    pub const fn needs_launch(self) -> bool {
        matches!(self, Self::FromLaunch { .. })
    }

    /// The game time at `now` of a world started at its launch `launched_at`; `None` for ACE's
    /// calendar, which reads its own clock.
    #[must_use]
    pub fn from_launch(self, now: DotNetDateTime, launched_at: DotNetDateTime) -> Option<f64> {
        match self {
            Self::SinceRetailClosed => None,
            Self::FromLaunch { at_launch } => Some(at_launch + (now - launched_at).total_seconds()),
        }
    }
}

/// A moment in UTC as a configuration writes it: `YYYY-MM-DDTHH:MM:SS`, with `Z` after it or not,
/// `T` or a space between the day and the time, or a day alone for its midnight.
///
/// # Errors
/// Text that is not such a moment, as a sentence.
pub fn parse_utc(text: &str) -> Result<DotNetDateTime, String> {
    let bad = || format!("{text:?} is not a moment in UTC, as 2026-12-01T00:00:00Z");
    let t = text.trim();
    let t = t.strip_suffix('Z').unwrap_or(t);
    let (day, time) = match t.split_once(['T', ' ']) {
        Some((d, h)) => (d, Some(h)),
        None => (t, None),
    };
    let numbers = |s: &str, sep: char, n: usize| -> Option<Vec<i32>> {
        let parts: Vec<&str> = s.split(sep).collect();
        (parts.len() == n
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
        .then(|| parts.iter().filter_map(|p| p.parse().ok()).collect())
        .filter(|v: &Vec<i32>| v.len() == n)
    };
    let d = numbers(day, '-', 3).ok_or_else(bad)?;
    let h = match time {
        Some(time) => numbers(time, ':', 3).ok_or_else(bad)?,
        None => vec![0, 0, 0],
    };
    let days_in = |y: i32, m: i32| match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    let ok = (1..=9999).contains(&d[0])
        && (1..=12).contains(&d[1])
        && (1..=days_in(d[0], d[1])).contains(&d[2])
        && (0..24).contains(&h[0])
        && (0..60).contains(&h[1])
        && (0..60).contains(&h[2]);
    if !ok {
        return Err(bad());
    }
    Ok(DotNetDateTime::new_hms(d[0], d[1], d[2], h[0], h[1], h[2]))
}
