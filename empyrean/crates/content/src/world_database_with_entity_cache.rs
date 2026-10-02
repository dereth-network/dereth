// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/WorldDatabaseWithEntityCache.cs
//! ACE's `WorldDatabaseWithEntityCache`: [`WorldDatabaseBase`] plus the caches the server reads
//! through (`DatabaseManager.World`). Weenies are cached converted to [`empyrean_entity::Weenie`];
//! every other cached row is shared as an `Arc`, as ACE shares the cached object.
//!
//! ACE's `ConcurrentDictionary`s become `Mutex<HashMap>`s: a lookup that misses runs the query
//! without holding the lock, as ACE's lock-free read-then-add does. A `null` stored in an ACE
//! cache is a `None` value here.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use empyrean_common::dotnet::DotNetDict;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{PropertyDataId, WeenieType};

use crate::adapter::weenie_converter::convert_to_entity_weenie;
use crate::entity::HouseListResults;
use crate::error::PackError;
use crate::models::world::*;
use crate::pack::{Pack, TableId};
use crate::world_database::{sql_ci_eq, WorldDatabaseBase};

type EntityWeenie = empyrean_entity::Weenie;
type Tiered<T> = HashMap<i32, HashMap<i32, Arc<Vec<T>>>>;
/// Source class id to target class id to cook book (`null` for a miss).
type CookbookCache = HashMap<u32, HashMap<u32, Option<Arc<CookBook>>>>;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// C# `string.ToLower()`; the server runs under the invariant culture.
fn to_lower(s: &str) -> String {
    s.to_lowercase()
}

#[derive(Debug, Default)]
struct Caches {
    weenie_cache: Mutex<HashMap<u32, Option<Arc<EntityWeenie>>>>,
    weenie_class_name_to_class_id_cache: Mutex<HashMap<String, u32>>,
    weenie_specific_caches_populated: Mutex<bool>,
    weenie_cache_by_type: Mutex<HashMap<WeenieType, Vec<Arc<EntityWeenie>>>>,
    scrolls_by_spell_id: Mutex<HashMap<u32, Arc<EntityWeenie>>>,
    creature_weenie_names_lower_invariant_cache: Mutex<HashMap<String, u32>>,
    cookbook_cache: Mutex<CookbookCache>,
    recipe_cache: Mutex<HashMap<u32, Option<Arc<Recipe>>>>,
    cached_encounters: Mutex<HashMap<u16, Arc<Vec<Encounter>>>>,
    cached_events: Mutex<HashMap<String, Option<Arc<Event>>>>,
    cached_house_portals: Mutex<HashMap<u32, Arc<Vec<HousePortal>>>>,
    cached_house_portals_by_landblock: Mutex<HashMap<u32, Arc<Vec<HousePortal>>>>,
    cached_landblock_instances: Mutex<HashMap<u16, Arc<Vec<LandblockInstance>>>>,
    cached_basement_house_guids: Mutex<HashMap<u16, u32>>,
    cached_points_of_interest: Mutex<HashMap<String, Option<Arc<PointsOfInterest>>>>,
    cached_quest: Mutex<HashMap<String, Option<Arc<Quest>>>>,
    spell_cache: Mutex<HashMap<u32, Option<Arc<Spell>>>>,
    cached_death_treasure: Mutex<HashMap<u32, Option<Arc<TreasureDeath>>>>,
    cached_treasure_material_base: Mutex<Option<Arc<Tiered<TreasureMaterialBase>>>>,
    cached_treasure_material_color: Mutex<Option<Arc<Tiered<TreasureMaterialColor>>>>,
    cached_treasure_material_groups: Mutex<Option<Arc<Tiered<TreasureMaterialGroups>>>>,
    cached_wielded_treasure: Mutex<HashMap<u32, Arc<Vec<TreasureWielded>>>>,
}

/// ACE's `WorldDatabaseWithEntityCache` over a pack. [`crate::PackContent`] is this type.
// ACE: WorldDatabaseWithEntityCache
#[derive(Debug)]
pub struct WorldDatabaseWithEntityCache {
    base: WorldDatabaseBase,
    c: Caches,
}

const NORMALIZE_EPSILON: f32 = 0.00001;

impl WorldDatabaseWithEntityCache {
    #[must_use]
    pub fn new(pack: Pack) -> Self {
        Self {
            base: WorldDatabaseBase::new(pack),
            c: Caches::default(),
        }
    }

    /// Map `world.pack` and validate its structure (not its hash; see [`Pack::verify_hash`]).
    pub fn open(path: &Path) -> Result<Self, PackError> {
        Pack::open(path).map(Self::new)
    }

    /// The uncached queries.
    #[must_use]
    pub fn base(&self) -> &WorldDatabaseBase {
        &self.base
    }

    // =====================================
    // Weenie
    // =====================================

    /// This will populate all sub collections except the following: LandblockInstances,
    /// PointsOfInterest. This will also update the weenie cache.
    // ACE: WorldDatabaseWithEntityCache.GetWeenie
    #[must_use]
    pub fn get_weenie(&self, weenie_class_id: u32) -> Option<Weenie> {
        let weenie = self.base.get_weenie(weenie_class_id);

        // If the weenie doesn't exist in the cache, we'll add it.
        if let Some(weenie) = &weenie {
            lock(&self.c.weenie_cache).insert(
                weenie_class_id,
                Some(Arc::new(convert_to_entity_weenie(weenie, false))),
            );
            lock(&self.c.weenie_class_name_to_class_id_cache)
                .insert(to_lower(&weenie.class_name), weenie.class_id);
        } else {
            lock(&self.c.weenie_cache).insert(weenie_class_id, None);
        }

        weenie
    }

