//! The Dereth calendar clock — the game time the sky, the lighting and the map panel's date read.
//!
//! It is the simulation's clock rather than anything that draws: the world state
//! (`crate::world_state::WorldState`) owns it, and `dereth_scene::sky::GameClock` re-exports it.
//! Nothing in it names a device.
//!
//! The calendar arithmetic is transcribed minimally, in the spirit of [`crate::models`]. The
//! clock offset is 0 in this build, so the whole clock is current time plus the region's time-zero
//! delta.

use dereth_assets::Region;

/// `GameTime` — the Dereth calendar and its current-time calculation.
///
/// `current_week_day` is still not kept: retail zeroes it in the constructor and **no code in the
/// image writes it**, so the date-string formatter receives a 0 it never uses.
///
/// The season and the three name tables are kept because the map panel asks for the season and
/// the date string, and this is the one object that knows the calendar; dropping the strings at
/// unpack would leave the panel no way to ask. `calc_day_begin` computes `current_season` in the
/// same breath as `current_day`; it is not an extra mechanism.
#[derive(Debug, Clone, PartialEq)]
pub struct GameClock {
    /// The region game time's `zero_time_of_year`, 3600.0 s in the shipped region.
    zero_time_of_year: f64,
    /// 10 in the shipped region.
    zero_year: u32,
    /// 7620.0 s — one in-game day is 2 h 07 min of real time.
    day_length: f32,
    /// 360.
    days_per_year: u32,
    /// Computed at unpack: `day_length * days_per_year`.
    year_length: f64,
    /// `times_of_day[i].begin`, the 16 named times. Kept for `calc_time_of_day`'s scan.
    times_of_day: Vec<f32>,
    /// `times_of_day[i].time_of_day_name` — what the date-string formatter prints as the time.
    time_of_day_names: Vec<String>,
    /// `seasons[i].begin`, for `calc_day_begin`'s season scan.
    season_begins: Vec<u32>,
    /// `seasons[i].season_name` — the first `%s` of the date.
    season_names: Vec<String>,
    /// `year_spec` — the last `%s` of the date ("P.Y." in the shipped region).
    year_spec: String,
    /// `current_season`, the index `calc_day_begin` leaves behind.
    pub current_season: usize,
    /// `present_time_of_day`, a float in `[0, 1)`: 0 = midnight, 0.5 = noon.
    pub present_time_of_day: f32,
    /// `time_of_day_begin`, **initialised to -1.0** as the "not yet computed" sentinel.
    time_of_day_begin: f64,
    /// `time_of_next_event`.
    time_of_next_event: f64,
    /// `present_time_in_day_unit`. See [`Self::use_time`] for the shipped bug in it.
    pub present_time_in_day_unit: f32,
    pub current_year: u32,
    pub current_day: u32,
    pub current_time_of_day: usize,
    /// `GameTime.TimeZeroDelta`, the global-registry variable used for local time adjustment:
    /// "Number of seconds to adjust
    /// Timer time to compute GameTime time. GameTime effects the state of the sky". **It is the
    /// only way to change the time of day in the client, and it is purely local.**
    pub time_zero_start_delta: f64,
}

impl GameClock {
    /// Initialize the game clock from the region's decoded time parameters.
    #[must_use]
    pub fn new(region: &Region) -> Self {
        let g = &region.game_time;
        Self {
            zero_time_of_year: g.zero_time_of_year,
            zero_year: g.zero_year,
            day_length: g.day_length,
            days_per_year: g.days_per_year,
            year_length: f64::from(g.day_length) * f64::from(g.days_per_year),
            times_of_day: g.times_of_day.iter().map(|t| t.begin).collect(),
            time_of_day_names: g.times_of_day.iter().map(|t| t.name.clone()).collect(),
            season_begins: g.seasons.iter().map(|s| s.begin).collect(),
            season_names: g.seasons.iter().map(|s| s.name.clone()).collect(),
            year_spec: g.year_spec.clone(),
            current_season: 0,
            present_time_of_day: 0.0,
            time_of_day_begin: -1.0,
            time_of_next_event: 0.0,
            present_time_in_day_unit: 0.0,
            current_year: g.zero_year,
            current_day: 0,
            current_time_of_day: 0,
            time_zero_start_delta: 0.0,
        }
    }

