// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Equipment.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Equipment.cs`.
//!
//! ACE's `Dictionary<ObjectGuid, WorldObject> EquippedObjects` is
//! [`CreatureEquipmentFields::equipped_objects`], a `DotNetDict` of guids, so the `FirstOrDefault`
//! lookups (the weapon, shield and ammo used by combat) see .NET's enumeration order. Members take
//! `(w, this, ..)` with `this` the creature; items are guids of objects in `World.objects`.
//!
//! # Shims
//!
//! Private functions marked `SHIM:` stand in for members of other ACE files:
//! `Creature.IsDualWieldAttack` (`Creature_Melee.cs`, needs `CurrentMotionState`),
//! `Creature.CombatMode` (`Creature_Combat.cs`), `WorldObject.EnqueueBroadcast` and
//! `EnqueueActionBroadcast`, `Creature.CreateItemSpell`/`RemoveItemSpell` (magic),
//! `WorldObject.OnSpellsDeactivated`, `WorldObject.HasProcSpell`, `PropertyManager.GetDouble`,
//! `CreateList.GetSetModifier` (`Entity/CreateList.cs`) and
//! `LootGenerationFactory.CreateRandomLootObjects`.

use std::sync::Arc;

use empyrean_common::dotnet::DotNetDict;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::treasure_wielded::TreasureWielded;
use empyrean_entity::enums::{
    CombatMode, CombatStyle, CoverageMask, DestinationType, EquipMask, ItemType, ParentLocation,
    Placement, PropertyInstanceId, PropertyInt, Sound, WeenieType,
};
use empyrean_entity::models::PropertiesCreateList;
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::messages::game_message_obj_desc_event::game_message_obj_desc_event;
use crate::network::game_messages::messages::game_message_parent_event::game_message_parent_event;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_equipment::{self, HeldItem};
use crate::world_objects::{container, creature_combat, creature_melee, world_object_weapon};
use crate::World;

/// Non-property fields declared in `Creature_Equipment.cs`.
#[derive(Debug, Default)]
pub struct CreatureEquipmentFields {
    /// Set once `AddBiotasToEquippedObjects` has run (`private set` in ACE).
    // ACE: Creature.EquippedObjectsLoaded
    pub equipped_objects_loaded: bool,
    /// Use `try_equip_object` and `try_dequip_object` to manipulate this dictionary. Do not
    /// manipulate this dictionary directly.
    // ACE: Creature.EquippedObjects
    pub equipped_objects: DotNetDict<ObjectGuid, ()>,
    /// This is initialized the first time an item is equipped that has a rating. If it is null,
    /// there are no equipped items with ratings.
    // ACE: Creature.equippedItemsRatingCache
    pub equipped_items_rating_cache: Option<DotNetDict<PropertyInt, i32>>,
}

// ================================================================================ helpers

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn object_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn fields(o: &WorldObject) -> &CreatureEquipmentFields {
    &o.creature
        .as_ref()
        .expect("InvalidCastException: not a Creature")
        .creature_equipment
}

fn fields_mut(o: &mut WorldObject) -> &mut CreatureEquipmentFields {
    &mut o
        .creature
        .as_mut()
        .expect("InvalidCastException: not a Creature")
        .creature_equipment
}

/// `EquippedObjects.Values.ToList()`: a snapshot of the guids, in enumeration order.
#[must_use]
pub fn equipped_objects_values(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    fields(object(w, this))
        .equipped_objects
        .keys()
        .copied()
        .collect()
}

/// `EquippedObjects.Values.FirstOrDefault(pred)`.
fn first_equipped(
    w: &World,
    this: ObjectGuid,
    pred: impl Fn(&WorldObject) -> bool,
) -> Option<ObjectGuid> {
    fields(object(w, this))
        .equipped_objects
        .keys()
        .copied()
        .find(|&g| pred(object(w, g)))
}

/// `x += y` / `x -= y` on an `int?` property (null stays null).
fn add_burden_and_value(w: &mut World, this: ObjectGuid, encumbrance: i32, value: i32) {
    let o = object_mut(w, this);
    let e = o.encumbrance_val().map(|c| c.wrapping_add(encumbrance));
    o.set_encumbrance_val(e);
    let v = o.value().map(|c| c.wrapping_add(value));
    o.set_value(v);
}

/// `WorldObject.EnqueueBroadcast(bool sendSelf, params GameMessage[] msgs)` and
/// `EnqueueBroadcast(params GameMessage[])`: the port in `world_object_networking.rs`.
fn enqueue_broadcast(w: &mut World, this: ObjectGuid, send_self: bool, msg: GameMessage) {
    let _ = crate::world_objects::world_object_networking::enqueue_broadcast(
        w,
        this,
        send_self,
        &[msg],
    );
}

/// `EnqueueActionBroadcast(p => p.TrackEquippedObject(this, item))`.
fn enqueue_action_broadcast_track_equipped_object(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
) {
    crate::world_objects::player_tracking::enqueue_action_broadcast_track_equipped_object(
        w, this, item,
    );
}