    /// This will populate all sub collections except the following: LandblockInstances,
    /// PointsOfInterest.
    // ACE: WorldDatabase.GetWeenie
    #[must_use]
    pub fn get_weenie_by_class_name(&self, weenie_class_name: &str) -> Option<Weenie> {
        let class_id = self.base.weenie_class_id_by_name(weenie_class_name)?;
        self.get_weenie(class_id)
    }

    /// This will populate all sub collections except the following: LandblockInstances,
    /// PointsOfInterest. This will also update the weenie cache.
    // ACE: WorldDatabaseWithEntityCache.GetAllWeenies
    #[must_use]
    pub fn get_all_weenies(&self) -> Vec<Weenie> {
        let weenies = self.base.get_all_weenies();

        // Add the weenies to the cache
        for weenie in &weenies {
            lock(&self.c.weenie_cache).insert(
                weenie.class_id,
                Some(Arc::new(convert_to_entity_weenie(weenie, false))),
            );
            lock(&self.c.weenie_class_name_to_class_id_cache)
                .insert(to_lower(&weenie.class_name), weenie.class_id);
        }

        weenies
    }

    /// This will make sure every weenie in the database has been read and cached.
    // ACE: WorldDatabaseWithEntityCache.CacheAllWeenies
    pub fn cache_all_weenies(&self) {
        let _ = self.get_all_weenies();

        self.populate_weenie_specific_caches();
    }

    /// Returns the number of weenies currently cached.
    // ACE: WorldDatabaseWithEntityCache.GetWeenieCacheCount
    #[must_use]
    pub fn get_weenie_cache_count(&self) -> i32 {
        count(
            lock(&self.c.weenie_cache)
                .values()
                .filter(|v| v.is_some())
                .count(),
        )
    }

    // ACE: WorldDatabaseWithEntityCache.ClearWeenieCache
    pub fn clear_weenie_cache(&self) {
        lock(&self.c.weenie_cache).clear();
        lock(&self.c.weenie_class_name_to_class_id_cache).clear();

        *lock(&self.c.weenie_specific_caches_populated) = false;
    }

