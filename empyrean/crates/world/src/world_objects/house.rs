// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/House.cs
//! Port of `Source/ACE.Server/WorldObjects/House.cs`.
//!
//! # Offline copies
//!
//! ACE's `House.Load` builds a private copy of a house (with its linked children and its
//! slumlord's paid items) from the shard when the house's landblock is not loaded, and callers
//! (`Player.House`, the rent queue, `RootHouse`) hold that copy. The port's objects live in
//! `World.objects`, keyed by guid, so a copy uses the house's own guids.
//! Each copy is registered in `HouseManagerState.offline`; a later `Load` of the same house reuses
//! the registered copy (every change ACE makes to a copy goes through it and is saved), and the
//! copy leaves the store when the landblock that owns the house loads its own objects
//! ([`evict_offline_copies`]). A guid held by a player or the rent queue therefore names the
//! live house when its landblock is loaded, and the copy otherwise ([`resolve`] reloads a copy
//! for a guid whose object has left the store).

use std::sync::Arc;

use empyrean_common::dotnet::{CsCast, DotNetDict, TimeSpan};
use empyrean_content::models::world::HousePortal as DbHousePortal;
use empyrean_entity::enums::{
    HookGroupType, HouseStatus, HouseType, MotionCommand, MotionStance, PhysicsState, PlayScript,
    PositionType, PropertyBool, PropertyDataId, PropertyInstanceId, PropertyInt, PropertyString,
    WeenieType,
};
use empyrean_entity::{Biota, LandblockId, ObjectGuid};

use crate::entity::i_player::{self, IPlayer};
use crate::entity::landblock;
use crate::entity::position_extensions::get_outdoor_cell;
use crate::factories::world_object_factory;
use crate::managers::{house_manager, landblock_manager, player_manager};
use crate::network::game_event::events::game_event_house_update_restrictions::game_event_house_update_restrictions;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_public_update_instance_id::game_message_public_update_instance_id;
use crate::network::motion::movement_data::Motion;
use crate::network::structure::house_data::{house_data_new, HouseData};
use crate::network::structure::restriction_db::{restriction_db_new, RestrictionDB};
use crate::physics::{object_maint, phys_ext};
use crate::world_objects::kinds::KindData;
use crate::world_objects::world_object::{CtorEnv, CtorSource, WorldObject};
use crate::world_objects::{
    hook, player_house, slum_lord, world_object_links, world_object_networking,
};
use crate::{dispatch, World};

// ACE: House.MaxGuests
pub const MAX_GUESTS: usize = 128;

/// Non-property fields declared in `House.cs`.
#[derive(Debug, Default)]
pub struct HouseFields {
    // TODO now that the new biota model uses a dictionary for this, see if we can remove this duplicate dictionary
    // ACE: House.Guests
    pub guests: DotNetDict<ObjectGuid, bool>,
    /// For linking mansions (guids of the other houses in the store)
    // ACE: House.LinkedHouses
    pub linked_houses: Vec<ObjectGuid>,
    // ACE: House._dungeonLandblockID
    pub dungeon_landblock_id: Option<u32>,
    // ACE: House._dungeonHouseGuid
    pub dungeon_house_guid: Option<u32>,
    // ACE: House._rootGuid
    pub root_guid: Option<ObjectGuid>,
}

// ================================================================================ helpers

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// This house's fields.
///
/// # Panics
/// When `this` is gone or is not a `House`.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &HouseFields {
    match &obj(w, this).kind {
        KindData::House(h) => &h.house,
        _ => panic!(
            "System.InvalidCastException: 0x{:08X} is not a House",
            this.full()
        ),
    }
}

/// This house's fields, mutably.
///
/// # Panics
/// As [`fields`].
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut HouseFields {
    match &mut obj_mut(w, this).kind {
        KindData::House(h) => &mut h.house,
        _ => panic!(
            "System.InvalidCastException: 0x{:08X} is not a House",
            this.full()
        ),
    }
}

fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

/// `wo as House != null`.
fn is_house(w: &World, g: ObjectGuid) -> bool {
    w.objects.get(g).is_some_and(WorldObject::is_house)
}

/// `ChildLinks`, in list order.
fn child_links(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    w.objects
        .get(this)
        .map(|o| o.wo.world_object_links.child_links.clone())
        .unwrap_or_default()
}

/// `LandblockId(landblock << 16 | 0xFFFF)` for the landblock a house guid lives on.
#[must_use]
pub fn landblock_id_of_house_guid(house_guid: u32) -> LandblockId {
    let landblock = (house_guid >> 12) & 0xFFFF;
    LandblockId::new(landblock << 16 | 0xFFFF)
}

/// `loaded.GetObject(guid) as House` on a loaded landblock.
fn landblock_house(w: &World, landblock_id: LandblockId, guid: ObjectGuid) -> Option<ObjectGuid> {
    landblock::get_object(w, landblock_id, guid, true).filter(|&g| is_house(w, g))
}

/// `CurrentLandblock?.HasDungeon` for an object.
fn current_landblock_has_dungeon(w: &mut World, g: ObjectGuid) -> Option<bool> {
    let id = w.objects.get(g)?.current_landblock?;
    Some(w.landblock_manager.landblocks.get_mut(id)?.has_dungeon())
}

// ================================================================================ properties

/// house open/closed status
/// 0 = closed, 1 = open
// ACE: House.OpenStatus
#[must_use]
pub fn open_status(w: &World, this: ObjectGuid) -> bool {
    obj(w, this).open_to_everyone()
}