/// `EnqueueActionBroadcast(p => p.RemoveTrackedEquippedObject(this, wo))`.
fn enqueue_action_broadcast_remove_tracked_equipped_object(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
) {
    crate::world_objects::player_tracking::enqueue_action_broadcast_remove_tracked_equipped_object(
        w, this, item,
    );
}

/// `Creature.CreateItemSpell(item, spellID)` (`Creature_Magic.cs`; the result is discarded here).
fn create_item_spell(w: &mut World, this: ObjectGuid, item: ObjectGuid, spell_id: u32) {
    let _ = crate::world_objects::creature_magic::create_item_spell(w, this, item, spell_id);
}

/// `Creature.RemoveItemSpell(item, spellId, silent)` (`Creature_Magic.cs`).
fn remove_item_spell(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
    spell_id: u32,
    silent: bool,
) {
    crate::world_objects::creature_magic::remove_item_spell(w, this, Some(item), spell_id, silent);
}

/// `WorldObject.OnSpellsDeactivated()` (`WorldObject_Magic.cs`).
fn on_spells_deactivated(w: &mut World, item: ObjectGuid) {
    crate::world_objects::world_object_magic::on_spells_deactivated(w, item);
}

/// `PropertyManager.GetDouble(key).Item`.
fn property_manager_get_double(w: &World, key: &str) -> f64 {
    crate::managers::property_manager::get_double(w, key, 0.0, true).item
}

/// `item.Biota.GetKnownSpellsIds(BiotaDatabaseLock)`: the spell book's ids, in its order.
fn get_known_spells_ids(w: &World, item: ObjectGuid) -> Vec<i32> {
    object(w, item)
        .biota
        .properties_spell_book
        .as_ref()
        .map(|b| b.keys().copied().collect())
        .unwrap_or_default()
}

// ================================================================================ Creature_Equipment.cs

/// The only time this should be used is to populate EquippedObjects from the ctor. Builds each
/// biota's object and adds it to `World.objects`.
///
/// # Panics
/// Where ACE throws: a biota whose weenie type has no class (`CreateWorldObject` returns null),
/// or one whose guid is already live.
// ACE: Creature.AddBiotasToEquippedObjects
// ACE-BUG: a wielded biota with an undefined WeenieType makes CreateWorldObject return null, and `worldObject.Guid` then throws NullReferenceException.
pub fn add_biotas_to_equipped_objects(
    w: &mut World,
    this: ObjectGuid,
    wielded_items: Vec<empyrean_entity::Biota>,
) {
    for biota in wielded_items {
        let world_object = crate::world_objects::world_object::CtorEnv::with_world(w, |env| {
            crate::factories::world_object_factory::create_world_object_from_biota(env, biota)
        })
        .expect("System.NullReferenceException: CreateWorldObject(biota) returned null");
        let guid = world_object.guid;
        assert!(
            w.objects.insert(world_object).is_ok(),
            "biota 0x{:08X} is already a live object",
            guid.full()
        );

        fields_mut(object_mut(w, this))
            .equipped_objects
            .insert(guid, ());

        add_item_to_equipped_items_rating_cache(w, this, guid);

        let encumbrance = object(w, guid).encumbrance_val().unwrap_or(0);
        let o = object_mut(w, this);
        let e = o.encumbrance_val().map(|c| c.wrapping_add(encumbrance));
        o.set_encumbrance_val(e);
    }

    fields_mut(object_mut(w, this)).equipped_objects_loaded = true;

    set_children(w, this);
}

// ACE: Creature.WieldedLocationIsAvailable
#[must_use]
pub fn wielded_location_is_available(
    w: &World,
    this: ObjectGuid,
    item: ObjectGuid,
    wielded_location: EquipMask,
) -> bool {
    // filtering to just armor here, or else trinkets and dual wielding breaks
    // update: cannot repro the break anymore?
    //var existing = this is Player ? GetEquippedClothingArmor(item.ClothingPriority ?? 0) : GetEquippedItems(item, wieldedLocation);
    let existing = get_equipped_items(w, this, item, wielded_location);

    // TODO: handle overlap from MeleeWeapon / MissileWeapon / Held

    existing.is_empty()
}

// ACE: Creature.HasEquippedItem
#[must_use]
pub fn has_equipped_item(w: &World, this: ObjectGuid, object_guid: ObjectGuid) -> bool {
    fields(object(w, this))
        .equipped_objects
        .contains_key(&object_guid)
}

/// Get Wielded Item. Returns null if not found. (The `uint` overload is `ObjectGuid::new(guid)`.)
// ACE: Creature.GetEquippedItem
#[must_use]
pub fn get_equipped_item(
    w: &World,
    this: ObjectGuid,
    object_guid: ObjectGuid,
) -> Option<ObjectGuid> {
    has_equipped_item(w, this, object_guid).then_some(object_guid)
}

/// Returns a list of equipped clothing/armor with any coverage overlap.
// ACE: Creature.GetEquippedClothingArmor
#[must_use]
pub fn get_equipped_clothing_armor(
    w: &World,
    this: ObjectGuid,
    coverage_mask: CoverageMask,
) -> Vec<ObjectGuid> {
    equipped_objects_values(w, this)
        .into_iter()
        .filter(|&i| {
            object(w, i)
                .clothing_priority()
                .is_some_and(|c| (c & coverage_mask) != CoverageMask::default())
        })
        .collect()
}

