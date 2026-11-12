//! A world's tallies ([`yalain_content::tallies`]) as the world keeps them: one count each, the
//! same for every player.
//!
//! **Filling.** A step or a give that adds to a tally adds as it runs, before anything it says, so
//! a line it says names the count it made; never past full. The moment a tally reaches full is
//! kept with its count.
//!
//! **Gates.** A step runs, a give takes an item, and a line is said only while every tally it
//! names is as it says: full or not yet full, its count at least or below a number.
//!
//! **Saying the count.** A line that fills with a tally takes the tally's percent (rounded down) as
//! its one variable; once the tally is full, its full line is said in its place, where it has one.
//!
//! **The event a fill starts.** An event a tally starts is on from its `after` seconds past the
//! moment the tally filled, for its `lasts` seconds, as if its own windows said so.
//!
//! **Resets.** At each world step a tally that resets on an event goes back to nothing if that
//! event has ended since the last step (also across a restart: the events that were on are kept
//! with the counts) or if the start it made of that event is over; one that resets every so many
//! seconds does at each new period of the calendar. As a tally resets, every object of the templates
//! it clears that the world made and that stands in the world, held by no one and no creature,
//! leaves it.
//!
//! **Kept.** The counts, their full moments, their periods and the events that were on are the
//! world record [`RECORD`], written in the pass that changes them and read at the first step, so a
//! restart keeps the stage. A count whose tally a world no longer has is dropped.
//!
//! A world with no tallies does none of this.

use std::collections::{BTreeMap, BTreeSet};

use auberean_script::native_object::Variable;
use yalain_content::quests::{Give, Step};
use yalain_content::talk::{Line, Words};
use yalain_content::tallies::{Adds, Gate, Reset, Tally};
use yalain_net::ServerNet;
use yalain_store::Store;

use crate::{progress, Clock, Server};

/// The world's record of its tallies.
pub const RECORD: &str = "tallies";

/// The record's first line.
const RECORD_HEAD: &str = "tallies 1";

/// The kind of variable a count is said as: a whole number.
const COUNT_KIND: u16 = 2;

/// One tally's count, and the moment (seconds since 1970) it last filled, while it is full; for a
/// tally that resets every so many seconds, the period it last reset in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Count {
    pub n: u32,
    pub filled: Option<i64>,
    pub period: Option<i64>,
}

/// The world's tallies, read from its record once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tallies {
    pub loaded: bool,
    pub counts: BTreeMap<String, Count>,
    /// The events tallies reset on that were on at the last step.
    pub on: BTreeSet<String>,
}

impl Tallies {
    /// The record in the server's encoding: a head line, then a line a count (`count`, its key,
    /// its number, its full moment and its period, `-` for none) and a line an event on (`on`, its
    /// key), tab-separated.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let opt = |v: Option<i64>| v.map_or_else(|| "-".to_string(), |v| v.to_string());
        let mut out = format!("{RECORD_HEAD}\n");
        for (key, c) in &self.counts {
            out.push_str(&format!(
                "count\t{key}\t{}\t{}\t{}\n",
                c.n,
                opt(c.filled),
                opt(c.period)
            ));
        }
        for key in &self.on {
            out.push_str(&format!("on\t{key}\n"));
        }
        out.into_bytes()
    }

    /// Read the record back; none when it does not read.
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(b).ok()?;
        let mut lines = text.lines();
        if lines.next()? != RECORD_HEAD {
            return None;
        }
        let opt = |s: &str| -> Option<Option<i64>> {
            if s == "-" {
                Some(None)
            } else {
                s.parse().ok().map(Some)
            }
        };
        let mut t = Self::default();
        for line in lines.filter(|l| !l.is_empty()) {
            let c: Vec<&str> = line.split('\t').collect();
            match c[..] {
                ["count", key, n, filled, period] => {
                    t.counts.insert(
                        key.to_string(),
                        Count {
                            n: n.parse().ok()?,
                            filled: opt(filled)?,
                            period: opt(period)?,
                        },
                    );
                }
                ["on", key] => {
                    t.on.insert(key.to_string());
                }
                _ => return None,
            }
        }
        Some(t)
    }
}