// ACE: House.OpenStatus
pub fn set_open_status(w: &mut World, this: ObjectGuid, value: bool) {
    obj_mut(w, this).set_open_to_everyone(value);
}

// ACE: House.SlumLord
#[must_use]
pub fn slum_lord(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    child_links(w, this)
        .into_iter()
        .find(|&l| w.objects.get(l).is_some_and(WorldObject::is_slum_lord))
}

// ACE: House.Hooks
#[must_use]
pub fn hooks(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    child_links(w, this)
        .into_iter()
        .filter(|&l| w.objects.get(l).is_some_and(WorldObject::is_hook))
        .collect()
}

// ACE: House.Storage
#[must_use]
pub fn storage(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    child_links(w, this)
        .into_iter()
        .filter(|&l| w.objects.get(l).is_some_and(WorldObject::is_storage))
        .collect()
}

// ACE: House.StorageAccess
#[must_use]
pub fn storage_access(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    fields(w, this)
        .guests
        .iter()
        .filter(|(_, v)| **v)
        .map(|(k, _)| *k)
        .collect()
}

// ACE: House.BootSpot
#[must_use]
pub fn boot_spot(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    child_links(w, this).into_iter().find(|&l| {
        w.objects
            .get(l)
            .is_some_and(|o| o.biota.weenie_type == WeenieType::BootSpot)
    })
}

// ACE: House.HousePortal
#[must_use]
pub fn house_portal(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    child_links(w, this)
        .into_iter()
        .find(|&l| w.objects.get(l).is_some_and(WorldObject::is_house_portal))
}

// ACE: House.Linkspots
#[must_use]
pub fn linkspots(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    child_links(w, this)
        .into_iter()
        .filter(|&l| is_link_spot(w, l))
        .collect()
}

/// `WeenieType == Generic && WeenieClassName.Equals("portaldestination")` (`WorldObject.IsLinkSpot`).
#[must_use]
pub fn is_link_spot(w: &World, g: ObjectGuid) -> bool {
    let Some(o) = w.objects.get(g) else {
        return false;
    };
    o.biota.weenie_type == WeenieType::Generic
        && weenie_class_name(w, o.biota.weenie_class_id) == "portaldestination"
}

/// `WorldObject.WeenieClassName`: the cached weenie's class name.
pub(crate) fn weenie_class_name(w: &World, wcid: u32) -> String {
    w.content
        .get_cached_weenie(wcid)
        .and_then(|x| x.class_name.clone())
        .unwrap_or_default()
}

// ACE: House.IsApartment
#[must_use]
pub fn is_apartment(w: &World, this: ObjectGuid) -> bool {
    obj(w, this).house_type() == HouseType::Apartment
}

// ACE: House.HasDungeon
#[must_use]
pub fn has_dungeon(w: &World, this: ObjectGuid) -> bool {
    house_portal(w, this).is_some()
}

// ================================================================================ construction

/// `new House(weenie, guid)` / `new House(biota)`: the `WorldObject` constructor, then House's
/// `InitializePropertyDictionaries` and `SetEphemeralValues`.
// ACE: House.House
pub fn house_ctor(o: &mut WorldObject, env: &CtorEnv<'_>, src: CtorSource) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    house_initialize_property_dictionaries(o);
    house_set_ephemeral_values(o, env);
}

// ACE: House.InitializePropertyDictionaries
fn house_initialize_property_dictionaries(o: &mut WorldObject) {
    if o.biota.house_permissions.is_none() {
        o.biota.house_permissions = Some(DotNetDict::new());
    }
}

// ACE: House.SetEphemeralValues
fn house_set_ephemeral_values(o: &mut WorldObject, env: &CtorEnv<'_>) {
    // Not ACE's (retail, V334): a house's restriction effect is its data id 44
    // (blue where the weenie gives none), sent as the description's script; its physics
    // description carries no default script, so a blue default script a saved house still holds
    // is dropped.
    if o.get_property(PropertyDataId::RestrictionEffect).is_none() {
        o.set_property(
            PropertyDataId::RestrictionEffect,
            PlayScript::RestrictionEffectBlue.0,
        );
    }
    if o.default_script_id() == Some(PlayScript::RestrictionEffectBlue.0) {
        o.set_default_script_id(None);
    }

    build_guests_on(o, env.w);

    // `LinkedHouses = new List<House>();`: the field's default
}

/// `BuildGuests` from the constructor, on an object that is not in the store yet.
///
/// The constructor has no `&mut World`, so a deleted guest is removed from the biota and the
/// object marked changed, and ACE's immediate `SaveBiotaToDatabase` is left to the object's next
/// save.
// DIVERGE: BuildGuests from the constructor defers the save of a deleted guest's removal to the object's next save (no &mut World in a constructor).
fn build_guests_on(o: &mut WorldObject, w: &World) {
    let mut guests = DotNetDict::new();

    let house_permissions = o.biota.clone_house_permissions();

    let mut deleted = Vec::new();

    for (key, value) in house_permissions.iter() {
        let (player, _) = player_manager::find_by_guid(w, *key);
        let Some(player) = player else {
            //Console.WriteLine($"{Name}.BuildGuests(): couldn't find guest {kvp.Key:X8}");
            log::warn!(
                "[HOUSE] {}.BuildGuests(): couldn't find guest {:08X}",
                o.get_property(PropertyString::Name).unwrap_or_default(),
                key
            );

            // character has been deleted -- automatically remove?
            deleted.push(*key);
            continue;
        };
        guests.add(player.guid(), *value);
    }

    if !deleted.is_empty() {
        for guid in deleted {
            o.biota.remove_house_guest(guid);
        }

        o.wo.world_object_database.changes_detected = true;
    }

    if let KindData::House(h) = &mut o.kind {
        h.house.guests = guests;
    }
}

