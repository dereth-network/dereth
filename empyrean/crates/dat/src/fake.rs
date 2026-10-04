//! `FakeDats`: an in-memory dat set for the unit and scenario tiers.
//!
//! A test inserts decoded objects directly (or raw bytes, to exercise a decoder), then builds a
//! [`DatManager`] from them exactly as the server builds one from the retail files, so everything
//! above the source — the cache, the up-front tables, `AddRetiredSkills` — runs unchanged.
//! [`sample`] has small synthetic objects to start from. Nothing here is ported from ACE.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_primitives::LandblockId;

use crate::dat_manager::{file_id, DatManager, DatManagerError};
use crate::database::{CachedObject, DatDatabaseType, DatFileType};
use crate::file_types::{
    CharGen, Iteration, SkillTable, SpellComponentsTable, SpellTable, XpTable,
};
use crate::source::DatSource;

#[derive(Default)]
struct FakeDb {
    decoded: BTreeMap<u32, CachedObject>,
    raw: BTreeMap<u32, Vec<u8>>,
    /// Directory-entry iterations set by `with_file_iteration`; any other file is at iteration 1.
    iterations: BTreeMap<u32, u32>,
}

/// An in-memory [`DatSource`]. Portal, cell and language databases always exist (empty unless
/// filled); the high-res one exists once something is inserted into it.
pub struct FakeDats {
    dbs: BTreeMap<DatDatabaseType, FakeDb>,
}

impl std::fmt::Debug for FakeDats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("FakeDats");
        for (db, v) in &self.dbs {
            d.field(&format!("{db:?}"), &(v.decoded.len() + v.raw.len()));
        }
        d.finish()
    }
}

impl Default for FakeDats {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeDats {
    #[must_use]
    pub fn new() -> Self {
        let mut dbs = BTreeMap::new();
        for db in [
            DatDatabaseType::Portal,
            DatDatabaseType::Cell,
            DatDatabaseType::Language,
        ] {
            dbs.insert(db, FakeDb::default());
        }
        Self { dbs }
    }

    /// Insert a decoded object under `id` in `db`.
    #[must_use]
    pub fn with<T: DatFileType>(mut self, db: DatDatabaseType, id: u32, obj: T) -> Self {
        self.dbs
            .entry(db)
            .or_default()
            .decoded
            .insert(id, Arc::new(obj) as Arc<dyn Any + Send + Sync>);
        self
    }

    /// Insert raw payload bytes under `id` in `db`; they are decoded on first read, as a retail
    /// file is.
    #[must_use]
    pub fn with_raw(mut self, db: DatDatabaseType, id: u32, bytes: Vec<u8>) -> Self {
        self.dbs.entry(db).or_default().raw.insert(id, bytes);
        self
    }

    /// Set the directory-entry iteration of `id` in `db` (ACE's `DatFile.Iteration`); a file
    /// without one is at iteration 1.
    #[must_use]
    pub fn with_file_iteration(mut self, db: DatDatabaseType, id: u32, iteration: u32) -> Self {
        self.dbs
            .entry(db)
            .or_default()
            .iterations
            .insert(id, iteration);
        self
    }

    #[must_use]
    pub fn with_portal<T: DatFileType>(self, id: u32, obj: T) -> Self {
        self.with(DatDatabaseType::Portal, id, obj)
    }

    #[must_use]
    pub fn with_cell<T: DatFileType>(self, id: u32, obj: T) -> Self {
        self.with(DatDatabaseType::Cell, id, obj)
    }

    #[must_use]
    pub fn with_language<T: DatFileType>(self, id: u32, obj: T) -> Self {
        self.with(DatDatabaseType::Language, id, obj)
    }

    /// Give `db` an iteration record (ACE's `Iteration`, file `0xFFFF0001`).
    #[must_use]
    pub fn with_iteration(self, db: DatDatabaseType, total: i32) -> Self {
        self.with(
            db,
            Iteration::FILE_ID,
            Iteration {
                total_iterations: total,
                ints: vec![(1, -total)],
            },
        )
    }

    #[must_use]
    pub fn with_xp_table(self, t: XpTable) -> Self {
        self.with_portal(file_id::XP_TABLE, t)
    }

    #[must_use]
    pub fn with_char_gen(self, t: CharGen) -> Self {
        self.with_portal(file_id::CHAR_GEN, t)
    }

    #[must_use]
    pub fn with_skill_table(self, t: SkillTable) -> Self {
        self.with_portal(file_id::SKILL_TABLE, t)
    }

    #[must_use]
    pub fn with_spell_table(self, t: SpellTable) -> Self {
        self.with_portal(file_id::SPELL_TABLE, t)
    }

    #[must_use]
    pub fn with_spell_components_table(self, t: SpellComponentsTable) -> Self {
        self.with_portal(file_id::SPELL_COMPONENTS_TABLE, t)
    }

    /// A landblock whose 81 vertices all stand at height-table index `height_index`, with no
    /// `LandblockInfo`.
    #[must_use]
    pub fn with_flat_landblock(self, id: LandblockId, height_index: u8) -> Self {
        self.with_cell(id.terrain_id().0, sample::flat_landblock(id, height_index))
    }

