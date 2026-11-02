//! The client's model of the game: the event queue's order and holding rules, the character list,
//! the player, the objects and their qualities, the inventory.
//!
//! **Depends on** the messages (`auberean-protocol`) and `auberean-primitives`. **Used by** the
//! client runtime (`auberean-client-runtime`) and the SDK (`auberean-client-sdk`).
//!
//! - [`events`]: the event queue: which received messages wait (for the player, for an object,
//!   for the messages stamped before them), in what order they take effect, and when the client
//!   asks for an object again.
//! - [`model`]: what the messages that took effect built: the character list, the player, the
//!   objects with their qualities and places, the inventory.
//!
//! **Must never** do I/O or read a clock: times come in as numbers.

pub mod characters;
pub mod events;
pub mod ids;
pub mod model;

pub use events::{EventQueue, Item, WorldState};
pub use model::{Effect, Model, Object, Qualities};
