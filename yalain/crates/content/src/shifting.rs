//! A world's shifting objects: placed objects of which one stands at a time, moving to another of
//! their places after each use (remains that crumble once searched and lie elsewhere next time).
//!
//! **One at a time.** Of a row's objects one stands in the world, picked at random as the world
//! starts; the others are kept from it.
//!
//! **Moving on.** A use of the one standing that is carried out takes it from the world, and
//! another of the row's objects, never the same, stands in its place: at once, or after the row's
//! time gone. A row that names how long one stands moves it on after that long unused too.
//!
//! A world with no shifting objects does none of this.

/// One row: the placed objects it moves among, by their places in the world's list of placed
/// objects.
#[derive(Debug, Clone, PartialEq)]
pub struct Shifting {
    pub key: String,
    /// At least two, each in no other row.
    pub objects: Vec<usize>,
    /// How long after a use none of them stands, in seconds; the next stands at once when none.
    pub gone_for: Option<f64>,
    /// How long one stands unused before it moves on, in seconds; until it is used when none.
    pub moves_after: Option<f64>,
}

/// Every shifting row of a world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShiftTable {
    pub rows: Vec<Shifting>,
}

impl ShiftTable {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The row that moves the placed object at this place in the world's list, if one does.
    #[must_use]
    pub fn of_object(&self, object: usize) -> Option<&Shifting> {
        self.rows.iter().find(|r| r.objects.contains(&object))
    }
}
