//! The date and time of day of a game time, as the calendar names them: the year, the season and
//! the day of the season, the part of the day.

use auberean_assets::region::GameTime;

use crate::sky::day_clock;

/// Where a game time falls in the calendar, by the calendar's own lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i64,
    /// The year's name (a string token).
    pub year_name: u32,
    /// The season (its place in the calendar's list) and its name; the day of the season, from 1.
    pub season: usize,
    pub season_name: u32,
    pub day: i64,
    /// The part of the day (its place in the calendar's list) and its name.
    pub part: usize,
    pub part_name: u32,
}

/// The date of game time `t` (seconds): the year; the season, the last whose first day has come
/// (the first with fewer than two seasons); the day of that season; the part of the day, the last
/// whose beginning has come (the first with fewer than two). `None` for a calendar with no seasons
/// or no parts of the day.
// D2-13060, D2-13061, D2-13062, D2-13068
#[must_use]
pub fn date(cal: &GameTime, t: f64) -> Option<Date> {
    let c = day_clock(cal, t);
    let last = |n: usize, reached: &dyn Fn(usize) -> bool| -> usize {
        let mut u = 0;
        if n > 1 {
            while u < n - 1 && reached(u + 1) {
                u += 1;
            }
        }
        u
    };
    let season = last(cal.seasons.len(), &|i| {
        c.day >= i64::from(cal.seasons[i].begin)
    });
    let part = last(cal.day_parts.len(), &|i| {
        c.fraction >= cal.day_parts[i].begin
    });
    let s = cal.seasons.get(season)?;
    let p = cal.day_parts.get(part)?;
    Some(Date {
        year: c.year,
        year_name: cal.year_name,
        season,
        season_name: s.name,
        day: c.day - i64::from(s.begin) + 1,
        part,
        part_name: p.name,
    })
}

/// The game's running clock over its calendar: the world's time from the game clock, kept up each
/// frame, with where the world is in the calendar as of its last update. Between the moments the
/// part of the day changes, only the time of day moves; the year, the day, the season and the part
/// of the day are worked out when one of those moments comes.
///
/// An admin's two commands act on it: dilating time (the world's time runs at a multiple of the
/// game clock's, the time of day where it was) and moving the time of day straight to a value.
///
/// A calendar's arithmetic is in 64 bits throughout, as the client's is, its floats widened.
#[derive(Clone, Debug, PartialEq)]
pub struct GameClock {
    calendar: GameTime,
    /// A year's length, in seconds.
    year_length: f64,
    /// How far through the day, `[0, 1)`.
    time_of_day: f32,
    /// The world's time the day began at; below zero, not worked out yet.
    day_begin: f64,
    /// The world's time the part of the day next changes at.
    next_change: f64,
    year: i32,
    day: i32,
    season: usize,
    part: usize,
    dilation: f64,
    /// Added to the world's time, in seconds: where the time of day was moved to.
    offset: f64,
    /// The time of day was moved, and the land has not been lit for it yet.
    needs_light: bool,
}

impl Default for GameClock {
    /// With no calendar: one long day (100,000 seconds) in a one-day year, at the normal rate.
    fn default() -> Self {
        Self::new(GameTime {
            day_length: 100_000.0,
            days_per_year: 1,
            ..GameTime::default()
        })
    }
}

/// The last of `n` entries whose beginning `reached` says has come (the first with fewer than two).
fn last_reached(n: usize, reached: impl Fn(usize) -> bool) -> usize {
    let mut u = 0;
    if n > 1 {
        while u < n - 1 && reached(u + 1) {
            u += 1;
        }
    }
    u
}

impl GameClock {
    /// A clock over a calendar, its first update still to come.
    // D2-13051
    #[must_use]
    pub fn new(calendar: GameTime) -> Self {
        Self {
            year_length: f64::from(calendar.day_length) * f64::from(calendar.days_per_year),
            calendar,
            time_of_day: 0.0,
            day_begin: -1.0,
            next_change: 0.0,
            year: 0,
            day: 0,
            season: 0,
            part: 0,
            dilation: 1.0,
            offset: 0.0,
            needs_light: false,
        }
    }

