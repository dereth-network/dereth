//! A world's hazards: placed objects, or spots, that harm a player who uses one or comes within its
//! reach (a gate whose use is death, a trapped floor).
//!
//! **Used.** A row that harms at a use harms a player who uses one of its objects, once it has
//! come to it, after what the use does otherwise (its line, its quest steps).
//!
//! **Touched.** A row that harms at a touch harms a player who comes within its reach of one of its
//! objects or spots, by straight-line distance from the player's feet; not again while the player
//! stays within it, and again once it has left and come back.
//!
//! **Harm.** The row's effects are put on the player in order, cast by the object at their
//! spellcraft; then its harm takes so much health, or all of it. A dead player is harmed by
//! nothing.
//!
//! A world with no hazards does none of this.

/// One row: its objects (by their places in the world's list of placed objects) and spots, when it
/// harms, and what it does.
#[derive(Debug, Clone, PartialEq)]
pub struct Hazard {
    pub key: String,
    pub objects: Vec<usize>,
    /// Places in the world: a cell and a place in its landblock.
    pub spots: Vec<(u32, [f32; 3])>,
    /// A use of one of its objects harms.
    pub used: bool,
    /// Coming within so many meters of one of its objects or spots harms.
    pub touch: Option<f32>,
    /// The effects put on, by their saved states, each at its spellcraft, in order.
    pub effects: Vec<(u32, f32)>,
    pub harm: Option<Harm>,
}

/// The health a hazard takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harm {
    /// So many points of health.
    Points(u32),
    /// All of it: the player dies.
    All,
}

/// Every hazard of a world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HazardTable {
    pub rows: Vec<Hazard>,
}

impl HazardTable {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The rows that harm at a use of the placed object at this place in the world's list.
    pub fn used(&self, object: usize) -> impl Iterator<Item = &Hazard> {
        self.rows
            .iter()
            .filter(move |r| r.used && r.objects.contains(&object))
    }
}
