// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory.cs`.
//!
//! Random loot: a treasure-death profile rolls item types and wcids, each item is created and
//! mutated (material, colour, gems, workmanship, burden, spells, value, ...). The other partial
//! files of ACE's `LootGenerationFactory` are the sibling `loot_generation_factory_*.rs` modules;
//! the `Factories/Tables` members this needs are in [`tables_logic`].
//!
//! **Shape.** Items are built as owned [`WorldObject`]s that are not in `World.objects` yet, as
//! ACE's are not on any landblock yet: creating one takes `&mut World` (a dynamic guid), mutating
//! it takes `&World` (content, dats, properties) and `&mut WorldObject`. The caller adds the
//! returned items to the store (a corpse, a chest, a player's pack).
//!
//! **Draw order.** Every `ThreadSafeRandom` draw is made where ACE makes it, including the one
//! each constructor makes (`InitializeHeartbeats`); the `loot` and `lootgen` vector areas replay
//! ACE's own factory draw for draw.
//!
//! **Forwarders** (members of other classes, called by their ACE names): [`has_mutate_filter`]
//! (`MutateFilters.HasMutateFilter`) and [`world_object_factory_create_new_world_object`]
//! (`WorldObjectFactory.CreateNewWorldObject(uint)`). `Spell.Level`, `BaseMana` and
//! `Formula.Level` are 5.1a's [`Spell`].

use std::sync::LazyLock;

use empyrean_common::dotnet::math::round;
use empyrean_common::dotnet::CsCast;
use empyrean_common::era::LootTables;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::{TreasureDeath, TreasureMaterialColor};
use empyrean_dat::file_types::ClothingTable;
use empyrean_entity::enums::{
    ItemType, MaterialType, MutateFilter, PropertyString, SpellId, WeenieType, WieldRequirement,
};
use empyrean_tables::enums::{
    LootBias, TreasureArmorType, TreasureItemCategory, TreasureItemType, TreasureWeaponType,
    WeenieClassName,
};
use empyrean_tables::logic::tables::{
    armor_type_chance, gem_class_chance, gem_material_chance, material_table,
    spell_level_progression, treasure_profile_item, treasure_profile_magic_item,
    treasure_profile_mundane, weapon_type_chance, workmanship_chance,
};
use empyrean_tables::logic::wcids::{
    aetheria_wcids, armor_wcids as armor_wcids_lookup, cloak_wcids,
    clothing_wcids as clothing_wcids_lookup, generic_wcids, jewelry_wcids,
    pet_device_wcids as pet_device_wcids_lookup, scroll_wcids,
    society_armor_wcids as society_armor_wcids_lookup,
};
use empyrean_tables::logic::weapons::{
    atlatl_wcids, bow_wcids_aluvian, bow_wcids_gharundim, bow_wcids_sho, caster_wcids,
    crossbow_wcids, finesse_weapon_wcids, heavy_weapon_wcids, light_weapon_wcids,
    two_handed_weapon_wcids,
};
use empyrean_tables::tables::caster_slot_spells::DESCRIPTORS;
use empyrean_tables::tables::treasure_item_type_chances;

#[path = "tables_logic/mod.rs"]
pub mod tables_logic;

use self::tables_logic::wcids::{
    armor_wcids, clothing_wcids, consume_wcids, heal_kit_wcids, lockpick_wcids, mana_stone_wcids,
    pet_device_wcids, society_armor_wcids, spell_component_wcids, weapon_wcids,
};
use crate::entity::spell::Spell;
use crate::factories::entity::treasure_roll::TreasureRoll;
use crate::factories::loot_generation_factory_aetheria::{
    create_aetheria, create_coalesced_mana, mutate_aetheria,
};
use crate::factories::loot_generation_factory_caster::mutate_caster;
use crate::factories::loot_generation_factory_clothing::{
    create_armor, create_cloak, mutate_armor, mutate_cloak,
};
use crate::factories::loot_generation_factory_dinnerware::{create_dinnerware, mutate_dinnerware};
use crate::factories::loot_generation_factory_gem::{create_gem, mutate_gem, mutate_value_gem};
use crate::factories::loot_generation_factory_jewelry::{create_jewelry, mutate_jewelry};
use crate::factories::loot_generation_factory_melee::mutate_melee_weapon;
use crate::factories::loot_generation_factory_missile::mutate_missile_weapon;
use crate::factories::loot_generation_factory_pet_device::mutate_pet_device;
use crate::factories::loot_generation_factory_scroll::create_random_scroll;
use crate::factories::loot_generation_factory_weapon::create_weapon;
use crate::factories::loot_tables;
use crate::factories::world_object_factory;
use crate::managers::property_manager;
use crate::world_objects::world_object::WorldObject;
use crate::World;

// Used for cumulative ServerPerformanceMonitor event recording
//private static readonly ThreadLocal<Stopwatch> stopwatch = new ThreadLocal<Stopwatch>(() => new Stopwatch());

/// The static constructor (`InitRares`, `InitClothingColors`): here the two tables are
/// `LazyLock`s built on first use ([`CLOTHING_COLORS`] and the rare wcids of
/// `loot_generation_factory_rare`), which is when .NET would run it.
// ACE: LootGenerationFactory.LootGenerationFactory
fn loot_generation_factory() {
    LazyLock::force(&CLOTHING_COLORS);
    crate::factories::loot_generation_factory_rare::init_rares();
}