/// Returns a list of equipped items with any overlap with input locations.
// ACE: Creature.GetEquippedItems
#[must_use]
pub fn get_equipped_items(
    w: &World,
    this: ObjectGuid,
    item: ObjectGuid,
    wielded_location: EquipMask,
) -> Vec<ObjectGuid> {
    if is_weapon_slot(wielded_location) {
        // TODO: change to coalesced CurrentWieldedLocation
        let (_placement, parent_location) =
            get_placement_location(object(w, item), wielded_location);
        return equipped_objects_values(w, this)
            .into_iter()
            .filter(|&i| {
                let o = object(w, i);
                // `i.CurrentWieldedLocation != EquipMask.MissileAmmo` is lifted: null differs.
                o.parent_location().is_some()
                    && o.parent_location() == Some(parent_location)
                    && o.current_wielded_location() != Some(EquipMask::MissileAmmo)
            })
            .collect();
    }

    let o = object(w, item);
    if o.is_clothing() {
        get_equipped_clothing_armor(w, this, o.clothing_priority().unwrap_or_default())
    } else {
        equipped_objects_values(w, this)
            .into_iter()
            .filter(|&i| {
                object(w, i)
                    .current_wielded_location()
                    .is_some_and(|c| (c & wielded_location) != EquipMask::None)
            })
            .collect()
    }
}

/// Returns the currently equipped primary weapon. ACE's default: `force_main_hand = false`.
// ACE: Creature.GetEquippedWeapon
#[must_use]
pub fn get_equipped_weapon(
    w: &World,
    this: ObjectGuid,
    force_main_hand: bool,
) -> Option<ObjectGuid> {
    let melee_weapon = get_equipped_melee_weapon(w, this, force_main_hand);
    melee_weapon.or_else(|| get_equipped_missile_weapon(w, this))
}

/// Returns the current equipped active melee weapon. This will normally be the primary melee
/// weapon, but if dual wielding, this will be the weapon for the next attack. ACE's default:
/// `force_main_hand = false`.
// ACE: Creature.GetEquippedMeleeWeapon
#[must_use]
pub fn get_equipped_melee_weapon(
    w: &World,
    this: ObjectGuid,
    force_main_hand: bool,
) -> Option<ObjectGuid> {
    if !creature_melee::is_dual_wield_attack(w, this)
        || creature_melee::dual_wield_alternate(w, this)
        || force_main_hand
    {
        return first_equipped(w, this, |e| {
            e.parent_location() == Some(ParentLocation::RightHand)
                && (e.current_wielded_location() == Some(EquipMask::MeleeWeapon)
                    || e.current_wielded_location() == Some(EquipMask::TwoHanded))
        });
    }

    get_dual_wield_weapon(w, this)
}

/// Returns the currently equipped secondary weapon.
// ACE: Creature.GetDualWieldWeapon
#[must_use]
pub fn get_dual_wield_weapon(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    first_equipped(w, this, |e| {
        !e.is_shield() && e.current_wielded_location() == Some(EquipMask::Shield)
    })
}

/// Returns the currently equipped wand.
// ACE: Creature.GetEquippedWand
#[must_use]
pub fn get_equipped_wand(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    first_equipped(w, this, |e| {
        e.current_wielded_location() == Some(EquipMask::Held)
    })
}

/// Returns the currently equipped missile weapon. This can be either a missile launcher (bow,
/// crossbow, atlatl) or stackable thrown weapons directly in the main hand slot.
// ACE: Creature.GetEquippedMissileWeapon
#[must_use]
pub fn get_equipped_missile_weapon(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    first_equipped(w, this, |e| {
        e.current_wielded_location() == Some(EquipMask::MissileWeapon)
    })
}

/// Returns the currently equipped missile launcher.
// ACE: Creature.GetEquippedMissileLauncher
#[must_use]
pub fn get_equipped_missile_launcher(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    first_equipped(w, this, |e| {
        e.current_wielded_location() == Some(EquipMask::MissileWeapon) && e.is_missile_launcher()
    })
}

/// Returns the current equipped weapon in main hand.
// ACE: Creature.GetEquippedMainHand
#[must_use]
pub fn get_equipped_main_hand(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    get_equipped_melee_weapon(w, this, true)
        .or_else(|| get_equipped_missile_weapon(w, this))
        .or_else(|| get_equipped_wand(w, this))
}

/// Returns either a shield, an off-hand weapon, or null.
// ACE: Creature.GetEquippedOffHand
#[must_use]
pub fn get_equipped_off_hand(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    first_equipped(w, this, |e| {
        e.current_wielded_location() == Some(EquipMask::Shield)
    })
}

/// Returns the currently equipped shield.
// ACE: Creature.GetEquippedShield
#[must_use]
pub fn get_equipped_shield(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    first_equipped(w, this, |e| {
        e.is_shield() && e.current_wielded_location() == Some(EquipMask::Shield)
    })
}

