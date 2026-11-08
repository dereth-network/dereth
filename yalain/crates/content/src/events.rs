//! A world's scheduled events: named states of the world, each on for its times, and what follows
//! them (camps and objects that stand only while one runs, quest steps that run only while one
//! runs, effects on every player while one runs).
//!
//! **Times are UTC**, in seconds since 1970 as the host's clock gives them: a dated window runs
//! from its start to its end; a weekly window starts on each of its days at its time of day and
//! runs to its end time, the next day when the end is not after the start (the same time: a whole
//! day). A world's players live in every timezone and its recorded sessions keep UTC, so one zone
//! is named for all of them.
//!
//! A world with no events gates nothing: every camp, object and step is as it would be without
//! this table.

/// The days of the week, Monday first, as an event's weekly window names them.
pub const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// Seconds in a day.
const DAY: i64 = 86_400;

/// One of an event's windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// From one time to another, in seconds since 1970 (UTC), the end not included.
    Dated { from: i64, until: i64 },
    /// Every week: on each day of `days` (bit 0 Monday to bit 6 Sunday) from `from` seconds into
    /// the day (UTC) to `until` seconds into it, or into the next day when `until` is not after
    /// `from`.
    Weekly { days: u8, from: u32, until: u32 },
}

impl Window {
    /// When the window that holds `unix` began and ends, if one does.
    #[must_use]
    pub fn holding(&self, unix: i64) -> Option<(i64, i64)> {
        match *self {
            Self::Dated { from, until } => (from <= unix && unix < until).then_some((from, until)),
            Self::Weekly { days, from, until } => {
                let (from, until) = (i64::from(from), i64::from(until));
                let length = if until > from {
                    until - from
                } else {
                    until + DAY - from
                };
                let today = unix.div_euclid(DAY);
                // A window that began yesterday may still run.
                [today - 1, today].into_iter().find_map(|day| {
                    let start = day * DAY + from;
                    (days & (1 << weekday(day)) != 0 && start <= unix && unix < start + length)
                        .then_some((start, start + length))
                })
            }
        }
    }
}

/// The day of the week of a day since 1970, Monday 0 (1 January 1970 was a Thursday).
#[must_use]
pub fn weekday(day: i64) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let d = (day + 3).rem_euclid(7) as u32;
    d
}

/// One event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub key: String,
    /// The install's game event it stands for, when it names one.
    pub game_event: Option<u32>,
    pub windows: Vec<Window>,
    /// The effects (saved states) every player in the world holds while it runs.
    pub on_players: Vec<u32>,
}

impl Event {
    /// When the window it is on in at `unix` began and ends, if it is on.
    #[must_use]
    pub fn running(&self, unix: i64) -> Option<(i64, i64)> {
        self.windows.iter().find_map(|w| w.holding(unix))
    }
}

/// A placed object an event gates, as it was compiled: found again by its template and place.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectGate {
    pub template: u32,
    pub cell: u32,
    pub origin: [f32; 3],
    pub event: String,
}

/// A world's events and what each gates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventTable {
    pub events: Vec<Event>,
    /// Camps that stand only while an event runs: the camp's id and the event's key.
    pub camps: Vec<(String, String)>,
    pub objects: Vec<ObjectGate>,
    /// Quest steps that run only while an event runs: the step and the event's key.
    pub steps: Vec<(crate::quests::Step, String)>,
}

impl EventTable {
    /// Does the table gate nothing?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// The event of a key.
    #[must_use]
    pub fn event(&self, key: &str) -> Option<&Event> {
        self.events.iter().find(|e| e.key == key)
    }

    /// The event a camp waits on, if any.
    #[must_use]
    pub fn of_camp(&self, id: &str) -> Option<&str> {
        self.camps
            .iter()
            .find(|(c, _)| c == id)
            .map(|(_, e)| e.as_str())
    }

    /// The event a placed object waits on, if any.
    #[must_use]
    pub fn of_object(&self, template: u32, cell: u32, origin: [f32; 3]) -> Option<&str> {
        self.objects
            .iter()
            .find(|g| {
                g.template == template
                    && g.cell == cell
                    && g.origin.map(f32::to_bits) == origin.map(f32::to_bits)
            })
            .map(|g| g.event.as_str())
    }

    /// The events a step waits on (it runs only while every one of them runs).
    pub fn of_step<'a>(
        &'a self,
        step: &'a crate::quests::Step,
    ) -> impl Iterator<Item = &'a str> + 'a {
        self.steps
            .iter()
            .filter(move |(s, _)| s == step)
            .map(|(_, e)| e.as_str())
    }
}

/// A time of day, `HH:MM`, in seconds into the day.
#[must_use]
pub fn time_of_day(s: &str) -> Option<u32> {
    let (h, m) = s.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 3600 + m * 60)
}

/// A UTC time, `YYYY-MM-DDTHH:MMZ` (seconds may follow the minutes), in seconds since 1970.
#[must_use]
pub fn utc(s: &str) -> Option<i64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-');
    let (y, mo, da): (i64, i64, i64) = (
        d.next().filter(|p| p.len() == 4)?.parse().ok()?,
        d.next().filter(|p| p.len() == 2)?.parse().ok()?,
        d.next().filter(|p| p.len() == 2)?.parse().ok()?,
    );
    if d.next().is_some() || !(1..=12).contains(&mo) || da < 1 || da > days_in(y, mo) {
        return None;
    }
    let mut t = time.split(':');
    let (h, mi): (i64, i64) = (
        t.next().filter(|p| p.len() == 2)?.parse().ok()?,
        t.next().filter(|p| p.len() == 2)?.parse().ok()?,
    );
    let sec: i64 = match t.next() {
        None => 0,
        Some(p) if p.len() == 2 => p.parse().ok()?,
        Some(_) => return None,
    };
    if t.next().is_some() || h > 23 || mi > 59 || sec > 59 {
        return None;
    }
    Some(days_from_civil(y, mo, da) * DAY + h * 3600 + mi * 60 + sec)
}

const fn days_in(y: i64, m: i64) -> i64 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970 of a date of the proleptic Gregorian calendar.
const fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
