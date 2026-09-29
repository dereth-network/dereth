//! The port of `Source/ACE.Server`'s gameplay: world objects, managers, factories, game messages,
//! events and actions.
//!
//! **Depends on** every server crate below it (`empyrean-common`, `empyrean-entity`,
//! `empyrean-tables`, `empyrean-content`, `empyrean-store`, `empyrean-dat`, `empyrean-net`) and the
//! shared engines and rules (`dereth-primitives`, `dereth-protocol`, `dereth-assets`,
//! `dereth-physics`, `dereth-animation`, `dereth-world-data`, `dereth-rules`); its tests
//! cross-check the port against the client's rules (`dereth-client-model`). **Used by** the
//! commands (`empyrean-command`), the server (`empyrean-server`) and the test kit
//! (`empyrean-testkit`).
//!
//! **Must never** open a socket (sessions and their datagrams are `empyrean-net`'s) or depend on a
//! client-only (`dereth/`) crate outside its tests (`cargo xtask separation`). Unported ACE code
//! is marked with `not_ported!`, never silently skipped.
//!
//! [`World`] owns all mutable world state, where ACE's static managers live; [`ObjectStore`] is the
//! single owner of every live object; [`dispatch`] is the generated virtual-method dispatch over
//! ACE's `WorldObject` class tree; and [`sessions`] is the game half of ACE's `Session`. The ported
//! folders keep ACE's layout: [`entity`], [`factories`], [`managers`], [`network`], [`physics`] and
//! [`world_objects`].

pub mod dispatch;
pub mod object_store;
pub mod sessions;
pub mod world;

pub use object_store::ObjectStore;
pub use world::World;

// @scaffold-mods begin
pub mod entity;
pub mod factories;
pub mod managers;
pub mod network;
pub mod physics;
pub mod world_objects;
// @scaffold-mods end
