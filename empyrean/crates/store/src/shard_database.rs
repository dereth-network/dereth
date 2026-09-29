// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/ShardDatabase.cs
//! `ShardDatabase`: ACE's shard queries and saves, over a storage backend.
//!
//! The trait has two parts:
//! * **backend primitives** (not ACE): row reads and writes that [`crate::SqliteShard`] and
//!   [`crate::MemShard`] implement, returning rows in the order MySQL/InnoDB returns them for ACE's
//!   queries (primary-key or index order, as noted on each [`BiotaQuery`]);
//! * **ACE's public methods** as provided methods, in ACE's order and control flow, calling
//!   [`ShardDatabase::get_biota`], [`ShardDatabase::save_biota`] and [`ShardDatabase::remove_biota`]
//!   through `self` so that [`crate::ShardDatabaseWithCaching`] overrides them as ACE's subclass
//!   does.
//!
//! DIVERGE (arch):
//! * There is no `ShardDbContext`: a save loads (or builds) the row model, updates it with
//!   `BiotaUpdater`, and [`ShardDatabase::write_biota`] writes it whole, in one savepoint. The rows
//!   written are the ones Entity Framework would leave in the table.
//! * The `ReaderWriterLockSlim` parameters are gone: callers pass owned snapshots.
//! * `...InParallel` methods run their items in order on the calling thread (ACE's
//!   `Parallel.ForEach` and `ConcurrentBag` give an unspecified order).
//! * Reads that fail in the backend (I/O, corruption) panic, as ACE's unhandled EF exceptions throw;
//!   the database thread catches that per job, as ACE's `DoWork` does.
//! * ACE keeps a `ConditionalWeakTable` of the `Character` objects it handed out and saves them
//!   through their original context. Here `save_character` writes the character row and all its
//!   property rows (an upsert), so saving a stub (`get_character_stub_by_*`, no properties loaded)
//!   would replace its properties; ACE would fail that save with a duplicate key instead.

use std::fmt;

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{PropertyInstanceId, WeenieType};
use empyrean_entity::ObjectGuid;

use crate::adapter::{BiotaConverter, BiotaUpdater};
use crate::entity::PossessedBiotas;
use crate::error::StoreError;
use crate::models::shard::{Biota, Character};

// ACE: ShardDatabase.PopulatedCollectionFlags
/// Which child collections of a biota have rows (`[Flags]` enum; stored in
/// `biota.populated_Collection_Flags` so a load can skip empty tables).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct PopulatedCollectionFlags(pub u32);

#[allow(missing_docs)]
impl PopulatedCollectionFlags {
    pub const BIOTA_PROPERTIES_ANIM_PART: Self = Self(0x1);
    pub const BIOTA_PROPERTIES_ATTRIBUTE: Self = Self(0x2);
    pub const BIOTA_PROPERTIES_ATTRIBUTE_2ND: Self = Self(0x4);
    pub const BIOTA_PROPERTIES_BODY_PART: Self = Self(0x8);
    pub const BIOTA_PROPERTIES_BOOK: Self = Self(0x10);
    pub const BIOTA_PROPERTIES_BOOK_PAGE_DATA: Self = Self(0x20);
    pub const BIOTA_PROPERTIES_BOOL: Self = Self(0x40);
    pub const BIOTA_PROPERTIES_CREATE_LIST: Self = Self(0x80);
    pub const BIOTA_PROPERTIES_DID: Self = Self(0x100);
    pub const BIOTA_PROPERTIES_EMOTE: Self = Self(0x200);
    pub const BIOTA_PROPERTIES_ENCHANTMENT_REGISTRY: Self = Self(0x400);
    pub const BIOTA_PROPERTIES_EVENT_FILTER: Self = Self(0x800);
    pub const BIOTA_PROPERTIES_FLOAT: Self = Self(0x1000);
    pub const BIOTA_PROPERTIES_GENERATOR: Self = Self(0x2000);
    pub const BIOTA_PROPERTIES_IID: Self = Self(0x4000);
    pub const BIOTA_PROPERTIES_INT: Self = Self(0x8000);
    pub const BIOTA_PROPERTIES_INT64: Self = Self(0x10000);
    pub const BIOTA_PROPERTIES_PALETTE: Self = Self(0x20000);
    pub const BIOTA_PROPERTIES_POSITION: Self = Self(0x40000);
    pub const BIOTA_PROPERTIES_SKILL: Self = Self(0x80000);
    pub const BIOTA_PROPERTIES_SPELL_BOOK: Self = Self(0x100000);
    pub const BIOTA_PROPERTIES_STRING: Self = Self(0x200000);
    pub const BIOTA_PROPERTIES_TEXTURE_MAP: Self = Self(0x400000);
    pub const HOUSE_PERMISSION: Self = Self(0x800000);
    pub const BIOTA_PROPERTIES_ALLEGIANCE: Self = Self(0x1000000);

