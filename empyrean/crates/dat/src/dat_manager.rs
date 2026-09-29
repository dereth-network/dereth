// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.DatLoader/DatManager.cs, Source/ACE.DatLoader/PortalDatDatabase.cs, Source/ACE.DatLoader/CellDatDatabase.cs, Source/ACE.DatLoader/LanguageDatDatabase.cs
//! ACE's `DatManager`: the four dat databases, opened once and shared.
//!
//! ACE keeps them in static properties set by `DatManager.Initialize`. Here `initialize` returns an
//! `Arc<DatManager>` that is immutable afterwards; the world holds it and passes it down.

use std::ops::Deref;
use std::sync::Arc;

use crate::database::{DatDatabase, DatDatabaseType, DatFileType};
use crate::file_types::skill_table::SkillTableExt;
use crate::file_types::{
    AceThrow, BadData, CharGen, ChatPoseTable, ContractTable, GeneratorTable, MasterProperty,
    NameFilterTable, RegionDesc, SecondaryAttributeTable, SkillTable, SpellComponentsTable,
    SpellTable, StringTable, TabooTable, XpTable,
};
use crate::source::DatSource;

/// The `FILE_ID`s of the tables ACE's `PortalDatDatabase` and `LanguageDatDatabase` load up front.
pub mod file_id {
    pub const BAD_DATA: u32 = 0x0E00_001A;
    pub const CHAT_POSE_TABLE: u32 = 0x0E00_0007;
    pub const CHAR_GEN: u32 = 0x0E00_0002;
    pub const CONTRACT_TABLE: u32 = 0x0E00_001D;
    pub const GENERATOR_TABLE: u32 = 0x0E00_000D;
    pub const MASTER_PROPERTY: u32 = 0x3900_0001;
    pub const NAME_FILTER_TABLE: u32 = 0x0E00_0020;
    pub const REGION_DESC: u32 = 0x1300_0000;
    pub const SECONDARY_ATTRIBUTE_TABLE: u32 = 0x0E00_0003;
    pub const SKILL_TABLE: u32 = 0x0E00_0004;
    pub const SPELL_COMPONENTS_TABLE: u32 = 0x0E00_000F;
    pub const SPELL_TABLE: u32 = 0x0E00_000E;
    pub const TABOO_TABLE: u32 = 0x0E00_001E;
    pub const XP_TABLE: u32 = 0x0E00_0018;
    /// `StringTable.CharacterTitle_FileID`, in the language dat.
    pub const CHARACTER_TITLES: u32 = 0x2300_000E;
}

/// End-of-retail iterations, which `Initialize` warns about when a dat differs.
pub const ITERATION_CELL: i32 = 982;
pub const ITERATION_PORTAL: i32 = 2072;
pub const ITERATION_HIRES: i32 = 497;
pub const ITERATION_LANGUAGE: i32 = 994;

/// Why the dats could not be brought up.
#[derive(Debug, thiserror::Error)]
pub enum DatManagerError {
    #[error("the dat source has no {0:?} database ({1})")]
    MissingDatabase(DatDatabaseType, String),
    #[error("SkillTable.AddRetiredSkills: {0}")]
    RetiredSkills(AceThrow),
}

/// A table loaded up front, answering a clear panic when a test's [`crate::FakeDats`] left it out.
fn required<'a, T>(t: Option<&'a Arc<T>>, name: &str, id: u32) -> &'a Arc<T> {
    t.unwrap_or_else(|| {
        panic!(
            "the dats have no {name} (0x{id:08X}): the retail dat always has it, so this is a \
             FakeDats that did not insert one"
        )
    })
}

macro_rules! portal_tables {
    ($( $field:ident, $try_get:ident: $ty:ty = $id:ident, $name:literal; )*) => {
        /// ACE's `PortalDatDatabase`: the portal dat plus the tables ACE reads at load.
        ///
        /// Each table has two accessors: the plain one, which ACE-shaped call sites use and which
        /// panics if the table is absent (only possible with a `FakeDats` that did not insert it),
        /// and a `try_` one answering `None`.
        #[derive(Debug)]
        pub struct PortalDatDatabase {
            base: DatDatabase,
            $( $field: Option<Arc<$ty>>, )*
        }

        impl PortalDatDatabase {
            $(
                #[must_use]
                pub fn $field(&self) -> &$ty {
                    required(self.$field.as_ref(), $name, file_id::$id)
                }

                #[must_use]
                pub fn $try_get(&self) -> Option<&Arc<$ty>> {
                    self.$field.as_ref()
                }
            )*

            /// The up-front tables that are absent, by ACE name.
            #[must_use]
            pub fn missing_tables(&self) -> Vec<&'static str> {
                let mut v = Vec::new();
                $( if self.$field.is_none() { v.push($name); } )*
                v
            }
        }
    };
}

