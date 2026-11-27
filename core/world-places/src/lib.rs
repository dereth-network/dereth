//! A world's places from its world database: where each portal stands, where it leads and who may
//! use it, its lifestones, vendors and people, and its towns and their drop points, written as the
//! documents a server publishes and a launcher builds.
//!
//! The documents are a portal table and the places ([`documents`]); a launcher builds them for a
//! world from the public database of the world's era ([`eras`]).
//!
//! **Depends on** `dereth-primitives` alone. **Used by** Empyrean's status endpoint
//! (`empyrean-server`), which publishes the documents of the content it loaded; `dereth-places`,
//! which builds them from an era's database; and `dereth-navigation`, which reads what a portal
//! asks ([`Requirement`]). The server and the tool write the documents with this one builder, so
//! they agree byte for byte on the same content.
//!
//! **Must never** read a file, touch the network or keep a clock: a world database arrives
//! through [`WorldSource`] (a server's own content, or a dump read by [`sql::read`]). It builds
//! for `wasm32-unknown-unknown`.

pub mod content;
pub mod documents;
pub mod eras;
pub mod requirement;
pub mod sql;
pub mod text;
pub mod towns;

pub use content::{
    GameEvent, Landmark, Placement, Sending, Spawn, Spot, Weenie, WorldContent, WorldSource,
};
pub use documents::{Documents, Source, WorldPlaces};
pub use requirement::Requirement;