// ACE: House.BuildGuests
pub fn build_guests(w: &mut World, this: ObjectGuid) {
    let mut guests = DotNetDict::new();

    let house_permissions = obj(w, this).biota.clone_house_permissions();

    let mut deleted = Vec::new();

    for (key, value) in house_permissions.iter() {
        let (player, _) = player_manager::find_by_guid(w, *key);
        let Some(player) = player else {
            //Console.WriteLine($"{Name}.BuildGuests(): couldn't find guest {kvp.Key:X8}");
            log::warn!(
                "[HOUSE] {}.BuildGuests(): couldn't find guest {:08X}",
                name(w, this),
                key
            );

            // character has been deleted -- automatically remove?
            deleted.push(*key);
            continue;
        };
        guests.add(player.guid(), *value);
    }

    fields_mut(w, this).guests = guests;

    if !deleted.is_empty() {
        for guid in deleted {
            obj_mut(w, this).biota.remove_house_guest(guid);
        }

        obj_mut(w, this).wo.world_object_database.changes_detected = true;

        dispatch::save_biota_to_database::save_biota_to_database(w, this, true);
    }
}

// ================================================================================ HouseData, Load

/// Builds a HouseData structure for this house
/// This is used to populate the info in the House panel
// ACE: House.GetHouseData
pub fn get_house_data(w: &mut World, this: ObjectGuid, owner: Option<IPlayer>) -> HouseData {
    let mut house_data = house_data_new(w);
    house_data.position = obj(w, this).location();
    house_data.r#type = obj(w, this).house_type();

    let slum_lord = slum_lord(w, this);
    match slum_lord {
        None => {
            //Console.WriteLine($"No slumlord found for {Name} ({Guid})");
            log::warn!("[HOUSE] No slumlord found for {} ({})", name(w, this), this);
        }
        Some(s) => {
            let buy = slum_lord::get_buy_items(w, s);
            house_data.set_buy_items(w, &buy);
            let rent = slum_lord::get_rent_items(w, s);
            house_data.set_rent_items(w, &rent);
        }
    }

    if let Some(owner) = owner {
        let purchase =
            i_player::get_property(w, owner, PropertyInt::HousePurchaseTimestamp).unwrap_or(0);
        house_data.buy_time = purchase.cast_unsigned();
        house_data.rent_time = get_rent_timestamp(w, this, house_data.buy_time);
        // `houseData.SetPaidItems(SlumLord)`: a null slumlord throws in SetPaidItems.
        let s = slum_lord.expect("System.NullReferenceException: SetPaidItems(null)");
        house_data.set_paid_items(w, s);
    }

    if obj(w, this).house_status() == HouseStatus::InActive {
        house_data.maintenance_free = true;
    }

    house_data
}

/// The offline copy of `house_guid` (the house and its linked children in the store with no
/// landblock, the slumlord from its own shard biota). A registered copy is reused (see the
/// module doc). `None` where ACE returns null (no slumlord, unless `is_basement`).
///
/// # Panics
/// Where ACE throws: no shard biota and no world instance for the guid (`biota.WeenieClassId` on
/// null), or no object created for it.
// ACE: House.Load
pub fn load(w: &mut World, house_guid: u32, is_basement: bool) -> Option<ObjectGuid> {
    let guid = ObjectGuid::new(house_guid);

    // DIVERGE: a registered offline copy is reused rather than rebuilt from the shard, and a live
    // house of the guid (its landblock still creating its objects) answers for ACE's second copy:
    // the store holds one object per guid (see the module doc).
    if w.objects.get(guid).is_some_and(WorldObject::is_house) {
        if is_basement {
            return Some(guid);
        }
        return slum_lord(w, guid).map(|_| guid);
    }

    let landblock = u16::try_from((house_guid >> 12) & 0xFFFF).expect("16 bits");

    let biota = shard_get_biota(w, house_guid);
    let instances = w.content.get_cached_instances_by_landblock(landblock);

    let biota = match biota {
        Some(b) => Some(b),
        None => instances
            .iter()
            .find(|h| h.guid == house_guid)
            .and_then(|house_instance| {
                let weenie = w.content.get_cached_weenie(house_instance.weenie_class_id);
                let object_guid = ObjectGuid::new(house_instance.guid);

                let new_world_object = CtorEnv::with_world(w, |env| {
                    world_object_factory::create_world_object(env, weenie, object_guid)
                });

                // `BiotaConverter.ConvertFromEntityBiota(newWorldObject.Biota)`
                new_world_object.map(|o| o.biota)
            }),
    };
    let biota = biota.expect("System.NullReferenceException: biota is null in House.Load");
    let weenie_class_id = biota.weenie_class_id;

    let biotas = vec![biota];
    let linked_houses = CtorEnv::with_world(w, |env| {
        world_object_factory::create_new_world_objects(
            env,
            &instances,
            &biotas,
            Some(weenie_class_id),
        )
    })
    .unwrap_or_else(|a| {
        panic!(
            "System.NullReferenceException: CreateNewWorldObjects aborted on 0x{:08X}",
            a.instance_guid
        )
    });

    let mut tops = Vec::new();
    for o in linked_houses {
        let g = o.guid;
        if let Err(dup) = w.objects.insert(o) {
            log::error!(
                "[HOUSE] House.Load: object 0x{} is already in the world",
                dup.guid
            );
            continue;
        }
        crate::world_objects::creature::post_insert(w, g);
        tops.push(g);
    }
    let first = *tops
        .first()
        .expect("System.ArgumentOutOfRangeException: linkedHouses[0]");
    house_manager::register_offline_copy(w, guid, tops.clone());

    for &linked_house in &tops {
        world_object_links::activate_links(w, linked_house, &instances, &biotas, Some(first));
    }

    assert!(
        is_house(w, first),
        "System.InvalidCastException: (House)linkedHouses[0]"
    );
    let house = first;

    if is_basement {
        return Some(house);
    }

    // load slumlord biota for rent
    let Some(old_slumlord) = slum_lord(w, house) else {
        // this can happen for basement dungeons
        //Console.WriteLine($"House.Load({houseGuid:X8}): couldn't find slumlord!");
        return None;
    };

    if let Some(slumlord_biota) = shard_get_biota(w, old_slumlord.full()) {
        let slumlord = CtorEnv::with_world(w, |env| {
            world_object_factory::create_world_object_from_biota(env, slumlord_biota)
        });
        if let Some(slumlord) = slumlord {
            // the weenie-built slumlord ActivateLinks made has the same guid: it leaves the store
            // before the shard's copy takes its place (ACE drops its reference).
            w.objects.remove(old_slumlord);
            let _ = w.objects.insert(slumlord);
            let slumlord = old_slumlord;

            dispatch::set_link_properties::set_link_properties(w, house, slumlord);

            let links = &mut obj_mut(w, house).wo.world_object_links.child_links;
            if let Some(i) = links.iter().position(|&l| l == old_slumlord) {
                links.remove(i);
            }
            links.push(slumlord);

            obj_mut(w, slumlord).wo.world_object_links.parent_link = Some(house);

            // `new SlumLord(biota)` starts its inventory load (V101: the inserter runs it)
            house_manager::load_offline_inventory(w, guid, slumlord);
        }
    }
    Some(house)
}