    /// `Enum.HasFlag`.
    #[must_use]
    pub const fn has_flag(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }
}

impl std::ops::BitOrAssign for PopulatedCollectionFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl fmt::Debug for PopulatedCollectionFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PopulatedCollectionFlags(0x{:X})", self.0)
    }
}

/// A biota-id query (backend primitive). Each variant is one ACE LINQ query; the ids come back in
/// the order MySQL returns them for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiotaQuery {
    /// `biota.id` in `min..=max`, ascending (primary key).
    IdRange { min: u32, max: u32 },
    /// `biota.weenie_Class_Id == wcid`, by id (`biota_wcid_idx`).
    WeenieClassId(u32),
    /// `biota.weenie_Type == type`, by id (`biota_type_idx`).
    WeenieType(i32),
    /// `object_Id` of the IID rows with this type and value, by object id (`type_value_idx`).
    InstanceId { r#type: u16, value: u32 },
    /// `object_Id` of the position rows with `position_Type == 1`, `obj_Cell_Id` in `min..=max` and
    /// `object_Id >= 0x80000000`, by cell then object id (`type_cell_idx`).
    LocationInCellRange { min: u32, max: u32 },
    /// Biotas of weenie type `weenie_type` with an IID row of type `iid_type` (and, when given,
    /// that value), by biota id.
    TypeWithInstanceId {
        weenie_type: i32,
        iid_type: u16,
        iid_value: Option<u32>,
    },
}

/// A character query (backend primitive). Rows come back by `id`; property lists are not loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterQuery<'a> {
    /// `account_Id == account_id && (include_deleted || !is_Deleted)`.
    Account {
        account_id: u32,
        include_deleted: bool,
    },
    /// `id == id && (include_deleted || !is_Deleted)`.
    Id { id: u32, include_deleted: bool },
    /// `name == name` (case-insensitive, as MySQL's collation compares) `&& !is_Deleted`.
    NameNotDeleted(&'a str),
    /// `!is_Deleted && !(delete_Time > 0) && name == name`.
    NameAvailable(&'a str),
    /// `!is_Deleted`.
    NotDeleted,
    /// Every row, deleted ones too (`context.Character`; the offline tools).
    All,
}

/// The two retention times of a [`crate::ShardDatabaseWithCaching`] (`DatabaseManager.Shard.BaseDatabase
/// is ShardDatabaseWithCaching`; the `database-shard-cache-*` commands).
#[derive(Debug)]
pub struct CacheRetentionTimes<'a> {
    /// `PlayerBiotaRetentionTime`.
    pub player_biota_retention_time: &'a mut empyrean_common::dotnet::datetime::TimeSpan,
    /// `NonPlayerBiotaRetentionTime`.
    pub non_player_biota_retention_time: &'a mut empyrean_common::dotnet::datetime::TimeSpan,
}

/// Storage for the shard database: the backend primitives (not ACE) and ACE's `ShardDatabase`
/// methods (provided). See the module documentation.
pub trait ShardDatabase: Send {
    // ---------------------------------------------------------------------------------------------
    // backend primitives
    // ---------------------------------------------------------------------------------------------

    /// The `biota` row alone (no child rows).
    ///
    /// # Errors
    /// A backend failure.
    fn load_biota_row(&mut self, id: u32) -> Result<Option<Biota>, StoreError>;

    /// Loads the child rows of `biota` for every collection named in `flags`, in primary-key or
    /// index order (see the SQLite backend for each table's order).
    ///
    /// # Errors
    /// A backend failure.
    fn load_biota_collections(
        &mut self,
        biota: &mut Biota,
        flags: PopulatedCollectionFlags,
    ) -> Result<(), StoreError>;

