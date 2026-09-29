// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.DatLoader/FileTypes/Iteration.cs
//! ACE's `ACE.DatLoader.FileTypes`, as the server sees them.
//!
//! The decoded shapes are the shared `dereth-assets` types, re-exported here under ACE's names where
//! the names differ ([`SetupModel`], [`RegionDesc`], ...). What ACE keeps *on* those types — the
//! helpers the server calls, such as `TabooTable.ContainsBadWord` or `SpellTable.GetSpellFormula` —
//! is ported in the submodules as extension traits and free functions. The one decoder `dereth-assets`
//! does not have, ACE's [`Iteration`], is ported here.

use dereth_assets as assets;
use dereth_dat::Cursor;

use crate::database::{DatFileType, UnpackError};

pub mod cell_landblock;
pub mod clothing_table;
pub mod combat_maneuver_table;
pub mod palette_set;
pub mod sex_cg;
pub mod skill_table;
pub mod spell_table;
pub mod taboo_table;

pub use assets::{
    Animation, BadData, CellLandblock, CharGen, ChatPoseTable, ClothingTable, CombatManeuverTable,
    ContractTable, DualDidMapper, EnumMapper, EnvCell, Environment, GfxObj, GfxObjDegradeInfo,
    LandblockInfo, MasterProperty, MotionTable, NameFilterTable, PaletteSet, QualityFilter, Scene,
    SkillTable, SpellTable, StringTable, TabooTable, Wave, XpTable,
};

/// ACE's `SetupModel` (`0x02`).
pub type SetupModel = assets::Setup;
/// ACE's `RegionDesc` (`0x13`).
pub type RegionDesc = assets::Region;
/// ACE's `SecondaryAttributeTable` (`0x0E000003`).
pub type SecondaryAttributeTable = assets::Attribute2ndTable;
/// ACE's `SpellComponentsTable` (`0x0E00000F`).
pub type SpellComponentsTable = assets::SpellComponentTable;
/// ACE's `GeneratorTable` (`0x0E00000D`, the object hierarchy). ACE's tree of `Generator` nodes is
/// the shared decoder's arena of nodes; node 0 is the root and its two children are ACE's
/// `PlayDayItems` and `WeenieObjectsItems`.
pub type GeneratorTable = assets::ObjectHierarchy;
/// ACE's `Texture` (`0x06`).
pub type Texture = assets::RenderSurface;

/// Where an ACE helper would have thrown, the port answers `Err` with the exception it would have
/// been, and the caller ports ACE's handling of it (usually: none, so the request fails).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AceThrow {
    /// `KeyNotFoundException` from a dictionary indexer.
    #[error("KeyNotFoundException: {0}")]
    KeyNotFound(u32),
    /// `ArgumentOutOfRangeException` from a list indexer (or `OverflowException` from
    /// `Convert.ToInt32` of an index beyond `int.MaxValue`).
    #[error("ArgumentOutOfRangeException: {0}")]
    IndexOutOfRange(u64),
    /// `DivideByZeroException` from unsigned integer division.
    #[error("DivideByZeroException")]
    DivideByZero,
    /// `ArgumentException` from `Dictionary.Add` of a key already present.
    #[error("ArgumentException: an item with the same key has already been added: {0}")]
    DuplicateKey(u32),
}

/// ACE's `Iteration`: a dat's version record, file `0xFFFF0001` in every dat.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Iteration {
    pub total_iterations: i32,
    /// ACE's `Ints`: `startingIteration -> consecutiveIterations`, in file order.
    pub ints: Vec<(i32, i32)>,
}

impl Iteration {
    /// ACE's `Iteration.FILE_ID`.
    pub const FILE_ID: u32 = 0xFFFF_0001;
}

impl DatFileType for Iteration {
    // ACE: Iteration.Unpack
    fn unpack(_file_id: u32, bytes: &[u8]) -> Result<Self, UnpackError> {
        let mut reader = Cursor::new(bytes);
        let total_iterations = reader.i32()?;
        let mut iteration_count = total_iterations;
        let mut ints = Vec::new();
        while iteration_count > 0 {
            let consecutive_iterations = reader.i32()?;
            let starting_iteration = reader.i32()?;
            if ints.iter().any(|(s, _)| *s == starting_iteration) {
                return Err(UnpackError::Server(format!(
                    "Iteration: duplicate starting iteration {starting_iteration}"
                )));
            }
            ints.push((starting_iteration, consecutive_iterations));
            iteration_count = iteration_count.wrapping_add(consecutive_iterations);
        }
        Ok(Self {
            total_iterations,
            ints,
        })
    }
}