/// Rolls a corpse's loot: the item, magic item and mundane item groups in turn (each a chance,
/// then a count), plus the mundane add-on (coalesced mana or aetheria).
// ACE: LootGenerationFactory.CreateRandomLootObjects
pub fn create_random_loot_objects(w: &mut World, profile: &TreasureDeath) -> Vec<WorldObject> {
    //stopwatch.Value.Restart();

    let mut loot = Vec::new();

    let mut item_chance = ThreadSafeRandom::next(1, 100);
    if item_chance <= profile.item_chance {
        let num_items = ThreadSafeRandom::next(profile.item_min_amount, profile.item_max_amount);

        for _ in 0..num_items {
            let loot_world_object = create_random_loot_objects_of_category(
                w,
                profile,
                TreasureItemCategory::Item,
                TreasureItemType::Undef,
            );

            if let Some(o) = loot_world_object {
                loot.push(o);
            }
        }
    }

    item_chance = ThreadSafeRandom::next(1, 100);
    if item_chance <= profile.magic_item_chance {
        let num_items =
            ThreadSafeRandom::next(profile.magic_item_min_amount, profile.magic_item_max_amount);

        for _ in 0..num_items {
            let loot_world_object = create_random_loot_objects_of_category(
                w,
                profile,
                TreasureItemCategory::MagicItem,
                TreasureItemType::Undef,
            );

            if let Some(o) = loot_world_object {
                loot.push(o);
            }
        }
    }

    item_chance = ThreadSafeRandom::next(1, 100);
    if item_chance <= profile.mundane_item_chance {
        let num_items = ThreadSafeRandom::next(
            profile.mundane_item_min_amount,
            profile.mundane_item_max_amount,
        );

        for _ in 0..num_items {
            let loot_world_object = create_random_loot_objects_of_category(
                w,
                profile,
                TreasureItemCategory::MundaneItem,
                TreasureItemType::Undef,
            );

            if let Some(o) = loot_world_object {
                loot.push(o);
            }
        }

        // extra roll for mundane:
        // https://asheron.fandom.com/wiki/Announcements_-_2010/04_-_Shedding_Skin :: May 5th, 2010 entry
        // aetheria and coalesced mana were handled in here
        let loot_world_object = try_roll_mundane_addon(w, profile);

        if let Some(o) = loot_world_object {
            loot.push(o);
        }
    }

    loot

    //ServerPerformanceMonitor.AddToCumulativeEvent(ServerPerformanceMonitor.CumulativeEventHistoryType.LootGenerationFactory_CreateRandomLootObjects, stopwatch.Value.Elapsed.TotalSeconds);
}

/// Not ACE: how many times the era's `LootTables::PackOnly` rule rolls again for a weenie the world
/// database lacks, before the roll is left to fail as ACE's does.
const PACK_ONLY_REROLLS: usize = 64;

/// Not ACE: whether the world database has the weenie `wcid`. The first time a wcid is found
/// missing it is logged, once for the world.
pub(crate) fn world_has_weenie(w: &World, wcid: u32) -> bool {
    if wcid != 0 && w.content.get_cached_weenie(wcid).is_some() {
        return true;
    }
    let first = w
        .loot_tables
        .missing_weenies
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(wcid);
    if first {
        log::info!(
            "Loot: the world database has no weenie {wcid}; loot that rolls it rolls again (era {})",
            w.era.id
        );
    }
    false
}

/// `CreateRandomLootObjects(TreasureDeath, TreasureItemCategory, TreasureItemType = Undef)`: one
/// item of the category (or of the given type), created and mutated.
pub fn create_random_loot_objects_of_category(
    w: &mut World,
    treasure_death: &TreasureDeath,
    category: TreasureItemCategory,
    treasure_item_type: TreasureItemType,
) -> Option<WorldObject> {
    let mut treasure_roll = roll_wcid(treasure_death, category, treasure_item_type)?;

    // DIVERGE: under the era's `LootTables::PackOnly` rule a rolled weenie the world database lacks
    // is rolled again, so a world older than the loot tables drops what it has. ACE (and the end
    // of retail, whose world has every weenie the tables name) creates nothing and logs an error.
    if w.era.loot == LootTables::PackOnly {
        let mut rerolls = 0;
        while treasure_roll.item_type != TreasureItemType::Scroll
            && rerolls < PACK_ONLY_REROLLS
            && !world_has_weenie(w, treasure_roll.wcid.0.cast_unsigned())
        {
            treasure_roll = roll_wcid(treasure_death, category, treasure_item_type)?;
            rerolls += 1;
        }
    }

    create_and_mutate_wcid(
        w,
        treasure_death,
        &mut treasure_roll,
        category == TreasureItemCategory::MagicItem,
    )
}