    /// Writes `biota` whole (the `biota` row and every child row), atomically. Rows with a
    /// surrogate `id` of 0 are inserted and get the next id, which is written back into `biota`.
    ///
    /// # Errors
    /// A constraint violation or backend failure; nothing is written then.
    fn write_biota(&mut self, biota: &mut Biota) -> Result<(), StoreError>;

    /// Deletes the biota and (by cascade) its child rows.
    ///
    /// # Errors
    /// A backend failure.
    fn delete_biota(&mut self, id: u32) -> Result<(), StoreError>;

    /// The ids matching `query`, in its documented order.
    ///
    /// # Errors
    /// A backend failure.
    fn query_biota_ids(&mut self, query: BiotaQuery) -> Result<Vec<u32>, StoreError>;

    /// `SELECT COUNT(*) FROM biota`.
    ///
    /// # Errors
    /// A backend failure.
    fn count_biotas(&mut self) -> Result<i64, StoreError>;

    /// The `character` rows matching `query`, by id, without property lists.
    ///
    /// # Errors
    /// A backend failure.
    fn query_characters(&mut self, query: CharacterQuery<'_>)
        -> Result<Vec<Character>, StoreError>;

    /// Loads the eight property lists of `character` (replacing what it holds), each in primary
    /// key order.
    ///
    /// # Errors
    /// A backend failure.
    fn load_character_properties(&mut self, character: &mut Character) -> Result<(), StoreError>;

    /// Writes `character` whole (its row, inserted or updated, and all eight property lists;
    /// property rows take the character's id), atomically.
    ///
    /// # Errors
    /// A constraint violation or backend failure; nothing is written then.
    fn write_character(&mut self, character: &Character) -> Result<(), StoreError>;

    /// Starts a transaction that groups the following writes (the database thread's batch).
    ///
    /// # Errors
    /// A backend failure.
    fn begin_batch(&mut self) -> Result<(), StoreError>;

    /// Commits the batch transaction.
    ///
    /// # Errors
    /// A backend failure; the batch is rolled back.
    fn commit_batch(&mut self) -> Result<(), StoreError>;

    /// Abandons the batch transaction (after a failed commit or a panic).
    fn rollback_batch(&mut self);

    /// Deletes the `character` row and (by cascade) its property rows and the allegiance rows that
    /// name it (for the offline tools). The default refuses (a backend without it).
    ///
    /// # Errors
    /// A backend failure, or a backend that cannot delete characters.
    fn delete_character(&mut self, _id: u32) -> Result<(), StoreError> {
        Err(StoreError::Constraint(
            "delete_character: not supported by this backend".to_owned(),
        ))
    }

    /// `this is ShardDatabaseWithCaching`: its retention times, or `None` for any other backend.
    fn cache_retention_times(&mut self) -> Option<CacheRetentionTimes<'_>> {
        None
    }

    // ---------------------------------------------------------------------------------------------
    // ACE
    // ---------------------------------------------------------------------------------------------

    // ACE: ShardDatabase.Exists
    /// Whether the database is reachable. An embedded database always is once opened, so ACE's
    /// retry loop (`Thread.Sleep(5000)`) never runs.
    fn exists(&mut self, _retry_until_found: bool) -> bool {
        log::info!("[DATABASE] Successfully connected to shard database.");
        true
    }

    // ACE: ShardDatabase.GetMaxGuidFoundInRange
    /// Will return `u32::MAX` if no records were found within the range provided.
    fn get_max_guid_found_in_range(&mut self, min: u32, max: u32) -> u32 {
        let ids = self
            .query_biota_ids(BiotaQuery::IdRange { min, max })
            .unwrap_or_else(|e| panic!("{e}"));

        // OrderByDescending(r => r.Id).FirstOrDefault()
        match ids.iter().max() {
            None => u32::MAX,
            Some(&id) => id,
        }
    }

