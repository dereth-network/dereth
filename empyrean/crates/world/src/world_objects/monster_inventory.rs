// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Inventory.cs
//! Port of `Source/ACE.Server/WorldObjects/Monster_Inventory.cs`.
//!
//! How a monster picks what to wear and wield from its inventory. Members take `(w, this, ..)`
//! with `this` the creature. The sorts are .NET's unstable `List<T>.Sort`
//! ([`list_sort`]) and the weapon choice shuffles with `ThreadSafeRandom`, in ACE's
//! order of draws.

use empyrean_common::dotnet::{format, math};
use empyrean_common::extensions::{enum_helper, list_extensions};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::treasure_wielded::TreasureWielded;
use empyrean_entity::enums::{CombatStyle, CoverageMaskHelper, EquipMask, WeenieType};
use empyrean_entity::ObjectGuid;

use crate::world_objects::container;
use crate::world_objects::creature_equipment;
use crate::world_objects::world_object::WorldObject;
use crate::World;
use empyrean_common::dotnet::sort::{compare_to, list_sort};

/// Non-property fields declared in `Monster_Inventory.cs`.
#[derive(Debug, Default)]
pub struct MonsterInventoryFields {}

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// `Creature.IsNPC` (`Creature.cs`).
fn is_npc(o: &WorldObject) -> bool {
    crate::world_objects::creature::is_npc(o)
}

/// `(uint)(x ?? 0)` of an `EquipMask?`.
fn valid_locations_bits(o: &WorldObject) -> u32 {
    o.valid_locations().map_or(0, |v| v.0)
}

/// `(uint)(x ?? 0)` of a `CoverageMask?`.
fn clothing_priority_bits(o: &WorldObject) -> u32 {
    o.clothing_priority().map_or(0, |c| c.0)
}

/// Determines the monster inventory items to wield. (The clothing and armor are worn on the way:
/// `SelectWieldedClothing` and `SelectWieldedArmor` equip what they pick.)
// ACE: Creature.SelectWieldedTreasure
pub fn select_wielded_treasure(w: &mut World, this: ObjectGuid) -> Vec<ObjectGuid> {
    /*foreach (var item in Inventory.Values)
    Console.WriteLine($"{item.Name} - {item.WeenieType}");*/

    let _wielded_clothing = select_wielded_clothing(w, this);
    let _wielded_armor = select_wielded_armor(w, this);

    let mut wielded_weapons = select_wielded_weapons(w, this);
    let wielded_shield = select_wielded_shield(w, this);

    if let Some(wielded_shield) = wielded_shield {
        if wielded_weapons.is_empty() || !object(w, wielded_weapons[0]).is_ranged() {
            wielded_weapons.push(wielded_shield);
        }
    }

    wielded_weapons
}

/// Removes `item` from the inventory and wields it at its `ValidLocations`; `equipped` gets it
/// on success.
///
/// Not ACE's (a fix, V316): an item that cannot be worn (a slot already
/// covered) goes back into the inventory, as `EquipInventoryItems` does for its own picks, so it
/// stays with the monster (and reaches its corpse); ACE left it in neither the inventory nor the
/// equipped items. An item no longer in the inventory (already worn through another slot's list)
/// is skipped, which is what the failed second wield amounted to.
fn remove_and_wield(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
    equipped: &mut Vec<ObjectGuid>,
) {
    if !container::try_remove_from_inventory(w, this, item, false) {
        return;
    }
    let valid_locations = object(w, item).valid_locations().unwrap_or(EquipMask::None);
    if creature_equipment::try_wield_object_with_broadcasting(w, this, item, valid_locations) {
        equipped.push(item);
    } else {
        container::try_add_to_inventory(w, this, item, 0, false, true);
    }
}