/// Returns the currently equipped missile ammo.
// ACE: Creature.GetEquippedAmmo
#[must_use]
pub fn get_equipped_ammo(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    first_equipped(w, this, |e| {
        e.current_wielded_location() == Some(EquipMask::MissileAmmo)
    })
}

/// Returns the ammo slot item for bows / atlatls, or the missile weapon for thrown weapons.
// ACE: Creature.GetMissileAmmo
#[must_use]
pub fn get_missile_ammo(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    let weapon = get_equipped_missile_weapon(w, this);

    if let Some(weapon) = weapon {
        if object(w, weapon).is_ammo_launcher() {
            return get_equipped_ammo(w, this);
        }
    }

    weapon
}

/// The ten gear ratings of `wo`, in the cache's order.
fn gear_ratings(o: &WorldObject) -> [(PropertyInt, i32); 10] {
    [
        (PropertyInt::GearDamage, o.gear_damage().unwrap_or(0)),
        (
            PropertyInt::GearDamageResist,
            o.gear_damage_resist().unwrap_or(0),
        ),
        (PropertyInt::GearCrit, o.gear_crit().unwrap_or(0)),
        (
            PropertyInt::GearCritResist,
            o.gear_crit_resist().unwrap_or(0),
        ),
        (
            PropertyInt::GearCritDamage,
            o.gear_crit_damage().unwrap_or(0),
        ),
        (
            PropertyInt::GearCritDamageResist,
            o.gear_crit_damage_resist().unwrap_or(0),
        ),
        (
            PropertyInt::GearHealingBoost,
            o.gear_healing_boost().unwrap_or(0),
        ),
        (PropertyInt::GearMaxHealth, o.gear_max_health().unwrap_or(0)),
        (
            PropertyInt::GearPKDamageRating,
            o.gear_pk_damage_rating().unwrap_or(0),
        ),
        (
            PropertyInt::GearPKDamageResistRating,
            o.gear_pk_damage_resist_rating().unwrap_or(0),
        ),
    ]
}

// ACE: Creature.AddItemToEquippedItemsRatingCache
fn add_item_to_equipped_items_rating_cache(w: &mut World, this: ObjectGuid, wo: ObjectGuid) {
    let ratings = gear_ratings(object(w, wo));
    if ratings.iter().all(|&(_, v)| v == 0) {
        return;
    }

    let f = fields_mut(object_mut(w, this));
    let cache = f.equipped_items_rating_cache.get_or_insert_with(|| {
        let mut cache = DotNetDict::new();
        for (rating, _) in ratings {
            cache.add(rating, 0);
        }
        cache
    });

    for (rating, value) in ratings {
        let entry = cache.get_mut(&rating).expect("the ten ratings");
        *entry = entry.wrapping_add(value);
    }
}

// ACE: Creature.RemoveItemFromEquippedItemsRatingCache
fn remove_item_from_equipped_items_rating_cache(w: &mut World, this: ObjectGuid, wo: ObjectGuid) {
    let ratings = gear_ratings(object(w, wo));
    let Some(cache) = fields_mut(object_mut(w, this))
        .equipped_items_rating_cache
        .as_mut()
    else {
        return;
    };

    for (rating, value) in ratings {
        let entry = cache.get_mut(&rating).expect("the ten ratings");
        *entry = entry.wrapping_sub(value);
    }
}

// ACE: Creature.GetEquippedItemsRatingSum
#[must_use]
pub fn get_equipped_items_rating_sum(w: &World, this: ObjectGuid, rating: PropertyInt) -> i32 {
    let Some(cache) = fields(object(w, this)).equipped_items_rating_cache.as_ref() else {
        return 0;
    };

    if let Some(&value) = cache.get(&rating) {
        return value;
    }

    log::error!(
        "Creature_Equipment.GetEquippedItemsRatingsSum() does not support {}",
        rating.to_dotnet_string()
    );
    0
}

/// Try to wield an object for non-player creatures.
// ACE: Creature.TryWieldObject
pub fn try_wield_object(
    w: &mut World,
    this: ObjectGuid,
    world_object: ObjectGuid,
    wielded_location: EquipMask,
) -> bool {
    // check wield requirements?
    if !try_equip_object(w, this, world_object, wielded_location) {
        return false;
    }

    // enqueue to ensure parent object has spawned,
    // and spell fx are visible
    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, 0.1);
    action_chain.add_action(Actor::Object(this), move |w| {
        try_activate_item_spells(w, this, world_object)
    });
    action_chain.enqueue_chain(w);

    true
}

/// Tries to activate item spells for a non-player creature.
// ACE: Creature.TryActivateItemSpells
fn try_activate_item_spells(w: &mut World, this: ObjectGuid, item: ObjectGuid) {
    if !object(w, this).attackable() {
        return;
    }

    // check activation requirements?
    for spell in get_known_spells_ids(w, item) {
        #[allow(clippy::cast_sign_loss)] // C#'s (uint)int
        create_item_spell(w, this, item, spell as u32);
    }
}