/// The item type (rolled from the profile's category column unless given) and its wcid.
/// `pub` for the vectors (ACE: private).
// ACE: LootGenerationFactory.RollWcid
#[must_use]
pub fn roll_wcid(
    treasure_death: &TreasureDeath,
    category: TreasureItemCategory,
    mut treasure_item_type: TreasureItemType,
) -> Option<TreasureRoll> {
    if treasure_item_type == TreasureItemType::Undef {
        treasure_item_type = roll_item_type(treasure_death, category);
    }

    if treasure_item_type == TreasureItemType::Undef {
        log::error!(
            "LootGenerationFactory.RollWcid({}, {}): treasureItemType == Undef",
            treasure_death.treasure_type,
            category
        );
        return None;
    }

    let mut treasure_roll = TreasureRoll::with_item_type(treasure_item_type);

    // TODO: quality mod
    match treasure_item_type {
        TreasureItemType::Pyreal => {
            treasure_roll.wcid = WeenieClassName::coinstack;
        }

        TreasureItemType::Gem => {
            let gem_class = gem_class_chance::roll(treasure_death.tier);
            let gem_result = gem_material_chance::roll(gem_class);

            treasure_roll.wcid = gem_result.class_name;
        }

        TreasureItemType::Jewelry => {
            treasure_roll.wcid = jewelry_wcids::roll(treasure_death.tier);
        }

        TreasureItemType::ArtObject => {
            treasure_roll.wcid = generic_wcids::roll(treasure_death.tier);
        }

        TreasureItemType::Weapon => {
            treasure_roll.weapon_type = weapon_type_chance::roll(treasure_death.tier);
            treasure_roll.wcid = weapon_wcids::roll(treasure_death, &mut treasure_roll.weapon_type);
        }

        TreasureItemType::Armor => {
            treasure_roll.armor_type = armor_type_chance::roll(treasure_death.tier);
            treasure_roll.wcid = armor_wcids::roll(treasure_death, &mut treasure_roll.armor_type);
        }

        TreasureItemType::Clothing => {
            treasure_roll.wcid = clothing_wcids::roll(treasure_death);
        }

        TreasureItemType::Scroll => {
            treasure_roll.wcid = scroll_wcids::roll();
        }

        TreasureItemType::Caster => {
            // only called if TreasureItemType.Caster was specified directly
            treasure_roll.weapon_type = TreasureWeaponType::Caster;
            treasure_roll.wcid = caster_wcids::roll(treasure_death.tier);
        }

        TreasureItemType::ManaStone => {
            treasure_roll.wcid = mana_stone_wcids::roll(treasure_death);
        }

        TreasureItemType::Consumable => {
            treasure_roll.wcid = consume_wcids::roll(treasure_death);
        }

        TreasureItemType::HealKit => {
            treasure_roll.wcid = heal_kit_wcids::roll(treasure_death);
        }

        TreasureItemType::Lockpick => {
            treasure_roll.wcid = lockpick_wcids::roll(treasure_death);
        }

        TreasureItemType::SpellComponent => {
            treasure_roll.wcid = spell_component_wcids::roll(treasure_death);
        }

        TreasureItemType::SocietyArmor
        | TreasureItemType::SocietyBreastplate
        | TreasureItemType::SocietyGauntlets
        | TreasureItemType::SocietyGirth
        | TreasureItemType::SocietyGreaves
        | TreasureItemType::SocietyHelm
        | TreasureItemType::SocietyPauldrons
        | TreasureItemType::SocietyTassets
        | TreasureItemType::SocietyVambraces
        | TreasureItemType::SocietySollerets => {
            treasure_roll.item_type = TreasureItemType::SocietyArmor; // collapse for mutation
            treasure_roll.armor_type = TreasureArmorType::Society;

            treasure_roll.wcid = society_armor_wcids::roll(treasure_death, treasure_item_type);
        }

        TreasureItemType::Cloak => {
            treasure_roll.wcid = cloak_wcids::roll();
        }

        TreasureItemType::PetDevice => {
            treasure_roll.wcid = pet_device_wcids::roll(treasure_death);
        }

        TreasureItemType::EncapsulatedSpirit => {
            treasure_roll.wcid = WeenieClassName::ace49485_encapsulatedspirit;
        }

        _ => {}
    }
    Some(treasure_roll)
}

/// Rolls for an overall item type, based on the `*_Chances` columns in the treasure_death profile.
// ACE: LootGenerationFactory.RollItemType
fn roll_item_type(
    treasure_death: &TreasureDeath,
    category: TreasureItemCategory,
) -> TreasureItemType {
    match category {
        TreasureItemCategory::Item => {
            treasure_profile_item::roll(treasure_death.item_treasure_type_selection_chances)
        }

        TreasureItemCategory::MagicItem => treasure_profile_magic_item::roll(
            treasure_death.magic_item_treasure_type_selection_chances,
        ),

        TreasureItemCategory::MundaneItem => {
            treasure_profile_mundane::roll(treasure_death.mundane_item_type_selection_chances)
        }

        _ => TreasureItemType::Undef,
    }
}

/// Creates the rolled wcid (a scroll is rolled and created by `CreateRandomScroll` instead) and
/// runs the item type's mutation.
// ACE: LootGenerationFactory.CreateAndMutateWcid
fn create_and_mutate_wcid(
    w: &mut World,
    treasure_death: &TreasureDeath,
    treasure_roll: &mut TreasureRoll,
    is_magical: bool,
) -> Option<WorldObject> {
    let mut wo = None;

    if treasure_roll.item_type != TreasureItemType::Scroll {
        wo = world_object_factory_create_new_world_object(w, treasure_roll.wcid.0.cast_unsigned());

        let Some(o) = wo.as_ref() else {
            log::error!(
                "CreateAndMutateWcid({}, {} - {}, {}, {}) - failed to create item",
                treasure_death.treasure_type,
                treasure_roll.wcid.0,
                treasure_roll.wcid,
                treasure_roll.get_item_type(),
                if is_magical { "True" } else { "False" }
            );
            return None;
        };

        treasure_roll.base_armor_level = o.armor_level().unwrap_or(0);
    }

    match treasure_roll.item_type {
        TreasureItemType::Pyreal => mutate_coins(expect_wo(&mut wo), treasure_death),
        TreasureItemType::Gem => mutate_gem(
            w,
            expect_wo(&mut wo),
            treasure_death,
            is_magical,
            treasure_roll,
        ),
        TreasureItemType::Jewelry => {
            let o = expect_wo(&mut wo);
            if treasure_roll.has_armor_level(o) {
                // crowns, coronets, diadems, etc.
                mutate_armor(w, o, treasure_death, is_magical, treasure_roll);
            } else {
                mutate_jewelry(w, o, treasure_death, is_magical, treasure_roll);
            }
        }
        TreasureItemType::ArtObject => {
            mutate_dinnerware(
                w,
                expect_wo(&mut wo),
                treasure_death,
                is_magical,
                treasure_roll,
            );
        }

        TreasureItemType::Weapon => match treasure_roll.weapon_type {
            TreasureWeaponType::Axe
            | TreasureWeaponType::Dagger
            | TreasureWeaponType::DaggerMS
            | TreasureWeaponType::Mace
            | TreasureWeaponType::MaceJitte
            | TreasureWeaponType::Spear
            | TreasureWeaponType::Staff
            | TreasureWeaponType::Sword
            | TreasureWeaponType::SwordMS
            | TreasureWeaponType::Unarmed
            | TreasureWeaponType::TwoHandedAxe
            | TreasureWeaponType::TwoHandedMace
            | TreasureWeaponType::TwoHandedSpear
            | TreasureWeaponType::TwoHandedSword => {
                mutate_melee_weapon(
                    w,
                    expect_wo(&mut wo),
                    treasure_death,
                    is_magical,
                    treasure_roll,
                );
            }

            TreasureWeaponType::Caster => {
                mutate_caster(
                    w,
                    expect_wo(&mut wo),
                    treasure_death,
                    is_magical,
                    treasure_roll,
                );
            }

            TreasureWeaponType::Bow | TreasureWeaponType::Crossbow | TreasureWeaponType::Atlatl => {
                mutate_missile_weapon(
                    w,
                    expect_wo(&mut wo),
                    treasure_death,
                    is_magical,
                    treasure_roll,
                );
            }

            _ => {
                log::error!(
                    "CreateAndMutateWcid({}, {} - {}, {}, {}) - unknown weapon type",
                    treasure_death.treasure_type,
                    treasure_roll.wcid.0,
                    treasure_roll.wcid,
                    treasure_roll.get_item_type(),
                    if is_magical { "True" } else { "False" }
                );
            }
        },

        TreasureItemType::Caster => {
            // alternate path -- only called if TreasureItemType.Caster was specified directly
            mutate_caster(
                w,
                expect_wo(&mut wo),
                treasure_death,
                is_magical,
                treasure_roll,
            );
        }

        TreasureItemType::Armor | TreasureItemType::Clothing | TreasureItemType::SocietyArmor => {
            // collapsed, after rolling for initial wcid
            mutate_armor(
                w,
                expect_wo(&mut wo),
                treasure_death,
                is_magical,
                treasure_roll,
            );
        }

        TreasureItemType::Scroll => {
            wo = create_random_scroll(w, treasure_death, Some(treasure_roll)); // using original method
        }

        TreasureItemType::Cloak => {
            mutate_cloak(w, expect_wo(&mut wo), treasure_death, Some(treasure_roll))
        }

        TreasureItemType::PetDevice => mutate_pet_device(expect_wo(&mut wo), treasure_death.tier),

        // other mundane items (mana stones, food/drink, healing kits, lockpicks, and spell components/peas) don't get mutated
        _ => {}
    }
    wo
}

