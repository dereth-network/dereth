// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/SlumLord.cs
//! Port of `Source/ACE.Server/WorldObjects/SlumLord.cs`.

use empyrean_entity::enums::{
    DestinationType, HouseBitfield, HouseStatus, MotionCommand, MotionStance, PropertyString,
};
use empyrean_entity::ObjectGuid;

use crate::entity::i_player::{self, IPlayer};
use crate::managers::{guid_manager, house_manager, player_manager, property_manager};
use crate::network::game_event::events::game_event_house_profile::game_event_house_profile;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_public_update_property_string::game_message_public_update_property_string;
use crate::network::motion::movement_data::Motion;
use crate::network::structure::house_profile::HouseProfile;
use crate::world_objects::world_object::{CtorEnv, CtorSource, WorldObject};
use crate::world_objects::{player_house, world_object_equipment, world_object_networking};
use crate::{dispatch, World};

/// Non-property fields declared in `SlumLord.cs`.
#[derive(Debug, Default)]
pub struct SlumLordFields {}

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

/// The house this slumlord is linked to
// ACE: SlumLord.House
#[must_use]
pub fn house(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    let parent = w.objects.get(this)?.wo.world_object_links.parent_link?;
    w.objects
        .get(parent)
        .is_some_and(WorldObject::is_house)
        .then_some(parent)
}

/// `Container.InventoryLoaded`.
#[must_use]
pub fn inventory_loaded(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .and_then(|o| o.container.as_ref())
        .is_some_and(|c| c.container.inventory_loaded)
}

/// `wo.SetStackSize(value)` on an object in the store.
pub fn set_stack_size(w: &mut World, g: ObjectGuid, value: i32) {
    obj_mut(w, g).set_stack_size(Some(value));
}

// ---- constructors and SetEphemeralValues ----

/// `new SlumLord(weenie, guid)` / `new SlumLord(biota)`: the `Container` constructor, then
/// SlumLord's `SetEphemeralValues`.
// ACE: SlumLord.SlumLord
pub fn slum_lord_ctor(o: &mut WorldObject, env: &CtorEnv<'_>, src: CtorSource) {
    crate::world_objects::container::container_ctor(o, env, src);
    slum_lord_set_ephemeral_values(o, env);
}

// ACE: SlumLord.SetEphemeralValues
fn slum_lord_set_ephemeral_values(o: &mut WorldObject, _env: &CtorEnv<'_>) {
    o.set_item_capacity(Some(120));
}

// ================================================================================ SlumLord.cs

// ACE: SlumLord.ActOnUse
pub fn slum_lord_act_on_use(w: &mut World, this: ObjectGuid, activator: ObjectGuid) {
    //Console.WriteLine($"SlumLord.ActOnUse({worldObject.Name})");

    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    // sent house profile
    let house_profile = get_house_profile(w, this);

    let session = player_manager::player_session(w, player)
        .expect("System.NullReferenceException: player.Session");
    let data = w.sessions.get_mut(session).expect("the player's session");
    let m = game_event_house_profile(data, this, &house_profile);
    enqueue_send(w, session, m);
}

/// # Panics
/// When the slumlord has no `HouseId` (`HouseId.Value`).
// ACE: SlumLord.GetHouseProfile
pub fn get_house_profile(w: &mut World, this: ObjectGuid) -> HouseProfile {
    let mut house_profile = HouseProfile {
        dwelling_id: obj(w, this)
            .house_id()
            .expect("System.InvalidOperationException: HouseId.Value"),
        ..HouseProfile::default()
    };

    if let Some(house) = house(w, this) {
        let h = obj(w, house);
        house_profile.r#type = h.house_type();

        if h.house_status() == HouseStatus::Disabled {
            house_profile.bitmask &= !HouseBitfield::Active;
        }

        if h.house_status() == HouseStatus::InActive {
            house_profile.maintenance_free = true;
        }
    }

    let o = obj(w, this);
    if o.house_requires_monarch() {
        house_profile.bitmask |= HouseBitfield::RequiresMonarch;
    }

    if let Some(min_level) = o.min_level() {
        house_profile.min_level = min_level;
    }

    if let Some(allegiance_min_level) = o.allegiance_min_level() {
        house_profile.min_alleg_rank = allegiance_min_level;
    }

    if let Some(owner_id) = o.house_owner() {
        let (owner, _) = player_manager::find_by_guid(w, owner_id);

        house_profile.owner_id = ObjectGuid::new(owner_id);
        house_profile.owner_name = owner.and_then(|p| i_player::name(w, p));
    }

    let buy = get_buy_items(w, this);
    house_profile.set_buy_items(w, &buy);
    let rent = get_rent_items(w, this);
    house_profile.set_rent_items(w, &rent);
    house_profile.set_paid_items(w, this);

    house_profile
}

