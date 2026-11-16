//! A world's rounds: one creature at a time going round places in order, each death bringing the
//! next (a guardian that moves on from shrine to shrine as it is killed).
//!
//! **One at a time.** As the world starts, the first stop's creature stands at its place. When the
//! round's creature dies, the next stop's stands at that stop's place, the first's after the last:
//! the creature the stop names for its killer's kingdom, where it names one, else its own.
//!
//! A world with no rounds does none of this.

use crate::Place;

/// One stop of a round: where its creature stands, and which creature.
#[derive(Debug, Clone, PartialEq)]
pub struct Stop {
    pub at: Place,
    /// The creature that stands here as the world starts, and after a killer of no kingdom the stop
    /// names.
    pub template: u32,
    /// The creature that stands here after a killer of a kingdom, `(kingdom, template)`, the
    /// kingdom as a description's faction names it.
    pub by_kingdom: Vec<(u32, u32)>,
}

impl Stop {
    /// The creature that stands here after a killer of `kingdom` (none: a killer in no kingdom, or
    /// no player).
    #[must_use]
    pub fn template_for(&self, kingdom: Option<u32>) -> u32 {
        kingdom
            .and_then(|k| self.by_kingdom.iter().find(|(x, _)| *x == k))
            .map_or(self.template, |(_, t)| *t)
    }
}

/// One row: its stops, in the order its creature goes round them.
#[derive(Debug, Clone, PartialEq)]
pub struct Round {
    pub key: String,
    /// At least two.
    pub stops: Vec<Stop>,
    /// How long after a death the next stop's creature stands, in seconds; at once when none.
    pub gap: Option<f64>,
}

/// Every round of a world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoundTable {
    pub rows: Vec<Round>,
}

impl RoundTable {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}
