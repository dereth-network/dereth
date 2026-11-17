//! What the world database says of kingdoms: the kingdoms themselves, as a description's faction
//! names them and as a row writes them; the effects that raise a player's kingdom points; the quest
//! steps and gives for one kingdom's players alone; what a kingdom's shrines ask of a player who
//! would join it; and the steps that change whom a player lets fight it.

use std::collections::BTreeMap;

use crate::quests::{Give, Step};

/// The kingdoms, as a description's faction names them.
pub mod kingdom {
    /// The Shadow.
    pub const SHADOW: u32 = 1;
    /// The Order of Dereth.
    pub const ORDER: u32 = 2;
    /// The Dominion.
    pub const DOMINION: u32 = 4;
    /// The neutral kingdom.
    pub const NEUTRAL: u32 = 0x8000_0000;
    /// Every kingdom, the neutral one first.
    pub const ALL: [u32; 4] = [NEUTRAL, SHADOW, ORDER, DOMINION];

    /// Each kingdom by the word a row names it with.
    pub const WORDS: [(&str, u32); 4] = [
        ("neutral", NEUTRAL),
        ("shadow", SHADOW),
        ("order", ORDER),
        ("dominion", DOMINION),
    ];

    /// The kingdom a row's word names.
    #[must_use]
    pub fn from_word(w: &str) -> Option<u32> {
        WORDS.iter().find(|(word, _)| *word == w).map(|(_, k)| *k)
    }

    /// The word a row names a kingdom with.
    #[must_use]
    pub fn word(k: u32) -> Option<&'static str> {
        WORDS.iter().find(|(_, x)| *x == k).map(|(w, _)| *w)
    }
}

/// What an effect does to the kingdom points of the player it is put on: raises them by `add`, but
/// never above `up_to`, and never lowers points already above it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointsBonus {
    pub add: i32,
    pub up_to: i32,
}

impl PointsBonus {
    /// The points a player holding `points` holds after the effect.
    #[must_use]
    pub fn raised(self, points: i32) -> i32 {
        if points >= self.up_to {
            points
        } else {
            points.saturating_add(self.add).min(self.up_to)
        }
    }
}

/// What a kingdom's faction shrines ask of a player who would join the kingdom there: a least level,
/// that it be sworn to no other kingdom (in none, or the neutral one), that it left its last kingdom
/// at least `wait` seconds ago, and gold by its level.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Join {
    pub least_level: i32,
    /// A player of another kingdom than the neutral one is refused.
    pub sworn_refused: bool,
    /// Seconds by the calendar since the player last left a kingdom; 0 for no wait.
    pub wait: u32,
    /// The gold a join costs from each level on: levels rising from 0.
    pub costs: Vec<(i32, u32)>,
}

impl Join {
    /// The gold a player of `level` pays to join: that of the last band at or below its level.
    #[must_use]
    pub fn cost(&self, level: i32) -> u32 {
        self.costs
            .iter()
            .take_while(|(from, _)| *from <= level)
            .last()
            .map_or(0, |(_, gold)| *gold)
    }
}

/// What a step does to the permissions its player always lets through (the kinds of attacker it
/// lets harm it, and whom kingdoms may fight anywhere): the bits it grants, then those it revokes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Permits {
    pub grant: u32,
    pub revoke: u32,
}

impl Permits {
    /// The permissions a player holding `allow` holds after the step.
    #[must_use]
    pub const fn applied(self, allow: u32) -> u32 {
        (allow | self.grant) & !self.revoke
    }
}

/// The world database's kingdom rows, as the server keeps them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KingdomTable {
    /// The effects that raise kingdom points, by their saved states.
    pub points: BTreeMap<u32, PointsBonus>,
    /// Quest steps that run only for players of one kingdom: the step and the kingdom.
    pub steps: Vec<(Step, u32)>,
    /// Gives taken only from players of one kingdom: the give and the kingdom.
    pub gives: Vec<(Give, u32)>,
    /// What each kingdom's shrines ask of a player who would join it, by the kingdom; a kingdom no
    /// row names asks nothing.
    pub joins: BTreeMap<u32, Join>,
    /// What steps do to their player's permissions, by the step's quest and key.
    pub permits: BTreeMap<(u32, String), Permits>,
}

impl KingdomTable {
    /// Is a step for players of this kingdom: for every kingdom, or for this one.
    #[must_use]
    pub fn step_for(&self, step: &Step, kingdom: u32) -> bool {
        for_kingdom(&self.steps, step, kingdom)
    }

    /// Is a give taken from players of this kingdom: from every kingdom, or from this one.
    #[must_use]
    pub fn give_for(&self, give: &Give, kingdom: u32) -> bool {
        for_kingdom(&self.gives, give, kingdom)
    }
}

/// Is a row for players of a kingdom: no gate names it, or one names this kingdom.
fn for_kingdom<T: PartialEq>(gates: &[(T, u32)], row: &T, kingdom: u32) -> bool {
    let mut gated = false;
    for (r, k) in gates {
        if r == row {
            if *k == kingdom {
                return true;
            }
            gated = true;
        }
    }
    !gated
}
