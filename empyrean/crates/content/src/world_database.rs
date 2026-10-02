// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/WorldDatabase.cs
//! ACE's `WorldDatabase`: the uncached world queries, here over a [`Pack`] instead of EF and MySQL.
//! Named [`WorldDatabaseBase`] because [`crate::WorldDatabase`] is the public API trait.
//!
//! Each EF query becomes a pack lookup that returns the rows in the order MySQL would: a table scan
//! returns primary-key order, and a `WHERE col = x` returns the order of the first-listed index
//! that starts with `col` (MySQL sorts `UNIQUE` indexes before plain ones and keeps the first of
//! equal-cost plans). The pack keeps every table in primary-key order; the few queries whose index
//! differs re-sort ([`mysql_index_order`]). `WHERE name = x` compares under the dump's
//! `utf8_general_ci` collation ([`sql_ci_eq`]). The `context` overloads collapse into one method,
//! since there is no `WorldDbContext`.
//!
//! Methods that ACE declares `virtual` and overrides in `WorldDatabaseWithEntityCache`
//! (`GetWeenie`, `GetAllWeenies`, `GetCookbook`, `GetAllCookbooks`, `GetRecipe`, `GetAllEvents`)
//! are the base versions here; base methods that call one of them virtually (`GetWeenie(string)`,
//! `GetCookbooksByRecipeId`) live on the derived type, where the override is visible.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::WeenieType;

use crate::entity::HouseListResults;
use crate::error::PackError;
use crate::models::world::*;
use crate::overlay::ContentOverlay;
use crate::pack::{Codec, Pack, TableId};
use crate::records::WeenieIndex;

/// A failed read of a pack that `open` already validated means the file changed underneath the
/// mapping or is corrupt; ACE would throw a database exception from the same call.
pub(crate) fn q<T>(r: Result<T, PackError>) -> T {
    r.unwrap_or_else(|e| panic!("world.pack read failed: {e}"))
}

/// MySQL `=` on a text column under `utf8_general_ci`: case-insensitive, and trailing spaces do not
/// count (`PAD SPACE`). The collation also folds accents; world-database names are ASCII.
#[must_use]
pub fn sql_ci_eq(a: &str, b: &str) -> bool {
    a.trim_end_matches(' ').to_lowercase() == b.trim_end_matches(' ').to_lowercase()
}