    // ACE: ShardDatabase.GetSequenceGaps
    /// Available ids, as gaps in the id sequence after `min`; a gap one id wide has `start == end`.
    ///
    /// ACE computes this with a MySQL user-variable query. Its result, ported here: walk the ids
    /// above `min` in order starting from the first of them; each jump from `prev` to `id` is the gap
    /// `(prev + 1, id - 1)`. Gaps are returned while the running total of ids already returned is
    /// below `limit_available_ids_returned` (`u32::MAX` means no limit), so the last gap can
    /// overshoot the limit. Ids below the first existing id above `min`, and ids after the last one,
    /// are not reported.
    fn get_sequence_gaps(
        &mut self,
        min: u32,
        limit_available_ids_returned: u32,
    ) -> Vec<(u32, u32)> {
        if min == u32::MAX {
            return Vec::new(); // WHERE id > uint.MaxValue
        }
        let ids = self
            .query_biota_ids(BiotaQuery::IdRange {
                min: min + 1,
                max: u32::MAX,
            })
            .unwrap_or_else(|e| panic!("{e}"));

        let mut gaps = Vec::new();
        let mut available_ids: u64 = 0;
        let Some(&first) = ids.first() else {
            return gaps;
        };
        let mut rownum = u64::from(first) - 1;

        for &id in &ids {
            let gap_starts_at = rownum + 1;
            rownum += 1;
            let id = u64::from(id);
            if rownum == id {
                continue; // gap_ends_at_not_inclusive = 0: filtered out
            }
            let gap_ends_at_not_inclusive = id;
            rownum = id;

            if limit_available_ids_returned != u32::MAX
                && available_ids >= u64::from(limit_available_ids_returned)
            {
                continue;
            }
            available_ids += gap_ends_at_not_inclusive - gap_starts_at;

            gaps.push((
                gap_starts_at.cs_cast(),
                (gap_ends_at_not_inclusive - 1).cs_cast(),
            ));
        }

        gaps
    }

    // ACE: ShardDatabase.GetBiotaCount
    fn get_biota_count(&mut self) -> i32 {
        i32::try_from(self.count_biotas().unwrap_or_else(|e| panic!("{e}"))).unwrap_or(i32::MAX)
    }

    // ACE: ShardDatabase.GetEstimatedBiotaCount
    /// DIVERGE: MySQL's `information_schema` row estimate does not exist in SQLite; this is the
    /// exact count.
    fn get_estimated_biota_count(&mut self, _db_name: &str) -> i32 {
        self.get_biota_count()
    }

    // ACE: ShardDatabase.GetBiota
    /// The biota with its populated collections, or `None`.
    fn get_biota(&mut self, id: u32, _do_not_add_to_cache: bool) -> Option<Biota> {
        let mut biota = self.load_biota_row(id).unwrap_or_else(|e| panic!("{e}"))?;

        let populated_collection_flags = PopulatedCollectionFlags(biota.populated_collection_flags);

        self.load_biota_collections(&mut biota, populated_collection_flags)
            .unwrap_or_else(|e| panic!("{e}"));

        Some(biota)
    }

    // ACE: ShardDatabase.GetBiotasByWcid
    fn get_biotas_by_wcid(&mut self, wcid: u32) -> Vec<Biota> {
        let results = self
            .query_biota_ids(BiotaQuery::WeenieClassId(wcid))
            .unwrap_or_else(|e| panic!("{e}"));

        let mut biotas = Vec::new();
        for result in results {
            // A row deleted between the two reads gives null in ACE; it is skipped here.
            if let Some(biota) = self.get_biota(result, false) {
                biotas.push(biota);
            }
        }

        biotas
    }

    // ACE: ShardDatabase.GetBiotasByType
    fn get_biotas_by_type(&mut self, r#type: WeenieType) -> Vec<Biota> {
        // warning: this query is currently unindexed!
        let i_type = r#type.0 as i32;

        let results = self
            .query_biota_ids(BiotaQuery::WeenieType(i_type))
            .unwrap_or_else(|e| panic!("{e}"));

        let mut biotas = Vec::new();
        for result in results {
            if let Some(biota) = self.get_biota(result, false) {
                biotas.push(biota);
            }
        }

        biotas
    }

    // ACE: ShardDatabase.DoSaveBiota
    /// Records the populated collections and writes the biota, retrying once, as ACE retries
    /// `SaveChanges`. False when both attempts fail.
    fn do_save_biota(&mut self, biota: &mut Biota) -> bool {
        set_biota_populated_collections(biota);

        let first_exception = match self.write_biota(biota) {
            Ok(()) => return true,
            Err(e) => e,
        };

        match self.write_biota(biota) {
            Ok(()) => {
                log::info!(
                    "[DATABASE] DoSaveBiota 0x{:08X}:{} retry succeeded after initial exception of: {}",
                    biota.id,
                    name_of(biota),
                    first_exception
                );
                true
            }
            Err(ex) => {
                // Character name might be in use or some other fault
                log::error!(
                    "[DATABASE] DoSaveBiota 0x{:08X}:{} failed first attempt with exception: {}",
                    biota.id,
                    name_of(biota),
                    first_exception
                );
                log::error!(
                    "[DATABASE] DoSaveBiota 0x{:08X}:{} failed second attempt with exception: {}",
                    biota.id,
                    name_of(biota),
                    ex
                );
                false
            }
        }
    }