/// This will set the CurrentWieldedLocation property to wieldedLocation and the Wielder property
/// to this guid and will add it to the EquippedObjects dictionary. It will also increase the
/// EncumbranceVal and Value.
// ACE: Creature.TryEquipObject
pub fn try_equip_object(
    w: &mut World,
    this: ObjectGuid,
    world_object: ObjectGuid,
    wielded_location: EquipMask,
) -> bool {
    // todo: verify wielded location is valid location
    if !wielded_location_is_available(w, this, world_object, wielded_location) {
        return false;
    }

    let biota_id = object(w, this).biota.id;
    let item = object_mut(w, world_object);
    item.set_current_wielded_location(Some(wielded_location));
    item.set_wielder_id(Some(biota_id));
    item.wielder = Some(this);

    fields_mut(object_mut(w, this))
        .equipped_objects
        .insert(world_object, ());

    add_item_to_equipped_items_rating_cache(w, this, world_object);

    let o = object(w, world_object);
    let (encumbrance, value) = (o.encumbrance_val().unwrap_or(0), o.value().unwrap_or(0));
    add_burden_and_value(w, this, encumbrance, value);

    try_set_child(w, this, world_object);

    dispatch::on_wield::on_wield(w, world_object, this);

    true
}

// ACE: Creature.TryWieldObjectWithBroadcasting
pub fn try_wield_object_with_broadcasting(
    w: &mut World,
    this: ObjectGuid,
    world_object: ObjectGuid,
    wielded_location: EquipMask,
) -> bool {
    // check wield requirements?
    if !try_equip_object_with_broadcasting(w, this, world_object, wielded_location) {
        return false;
    }

    try_activate_item_spells(w, this, world_object);

    true
}

/// This will set the CurrentWieldedLocation property to wieldedLocation and the Wielder property
/// to this guid and will add it to the EquippedObjects dictionary. It will also increase the
/// EncumbranceVal and Value.
// ACE: Creature.TryEquipObjectWithBroadcasting
pub fn try_equip_object_with_broadcasting(
    w: &mut World,
    this: ObjectGuid,
    world_object: ObjectGuid,
    wielded_location: EquipMask,
) -> bool {
    if !try_equip_object(w, this, world_object, wielded_location) {
        return false;
    }

    if is_in_child_location(w, this, world_object) {
        // Is this equipped item visible to others?
        let msg = game_message_sound(this, Sound::WieldObject, 1.0);
        enqueue_broadcast(w, this, false, msg);
    }

    if object(w, world_object).parent_location().is_some() {
        let (creature, item) = w
            .objects
            .get2_mut(this, world_object)
            .expect("the creature and its item");
        let msg = game_message_parent_event(creature, item, None, None);
        enqueue_broadcast(w, this, true, msg);
    }

    let msg = game_message_obj_desc_event(w, this);
    enqueue_broadcast(w, this, true, msg);

    // Notify viewers in the area that we've equipped the item
    enqueue_action_broadcast_track_equipped_object(w, this, world_object);

    true
}

/// This will remove the Wielder and CurrentWieldedLocation properties on the item and will remove
/// it from the EquippedObjects dictionary. It does not add it to inventory as you could be
/// unwielding to the ground or a chest. It will also decrease the EncumbranceVal and Value.
/// Returns `(worldObject, wieldedLocation)`, or `None`.
// ACE: Creature.TryDequipObject
pub fn try_dequip_object(
    w: &mut World,
    this: ObjectGuid,
    object_guid: ObjectGuid,
) -> Option<(ObjectGuid, EquipMask)> {
    fields_mut(object_mut(w, this))
        .equipped_objects
        .remove(&object_guid)?;
    let world_object = object_guid;

    remove_item_from_equipped_items_rating_cache(w, this, world_object);

    let item = object_mut(w, world_object);
    let wielded_location = item.current_wielded_location().unwrap_or(EquipMask::None);

    item.remove_property(PropertyInt::CurrentWieldedLocation);
    item.remove_property(PropertyInstanceId::Wielder);
    item.wielder = None;

    on_spells_deactivated(w, world_object);

    let o = object(w, world_object);
    let (encumbrance, value) = (o.encumbrance_val().unwrap_or(0), o.value().unwrap_or(0));
    add_burden_and_value(w, this, encumbrance.wrapping_neg(), value.wrapping_neg());

    clear_child(w, this, world_object);

    remove_child(w, this, world_object);

    dispatch::on_un_wield::on_un_wield(w, world_object, this);

    Some((world_object, wielded_location))
}

/// `Children.Remove(Children.Find(s => s.Guid == wo.Guid.Full))`: the first child with that guid.
fn remove_child(w: &mut World, this: ObjectGuid, item: ObjectGuid) {
    let children = &mut object_mut(w, this).wo.world_object_properties.children;
    if let Some(i) = children.iter().position(|s| s.guid == item.full()) {
        children.remove(i);
    }
}

/// Called by non-player creatures to unwield an item, removing any spells casted by the item.
/// ACE's default: `dropping_to_landscape = false`.
// ACE: Creature.TryUnwieldObjectWithBroadcasting
pub fn try_unwield_object_with_broadcasting(
    w: &mut World,
    this: ObjectGuid,
    object_guid: ObjectGuid,
    dropping_to_landscape: bool,
) -> Option<(ObjectGuid, EquipMask)> {
    let (world_object, wielded_location) =
        try_dequip_object_with_broadcasting(w, this, object_guid, dropping_to_landscape)?;

    // remove item spells
    for spell in get_known_spells_ids(w, world_object) {
        #[allow(clippy::cast_sign_loss)] // C#'s (uint)int
        remove_item_spell(w, this, world_object, spell as u32, true);
    }

    Some((world_object, wielded_location))
}

