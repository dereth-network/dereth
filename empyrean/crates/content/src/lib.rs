//! The world content database: the World side of ACE's `Source/ACE.Database` over the read-only,
//! memory-mapped `world.pack`, and the importer that builds the pack from ACE's SQL dump.
//!
//! **Depends on** `empyrean-common`, `empyrean-entity` and `empyrean-store`. **Used by** the
//! gameplay crate (`empyrean-world`), the commands (`empyrean-command`), the server
//! (`empyrean-server`), the pack builder (`empyrean-import`) and the test kit (`empyrean-testkit`).
//!
//! **Must never** write the pack it serves: `world.pack` is immutable at run time, and the
//! developer content commands write to the [`overlay`] beside it. Nor does it hold data-file
//! content (terrain, cells, geometry, spells, character creation): the server reads the player's
//! own data files at run time through `empyrean-dat`, as ACE does. Its one `unsafe` is the memory
//! map in [`pack`].
//!
//! * [`WorldDatabase`]: ACE's `DatabaseManager.World` API; [`PackContent`] (the pack) and
//!   [`MemContent`] (an in-memory builder for tests) implement it.
//! * [`models::world`]: ACE's World-DB models, one module per ACE file.
//! * [`adapter::weenie_converter`]: `WeenieConverter.ConvertToEntityWeenie`, into
//!   [`empyrean_entity::Weenie`].
//! * [`import`]: the SQL dump and content files → `world.pack` (the `empyrean-import` binary's
//!   work).
//! * [`export`] and [`gdle`]: ACE's `SQLFormatters` and `Source/ACE.Adapter` (the GDLE and
//!   Lifestoned JSON formats).
//! * [`pack`]: the container (our own format).
//! * [`corrections`]: not ACE; corrections to ACE's stored weenie and spell data, applied on read.

pub mod adapter;
pub mod builders;
pub mod content;
pub mod corrections;
pub mod entity;
pub mod error;
pub mod export;
pub mod extensions;
pub mod gdle;
pub mod import;
pub mod models;
pub mod overlay;
pub mod pack;
pub mod records;
pub mod world_database;
pub mod world_database_with_entity_cache;

pub use content::{MemContent, PackContent, WorldDatabase};
pub use error::{ImportError, PackError};
pub use import::WorldContent;
pub use world_database::WorldDatabaseBase;
pub use world_database_with_entity_cache::WorldDatabaseWithEntityCache;