portal_tables! {
    bad_data, try_bad_data: BadData = BAD_DATA, "BadData";
    chat_pose_table, try_chat_pose_table: ChatPoseTable = CHAT_POSE_TABLE, "ChatPoseTable";
    char_gen, try_char_gen: CharGen = CHAR_GEN, "CharGen";
    contract_table, try_contract_table: ContractTable = CONTRACT_TABLE, "ContractTable";
    generator_table, try_generator_table: GeneratorTable = GENERATOR_TABLE, "GeneratorTable";
    master_property, try_master_property: MasterProperty = MASTER_PROPERTY, "MasterProperty";
    name_filter_table, try_name_filter_table: NameFilterTable = NAME_FILTER_TABLE, "NameFilterTable";
    region_desc, try_region_desc: RegionDesc = REGION_DESC, "RegionDesc";
    secondary_attribute_table, try_secondary_attribute_table: SecondaryAttributeTable = SECONDARY_ATTRIBUTE_TABLE, "SecondaryAttributeTable";
    skill_table, try_skill_table: SkillTable = SKILL_TABLE, "SkillTable";
    spell_components_table, try_spell_components_table: SpellComponentsTable = SPELL_COMPONENTS_TABLE, "SpellComponentsTable";
    spell_table, try_spell_table: SpellTable = SPELL_TABLE, "SpellTable";
    taboo_table, try_taboo_table: TabooTable = TABOO_TABLE, "TabooTable";
    xp_table, try_xp_table: XpTable = XP_TABLE, "XpTable";
}

impl PortalDatDatabase {
    /// Read every up-front table, in ACE's order. The skill table is read uncached, has the
    /// retired skills added (ACE's `DatManager.Initialize` does that to the cached instance), and
    /// is then cached, so `read_from_dat::<SkillTable>` answers the same edited table.
    // ACE: PortalDatDatabase.PortalDatDatabase
    fn new(base: DatDatabase) -> Result<Self, DatManagerError> {
        fn read<T: DatFileType>(base: &DatDatabase, id: u32) -> Option<Arc<T>> {
            base.read_from_dat::<T>(id)
        }
        let bad_data = read(&base, file_id::BAD_DATA);
        let chat_pose_table = read(&base, file_id::CHAT_POSE_TABLE);
        let char_gen = read(&base, file_id::CHAR_GEN);
        let contract_table = read(&base, file_id::CONTRACT_TABLE);
        let generator_table = read(&base, file_id::GENERATOR_TABLE);
        let master_property = read(&base, file_id::MASTER_PROPERTY);
        let name_filter_table = read(&base, file_id::NAME_FILTER_TABLE);
        let region_desc = read(&base, file_id::REGION_DESC);
        let secondary_attribute_table = read(&base, file_id::SECONDARY_ATTRIBUTE_TABLE);
        let skill_table = match base.unpack_uncached::<SkillTable>(file_id::SKILL_TABLE) {
            Some(mut t) => {
                // DatManager.Initialize: PortalDat.SkillTable.AddRetiredSkills();
                t.add_retired_skills()
                    .map_err(DatManagerError::RetiredSkills)?;
                let t = Arc::new(t);
                base.insert_cache(file_id::SKILL_TABLE, Arc::clone(&t));
                Some(t)
            }
            None => None,
        };
        let spell_components_table = read(&base, file_id::SPELL_COMPONENTS_TABLE);
        let spell_table = read(&base, file_id::SPELL_TABLE);
        let taboo_table = read(&base, file_id::TABOO_TABLE);
        let xp_table = read(&base, file_id::XP_TABLE);
        Ok(Self {
            base,
            bad_data,
            chat_pose_table,
            char_gen,
            contract_table,
            generator_table,
            master_property,
            name_filter_table,
            region_desc,
            secondary_attribute_table,
            skill_table,
            spell_components_table,
            spell_table,
            taboo_table,
            xp_table,
        })
    }
}

impl Deref for PortalDatDatabase {
    type Target = DatDatabase;
    fn deref(&self) -> &DatDatabase {
        &self.base
    }
}

/// ACE's `CellDatDatabase`: landblocks (`0xXXYYFFFF`), landblock infos (`0xXXYYFFFE`) and
/// environment cells (`0xXXYY0100` up).
#[derive(Debug)]
pub struct CellDatDatabase {
    base: DatDatabase,
}

impl CellDatDatabase {
    // ACE: CellDatDatabase.CellDatDatabase
    fn new(base: DatDatabase) -> Self {
        Self { base }
    }

    /// ACE's `ExtractLandblockContents` writes every cell file to disk for the developer
    /// `export-cell-dat` command. Not ported: it belongs with that command.
    // ACE: CellDatDatabase.ExtractLandblockContents
    pub fn extract_landblock_contents(&self, _path: &str) {
        empyrean_common::not_ported!("ACE: CellDatDatabase.ExtractLandblockContents");
    }
}