/// The created item, for the mutation arms (created above unless the type is a scroll).
fn expect_wo(wo: &mut Option<WorldObject>) -> &mut WorldObject {
    wo.as_mut().expect("the item was created")
}

// ACE: LootGenerationFactory.TryRollMundaneAddon
fn try_roll_mundane_addon(w: &mut World, profile: &TreasureDeath) -> Option<WorldObject> {
    // coalesced mana only dropped in tiers 1-4
    if profile.tier <= 4 {
        try_roll_coalesced_mana(w, profile)
    }
    // aetheria dropped in tiers 5+
    else {
        try_roll_aetheria(w, profile)
    }
}

// ACE: LootGenerationFactory.TryRollCoalescedMana
fn try_roll_coalesced_mana(w: &mut World, profile: &TreasureDeath) -> Option<WorldObject> {
    // 2% chance in here, which turns out to be less per corpse w/ MundaneItemChance > 0,
    // when the outer MundaneItemChance roll is factored in

    // loot quality mod?
    let rng = ThreadSafeRandom::next_float(0.0, 1.0);

    if rng < f64::from(0.02f32) {
        create_coalesced_mana(w, profile)
    } else {
        None
    }
}

// ACE: LootGenerationFactory.TryRollAetheria
fn try_roll_aetheria(w: &mut World, profile: &TreasureDeath) -> Option<WorldObject> {
    let aetheria_drop_rate: f32 = property_manager::get_double(w, "aetheria_drop_rate", 0.0, true)
        .item
        .cs_cast();

    if aetheria_drop_rate <= 0.0 {
        return None;
    }

    let drop_rate_mod = 1.0f32 / aetheria_drop_rate;

    // 2% base chance in here, which turns out to be less per corpse w/ MundaneItemChance > 0,
    // when the outer MundaneItemChance roll is factored in

    // loot quality mod?
    let rng = ThreadSafeRandom::next_float(0.0, 1.0 * drop_rate_mod);

    if rng < f64::from(0.02f32) {
        create_aetheria(w, profile, true)
    } else {
        None
    }
}

/// Returns an appropriate material type for the World Object based on its loot tier.
// ACE: LootGenerationFactory.GetMaterialType
pub(crate) fn get_material_type(w: &World, wo: &WorldObject, tier: i32) -> MaterialType {
    let Some(tsys_mutation_data) = wo.tsys_mutation_data() else {
        log::warn!(
            "[LOOT] Missing PropertyInt.TsysMutationData on loot item {} - {}",
            wo.biota.weenie_class_id,
            wo_name(wo)
        );
        return get_default_material_type(Some(wo));
    };

    let material_code = tsys_mutation_data & 0xFF;

    // Enforce some bounds
    // Data only goes to Tier 6 at the moment... Just in case the loot gem goes above this first, we'll cap it here for now.
    let tier = tier.clamp(1, 6);

    let Some(material_base) = w
        .content
        .get_cached_treasure_material_base(material_code, tier)
    else {
        return get_default_material_type(Some(wo));
    };

    let rng = ThreadSafeRandom::next_float(0.0, 1.0);
    let mut probability = 0.0f32;
    for m in material_base.iter() {
        probability += m.probability;
        if rng < f64::from(probability) {
            // Ivory is unique... It doesn't have a group
            if m.material_id == MaterialType::Ivory.0 {
                return MaterialType(m.material_id);
            }

            let Some(material_group) = w
                .content
                .get_cached_treasure_material_group(m.material_id.cast_signed(), tier)
            else {
                return get_default_material_type(Some(wo));
            };

            let group_rng = ThreadSafeRandom::next_float(0.0, 1.0);
            let mut group_probability = 0.0f32;
            for g in material_group.iter() {
                group_probability += g.probability;
                if group_rng < f64::from(group_probability) {
                    return MaterialType(g.material_id);
                }
            }
            break;
        }
    }
    get_default_material_type(Some(wo))
}