    /// Put the clock at a chosen fraction of the day, by choosing the `TimeZeroDelta` that does it.
    ///
    /// This is not a new mechanism: it is the arithmetic behind `//timeadjust`, i.e. what the
    /// client's own `GameTime.TimeZeroDelta` console variable is for. `cur_time` is the timer
    /// reading the delta will be applied on top of.
    pub fn set_time_of_day(&mut self, cur_time: f64, fraction: f32) {
        // Solve `present_time_of_day(cur_time + delta) == fraction` for a delta in [0, year_length),
        // by asking where in *absolute* game time that fraction of that day falls. Working from the
        // zero point rather than from the current reading keeps it independent of when it is called.
        let want = f64::from(fraction.rem_euclid(1.0)) * f64::from(self.day_length);
        let t = cur_time + self.time_zero_start_delta;
        let y = t + self.zero_time_of_year;
        let day_index = (y / f64::from(self.day_length)).floor();
        let target = day_index.mul_add(f64::from(self.day_length), want) - self.zero_time_of_year;
        let mut delta = target - cur_time;
        // Keep absolute game time non-negative. `calc_day_begin`'s year and day floors assume it,
        // and the time adjustment zeroes `time_of_next_event` so a negative `t` would never reach
        // `calc_time_of_day` at all. Adding whole days does not move the fraction.
        while cur_time + delta < 0.0 {
            delta += f64::from(self.day_length);
        }
        self.time_zero_start_delta = delta;
        // The time adjustment zeroes `time_of_next_event` and re-runs `UseTime` so everything is
        // recomputed rather than waiting for the next boundary.
        self.time_of_next_event = 0.0;
        self.time_of_day_begin = -1.0;
        self.use_time(cur_time);
    }

    /// Put the clock at an **absolute** Dereth second — the whole calendar, not just the hour.
    ///
    /// **This is the other half of what [`Self::set_time_of_day`] does.** A day
    /// fraction alone does not decide the world's colour: day-group selection
    /// hashes `(current_year * days_per_year + current_day)` and the twenty day groups
    /// carry *different* `SkyTimeOfDay` ramps at the same hour. At the retail capture's Foredawn
    /// (fraction 0.1814) group 5 ("Sunny") has `amb_color` (177, 177, 255) — green equal to red —
    /// while group 15 ("Rainy"), which is the group retail's own date hashed to, has
    /// (191, 134, 255). `set_time_of_day(0.0, 0.1814)` lands on year 10 **day 1**, i.e. group 5,
    /// so setting retail's hour alone gives a headless frame with no purple in it.
    ///
    /// The mechanism is the same one, and it is retail's only one: the time-adjustment handler
    /// writes `GameTime.TimeZeroDelta`, zeroes `time_of_next_event`, and updates the clock again.
    /// `absolute` is what a `TimeSync` would have put in —
    /// ACE's `Timers.PortalYearTicks`, i.e. `DerethDateTime.UtcNowToEMUTime`.
    pub fn set_game_time(&mut self, cur_time: f64, absolute: f64) {
        self.time_zero_start_delta = absolute - cur_time;
        self.time_of_next_event = 0.0;
        self.time_of_day_begin = -1.0;
        self.use_time(cur_time);
    }

