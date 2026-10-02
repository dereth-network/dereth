//! The generated literal data of ACE.Server's factories: loot tables, house cells, weenie class
//! names and the table logic.
//!
//! **Depends on** `empyrean-common` and `empyrean-entity`. **Used by** the gameplay crate
//! (`empyrean-world`) and the commands (`empyrean-command`).
//!
//! **Must never** be edited by hand outside [`logic`] and [`entity`]: the rest is generated from
//! ACE's source, and a check fails when the committed output is stale. It is kept apart so the
//! gameplay crate never recompiles it.
//!
//! - [`enums`]: ACE.Server's `Factories/Enum` enums (generated), `WeenieClassName` among them.
//! - [`tables`]: `Factories/Tables/**` literal data (generated), one module per C# file.
//! - [`house_cell`]: `Entity/HouseCell.cs` (generated).
//! - [`logic`]: the methods of the table classes, hand-ported.
//! - [`entity`]: `ChanceTable` and `GemResult`, the element types the tables are made of.
//! - [`era`]: the loot tables of the earlier eras, generated from ClassicACE's source, and
//!   `logic::era` rolls them.

pub mod entity;
pub mod enums;
pub mod era;
#[rustfmt::skip]
pub mod house_cell;
pub mod logic;
pub mod rng;
pub mod static_map;
pub mod tables;