    /// Weenies will have all their collections populated except the following:
    /// LandblockInstances, PointsOfInterest.
    // ACE: WorldDatabaseWithEntityCache.GetCachedWeenie
    #[must_use]
    pub fn get_cached_weenie(&self, weenie_class_id: u32) -> Option<Arc<EntityWeenie>> {
        if let Some(value) = lock(&self.c.weenie_cache).get(&weenie_class_id) {
            return value.clone();
        }

        let _ = self.get_weenie(weenie_class_id); // This will add the result into the caches

        lock(&self.c.weenie_cache)
            .get(&weenie_class_id)
            .cloned()
            .flatten()
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedWeenie
    #[must_use]
    pub fn get_cached_weenie_by_class_name(
        &self,
        weenie_class_name: &str,
    ) -> Option<Arc<EntityWeenie>> {
        let key = to_lower(weenie_class_name);
        let cached = lock(&self.c.weenie_class_name_to_class_id_cache)
            .get(&key)
            .copied();
        if let Some(value) = cached {
            return self.get_cached_weenie(value); // This will add the result into the caches
        }

        let _ = self.get_weenie_by_class_name(weenie_class_name); // This will add the result into the caches

        let value = lock(&self.c.weenie_class_name_to_class_id_cache)
            .get(&key)
            .copied()
            .unwrap_or(0);

        self.get_cached_weenie(value) // This will add the result into the caches
    }

    // ACE: WorldDatabaseWithEntityCache.ClearCachedWeenie
    pub fn clear_cached_weenie(&self, weenie_class_id: u32) -> bool {
        lock(&self.c.weenie_cache)
            .remove(&weenie_class_id)
            .is_some()
    }

    /// ACE walks `weenieCache.Values`, a `ConcurrentDictionary` whose order depends on its bucket
    /// count (which depends on the processor count); this walks ascending class id.
    // ACE: WorldDatabaseWithEntityCache.PopulateWeenieSpecificCaches
    // DIVERGE: cached weenies are visited in class-id order, not ConcurrentDictionary bucket order.
    fn populate_weenie_specific_caches(&self) {
        let mut all: Vec<Arc<EntityWeenie>> = lock(&self.c.weenie_cache)
            .values()
            .flatten()
            .cloned()
            .collect();
        all.sort_by_key(|w| w.weenie_class_id);

        // populate weenieCacheByType
        {
            let mut by_type = lock(&self.c.weenie_cache_by_type);
            for weenie in &all {
                let weenies = by_type.entry(weenie.weenie_type).or_default();
                if !weenies.iter().any(|w| Arc::ptr_eq(w, weenie)) {
                    weenies.push(weenie.clone());
                }
            }
        }

        // populate scrollsBySpellID
        {
            let mut scrolls = lock(&self.c.scrolls_by_spell_id);
            for weenie in &all {
                if weenie.weenie_type == WeenieType::Scroll {
                    if let Some(value) = weenie
                        .properties_did
                        .as_ref()
                        .and_then(|d| d.get(&PropertyDataId::Spell))
                    {
                        scrolls.insert(*value, weenie.clone());
                    }
                }
            }
        }

        *lock(&self.c.weenie_specific_caches_populated) = true;
    }

    /// `count` weenies of one type, each drawn with `ThreadSafeRandom.Next(0, n - 1)`.
    // ACE: WorldDatabaseWithEntityCache.GetRandomWeeniesOfType
    #[must_use]
    pub fn get_random_weenies_of_type(
        &self,
        weenie_type_id: i32,
        count: i32,
    ) -> Vec<Option<Arc<EntityWeenie>>> {
        #[allow(clippy::cast_sign_loss)]
        let weenie_type = WeenieType(weenie_type_id as u32);
        let cached = lock(&self.c.weenie_cache_by_type)
            .get(&weenie_type)
            .cloned();
        let weenies = match cached {
            Some(w) => w,
            None => {
                if !*lock(&self.c.weenie_specific_caches_populated) {
                    let results: Vec<u32> = self
                        .base
                        .weenie_index()
                        .into_iter()
                        .filter(|(_, i)| i.r#type == weenie_type_id)
                        .map(|(k, _)| k)
                        .collect();

                    let mut weenies = Vec::new();

                    if results.is_empty() {
                        return weenies;
                    }

                    for _ in 0..count {
                        let index = ThreadSafeRandom::next(0, len_i32(results.len()) - 1);

                        let weenie = self.get_cached_weenie(results[idx(index)]);

                        weenies.push(weenie);
                    }

                    return weenies;
                }

                lock(&self.c.weenie_cache_by_type).insert(weenie_type, Vec::new());
                Vec::new()
            }
        };

        if weenies.is_empty() {
            return Vec::new();
        }

        let mut results = Vec::new();

        for _ in 0..count {
            let index = ThreadSafeRandom::next(0, len_i32(weenies.len()) - 1);

            let weenie = self.get_cached_weenie(weenies[idx(index)].weenie_class_id);

            results.push(weenie);
        }

        results
    }

    /// Before the weenie caches are populated, the scroll comes from a join that materialises only
    /// the `weenie` row, so the converted weenie has no properties (ACE's only caller reads just its
    /// class id).
    // ACE: WorldDatabaseWithEntityCache.GetScrollWeenie
    #[must_use]
    pub fn get_scroll_weenie(&self, spell_id: u32) -> Option<Arc<EntityWeenie>> {
        if let Some(weenie) = lock(&self.c.scrolls_by_spell_id).get(&spell_id) {
            return Some(weenie.clone());
        }
        if *lock(&self.c.weenie_specific_caches_populated) {
            return None;
        }

        #[allow(clippy::cast_possible_wrap)]
        let scroll = WeenieType::Scroll.0 as i32;
        let (class_id, index) = self
            .base
            .weenie_index()
            .into_iter()
            .find(|(_, i)| i.r#type == scroll && i.spell_did == Some(spell_id))?;
        let row = Weenie {
            class_id,
            class_name: index.class_name,
            r#type: index.r#type,
            ..Default::default()
        };

        let weenie = Arc::new(convert_to_entity_weenie(&row, false));

        lock(&self.c.scrolls_by_spell_id).insert(spell_id, weenie.clone());

        Some(weenie)
    }

    // ACE: WorldDatabaseWithEntityCache.IsCreatureNameInWorldDatabase
    #[must_use]
    pub fn is_creature_name_in_world_database(&self, name: &str) -> bool {
        if lock(&self.c.creature_weenie_names_lower_invariant_cache)
            .contains_key(&name.to_lowercase())
        {
            return true;
        }

        #[allow(clippy::cast_possible_wrap)]
        let creature = WeenieType::Creature.0 as i32;
        let found = self.base.weenie_index().into_iter().find(|(_, i)| {
            i.r#type == creature && i.name.as_deref().is_some_and(|n| sql_ci_eq(n, name))
        });

        let Some((class_id, index)) = found else {
            return false;
        };

        let weenie_name = index.name.unwrap_or_default().to_lowercase();

        lock(&self.c.creature_weenie_names_lower_invariant_cache)
            .entry(weenie_name)
            .or_insert(class_id);

        true
    }

    // =====================================
    // CookBook
    // =====================================

    // ACE: WorldDatabaseWithEntityCache.GetCookbook
    #[must_use]
    pub fn get_cookbook(
        &self,
        source_weenie_class_id: u32,
        target_weenie_class_id: u32,
    ) -> Option<Arc<CookBook>> {
        let cookbook = self
            .base
            .get_cookbook(source_weenie_class_id, target_weenie_class_id)
            .map(Arc::new);

        {
            // We double check before commiting the recipe.
            let mut cache = lock(&self.c.cookbook_cache);
            let source_recipes = cache.entry(source_weenie_class_id).or_default();
            source_recipes
                .entry(target_weenie_class_id)
                .or_insert_with(|| cookbook.clone());
        }

        if let Some(cookbook) = &cookbook {
            // build secondary index for RecipeManager_New caching
            lock(&self.c.recipe_cache)
                .entry(cookbook.recipe_id)
                .or_insert_with(|| cookbook.recipe.clone());
        }
        cookbook
    }

    // ACE: WorldDatabaseWithEntityCache.GetAllCookbooks
    #[must_use]
    pub fn get_all_cookbooks(&self) -> Vec<Arc<CookBook>> {
        let cookbooks: Vec<Arc<CookBook>> = self
            .base
            .get_all_cookbooks()
            .into_iter()
            .map(Arc::new)
            .collect();

        // Add the cookbooks to the cache
        {
            let mut cache = lock(&self.c.cookbook_cache);
            for cookbook in &cookbooks {
                let source_recipes = cache.entry(cookbook.source_wcid).or_default();
                source_recipes
                    .entry(cookbook.target_wcid)
                    .or_insert_with(|| Some(cookbook.clone()));
            }
        }

        // build secondary index for RecipeManager_New caching
        {
            let mut recipes = lock(&self.c.recipe_cache);
            for cookbook in &cookbooks {
                recipes
                    .entry(cookbook.recipe_id)
                    .or_insert_with(|| cookbook.recipe.clone());
            }
        }

        cookbooks
    }

    // ACE: WorldDatabaseWithEntityCache.CacheAllCookbooks
    pub fn cache_all_cookbooks(&self) {
        let _ = self.get_all_cookbooks();
    }

    /// Returns the number of Cookbooks currently cached.
    // ACE: WorldDatabaseWithEntityCache.GetCookbookCacheCount
    #[must_use]
    pub fn get_cookbook_cache_count(&self) -> i32 {
        count(lock(&self.c.cookbook_cache).len())
    }

    // ACE: WorldDatabaseWithEntityCache.ClearCookbookCache
    pub fn clear_cookbook_cache(&self) {
        lock(&self.c.cookbook_cache).clear();

        lock(&self.c.recipe_cache).clear();
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedCookbook
    #[must_use]
    pub fn get_cached_cookbook(
        &self,
        source_weenie_class_id: u32,
        target_weenie_class_id: u32,
    ) -> Option<Arc<CookBook>> {
        {
            let cache = lock(&self.c.cookbook_cache);
            if let Some(value) = cache
                .get(&source_weenie_class_id)
                .and_then(|r| r.get(&target_weenie_class_id))
            {
                return value.clone();
            }
        }
        self.get_cookbook(source_weenie_class_id, target_weenie_class_id) // This will add the result into the cache
    }

    /// `CookBook.Where(RecipeId == recipeId)`, then `GetCookbook` (the cached override) per row.
    // ACE: WorldDatabase.GetCookbooksByRecipeId
    #[must_use]
    pub fn get_cookbooks_by_recipe_id(&self, recipe_id: u32) -> Vec<Option<Arc<CookBook>>> {
        let base_records = self.base.cookbook_rows_by_recipe_id(recipe_id);

        base_records
            .iter()
            .map(|b| self.get_cookbook(b.source_wcid, b.target_wcid))
            .collect()
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedRecipe
    #[must_use]
    pub fn get_cached_recipe(&self, recipe_id: u32) -> Option<Arc<Recipe>> {
        if let Some(recipe) = lock(&self.c.recipe_cache).get(&recipe_id) {
            return recipe.clone();
        }
        self.get_recipe(recipe_id) // This will add the result in the cache
    }

    // ACE: WorldDatabaseWithEntityCache.GetRecipe
    #[must_use]
    pub fn get_recipe(&self, recipe_id: u32) -> Option<Arc<Recipe>> {
        let recipe = self.base.get_recipe(recipe_id).map(Arc::new);

        lock(&self.c.recipe_cache)
            .entry(recipe_id)
            .or_insert_with(|| recipe.clone());
        recipe
    }

    // =====================================
    // Encounter
    // =====================================

    /// Returns the number of Encounters currently cached.
    // ACE: WorldDatabaseWithEntityCache.GetEncounterCacheCount
    #[must_use]
    pub fn get_encounter_cache_count(&self) -> i32 {
        count(lock(&self.c.cached_encounters).len())
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedEncountersByLandblock
    #[must_use]
    pub fn get_cached_encounters_by_landblock(&self, landblock: u16) -> Arc<Vec<Encounter>> {
        if let Some(value) = lock(&self.c.cached_encounters).get(&landblock) {
            return value.clone();
        }

        let results = Arc::new(self.base.encounters(landblock));

        lock(&self.c.cached_encounters)
            .entry(landblock)
            .or_insert_with(|| results.clone());
        results
    }

    // ACE: WorldDatabaseWithEntityCache.ClearCachedEncountersByLandblock
    pub fn clear_cached_encounters_by_landblock(&self, landblock: u16) -> bool {
        lock(&self.c.cached_encounters).remove(&landblock).is_some()
    }

    // ACE: WorldDatabaseWithEntityCache.ClearCachedEvent
    pub fn clear_cached_event(&self, event_name: &str) -> bool {
        lock(&self.c.cached_events)
            .remove(&to_lower(event_name))
            .is_some()
    }

    // =====================================
    // Event
    // =====================================

    // ACE: WorldDatabaseWithEntityCache.GetAllEvents
    #[must_use]
    pub fn get_all_events(&self) -> Vec<Arc<Event>> {
        let events: Vec<Arc<Event>> = self
            .base
            .get_all_events()
            .into_iter()
            .map(Arc::new)
            .collect();

        let mut cache = lock(&self.c.cached_events);
        for result in &events {
            cache.insert(to_lower(&result.name), Some(result.clone()));
        }

        events
    }

    /// Returns the number of Events currently cached.
    // ACE: WorldDatabaseWithEntityCache.GetEventsCacheCount
    #[must_use]
    pub fn get_events_cache_count(&self) -> i32 {
        count(
            lock(&self.c.cached_events)
                .values()
                .filter(|v| v.is_some())
                .count(),
        )
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedEvent
    #[must_use]
    pub fn get_cached_event(&self, name: &str) -> Option<Arc<Event>> {
        let name_to_lower = to_lower(name);

        if let Some(value) = lock(&self.c.cached_events).get(&name_to_lower) {
            return value.clone();
        }

        let result = self
            .base
            .all::<Event>(TableId::EVENT)
            .into_iter()
            .find(|r| sql_ci_eq(&to_lower(&r.name), &name_to_lower))
            .map(Arc::new);

        lock(&self.c.cached_events).insert(name_to_lower, result.clone());
        result
    }

    // =====================================
    // HousePortal
    // =====================================

    // ACE: WorldDatabaseWithEntityCache.CacheAllHousePortals
    pub fn cache_all_house_portals(&self) {
        let mut groups: Vec<(u32, Vec<HousePortal>)> = Vec::new();
        for r in self.base.all::<HousePortal>(TableId::HOUSE_PORTAL) {
            match groups.iter_mut().find(|(k, _)| *k == r.house_id) {
                Some((_, v)) => v.push(r),
                None => groups.push((r.house_id, vec![r])),
            }
        }

        let mut cache = lock(&self.c.cached_house_portals);
        for (key, list) in groups {
            cache.insert(key, Arc::new(list));
        }
    }

    /// `Where(p => p.HouseId == houseId)` reads through `UNIQUE (house_Id, obj_Cell_Id)`.
    // ACE: WorldDatabaseWithEntityCache.GetCachedHousePortals
    #[must_use]
    pub fn get_cached_house_portals(&self, house_id: u32) -> Arc<Vec<HousePortal>> {
        if let Some(value) = lock(&self.c.cached_house_portals).get(&house_id) {
            return value.clone();
        }

        let mut results: Vec<HousePortal> = self
            .base
            .all::<HousePortal>(TableId::HOUSE_PORTAL)
            .into_iter()
            .filter(|p| p.house_id == house_id)
            .collect();
        results.sort_by_key(|p| p.obj_cell_id);
        let results = Arc::new(results);

        lock(&self.c.cached_house_portals).insert(house_id, results.clone());

        results
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedHousePortalsByLandblock
    #[must_use]
    pub fn get_cached_house_portals_by_landblock(
        &self,
        landblock_id: u32,
    ) -> Arc<Vec<HousePortal>> {
        if let Some(value) = lock(&self.c.cached_house_portals_by_landblock).get(&landblock_id) {
            return value.clone();
        }

        let results = Arc::new(
            self.base
                .all::<HousePortal>(TableId::HOUSE_PORTAL)
                .into_iter()
                .filter(|p| landblock_id == p.obj_cell_id >> 16)
                .collect::<Vec<_>>(),
        );

        lock(&self.c.cached_house_portals_by_landblock).insert(landblock_id, results.clone());

        results
    }

    // =====================================
    // LandblockInstance
    // =====================================

    /// Returns the number of LandblockInstances currently cached.
    // ACE: WorldDatabaseWithEntityCache.GetLandblockInstancesCacheCount
    #[must_use]
    pub fn get_landblock_instances_cache_count(&self) -> i32 {
        count(lock(&self.c.cached_landblock_instances).len())
    }

    /// Clears the cached landblock instances for all landblocks
    // ACE: WorldDatabaseWithEntityCache.ClearCachedLandblockInstances
    pub fn clear_cached_landblock_instances(&self) {
        lock(&self.c.cached_landblock_instances).clear();
    }

    /// Clears the cached landblock instances for a specific landblock
    // ACE: WorldDatabaseWithEntityCache.ClearCachedInstancesByLandblock
    pub fn clear_cached_instances_by_landblock(&self, landblock: u16) -> bool {
        lock(&self.c.cached_landblock_instances)
            .remove(&landblock)
            .is_some()
    }

    /// Returns statics spawn map and their links for the landblock
    // ACE: WorldDatabaseWithEntityCache.GetCachedInstancesByLandblock
    #[must_use]
    pub fn get_cached_instances_by_landblock(&self, landblock: u16) -> Arc<Vec<LandblockInstance>> {
        if let Some(value) = lock(&self.c.cached_landblock_instances).get(&landblock) {
            return value.clone();
        }

        let results = Arc::new(self.base.instances(landblock));

        lock(&self.c.cached_landblock_instances)
            .entry(landblock)
            .or_insert(results)
            .clone()
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedBasementHouseGuid
    #[must_use]
    pub fn get_cached_basement_house_guid(&self, landblock: u16) -> u32 {
        if let Some(value) = lock(&self.c.cached_basement_house_guids).get(&landblock) {
            return *value;
        }

        let result = self.base.instances(landblock).into_iter().find(|r| {
            r.weenie_class_id != 11730 /* Exclude House Portal */
                && r.weenie_class_id != 278 /* Exclude Door */
                && r.weenie_class_id != 568 /* Exclude Door (entry) */
                && !r.is_link_child
        });

        let Some(result) = result else { return 0 };

        lock(&self.c.cached_basement_house_guids).insert(landblock, result.guid);

        result.guid
    }

    // =====================================
    // PointsOfInterest
    // =====================================

    /// Retrieves all points of interest from the database and adds/updates the points of interest
    /// cache entries with every point of interest retrieved.
    // ACE: WorldDatabaseWithEntityCache.CacheAllPointsOfInterest
    pub fn cache_all_points_of_interest(&self) {
        let results = self
            .base
            .all::<PointsOfInterest>(TableId::POINTS_OF_INTEREST);

        let mut cache = lock(&self.c.cached_points_of_interest);
        for result in results {
            cache.insert(to_lower(&result.name), Some(Arc::new(result)));
        }
    }

    /// Returns the number of PointsOfInterest currently cached.
    // ACE: WorldDatabaseWithEntityCache.GetPointsOfInterestCacheCount
    #[must_use]
    pub fn get_points_of_interest_cache_count(&self) -> i32 {
        count(
            lock(&self.c.cached_points_of_interest)
                .values()
                .filter(|v| v.is_some())
                .count(),
        )
    }

    /// Returns a copy of the PointsOfInterest cache. ACE copies a `ConcurrentDictionary` keyed by
    /// string, whose order changes from run to run (randomised string hashing); this is sorted.
    // ACE: WorldDatabaseWithEntityCache.GetPointsOfInterestCache
    // DIVERGE: a sorted Vec copy instead of a ConcurrentDictionary copy (whose order is random per run).
    #[must_use]
    pub fn get_points_of_interest_cache(&self) -> Vec<(String, Option<Arc<PointsOfInterest>>)> {
        let mut v: Vec<(String, Option<Arc<PointsOfInterest>>)> =
            lock(&self.c.cached_points_of_interest)
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedPointOfInterest
    #[must_use]
    pub fn get_cached_point_of_interest(&self, name: &str) -> Option<Arc<PointsOfInterest>> {
        let name_to_lower = to_lower(name);

        if let Some(value) = lock(&self.c.cached_points_of_interest).get(&name_to_lower) {
            return value.clone();
        }

        let result = self
            .base
            .all::<PointsOfInterest>(TableId::POINTS_OF_INTEREST)
            .into_iter()
            .find(|r| sql_ci_eq(&to_lower(&r.name), &name_to_lower))
            .map(Arc::new);

        lock(&self.c.cached_points_of_interest).insert(name_to_lower, result.clone());
        result
    }

    // =====================================
    // Quest
    // =====================================

    // ACE: WorldDatabaseWithEntityCache.ClearCachedQuest
    pub fn clear_cached_quest(&self, quest_name: &str) -> bool {
        lock(&self.c.cached_quest).remove(quest_name).is_some()
    }

    /// The cache key is the name as given; the query compares under the MySQL collation, so a
    /// differently-cased name finds the same quest under a second key.
    // ACE: WorldDatabaseWithEntityCache.GetCachedQuest
    #[must_use]
    pub fn get_cached_quest(&self, quest_name: &str) -> Option<Arc<Quest>> {
        if let Some(quest) = lock(&self.c.cached_quest).get(quest_name) {
            return quest.clone();
        }

        let quest = self
            .base
            .all::<Quest>(TableId::QUEST)
            .into_iter()
            .find(|q| sql_ci_eq(&q.name, quest_name))
            .map(Arc::new);
        lock(&self.c.cached_quest).insert(quest_name.to_owned(), quest.clone());

        quest
    }

    // =====================================
    // Spell
    // =====================================

    /// This takes under 1 second to complete.
    // ACE: WorldDatabaseWithEntityCache.CacheAllSpells
    pub fn cache_all_spells(&self) {
        let results = self.base.all::<Spell>(TableId::SPELL);

        let mut cache = lock(&self.c.spell_cache);
        for mut result in results {
            // Not ACE: the retail corrections to the stored data (`crate::corrections`).
            crate::corrections::apply_spell_for(&mut result, self.base().pack().era());
            cache.insert(result.id, Some(Arc::new(result)));
        }
    }

    /// Returns the number of Spells currently cached.
    // ACE: WorldDatabaseWithEntityCache.GetSpellCacheCount
    #[must_use]
    pub fn get_spell_cache_count(&self) -> i32 {
        count(
            lock(&self.c.spell_cache)
                .values()
                .filter(|v| v.is_some())
                .count(),
        )
    }

    // ACE: WorldDatabaseWithEntityCache.ClearSpellCache
    pub fn clear_spell_cache(&self) {
        lock(&self.c.spell_cache).clear();
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedSpell
    #[must_use]
    pub fn get_cached_spell(&self, spell_id: u32) -> Option<Arc<Spell>> {
        if let Some(spell) = lock(&self.c.spell_cache).get(&spell_id) {
            return spell.clone();
        }

        let result = self
            .base
            .get::<Spell>(TableId::SPELL, u64::from(spell_id))
            .map(|mut s| {
                // Not ACE: the retail corrections to the stored data (`crate::corrections`).
                crate::corrections::apply_spell_for(&mut s, self.base.pack().era());
                s
            })
            .map(Arc::new);

        lock(&self.c.spell_cache).insert(spell_id, result.clone());
        result
    }

    // =====================================
    // TreasureDeath
    // =====================================

    /// This takes under 1 second to complete.
    // ACE: WorldDatabaseWithEntityCache.CacheAllTreasuresDeath
    pub fn cache_all_treasures_death(&self) {
        let results = self.base.all::<TreasureDeath>(TableId::TREASURE_DEATH);

        let mut cache = lock(&self.c.cached_death_treasure);
        for result in results {
            cache.insert(result.treasure_type, Some(Arc::new(result)));
        }
    }

    /// Returns the number of TreasureDeath currently cached.
    // ACE: WorldDatabaseWithEntityCache.GetDeathTreasureCacheCount
    #[must_use]
    pub fn get_death_treasure_cache_count(&self) -> i32 {
        count(
            lock(&self.c.cached_death_treasure)
                .values()
                .filter(|v| v.is_some())
                .count(),
        )
    }

    /// `FirstOrDefault(r => r.TreasureType == dataId)` through `treasureType_idx`: lowest id.
    // ACE: WorldDatabaseWithEntityCache.GetCachedDeathTreasure
    #[must_use]
    pub fn get_cached_death_treasure(&self, data_id: u32) -> Option<Arc<TreasureDeath>> {
        if let Some(value) = lock(&self.c.cached_death_treasure).get(&data_id) {
            return value.clone();
        }

        let result = self
            .base
            .all::<TreasureDeath>(TableId::TREASURE_DEATH)
            .into_iter()
            .find(|r| r.treasure_type == data_id)
            .map(Arc::new);

        lock(&self.c.cached_death_treasure).insert(data_id, result.clone());
        result
    }

    // =====================================
    // TreasureMaterial
    // =====================================

    // ACE: WorldDatabaseWithEntityCache.CacheAllTreasureMaterialBase
    pub fn cache_all_treasure_material_base(&self) {
        let mut table: DotNetDict<i32, DotNetDict<i32, Vec<TreasureMaterialBase>>> =
            DotNetDict::new();

        let results = self
            .base
            .all::<TreasureMaterialBase>(TableId::TREASURE_MATERIAL_BASE)
            .into_iter()
            .filter(|i| i.probability > 0.0);

        for result in results {
            let material_code =
                table.get_or_insert_with(as_i32(result.material_code), DotNetDict::new);
            let chances = material_code.get_or_insert_with(as_i32(result.tier), Vec::new);
            chances.push(result.ace_clone());
        }
        Self::treasure_material_base_normalize(&mut table);

        *lock(&self.c.cached_treasure_material_base) = Some(Arc::new(freeze(table)));
    }

    // ACE: WorldDatabaseWithEntityCache.TreasureMaterialBase_Normalize
    fn treasure_material_base_normalize(
        material_base: &mut DotNetDict<i32, DotNetDict<i32, Vec<TreasureMaterialBase>>>,
    ) {
        for (_material_code, tiers) in material_base.iter_mut() {
            for (_tier, list) in tiers.iter_mut() {
                normalize(list.iter_mut().map(|i| &mut i.probability));
            }
        }
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedTreasureMaterialBase
    #[must_use]
    pub fn get_cached_treasure_material_base(
        &self,
        material_code: i32,
        tier: i32,
    ) -> Option<Arc<Vec<TreasureMaterialBase>>> {
        if lock(&self.c.cached_treasure_material_base).is_none() {
            self.cache_all_treasure_material_base();
        }

        let cache = lock(&self.c.cached_treasure_material_base).clone()?;
        cache
            .get(&material_code)
            .and_then(|tiers| tiers.get(&tier))
            .cloned()
    }

    // ACE: WorldDatabaseWithEntityCache.CacheAllTreasureMaterialColor
    pub fn cache_all_treasure_material_color(&self) {
        let mut table: DotNetDict<i32, DotNetDict<i32, Vec<TreasureMaterialColor>>> =
            DotNetDict::new();

        let results = self
            .base
            .all::<TreasureMaterialColor>(TableId::TREASURE_MATERIAL_COLOR);

        for result in results {
            let color_codes = table.get_or_insert_with(as_i32(result.material_id), DotNetDict::new);
            let list = color_codes.get_or_insert_with(as_i32(result.color_code), Vec::new);
            list.push(result.ace_clone());
        }

        Self::treasure_material_color_normalize(&mut table);

        *lock(&self.c.cached_treasure_material_color) = Some(Arc::new(freeze(table)));
    }

    // ACE: WorldDatabaseWithEntityCache.TreasureMaterialColor_Normalize
    fn treasure_material_color_normalize(
        material_color: &mut DotNetDict<i32, DotNetDict<i32, Vec<TreasureMaterialColor>>>,
    ) {
        for (_material, color_codes) in material_color.iter_mut() {
            for (_color_code, list) in color_codes.iter_mut() {
                normalize(list.iter_mut().map(|i| &mut i.probability));
            }
        }
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedTreasureMaterialColors
    #[must_use]
    pub fn get_cached_treasure_material_colors(
        &self,
        material_id: i32,
        tsys_color_code: i32,
    ) -> Option<Arc<Vec<TreasureMaterialColor>>> {
        if lock(&self.c.cached_treasure_material_color).is_none() {
            self.cache_all_treasure_material_color();
        }

        let cache = lock(&self.c.cached_treasure_material_color).clone()?;
        cache
            .get(&material_id)
            .and_then(|codes| codes.get(&tsys_color_code))
            .cloned()
    }

    // ACE: WorldDatabaseWithEntityCache.CacheAllTreasureMaterialGroups
    pub fn cache_all_treasure_material_groups(&self) {
        let mut table: DotNetDict<i32, DotNetDict<i32, Vec<TreasureMaterialGroups>>> =
            DotNetDict::new();

        let results = self
            .base
            .all::<TreasureMaterialGroups>(TableId::TREASURE_MATERIAL_GROUPS);

        for result in results {
            let tiers = table.get_or_insert_with(as_i32(result.material_group), DotNetDict::new);
            let list = tiers.get_or_insert_with(as_i32(result.tier), Vec::new);
            list.push(result.ace_clone());
        }
        Self::treasure_material_groups_normalize(&mut table);

        *lock(&self.c.cached_treasure_material_groups) = Some(Arc::new(freeze(table)));
    }

    // ACE: WorldDatabaseWithEntityCache.TreasureMaterialGroups_Normalize
    fn treasure_material_groups_normalize(
        material_groups: &mut DotNetDict<i32, DotNetDict<i32, Vec<TreasureMaterialGroups>>>,
    ) {
        for (_material_group, tiers) in material_groups.iter_mut() {
            for (_tier, list) in tiers.iter_mut() {
                normalize(list.iter_mut().map(|i| &mut i.probability));
            }
        }
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedTreasureMaterialGroup
    #[must_use]
    pub fn get_cached_treasure_material_group(
        &self,
        material_group: i32,
        tier: i32,
    ) -> Option<Arc<Vec<TreasureMaterialGroups>>> {
        if lock(&self.c.cached_treasure_material_groups).is_none() {
            self.cache_all_treasure_material_groups();
        }

        let cache = lock(&self.c.cached_treasure_material_groups).clone()?;
        cache
            .get(&material_group)
            .and_then(|tiers| tiers.get(&tier))
            .cloned()
    }

    // =====================================
    // TreasureWielded
    // =====================================

    /// This takes under 1 second to complete.
    // ACE: WorldDatabaseWithEntityCache.CacheAllTreasureWielded
    pub fn cache_all_treasure_wielded(&self) {
        let mut groups: Vec<(u32, Vec<TreasureWielded>)> = Vec::new();
        for r in self.base.all::<TreasureWielded>(TableId::TREASURE_WIELDED) {
            match groups.iter_mut().find(|(k, _)| *k == r.treasure_type) {
                Some((_, v)) => v.push(r),
                None => groups.push((r.treasure_type, vec![r])),
            }
        }

        let mut cache = lock(&self.c.cached_wielded_treasure);
        for (key, list) in groups {
            cache.insert(key, Arc::new(list));
        }
    }

    /// Returns the number of TreasureWielded currently cached.
    // ACE: WorldDatabaseWithEntityCache.GetWieldedTreasureCacheCount
    #[must_use]
    pub fn get_wielded_treasure_cache_count(&self) -> i32 {
        count(lock(&self.c.cached_wielded_treasure).len())
    }

    // ACE: WorldDatabaseWithEntityCache.GetCachedWieldedTreasure
    #[must_use]
    pub fn get_cached_wielded_treasure(&self, data_id: u32) -> Arc<Vec<TreasureWielded>> {
        if let Some(value) = lock(&self.c.cached_wielded_treasure).get(&data_id) {
            return value.clone();
        }

        let results = Arc::new(
            self.base
                .all::<TreasureWielded>(TableId::TREASURE_WIELDED)
                .into_iter()
                .filter(|r| r.treasure_type == data_id)
                .collect::<Vec<_>>(),
        );

        lock(&self.c.cached_wielded_treasure).insert(data_id, results.clone());
        results
    }

    // ACE: WorldDatabaseWithEntityCache.ClearWieldedTreasureCache
    pub fn clear_wielded_treasure_cache(&self) {
        lock(&self.c.cached_wielded_treasure).clear();
    }

    // ---- inherited, unchanged ----

    // ACE: WorldDatabase.Exists
    #[must_use]
    pub fn exists(&self, retry_until_found: bool) -> bool {
        self.base.exists(retry_until_found)
    }
    // ACE: WorldDatabase.GetAllWeenieNames
    #[must_use]
    pub fn get_all_weenie_names(&self) -> DotNetDict<u32, String> {
        self.base.get_all_weenie_names()
    }
    // ACE: WorldDatabase.GetAllWeenieClassNames
    #[must_use]
    pub fn get_all_weenie_class_names(&self) -> DotNetDict<u32, String> {
        self.base.get_all_weenie_class_names()
    }
    // ACE: WorldDatabase.GetHousesAll
    #[must_use]
    pub fn get_houses_all(&self) -> Vec<HouseListResults> {
        self.base.get_houses_all()
    }
    // ACE: WorldDatabase.GetLandblockInstanceByGuid
    #[must_use]
    pub fn get_landblock_instance_by_guid(&self, guid: u32) -> Option<LandblockInstance> {
        self.base.get_landblock_instance_by_guid(guid)
    }
    // ACE: WorldDatabase.GetLandblockInstancesByWcid
    #[must_use]
    pub fn get_landblock_instances_by_wcid(&self, wcid: u32) -> Vec<LandblockInstance> {
        self.base.get_landblock_instances_by_wcid(wcid)
    }
    // ACE: WorldDatabase.GetAllSpellNames
    #[must_use]
    pub fn get_all_spell_names(&self) -> DotNetDict<u32, String> {
        self.base.get_all_spell_names()
    }
    // ACE: WorldDatabase.GetAllTreasureDeath
    #[must_use]
    pub fn get_all_treasure_death(&self) -> DotNetDict<u32, TreasureDeath> {
        self.base.get_all_treasure_death()
    }
    /// Not ACE: `GemCountChance`'s static-constructor query (every `treasure_gem_count` row, by id).
    #[must_use]
    pub fn get_all_treasure_gem_count(&self) -> Vec<TreasureGemCount> {
        self.base.get_all_treasure_gem_count()
    }
    // ACE: WorldDatabase.GetAllTreasureWielded
    #[must_use]
    pub fn get_all_treasure_wielded(&self) -> DotNetDict<u32, Vec<TreasureWielded>> {
        self.base.get_all_treasure_wielded()
    }
    // ACE: WorldDatabase.GetVersion
    #[must_use]
    pub fn get_version(&self) -> Option<Version> {
        self.base.get_version()
    }
    // ACE: WorldDatabase.IsWorldDatabaseGuidRangeValid
    #[must_use]
    pub fn is_world_database_guid_range_valid(&self) -> bool {
        self.base.is_world_database_guid_range_valid()
    }
}

/// `list.Sum(i => i.Probability)` (LINQ sums `float`s in a `double` and narrows the result), then
/// scale by `1.0f / total` unless already within `NormalizeEpsilon` of 1.
fn normalize<'a>(probabilities: impl Iterator<Item = &'a mut f32>) {
    let list: Vec<&'a mut f32> = probabilities.collect();
    #[allow(clippy::cast_possible_truncation)]
    let total_probability = list.iter().fold(0.0f64, |s, p| s + f64::from(**p)) as f32;

    if (1.0f32 - total_probability).abs() < NORMALIZE_EPSILON {
        return;
    }

    let factor = 1.0f32 / total_probability;

    for item in list {
        *item *= factor;
    }
}

fn freeze<T>(mut table: DotNetDict<i32, DotNetDict<i32, Vec<T>>>) -> Tiered<T> {
    let keys: Vec<i32> = table.keys().copied().collect();
    keys.into_iter()
        .map(|k| {
            let mut inner = table.remove(&k).expect("key listed");
            let inner_keys: Vec<i32> = inner.keys().copied().collect();
            let m = inner_keys
                .into_iter()
                .map(|i| (i, Arc::new(inner.remove(&i).expect("key listed"))))
                .collect();
            (k, m)
        })
        .collect()
}

/// C# `(int)uint`: the bit pattern.
#[allow(clippy::cast_possible_wrap)]
fn as_i32(v: u32) -> i32 {
    v as i32
}

fn count(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

fn len_i32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

#[allow(clippy::cast_sign_loss)]
fn idx(i: i32) -> usize {
    i as usize
}