/// `item.Destroy(false)` on a create-list object that never entered the world: it is marked
/// destroyed, its biota was never saved (so `RemoveBiotaFromDatabase` only marks it changed), and
/// its dynamic guid goes back to `GuidManager`. The object stays readable, as ACE's list does.
fn destroy_detached(w: &mut World, o: &mut WorldObject) {
    if o.wo.world_object.is_destroyed {
        return;
    }
    o.wo.world_object.is_destroyed = true;
    o.set_released_timestamp(Some(w.now.unix_time));
    // no inventory, no equipment, not a pet, vendor or generator, on no landblock
    o.wo.world_object_database.changes_detected = true;
    if o.guid.is_dynamic() {
        guid_manager::recycle_dynamic_guid(w, o.guid);
    }
}

/// The create-list objects of `type`, each `Destroy(false)`ed as ACE's list is (see
/// [`destroy_detached`]). A `null` entry of the list is skipped here and throws in ACE's
/// `ForEach` (its `NullReferenceException`), so it panics.
fn get_create_list_destroyed(
    w: &mut World,
    this: ObjectGuid,
    r#type: DestinationType,
) -> Vec<WorldObject> {
    let list = world_object_equipment::get_create_list_for_slum_lord(w, this, r#type);

    let mut out = Vec::new();
    for item in list {
        let mut item =
            item.expect("System.NullReferenceException: item.Destroy on a null create-list object");
        destroy_detached(w, &mut item);
        out.push(item);
    }
    out
}

/// Returns the list of items required to purchase this dwelling
// ACE: SlumLord.GetBuyItems
pub fn get_buy_items(w: &mut World, this: ObjectGuid) -> Vec<WorldObject> {
    get_create_list_destroyed(w, this, DestinationType::HouseBuy)
}

/// Returns the list of items required to rent this dwelling
// ACE: SlumLord.GetRentItems
pub fn get_rent_items(w: &mut World, this: ObjectGuid) -> Vec<WorldObject> {
    get_create_list_destroyed(w, this, DestinationType::HouseRent)
}

/// Returns TRUE if rent is already paid for current maintenance period
// ACE: SlumLord.IsRentPaid
pub fn is_rent_paid(w: &mut World, this: ObjectGuid) -> bool {
    if house(w, this).is_some_and(|h| obj(w, h).house_status() == HouseStatus::InActive) {
        return true;
    }

    let house_profile = get_house_profile(w, this);

    for rent_item in &house_profile.rent {
        if rent_item.paid < rent_item.num {
            return false;
        }
    }
    true
}

/// `IsRentPaid()` where only a `&World` is at hand (appraisal): the same answer, with the rent
/// items' amounts read from the create list and their weenies.
///
/// ACE builds (and destroys) the create-list objects, drawing and recycling dynamic guids; this
/// reads the same numbers without them.
// DIVERGE: IsRentPaid for appraisal (&World) reads the rent amounts from the create list without building and destroying the objects (no dynamic guids drawn).
#[must_use]
pub fn is_rent_paid_ref(w: &World, this: ObjectGuid) -> bool {
    if house(w, this).is_some_and(|h| obj(w, h).house_status() == HouseStatus::InActive) {
        return true;
    }

    let create_list = obj(w, this)
        .biota
        .properties_create_list
        .clone()
        .expect("System.NullReferenceException: Biota.PropertiesCreateList");
    let mut rent: Vec<crate::network::structure::house_payment::HousePayment> = create_list
        .iter()
        .filter(|x| x.destination_type == DestinationType::HouseRent)
        .map(|item| {
            let weenie = w
                .content
                .get_cached_weenie(item.weenie_class_id)
                .expect("System.NullReferenceException: create-list weenie");
            let num = if item.stack_size > 0 {
                item.stack_size
            } else {
                weenie
                    .get_property(empyrean_entity::enums::PropertyInt::StackSize)
                    .unwrap_or(1)
            };
            crate::network::structure::house_payment::HousePayment {
                num,
                weenie_id: item.weenie_class_id,
                ..Default::default()
            }
        })
        .collect();
    crate::network::structure::house_data::set_paid_items(w, this, &mut [], &mut rent);

    rent.iter().all(|r| r.paid >= r.num)
}

/// Returns TRUE if this player has the minimum requirements to purchase / rent this house
// ACE: SlumLord.HasRequirements
pub fn has_requirements(w: &mut World, this: ObjectGuid, player: ObjectGuid) -> bool {
    if !property_manager::get_bool(w, "house_purchase_requirements", false, true).item {
        return true;
    }

    let Some(slumlord_min) = obj(w, this).allegiance_min_level() else {
        return true;
    };

    let mut allegiance_min_level = property_manager::get_long(w, "mansion_min_rank", -1, true).item;
    if allegiance_min_level == -1 {
        allegiance_min_level = i64::from(slumlord_min);
    }

    let allegiance = player_house::allegiance::player_allegiance(w, player);
    let rank = player_house::allegiance::player_allegiance_node_rank(w, player);
    // `player.AllegianceNode.Rank` is read unguarded when `player.Allegiance` is set (AllegianceManager sets both).
    if allegiance_min_level > 0
        && (allegiance.is_none()
            || i64::from(rank.expect("System.NullReferenceException: player.AllegianceNode"))
                < allegiance_min_level)
    {
        // `Console.WriteLine`
        log::info!(
            "{}.HasRequirements({}) - allegiance rank {} < {}",
            dispatch::name::name(w, this).unwrap_or_default(),
            dispatch::name::name(w, player).unwrap_or_default(),
            rank.unwrap_or(0),
            allegiance_min_level
        );
        return false;
    }
    true
}

// ACE: SlumLord.GetAllegianceMinLevel
#[must_use]
pub fn get_allegiance_min_level(w: &World, this: ObjectGuid) -> i32 {
    let Some(slumlord_min) = obj(w, this).allegiance_min_level() else {
        return 0;
    };

    let mut allegiance_min_level = property_manager::get_long(w, "mansion_min_rank", -1, true).item;
    if allegiance_min_level == -1 {
        allegiance_min_level = i64::from(slumlord_min);
    }

    // `(int)allegianceMinLevel`: an unchecked long to int conversion
    #[allow(clippy::cast_possible_truncation)]
    let v = allegiance_min_level as i32;
    v
}

// ACE: SlumLord.OnInitialInventoryLoadCompleted
pub fn slum_lord_on_initial_inventory_load_completed(w: &mut World, this: ObjectGuid) {
    house_manager::on_initial_inventory_load_completed(w, this);
}

// ACE: SlumLord.On
pub fn on(w: &mut World, this: ObjectGuid) {
    let on = Motion::new(MotionStance::Invalid, MotionCommand::On, 1.0);

    set_and_broadcast_motion(w, this, on);
}

// ACE: SlumLord.Off
pub fn off(w: &mut World, this: ObjectGuid) {
    let off = Motion::new(MotionStance::Invalid, MotionCommand::Off, 1.0);

    if obj(w, this).current_landblock.is_some() {
        set_and_broadcast_motion(w, this, off);
    }
}

// ACE: SlumLord.SetAndBroadcastMotion
fn set_and_broadcast_motion(w: &mut World, this: ObjectGuid, motion: Motion) {
    world_object_networking::shims::set_current_motion_state(w, this, Some(motion.clone()));
    world_object_networking::enqueue_broadcast_motion(w, this, &motion, None, None);
}

// ACE: SlumLord.SetAndBroadcastName
pub fn set_and_broadcast_name(w: &mut World, this: ObjectGuid, house_owner_name: Option<&str>) {
    let new_name = match house_owner_name.filter(|n| !n.trim().is_empty()) {
        None => {
            let wcid = obj(w, this).biota.weenie_class_id;
            match w.content.get_cached_weenie(wcid) {
                Some(weenie) => weenie.get_property(PropertyString::Name),
                None => {
                    let h = house(w, this).expect("System.NullReferenceException: House");
                    Some(obj(w, h).house_type().to_dotnet_string())
                }
            }
        }
        Some(owner) => Some(format!(
            "{owner}'s {}",
            dispatch::name::name(w, this).unwrap_or_default()
        )),
    };
    match new_name {
        Some(n) => dispatch::name::set_name(w, this, n),
        // `Name = null` removes the property
        None => obj_mut(w, this).remove_property(PropertyString::Name),
    }

    if obj(w, this).current_landblock.is_some() {
        let name = dispatch::name::name(w, this);
        let m = game_message_public_update_property_string(
            obj_mut(w, this),
            PropertyString::Name,
            name.as_deref(),
        );
        world_object_networking::enqueue_broadcast(w, this, true, &[m]);
    }
}

/// This event is raised when HouseManager removes item for rent
// ACE: SlumLord.OnRemoveItem
pub fn slum_lord_on_remove_item(w: &mut World, _this: ObjectGuid, removed_item: ObjectGuid) {
    //Console.WriteLine("Slumlord.OnRemoveItem()");

    // Here we explicitly remove the payment from the database to avoid storing unneeded objects and free guid.
    if w.objects
        .get(removed_item)
        .is_some_and(|o| !o.wo.world_object.is_destroyed)
    {
        crate::world_objects::world_object::destroy(w, removed_item, true, false);
    }
}

/// `IPlayer` of an online player.
#[must_use]
pub fn online(player: ObjectGuid) -> IPlayer {
    IPlayer::Online(player)
}