/// Read the tallies once. Whether they are read: while the store cannot answer, nothing about them
/// is kept.
pub fn ensure<S: Store>(s: &mut Server<S>) -> bool {
    if s.tallies.loaded {
        return true;
    }
    let Some(read) = crate::saves::read_record(s, RECORD, Tallies::from_bytes) else {
        return false;
    };
    let mut read = read.unwrap_or_default();
    let tallies = &s.content.tallies;
    read.counts.retain(|key, _| tallies.tally(key).is_some());
    s.tallies = Tallies {
        loaded: true,
        ..read
    };
    true
}

/// Keep the tallies as they now stand, at the end of this pass.
fn keep<S: Store>(s: &mut Server<S>) {
    if s.tallies.loaded {
        let b = s.tallies.to_bytes();
        s.saves.hold_record(RECORD, b);
    }
}

/// A tally's count now.
#[must_use]
pub fn count<S: Store>(s: &Server<S>, key: &str) -> u32 {
    s.tallies.counts.get(key).map_or(0, |c| c.n)
}

/// Is a tally full now?
#[must_use]
pub fn is_full<S: Store>(s: &Server<S>, key: &str) -> bool {
    s.content
        .tallies
        .tally(key)
        .is_some_and(|t| count(s, key) >= t.full)
}

fn holds<'a, S: Store>(s: &Server<S>, mut gates: impl Iterator<Item = &'a Gate>) -> bool {
    gates.all(|g| {
        s.content
            .tallies
            .tally(&g.tally)
            .is_some_and(|t| g.holds.by(count(s, &g.tally), t.full))
    })
}

/// May a step run now, by the tallies it names?
#[must_use]
pub fn step_for<S: Store>(s: &Server<S>, step: &Step) -> bool {
    let t = &s.content.tallies;
    t.is_empty() || holds(s, t.of_step(step))
}

/// May a give take its item now, by the tallies it names?
#[must_use]
pub fn give_for<S: Store>(s: &Server<S>, give: &Give) -> bool {
    let t = &s.content.tallies;
    t.is_empty() || holds(s, t.of_give(give))
}

/// May a line be said now, by the tallies it names?
#[must_use]
pub fn line_for<S: Store>(s: &Server<S>, line: &Line) -> bool {
    let t = &s.content.tallies;
    t.is_empty() || holds(s, t.of_line(line))
}

/// A step has run: what it adds.
pub fn step_ran<S: Store>(s: &mut Server<S>, step: &Step, clock: Clock) {
    if s.content.tallies.is_empty() {
        return;
    }
    let adds: Vec<Adds> = s.content.tallies.step_adds(step).cloned().collect();
    for a in adds {
        add(s, &a, clock);
    }
}

/// A give has taken its item: what it adds.
pub fn give_ran<S: Store>(s: &mut Server<S>, give: &Give, clock: Clock) {
    if s.content.tallies.is_empty() {
        return;
    }
    let adds: Vec<Adds> = s.content.tallies.give_adds(give).cloned().collect();
    for a in adds {
        add(s, &a, clock);
    }
}

/// Add to a tally, never past full; the moment it reaches full is kept.
fn add<S: Store>(s: &mut Server<S>, a: &Adds, clock: Clock) {
    if !ensure(s) {
        return;
    }
    let Some(full) = s.content.tallies.tally(&a.tally).map(|t| t.full) else {
        return;
    };
    let c = s.tallies.counts.entry(a.tally.clone()).or_default();
    if c.n >= full {
        return;
    }
    c.n = c.n.saturating_add(a.by).min(full);
    if c.n >= full {
        c.filled = Some(clock.unix);
    }
    keep(s);
}