/// This will remove the Wielder and CurrentWieldedLocation properties on the item and will remove
/// it from the EquippedObjects dictionary. It does not add it to inventory as you could be
/// unwielding to the ground or a chest. It will also decrease the EncumbranceVal and Value.
/// ACE's default: `dropping_to_landscape = false`.
// ACE: Creature.TryDequipObjectWithBroadcasting
pub fn try_dequip_object_with_broadcasting(
    w: &mut World,
    this: ObjectGuid,
    object_guid: ObjectGuid,
    dropping_to_landscape: bool,
) -> Option<(ObjectGuid, EquipMask)> {
    let (world_object, wielded_location) = try_dequip_object(w, this, object_guid)?;

    if (wielded_location & EquipMask::Selectable) != EquipMask::None {
        // Is this equipped item visible to others?
        let msg = game_message_sound(this, Sound::UnwieldObject, 1.0);
        enqueue_broadcast(w, this, false, msg);
    }

    let msg = game_message_obj_desc_event(w, this);
    enqueue_broadcast(w, this, true, msg);

    // If item has any spells, remove them from the registry on unequip
    let spell_book: Option<Vec<i32>> = object(w, world_object)
        .biota
        .properties_spell_book
        .as_ref()
        .map(|b| b.keys().copied().collect());
    if let Some(spell_book) = spell_book {
        for spell in spell_book {
            #[allow(clippy::cast_sign_loss)] // C#'s (uint)int
            let spell = spell as u32;
            if world_object_weapon::has_proc_spell(object(w, world_object), spell) {
                continue;
            }

            remove_item_spell(w, this, world_object, spell, true);
        }
    }

    if !dropping_to_landscape {
        // This should only be called if the object is going to the private storage, not when dropped on the landscape
        enqueue_action_broadcast_remove_tracked_equipped_object(w, this, world_object);
    }

    Some((world_object, wielded_location))
}

// ACE: Creature.IsInChildLocation
#[must_use]
pub fn is_in_child_location(w: &World, _this: ObjectGuid, item: ObjectGuid) -> bool {
    let o = object(w, item);
    let Some(current_wielded_location) = o.current_wielded_location() else {
        return false;
    };

    if (current_wielded_location & EquipMask::Selectable) != EquipMask::None {
        return true;
    }

    if (current_wielded_location & EquipMask::MissileAmmo) != EquipMask::None {
        let wielder = o.wielder;

        if let Some(creature) =
            wielder.filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_creature))
        {
            let Some(weapon) = get_equipped_missile_weapon(w, creature) else {
                return false;
            };

            if creature_combat::combat_mode(w, creature) == CombatMode::Missile
                && object(w, weapon).biota.weenie_type == WeenieType::MissileLauncher
            {
                return true;
            }
        }
    }

    false
}

/// This method sets properties needed for items that will be child items. Items here are only
/// items equipped in the hands. This deals with the orientation and positioning for visual
/// appearance of the child items held by the parent. If the item isn't in a valid child state
/// (CurrentWieldedLocation), the child properties will be cleared (Placement, ParentLocation,
/// Location).
// ACE: Creature.TrySetChild
fn try_set_child(w: &mut World, this: ObjectGuid, item: ObjectGuid) -> bool {
    if !is_in_child_location(w, this, item) {
        clear_child(w, this, item);
        return false;
    }

    let o = object(w, item);
    let current_wielded_location = o.current_wielded_location().unwrap_or(EquipMask::None);
    let (placement, parent_location) = get_placement_location(o, current_wielded_location);

    let held_item = HeldItem::new(item.full(), parent_location.0, current_wielded_location);
    let location = {
        let creature = object_mut(w, this);
        creature.wo.world_object_properties.children.push(held_item);
        creature.location()
    };

    let o = object_mut(w, item);
    o.set_placement(Some(placement));
    o.set_parent_location(Some(parent_location));
    // DIVERGE: `item.Location = Location` makes the item share the wielder's `Position` object, so
    // the wielder's in-place moves move the item too; here the item takes a copy, and its saved
    // Location stays where the wielder was at the wield until the next `TrySetChild` (V205).
    o.set_location(location);

    true
}

/// `(placement, parentLocation)` for an item wielded at `wielded_location`.
// ACE: Creature.GetPlacementLocation
#[must_use]
pub fn get_placement_location(
    item: &WorldObject,
    wielded_location: EquipMask,
) -> (Placement, ParentLocation) {
    match wielded_location {
        EquipMask::MeleeWeapon | EquipMask::Held | EquipMask::TwoHanded => {
            (Placement::RightHandCombat, ParentLocation::RightHand)
        }

        EquipMask::Shield => {
            if item.item_type() == ItemType::Armor {
                (Placement::Shield, ParentLocation::Shield)
            } else {
                (Placement::RightHandNonCombat, ParentLocation::LeftWeapon)
            }
        }

        EquipMask::MissileWeapon => {
            if item.default_combat_style() == Some(CombatStyle::Bow)
                || item.default_combat_style() == Some(CombatStyle::Crossbow)
            {
                (Placement::LeftHand, ParentLocation::LeftHand)
            } else {
                (Placement::RightHandCombat, ParentLocation::RightHand)
            }
        }

        _ => (Placement::Default, ParentLocation::None),
    }
}

