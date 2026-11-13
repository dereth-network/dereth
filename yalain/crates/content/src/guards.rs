//! A world's guarded objects: placed objects usable only while every creature of a camp is dead
//! (a rift no one may close while its keeper stands).
//!
//! **Refused.** A use of a guarded object while a creature of its camp lives is refused where the
//! player has walked to it: the object says the row's line, and nothing else comes of the use.
//!
//! **Hidden.** A row that hides its objects keeps them from the world while a creature of the camp
//! lives: they come as its last creature dies and go as one comes back.
//!
//! **Gone a while.** A row that names a time takes an object from the world at each use of it that
//! is carried out, for that long; then it comes back (unless the row hides it again by then).
//!
//! A world with no guarded objects does none of this.

/// One row: the camp whose creatures guard, and the placed objects they guard (by their places in
/// the world's list of placed objects, and the camp by its place in the spawn table).
#[derive(Debug, Clone, PartialEq)]
pub struct Guarded {
    pub key: String,
    pub camp: usize,
    pub objects: Vec<usize>,
    /// What an object says to a player whose use is refused: a string and its table.
    pub refused: Option<(u32, u32)>,
    /// The objects stand in the world only while every creature of the camp is dead.
    pub hidden: bool,
    /// How long a use carried out takes an object from the world, in seconds.
    pub gone_for: Option<f64>,
}

/// Every guarded row of a world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuardTable {
    pub rows: Vec<Guarded>,
}

impl GuardTable {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The rows that guard the placed object at this place in the world's list.
    pub fn of_object(&self, object: usize) -> impl Iterator<Item = &Guarded> {
        self.rows
            .iter()
            .filter(move |r| r.objects.contains(&object))
    }
}