/// The window of an event that a full tally started that holds `unix`, if one does.
#[must_use]
pub fn started<S: Store>(s: &Server<S>, event: &str, unix: i64) -> Option<(i64, i64)> {
    s.content.tallies.tallies.iter().find_map(|t| {
        let st = t.starts.as_ref().filter(|st| st.event == event)?;
        let filled = s.tallies.counts.get(&t.key)?.filled?;
        let from = filled + i64::from(st.after);
        let until = from + i64::from(st.lasts);
        (from <= unix && unix < until).then_some((from, until))
    })
}

/// Has the start a full tally made of its own event come and gone by `unix`?
fn start_over(t: &Tally, c: &Count, unix: i64) -> bool {
    match (&t.starts, &t.resets, c.filled) {
        (Some(st), Reset::Ends(e), Some(filled)) if st.event == *e => {
            unix >= filled + i64::from(st.after) + i64::from(st.lasts)
        }
        _ => false,
    }
}

/// The world's step, with the events on now: reset each tally due to, and keep which events
/// tallies reset on are on.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, on: &BTreeSet<String>, clock: Clock) {
    if s.content.tallies.is_empty() || !ensure(s) {
        return;
    }
    let tallies = s.content.tallies.tallies.clone();
    let mut changed = false;
    let mut due = Vec::new();
    for t in &tallies {
        let c = s.tallies.counts.entry(t.key.clone()).or_default();
        match &t.resets {
            Reset::Never => {}
            Reset::Ends(e) => {
                let ended = s.tallies.on.contains(e) && !on.contains(e);
                if ended || start_over(t, c, clock.unix) {
                    due.push(t);
                }
            }
            Reset::Every(seconds) => {
                let period = clock.unix.div_euclid(i64::from(*seconds));
                match c.period {
                    Some(p) if p == period => {}
                    None => {
                        c.period = Some(period);
                        changed = true;
                    }
                    Some(_) => {
                        c.period = Some(period);
                        due.push(t);
                    }
                }
            }
        }
    }
    let named: BTreeSet<String> = tallies
        .iter()
        .filter_map(|t| match &t.resets {
            Reset::Ends(e) => Some(e.clone()),
            _ => None,
        })
        .filter(|e| on.contains(e))
        .collect();
    if named != s.tallies.on {
        s.tallies.on = named;
        changed = true;
    }
    for t in due {
        reset(s, net, t);
        changed = true;
    }
    if changed {
        keep(s);
    }
}

/// A tally goes back to nothing, and what it clears leaves the world.
fn reset<S: Store>(s: &mut Server<S>, net: &mut ServerNet, t: &Tally) {
    if let Some(c) = s.tallies.counts.get_mut(&t.key) {
        c.n = 0;
        c.filled = None;
    }
    if t.clears.is_empty() {
        return;
    }
    let gone: Vec<u64> = s
        .things
        .by_id
        .values()
        .filter(|x| {
            t.clears.contains(&x.template)
                && x.at.is_some()
                && x.container.is_none()
                && x.creature.is_none()
        })
        .map(|x| x.id)
        .collect();
    for id in gone {
        crate::death::vanish(s, net, id);
    }
}

/// What a line says now: its words, or its tally's full line once the tally is full; and the
/// variable that says the count, for a line that fills with one.
#[must_use]
pub fn said<S: Store>(s: &Server<S>, words: &Words) -> (Words, Option<Variable>) {
    let Some(f) = s.content.tallies.fill_of(words) else {
        return (words.clone(), None);
    };
    let Some(t) = s.content.tallies.tally(&f.tally) else {
        return (words.clone(), None);
    };
    let n = count(s, &t.key);
    if n >= t.full {
        if let Some(full) = &f.full {
            return (full.clone(), None);
        }
    }
    let count = (f.variable != 0).then(|| progress::word(f.variable, COUNT_KIND, t.percent(n)));
    (words.clone(), count)
}