impl Deref for CellDatDatabase {
    type Target = DatDatabase;
    fn deref(&self) -> &DatDatabase {
        &self.base
    }
}

/// ACE's `LanguageDatDatabase`: the English string tables, with the character titles up front.
#[derive(Debug)]
pub struct LanguageDatDatabase {
    base: DatDatabase,
    character_titles: Option<Arc<StringTable>>,
}

impl LanguageDatDatabase {
    // ACE: LanguageDatDatabase.LanguageDatDatabase
    fn new(base: DatDatabase) -> Self {
        let character_titles = base.read_from_dat::<StringTable>(file_id::CHARACTER_TITLES);
        Self {
            base,
            character_titles,
        }
    }

    /// The character-title strings (`0x2300000E`). Panics if absent, as the portal tables do.
    #[must_use]
    pub fn character_titles(&self) -> &StringTable {
        required(
            self.character_titles.as_ref(),
            "CharacterTitles",
            file_id::CHARACTER_TITLES,
        )
    }

    #[must_use]
    pub fn try_character_titles(&self) -> Option<&Arc<StringTable>> {
        self.character_titles.as_ref()
    }
}

impl Deref for LanguageDatDatabase {
    type Target = DatDatabase;
    fn deref(&self) -> &DatDatabase {
        &self.base
    }
}

/// ACE's `DatManager`: the four databases.
#[derive(Debug)]
pub struct DatManager {
    cell_dat: CellDatDatabase,
    portal_dat: PortalDatDatabase,
    high_res_dat: Option<DatDatabase>,
    language_dat: LanguageDatDatabase,
}

impl DatManager {
    /// Open the dats from `source` and read the up-front tables, logging what ACE logs.
    ///
    /// ACE's `keepOpen` has no counterpart (a file stays open for the life of its source) and its
    /// `loadCell = false` is not offered: the server always loads the cell dat.
    ///
    /// # Errors
    ///
    /// The source lacks the portal, cell or language dat (ACE logs and runs on with a null
    /// database, failing at first use; this fails at start), or the skill table already holds a
    /// retired skill (ACE throws out of `Initialize`).
    // ACE: DatManager.Initialize
    pub fn initialize(source: Arc<dyn DatSource>) -> Result<Arc<Self>, DatManagerError> {
        for db in [
            DatDatabaseType::Cell,
            DatDatabaseType::Portal,
            DatDatabaseType::Language,
        ] {
            if !source.has_database(db) {
                // DIVERGE: ACE logs a FileNotFoundException here and continues with a null
                // database; a server without its dats cannot run, so this refuses to start.
                return Err(DatManagerError::MissingDatabase(db, source.describe(db)));
            }
        }

        let cell_dat =
            CellDatDatabase::new(DatDatabase::new(DatDatabaseType::Cell, Arc::clone(&source)));
        log_opened(&cell_dat, ITERATION_CELL);

        let portal_dat = PortalDatDatabase::new(DatDatabase::new(
            DatDatabaseType::Portal,
            Arc::clone(&source),
        ))?;
        log_opened(&portal_dat, ITERATION_PORTAL);
        for name in portal_dat.missing_tables() {
            log::error!("{} has no {name}", portal_dat.file_path());
        }

        // Load the client_highres.dat file. This is not required for ACE operation.
        let high_res_dat = source
            .has_database(DatDatabaseType::HighRes)
            .then(|| DatDatabase::new(DatDatabaseType::HighRes, Arc::clone(&source)));
        if let Some(hi) = &high_res_dat {
            log_opened(hi, ITERATION_HIRES);
        }

        let language_dat = LanguageDatDatabase::new(DatDatabase::new(
            DatDatabaseType::Language,
            Arc::clone(&source),
        ));
        log_opened(&language_dat, ITERATION_LANGUAGE);

        Ok(Arc::new(Self {
            cell_dat,
            portal_dat,
            high_res_dat,
            language_dat,
        }))
    }

    #[must_use]
    pub fn cell_dat(&self) -> &CellDatDatabase {
        &self.cell_dat
    }

    #[must_use]
    pub fn portal_dat(&self) -> &PortalDatDatabase {
        &self.portal_dat
    }

    /// `None` when `client_highres.dat` is absent (ACE's `HighResDat` is then null).
    #[must_use]
    pub fn high_res_dat(&self) -> Option<&DatDatabase> {
        self.high_res_dat.as_ref()
    }

    #[must_use]
    pub fn language_dat(&self) -> &LanguageDatDatabase {
        &self.language_dat
    }
}

fn log_opened(db: &DatDatabase, expected: i32) {
    let count = db.all_files_count();
    let iteration = db.iteration();
    log::info!(
        "Successfully opened {} file, containing {count} records, iteration {iteration}",
        db.file_path()
    );
    if iteration != expected {
        log::warn!(
            "{} iteration does not match expected end-of-retail version of {expected}.",
            db.file_path()
        );
    }
}
