//! A world's offerings: a placed object that takes a set of items from players and, once it holds
//! the whole set, opens the way for a while.
//!
//! **Taking.** An item of the set given to the object is kept (it leaves the giver's pack and the
//! world), whoever gives it: what the object holds is the world's, the same for every player. The
//! object says its lines for a give of that item. An item of the set it holds already, or any
//! while it stands open, is refused and handed back.
//!
//! **What stands for it.** An item of the set may name a placed object that stands in the world
//! only while the offering holds that item and is not yet open (a flare for each part given).
//!
//! **Open.** As the last item of the set is given, the objects standing for the others go, and the
//! offering's own objects (a portal) stand in the world, for everyone, for its `lasts` seconds;
//! then they go and the offering holds nothing again. What it holds and how long it stays open are
//! kept across a restart.
//!
//! A world with no offerings does none of this.

/// One item of an offering's set: its template, and the placed object that stands while the
/// offering holds it, if any (by its place in the world's list of placed objects).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub item: u32,
    pub shows: Option<usize>,
}

/// One offering: the placed object given to, its set, the placed objects that stand once it holds
/// the whole set, and for how long, in seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offering {
    pub key: String,
    pub object: usize,
    pub takes: Vec<Part>,
    pub opens: Vec<usize>,
    pub lasts: u32,
}

impl Offering {
    /// Does the offering stand this placed object only for a while: one standing for an item, or
    /// one it opens?
    #[must_use]
    pub fn gates(&self, object: usize) -> bool {
        self.opens.contains(&object) || self.takes.iter().any(|p| p.shows == Some(object))
    }
}

/// Every offering of a world.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OfferingTable {
    pub offerings: Vec<Offering>,
}

impl OfferingTable {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.offerings.is_empty()
    }

    /// The offering a placed object takes, if any.
    #[must_use]
    pub fn of_object(&self, object: usize) -> Option<&Offering> {
        self.offerings.iter().find(|o| o.object == object)
    }

    /// The offering by its key.
    #[must_use]
    pub fn offering(&self, key: &str) -> Option<&Offering> {
        self.offerings.iter().find(|o| o.key == key)
    }
}
