//! What a world asks of an oath of allegiance, beyond what every oath asks.
//!
//! **A least level** a vassal must be at to swear, and **a rank** a vassal must be at or under to
//! swear to a patron of a higher rank than its own. A world that sets neither asks neither: every
//! level and rank may swear.

/// A world's terms for an oath: none set, none asked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Oath {
    /// The least level a vassal swears at.
    pub least_level: Option<i32>,
    /// The highest rank from which a vassal swears to a patron of a higher rank.
    pub rank_at_most: Option<u32>,
}

impl Oath {
    /// Is a vassal of `level` too low to swear?
    #[must_use]
    pub fn too_low(&self, level: i32) -> Option<i32> {
        self.least_level.filter(|l| level < *l)
    }

    /// Is a vassal of rank `mine` too high to swear to a patron of rank `its`?
    #[must_use]
    pub fn rank_too_high(&self, mine: u32, its: u32) -> Option<u32> {
        self.rank_at_most.filter(|most| its > mine && mine > *most)
    }
}