/// Selects the clothing to wear from a monster's inventory.
// ACE: Creature.SelectWieldedClothing
pub fn select_wielded_clothing(w: &mut World, this: ObjectGuid) -> Vec<ObjectGuid> {
    let mut clothing: Vec<ObjectGuid> =
        container::get_inventory_items_of_type_weenie_type(w, this, WeenieType::Clothing)
            .into_iter()
            .filter(|&c| {
                let o = object(w, c);
                (clothing_priority_bits(o) & CoverageMaskHelper::Underwear.0) != 0
                    || (valid_locations_bits(o) & EquipMask::Cloak.0) != 0
            })
            .collect();

    if clothing.is_empty() {
        return Vec::new();
    }

    // sort by # of areas covered
    // prioritize clothing that covers more areas
    // ie., a shirt that covers both upper arms and lower arms
    list_sort(&mut clothing, |&a, &b| valid_location_comparer(w, a, b));
    clothing.reverse();

    let shirts: Vec<ObjectGuid> = clothing
        .iter()
        .copied()
        .filter(|&c| {
            (clothing_priority_bits(object(w, c)) & CoverageMaskHelper::UnderwearShirt.0) != 0
        })
        .collect();
    let pants: Vec<ObjectGuid> = clothing
        .iter()
        .copied()
        .filter(|&c| {
            (clothing_priority_bits(object(w, c)) & CoverageMaskHelper::UnderwearPants.0) != 0
        })
        .collect();

    /*Console.WriteLine("\nSelectWieldedClothing\nShirts:");
    foreach (var item in shirts)
        DebugArmorClothing(item);

    Console.WriteLine("Pants:");
    foreach (var item in pants)
        DebugArmorClothing(item);*/

    // try to equip the clothing at top of lists
    let mut equipped = Vec::new();

    if let Some(&item) = pants.first() {
        remove_and_wield(w, this, item, &mut equipped);
    }

    if let Some(&item) = shirts.first() {
        remove_and_wield(w, this, item, &mut equipped);
    }

    let cloaks: Vec<ObjectGuid> = clothing
        .iter()
        .copied()
        .filter(|&c| (valid_locations_bits(object(w, c)) & EquipMask::Cloak.0) != 0)
        .collect();
    if let Some(&item) = cloaks.first() {
        remove_and_wield(w, this, item, &mut equipped);
    }

    equipped
}

// ACE: Creature.SelectWieldedArmor
pub fn select_wielded_armor(w: &mut World, this: ObjectGuid) -> Vec<ObjectGuid> {
    // technically selecting all outerwear,
    // includes things like hats and slippers
    let mut armor: Vec<ObjectGuid> =
        container::get_inventory_items_of_type_weenie_type(w, this, WeenieType::Clothing)
            .into_iter()
            .filter(|&a| {
                (clothing_priority_bits(object(w, a)) & CoverageMaskHelper::Outerwear.0) != 0
            })
            .collect();

    if armor.is_empty() {
        return Vec::new();
    }

    // sort by # of areas covered, and then AL
    // prioritize AL first, and then armor that covers more areas
    list_sort(&mut armor, |&a, &b| valid_location_comparer(w, a, b));
    armor.reverse();
    list_sort(&mut armor, |&a, &b| armor_level_comparer(w, a, b));
    armor.reverse();

    // divide up into slots?
    // use CoverageMask or EquipMask?
    // boots / lowerLegs:
    // Boots - Locations: LowerLegWear, FootWear, Coverage: Feet
    // coverage mask data seems like it could be buggy / inconsistent in PY16...

    let slot = |mask: EquipMask| -> Vec<ObjectGuid> {
        armor
            .iter()
            .copied()
            .filter(|&c| (valid_locations_bits(object(w, c)) & mask.0) != 0)
            .collect()
    };
    let head = slot(EquipMask::HeadWear);
    let chest = slot(EquipMask::ChestArmor | EquipMask::ChestWear);
    let upper_arms = slot(EquipMask::UpperArmArmor | EquipMask::UpperArmWear); // this will also grab chest pieces that also cover upper arms etc.
    let lower_arms = slot(EquipMask::LowerArmArmor | EquipMask::LowerArmWear);
    let hands = slot(EquipMask::HandWear);
    let abdomen = slot(EquipMask::AbdomenArmor);
    let upper_legs = slot(EquipMask::UpperLegArmor | EquipMask::UpperLegWear);
    let lower_legs = slot(EquipMask::LowerLegArmor | EquipMask::LowerLegWear);
    let feet = slot(EquipMask::FootWear);

    /*Console.WriteLine("\nSelectWieldedArmor");
    foreach (var item in armor)
        DebugArmorClothing(item);*/

    // try to equip the clothing at top of lists
    let mut equipped = Vec::new();

    let sorted: Vec<ObjectGuid> = [
        head, chest, upper_arms, lower_arms, hands, upper_legs, abdomen, lower_legs, feet,
    ]
    .into_iter()
    .flatten()
    .collect();

    for item in sorted {
        remove_and_wield(w, this, item, &mut equipped);
    }

    equipped
}