    /// Build the `DatManager` over these files.
    ///
    /// # Errors
    ///
    /// As [`DatManager::initialize`].
    pub fn build(self) -> Result<Arc<DatManager>, DatManagerError> {
        DatManager::initialize(Arc::new(self))
    }
}

impl DatSource for FakeDats {
    fn has_database(&self, db: DatDatabaseType) -> bool {
        self.dbs.contains_key(&db)
    }

    fn file_ids(&self, db: DatDatabaseType) -> Vec<u32> {
        let Some(d) = self.dbs.get(&db) else {
            return Vec::new();
        };
        let mut ids: Vec<u32> = d.decoded.keys().chain(d.raw.keys()).copied().collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn contains(&self, db: DatDatabaseType, id: u32) -> bool {
        self.dbs
            .get(&db)
            .is_some_and(|d| d.decoded.contains_key(&id) || d.raw.contains_key(&id))
    }

    fn read(&self, db: DatDatabaseType, id: u32) -> Option<Vec<u8>> {
        self.dbs.get(&db)?.raw.get(&id).cloned()
    }

    fn file_iteration(&self, db: DatDatabaseType, id: u32) -> Option<u32> {
        if !self.contains(db, id) {
            return None;
        }
        Some(self.dbs.get(&db)?.iterations.get(&id).copied().unwrap_or(1))
    }

    fn decoded(&self, db: DatDatabaseType, id: u32) -> Option<CachedObject> {
        self.dbs.get(&db)?.decoded.get(&id).cloned()
    }

    fn describe(&self, db: DatDatabaseType) -> String {
        format!("FakeDats({})", db.file_name())
    }
}

/// Small synthetic dat objects for tests. Values are made up; only their shape is retail's.
pub mod sample {
    use std::collections::BTreeMap;

    use dereth_assets::tables::{
        HeritageGroup, ObjDesc, SexCg, SkillBase, SkillFormula, SpellBase, SpellComponent,
        StarterArea,
    };
    use dereth_physics::land::VERTEX_COUNT;
    use dereth_primitives::{CellId, DataId, Frame, LandblockId, Position, Quat, Vec3};

    use crate::dat_manager::file_id;
    use crate::file_types::spell_table::compute_hash;
    use crate::file_types::{
        CellLandblock, CharGen, SkillTable, SpellComponentsTable, SpellTable, XpTable,
    };

    /// A four-level XP table: level 2 at 1,000 XP, level 3 at 2,500.
    #[must_use]
    pub fn xp_table() -> XpTable {
        XpTable {
            id: DataId(file_id::XP_TABLE),
            attribute_xp: vec![0, 110, 277],
            vital_xp: vec![0, 73, 183],
            trained_xp: vec![0, 55, 138],
            specialized_xp: vec![0, 40, 100],
            level_xp: vec![0, 0, 1_000, 2_500],
            level_credits: vec![0, 0, 1, 1],
        }
    }

    fn empty_objdesc() -> ObjDesc {
        ObjDesc {
            version: 0x11,
            palette: None,
            subpalettes: Vec::new(),
            texture_changes: Vec::new(),
            anim_part_changes: Vec::new(),
        }
    }

    /// The Holtburg outdoor cell this sample's starting area uses.
    pub const START_CELL: CellId = CellId(0xA9B4_0019);

    /// A CharGen with one starting area (one location in [`START_CELL`]) and one heritage
    /// (key 1, "Aluvian") with one sex (key 1, "Male", setup `0x02000001`).
    #[must_use]
    pub fn char_gen() -> CharGen {
        let sex = SexCg {
            naming_help: None,
            name: "Male".into(),
            scale: 100,
            setup: DataId(0x0200_0001),
            sound_table: DataId(0x2000_0001),
            icon: 0x0600_0001,
            base_palette: DataId(0x0400_007E),
            skin_palset: DataId(0x0F00_0001),
            physics_table: DataId(0x3400_0004),
            motion_table: DataId(0x0900_0001),
            combat_table: DataId(0x3000_0000),
            base_objdesc: empty_objdesc(),
            hair_colors: vec![0x0F00_0002],
            hair_styles: Vec::new(),
            eye_colors: vec![0x0F00_0003],
            eye_strips: Vec::new(),
            nose_strips: Vec::new(),
            mouth_strips: Vec::new(),
            headgear: Vec::new(),
            shirts: Vec::new(),
            pants: Vec::new(),
            footwear: Vec::new(),
            clothing_colors: vec![0x0F00_0004],
        };
        let heritage = HeritageGroup {
            description: None,
            sex_order: vec![1],
            template_presentations: std::collections::BTreeMap::new(),
            name: "Aluvian".into(),
            icon: 0x0600_0002,
            setup: DataId(0x0200_0001),
            environment_setup: DataId(0x0200_0002),
            attribute_credits: 330,
            skill_credits: 50,
            primary_start_areas: vec![0],
            secondary_start_areas: Vec::new(),
            skills: vec![(6, 10, 20)],
            templates: Vec::new(),
            sex_table_marker: 0,
            sexes: BTreeMap::from([(1, sex)]),
        };
        let start = Position::new(
            START_CELL,
            Frame::new(Vec3::new(84.0, 7.1, 94.005), Quat::IDENTITY),
        );
        CharGen {
            heritage_order: vec![1],
            help_strings: Vec::new(),
            id: DataId(file_id::CHAR_GEN),
            second_data_id: DataId(0),
            starter_areas: vec![StarterArea {
                name: "Holtburg".into(),
                locations: vec![start],
            }],
            hg_table_marker: 0,
            heritage_groups: BTreeMap::from([(1, heritage)]),
        }
    }