    // ACE: ShardDatabase.SaveBiota
    /// Saves the entity biota: converts it when it is new, otherwise updates the stored rows.
    /// `biota` is the caller's snapshot; `BiotaUpdater` records row ids in it.
    fn save_biota(
        &mut self,
        biota: &mut empyrean_entity::Biota,
        do_not_add_to_cache: bool,
    ) -> bool {
        let existing_biota = self.get_biota(biota.id, do_not_add_to_cache);

        let mut existing_biota = match existing_biota {
            None => BiotaConverter::convert_from_entity_biota(biota, false),
            Some(mut existing_biota) => {
                BiotaUpdater::update_database_biota(biota, &mut existing_biota);
                existing_biota
            }
        };

        self.do_save_biota(&mut existing_biota)
    }

    // ACE: ShardDatabase.SaveBiotasInParallel
    fn save_biotas_in_parallel(
        &mut self,
        biotas: &mut [empyrean_entity::Biota],
        do_not_add_to_cache: bool,
    ) -> bool {
        let mut result = true;

        for biota in biotas.iter_mut() {
            if !self.save_biota(biota, do_not_add_to_cache) {
                result = false;
            }
        }

        result
    }

    // ACE: ShardDatabase.RemoveBiota
    /// Deletes the biota; true when it was absent. Retries once, as ACE does.
    fn remove_biota(&mut self, id: u32) -> bool {
        let existing_biota = self.load_biota_row(id).unwrap_or_else(|e| panic!("{e}"));

        if existing_biota.is_none() {
            return true;
        }

        let first_exception = match self.delete_biota(id) {
            Ok(()) => return true,
            Err(e) => e,
        };

        match self.delete_biota(id) {
            Ok(()) => {
                log::info!("[DATABASE] RemoveBiota 0x{id:08X} retry succeeded after initial exception of: {first_exception}");
                true
            }
            Err(ex) => {
                // Character name might be in use or some other fault
                log::error!("[DATABASE] RemoveBiota 0x{id:08X} failed first attempt with exception: {first_exception}");
                log::error!(
                    "[DATABASE] RemoveBiota 0x{id:08X} failed second attempt with exception: {ex}"
                );
                false
            }
        }
    }

    // ACE: ShardDatabase.RemoveBiotasInParallel
    fn remove_biotas_in_parallel(&mut self, ids: &[u32]) -> bool {
        let mut result = true;

        for &id in ids {
            if !self.remove_biota(id) {
                result = false;
            }
        }

        result
    }

    // ACE: ShardDatabase.GetPossessedBiotasInParallel
    fn get_possessed_biotas_in_parallel(&mut self, id: u32) -> PossessedBiotas {
        let inventory = self.get_inventory_in_parallel(id, true);

        let wielded_items = self.get_wielded_items_in_parallel(id);

        PossessedBiotas::new(inventory, wielded_items)
    }

    // ACE: ShardDatabase.GetInventoryInParallel
    /// The items whose container is `parent_id` (and, with `included_nested_items`, the contents of
    /// those that are containers), each followed by its contents.
    fn get_inventory_in_parallel(
        &mut self,
        parent_id: u32,
        included_nested_items: bool,
    ) -> Vec<Biota> {
        let mut inventory = Vec::new();

        let results = self
            .query_biota_ids(BiotaQuery::InstanceId {
                r#type: PropertyInstanceId::Container.0,
                value: parent_id,
            })
            .unwrap_or_else(|e| panic!("{e}"));

        for result in results {
            let biota = self.get_biota(result, false);

            if let Some(biota) = biota {
                let is_container = biota.weenie_type == WeenieType::Container.0 as i32;
                let biota_id = biota.id;
                inventory.push(biota);

                if included_nested_items && is_container {
                    let sub_items = self.get_inventory_in_parallel(biota_id, false);

                    inventory.extend(sub_items);
                }
            }
        }

        inventory
    }

