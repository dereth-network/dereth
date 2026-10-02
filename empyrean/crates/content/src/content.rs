//! The public world-content API: the [`WorldDatabase`] trait (ACE's `WorldDatabaseWithEntityCache`
//! public surface, method for method) and its two implementations, [`PackContent`] over the
//! memory-mapped `world.pack` and [`MemContent`], an in-memory builder for tests.
//!
//! Both implementations are the same ported code: `MemContent` lays its rows out as a pack in
//! memory on first use, so a test exercises exactly the path production takes.

use std::sync::{Arc, OnceLock};

use empyrean_common::dotnet::DotNetDict;
use empyrean_common::era::EraId;

use crate::entity::HouseListResults;
use crate::import::WorldContent;
use crate::models::world::*;
use crate::overlay::ContentOverlay;
use crate::pack::Pack;
use crate::world_database_with_entity_cache::WorldDatabaseWithEntityCache;

type EntityWeenie = empyrean_entity::Weenie;

/// ACE's `WorldDatabaseWithEntityCache` over `world.pack`.
pub type PackContent = WorldDatabaseWithEntityCache;

macro_rules! world_database_api {
    ($( $(#[$m:meta])* fn $name:ident(&self $(, $arg:ident: $ty:ty)*) $(-> $ret:ty)?; )*) => {
        /// ACE's `DatabaseManager.World` (`WorldDatabaseWithEntityCache`, including what it inherits
        /// from `WorldDatabase`), with ACE's method names in snake case. C# overloads get a suffix
        /// (`get_weenie_by_class_name`). Rows are `crate::models::world` types; cached weenies are
        /// converted [`empyrean_entity::Weenie`]s. See the inherent methods of
        /// [`WorldDatabaseWithEntityCache`] for each method's notes.
        pub trait WorldDatabase: Send + Sync + std::fmt::Debug {
            $( $(#[$m])* fn $name(&self $(, $arg: $ty)*) $(-> $ret)?; )*

            /// Not ACE: the editable content overlay in front of the pack, when one is attached.
            /// ACE's developer content commands write through it.
            fn overlay(&self) -> Option<Arc<ContentOverlay>> {
                None
            }

            /// Not ACE: the pack's BLAKE3 content hash, as hex; `None` for content with no pack.
            /// With [`crate::corrections::digest`] it names the world the game sees.
            fn content_hash(&self) -> Option<String> {
                None
            }

            /// Not ACE: the era the content was built for (the pack's header).
            fn era(&self) -> EraId {
                EraId::Eor
            }

            /// Not ACE: the class ids of every weenie of one type, in class id order.
            fn weenie_class_ids_of_type(&self, _weenie_type_id: i32) -> Vec<u32> {
                Vec::new()
            }
        }

        impl WorldDatabase for WorldDatabaseWithEntityCache {
            $( fn $name(&self $(, $arg: $ty)*) $(-> $ret)? { WorldDatabaseWithEntityCache::$name(self $(, $arg)*) } )*

            fn overlay(&self) -> Option<Arc<ContentOverlay>> {
                self.base().overlay().cloned()
            }

            fn content_hash(&self) -> Option<String> {
                Some(crate::pack::hex(&self.base().pack().header().content_hash))
            }

            fn era(&self) -> EraId {
                self.base().pack().era()
            }

            fn weenie_class_ids_of_type(&self, weenie_type_id: i32) -> Vec<u32> {
                let mut ids: Vec<u32> = self.base()
                    .weenie_index()
                    .into_iter()
                    .filter(|(_, i)| i.r#type == weenie_type_id)
                    .map(|(k, _)| k)
                    .collect();
                ids.sort_unstable();
                ids
            }
        }

        impl WorldDatabase for MemContent {
            $( fn $name(&self $(, $arg: $ty)*) $(-> $ret)? { self.db().$name($($arg),*) } )*

            fn overlay(&self) -> Option<Arc<ContentOverlay>> {
                self.db().base().overlay().cloned()
            }

            fn content_hash(&self) -> Option<String> {
                Some(crate::pack::hex(&self.db().base().pack().header().content_hash))
            }

            fn era(&self) -> EraId {
                self.db().base().pack().era()
            }

            fn weenie_class_ids_of_type(&self, weenie_type_id: i32) -> Vec<u32> {
                let mut ids: Vec<u32> = self.db().base()
                    .weenie_index()
                    .into_iter()
                    .filter(|(_, i)| i.r#type == weenie_type_id)
                    .map(|(k, _)| k)
                    .collect();
                ids.sort_unstable();
                ids
            }
        }
    };
}

world_database_api! {
    fn exists(&self, retry_until_found: bool) -> bool;

    // Weenie
    fn get_weenie(&self, weenie_class_id: u32) -> Option<Weenie>;
    fn get_weenie_by_class_name(&self, weenie_class_name: &str) -> Option<Weenie>;
    fn get_all_weenies(&self) -> Vec<Weenie>;
    fn cache_all_weenies(&self);
    fn get_weenie_cache_count(&self) -> i32;
    fn clear_weenie_cache(&self);
    fn get_cached_weenie(&self, weenie_class_id: u32) -> Option<Arc<EntityWeenie>>;
    fn get_cached_weenie_by_class_name(&self, weenie_class_name: &str) -> Option<Arc<EntityWeenie>>;
    fn clear_cached_weenie(&self, weenie_class_id: u32) -> bool;
    fn get_random_weenies_of_type(&self, weenie_type_id: i32, count: i32) -> Vec<Option<Arc<EntityWeenie>>>;
    fn get_scroll_weenie(&self, spell_id: u32) -> Option<Arc<EntityWeenie>>;
    fn is_creature_name_in_world_database(&self, name: &str) -> bool;
    fn get_all_weenie_names(&self) -> DotNetDict<u32, String>;
    fn get_all_weenie_class_names(&self) -> DotNetDict<u32, String>;
    fn get_houses_all(&self) -> Vec<HouseListResults>;

    // CookBook and Recipe
    fn get_cookbook(&self, source_weenie_class_id: u32, target_weenie_class_id: u32) -> Option<Arc<CookBook>>;
    fn get_all_cookbooks(&self) -> Vec<Arc<CookBook>>;
    fn cache_all_cookbooks(&self);
    fn get_cookbook_cache_count(&self) -> i32;
    fn clear_cookbook_cache(&self);
    fn get_cached_cookbook(&self, source_weenie_class_id: u32, target_weenie_class_id: u32) -> Option<Arc<CookBook>>;
    fn get_cookbooks_by_recipe_id(&self, recipe_id: u32) -> Vec<Option<Arc<CookBook>>>;
    fn get_cached_recipe(&self, recipe_id: u32) -> Option<Arc<Recipe>>;
    fn get_recipe(&self, recipe_id: u32) -> Option<Arc<Recipe>>;

    // Encounter
    fn get_encounter_cache_count(&self) -> i32;
    fn get_cached_encounters_by_landblock(&self, landblock: u16) -> Arc<Vec<Encounter>>;
    fn clear_cached_encounters_by_landblock(&self, landblock: u16) -> bool;

    // Event
    fn clear_cached_event(&self, event_name: &str) -> bool;
    fn get_all_events(&self) -> Vec<Arc<Event>>;
    fn get_events_cache_count(&self) -> i32;
    fn get_cached_event(&self, name: &str) -> Option<Arc<Event>>;

    // HousePortal
    fn cache_all_house_portals(&self);
    fn get_cached_house_portals(&self, house_id: u32) -> Arc<Vec<HousePortal>>;
    fn get_cached_house_portals_by_landblock(&self, landblock_id: u32) -> Arc<Vec<HousePortal>>;

    // LandblockInstance
    fn get_landblock_instances_cache_count(&self) -> i32;
    fn clear_cached_landblock_instances(&self);
    fn clear_cached_instances_by_landblock(&self, landblock: u16) -> bool;
    fn get_cached_instances_by_landblock(&self, landblock: u16) -> Arc<Vec<LandblockInstance>>;
    fn get_cached_basement_house_guid(&self, landblock: u16) -> u32;
    fn get_landblock_instance_by_guid(&self, guid: u32) -> Option<LandblockInstance>;
    fn get_landblock_instances_by_wcid(&self, wcid: u32) -> Vec<LandblockInstance>;
    fn is_world_database_guid_range_valid(&self) -> bool;

    // PointsOfInterest
    fn cache_all_points_of_interest(&self);
    fn get_points_of_interest_cache_count(&self) -> i32;
    fn get_points_of_interest_cache(&self) -> Vec<(String, Option<Arc<PointsOfInterest>>)>;
    fn get_cached_point_of_interest(&self, name: &str) -> Option<Arc<PointsOfInterest>>;

    // Quest
    fn clear_cached_quest(&self, quest_name: &str) -> bool;
    fn get_cached_quest(&self, quest_name: &str) -> Option<Arc<Quest>>;

    // Spell
    fn cache_all_spells(&self);
    fn get_spell_cache_count(&self) -> i32;
    fn clear_spell_cache(&self);
    fn get_cached_spell(&self, spell_id: u32) -> Option<Arc<Spell>>;
    fn get_all_spell_names(&self) -> DotNetDict<u32, String>;

    // Treasure
    fn cache_all_treasures_death(&self);
    fn get_death_treasure_cache_count(&self) -> i32;
    fn get_cached_death_treasure(&self, data_id: u32) -> Option<Arc<TreasureDeath>>;
    fn get_all_treasure_death(&self) -> DotNetDict<u32, TreasureDeath>;
    fn cache_all_treasure_material_base(&self);
    fn get_cached_treasure_material_base(&self, material_code: i32, tier: i32) -> Option<Arc<Vec<TreasureMaterialBase>>>;
    fn cache_all_treasure_material_color(&self);
    fn get_cached_treasure_material_colors(&self, material_id: i32, tsys_color_code: i32) -> Option<Arc<Vec<TreasureMaterialColor>>>;
    fn cache_all_treasure_material_groups(&self);
    fn get_cached_treasure_material_group(&self, material_group: i32, tier: i32) -> Option<Arc<Vec<TreasureMaterialGroups>>>;
    fn cache_all_treasure_wielded(&self);
    fn get_wielded_treasure_cache_count(&self) -> i32;
    fn get_cached_wielded_treasure(&self, data_id: u32) -> Arc<Vec<TreasureWielded>>;
    fn clear_wielded_treasure_cache(&self);
    fn get_all_treasure_wielded(&self) -> DotNetDict<u32, Vec<TreasureWielded>>;
    /// Not ACE: the `treasure_gem_count` rows `GemCountChance`'s static constructor reads.
    fn get_all_treasure_gem_count(&self) -> Vec<TreasureGemCount>;

    // Version
    fn get_version(&self) -> Option<Version>;
}

/// An in-memory world database for tests:
/// `MemContent::new().weenie(w).landblock_instance(i)`. Rows are laid out as a pack on first use;
/// adding a row afterwards starts a fresh database (and fresh caches).
#[derive(Debug, Default)]
pub struct MemContent {
    content: WorldContent,
    db: OnceLock<WorldDatabaseWithEntityCache>,
}

macro_rules! adders {
    ($($(#[$m:meta])* $name:ident: $ty:ty => $field:ident;)*) => {
        impl MemContent {
            $(
                $(#[$m])*
                #[must_use]
                pub fn $name(mut self, row: $ty) -> Self {
                    self.content.$field.push(row);
                    self.db = OnceLock::new();
                    self
                }
            )*
        }
    };
}

adders! {
    /// A weenie with its property rows (see [`Weenie::new`] and its `with_*` helpers).
    weenie: Weenie => weenies;
    /// A landblock instance with its links; `landblock` is computed from `obj_cell_id`.
    landblock_instance: LandblockInstance => landblock_instances;
    encounter: Encounter => encounters;
    /// A cook book row; its recipe is joined from [`MemContent::recipe`] rows at query time.
    cook_book: CookBook => cook_books;
    recipe: Recipe => recipes;
    event: Event => events;
    house_portal: HousePortal => house_portals;
    point_of_interest: PointsOfInterest => points_of_interest;
    quest: Quest => quests;
    spell: Spell => spells;
    treasure_death: TreasureDeath => treasure_death;
    treasure_gem_count: TreasureGemCount => treasure_gem_count;
    treasure_material_base: TreasureMaterialBase => treasure_material_base;
    treasure_material_color: TreasureMaterialColor => treasure_material_color;
    treasure_material_groups: TreasureMaterialGroups => treasure_material_groups;
    treasure_wielded: TreasureWielded => treasure_wielded;
    version: Version => versions;
}

impl MemContent {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The era the content is built for (end of retail unless set).
    #[must_use]
    pub fn era(mut self, era: EraId) -> Self {
        self.content.era = era;
        self.db = OnceLock::new();
        self
    }

    /// A database over already-assembled content (the importer's output, for example).
    #[must_use]
    pub fn from_content(content: WorldContent) -> Self {
        Self {
            content,
            db: OnceLock::new(),
        }
    }

    #[must_use]
    pub fn content(&self) -> &WorldContent {
        &self.content
    }

    /// The ported database over this content.
    ///
    /// # Panics
    /// When two rows share a key (two weenies with one class id, for example).
    pub fn db(&self) -> &WorldDatabaseWithEntityCache {
        self.db.get_or_init(|| {
            let (bytes, _) = self
                .content
                .to_pack([0; 16])
                .unwrap_or_else(|e| panic!("MemContent: {e}"));
            WorldDatabaseWithEntityCache::new(
                Pack::from_bytes(bytes).expect("a pack the writer just laid out"),
            )
        })
    }
}