/// Gets a randomized default material type for when a weenie does not have TsysMutationData.
// ACE: LootGenerationFactory.GetDefaultMaterialType
fn get_default_material_type(wo: Option<&WorldObject>) -> MaterialType {
    let Some(wo) = wo else {
        return MaterialType::Unknown;
    };

    let mut material = MaterialType::Unknown;
    let default_material_entry = ThreadSafeRandom::next(0, 4);

    let default_material = |row: usize| {
        let entry = usize::try_from(default_material_entry).expect("0..=4");
        MaterialType(loot_tables::DEFAULT_MATERIAL[row][entry].cast_unsigned())
    };

    let weenie_type = wo.biota.weenie_type;
    match weenie_type {
        WeenieType::Caster => material = default_material(3),
        WeenieType::Clothing => {
            if wo.item_type() == ItemType::Armor {
                material = default_material(0);
            }
            if wo.item_type() == ItemType::Clothing {
                material = default_material(5);
            }
        }
        WeenieType::MissileLauncher | WeenieType::Missile => material = default_material(1),
        WeenieType::MeleeWeapon => material = default_material(2),
        WeenieType::Generic => {
            if wo.item_type() == ItemType::Jewelry {
                material = default_material(3);
            }
            if wo.item_type() == ItemType::MissileWeapon {
                material = default_material(4);
            }
        }
        _ => material = MaterialType::Unknown,
    }

    material
}

/// `clothingColors`: PaletteTemplates 1-18, each with probability 1 (`InitClothingColors`).
static CLOTHING_COLORS: LazyLock<Vec<TreasureMaterialColor>> = LazyLock::new(init_clothing_colors);

// ACE: LootGenerationFactory.InitClothingColors
fn init_clothing_colors() -> Vec<TreasureMaterialColor> {
    let mut clothing_colors = Vec::new();
    for i in 1u32..19 {
        let tmc = TreasureMaterialColor {
            palette_template: i,
            probability: 1.0,
            ..TreasureMaterialColor::default()
        };
        clothing_colors.push(tmc);
    }
    clothing_colors
}

/// Assign a random color (Int.PaletteTemplate and Float.Shade) to a World Object based on the
/// material assigned to it.
// ACE: LootGenerationFactory.MutateColor
pub(crate) fn mutate_color(w: &World, wo: &mut WorldObject) {
    let (Some(material_type), Some(tsys_mutation_data), Some(clothing_base_id)) = (
        wo.material_type(),
        wo.tsys_mutation_data(),
        wo.clothing_base(),
    ) else {
        return;
    };
    if material_type.0 == 0 {
        return;
    }

    let color_code: u8 = (tsys_mutation_data >> 16).cs_cast();

    // BYTE spellCode = (tsysMutationData >> 24) & 0xFF;
    // BYTE colorCode = (tsysMutationData >> 16) & 0xFF;
    // BYTE gemCode = (tsysMutationData >> 8) & 0xFF;
    // BYTE materialCode = (tsysMutationData >> 0) & 0xFF;

    let cached = w
        .content
        .get_cached_treasure_material_colors(material_type.0.cast_signed(), i32::from(color_code));

    let colors: &[TreasureMaterialColor] = match cached.as_deref() {
        Some(colors) => colors,
        None => {
            // legacy support for hardcoded colorCode 0 table
            if color_code == 0 && material_type.0 > 0 {
                // This is a unique situation that typically applies to Under Clothes.
                // If the Color Code is 0, they can be PaletteTemplate 1-18, assuming there is a MaterialType
                // (gems have ColorCode of 0, but also no MaterialCode as they are defined by the weenie)

                // this can be removed after all servers have upgraded to latest db
                &CLOTHING_COLORS
            } else {
                return;
            }
        }
    };

    // Load the clothingBase associated with the WorldObject (an absent file reads as an empty table)
    let clothing_base = w
        .dats
        .portal_dat()
        .read_from_dat::<ClothingTable>(clothing_base_id);
    let contains = |pt: u32| {
        clothing_base
            .as_ref()
            .is_some_and(|c| c.palette_templates.contains_key(&pt))
    };

    // TODO : Probably better to use an intersect() function here. I defer to someone who knows how these work better than I - Optim
    // Compare the colors list and the clothingBase PaletteTemplates and remove any invalid items
    let colors: Vec<&TreasureMaterialColor> = colors
        .iter()
        .filter(|e| contains(e.palette_template))
        .collect();

    let total_probability = sum_probability(colors.iter().map(|c| c.probability));
    // If there's zero chance to get a random color, no point in continuing.
    if total_probability == 0.0 {
        return;
    }

    let rng = ThreadSafeRandom::next_float(0.0, total_probability);

    let mut palette_template = 0u32;
    let mut probability = 0.0f32;
    // Loop through the colors until we've reach our target value
    for color in &colors {
        probability += color.probability;
        if rng < f64::from(probability) {
            palette_template = color.palette_template;
            break;
        }
    }

    if palette_template > 0 {
        let clothing_base = clothing_base
            .as_ref()
            .expect("the palette template came from this table");
        let clo_sub_pal = &clothing_base.palette_templates[&palette_template];
        // Make sure this entry has a valid icon, otherwise there's likely something wrong with the ClothingBase value for this WorldObject (e.g. not supposed to be a loot item)
        if clo_sub_pal.icon.0 > 0 {
            // Assign the appropriate Icon and PaletteTemplate
            wo.set_icon_id(clo_sub_pal.icon.0);
            wo.set_palette_template(Some(palette_template.cast_signed()));

            // Throw some shade, at random
            wo.set_shade(Some(ThreadSafeRandom::next_float(0.0, 1.0)));

            // Some debug info...
            // log.Info($"Color success for {wo.MaterialType}({(int)wo.MaterialType}) - {wo.WeenieClassId} - {wo.Name}. PaletteTemplate {paletteTemplate} applied.");
        }
    } else {
        log::warn!(
            "[LOOT] Color looked failed for {} ({}) - {} - {}.",
            material_type,
            material_type.0,
            wo.biota.weenie_class_id,
            wo_name(wo)
        );
    }
}

/// `Enumerable.Sum(i => i.Probability)` over floats: .NET accumulates in double and returns float.
#[allow(clippy::cast_possible_truncation)]
fn sum_probability(values: impl Iterator<Item = f32>) -> f32 {
    let mut sum = 0.0f64;
    for v in values {
        sum += f64::from(v);
    }
    sum as f32
}

