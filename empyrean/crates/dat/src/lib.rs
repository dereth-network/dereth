//! ACE's `DatManager` surface (`Source/ACE.DatLoader`) over the shared data-file readers, plus the
//! server-only decoders.
//!
//! **Depends on** `empyrean-common`, `empyrean-entity` and the shared readers it is built on
//! (`dereth-primitives`, `dereth-dat`, `dereth-assets`, `dereth-physics`, `dereth-world-data`).
//! **Used by** the gameplay crate (`empyrean-world`), the commands (`empyrean-command`), the server
//! (`empyrean-server`) and the test kit (`empyrean-testkit`).
//!
//! **Must never** carry a second copy of a decoder the shared crates have, or depend on a
//! client-only (`dereth/`) crate (`cargo xtask separation`): the data files are the player's
//! own, read at run time, and never packed into `world.pack`.
//!
//! - [`DatManager`] opens the files once, reads the tables ACE reads up front and is shared as an
//!   `Arc`. Its databases answer ACE's `ReadFromDat<T>` as [`DatDatabase::read_from_dat`], with a
//!   decoded-object cache behind a `OnceLock` per file id.
//! - [`DatSource`] is where the bytes come from: [`RealDats`] (the retail files) or [`FakeDats`]
//!   (in-memory objects for the unit tier).
//! - [`file_types`] holds ACE's names for the shared decoded types and ports of the helpers ACE
//!   keeps on them.
//! - [`physics`] turns the data files into the shared physics inputs.

pub mod dat_manager;
pub mod database;
pub mod fake;
pub mod file_types;
pub mod physics;
pub mod source;

pub use dat_manager::{
    file_id, CellDatDatabase, DatManager, DatManagerError, LanguageDatDatabase, PortalDatDatabase,
};
pub use database::{DatDatabase, DatDatabaseType, DatFileType, UnpackError};
pub use fake::FakeDats;
pub use file_types::AceThrow;
pub use source::{DatSource, RealDats};
