//! A world's lockfile: the ids its symbols were given, kept so a rebuild gives the same ones.
//!
//! ```toml
//! # This world's ids, numbered by lyceum build. Commit it with world.toml: a rebuild keeps
//! # every id here.
//!
//! [render-surface]
//! "sample:stripes" = 0x06007577
//!
//! [wcid]
//! "sample:guide" = 100000
//!
//! [guid]
//! "sample:guide@A9B4" = 0x7A9B4FFF
//!
//! [cell]
//! "sample:hall@A9B4" = 0xA9B4018A
//!
//! [treasure]
//! "sample:drudge-loot" = 100000
//! ```
//!
//! Ids are per world: one world's lockfile says nothing about another's. A symbol keeps its id as
//! long as the lockfile holds it, and the lockfile keeps a symbol no source uses any more, so an id
//! is never handed to something else in the same world.
//!
//! The lockfile is the shared pack format's (`dereth_pack::lock`), in Lyceum's kinds.

/// The ids of a world's symbols, by kind.
pub type Lock = dereth_pack::Lock<crate::dialect::Ac1>;