    // ACE: ShardDatabase.GetWieldedItemsInParallel
    fn get_wielded_items_in_parallel(&mut self, parent_id: u32) -> Vec<Biota> {
        let mut wielded_items = Vec::new();

        let results = self
            .query_biota_ids(BiotaQuery::InstanceId {
                r#type: PropertyInstanceId::Wielder.0,
                value: parent_id,
            })
            .unwrap_or_else(|e| panic!("{e}"));

        for result in results {
            if let Some(biota) = self.get_biota(result, false) {
                wielded_items.push(biota);
            }
        }

        wielded_items
    }

    // ACE: ShardDatabase.GetStaticObjectsByLandblock
    fn get_static_objects_by_landblock(&mut self, landblock_id: u16) -> Vec<Biota> {
        let mut static_objects = Vec::new();

        let static_landblock_id = 0x70000 | u32::from(landblock_id);

        let min = static_landblock_id << 12;
        let max = min | 0xFFF;

        let results = self
            .query_biota_ids(BiotaQuery::IdRange { min, max })
            .unwrap_or_else(|e| panic!("{e}"));

        for result in results {
            if let Some(biota) = self.get_biota(result, false) {
                static_objects.push(biota);
            }
        }

        static_objects
    }

    // ACE: ShardDatabase.GetDynamicObjectsByLandblock
    fn get_dynamic_objects_by_landblock(&mut self, landblock_id: u16) -> Vec<Biota> {
        let mut dynamics = Vec::new();

        // (uint)(landblockId << 16): the shift is done in int.
        let min = (i32::from(landblock_id) << 16) as u32;
        let max = min | 0xFFFF;

        let results = self
            .query_biota_ids(BiotaQuery::LocationInCellRange { min, max })
            .unwrap_or_else(|e| panic!("{e}"));

        for result in results {
            let biota = self
                .get_biota(result, false)
                .expect("NullReferenceException: GetDynamicObjectsByLandblock found no biota for a position row");

            // Filter out objects that are in a container
            if biota
                .biota_properties_iid
                .iter()
                .any(|r| r.r#type == 2 && r.value != 0)
            {
                continue;
            }

            // Filter out wielded objects
            if biota
                .biota_properties_iid
                .iter()
                .any(|r| r.r#type == 3 && r.value != 0)
            {
                continue;
            }

            dynamics.push(biota);
        }

        dynamics
    }

    // ACE: ShardDatabase.GetHousesOwned
    /// The slumlord biotas that have a house owner. As in ACE (`select biota` without includes), only
    /// the `biota` rows are loaded; their collections are empty.
    fn get_houses_owned(&mut self) -> Vec<Biota> {
        let ids = self
            .query_biota_ids(BiotaQuery::TypeWithInstanceId {
                weenie_type: WeenieType::SlumLord.0 as i32,
                iid_type: PropertyInstanceId::HouseOwner.0,
                iid_value: None,
            })
            .unwrap_or_else(|e| panic!("{e}"));

        ids.into_iter()
            .filter_map(|id| self.load_biota_row(id).unwrap_or_else(|e| panic!("{e}")))
            .collect()
    }

    // ACE: ShardDatabase.IsCharacterNameAvailable
    fn is_character_name_available(&mut self, name: &str) -> bool {
        let result = self
            .query_characters(CharacterQuery::NameAvailable(name))
            .unwrap_or_else(|e| panic!("{e}"));

        result.is_empty()
    }

    // ACE: ShardDatabase.GetCharacters
    fn get_characters(&mut self, account_id: u32, include_deleted: bool) -> Vec<Character> {
        get_character_list(self, account_id, include_deleted, 0)
    }

    // ACE: ShardDatabase.GetCharacter
    fn get_character(&mut self, character_id: u32) -> Option<Character> {
        get_character_list(self, 0, true, character_id)
            .into_iter()
            .next()
    }

    // ACE: ShardDatabase.GetCharacterStubByName
    /// When searching by name, only non-deleted characters matter. No property lists are loaded.
    fn get_character_stub_by_name(&mut self, name: &str) -> Option<Character> {
        self.query_characters(CharacterQuery::NameNotDeleted(name))
            .unwrap_or_else(|e| panic!("{e}"))
            .into_iter()
            .next()
    }

    // ACE: ShardDatabase.GetCharacterStubByGuid
    /// No property lists are loaded.
    fn get_character_stub_by_guid(&mut self, guid: u32) -> Option<Character> {
        self.query_characters(CharacterQuery::Id {
            id: guid,
            include_deleted: true,
        })
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .next()
    }

    // ACE: ShardDatabase.SaveCharacter
    /// Writes the character and its property lists, retrying once, as ACE does.
    fn save_character(&mut self, character: &Character) -> bool {
        let first_exception = match self.write_character(character) {
            Ok(()) => return true,
            Err(e) => e,
        };

        match self.write_character(character) {
            Ok(()) => {
                log::info!(
                    "[DATABASE] SaveCharacter-1 0x{:08X}:{} retry succeeded after initial exception of: {}",
                    character.id,
                    character.name,
                    first_exception
                );
                true
            }
            Err(ex) => {
                // Character name might be in use or some other fault
                log::error!(
                    "[DATABASE] SaveCharacter-1 0x{:08X}:{} failed first attempt with exception: {}",
                    character.id,
                    character.name,
                    first_exception
                );
                log::error!(
                    "[DATABASE] SaveCharacter-1 0x{:08X}:{} failed second attempt with exception: {}",
                    character.id,
                    character.name,
                    ex
                );
                false
            }
        }
    }

    // ACE: ShardDatabase.AddCharacterInParallel
    /// Saves a new character: its biota, then its possessions, then the character. Stops at the
    /// first failure (the earlier saves stay, as in ACE).
    fn add_character_in_parallel(
        &mut self,
        biota: &mut empyrean_entity::Biota,
        possessions: &mut [empyrean_entity::Biota],
        character: &Character,
    ) -> bool {
        if !self.save_biota(biota, false) {
            return false; // Biota save failed which mean Character fails.
        }

        if !self.save_biotas_in_parallel(possessions, false) {
            return false;
        }

        if !self.save_character(character) {
            return false;
        }

        true
    }

    // ACE: ShardDatabase.GetAllPlayerBiotasInParallel
    /// This will get all player biotas that are backed by characters that are not deleted.
    fn get_all_player_biotas_in_parallel(&mut self) -> Vec<empyrean_entity::Biota> {
        let mut biotas = Vec::new();

        let results = self
            .query_characters(CharacterQuery::NotDeleted)
            .unwrap_or_else(|e| panic!("{e}"));

        for result in results {
            let biota = self.get_biota(result.id, true);

            if let Some(biota) = biota {
                let converted_biota = BiotaConverter::convert_to_entity_biota(&biota, false);

                biotas.push(converted_biota);
            } else {
                log::error!(
                    "ShardDatabase.GetAllPlayerBiotasInParallel() - couldn't find biota for character 0x{:08X}",
                    result.id
                );
            }
        }

        biotas
    }

    // ACE: ShardDatabase.GetAllegianceID
    /// The allegiance biota of `monarch_id`.
    // ACE-BUG: the LINQ query projects `biota.Id` (a uint), so `FirstOrDefault()` yields 0, not null,
    // when there is no allegiance, and the `uint?` result is never null. Callers that test for null
    // then load biota 0 (which never exists), so the effect is only an extra lookup.
    fn get_allegiance_id(&mut self, monarch_id: u32) -> Option<u32> {
        let ids = self
            .query_biota_ids(BiotaQuery::TypeWithInstanceId {
                weenie_type: WeenieType::Allegiance.0 as i32,
                iid_type: PropertyInstanceId::Monarch.0,
                iid_value: Some(monarch_id),
            })
            .unwrap_or_else(|e| panic!("{e}"));

        Some(ids.first().copied().unwrap_or(0))
    }

    // ACE: ShardDatabase.RenameCharacter
    /// Renames the character (`character` is the caller's snapshot; its name is changed) and saves
    /// it, retrying once.
    fn rename_character(&mut self, character: &mut Character, new_name: &str) -> bool {
        new_name.clone_into(&mut character.name);

        let first_exception = match self.write_character(character) {
            Ok(()) => return true,
            Err(e) => e,
        };

        match self.write_character(character) {
            Ok(()) => {
                log::info!(
                    "[DATABASE] RenameCharacter 0x{:08X}:{} retry succeeded after initial exception of: {}",
                    character.id,
                    character.name,
                    first_exception
                );
                true
            }
            Err(ex) => {
                // Character name might be in use or some other fault
                log::error!(
                    "[DATABASE] RenameCharacter 0x{:08X}:{} failed first attempt with exception: {}",
                    character.id,
                    character.name,
                    first_exception
                );
                log::error!(
                    "[DATABASE] RenameCharacter 0x{:08X}:{} failed second attempt with exception: {}",
                    character.id,
                    character.name,
                    ex
                );
                false
            }
        }
    }
}

// ACE: ShardDatabase.SetBiotaPopulatedCollections
/// Records in `biota.populated_collection_flags` which child collections have rows.
pub fn set_biota_populated_collections(biota: &mut Biota) {
    type F = PopulatedCollectionFlags;
    let mut populated_collection_flags = F::default();

    if !biota.biota_properties_anim_part.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_ANIM_PART;
    }
    if !biota.biota_properties_attribute.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_ATTRIBUTE;
    }
    if !biota.biota_properties_attribute_2nd.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_ATTRIBUTE_2ND;
    }
    if !biota.biota_properties_body_part.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_BODY_PART;
    }
    if biota.biota_properties_book.is_some() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_BOOK;
    }
    if !biota.biota_properties_book_page_data.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_BOOK_PAGE_DATA;
    }
    if !biota.biota_properties_bool.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_BOOL;
    }
    if !biota.biota_properties_create_list.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_CREATE_LIST;
    }
    if !biota.biota_properties_did.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_DID;
    }
    if !biota.biota_properties_emote.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_EMOTE;
    }
    if !biota.biota_properties_enchantment_registry.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_ENCHANTMENT_REGISTRY;
    }
    if !biota.biota_properties_event_filter.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_EVENT_FILTER;
    }
    if !biota.biota_properties_float.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_FLOAT;
    }
    if !biota.biota_properties_generator.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_GENERATOR;
    }
    if !biota.biota_properties_iid.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_IID;
    }
    if !biota.biota_properties_int.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_INT;
    }
    if !biota.biota_properties_int64.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_INT64;
    }
    if !biota.biota_properties_palette.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_PALETTE;
    }
    if !biota.biota_properties_position.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_POSITION;
    }
    if !biota.biota_properties_skill.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_SKILL;
    }
    if !biota.biota_properties_spell_book.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_SPELL_BOOK;
    }
    if !biota.biota_properties_string.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_STRING;
    }
    if !biota.biota_properties_texture_map.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_TEXTURE_MAP;
    }
    if !biota.house_permission.is_empty() {
        populated_collection_flags |= F::HOUSE_PERMISSION;
    }
    if !biota.biota_properties_allegiance.is_empty() {
        populated_collection_flags |= F::BIOTA_PROPERTIES_ALLEGIANCE;
    }

    biota.populated_collection_flags = populated_collection_flags.0;
}