/// Displays information about a a piece of armor / clothing (ACE writes it to the console).
// ACE: Creature.DebugArmorClothing
pub fn debug_armor_clothing(w: &World, _this: ObjectGuid, item: ObjectGuid) {
    let o = object(w, item);
    let locations = o
        .valid_locations()
        .map(|l| l.to_dotnet_string())
        .unwrap_or_default();
    let coverage = o
        .clothing_priority()
        .map(|c| c.to_dotnet_string())
        .unwrap_or_default();
    let name = crate::dispatch::name::name(w, item).unwrap_or_default();

    log::info!("{name} - Locations: {locations}, Coverage: {coverage}");
}

/// Compares the number of body parts covered by 2 pieces of clothing.
///
/// # Panics
/// When either `ValidLocations` is null: the `(uint)` cast of a null `EquipMask?` throws.
// ACE: Creature.ValidLocationComparer
// ACE-BUG: `(uint)a.ValidLocations` throws InvalidOperationException for an item with no ValidLocations, which SelectWieldedClothing admits (underwear coverage with a null ValidLocations) and SelectWieldedArmor admits (any outerwear coverage).
#[must_use]
pub fn valid_location_comparer(w: &World, a: ObjectGuid, b: ObjectGuid) -> i32 {
    let bits = |g: ObjectGuid| {
        object(w, g)
            .valid_locations()
            .expect("System.InvalidOperationException: Nullable object must have a value.")
            .0
    };
    compare_to(
        enum_helper::num_flags(bits(a)),
        enum_helper::num_flags(bits(b)),
    )
}

/// Compares the armor levels of 2 pieces of clothing (ACE's summary repeats the one above).
// ACE: Creature.ArmorLevelComparer
#[must_use]
pub fn armor_level_comparer(w: &World, a: ObjectGuid, b: ObjectGuid) -> i32 {
    compare_to(
        object(w, a).armor_level().unwrap_or(0),
        object(w, b).armor_level().unwrap_or(0),
    )
}

/// Sorts the inventory into weapons and ammunition (similar to
/// `GetInventoryItemsOfTypeWeenieType`, optimized for this particular scenario).
// ACE: Creature.GetMonsterInventory
// ACE-BUG: `(item.ValidLocations & EquipMask.Selectable) != 0` is lifted, so an item with a null ValidLocations compares as selectable and an NPC treats it as a weapon.
pub fn get_monster_inventory(
    w: &World,
    this: ObjectGuid,
    all_weapons: &mut Vec<ObjectGuid>,
    ammo: &mut Vec<ObjectGuid>,
) {
    let npc = is_npc(object(w, this));
    for item in container::inventory_values(w, this) {
        let o = object(w, item);
        match o.biota.weenie_type {
            WeenieType::MeleeWeapon
            | WeenieType::MissileLauncher
            | WeenieType::Missile
            | WeenieType::Caster => {
                all_weapons.push(item);
            }

            WeenieType::Ammunition => {
                ammo.push(item);
            }

            _ => {
                // 6873 - Ulgrim the Unpleasant wields => 161 - Mug
                // 70995 - Ulgrim the Unquiet wields => 27808 - Great Elariwood Idol
                if npc
                    && o.valid_locations()
                        .is_none_or(|v| (v & EquipMask::Selectable) != EquipMask::None)
                {
                    all_weapons.push(item);
                }
            }
        }
    }
}

