//! The pack format the world builders share: worlds and their packs, symbols and the lockfile that
//! keeps their ids, format versions and upgrades, and the conflicts between packs, each game's
//! words supplied by its builder.
//!
//! **Depends on** `dereth-dat` (its errors), `serde`, `toml` and `toml_edit`. **Used by** each
//! world builder, which supplies its game's words for the format as a [`Dialect`]: Lyceum
//! (`lyceum-foundry`) is one.
//!
//! **Must never** know a game's records, operations or server content: what a symbol names, what
//! a world is built on and what an override's items are called are the dialect's. And it reads
//! and writes sources only through a [`store::SourceStore`], so a builder with no file system of
//! its own supplies one.
//!
//! A **world** ([`world`]) lists its packs in load order and says which game it is for; a
//! **pack** ([`pack`]) is a folder of sources, its header naming it. Everything a pack makes is
//! named by a **symbol** ([`symbol`]), never numbered: a build numbers the symbols for that world
//! alone and keeps the ids in the world's **lockfile** ([`lock`]). A source names the **format**
//! it is written in ([`format`](mod@format)), and an upgrade rewrites only that. Where two packs set one thing
//! differently, the world's overrides say which wins ([`conflict`]); a build refuses any conflict
//! they do not settle, and lists them all. A row of a pack can say where it came from
//! ([`provenance`]): the data files, a recorded session, another project's table, or a person.
//! An editor holds a world's sources open as text ([`session`]): each change is new text for some
//! files, undone and redone as text, and saved only where it differs from the store's.
//! A world can be ordered **layers** ([`lineage`]): its own packs, the base, then patches, each
//! a delta on the layer before or a checkpoint, a whole known world pinned to its source; the
//! world at any layer is the layers up to it folded, field by field, each field with its
//! provenance ([`fold`]), and the fold lists what is wrong or worth knowing in each layer. A game
//! whose rows are records of fields reads them from TOML tables as [`records`] says.
//! [`error`] is what any of that could not do.

pub mod conflict;
pub mod dialect;
pub mod error;
pub mod fold;
pub mod format;
pub mod lineage;
pub mod lock;
pub mod pack;
pub mod provenance;
pub mod records;
pub mod session;
pub mod store;
pub mod symbol;
pub mod world;

pub use dialect::{Dialect, DEFAULT_GAME};
pub use error::{Error, Loc};
pub use lock::Lock;
pub use symbol::{Numbering, SymbolKind};
pub use world::World;
