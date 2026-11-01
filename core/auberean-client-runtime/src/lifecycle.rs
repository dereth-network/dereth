//! An object's life as its scripts are told it: made, in the world, out of it, destroyed.
//!
//! Each event is one of the interface's functions, called on the object's own instance with no
//! arguments (a result slot, then the instance):
//!
//! - **made**: when an object (or a newer incarnation of one) is created, before it is placed;
//!   its instance is made then, as the client makes it with the object;
//! - **into the world**: when an object with no parent is placed, by its creation or by a position
//!   while it is out of the world;
//! - **out of the world**, then **after leaving it**: when an object in the world is contained,
//!   leaves, is carried by another, or goes from the client (a newer incarnation in its place, or
//!   the server destroying it);
//! - **destroyed**: when the server destroys an object (never the player), before it goes.
//!
//! The client places an object only where its body can stand; this client has no world of cells
//! to place it in, so an object counts as placed once it has a place and no parent.

use std::collections::BTreeSet;

use crate::host::{CallResult, Scripts};
use crate::skills::Asker;

/// The interface's functions an object's scripts are told its life through.
pub mod function {
    /// The interface's package.
    const INTERFACE: u32 = 0x7F;
    pub const HANDLE_CREATION: u32 = INTERFACE << 16 | 0x15;
    pub const HANDLE_DESTRUCTION: u32 = INTERFACE << 16 | 0x16;
    pub const HANDLE_ENTER_WORLD: u32 = INTERFACE << 16 | 0x17;
    pub const HANDLE_LEAVE_WORLD: u32 = INTERFACE << 16 | 0x18;
    pub const HANDLE_POST_LEAVE_WORLD: u32 = INTERFACE << 16 | 0x19;
}

/// Which objects are in the world as their scripts were told, and how many times each life
/// function was called.
#[derive(Clone, Debug, Default)]
pub struct Life {
    entered: BTreeSet<u64>,
    pub told: std::collections::BTreeMap<u32, u64>,
}

impl Life {
    /// Whether an object's scripts were told it is in the world.
    #[must_use]
    pub fn in_world(&self, object: u64) -> bool {
        self.entered.contains(&object)
    }

    /// Everything forgotten: the character left the world.
    pub fn clear(&mut self) {
        self.entered.clear();
    }

    /// Count a call of a life function.
    pub(crate) fn count(&mut self, function: u32) {
        *self.told.entry(function).or_default() += 1;
    }
}

impl Scripts {
    /// Call one of the life functions on an object's instance (made now if it has none), as the
    /// interface glue calls it: a result slot, then the instance. `None` when the object is not
    /// here or has no scripts.
    // D2-04679, D2-04680, D2-04681, D2-04682, D2-04683
    pub(crate) fn tell_life(&mut self, object: u64, function: u32) -> Option<CallResult> {
        let mut a = Asker {
            s: self,
            incomplete: Vec::new(),
        };
        a.ask_object(object, None, function, &[], &[], false)
            .map(|answer| answer.result)
    }
}

/// What one message does to the objects' lives, worked out before it takes effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// A new object, or a newer incarnation in an old one's place (which leaves the world first).
    Made { object: u64, replaced: bool },
    /// A position for an object.
    Placed(u64),
    /// The object is taken out of the world (contained, leaving, carried).
    Taken(u64),
    /// The server destroyed the object.
    Destroyed(u64),
}

/// The change a message makes to an object's life, given what the client has.
#[must_use]
pub fn change_of(
    m: &auberean_protocol::ServerMessage,
    model: &auberean_client_model::Model,
) -> Option<Change> {
    use auberean_protocol::ServerMessage as S;
    Some(match m {
        S::CreateObject(c) => {
            let old = model.object(c.id);
            if old.is_some_and(|o| o.instance_stamp == c.physics.instance_stamp) {
                return None;
            }
            Change::Made {
                object: c.id,
                replaced: old.is_some(),
            }
        }
        S::Position(p) | S::PositionCell(p) => Change::Placed(p.sender.id),
        S::Contain(o) => Change::Taken(o.0.id),
        S::ObjectLeaves(e) => Change::Taken(e.sender.id),
        S::Parent(e) => Change::Taken(e.sender.id),
        S::ForgetObject(o) if Some(o.0.id) != model.player => Change::Destroyed(o.0.id),
        _ => return None,
    })
}

/// Whether an object stands to be placed: it is here, has a place and has no parent.
#[must_use]
pub fn placeable(model: &auberean_client_model::Model, object: u64) -> bool {
    model
        .object(object)
        .is_some_and(|o| o.place.is_some() && o.parent.is_none())
}

impl Life {
    /// Mark an object in the world; whether it was out of it.
    pub(crate) fn enter(&mut self, object: u64) -> bool {
        self.entered.insert(object)
    }

    /// Mark an object out of the world; whether it was in it.
    pub(crate) fn leave(&mut self, object: u64) -> bool {
        self.entered.remove(&object)
    }
}
