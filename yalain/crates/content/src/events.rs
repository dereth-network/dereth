//! A world's scheduled events: named states of the world, each on for its times, and what follows
//! them (camps and objects that stand only while one runs, quest steps that run only while one
//! runs, lines said only while one runs or only while it does not, effects on every player while
//! one runs).
//!
//! **Times are UTC**, in seconds since 1970 as the host's clock gives them: a dated window runs
//! from its start to its end; a weekly window starts on each of its days at its time of day and
//! runs to its end time, the next day when the end is not after the start (the same time: a whole
//! day). A world's players live in every timezone and its recorded sessions keep UTC, so one zone
//! is named for all of them.
//!
//! **Game times** follow the world's own clock instead: a window of the game's hours runs each
//! game day from one hour to another (the game's day split into 24 hours, into the next day when
//! the end is not after the start), and a window of the game's day or night runs while the part of
//! the game's day is not, or is, night by the world's calendar.
//!
//! **Started by a quest's failure.** An event may also come on as a named quest fails for any
//! player, for everyone, and stay on until a time has passed or a camp's creatures are all dead;
//! while it is on it may keep placed objects from the world (a person who stands aside while it
//! runs).
//!
//! A world with no events gates nothing: every camp, object, step and line is as it would be
//! without this table.

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
    /// Every game day, by the game's clock: from `from` to `until`, each in seconds of a 24-hour
    /// clock of the game's day, into the next game day when `until` is not after `from`.
    GameHours { from: u32, until: u32 },
    /// While the part of the game's day is night (`night`), or is not.
    GameDay { night: bool },
}

/// Where the game's day stands now, by the world's calendar: how far through the day (a share in
/// `[0, 1)`), the day's length in seconds, and its parts (where each begins, as a share of the
/// day, and whether it is night), in order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameNow<'a> {
    pub through: f64,
    pub day_length: f64,
    pub parts: &'a [(f32, bool)],
}

impl GameNow<'_> {
    /// When the window of the game's day from share `from` to share `until` (into the next day when
    /// `until` is not after `from`) that holds now began and ends, in host seconds from `unix`, if
    /// one does.
    fn span(&self, unix: i64, from: f64, until: f64) -> Option<(i64, i64)> {
        let length = (until - from).rem_euclid(1.0);
        let length = if length == 0.0 { 1.0 } else { length };
        let since = (self.through - from).rem_euclid(1.0);
        if since >= length {
            return None;
        }
        #[allow(clippy::cast_possible_truncation)]
        let began = unix - (since * self.day_length).round() as i64;
        #[allow(clippy::cast_possible_truncation)]
        let ends = began + (length * self.day_length).round() as i64;
        Some((began, ends))
    }

    /// The part of the day now: its place in the list (the first until the second begins).
    fn part(&self) -> Option<usize> {
        if self.parts.is_empty() {
            return None;
        }
        let mut u = 0;
        while u + 1 < self.parts.len() && self.through >= f64::from(self.parts[u + 1].0) {
            u += 1;
        }
        Some(u)
    }

    /// Where the part `i` begins, as a share of the day: the first part at the day's start.
    fn begins(&self, i: usize) -> f64 {
        if i == 0 {
            0.0
        } else {
            f64::from(self.parts[i].0)
        }
    }

    /// The run of parts about now that are night (`night`), or are not: when it began and ends.
    fn run(&self, unix: i64, night: bool) -> Option<(i64, i64)> {
        let now = self.part()?;
        let n = self.parts.len();
        if self.parts[now].1 != night {
            return None;
        }
        if self.parts.iter().all(|p| p.1 == night) {
            return self.span(unix, 0.0, 0.0);
        }
        // Back to the first of the run, and on to the first part past it.
        let mut first = now;
        while self.parts[(first + n - 1) % n].1 == night {
            first = (first + n - 1) % n;
        }
        let mut past = (now + 1) % n;
        while self.parts[past].1 == night {
            past = (past + 1) % n;
        }
        self.span(unix, self.begins(first), self.begins(past))
    }
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
            Self::GameHours { .. } | Self::GameDay { .. } => None,
        }
    }

    /// When the window of the game's clock that holds now began and ends, in host seconds from
    /// `unix`, if one does; none for a window of the host's clock.
    #[must_use]
    pub fn holding_game(&self, unix: i64, now: &GameNow<'_>) -> Option<(i64, i64)> {
        match *self {
            Self::GameHours { from, until } => {
                let day = 86_400.0;
                now.span(unix, f64::from(from) / day, f64::from(until) / day)
            }
            Self::GameDay { night } => now.run(unix, night),
            Self::Dated { .. } | Self::Weekly { .. } => None,
        }
    }

    /// Does the window follow the game's clock?
    #[must_use]
    pub const fn by_the_game(&self) -> bool {
        matches!(self, Self::GameHours { .. } | Self::GameDay { .. })
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
    /// The quest whose failure starts it, and what ends it then.
    pub failure: Option<Failure>,
    /// The placed objects it keeps from the world while it runs, by their places in the world's
    /// list of placed objects.
    pub hides: Vec<usize>,
}