// ACE: Creature.SelectWieldedWeapons
pub fn select_wielded_weapons(w: &mut World, this: ObjectGuid) -> Vec<ObjectGuid> {
    //Console.WriteLine($"{Name}.SelectWieldedWeapons()");

    let mut all_weapons = Vec::new();
    let mut ammo = Vec::new();

    get_monster_inventory(w, this, &mut all_weapons, &mut ammo);

    if all_weapons.is_empty() {
        return Vec::new();
    }

    //DebugTreasure();

    // select the best weapon available
    loop {
        let Some(weapon) = find_inventory_weapon(w, this, Some(&mut all_weapons)) else {
            return Vec::new();
        };

        // does this weapon require ammo?
        if object(w, weapon).is_ammo_launcher() {
            // find the best ammo for this weapon
            let weapon_ammo_type = object(w, weapon).ammo_type();
            let mut ammo_type: Vec<ObjectGuid> = ammo
                .iter()
                .copied()
                .filter(|&i| object(w, i).ammo_type() == weapon_ammo_type)
                .collect();
            let cur_ammo = find_inventory_weapon(w, this, Some(&mut ammo_type));

            let Some(cur_ammo) = cur_ammo else {
                // npcs don't require ammo
                if is_npc(object(w, this)) {
                    return vec![weapon];
                }

                // remove from possible selections
                if let Some(i) = all_weapons.iter().position(|&g| g == weapon) {
                    all_weapons.remove(i);
                }
                continue; // find next best weapon
            };

            //Console.WriteLine("Ammo type: " + (AmmoType)(weapon.AmmoType ?? 0));

            return vec![weapon, cur_ammo];
        }

        // CombatUse / DefaultCombatStyle / ValidLocations?
        let ai_allowed_combat_style = object(w, this).ai_allowed_combat_style();
        if (ai_allowed_combat_style & CombatStyle::DualWield) == CombatStyle::DualWield {
            let o = object(w, weapon);
            if o.biota.weenie_type == WeenieType::MeleeWeapon && !o.is_two_handed() {
                let dual_wield = all_weapons
                    .iter()
                    .copied()
                    .find(|&i| object(w, i).auto_wield_left());
                if let Some(dual_wield) = dual_wield {
                    return vec![weapon, dual_wield];
                }
            }
        }

        return vec![weapon];
    }
}

/// Shuffles `weapons` in place (`ThreadSafeRandom`) and takes the first that is not an off-hand
/// (`AutoWieldLeft`) weapon. `None` for a null list.
// ACE: Creature.FindInventoryWeapon
pub fn find_inventory_weapon(
    w: &World,
    _this: ObjectGuid,
    weapons: Option<&mut Vec<ObjectGuid>>,
) -> Option<ObjectGuid> {
    let weapons = weapons?;

    //var highestMax = weapons.OrderByDescending(w => w.GetBaseDamage().Max).FirstOrDefault();
    //var highestAvg = weapons.OrderByDescending(w => w.GetBaseDamage().Avg).FirstOrDefault();

    //return highestMax;

    // did monsters select best weapons, or just a random weapon?
    // see: lugians (wielded treasure table 439), 100% spawn with rocks if most damage potential selected
    list_extensions::shuffle(weapons);
    weapons
        .iter()
        .copied()
        .find(|&i| !object(w, i).auto_wield_left())

    /*var rng = ThreadSafeRandom.Next(0, weapons.Count);
    if (rng == weapons.Count)
        return null;    // choose no weapon? lugians should have ~33% chance to select rock, according to retail pcaps

    return weapons[rng];*/
}