// ACE: Creature.IsWeaponSlot
#[must_use]
pub fn is_weapon_slot(equip_mask: EquipMask) -> bool {
    matches!(
        equip_mask,
        EquipMask::MeleeWeapon
            | EquipMask::Held
            | EquipMask::TwoHanded
            | EquipMask::Shield
            | EquipMask::MissileWeapon
    )
}

/// This clears the child properties: Placement = Resting, ParentLocation = null, Location = null.
// ACE: Creature.ClearChild
pub fn clear_child(w: &mut World, _this: ObjectGuid, item: ObjectGuid) {
    let o = object_mut(w, item);
    o.set_placement(Some(Placement::Resting));
    o.set_parent_location(None);
    o.set_location(None);
}

/// Removes an existing object from Children if exists, and resets to new Child position.
// ACE: Creature.ResetChild
pub fn reset_child(w: &mut World, this: ObjectGuid, item: ObjectGuid) {
    remove_child(w, this, item);
    try_set_child(w, this, item);
}

/// This is called prior to SendSelf to load up the child list for wielded items that are held in
/// a hand.
// ACE: Creature.SetChildren
fn set_children(w: &mut World, this: ObjectGuid) {
    object_mut(w, this)
        .wo
        .world_object_properties
        .children
        .clear();

    for item in equipped_objects_values(w, this) {
        if object(w, item).current_wielded_location().is_some() {
            try_set_child(w, this, item);
        }
    }
}

/// Creates the create list's `Wield` entries (one roll per treasure set) and puts them in the
/// inventory, destroying what does not fit.
// ACE: Creature.GenerateWieldList
pub fn generate_wield_list(w: &mut World, this: ObjectGuid) {
    let Some(create_list) = object(w, this).biota.properties_create_list.clone() else {
        return;
    };

    let wielded: Vec<PropertiesCreateList> = create_list
        .iter()
        .filter(|i| (i.destination_type & DestinationType::Wield) != DestinationType::default())
        .cloned()
        .collect();

    let items = create_list_select(w, &wielded);

    for item in items {
        let wo = world_object_equipment::create_new_world_object_from_create_list(w, &item);

        let Some(wo) = wo else { continue };

        //if (wo.ValidLocations == null || (ItemCapacity ?? 0) > 0)
        {
            let guid = wo.guid;
            assert!(w.objects.insert(wo).is_ok(), "fresh dynamic guid");
            if !container::try_add_to_inventory(w, this, guid, 0, false, true) {
                crate::world_objects::world_object::destroy(w, guid, true, false);
            }
        }
        //else
        //TryWieldObject(wo, (EquipMask)wo.ValidLocations);
    }
}

/// `DestinationType.HasFlag(flag)`.
fn has_flag(value: DestinationType, flag: DestinationType) -> bool {
    (value & flag) == flag
}

/// Selects the create-list entries to create: every non-treasure entry, and one entry of each
/// treasure set (a run of entries whose probabilities, in `Shade`, add up to 1).
// ACE: Creature.CreateListSelect
#[must_use]
pub fn create_list_select(
    w: &World,
    create_list: &[PropertiesCreateList],
) -> Vec<PropertiesCreateList> {
    let trophy_drop_rate = property_manager_get_double(w, "trophy_drop_rate");
    #[allow(clippy::float_cmp)] // C#'s `!=`
    if trophy_drop_rate != 1.0 {
        #[allow(clippy::cast_possible_truncation)] // C#'s (float)double
        return create_list_select_with_drop_rate(create_list, trophy_drop_rate as f32);
    }

    let mut rng = ThreadSafeRandom::next_float(0.0, 1.0);
    let mut total_probability = 0.0f32;
    let mut rng_selected = false;

    let mut results = Vec::new();

    for item in create_list {
        let destination_type = item.destination_type;
        let use_rng = has_flag(destination_type, DestinationType::Treasure) && item.shade != 0.0;

        let shade_or_probability = item.shade;

        if use_rng {
            // handle sets in 0-1 chunks
            if total_probability >= 1.0 {
                total_probability = 0.0;
                rng = ThreadSafeRandom::next_float(0.0, 1.0);
                rng_selected = false;
            }

            let probability = shade_or_probability;

            total_probability += probability;

            if rng_selected || rng >= f64::from(total_probability) {
                continue;
            }

            rng_selected = true;
        }

        results.push(item.clone());
    }

    results
}

