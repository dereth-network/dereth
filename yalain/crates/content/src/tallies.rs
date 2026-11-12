//! A world's tallies: named counts the whole world keeps, which players' errands fill (supplies
//! gathered for a town's festival, a road's dangers cleared), and what follows them.
//!
//! **Filling.** A quest step or a give that names a tally adds its amount as it runs, never past
//! the tally's full. Steps, gives and lines may wait on a tally being full or not yet full, or on
//! its count being at least, or below, some number, so a person's offers and lines change as the
//! count does; a line may say the count as a percent, and say another line once it is full.
//!
//! **What fullness starts.** A tally may start an event some seconds after it fills, for a time:
//! an event the tally starts is on then as if its own clock said so (an event with no windows of
//! its own is on only so).
//!
//! **Resets.** A tally goes back to nothing as an event ends, every so many seconds by the
//! calendar, or never; as it does, the objects of the templates it clears that effects placed in
//! the world are taken away.
//!
//! A world with no tallies does none of this.

use crate::quests::{Give, Step};
use crate::talk::{Line, Words};

/// What a tally starts when it fills: an event, on from `after` seconds after the tally filled
/// for `lasts` seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub event: String,
    pub after: u32,
    pub lasts: u32,
}

/// When a tally goes back to nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Reset {
    /// It keeps its count for good.
    #[default]
    Never,
    /// As this event ends.
    Ends(String),
    /// Every so many seconds by the calendar, counted from 00:00 UTC on 1 January 1970 (86400:
    /// each day at 00:00 UTC).
    Every(u32),
}

/// One tally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tally {
    pub key: String,
    /// The count at which it is full, above 0.
    pub full: u32,
    pub starts: Option<Start>,
    pub resets: Reset,
    /// The templates of the objects effects placed in the world that go as it resets.
    pub clears: Vec<u32>,
}

impl Tally {
    /// How full a count is, as a percent rounded down.
    #[must_use]
    pub fn percent(&self, count: u32) -> u32 {
        let p = u64::from(count.min(self.full)) * 100 / u64::from(self.full.max(1));
        u32::try_from(p).unwrap_or(100)
    }
}

/// What a step, a give or a line waits on: a tally as it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gate {
    pub tally: String,
    pub holds: Holds,
}

/// What a gate needs of its tally's count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holds {
    Full,
    NotFull,
    /// At least this many.
    AtLeast(u32),
    /// Fewer than this many.
    Below(u32),
}

impl Holds {
    /// Does a count of a tally full at `full` hold it?
    #[must_use]
    pub const fn by(self, count: u32, full: u32) -> bool {
        match self {
            Self::Full => count >= full,
            Self::NotFull => count < full,
            Self::AtLeast(n) => count >= n,
            Self::Below(n) => count < n,
        }
    }
}

/// What a step or a give adds to a tally as it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adds {
    pub tally: String,
    pub by: u32,
}

/// A line that says a tally's count: its one variable, which takes the percent (0: the install's
/// text was not at hand to name it, and the line is said as it is), and what is said in its place
/// once the tally is full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fill {
    pub tally: String,
    pub variable: u32,
    pub full: Option<Words>,
}

/// A world's tallies and what fills, waits on and says them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TallyTable {
    pub tallies: Vec<Tally>,
    /// Quest steps that run only while a tally is full, or not yet full.
    pub steps: Vec<(Step, Gate)>,
    /// Gives that take an item only while a tally is full, or not yet full.
    pub gives: Vec<(Give, Gate)>,
    /// Lines said only while a tally is full, or not yet full.
    pub lines: Vec<(Line, Gate)>,
    /// What steps and gives add as they run.
    pub step_adds: Vec<(Step, Adds)>,
    pub give_adds: Vec<(Give, Adds)>,
    /// The lines that say a tally's count, by their words.
    pub fills: Vec<(Words, Fill)>,
}

/// Are two steps one, by their quest and key?
fn same_step(a: &Step, b: &Step) -> bool {
    a.quest == b.quest && a.key == b.key
}

impl TallyTable {
    /// Does the world keep no tallies?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tallies.is_empty()
    }

    /// The tally of a key.
    #[must_use]
    pub fn tally(&self, key: &str) -> Option<&Tally> {
        self.tallies.iter().find(|t| t.key == key)
    }

    /// What a step waits on: the gates of the step of its quest and key, whatever else two steps
    /// share.
    pub fn of_step<'a>(&'a self, step: &'a Step) -> impl Iterator<Item = &'a Gate> + 'a {
        self.steps
            .iter()
            .filter(move |(s, _)| same_step(s, step))
            .map(|(_, g)| g)
    }

    /// What a give waits on.
    pub fn of_give<'a>(&'a self, give: &'a Give) -> impl Iterator<Item = &'a Gate> + 'a {
        self.gives
            .iter()
            .filter(move |(g, _)| g == give)
            .map(|(_, g)| g)
    }

    /// What a line waits on (the one placed person it may belong to aside: a line is the same
    /// line wherever its person stands in the pack).
    pub fn of_line<'a>(&'a self, line: &'a Line) -> impl Iterator<Item = &'a Gate> + 'a {
        self.lines
            .iter()
            .filter(move |(l, _)| {
                Line {
                    object: line.object,
                    ..l.clone()
                } == *line
            })
            .map(|(_, g)| g)
    }

    /// What a step adds as it runs: the adds of the step of its quest and key.
    pub fn step_adds<'a>(&'a self, step: &'a Step) -> impl Iterator<Item = &'a Adds> + 'a {
        self.step_adds
            .iter()
            .filter(move |(s, _)| same_step(s, step))
            .map(|(_, a)| a)
    }

    /// What a give adds as it runs.
    pub fn give_adds<'a>(&'a self, give: &'a Give) -> impl Iterator<Item = &'a Adds> + 'a {
        self.give_adds
            .iter()
            .filter(move |(g, _)| g == give)
            .map(|(_, a)| a)
    }

    /// The count a line's words say, if they say one.
    #[must_use]
    pub fn fill_of(&self, words: &Words) -> Option<&Fill> {
        self.fills.iter().find(|(w, _)| w == words).map(|(_, f)| f)
    }
}