    /// The world's time at game clock time `t`.
    #[must_use]
    pub fn world_time(&self, t: f64) -> f64 {
        t * self.dilation + self.offset
    }

    fn day_length(&self) -> f64 {
        f64::from(self.calendar.day_length)
    }

    /// The year, the seconds into it, and the day of it, at world time `t`.
    fn in_year(&self, t: f64) -> (i32, f64, i32) {
        let d = t + self.calendar.zero_time;
        let whole = (d / self.year_length).floor();
        let rem = d - whole * self.year_length;
        let day = dereth_primitives::num::to_i32_f64((rem / self.day_length()).floor());
        (
            dereth_primitives::num::to_i32_f64(whole) + self.calendar.zero_year,
            rem,
            day,
        )
    }

    /// The season a day of the year falls in.
    fn season_of(&self, day: i32) -> usize {
        let seasons = &self.calendar.seasons;
        last_reached(seasons.len(), |i| day >= seasons[i].begin)
    }

    /// The part of the day a fraction of the day falls in.
    fn part_of(&self, fraction: f64) -> usize {
        let parts = &self.calendar.day_parts;
        last_reached(parts.len(), |i| fraction >= f64::from(parts[i].begin))
    }

    /// Where the day begins, and the year, day and season, at world time `t`.
    // D2-13060
    fn work_out_day(&mut self, t: f64) {
        let (year, rem, day) = self.in_year(t);
        self.year = year;
        self.day = day;
        self.season = self.season_of(day);
        let begin = t - (rem - f64::from(day) * self.day_length());
        self.day_begin = begin;
        self.next_change = begin;
    }

    /// The part of the day at world time `t`, and when the next begins (the day's end for the
    /// last). A calendar with no parts changes one day's length past the zero of time.
    // D2-13061
    fn work_out_part(&mut self, t: f64) {
        let fraction = (t - self.day_begin) / self.day_length();
        let n = self.calendar.day_parts.len();
        if n == 0 {
            self.next_change = self.day_length();
            return;
        }
        let u = self.part_of(fraction);
        self.part = u;
        let next = if u == n - 1 {
            self.day_length()
        } else {
            f64::from(self.calendar.day_parts[u + 1].begin) * self.day_length()
        };
        self.next_change = next + self.day_begin;
    }

    /// Bring the clock up to game clock time `t`: the time of day always; at a change of the part
    /// of the day, the part, and the day when a new one has begun.
    // D2-13067
    #[allow(clippy::cast_possible_truncation)]
    pub fn use_time(&mut self, t: f64) {
        let world = self.world_time(t);
        if self.day_begin < 0.0 {
            self.work_out_day(world);
        }
        let through = (world - self.day_begin) / self.day_length();
        self.time_of_day = through as f32;
        if world < self.next_change {
            return;
        }
        if through >= 1.0 {
            self.work_out_day(world);
            self.time_of_day = ((world - self.day_begin) / self.day_length()) as f32;
        }
        self.work_out_part(world);
    }

    /// Make the world's time run at `rate` times the game clock's, the time of day where it was:
    /// refused at or below 0.0002 of a second a second. The day is worked out afresh at the next
    /// update.
    // D2-13069, D2-13071
    pub fn dilate(&mut self, rate: f32, t: f64) -> bool {
        if rate <= 0.0002 {
            return false;
        }
        let was_time_of_day = f64::from(self.time_of_day);
        let days = self.calendar.days_per_year;
        let was_day = days.wrapping_mul(self.year).wrapping_add(self.day);
        self.dilation = f64::from(rate);
        self.day_begin = -1.0;
        self.use_time(t);
        let passed = days
            .wrapping_mul(self.year)
            .wrapping_sub(was_day)
            .wrapping_add(self.day);
        // The count is read back unsigned, as the client reads it.
        let passed = f64::from(passed.cast_unsigned());
        self.day_begin = -1.0;
        self.offset -= (passed + f64::from(self.time_of_day) - was_time_of_day) * self.day_length();
        true
    }