/// A gem class for the tier, then a gem of that class: its material.
// ACE: LootGenerationFactory.RollGemType
pub(crate) fn roll_gem_type(tier: i32) -> MaterialType {
    // previous formula
    //return (MaterialType)ThreadSafeRandom.Next(10, 50);

    // the gem class value can be further utilized for determining the item's monetary value
    let gem_class = gem_class_chance::roll(tier);

    let gem_result = gem_material_chance::roll(gem_class);

    gem_result.material_type
}

const WEAPON_BULK: f32 = 0.50;
const ARMOR_BULK: f32 = 0.25;

/// Lowers the burden by the quality interval, down to the item's bulk floor.
// ACE: LootGenerationFactory.MutateBurden
pub(crate) fn mutate_burden(
    wo: &mut WorldObject,
    treasure_death: &TreasureDeath,
    is_weapon: bool,
) -> bool {
    // ensure item has burden
    let Some(prev_burden) = wo.encumbrance_val() else {
        return false;
    };

    let quality_interval = tables_logic::tables::quality_chance::roll_interval(treasure_death);

    // only continue if the initial roll to modify the quality succeeded
    if quality_interval == 0.0 {
        return false;
    }

    // only continue if initial roll succeeded?
    let mut bulk = if is_weapon { WEAPON_BULK } else { ARMOR_BULK };
    let bulk_mod: f32 = wo.bulk_mod().unwrap_or(1.0).cs_cast();
    bulk *= bulk_mod;

    let max_burden_mod = 1.0 - bulk;

    let burden_mod = 1.0 - (quality_interval * max_burden_mod);

    // modify burden
    #[allow(clippy::cast_precision_loss)]
    let burden = prev_burden as f32 * burden_mod;
    wo.set_encumbrance_val(Some(round(f64::from(burden)).cs_cast()));

    if wo.encumbrance_val().is_some_and(|e| e < 1) {
        wo.set_encumbrance_val(Some(1));
    }

    //Console.WriteLine($"Modified burden from {prevBurden} to {wo.EncumbranceVal} for {wo.Name} ({wo.WeenieClassId})");

    true
}

/// The item's value from its armor level, workmanship, material, gems and tier, plus its spells.
// ACE: LootGenerationFactory.MutateValue
pub(crate) fn mutate_value(
    w: &World,
    wo: &mut WorldObject,
    tier: i32,
    _roll: Option<&TreasureRoll>,
) {
    if wo.value().is_none() {
        wo.set_value(Some(0)); // fixme: data
    }

    //var weenieValue = wo.Value;

    if wo.is_gem() {
        mutate_value_gem(wo);
    } else {
        if wo.has_armor_level() {
            crate::factories::loot_generation_factory_clothing::mutate_value_armor(wo);
        }

        mutate_value_generic(wo, tier);
    }

    mutate_value_spells(w, wo);

    /*Console.WriteLine($"Mutating value for {wo.Name} ({weenieValue:N0} -> {wo.Value:N0})");
    ...*/
}

// increase for a wider variance in item value ranges
const VALUE_FACTOR: f32 = 1.0 / 3.0;

const VALUE_NON_FACTOR: f32 = 1.0 - VALUE_FACTOR;

// ACE: LootGenerationFactory.MutateValue_Generic
fn mutate_value_generic(wo: &mut WorldObject, tier: i32) {
    // confirmed from retail magloot logs, matches up relatively closely

    #[allow(clippy::cast_possible_truncation)]
    let rng = ThreadSafeRandom::next_float(0.7, 1.25) as f32;

    let workmanship_mod = workmanship_chance::get_modifier(wo.item_workmanship());

    let material_mod = material_table::get_value_mod(wo.material_type());
    let gem_value = gem_material_chance::gem_value(wo.gem_type());

    let tier_mod = ITEM_VALUE_TIER_MOD[usize::try_from(tier.clamp(1, 8) - 1).expect("1..=8")];

    let value = wo
        .value()
        .expect("NullReferenceException: Value was set above");

    #[allow(clippy::cast_precision_loss)]
    let mut new_value =
        value as f32 * VALUE_FACTOR + material_mod * tier_mod as f32 + gem_value as f32;

    new_value *= workmanship_mod /* + qualityMod */ * rng;

    #[allow(clippy::cast_precision_loss)]
    {
        new_value += value as f32 * VALUE_NON_FACTOR;
    }

    let i_value: i32 = f64::from(new_value).ceil().cs_cast();

    // only raise value?
    if i_value > value {
        wo.set_value(Some(i_value));
    }
}

/// Adds twice the max mana and ten per spell level (the SpellDID and the spell book).
// ACE: LootGenerationFactory.MutateValue_Spells
fn mutate_value_spells(w: &World, wo: &mut WorldObject) {
    if let Some(item_max_mana) = wo.item_max_mana() {
        let value = wo
            .value()
            .map(|v| v.wrapping_add(item_max_mana.wrapping_mul(2)));
        wo.set_value(value);
    }

    let mut spell_level_sum = 0i32;

    if let Some(spell_did) = wo.spell_did() {
        let spell = Spell::new(w, spell_did, true);
        spell_level_sum += spell.level().cast_signed();
    }

    if let Some(book) = wo.biota.properties_spell_book.as_ref() {
        for &spell_id in book.keys() {
            let spell = Spell::from_int(w, spell_id, true);
            spell_level_sum += spell.level().cast_signed();
        }
    }
    let value = wo
        .value()
        .map(|v| v.wrapping_add(spell_level_sum.wrapping_mul(10)));
    wo.set_value(value);
}

const ITEM_VALUE_TIER_MOD: [i32; 8] = [
    25,   // T1
    50,   // T2
    100,  // T3
    250,  // T4
    500,  // T5
    1000, // T6
    2000, // T7
    3000, // T8
];

/// The min/max amount of pyreals that can be rolled per tier, from magloot corpse logs
const COIN_RANGES: [(i32, i32); 8] = [
    (5, 50),     // T1
    (10, 200),   // T2
    (10, 500),   // T3
    (25, 1000),  // T4
    (50, 5000),  // T5
    (250, 5000), // T6
    (250, 5000), // T7
    (250, 5000), // T8
];