    /// A skill table with Melee Defense (6) only.
    #[must_use]
    pub fn skill_table() -> SkillTable {
        let melee_defense = SkillBase {
            description: "Helps you evade melee attacks.".into(),
            name: "Melee Defense".into(),
            icon: 0x0600_0003,
            trained_cost: 10,
            specialized_cost: 20,
            category: 1,
            chargen_use: 1,
            min_level: 1,
            formula: SkillFormula {
                w: 0,
                x: 1,
                y: 0,
                z: 3,
                attr1: 4,
                attr2: 3,
            },
            upper_bound: 0.0,
            lower_bound: 0.0,
            learn_mod: 0.0,
        };
        SkillTable {
            id: DataId(file_id::SKILL_TABLE),
            buckets: 64,
            skills: BTreeMap::from([(6, melee_defense)]),
        }
    }

    fn component(name: &str, component_type: u32, text: &str) -> SpellComponent {
        SpellComponent {
            name: name.into(),
            category: 0,
            icon: 0,
            component_type,
            gesture: 0,
            time: 0.0,
            text: text.into(),
            cdm: 0.0,
        }
    }

    /// A component table with one scarab, herb, powder, potion and talisman.
    #[must_use]
    pub fn spell_components_table() -> SpellComponentsTable {
        SpellComponentsTable {
            id: DataId(file_id::SPELL_COMPONENTS_TABLE),
            buckets: 256,
            components: BTreeMap::from([
                (1, component("Lead Scarab", 1, "")),
                (10, component("Hyssop", 2, "Zojak")),
                (20, component("Powdered Agate", 3, "Tugak")),
                (30, component("Stibnite", 4, "Quaril")),
                (40, component("Poplar Talisman", 5, "")),
            ]),
        }
    }

    /// The name of [`spell_table`]'s one spell, id 1.
    pub const SPELL_NAME: &str = "Test Spell";
    /// Its description (the other half of the formula key).
    pub const SPELL_DESC: &str = "A spell for tests.";
    /// Its decrypted formula: scarab, herb, powder, potion, talisman.
    pub const SPELL_FORMULA: [u32; 5] = [1, 10, 20, 30, 40];

    /// A spell table with one spell, id 1, whose raw component slots encrypt [`SPELL_FORMULA`]
    /// under the name/description key.
    #[must_use]
    pub fn spell_table() -> SpellTable {
        let key = (compute_hash(SPELL_NAME) % 0x1210_7680)
            .wrapping_add(compute_hash(SPELL_DESC) % 0xBEAD_CF45);
        let mut raw_comps = [0u32; 8];
        for (slot, c) in raw_comps.iter_mut().zip(SPELL_FORMULA) {
            *slot = c.wrapping_add(key);
        }
        let spell = SpellBase {
            name: SPELL_NAME.into(),
            description: SPELL_DESC.into(),
            school: 4,
            icon: 0x0600_0004,
            category: 1,
            bitfield: 0,
            base_mana: 5,
            base_range_constant: 0.0,
            base_range_mod: 0.0,
            power: 1,
            spell_economy_mod: 1.0,
            formula_version: 0,
            component_loss: 0.0,
            meta_spell_type: 1,
            meta_spell_id: 1,
            duration: Some((1800.0, 0.0, 0.0)),
            portal_lifetime: None,
            raw_comps,
            comp_key: key,
            comps: SPELL_FORMULA.to_vec(),
            caster_effect: 0,
            target_effect: 0,
            fizzle_effect: 0,
            recovery_interval: 0.0,
            recovery_amount: 0.0,
            display_order: 1,
            non_component_target_type: 0,
            mana_mod: 0,
        };
        SpellTable {
            id: DataId(file_id::SPELL_TABLE),
            spell_buckets: 64,
            spells: BTreeMap::from([(1, spell)]),
            spellset_bucket_index: 1,
            spellsets: BTreeMap::new(),
        }
    }

    /// A flat landblock at height-table index `height_index`, with no `LandblockInfo`.
    #[must_use]
    pub fn flat_landblock(id: LandblockId, height_index: u8) -> CellLandblock {
        CellLandblock {
            id: id.terrain_id(),
            lbi_exists: 0,
            terrain: [0; VERTEX_COUNT],
            height: [height_index; VERTEX_COUNT],
        }
    }
}