/// The `CreateListSelect(createList, dropRateMod)` overload: the trophy chances of each set scaled
/// by `drop_rate_mod`.
// ACE: Creature.CreateListSelect
#[must_use]
pub fn create_list_select_with_drop_rate(
    create_list: &[PropertiesCreateList],
    drop_rate_mod: f32,
) -> Vec<PropertiesCreateList> {
    let create_list_entity = crate::entity::create_list::CreateList::new(create_list);
    // (TrophyMod, NoneMod) of `CreateListSetModifier`.
    let mut modifier: Option<(f32, f32)> = None;

    let mut rng = ThreadSafeRandom::next_float(0.0, 1.0);
    let mut total_probability = 0.0f32;
    let mut rng_selected = false;

    let mut results = Vec::new();

    for (i, item) in create_list.iter().enumerate() {
        let destination_type = item.destination_type;
        let use_rng = has_flag(destination_type, DestinationType::Treasure) && item.shade != 0.0;

        let shade_or_probability = item.shade;

        if use_rng {
            // handle sets in 0-1 chunks
            if total_probability == 0.0 || total_probability >= 1.0 {
                total_probability = 0.0;
                rng = ThreadSafeRandom::next_float(0.0, 1.0);
                rng_selected = false;

                let m = create_list_entity.get_set_modifier(i, drop_rate_mod);
                modifier = Some((m.trophy_mod, m.none_mod()));
            }

            let (trophy_mod, none_mod) = modifier.expect("set above");
            let probability = shade_or_probability
                * if item.weenie_class_id != 0 {
                    trophy_mod
                } else {
                    none_mod
                };

            total_probability += probability;

            if rng_selected || rng >= f64::from(total_probability) {
                continue;
            }

            rng_selected = true;
        }

        results.push(item.clone());
    }

    results
}

/// `Creature.WieldedTreasure`: the wielded-treasure table of `WieldedTreasureType`, or `None`.
// ACE: Creature.WieldedTreasure
#[must_use]
pub fn wielded_treasure(w: &World, this: ObjectGuid) -> Option<Arc<Vec<TreasureWielded>>> {
    let wielded_treasure_type = object(w, this).wielded_treasure_type()?;
    Some(w.content.get_cached_wielded_treasure(wielded_treasure_type))
}

/// Rolls the creature's wielded-treasure table and puts the items in its inventory, destroying
/// what does not fit.
// ACE: Creature.GenerateWieldedTreasure
pub fn generate_wielded_treasure(w: &mut World, this: ObjectGuid) {
    let Some(table) = wielded_treasure(w, this) else {
        return;
    };

    //var table = new TreasureWieldedTable(WieldedTreasure);

    let Some(wielded_treasure) = world_object_equipment::generate_wielded_treasure_sets(w, &table)
    else {
        return;
    };

    for item in wielded_treasure {
        //if (item.ValidLocations == null || (ItemCapacity ?? 0) > 0)
        {
            let guid = item.guid;
            assert!(w.objects.insert(item).is_ok(), "fresh dynamic guid");
            if !container::try_add_to_inventory(w, this, guid, 0, false, true) {
                crate::world_objects::world_object::destroy(w, guid, true, false);
            }
        }
        //else
        //TryWieldObject(item, (EquipMask)item.ValidLocations);
    }
}

/// # Panics
/// When there is no death treasure and the wielded table rolls nothing (see the ACE-BUG).
// ACE: Creature.GenerateInventoryTreasure
// ACE-BUG: with no death treasure, a wielded-treasure table that generates nothing makes GenerateWieldedTreasureSets return null, and the foreach over it throws NullReferenceException.
pub fn generate_inventory_treasure(w: &mut World, this: ObjectGuid) {
    let Some(inventory_treasure_type) =
        object(w, this).inventory_treasure_type().filter(|&t| t > 0)
    else {
        return;
    };

    // based on property name found in older data, this property was only found 5 weenies (entirely contained in Focusing Stone quest)
    // guessing that the value might have possibly allowed for either Death or Wielded treasure, but technically it might have only been the former.
    // so for now, coded for checking both types.
    // Although the property's name seemingly was removed, either it's value was still used in code OR its value was moved into DeathTreasureType/CreateList
    // because pcaps for these 5 objects do show similar, if not exact, results on corpses.

    let treasure_death = w.content.get_cached_death_treasure(inventory_treasure_type);
    let treasure_wielded = w
        .content
        .get_cached_wielded_treasure(inventory_treasure_type);

    let treasure: Option<Vec<WorldObject>> = if let Some(treasure_death) = treasure_death {
        Some(
            crate::factories::loot_generation_factory::create_random_loot_objects(
                w,
                &treasure_death,
            ),
        )
    } else {
        world_object_equipment::generate_wielded_treasure_sets(w, &treasure_wielded)
    };

    for item in treasure.expect("System.NullReferenceException: treasure") {
        let guid = item.guid;
        assert!(w.objects.insert(item).is_ok(), "fresh dynamic guid");
        object_mut(w, guid).wo.world_object.destination_type = DestinationType::Treasure;
        // add this flag so item can move over to corpse upon death
        // (ACE logic: it is likely all inventory of a creature was moved over without reservation (bonded rules enforced), but ACE is slightly different in how it handles it for net same result)

        if !container::try_add_to_inventory(w, this, guid, 0, false, true) {
            crate::world_objects::world_object::destroy(w, guid, true, false);
        }
    }
}