/// An event a quest's failure starts: as the quest fails for any player, the event comes on for
/// everyone, and goes off once `lasts` seconds have passed or the camp `until_killed` names has
/// made its creatures and keeps none alive, whichever comes first (at least one of them is named).
/// A failure while it is on changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure {
    pub quest: u32,
    pub lasts: Option<u32>,
    /// The camp, by its row in the spawn table.
    pub until_killed: Option<usize>,
}

impl Event {
    /// When the window of the host's clock it is on in at `unix` began and ends, if it is on.
    #[must_use]
    pub fn running(&self, unix: i64) -> Option<(i64, i64)> {
        self.windows.iter().find_map(|w| w.holding(unix))
    }

    /// When the window of the game's clock it is on in now began and ends, if it is on.
    #[must_use]
    pub fn running_by_the_game(&self, unix: i64, now: &GameNow<'_>) -> Option<(i64, i64)> {
        self.windows.iter().find_map(|w| w.holding_game(unix, now))
    }
}

/// A placed object an event gates: the object by its place in the world's list of placed objects,
/// so two alike at one place may wait on different events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectGate {
    pub object: usize,
    pub event: String,
}

/// What a line waits on of an event: that it runs (`on`), or that it does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineGate {
    pub event: String,
    pub on: bool,
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
    /// Lines said only while an event runs, or only while it does not.
    pub lines: Vec<(crate::talk::Line, LineGate)>,
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

    /// The event the placed object at this place in the world's list waits on, if any.
    #[must_use]
    pub fn of_object(&self, object: usize) -> Option<&str> {
        self.objects
            .iter()
            .find(|g| g.object == object)
            .map(|g| g.event.as_str())
    }

    /// The events that keep the placed object at this place in the world's list from the world
    /// while they run.
    pub fn hiding(&self, object: usize) -> impl Iterator<Item = &str> + '_ {
        self.events
            .iter()
            .filter(move |e| e.hides.contains(&object))
            .map(|e| e.key.as_str())
    }

    /// The events a quest's failure starts.
    pub fn failing(&self, quest: u32) -> impl Iterator<Item = &Event> + '_ {
        self.events
            .iter()
            .filter(move |e| e.failure.is_some_and(|f| f.quest == quest))
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

    /// What a line waits on of the events (the one placed person it may belong to aside: a line is
    /// the same line wherever its person stands in the pack).
    pub fn of_line<'a>(
        &'a self,
        line: &'a crate::talk::Line,
    ) -> impl Iterator<Item = &'a LineGate> + 'a {
        self.lines
            .iter()
            .filter(move |(l, _)| {
                crate::talk::Line {
                    object: line.object,
                    ..l.clone()
                } == *line
            })
            .map(|(_, g)| g)
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
