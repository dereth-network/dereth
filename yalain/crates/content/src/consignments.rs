//! What a world allows a player to put up for sale with each broker.
//!
//! **A player's own** consignments at one broker, on sale, expired or sold and not yet collected:
//! as many as the world sets, and more by what its effects of extra consignments add. A world that
//! sets none holds a player to nothing of its own: the broker's own limit for an account alone.

/// A world's terms for consignments: none set, none asked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Consignments {
    /// How many a player holds at one broker before its effects add any.
    pub at_a_broker: Option<u32>,
}