/// `WHERE object_Id = x` on a weenie child table reads through the table's `UNIQUE (object_Id, …)`
/// index, so rows come back in that index's order. The pack stores primary-key order.
pub(crate) fn mysql_index_order(w: &mut Weenie) {
    w.weenie_properties_anim_part.sort_by_key(|r| r.index);
    w.weenie_properties_attribute.sort_by_key(|r| r.r#type);
    w.weenie_properties_attribute_2nd.sort_by_key(|r| r.r#type);
    w.weenie_properties_body_part.sort_by_key(|r| r.key);
    w.weenie_properties_book_page_data
        .sort_by_key(|r| r.page_id);
    w.weenie_properties_bool.sort_by_key(|r| r.r#type);
    w.weenie_properties_did.sort_by_key(|r| r.r#type);
    w.weenie_properties_event_filter.sort_by_key(|r| r.event);
    w.weenie_properties_float.sort_by_key(|r| r.r#type);
    w.weenie_properties_iid.sort_by_key(|r| r.r#type);
    w.weenie_properties_int.sort_by_key(|r| r.r#type);
    w.weenie_properties_int64.sort_by_key(|r| r.r#type);
    w.weenie_properties_palette
        .sort_by_key(|r| (r.sub_palette_id, r.offset, r.length));
    w.weenie_properties_position
        .sort_by_key(|r| r.position_type);
    w.weenie_properties_skill.sort_by_key(|r| r.r#type);
    w.weenie_properties_string.sort_by_key(|r| r.r#type);
    w.weenie_properties_texture_map
        .sort_by_key(|r| (r.index, r.old_id));
    // Create list, emotes (and their actions) and generators have only a plain `(object_Id)` key,
    // which ends in the primary key: id order, as stored. The spell book is `OrderBy(Id)` in ACE.
}

/// ACE's `WorldDatabase` over a pack.
// ACE: WorldDatabase
#[derive(Debug)]
pub struct WorldDatabaseBase {
    pack: Pack,
    /// Not ACE: the editable overlay, read before the pack once attached.
    overlay: OnceLock<Arc<ContentOverlay>>,
}

impl WorldDatabaseBase {
    #[must_use]
    pub fn new(pack: Pack) -> Self {
        Self {
            pack,
            overlay: OnceLock::new(),
        }
    }

    #[must_use]
    pub fn pack(&self) -> &Pack {
        &self.pack
    }

    /// Not ACE: put `overlay` in front of the pack. Only one overlay can be attached; a second
    /// call gives it back.
    pub fn attach_overlay(&self, overlay: Arc<ContentOverlay>) -> Result<(), Arc<ContentOverlay>> {
        self.overlay.set(overlay)
    }

    /// Not ACE: the attached overlay.
    #[must_use]
    pub fn overlay(&self) -> Option<&Arc<ContentOverlay>> {
        self.overlay.get()
    }

    // ---- pack queries (the EF `DbSet`s) ----
    // Not ACE: every read goes through `get`/`all_keyed`, which put the overlay's records (when
    // one is attached) in front of the pack's.

    /// One record: the overlay's version when it has one (`None` when it deleted it), else the
    /// pack's.
    pub(crate) fn get<T: Codec>(&self, table: TableId, key: u64) -> Option<T> {
        if let Some(o) = self.overlay.get() {
            if let Some(v) = o.layer().decoded::<T>(table, key) {
                return v;
            }
        }
        q(self.pack.get::<T>(table, key))
    }

    /// Every record of a table, ascending by key, the overlay's in front of the pack's.
    pub(crate) fn all_keyed<T: Codec>(&self, table: TableId) -> Vec<(u64, T)> {
        let base = q(self.pack.all::<T>(table));
        match self.overlay.get() {
            None => base,
            Some(o) => o.layer().merge(table, base),
        }
    }

    pub(crate) fn all<T: Codec>(&self, table: TableId) -> Vec<T> {
        self.all_keyed::<T>(table)
            .into_iter()
            .map(|(_, v)| v)
            .collect()
    }

    pub(crate) fn weenie_row(&self, weenie_class_id: u32) -> Option<Weenie> {
        self.get::<Weenie>(TableId::WEENIE, u64::from(weenie_class_id))
    }

    /// `(class id, index)` for every weenie, ascending by class id (the table's primary key).
    pub(crate) fn weenie_index(&self) -> Vec<(u32, WeenieIndex)> {
        self.all_keyed::<WeenieIndex>(TableId::WEENIE_INDEX)
            .into_iter()
            .map(|(k, v)| (u32::try_from(k).expect("class id key"), v))
            .collect()
    }

    /// `LandblockInstance.Where(r => r.Landblock == landblock)` with its links: guid order.
    pub(crate) fn instances(&self, landblock: u16) -> Vec<LandblockInstance> {
        self.get::<Vec<LandblockInstance>>(TableId::LANDBLOCK_INSTANCE, u64::from(landblock))
            .unwrap_or_default()
    }

    /// Every landblock instance, in guid (primary-key) order.
    pub(crate) fn all_instances(&self) -> Vec<LandblockInstance> {
        let mut all: Vec<LandblockInstance> = self
            .all::<Vec<LandblockInstance>>(TableId::LANDBLOCK_INSTANCE)
            .into_iter()
            .flatten()
            .collect();
        all.sort_by_key(|i| i.guid);
        all
    }

    /// `Encounter.Where(r => r.Landblock == landblock)`: through `UNIQUE (landblock, cell_X,
    /// cell_Y)`, the first index on `landblock`.
    pub(crate) fn encounters(&self, landblock: u16) -> Vec<Encounter> {
        let mut v = self
            .get::<Vec<Encounter>>(TableId::ENCOUNTER, u64::from(landblock))
            .unwrap_or_default();
        v.sort_by_key(|e| (e.cell_x, e.cell_y));
        v
    }

    /// Every cook book row, id order.
    pub(crate) fn all_cookbook_rows(&self) -> Vec<CookBook> {
        let mut all: Vec<CookBook> = self
            .all::<Vec<CookBook>>(TableId::COOK_BOOK)
            .into_iter()
            .flatten()
            .collect();
        all.sort_by_key(|c| c.id);
        all
    }

    // ---- ported members ----

    /// There is no server to reach: the pack was opened, so the database exists.
    // ACE: WorldDatabase.Exists
    #[must_use]
    pub fn exists(&self, _retry_until_found: bool) -> bool {
        log::info!(
            "[DATABASE] Successfully opened world database ({} records).",
            self.pack.header().index_count
        );
        true
    }

    // =====================================
    // Weenie
    // =====================================

    /// This will populate all sub collections except the following: LandblockInstances,
    /// PointsOfInterest. Creature-only and book-only collections stay empty for other types, as in
    /// ACE, even when the database has rows for them.
    // ACE: WorldDatabase.GetWeenie
    #[must_use]
    pub fn get_weenie(&self, weenie_class_id: u32) -> Option<Weenie> {
        let mut weenie = self.weenie_row(weenie_class_id)?;
        mysql_index_order(&mut weenie);
        // Not ACE: the retail corrections to the stored data (`crate::corrections`).
        crate::corrections::apply_for(&mut weenie, self.pack.era());

        #[allow(clippy::cast_sign_loss)]
        let weenie_type = WeenieType(weenie.r#type as u32);

        let is_creature = weenie_type == WeenieType::Creature
            || weenie_type == WeenieType::Cow
            || weenie_type == WeenieType::Sentinel
            || weenie_type == WeenieType::Admin
            || weenie_type == WeenieType::Vendor
            || weenie_type == WeenieType::CombatPet
            || weenie_type == WeenieType::Pet;

        if !is_creature {
            weenie.weenie_properties_attribute = Vec::new();
            weenie.weenie_properties_attribute_2nd = Vec::new();
            weenie.weenie_properties_body_part = Vec::new();
            weenie.weenie_properties_skill = Vec::new();
        }

        if weenie_type != WeenieType::Book {
            weenie.weenie_properties_book = None;
            weenie.weenie_properties_book_page_data = Vec::new();
        }

        weenie.weenie_properties_spell_book.sort_by_key(|r| r.id);

        Some(weenie)
    }

    /// Every weenie with every collection, as EF's tracked `Load()` of all tables fixes them up:
    /// primary-key order throughout, and no creature/book filtering.
    // ACE: WorldDatabase.GetAllWeenies
    #[must_use]
    pub fn get_all_weenies(&self) -> Vec<Weenie> {
        let mut all = self.all::<Weenie>(TableId::WEENIE);
        // Not ACE: the retail corrections to the stored data (`crate::corrections`).
        let era = self.pack.era();
        for w in &mut all {
            crate::corrections::apply_for(w, era);
        }
        all
    }

    /// Not ACE: the weenie row as the pack (or the overlay) stores it, before the retail
    /// corrections [`Self::get_weenie`] applies, with ACE's row order.
    #[must_use]
    pub fn get_stored_weenie(&self, weenie_class_id: u32) -> Option<Weenie> {
        let mut weenie = self.weenie_row(weenie_class_id)?;
        mysql_index_order(&mut weenie);
        Some(weenie)
    }

    /// The class id of the first weenie whose `ClassName` equals `weenie_class_name` (MySQL
    /// collation); the first half of ACE's `GetWeenie(context, string)`.
    pub(crate) fn weenie_class_id_by_name(&self, weenie_class_name: &str) -> Option<u32> {
        self.weenie_index()
            .into_iter()
            .find(|(_, i)| sql_ci_eq(&i.class_name, weenie_class_name))
            .map(|(k, _)| k)
    }

    // ACE: WorldDatabase.GetAllWeenieNames
    #[must_use]
    pub fn get_all_weenie_names(&self) -> DotNetDict<u32, String> {
        let mut d = DotNetDict::new();
        for (k, i) in self.weenie_index() {
            d.add(k, i.name.unwrap_or_default());
        }
        d
    }

    /// Class-id order: a plain `ToDictionary` over the table scan.
    // ACE: WorldDatabase.GetAllWeenieClassNames
    #[must_use]
    pub fn get_all_weenie_class_names(&self) -> DotNetDict<u32, String> {
        let mut d = DotNetDict::new();
        for (k, i) in self.weenie_index() {
            d.add(k, i.class_name);
        }
        d
    }

    /// `weenie JOIN landblock_instance WHERE weenie.type == SlumLord`. MySQL drives the join from
    /// the instance table (the weenie side is an `eq_ref` on its primary key), so results come in
    /// instance guid order.
    // ACE: WorldDatabase.GetHousesAll
    #[must_use]
    pub fn get_houses_all(&self) -> Vec<HouseListResults> {
        #[allow(clippy::cast_possible_wrap)]
        let slum_lord = WeenieType::SlumLord.0 as i32;
        let slum_lords: HashMap<u32, ()> = self
            .weenie_index()
            .into_iter()
            .filter(|(_, i)| i.r#type == slum_lord)
            .map(|(k, _)| (k, ()))
            .collect();
        let mut rows: HashMap<u32, Weenie> = HashMap::new();
        let mut results = Vec::new();
        for winst in self.all_instances() {
            if !slum_lords.contains_key(&winst.weenie_class_id) {
                continue;
            }
            let weenie = rows
                .entry(winst.weenie_class_id)
                .or_insert_with(|| {
                    // The join materialises only the weenie row itself, no child collections.
                    let w = self.weenie_row(winst.weenie_class_id).unwrap_or_default();
                    Weenie {
                        class_id: w.class_id,
                        class_name: w.class_name,
                        r#type: w.r#type,
                        last_modified: w.last_modified,
                        ..Default::default()
                    }
                })
                .clone();
            let mut winst = winst;
            winst.landblock_instance_link = Vec::new();
            results.push(HouseListResults::new(weenie, winst));
        }
        results
    }

    // =====================================
    // CookBook
    // =====================================

    /// `FirstOrDefault(r => r.SourceWCID == source && r.TargetWCID == target)` with the recipe and
    /// all its children included.
    // ACE: WorldDatabase.GetCookbook
    #[must_use]
    pub fn get_cookbook(
        &self,
        source_weenie_class_id: u32,
        target_weenie_class_id: u32,
    ) -> Option<CookBook> {
        let key = (u64::from(source_weenie_class_id) << 32) | u64::from(target_weenie_class_id);
        let rows = self.get::<Vec<CookBook>>(TableId::COOK_BOOK, key)?;
        let mut result = rows.into_iter().min_by_key(|c| c.id)?;
        result.recipe = self.get_recipe(result.recipe_id).map(Arc::new);
        Some(result)
    }

    /// Every cook book in id order; EF's fix-up gives each the one tracked instance of its recipe.
    // ACE: WorldDatabase.GetAllCookbooks
    #[must_use]
    pub fn get_all_cookbooks(&self) -> Vec<CookBook> {
        let recipes: HashMap<u32, Arc<Recipe>> = self
            .all::<Recipe>(TableId::RECIPE)
            .into_iter()
            .map(|r| (r.id, Arc::new(r)))
            .collect();
        self.all_cookbook_rows()
            .into_iter()
            .map(|mut c| {
                c.recipe = recipes.get(&c.recipe_id).cloned();
                c
            })
            .collect()
    }

    /// `CookBook.Where(i => i.RecipeId == recipeId)`: through `UNIQUE (recipe_Id, source, target)`.
    pub(crate) fn cookbook_rows_by_recipe_id(&self, recipe_id: u32) -> Vec<CookBook> {
        let mut rows: Vec<CookBook> = self
            .all_cookbook_rows()
            .into_iter()
            .filter(|c| c.recipe_id == recipe_id)
            .collect();
        rows.sort_by_key(|c| (c.source_wcid, c.target_wcid));
        rows
    }

    // =====================================
    // Recipe
    // =====================================

    // ACE: WorldDatabase.GetRecipe
    #[must_use]
    pub fn get_recipe(&self, recipe_id: u32) -> Option<Recipe> {
        self.get::<Recipe>(TableId::RECIPE, u64::from(recipe_id))
    }

    // =====================================
    // Event
    // =====================================

    /// This takes under 1 second to complete.
    // ACE: WorldDatabase.GetAllEvents
    #[must_use]
    pub fn get_all_events(&self) -> Vec<Event> {
        self.all::<Event>(TableId::EVENT)
    }

    // =====================================
    // LandblockInstance
    // =====================================

    /// World guids are `0x7LLLLnnn`, so the landblock in the guid is tried first; any other guid
    /// falls back to a scan of every landblock.
    // ACE: WorldDatabase.GetLandblockInstanceByGuid
    #[must_use]
    pub fn get_landblock_instance_by_guid(&self, guid: u32) -> Option<LandblockInstance> {
        let hint = u16::try_from((guid >> 12) & 0xFFFF).expect("16 bits");
        if let Some(i) = self.instances(hint).into_iter().find(|r| r.guid == guid) {
            return Some(i);
        }
        self.all_instances().into_iter().find(|r| r.guid == guid)
    }

    // ACE: WorldDatabase.GetLandblockInstancesByWcid
    #[must_use]
    pub fn get_landblock_instances_by_wcid(&self, wcid: u32) -> Vec<LandblockInstance> {
        self.all_instances()
            .into_iter()
            .filter(|i| i.weenie_class_id == wcid)
            .collect()
    }

    // =====================================
    // Spell
    // =====================================

    // ACE: WorldDatabase.GetAllSpellNames
    #[must_use]
    pub fn get_all_spell_names(&self) -> DotNetDict<u32, String> {
        let mut d = DotNetDict::new();
        for r in self.all::<Spell>(TableId::SPELL) {
            d.add(r.id, r.name);
        }
        d
    }

    // =====================================
    // TreasureDeath
    // =====================================

    /// # Panics
    /// On two rows with one `TreasureType`, as `ToDictionary` throws `ArgumentException`.
    // ACE: WorldDatabase.GetAllTreasureDeath
    #[must_use]
    pub fn get_all_treasure_death(&self) -> DotNetDict<u32, TreasureDeath> {
        let mut d = DotNetDict::new();
        for r in self.all::<TreasureDeath>(TableId::TREASURE_DEATH) {
            d.add(r.treasure_type, r);
        }
        d
    }

    // =====================================
    // TreasureGemCount
    // =====================================

    /// Not ACE: the `ctx.TreasureGemCount` query of `GemCountChance`'s static constructor
    /// (`Factories/Tables/GemCountChance.cs`), every row in primary-key order (the order MariaDB
    /// returns them in); the constructor keeps the rows whose `Chance > 0`, as ACE's `Where` does.
    #[must_use]
    pub fn get_all_treasure_gem_count(&self) -> Vec<TreasureGemCount> {
        self.all::<TreasureGemCount>(TableId::TREASURE_GEM_COUNT)
    }

    // =====================================
    // TreasureWielded
    // =====================================

    // ACE: WorldDatabase.GetAllTreasureWielded
    #[must_use]
    pub fn get_all_treasure_wielded(&self) -> DotNetDict<u32, Vec<TreasureWielded>> {
        let mut treasure: DotNetDict<u32, Vec<TreasureWielded>> = DotNetDict::new();
        for record in self.all::<TreasureWielded>(TableId::TREASURE_WIELDED) {
            if !treasure.contains_key(&record.treasure_type) {
                treasure.add(record.treasure_type, Vec::new());
            }
            treasure
                .get_mut(&record.treasure_type)
                .expect("just added")
                .push(record);
        }
        treasure
    }

    // =====================================
    // Version
    // =====================================

    /// Get the version information stored in database
    // ACE: WorldDatabase.GetVersion
    #[must_use]
    pub fn get_version(&self) -> Option<Version> {
        self.get::<Version>(TableId::VERSION, 1)
    }

    // =====================================
    // IsWorldDatabaseGuidRangeValid
    // =====================================

    // ACE: WorldDatabase.IsWorldDatabaseGuidRangeValid
    #[must_use]
    pub fn is_world_database_guid_range_valid(&self) -> bool {
        !self
            .all::<Vec<LandblockInstance>>(TableId::LANDBLOCK_INSTANCE)
            .iter()
            .flatten()
            .any(|i| i.guid >= 0x8000_0000)
    }
}