/// A random shield from the inventory, or `None`.
// ACE: Creature.SelectWieldedShield
pub fn select_wielded_shield(w: &mut World, this: ObjectGuid) -> Option<ObjectGuid> {
    let shields: Vec<ObjectGuid> = container::inventory_values(w, this)
        .into_iter()
        .filter(|&g| object(w, g).is_shield())
        .collect();

    if shields.is_empty() {
        return None;
    }

    // select a random shield
    let rng = ThreadSafeRandom::next(0, i32::try_from(shields.len()).unwrap_or(i32::MAX) - 1);
    Some(shields[usize::try_from(rng).expect("0..Count")])
}

/// Lists the possible wielded treasure with its chances (ACE writes it to the console).
///
/// # Panics
/// When a table entry names a weenie that does not exist (`weenie.ClassName` on null).
// ACE: Creature.DebugTreasure
pub fn debug_treasure(w: &World, this: ObjectGuid) {
    let Some(wielded_treasure) = creature_equipment::wielded_treasure(w, this) else {
        return;
    };

    let name = crate::dispatch::name::name(w, this).unwrap_or_default();
    log::info!("{name} possible wielded treasure:");
    for item in wielded_treasure.iter() {
        let weenie = w
            .content
            .get_cached_weenie(item.weenie_class_id)
            .expect("System.NullReferenceException: weenie");
        let probability = math::round_digits(f64::from(item.probability * 100.0), 2);
        log::info!(
            "{} - {}%",
            weenie.class_name.clone().unwrap_or_default(),
            format::to_string(probability)
        );
    }

    let total_probability = math::round_digits(
        f64::from(get_total_probability(Some(&wielded_treasure)) * 100.0),
        2,
    );
    log::info!(
        "Probability of spawning any items: {}%",
        format::to_string(total_probability)
    );
}

/// `Sum - Product` of the entries' probabilities. LINQ's `Sum` over `float` accumulates in
/// `double` and narrows at the end; `Product` (ACE's extension) multiplies in `float`.
// ACE: Creature.GetTotalProbability
#[must_use]
pub fn get_total_probability(items: Option<&[TreasureWielded]>) -> f32 {
    let Some(items) = items else { return 0.0 };

    let prob: Vec<f32> = items.iter().map(|i| i.probability).collect();

    let mut sum = 0.0f64;
    for &p in &prob {
        sum += f64::from(p);
    }
    #[allow(clippy::cast_possible_truncation)] // LINQ's (float)double
    let total_sum = sum as f32;
    let total_product = list_extensions::product(&prob);

    total_sum - total_product
}

/// Equips what `SelectWieldedWeapons` (weapons only) or `SelectWieldedTreasure` picks; an item
/// that cannot be wielded goes back into the inventory. ACE's default: `weapons_only = false`.
// ACE: Creature.EquipInventoryItems
pub fn equip_inventory_items(w: &mut World, this: ObjectGuid, weapons_only: bool) {
    let items = if weapons_only {
        select_wielded_weapons(w, this)
    } else {
        select_wielded_treasure(w, this)
    };

    for item in items {
        let Some(valid_locations) = object(w, item).valid_locations() else {
            continue;
        };

        //Console.WriteLine($"{Name} equipping {item.Name}");

        if !container::try_remove_from_inventory(w, this, item, false) {
            continue;
        }

        let success = if weapons_only {
            creature_equipment::try_wield_object_with_broadcasting(w, this, item, valid_locations)
        } else {
            creature_equipment::try_wield_object(w, this, item, valid_locations)
        };

        if !success {
            container::try_add_to_inventory(w, this, item, 0, false, true);
        }
    }
}