    /// Advance the game clock from the current client time; the frame loop calls this every frame.
    ///
    /// `t` is the client time plus `time_zero_start_delta` (the clock offset is 0). The day begin
    /// is computed first if it is unset (negative); the present time of day is
    /// `(t - time_of_day_begin) / day_length` as an `f32`. Once `t` reaches the next event time,
    /// the day begin is recomputed if the time of day has reached 1.0, and then the time-of-day
    /// state is recomputed.
    pub fn use_time(&mut self, cur_time: f64) {
        let t = cur_time + self.time_zero_start_delta;
        if self.time_of_day_begin < 0.0 {
            self.calc_day_begin(t);
        }
        self.present_time_of_day = self.fraction_of_day(t);
        if t >= self.time_of_next_event {
            if self.present_time_of_day >= 1.0 {
                self.calc_day_begin(t);
                self.present_time_of_day = self.fraction_of_day(t);
            }
            self.calc_time_of_day(t);
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: the client narrows to `float` here too -- `present_time_of_day` is an f32 field and
    // the division is done in wider precision before the narrowing store. Not a float-to-int
    // conversion.
    fn fraction_of_day(&self, t: f64) -> f32 {
        ((t - self.time_of_day_begin) / f64::from(self.day_length)) as f32
    }

    /// Recompute the current year and day and the start of the active day.
    ///
    /// The `float` narrowings on `rem` and on the last line are deliberate: the day boundary is
    /// computed in single precision, and doing it in double moves day transitions by milliseconds.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: the two `as f32` are the client's own narrowings, transcribed; the integer
    // conversions go through `dereth_primitives::num`.
    fn calc_day_begin(&mut self, t: f64) {
        let y = t + self.zero_time_of_year;
        let years = (y / self.year_length).floor();
        let year = dereth_primitives::num::to_i32_f64(years);
        self.current_year = u32::try_from(year)
            .unwrap_or(0)
            .wrapping_add(self.zero_year);
        let rem = (y - years * self.year_length) as f32;
        let day = dereth_primitives::num::floor_to_i32(rem / self.day_length);
        self.current_day = u32::try_from(day).unwrap_or(0);

        // The current season is the last index `i` with
        // `seasons[i].begin <= current_day`, clamped to `len - 1` — the same shape as the
        // time-of-day scan. The scan compares against `seasons[i + 1]`, which is why
        // `seasons[0].begin` is never read.
        let n = self.season_begins.len();
        let mut i = 0usize;
        while i + 1 < n && self.season_begins[i + 1] <= self.current_day {
            i += 1;
        }
        self.current_season = i;

        self.time_of_day_begin = f64::from((t as f32) - (rem - (day as f32) * self.day_length));
    }

    /// Formats the map panel's date and time through the no-argument overload, forwarding the five
    /// `current_*` fields to the shared formatter.
    ///
    /// `None` when the region supplied no seasons or no times of day, which is the client's
    /// no-current-game-time arm as far as the map is concerned: the panel keeps its prefixes
    /// and shows a space.
    #[must_use]
    pub fn date_time_strings(&self) -> Option<(String, String)> {
        let season = self.season_names.get(self.current_season)?;
        let time_of_day = self.time_of_day_names.get(self.current_time_of_day)?;
        Some(dereth_presentation::map::date_time_strings(
            season,
            self.current_day,
            self.current_year,
            &self.year_spec,
            time_of_day,
        ))
    }

    /// Computes the fraction of the current named time of day, including the shipped bug in the
    /// last branch.
    ///
    /// **Bug preserved.** In the last-entry branch `present_time_in_day_unit` divides by
    /// `day_length - begin` (7619.06) instead of `1 - begin` (0.0625), so the value is ~0 rather
    /// than ramping during the final named time of day. This matches retail; nothing this crate
    /// draws reads it, and making it correct would be wrong.
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: the client's own narrowing of the fraction and of `time_of_day_begin`.
    fn calc_time_of_day(&mut self, t: f64) {
        let n = self.times_of_day.len();
        if n == 0 {
            self.time_of_next_event = f64::from(self.day_length) + self.time_of_day_begin;
            return;
        }
        let f = ((t - self.time_of_day_begin) / f64::from(self.day_length)) as f32;
        // "last index i such that times_of_day[i].begin <= f", a linear scan from 0 that stops at
        // the first entry whose begin is greater, clamped to count - 1.
        let mut i = 0usize;
        while i + 1 < n && self.times_of_day[i + 1] <= f {
            i += 1;
        }
        self.current_time_of_day = i;
        let begin = self.times_of_day[i];
        let base = self.time_of_day_begin as f32;
        if i != n - 1 {
            let next = self.times_of_day[i + 1];
            self.time_of_next_event = f64::from(next.mul_add(self.day_length, base));
            self.present_time_in_day_unit = (f - begin) / (next - begin);
        } else {
            self.time_of_next_event = f64::from(self.day_length + base);
            // The bug: `day_length - begin`, not `1 - begin`.
            self.present_time_in_day_unit = (f - begin) / (self.day_length - begin);
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn retail_clock() -> GameClock {
        let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail region is these tests' oracle and the dats are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        });
        let region =
            dereth_world_data::landblock::load_region(&store).expect("the retail region decodes");
        GameClock::new(&region)
    }

    /// Oracle: `UseTime` over the retail region's own constants. A day is 7620 s, so the fraction
    /// has to advance by exactly 1/7620
    /// per real second and wrap at the day boundary.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_day_fraction_advances_with_the_timer_and_wraps() {
        let mut c = retail_clock();
        assert_eq!(c.day_length, 7620.0);
        assert_eq!(c.days_per_year, 360);
        assert_eq!(c.times_of_day.len(), 16);

        c.use_time(0.0);
        let a = c.present_time_of_day;
        c.use_time(762.0);
        let b = c.present_time_of_day;
        assert!(
            (b - a - 0.1).abs() < 1e-4,
            "762 s is a tenth of a day: {a} -> {b}"
        );

        // Across the boundary the fraction comes back into [0, 1) and the day advances.
        let day = c.current_day;
        c.use_time(8000.0);
        assert!(
            (0.0..1.0).contains(&c.present_time_of_day),
            "{} is outside [0, 1)",
            c.present_time_of_day
        );
        assert_eq!(c.current_day, day + 1, "the day rolled over");
    }

    /// Oracle: the time-of-day setter writes `time_zero_start_delta`, zeroes
    /// `time_of_next_event`, and re-runs `UseTime`; this is the only clock-adjustment path.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn setting_the_time_of_day_lands_on_the_time_asked_for() {
        let mut c = retail_clock();
        for want in [0.0f32, 0.25, 0.5, 0.75, 0.99] {
            c.set_time_of_day(12.5, want);
            assert!(
                (c.present_time_of_day - want).abs() < 1e-3,
                "asked for {want}, got {}",
                c.present_time_of_day
            );
            // And it keeps running from there.
            c.use_time(12.5 + 762.0);
            let expect = (want + 0.1) % 1.0;
            assert!(
                (c.present_time_of_day - expect).abs() < 1e-3,
                "{} is not {expect} a tenth of a day later",
                c.present_time_of_day
            );
        }
    }

    /// Oracle: the retail region's 16 named times, each 1/16 of a day apart, and
    /// `calc_time_of_day`'s "last index i such that begin <= f" scan.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_named_time_of_day_is_the_documented_scan() {
        let mut c = retail_clock();
        // 0 = Darktide (midnight), 8 = Midsong (noon), 15 = Gloaming-and-Half.
        for (fraction, index) in [(0.0f32, 0usize), (0.3, 4), (0.5, 8), (0.99, 15)] {
            c.set_time_of_day(0.0, fraction);
            assert_eq!(c.current_time_of_day, index, "at {fraction}");
        }
    }
}