/// `DatabaseManager.Shard.BaseDatabase.GetBiota(id)`, converted to the entity model.
pub(crate) fn shard_get_biota(w: &World, id: u32) -> Option<Biota> {
    let b = w.shard.base_database().get_biota(id, false)?;
    Some(
        empyrean_store::adapter::biota_converter::BiotaConverter::convert_to_entity_biota(
            &b, false,
        ),
    )
}

/// Removes the registered offline copies of houses on `landblock` from the store (with their
/// children and the slumlord's items), before the landblock creates its own objects. Called by
/// `Landblock.CreateWorldObjects` (not ACE: see the module doc).
pub fn evict_offline_copies(w: &mut World, landblock: u16) {
    for root in house_manager::offline_copies_on_landblock(w, landblock) {
        house_manager::evict_offline_copy(w, root);
    }
}

/// A guid held for a house (`Player.House`, the rent queue): the object when it is in the store,
/// else the offline copy `House.GetHouse` gives.
///
/// ACE keeps the unloaded object alive through the reference; here a house whose landblock
/// unloaded left the store and is reloaded.
// DIVERGE: a held house guid whose object left the store (its landblock unloaded) is reloaded as an offline copy; ACE keeps the unloaded object alive through the reference.
pub fn resolve(w: &mut World, house: ObjectGuid) -> Option<ObjectGuid> {
    if is_house(w, house) {
        return Some(house);
    }
    get_house(w, house.full())
}

// ================================================================================ rent

/// The client automatically adds this amount of time to the beginning of the current maintenance period
// ACE: House.RentInterval
pub const RENT_INTERVAL: TimeSpan = TimeSpan::from_ticks(30 * 24 * 3600 * 10_000_000);

/// Returns the beginning of the current maintenance period
// ACE: House.GetRentTimestamp
#[must_use]
pub fn get_rent_timestamp(w: &World, this: ObjectGuid, purchase_time: u32) -> u32 {
    get_rent_timestamp_at(w.now.unix_time, is_apartment(w, this), purchase_time)
}

/// `(uint)RentInterval.TotalSeconds`, tripled for apartments.
fn rent_interval_secs(is_apartment: bool) -> u32 {
    let mut rent_interval_secs: u32 = RENT_INTERVAL.total_seconds().cs_cast();
    if is_apartment {
        rent_interval_secs = rent_interval_secs.wrapping_mul(3); // apartment maintenance every 90 days
    }
    rent_interval_secs
}

/// [`get_rent_timestamp`] at `unix_time` (`Time.GetUnixTime()`).
#[must_use]
pub fn get_rent_timestamp_at(unix_time: f64, is_apartment: bool, purchase_time: u32) -> u32 {
    // get the purchaseTime -> currentTime offset
    let current_time: u32 = unix_time.cs_cast();
    let offset = current_time.wrapping_sub(purchase_time);

    // calculate # of full periods in offset
    let rent_interval_secs = rent_interval_secs(is_apartment);
    let periods = offset / rent_interval_secs;

    // return beginning of current period
    purchase_time.wrapping_add(rent_interval_secs.wrapping_mul(periods))
}

/// Returns the end of the current maintenance period
// ACE: House.GetRentDue
#[must_use]
pub fn get_rent_due(w: &World, this: ObjectGuid, purchase_time: u32) -> u32 {
    get_rent_due_at(w.now.unix_time, is_apartment(w, this), purchase_time)
}

