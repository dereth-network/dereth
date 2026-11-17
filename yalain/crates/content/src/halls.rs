//! A world's allegiance halls: placed binding stones a monarch uses to bind its whole allegiance's
//! Allegiance Recall to the hall.
//!
//! **Binding.** The monarch of an allegiance who uses a hall's stone, the stone's own requirements
//! met (the quest its template needs), pays the row's cost in money and binds its allegiance to the
//! hall: from then on Allegiance Recall takes any member there, until the monarch binds another
//! hall. A monarch short of the cost is told so and nothing is paid or bound.
//!
//! **The place.** A row names where the recall takes a member, and the name the allegiance's
//! export gives the hall (the place's destination name, empty when it has none).
//!
//! A world with no halls binds no allegiance.

use crate::Place;

/// One row: the placed stone (by its place in the world's list of placed objects), where the recall
/// takes a member, the name the export gives it, and what binding costs.
#[derive(Debug, Clone, PartialEq)]
pub struct Hall {
    pub key: String,
    /// In no other row.
    pub stone: usize,
    pub place: Place,
    /// The place's destination name, empty when it has none.
    pub name: String,
    /// The money the monarch pays to bind.
    pub cost: u32,
}

/// Every hall of a world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HallTable {
    pub rows: Vec<Hall>,
}

impl HallTable {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The hall whose stone is the placed object at this place in the world's list, if one is.
    #[must_use]
    pub fn of_stone(&self, object: usize) -> Option<&Hall> {
        self.rows.iter().find(|r| r.stone == object)
    }
}