// ACE: LootGenerationFactory.MutateCoins
fn mutate_coins(wo: &mut WorldObject, profile: &TreasureDeath) {
    let tier_range = tables_logic::at(&COIN_RANGES, profile.tier - 1);

    // flat rng range, according to magloot corpse logs
    let rng = ThreadSafeRandom::next(tier_range.0, tier_range.1);

    set_stack_size(wo, Some(rng));
}

/// `"{Name} of {descriptor}"` from the first spell (SpellDID, then the spell book) whose level-1
/// spell has a caster descriptor, else the name.
// ACE: LootGenerationFactory.GetLongDesc
#[must_use]
pub fn get_long_desc(wo: &WorldObject) -> Option<String> {
    if let Some(spell_did) = wo.spell_did() {
        if let Some(long_desc) = try_get_long_desc(wo, SpellId(spell_did)) {
            return Some(long_desc);
        }
    }

    if let Some(book) = wo.biota.properties_spell_book.as_ref() {
        for &spell_id in book.keys() {
            if let Some(long_desc) = try_get_long_desc(wo, SpellId(spell_id.cast_unsigned())) {
                return Some(long_desc);
            }
        }
    }
    wo.get_property(PropertyString::Name)
}

// ACE: LootGenerationFactory.TryGetLongDesc
fn try_get_long_desc(wo: &WorldObject, spell_id: SpellId) -> Option<String> {
    let spell_levels = spell_level_progression::get_spell_levels(spell_id)?;

    let descriptor = DESCRIPTORS.get(&spell_levels[0])?;
    Some(format!("{} of {descriptor}", wo_name(wo)))
}

/// T7+ items need level 150; T8 has a 90% chance of 180 instead.
// ACE: LootGenerationFactory.RollWieldLevelReq_T7_T8
pub(crate) fn roll_wield_level_req_t7_t8(wo: &mut WorldObject, profile: &TreasureDeath) {
    if profile.tier < 7 {
        return;
    }

    let mut wield_level_req = 150;

    if profile.tier == 8 {
        // t8 had a 90% chance for 180
        // loot quality mod?
        let rng = ThreadSafeRandom::next_float(0.0, 1.0);

        if rng < f64::from(0.9f32) {
            wield_level_req = 180;
        }
    }

    wo.set_wield_requirements(WieldRequirement::Level);
    wo.set_wield_difficulty(Some(wield_level_req));

    // as per retail pcaps, must be set to appear in client
    wo.set_wield_skill_type(Some(1));
}