/// [`get_rent_due`] at `unix_time`.
#[must_use]
pub fn get_rent_due_at(unix_time: f64, is_apartment: bool, purchase_time: u32) -> u32 {
    let current_period = get_rent_timestamp_at(unix_time, is_apartment, purchase_time);

    let rent_interval_secs = rent_interval_secs(is_apartment);

    current_period.wrapping_add(rent_interval_secs)
}

// ================================================================================ links

// ACE: House.SetLinkProperties
pub fn house_set_link_properties(w: &mut World, this: ObjectGuid, wo: ObjectGuid) {
    // for house dungeons, link to outdoor house properties
    let mut house_owner = obj(w, this).house_owner();
    let mut house_id = obj(w, this).house_id();
    let mut house_hooks_visible = obj(w, this).house_hooks_visible();
    if current_landblock_has_dungeon(w, this) == Some(true)
        && obj(w, this).house_type() != HouseType::Apartment
    {
        let wcid = obj(w, this).biota.weenie_class_id;
        let location_landblock = obj(w, this).location().map(|l| l.landblock());
        let biotas = w.shard.base_database().get_biotas_by_wcid(wcid);
        let location_type = PositionType::Location.0;
        let biota = biotas
            .into_iter()
            .filter(|bio| !bio.biota_properties_position.is_empty())
            .find(|b| {
                let p = b
                    .biota_properties_position
                    .iter()
                    .find(|p| p.position_type == location_type)
                    .expect("System.NullReferenceException: no Location position");
                Some(p.obj_cell_id >> 16) != location_landblock
            });
        if let Some(biota) = biota {
            let biota =
                empyrean_store::adapter::biota_converter::BiotaConverter::convert_to_entity_biota(
                    &biota, false,
                );
            let house = CtorEnv::with_world(w, |env| {
                world_object_factory::create_world_object_from_biota(env, biota)
            })
            .filter(WorldObject::is_house)
            .expect("System.NullReferenceException: (House)CreateWorldObject(biota)");
            let (owner, owner_name) = (house.house_owner(), house.house_owner_name());
            let o = obj_mut(w, this);
            o.set_house_owner_prop(owner);
            o.set_house_owner_name(owner_name);
            house_owner = house.house_owner();
            house_id = house.house_id();
            house_hooks_visible = house.house_hooks_visible();
        }
    }

    //Console.WriteLine($"House.SetLinkProperties({wo.Name}) (0x{wo.Guid}): WeenieType {wo.WeenieType} | HouseId:{house.HouseId} | HouseOwner: {house.HouseOwner} | HouseOwnerName: {house.HouseOwnerName}");

    {
        let o = obj_mut(w, wo);
        o.set_house_id(house_id);
        o.set_house_owner_prop(house_owner);
        //wo.HouseInstance = house.HouseInstance;
        //wo.HouseOwnerName = house.HouseOwnerName;
    }

    if house_owner.is_some() && obj(w, wo).is_slum_lord() {
        world_object_networking::shims::set_current_motion_state(
            w,
            wo,
            Some(Motion::new(MotionStance::Invalid, MotionCommand::On, 1.0)),
        );
    }

    // the inventory items haven't been loaded yet
    if obj(w, wo).is_hook() {
        if hook::has_item(w, wo) {
            set_hook_physics(w, wo, false, false, false);
        } else if !house_hooks_visible.unwrap_or(true) {
            set_hook_physics(w, wo, true, true, true);
        }
    }
}

/// `hook.NoDraw = ..; hook.UiHidden = ..; hook.Ethereal = ..;`
pub(crate) fn set_hook_physics(
    w: &mut World,
    hook: ObjectGuid,
    no_draw: bool,
    ui_hidden: bool,
    ethereal: bool,
) {
    phys_ext::set_physics_property_state(
        w,
        hook,
        PropertyBool::NoDraw,
        PhysicsState::NoDraw,
        Some(no_draw),
    );
    obj_mut(w, hook).set_ui_hidden(ui_hidden);
    phys_ext::set_physics_property_state(
        w,
        hook,
        PropertyBool::Ethereal,
        PhysicsState::Ethereal,
        Some(ethereal),
    );
}

// ACE: House.UpdateLinkProperties
pub fn house_update_link_properties(w: &mut World, this: ObjectGuid, wo: ObjectGuid) {
    let house_owner = obj(w, this).house_owner();
    if obj(w, wo).house_owner() != house_owner {
        //Console.WriteLine($"{Name}.UpdateLinkProperties({wo.Name} - {wo.Guid}) - HouseOwner: {HouseOwner:X8}");
        let m = game_message_public_update_instance_id(
            obj_mut(w, wo),
            PropertyInstanceId::HouseOwner,
            ObjectGuid::new(house_owner.unwrap_or(0)),
        );
        world_object_networking::enqueue_broadcast(w, wo, true, &[m]);
    }

    house_set_link_properties(w, this, wo);
}

// ================================================================================ permissions

/// Returns TRUE if this player has guest or storage access
// ACE: House.HasPermission
#[must_use]
pub fn has_permission(w: &World, this: ObjectGuid, player: ObjectGuid, storage: bool) -> bool {
    let h = obj(w, this);
    let Some(house_owner) = h.house_owner() else {
        return false;
    };

    if player.full() == house_owner {
        return true;
    }

    let (owner, _) = player_manager::find_by_guid(w, house_owner);

    if let Some(owner_account) = owner.and_then(|o| i_player::account(w, o)) {
        let player_account = player_house::account(w, player);
        if owner_account.account_id == player_account.account_id {
            return true;
        }
    }

    // handle allegiance permissions
    if let Some(monarch_id) = h.monarch_id() {
        if player_house::allegiance::monarch_id(w, player) == Some(monarch_id) {
            if storage {
                if storage_access(w, this).contains(&ObjectGuid::new(monarch_id)) {
                    return true;
                }
            } else if fields(w, this)
                .guests
                .contains_key(&ObjectGuid::new(monarch_id))
            {
                return true;
            }
        }
    }

    if storage {
        storage_access(w, this).contains(&player)
    } else {
        h.open_to_everyone() || fields(w, this).guests.contains_key(&player)
    }
}

