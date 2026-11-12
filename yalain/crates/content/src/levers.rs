//! A world's levers: placed objects whose use opens doors of the world, for everyone, for a while.
//!
//! **Pulling.** A row names its levers (one or more placed objects) and the doors it opens (placed
//! doors). A use of a lever pulls it; once every lever of a row is pulled, the row's doors open as
//! a use opens a door, and the pulls start over. A row with one lever opens its doors at each pull.
//! A pull of a row with several waits for the others for the row's `within` seconds, or for as long
//! as it takes when the row names none.
//!
//! **Open.** The doors stay open for the row's `open_for` seconds in place of a door's own ten,
//! then close by themselves (a locked door locking again as it closes); with none they stay open
//! until a use closes them. A pull while they stand open holds them open that long again.
//!
//! A world with no levers does none of this.

/// One row: the levers that must be pulled, and the doors their pulls open. Objects are named by
/// their places in the world's list of placed objects.
#[derive(Debug, Clone, PartialEq)]
pub struct Lever {
    pub key: String,
    pub levers: Vec<usize>,
    pub opens: Vec<usize>,
    /// How long the doors stay open once open, in seconds; none: until a use closes them.
    pub open_for: Option<f64>,
    /// How long a pull waits for the row's other levers, in seconds; none: for as long as it takes.
    pub within: Option<f64>,
}

/// Every lever row of a world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LeverTable {
    pub rows: Vec<Lever>,
}

impl LeverTable {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Is the placed object at this place in the world's list a lever of some row?
    #[must_use]
    pub fn is_lever(&self, object: usize) -> bool {
        self.rows.iter().any(|r| r.levers.contains(&object))
    }

    /// The rows a placed object is a lever of.
    pub fn pulled_by(&self, object: usize) -> impl Iterator<Item = &Lever> {
        self.rows.iter().filter(move |r| r.levers.contains(&object))
    }
}
