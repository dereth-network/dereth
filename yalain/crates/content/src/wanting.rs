//! A world's wanting objects: placed objects that each want one item of a set, drawn at random and
//! kept until a use or a give of it is met (crystals each wanting one of three serums).
//!
//! **Drawn.** Each object of a row draws one item of the row's set by weight the first time an item
//! of the set comes to it, and keeps it: the same for every player, and apart from the row's other
//! objects.
//!
//! **Refused.** Another item of the set used on the object or given to it is refused once the
//! player has come to it: the object says the row's line, the item is kept by the player, and no
//! quest step runs.
//!
//! **Met.** The item drawn, used or given so that it runs a quest step (or, given, a give row takes
//! it), is the object's want met: it is forgotten, and the next item of the set to come draws again.
//! The wanted item otherwise goes on as any other, its want kept.
//!
//! A world with no wanting objects does none of this.

/// One row: the placed objects that each want an item (by their places in the world's list of
/// placed objects), the set an item is drawn from with each one's weight, and what an object says
/// to an item of the set it does not want.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanting {
    pub key: String,
    /// At least one, each in no other row.
    pub objects: Vec<usize>,
    /// `(template, weight)`, at least two, each template once and each weight above 0.
    pub items: Vec<(u32, u32)>,
    /// What an object says to an item of the set it does not want: a string and its table.
    pub refused: Option<(u32, u32)>,
}

impl Wanting {
    /// Is an item of this template one of the row's set?
    #[must_use]
    pub fn in_set(&self, item: u32) -> bool {
        self.items.iter().any(|(t, _)| *t == item)
    }
}

/// Every wanting row of a world.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WantTable {
    pub rows: Vec<Wanting>,
}

impl WantTable {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The row the placed object at this place in the world's list wants by, if one does.
    #[must_use]
    pub fn of_object(&self, object: usize) -> Option<&Wanting> {
        self.rows.iter().find(|r| r.objects.contains(&object))
    }
}
