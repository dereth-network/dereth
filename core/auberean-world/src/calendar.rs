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