    /// Move the time of day straight to `to` (a fraction of the day): the world's time moves on or
    /// back, never below zero by more than a day, and the land is lit for it at once.
    // D2-13070
    pub fn alter_time_of_day(&mut self, to: f32, t: f64) {
        self.needs_light = true;
        self.offset += (f64::from(to) - f64::from(self.time_of_day)) * self.day_length();
        if self.offset < 0.0 {
            self.offset += self.day_length();
        }
        self.next_change = 0.0;
        self.use_time(t);
    }

    /// How fast the world's time runs against the game clock's.
    // D2-13054
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn dilation(&self) -> f32 {
        self.dilation as f32
    }

    /// Seconds of the world's time until the day ends.
    // D2-10995
    #[must_use]
    pub fn time_til_tomorrow(&self) -> f64 {
        (1.0 - f64::from(self.time_of_day)) * self.day_length()
    }

    /// Whether the land waits to be lit for a moved time of day.
    // D2-10996
    #[must_use]
    pub const fn needs_light(&self) -> bool {
        self.needs_light
    }

    /// The land has been lit.
    // D2-10997
    pub fn lit(&mut self) {
        self.needs_light = false;
    }

    /// How far through the day, `[0, 1)`.
    #[must_use]
    pub const fn time_of_day(&self) -> f32 {
        self.time_of_day
    }

    #[must_use]
    pub const fn year(&self) -> i32 {
        self.year
    }

    /// The day of the year, from 0.
    #[must_use]
    pub const fn day(&self) -> i32 {
        self.day
    }

    #[must_use]
    pub fn day_length_seconds(&self) -> f32 {
        self.calendar.day_length
    }

    #[must_use]
    pub const fn days_per_year(&self) -> i32 {
        self.calendar.days_per_year
    }

    /// Whether the present part of the day is a night's: its word, as stored.
    #[must_use]
    pub fn night(&self) -> u32 {
        self.calendar
            .day_parts
            .get(self.part)
            .map_or(0, |p| p.night)
    }

    /// The date and time as the client writes them, for season `season` and part of the day `part`
    /// by their places in the calendar's lists: the year and the day are always the present ones.
    fn written(&self, season: usize, part: usize) -> Option<DateTime> {
        let begins = self.calendar.seasons.get(self.season)?.begin;
        Some(DateTime {
            year: self.year,
            year_name: self.calendar.year_name,
            day: self.day - begins + 1,
            season_name: self.calendar.seasons.get(season)?.name,
            part_name: self.calendar.day_parts.get(part)?.name,
        })
    }

    /// The present date and time, as the client writes them.
    #[must_use]
    pub fn current_date_time(&self) -> Option<DateTime> {
        self.written(self.season, self.part)
    }

    /// The date and time at a stamp (world time `t`), as the client writes them, and with the
    /// client's own limits: the year, the day and the season are the present ones, and the part of
    /// the day is worked out from the stamp's seconds into the year less the present day's.
    // D2-13063, D2-13064
    #[must_use]
    pub fn date_time_at(&self, t: f64) -> Option<DateTime> {
        let (_, rem, _) = self.in_year(t);
        let through =
            (t - (t - (rem - f64::from(self.day) * self.day_length()))) / self.day_length();
        self.written(self.season_of(self.day), self.part_of(through))
    }
}

/// What a date and time are written from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: i32,
    /// The year's name (a string token).
    pub year_name: u32,
    /// The day of the season, from 1.
    pub day: i32,
    /// The season's name (a string token).
    pub season_name: u32,
    /// The part of the day's name (a string token).
    pub part_name: u32,
}