// ACE: House.AddGuest
pub fn add_guest(w: &mut World, this: ObjectGuid, guest: IPlayer, storage: bool) {
    let o = obj_mut(w, this);
    o.biota
        .add_or_update_house_guest(guest.guid().full(), storage);
    o.wo.world_object_database.changes_detected = true;

    build_guests(w, this);
    update_restriction_db(w, this, None);

    if obj(w, this).current_landblock.is_none() {
        dispatch::save_biota_to_database::save_biota_to_database(w, this, true);
    }
}

// ACE: House.ModifyGuest
pub fn modify_guest(w: &mut World, this: ObjectGuid, guest: IPlayer, storage: bool) {
    let existing_storage = obj(w, this)
        .biota
        .get_house_guest_storage_permission(guest.guid().full());

    let Some(existing_storage) = existing_storage else {
        //Console.WriteLine($"{Name}.FindGuest({guest.Guid}): couldn't find {guest.Name}");
        log::warn!(
            "[HOUSE] {}.FindGuest({}): couldn't find {}",
            name(w, this),
            guest.guid(),
            i_player::name(w, guest).unwrap_or_default()
        );

        return;
    };

    if existing_storage == storage {
        return;
    }

    let o = obj_mut(w, this);
    o.biota
        .add_or_update_house_guest(guest.guid().full(), storage);
    o.wo.world_object_database.changes_detected = true;

    build_guests(w, this);
    update_restriction_db(w, this, None);

    if obj(w, this).current_landblock.is_none() {
        dispatch::save_biota_to_database::save_biota_to_database(w, this, true);
    }
}

// ACE: House.RemoveGuest
pub fn remove_guest(w: &mut World, this: ObjectGuid, guest: IPlayer) {
    if !obj_mut(w, this)
        .biota
        .remove_house_guest(guest.guid().full())
    {
        return;
    }

    obj_mut(w, this).wo.world_object_database.changes_detected = true;

    build_guests(w, this);
    update_restriction_db(w, this, None);

    if obj(w, this).current_landblock.is_none() {
        dispatch::save_biota_to_database::save_biota_to_database(w, this, true);
    }
}

// ACE: House.ClearPermissions
pub fn clear_permissions(w: &mut World, this: ObjectGuid) {
    // `foreach (var guest in Guests.Keys)`: RemoveGuest's BuildGuests assigns a new dictionary, so
    // the enumeration runs over the one it started with.
    let guests: Vec<ObjectGuid> = fields(w, this).guests.keys().copied().collect();
    for guest in guests {
        let (player, _) = player_manager::find_by_guid(w, guest.full());
        let Some(player) = player else {
            //Console.WriteLine($"{Name}.ClearPermissions(): couldn't find {guest}");
            log::warn!(
                "[HOUSE] {}.ClearPermissions(): couldn't find {}",
                name(w, this),
                guest
            );
            continue;
        };
        remove_guest(w, this, player);
    }
    obj_mut(w, this).set_open_to_everyone(true);
}

/// Returns the database HousePortals for a HouseID
///
/// # Panics
/// When the house has no `HouseId` (`HouseId.Value`).
// ACE: House.GetHousePortals
#[must_use]
pub fn get_house_portals(w: &World, this: ObjectGuid) -> Arc<Vec<DbHousePortal>> {
    // the database house portals are different from the HousePortal weenie objects
    // the db info contains the portal destinations

    let house_id = obj(w, this)
        .house_id()
        .expect("System.InvalidOperationException: HouseId.Value");
    w.content.get_cached_house_portals(house_id)
}

// ACE: House.DungeonLandblockID
pub fn dungeon_landblock_id(w: &mut World, this: ObjectGuid) -> u32 {
    if let Some(v) = fields(w, this).dungeon_landblock_id {
        return v;
    }
    let root_house = root_house(w, this).expect("System.NullReferenceException: RootHouse");
    let root_house_block = obj(w, root_house)
        .location()
        .expect("System.NullReferenceException: RootHouse.Location")
        .landblock_id()
        .raw()
        | 0xFFFF;

    let house_portals = get_house_portals(w, this);

    let Some(dungeon_portal) = house_portals
        .iter()
        .find(|i| (i.obj_cell_id | 0xFFFF) != root_house_block)
    else {
        return 0;
    };

    let v = dungeon_portal.obj_cell_id | 0xFFFF;
    fields_mut(w, this).dungeon_landblock_id = Some(v);
    v
}

// ACE: House.DungeonHouseGuid
pub fn dungeon_house_guid(w: &mut World, this: ObjectGuid) -> u32 {
    if let Some(v) = fields(w, this).dungeon_house_guid {
        return v;
    }
    let dungeon_landblock_id = dungeon_landblock_id(w, this);
    if dungeon_landblock_id == 0 {
        return 0;
    }

    let landblock = u16::try_from((dungeon_landblock_id >> 16) & 0xFFFF).expect("16 bits");

    let basement_guid = w.content.get_cached_basement_house_guid(landblock);

    if basement_guid == 0 {
        return 0;
    }

    fields_mut(w, this).dungeon_house_guid = Some(basement_guid);
    basement_guid
}