/// This is only called by /cirand, and isn't really part of standard lootgen.
// ACE: LootGenerationFactory.CreateRandomObjectsOfType
pub fn create_random_objects_of_type(
    w: &mut World,
    r#type: WeenieType,
    count: i32,
) -> Vec<Option<WorldObject>> {
    let weenies = w
        .content
        .get_random_weenies_of_type(r#type.0.cast_signed(), count);

    let mut world_objects = Vec::new();

    for weenie in weenies {
        let weenie = weenie.expect("NullReferenceException: GetRandomWeeniesOfType returned null");
        let wo = world_object_factory_create_new_world_object(w, weenie.weenie_class_id);
        world_objects.push(wo);
    }

    world_objects
}

/// This is only called by /testlootgen command. The actual lootgen system doesn't use this.
// ACE: LootGenerationFactory.CreateRandomLootObjects_Test
pub fn create_random_loot_objects_test(
    w: &mut World,
    profile: &TreasureDeath,
    is_magical: bool,
    loot_bias: LootBias,
) -> Option<WorldObject> {
    let mut treasure_item_type_chances = if is_magical {
        &treasure_item_type_chances::DEFAULT_MAGICAL
    } else {
        &treasure_item_type_chances::DEFAULT_NON_MAGICAL
    };

    match loot_bias {
        LootBias::Armor => treasure_item_type_chances = &treasure_item_type_chances::ARMOR,
        LootBias::Weapons => treasure_item_type_chances = &treasure_item_type_chances::WEAPONS,
        LootBias::Jewelry => treasure_item_type_chances = &treasure_item_type_chances::JEWELRY,

        LootBias::MagicEquipment | LootBias::MixedEquipment => {
            treasure_item_type_chances = &treasure_item_type_chances::MIXED_MAGIC_EQUIPMENT;
        }
        _ => {}
    }

    let treasure_item_type = treasure_item_type_chances.roll(0.0);

    match treasure_item_type {
        TreasureItemType::Gem => create_gem(w, profile, is_magical),

        TreasureItemType::Armor => create_armor(w, profile, is_magical, true),

        TreasureItemType::Clothing => create_armor(w, profile, is_magical, false),

        TreasureItemType::Cloak => create_cloak(w, profile, true),

        TreasureItemType::Weapon => create_weapon(w, profile, is_magical),

        TreasureItemType::Jewelry => create_jewelry(w, profile, is_magical),

        TreasureItemType::ArtObject => {
            // Added Dinnerware at tail end of distribution, as
            // they are mutable loot drops that don't belong with the non-mutable drops
            // TODO: Will likely need some adjustment/fine tuning
            create_dinnerware(w, profile, is_magical)
        }

        _ => None,
    }
}

/// This is only called by the /lootgen command. Even though this is not called by normal
/// gameplay, it should still produce functionally identical results. Returns whether the item
/// is a lootgen item that was mutated.
// ACE: LootGenerationFactory.MutateItem
#[allow(unused_assignments)] // ACE sets roll.ItemType before calls that ignore it
pub fn mutate_item(
    w: &World,
    item: &mut WorldObject,
    profile: &TreasureDeath,
    is_magical: bool,
) -> bool {
    // should ideally be split up between getting the item type,
    // and getting the specific mutate function parameters
    // however, with the way the current loot tables are set up, this is not ideal...

    // this function does a bunch of o(n) lookups through the loot tables,
    // and is only used for the /lootgen dev command currently
    // if this needs to be used in high performance scenarios, the collections for the loot tables will
    // will need to be updated to support o(1) queries

    // update: most of the o(n) lookup issues have been fixed,
    // however this is still looking into more hashtables than necessary.
    // ideally there should only be 1 hashtable that gets the roll.ItemType,
    // and any other necessary info (armorType / weaponType)
    // then just call the existing mutation method

    let mut roll = TreasureRoll::new();

    roll.wcid = WeenieClassName(item.biota.weenie_class_id.cast_signed());
    roll.base_armor_level = item.armor_level().unwrap_or(0);

    let weapon_type = heavy_weapon_wcids::try_get_value(roll.wcid)
        .or_else(|| light_weapon_wcids::try_get_value(roll.wcid))
        .or_else(|| finesse_weapon_wcids::try_get_value(roll.wcid))
        .or_else(|| two_handed_weapon_wcids::try_get_value(roll.wcid));
    let missile_type = bow_wcids_aluvian::try_get_value(roll.wcid)
        .or_else(|| bow_wcids_gharundim::try_get_value(roll.wcid))
        .or_else(|| bow_wcids_sho::try_get_value(roll.wcid))
        .or_else(|| crossbow_wcids::try_get_value(roll.wcid))
        .or_else(|| atlatl_wcids::try_get_value(roll.wcid));

    if roll.wcid == WeenieClassName::coinstack {
        roll.item_type = TreasureItemType::Pyreal;
        mutate_coins(item, profile);
    } else if gem_material_chance::contains(roll.wcid) {
        roll.item_type = TreasureItemType::Gem;
        mutate_gem(w, item, profile, is_magical, &mut roll);
    } else if jewelry_wcids::contains(roll.wcid) {
        roll.item_type = TreasureItemType::Jewelry;

        if roll.has_armor_level(item) {
            // crowns, coronets, diadems, etc.
            mutate_armor(w, item, profile, is_magical, &mut roll);
        } else {
            mutate_jewelry(w, item, profile, is_magical, &mut roll);
        }
    } else if generic_wcids::contains(roll.wcid) {
        roll.item_type = TreasureItemType::ArtObject;
        mutate_dinnerware(w, item, profile, is_magical, &mut roll);
    } else if let Some(weapon_type) = weapon_type {
        roll.item_type = TreasureItemType::Weapon;
        roll.weapon_type = weapon_type;
        mutate_melee_weapon(w, item, profile, is_magical, &mut roll);
    } else if let Some(weapon_type) = missile_type {
        roll.item_type = TreasureItemType::Weapon;
        roll.weapon_type = weapon_type;
        mutate_missile_weapon(w, item, profile, is_magical, &mut roll);
    } else if caster_wcids::contains(roll.wcid) {
        roll.item_type = TreasureItemType::Weapon;
        roll.weapon_type = TreasureWeaponType::Caster;
        mutate_caster(w, item, profile, is_magical, &mut roll);
    } else if let Some(armor_type) = armor_wcids_lookup::try_get_value(roll.wcid) {
        roll.item_type = TreasureItemType::Armor;
        roll.armor_type = armor_type;
        mutate_armor(w, item, profile, is_magical, &mut roll);
    } else if society_armor_wcids_lookup::contains(roll.wcid) {
        roll.item_type = TreasureItemType::SocietyArmor; // collapsed for mutation
        roll.armor_type = TreasureArmorType::Society;
        mutate_armor(w, item, profile, is_magical, &mut roll);
    } else if clothing_wcids_lookup::contains(roll.wcid) {
        roll.item_type = TreasureItemType::Clothing;
        mutate_armor(w, item, profile, is_magical, &mut roll);
    }
    // scrolls don't really get mutated, even though they are in the main mutation method still
    else if cloak_wcids::contains(roll.wcid) {
        roll.item_type = TreasureItemType::Cloak;
        mutate_cloak(w, item, profile, Some(&mut roll));
    } else if pet_device_wcids_lookup::contains(roll.wcid) {
        roll.item_type = TreasureItemType::PetDevice;
        mutate_pet_device(item, profile.tier);
    } else if aetheria_wcids::contains(roll.wcid) {
        // mundane add-on
        mutate_aetheria(item, profile);
    }
    // other mundane items (mana stones, food/drink, healing kits, lockpicks, and spell components/peas) don't get mutated
    // it should be safe to return false here, for the 1 caller that currently uses this method
    // since it's not this function's responsibility to determine if an item is a lootgen item,
    // and only returns true if the item has been mutated.
    else {
        return false;
    }

    true
}

// ---------------------------------------------------------------------------------------------
// helpers and shims (not ACE members of this file)
// ---------------------------------------------------------------------------------------------

/// `wo.Name` for log lines and descriptions (`PropertyString.Name`, the virtual getter's base).
pub(crate) fn wo_name(wo: &WorldObject) -> String {
    wo.get_property(PropertyString::Name).unwrap_or_default()
}

/// `(byte?)(wo.TsysMutationData >> 8)`: `WorldObject.GemCode`.
pub(crate) fn gem_code(wo: &WorldObject) -> Option<u8> {
    wo.tsys_mutation_data().map(|d| (d >> 8).cs_cast())
}

/// `WorldObject.SetStackSize(int?)` (ported by 4.5a in `stackable.rs`).
pub(crate) fn set_stack_size(wo: &mut WorldObject, value: Option<i32>) {
    wo.set_stack_size(value);
}

/// `WorldObjectFactory.CreateNewWorldObject(uint weenieClassId)`: the cached weenie, a new
/// dynamic guid (only when the weenie exists), the constructor, and the guid recycled when it
/// builds nothing. The object is returned, not added to `World.objects`.
pub fn world_object_factory_create_new_world_object(
    w: &mut World,
    weenie_class_id: u32,
) -> Option<WorldObject> {
    loot_generation_factory();

    world_object_factory::create_new_world_object_by_wcid_in_world(w, weenie_class_id)
}

/// `MutateFilters.HasMutateFilter(this WorldObject, MutateFilter)` (`Entity/MutateFilters.cs`).
#[must_use]
pub fn has_mutate_filter(wo: &WorldObject, filter: MutateFilter) -> bool {
    crate::entity::mutate_filters::has_mutate_filter(wo, filter)
}