// ACE: ShardDatabase.GetCharacterList
/// The characters of an account (`account_id > 0`) or the one character `character_id`, with their
/// property lists. DIVERGE: ACE returns the same `Character` object it handed out before (its
/// `CharacterContexts` table); here every call returns fresh copies from the database.
fn get_character_list<S: ShardDatabase + ?Sized>(
    db: &mut S,
    account_id: u32,
    include_deleted: bool,
    character_id: u32,
) -> Vec<Character> {
    let query = if account_id > 0 {
        CharacterQuery::Account {
            account_id,
            include_deleted,
        }
    } else {
        CharacterQuery::Id {
            id: character_id,
            include_deleted,
        }
    };

    let mut results = db.query_characters(query).unwrap_or_else(|e| panic!("{e}"));

    for result in &mut results {
        // No reference, pull all the properties
        db.load_character_properties(result)
            .unwrap_or_else(|e| panic!("{e}"));
    }

    results
}

/// `biota.GetProperty(PropertyString.Name)` for the log lines (empty when absent, as C#'s
/// interpolation of null).
fn name_of(biota: &Biota) -> &str {
    biota
        .get_property_string(empyrean_entity::enums::PropertyString::Name)
        .unwrap_or("")
}

/// `ObjectGuid.IsPlayer(id)`.
pub(crate) fn is_player(id: u32) -> bool {
    ObjectGuid::is_player_guid(id)
}