/// For villas and mansions, the basement dungeons contain their own House weenie
/// This dungeon House needs to reference the main outdoor house for various operations,
/// such as returning the permissions list.
// ACE: House.RootHouse
pub fn root_house(w: &mut World, this: ObjectGuid) -> Option<ObjectGuid> {
    //if (HouseType == ACE.Entity.Enum.HouseType.Apartment || HouseType == ACE.Entity.Enum.HouseType.Cottage)
    //return this;

    let root_guid = root_guid(w, this);
    let landblock_id = landblock_id_of_house_guid(root_guid.full());
    let is_loaded = landblock_manager::is_loaded(w, landblock_id);

    if !is_loaded {
        // do not cache, in case permissions have changed
        return load(w, root_guid.full(), false);
    }

    landblock_house(w, landblock_id, root_guid)
}

/// `RootHouse` where only a `&World` is at hand (the object serialisers): the loaded root, or its
/// offline copy.
///
/// When neither is in the store ACE loads a copy; here the house itself answers (logged).
// DIVERGE: RootHouse from the object serialisers (&World) cannot load an offline copy; an unloaded root with no copy in the store is answered by the house itself.
#[must_use]
pub fn root_house_ref(w: &World, this: ObjectGuid) -> ObjectGuid {
    let root_guid = root_guid_ref(w, this);
    let landblock_id = landblock_id_of_house_guid(root_guid.full());
    if landblock_manager::is_loaded(w, landblock_id) {
        if let Some(h) = landblock_house(w, landblock_id, root_guid) {
            return h;
        }
    } else if is_house(w, root_guid) {
        return root_guid;
    }
    log::warn!("[HOUSE] RootHouse of 0x{this} is not in the store; the house answers for it");
    this
}

// ACE: House.RootGuid
pub fn root_guid(w: &mut World, this: ObjectGuid) -> ObjectGuid {
    if let Some(g) = fields(w, this).root_guid {
        return g;
    }
    let g = root_guid_ref(w, this);
    fields_mut(w, this).root_guid = Some(g);
    g
}

/// `RootGuid` without caching it.
#[must_use]
pub fn root_guid_ref(w: &World, this: ObjectGuid) -> ObjectGuid {
    if let Some(g) = fields(w, this).root_guid {
        return g;
    }
    match empyrean_tables::house_cell::ROOT_GUIDS.get(&this.full()) {
        Some(root_guid) => ObjectGuid::new(*root_guid),
        None => {
            log::error!("House.RootGuid - couldn't find root guid for house guid {this}");
            this
        }
    }
}

// ACE: House.GetAllegianceAccessLevel
#[must_use]
pub fn get_allegiance_access_level(w: &World, this: ObjectGuid) -> Option<bool> {
    let monarch_id = obj(w, this).monarch_id()?;

    fields(w, this)
        .guests
        .get(&ObjectGuid::new(monarch_id))
        .copied()
}

// ACE: House.GetHouse
pub fn get_house(w: &mut World, house_guid: u32) -> Option<ObjectGuid> {
    let landblock_id = landblock_id_of_house_guid(house_guid);
    let is_loaded = landblock_manager::is_loaded(w, landblock_id);

    if !is_loaded {
        return load(w, house_guid, false);
    }

    landblock_house(w, landblock_id, ObjectGuid::new(house_guid))
}

// ACE: House.GetDungeonHouse
pub fn get_dungeon_house(w: &mut World, this: ObjectGuid) -> Option<ObjectGuid> {
    let landblock_id = LandblockId::new(dungeon_landblock_id(w, this));
    let is_loaded = landblock_manager::is_loaded(w, landblock_id);

    if !is_loaded {
        let dungeon_house_guid = dungeon_house_guid(w, this);
        return load(w, dungeon_house_guid, true);
    }

    let wcid = obj(w, this).biota.weenie_class_id;
    let wos = landblock::get_world_objects_for_physics_handling(w, landblock_id);
    wos.into_iter()
        .find(|&wo| {
            w.objects
                .get(wo)
                .is_some_and(|o| o.biota.weenie_class_id == wcid)
        })
        .filter(|&g| is_house(w, g))
}

// ACE: House.OnProperty
pub fn on_property(w: &mut World, this: ObjectGuid, player: ObjectGuid) -> bool {
    let Some(location) = obj(w, this).location() else {
        return false;
    };
    let player_location = obj(w, player)
        .location()
        .expect("System.NullReferenceException: player.Location");

    if obj(w, this).house_type() == HouseType::Apartment {
        return player_location.cell() == location.cell();
    }

    let player_outdoor = get_outdoor_cell(&player_location);
    if player_outdoor == get_outdoor_cell(&location) {
        return true;
    }

    for linked_house in fields(w, this).linked_houses.clone() {
        let linked_location = obj(w, linked_house)
            .location()
            .expect("System.NullReferenceException: linkedHouse.Location");
        if player_outdoor == get_outdoor_cell(&linked_location) {
            return true;
        }
    }

    if has_dungeon(w, this)
        && (player_location.cell() | 0xFFFF) == dungeon_landblock_id(w, this)
        && (player_location.cell() & 0xFFFF) >= 0x100
    {
        return true;
    }
    false
}

// ACE: House.BootAll
pub fn boot_all(
    w: &mut World,
    this: ObjectGuid,
    booter: ObjectGuid,
    guests: bool,
    allegiance_house: bool,
) -> i32 {
    let players = player_manager::get_all_online(w);

    let mut booted = 0;
    for player in players {
        // exclude booter
        if player == booter {
            continue;
        }

        if !on_property(w, this, player) {
            continue;
        }

        // keep guests if closing house
        if !guests && has_permission(w, this, player, false) {
            continue;
        }

        let player_name = name(w, player);
        player_house::handle_action_boot(w, booter, &player_name, allegiance_house);
        booted += 1;
    }
    booted
}

// ================================================================================ restrictions

// ACE: House.UpdateRestrictionDB
pub fn update_restriction_db(w: &mut World, this: ObjectGuid, restrictions: Option<RestrictionDB>) {
    // get restrictions for root house
    let restrictions = restrictions.unwrap_or_else(|| restriction_db_new(w, Some(this)));

    send_restriction_db(w, this, &restrictions);

    // for mansions, update the linked houses
    for linked_house in fields(w, this).linked_houses.clone() {
        send_restriction_db(w, linked_house, &restrictions);
    }

    // update house dungeon

    if has_dungeon(w, this) {
        let dungeon_house = get_dungeon_house(w, this);
        let Some(dungeon_house) = dungeon_house else {
            return;
        };
        if obj(w, dungeon_house).phys.is_none() {
            return;
        }

        send_restriction_db(w, dungeon_house, &restrictions);
    }
}

// ACE: House.SendRestrictionDB
pub fn send_restriction_db(w: &mut World, this: ObjectGuid, restrictions: &RestrictionDB) {
    let Some(h) = obj(w, this).phys else { return };

    let nearby_players = object_maint::get_known_players_values_as_player(w, h);
    for player in nearby_players {
        let session = player_manager::player_session(w, player)
            .expect("System.NullReferenceException: player.Session");
        let m1 = game_message_public_update_instance_id(
            obj_mut(w, this),
            PropertyInstanceId::HouseOwner,
            ObjectGuid::new(restrictions.house_owner),
        );
        let (sessions, objects) = (&mut w.sessions, &mut w.objects);
        let data = sessions.get_mut(session).expect("the player's session");
        let house = objects.get_mut(this).expect("present");
        let m2 = game_event_house_update_restrictions(data, house, restrictions);
        enqueue_send(w, session, m1);
        enqueue_send(w, session, m2);
    }
}

// ACE: House.ClearRestrictions
pub fn clear_restrictions(w: &mut World, this: ObjectGuid) {
    if obj(w, this).phys.is_none() {
        return;
    }

    let restriction_db = RestrictionDB::default();

    update_restriction_db(w, this, Some(restriction_db));
}

// ================================================================================ hooks

/// `HookGroupLimits[houseType][hookGroupType]`.
///
/// # Panics
/// For a pair missing from the table (`KeyNotFoundException`).
// ACE: House.HookGroupLimits
#[must_use]
pub fn hook_group_limit(house_type: HouseType, hook_group_type: HookGroupType) -> i32 {
    // Undef, NoisemakingItems, TestItems, PortalItems, WritableItems, SpellCastingItems, SpellTeachingItems
    let row: [i32; 7] = match house_type {
        HouseType::Undef => [-1, -1, -1, -1, -1, -1, -1],
        HouseType::Cottage => [-1, -1, -1, -1, 1, 5, 0],
        HouseType::Villa => [-1, -1, -1, -1, 1, 10, 0],
        HouseType::Mansion => [-1, -1, -1, -1, 3, 15, 1],
        HouseType::Apartment => [-1, -1, -1, 0, 0, -1, 0],
        _ => panic!(
            "System.Collections.Generic.KeyNotFoundException: HouseType {}",
            house_type.0
        ),
    };
    let order = [
        HookGroupType::Undef,
        HookGroupType::NoisemakingItems,
        HookGroupType::TestItems,
        HookGroupType::PortalItems,
        HookGroupType::WritableItems,
        HookGroupType::SpellCastingItems,
        HookGroupType::SpellTeachingItems,
    ];
    let i = order
        .iter()
        .position(|&g| g == hook_group_type)
        .unwrap_or_else(|| {
            panic!(
                "System.Collections.Generic.KeyNotFoundException: HookGroupType {}",
                hook_group_type.0
            )
        });
    row[i]
}

// ACE: House.GetHookGroupCurrentCount
#[must_use]
pub fn get_hook_group_current_count(
    w: &World,
    this: ObjectGuid,
    hook_group_type: HookGroupType,
) -> i32 {
    let n = hooks(w, this)
        .into_iter()
        .filter(|&h| {
            hook::has_item(w, h)
                && hook::item(w, h)
                    .and_then(|i| w.objects.get(i))
                    .and_then(WorldObject::hook_group)
                    .unwrap_or(HookGroupType::Undef)
                    == hook_group_type
        })
        .count();
    i32::try_from(n).unwrap_or(i32::MAX)
}

// ACE: House.GetHookGroupMaxCount
#[must_use]
pub fn get_hook_group_max_count(
    w: &World,
    this: ObjectGuid,
    hook_group_type: HookGroupType,
) -> i32 {
    hook_group_limit(obj(w, this).house_type(), hook_group_type)
}

/// `HouseCurrentHooksUsable += delta` (the setter removes the property at the maximum).
pub fn add_house_current_hooks_usable(w: &mut World, this: ObjectGuid, delta: i32) {
    let o = obj_mut(w, this);
    let v = o.house_current_hooks_usable().wrapping_add(delta);
    o.set_house_current_hooks_usable(v);
}
