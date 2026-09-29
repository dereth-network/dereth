// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Container.cs
//! Port of `Source/ACE.Server/WorldObjects/Container.cs`.
//!
//! # Inventory
//!
//! ACE's `Dictionary<ObjectGuid, WorldObject> Inventory` is [`ContainerFields::inventory`], a
//! `DotNetDict` of guids, so "first match" and every `Inventory.Values` walk see .NET's
//! enumeration order (insertion order, with a removed slot reused by the next add). Every item in
//! it is a live object in `World.objects`; an entry whose object has left the store is an
//! invariant violation and panics.
//!
//! Members take `(w, this, ..)` with `this` the container. Items are guids of
//! objects already in `World.objects`; a caller that creates an object inserts it first.
//!
//! # Shims
//!
//! Members of other ACE files that the container code calls, each a private
//! function here marked `SHIM:` (no anchor):
//! `Player.SendTransientError`,
//! `WorldObject.WeenieClassName` (`WorldObject_Properties.cs`), `PropertyManager.GetBool`
//! and .NET's `List<T>.Sort(Comparison<T>)` (`empyrean_common::dotnet::sort::list_sort`).

use empyrean_common::dotnet::sort::{compare_to, list_sort};
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{
    ChatMessageType, DestinationType, Placement, WeenieClassName, WeenieType,
};
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_action::Action;
use crate::entity::actions::i_actor::Actor;
use crate::managers::player_manager::player_session;
use crate::network::game_event::events::game_event_close_ground_container::game_event_close_ground_container;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_view_contents::game_event_view_contents;
use crate::network::game_messages::game_message::{enqueue_send, enqueue_send_many, GameMessage};
use crate::network::game_messages::messages::game_message_create_object::game_message_create_object;
use crate::network::game_messages::messages::game_message_delete_object::game_message_delete_object;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::world_object::{CtorEnv, WorldObject};
use crate::World;

/// Non-property fields declared in `Container.cs`.
#[derive(Debug, Default)]
pub struct ContainerFields {
    /// Set once the container's contents have been placed (`private set` in ACE).
    // ACE: Container.InventoryLoaded
    pub inventory_loaded: bool,
    /// All main pack items and all side slot items (packs and foci), in .NET enumeration order.
    /// The items inside a side pack are in that pack's own `Inventory`. Do not manipulate it
    /// directly: use `try_add_to_inventory` / `try_remove_from_inventory`.
    // ACE: Container.Inventory
    pub inventory: DotNetDict<ObjectGuid, ()>,
    /// Not ACE: set by the `Container(Biota)` constructor when the container must load its own
    /// inventory from the shard; [`post_insert_load_inventory`] runs that load once the object is
    /// in `World.objects`.
    pub inventory_load_pending: bool,
    /// Not ACE: set by `Container.SetEphemeralValues(false)` (a container built from its weenie)
    /// and by the Olthoi player's constructor, whose `GenerateContainList` needs the world;
    /// [`post_insert_generate_contain_list`] runs it once the object is in `World.objects`.
    pub generate_contain_list_pending: bool,
}

// ================================================================================ helpers

/// The container `this` (ACE's `this`): panics like the C# `NullReferenceException` when the
/// object is gone.
fn object(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects.get(this).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            this.full()
        )
    })
}

fn object_mut(w: &mut World, this: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(this).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            this.full()
        )
    })
}

/// `Inventory` of a container object.
///
/// # Panics
/// When `o` is not a `Container` (the C# cast would have failed first).
#[must_use]
pub fn inventory(o: &WorldObject) -> &DotNetDict<ObjectGuid, ()> {
    &o.container
        .as_ref()
        .expect("InvalidCastException: not a Container")
        .container
        .inventory
}

fn inventory_mut(o: &mut WorldObject) -> &mut DotNetDict<ObjectGuid, ()> {
    &mut o
        .container
        .as_mut()
        .expect("InvalidCastException: not a Container")
        .container
        .inventory
}

/// `Inventory.Values.ToList()`: a snapshot of the guids, in enumeration order.
#[must_use]
pub fn inventory_values(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    inventory(object(w, this)).keys().copied().collect()
}

/// `list.Count` as the C# `int`.
fn count_i32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// `x += y` on an `int?` property: a null stays null (lifted `+`).
fn add_nullable(current: Option<i32>, amount: i32) -> Option<i32> {
    current.map(|c| c.wrapping_add(amount))
}

/// `EncumbranceVal += encumbrance; Value += value;` on `this`.
fn add_burden_and_value(w: &mut World, this: ObjectGuid, encumbrance: i32, value: i32) {
    let o = object_mut(w, this);
    let e = add_nullable(o.encumbrance_val(), encumbrance);
    o.set_encumbrance_val(e);
    let v = add_nullable(o.value(), value);
    o.set_value(v);
}

/// `EncumbranceVal -= encumbrance; Value -= value;` on `this`.
fn subtract_burden_and_value(w: &mut World, this: ObjectGuid, encumbrance: i32, value: i32) {
    let o = object_mut(w, this);
    let e = o.encumbrance_val().map(|c| c.wrapping_sub(encumbrance));
    o.set_encumbrance_val(e);
    let v = o.value().map(|c| c.wrapping_sub(value));
    o.set_value(v);
}

/// `(worldObject.EncumbranceVal ?? 0, worldObject.Value ?? 0)`.
fn burden_and_value(w: &World, item: ObjectGuid) -> (i32, i32) {
    let o = object(w, item);
    (o.encumbrance_val().unwrap_or(0), o.value().unwrap_or(0))
}

impl WorldObject {
    /// Containers and objects that need a pack slot (foci) go in the side slots.
    /// Declared in `WorldObject_Properties.cs`; ported here with its only callers.
    // ACE: WorldObject.UseBackpackSlot
    #[must_use]
    pub fn use_backpack_slot(&self) -> bool {
        self.biota.weenie_type == WeenieType::Container || self.requires_pack_slot()
    }
}

/// SHIM: `WorldObject.WeenieClassName` (get): the cached weenie's class name, or
/// `"WeenieClassName_NOT_FOUND"` (with ACE's warning) when the weenie is gone.
fn weenie_class_name(w: &World, item: ObjectGuid) -> String {
    let weenie_class_id = object(w, item).biota.weenie_class_id;
    if let Some(weenie) = w.content.get_cached_weenie(weenie_class_id) {
        // A weenie without a class name reads as empty (ACE would carry the null on).
        weenie.class_name.clone().unwrap_or_default()
    } else {
        log::warn!(
            "WorldObject.WeenieClassName -- No cached weenie found for WCID {weenie_class_id}"
        );
        "WeenieClassName_NOT_FOUND".to_owned()
    }
}

/// `string.Equals(a, b, StringComparison.OrdinalIgnoreCase)`: equal after simple upper-casing of
/// each char (a char whose upper case is several chars compares as itself, as .NET's simple case
/// mapping does).
fn equals_ordinal_ignore_case(a: &str, b: &str) -> bool {
    fn upper(c: char) -> char {
        let mut u = c.to_uppercase();
        match (u.next(), u.next()) {
            (Some(one), None) => one,
            _ => c,
        }
    }
    a.chars().count() == b.chars().count()
        && a.chars()
            .zip(b.chars())
            .all(|(x, y)| x == y || upper(x) == upper(y))
}

/// `PropertyManager.GetBool(key).Item`.
fn property_manager_get_bool(w: &World, key: &str) -> bool {
    crate::managers::property_manager::get_bool(w, key, false, true).item
}

/// `Player.GetEncumbranceCapacity()` (the name the container code uses).
pub use crate::world_objects::player_inventory::get_encumbrance_capacity as player_get_encumbrance_capacity;
/// `Player.HasEnoughBurdenToAddToInventory(WorldObject worldObject)`.
use crate::world_objects::player_inventory::has_enough_burden_to_add_to_inventory as player_has_enough_burden_to_add_to_inventory;
/// `Player.HasEnoughBurdenToAddToInventory(List<WorldObject> worldObjects)`.
use crate::world_objects::player_inventory::has_enough_burden_to_add_to_inventory_list as player_has_enough_burden_to_add_to_inventory_list;
/// `Player.HasEnoughBurdenToAddToInventory(int totalEncumbranceToCheck)`.
pub(crate) use crate::world_objects::player_inventory::has_enough_burden_to_add_to_inventory_total as player_has_enough_burden_to_add_to_inventory_total;

/// `Player.LastOpenedContainerId` (get), the `Player_Use.cs` field.
fn player_last_opened_container_id(w: &World, player: ObjectGuid) -> ObjectGuid {
    crate::world_objects::player_inventory::last_opened_container_id(w, player)
}

/// `Player.LastOpenedContainerId` (set).
fn player_set_last_opened_container_id(w: &mut World, player: ObjectGuid, value: ObjectGuid) {
    if let Some(p) = w.objects.get_mut(player).and_then(|o| o.player.as_mut()) {
        p.player_use.last_opened_container_id = value;
    }
}

/// SHIM: `Player.SendTransientError(msg)`:
/// `Session.Network.EnqueueSend(new GameEventCommunicationTransientString(Session, msg))`.
fn player_send_transient_error(w: &mut World, player: ObjectGuid, msg: &str) {
    let Some(session) = player_session(w, player) else {
        return;
    };
    let Some(data) = w.sessions.get_mut(session) else {
        return;
    };
    let message = game_event_communication_transient_string(data, msg);
    enqueue_send(w, session, message);
}

// ================================================================================ Container.cs

/// The only time this should be used is to populate Inventory from the ctor. Builds each biota's
/// object, adds it to `World.objects` and sorts it in; objects nothing claims are dropped again.
///
/// # Panics
/// Where ACE throws: a biota whose weenie type has no class (`CreateWorldObject` returns null and
/// the sort dereferences it), or a biota whose guid is already live.
// ACE: Container.SortBiotasIntoInventory
// ACE-BUG: a biota with an undefined WeenieType makes CreateWorldObject return null, and SortWorldObjectsIntoInventory then throws NullReferenceException on it.
pub fn sort_biotas_into_inventory(
    w: &mut World,
    this: ObjectGuid,
    biotas: Vec<empyrean_entity::Biota>,
) {
    let mut world_objects = Vec::new();

    for biota in biotas {
        let world_object = CtorEnv::with_world(w, |env| {
            crate::factories::world_object_factory::create_world_object_from_biota(env, biota)
        })
        .expect("System.NullReferenceException: CreateWorldObject(biota) returned null");
        let guid = world_object.guid;
        assert!(
            w.objects.insert(world_object).is_ok(),
            "biota 0x{:08X} is already a live object",
            guid.full()
        );
        // a side pack's own `Container(Biota)` load (its constructor's, in ACE)
        post_insert_load_inventory(w, guid);
        world_objects.push(guid);
    }

    sort_world_objects_into_inventory(w, this, &mut world_objects);

    if !world_objects.is_empty() {
        log::error!("Inventory detected without a container to put it in to.");
    }

    // Not ACE: the unclaimed objects are unreachable (C# collects them), so they leave the store.
    for guid in world_objects {
        w.objects.remove(guid);
    }
}

/// The only time this should be used is to populate Inventory from the ctor. This will remove
/// from `world_objects` as they're sorted. The objects must already be in `World.objects`.
// ACE: Container.SortWorldObjectsIntoInventory
pub fn sort_world_objects_into_inventory(
    w: &mut World,
    this: ObjectGuid,
    world_objects: &mut Vec<ObjectGuid>,
) {
    let biota_id = object(w, this).biota.id;

    // This will pull out all of our main pack items and side slot items (foci & containers)
    for i in (0..world_objects.len()).rev() {
        let item = world_objects[i];
        let o = object(w, item);
        if o.container_id().unwrap_or(0) == biota_id {
            let is_side_container = o.biota.weenie_type == WeenieType::Container;
            let (encumbrance, value) = (o.encumbrance_val().unwrap_or(0), o.value().unwrap_or(0));

            inventory_mut(object_mut(w, this)).insert(item, ());
            object_mut(w, item).wo.world_object_properties.container = Some(this);

            if !is_side_container {
                // We skip over containers because we'll add their burden/value in the next loop.
                add_burden_and_value(w, this, encumbrance, value);
            }

            world_objects.remove(i);
        }
    }

    // Make sure placement positions are correct. They could get out of sync from a client issue, server issue, or orphaned biota
    for side in [false, true] {
        let mut items: Vec<ObjectGuid> = inventory_values(w, this)
            .into_iter()
            .filter(|&g| object(w, g).use_backpack_slot() == side)
            .collect();
        items.sort_by_key(|&g| object(w, g).placement_position());
        for (i, item) in items.into_iter().enumerate() {
            object_mut(w, item).set_placement_position(Some(count_i32(i)));
        }
    }

    object_mut(w, this)
        .container
        .as_mut()
        .expect("a Container")
        .container
        .inventory_loaded = true;

    // All that should be left are side pack sub contents.

    let side_containers: Vec<ObjectGuid> = inventory_values(w, this)
        .into_iter()
        .filter(|&g| object(w, g).biota.weenie_type == WeenieType::Container)
        .collect();
    for container in side_containers {
        sort_world_objects_into_inventory(w, container, world_objects); // This will set the InventoryLoaded flag for this sideContainer
        let (encumbrance, value) = {
            let c = object(w, container);
            (c.encumbrance_val(), c.value())
        };
        // `EncumbranceVal += container.EncumbranceVal;` both `int?`: a null on either side gives null.
        let o = object_mut(w, this);
        let e = o
            .encumbrance_val()
            .zip(encumbrance)
            .map(|(a, b)| a.wrapping_add(b)); // This value includes the containers burden itself + all child items
        o.set_encumbrance_val(e);
        let v = o.value().zip(value).map(|(a, b)| a.wrapping_add(b)); // This value includes the containers value itself + all child items
        o.set_value(v);
    }

    dispatch::on_initial_inventory_load_completed::on_initial_inventory_load_completed(w, this);
}

/// Starts the inventory load the `Container(Biota)` constructor marked, once: called by the
/// insertion hook for every newly inserted object (anything else is left alone).
pub fn post_insert_load_inventory(w: &mut World, this: ObjectGuid) {
    let Some(c) = w.objects.get_mut(this).and_then(|o| o.container.as_mut()) else {
        return;
    };
    if !c.container.inventory_load_pending {
        return;
    }
    c.container.inventory_load_pending = false;
    container_ctor_load_inventory(w, this);
}

/// Runs the `GenerateContainList` the constructor marked, once: called by the insertion hook for
/// every newly inserted object (anything else is left alone).
pub fn post_insert_generate_contain_list(w: &mut World, this: ObjectGuid) {
    let Some(c) = w.objects.get_mut(this).and_then(|o| o.container.as_mut()) else {
        return;
    };
    if !c.container.generate_contain_list_pending {
        return;
    }
    c.container.generate_contain_list_pending = false;
    generate_contain_list(w, this);
}

/// `Container(Biota)`'s inventory load, the part of the constructor that needs the world: "A
/// player has their possessions passed via the ctor. All other world objects must load their own
/// inventory". Run it once the object is in `World.objects` (see `container_ctor`).
pub fn container_ctor_load_inventory(w: &mut World, this: ObjectGuid) {
    let biota_id = object(w, this).biota.id;
    w.shard.get_inventory_in_parallel(
        biota_id,
        false,
        Some(Box::new(move |w: &mut World, biotas: Vec<empyrean_store::models::shard::Biota>| {
            let biotas: Vec<empyrean_entity::Biota> = biotas
                .iter()
                .map(|b| empyrean_store::adapter::biota_converter::BiotaConverter::convert_to_entity_biota(b, false))
                .collect();
            dispatch::enqueue_action::enqueue_action(w, this, Action::delegate(move |w: &mut World| sort_biotas_into_inventory(w, this, biotas)));
        })),
    );
}

/// Counts the number of actual inventory items, ignoring Packs/Foci.
// ACE: Container.CountPackItems
fn count_pack_items(w: &World, this: ObjectGuid) -> i32 {
    count_i32(
        inventory(object(w, this))
            .keys()
            .filter(|&&g| !object(w, g).use_backpack_slot())
            .count(),
    )
}

/// Counts the number of containers in inventory, including Foci.
// ACE: Container.CountContainers
fn count_containers(w: &World, this: ObjectGuid) -> i32 {
    count_i32(
        inventory(object(w, this))
            .keys()
            .filter(|&&g| object(w, g).use_backpack_slot())
            .count(),
    )
}

/// ACE's default: `include_side_packs = true`.
// ACE: Container.GetFreeInventorySlots
#[must_use]
pub fn get_free_inventory_slots(w: &World, this: ObjectGuid, include_side_packs: bool) -> i32 {
    // Retail's reading (V247): the capacity byte is signed, so 128..=254 leaves no
    // room and 255 is unlimited, counted here as `i32::MAX` free slots.
    let free = |g: ObjectGuid| {
        dereth_rules::capacity::free_slots(
            object(w, g).item_capacity().unwrap_or(0),
            count_pack_items(w, g),
        )
        .unwrap_or(i32::MAX)
    };
    let mut free_slots = free(this);

    if include_side_packs {
        for side_pack in inventory_values(w, this)
            .into_iter()
            .filter(|&g| object(w, g).is_container())
        {
            free_slots = free_slots.saturating_add(free(side_pack));
        }
    }

    free_slots
}

// ACE: Container.GetFreeContainerSlots
#[must_use]
pub fn get_free_container_slots(w: &World, this: ObjectGuid) -> i32 {
    // Retail's signed capacity byte (V247); an unlimited one counts as `i32::MAX` free.
    dereth_rules::capacity::free_slots(
        object(w, this).container_capacity().unwrap_or(0),
        count_containers(w, this),
    )
    .unwrap_or(i32::MAX)
}

/// This method will check all containers in our possession in main inventory or any side packs.
// ACE: Container.HasInventoryItem
#[must_use]
pub fn has_inventory_item(w: &World, this: ObjectGuid, object_guid: ObjectGuid) -> bool {
    get_inventory_item(w, this, object_guid).is_some()
}

/// This method will check all containers in our possession in main inventory or any side packs.
/// (The `GetInventoryItem(uint)` overload is `ObjectGuid::new(guid)` here.)
// ACE: Container.GetInventoryItem
#[must_use]
pub fn get_inventory_item(
    w: &World,
    this: ObjectGuid,
    object_guid: ObjectGuid,
) -> Option<ObjectGuid> {
    get_inventory_item_with_container(w, this, object_guid).map(|(item, _)| item)
}

/// `GetInventoryItem(objectGuid, out Container container)`: the item and the container holding
/// it (`this` or one of its side packs).
// ACE: Container.GetInventoryItem
#[must_use]
pub fn get_inventory_item_with_container(
    w: &World,
    this: ObjectGuid,
    object_guid: ObjectGuid,
) -> Option<(ObjectGuid, ObjectGuid)> {
    // First search my main pack for this item..
    if inventory(object(w, this)).contains_key(&object_guid) {
        return Some((object_guid, this));
    }

    // Next search all containers for item.. run function again for each container.
    let side_containers: Vec<ObjectGuid> = inventory_values(w, this)
        .into_iter()
        .filter(|&g| object(w, g).biota.weenie_type == WeenieType::Container)
        .collect();
    for side_container in side_containers {
        let container_item = get_inventory_item(w, side_container, object_guid);

        if let Some(container_item) = container_item {
            return Some((container_item, side_container));
        }
    }

    None
}

/// The local items matching `pred`, `OrderBy(PlacementPosition)` (stable; a null position sorts
/// first), then each side container's matches (`get_side`), side containers also in placement
/// order.
fn items_then_side_containers(
    w: &World,
    this: ObjectGuid,
    pred: &dyn Fn(ObjectGuid) -> bool,
    get_side: &dyn Fn(&World, ObjectGuid) -> Vec<ObjectGuid>,
) -> Vec<ObjectGuid> {
    let mut items = Vec::new();

    let mut local_inventory: Vec<ObjectGuid> = inventory_values(w, this)
        .into_iter()
        .filter(|&g| pred(g))
        .collect();
    local_inventory.sort_by_key(|&g| object(w, g).placement_position());

    items.extend(local_inventory);

    let mut side_containers: Vec<ObjectGuid> = inventory_values(w, this)
        .into_iter()
        .filter(|&g| object(w, g).biota.weenie_type == WeenieType::Container)
        .collect();
    side_containers.sort_by_key(|&g| object(w, g).placement_position());
    for container in side_containers {
        items.extend(get_side(w, container));
    }

    items
}

/// This method is used to get all inventory items of a type in this container (example of usage
/// get all items of coin on player).
// ACE: Container.GetInventoryItemsOfTypeWeenieType
#[must_use]
pub fn get_inventory_items_of_type_weenie_type(
    w: &World,
    this: ObjectGuid,
    r#type: WeenieType,
) -> Vec<ObjectGuid> {
    items_then_side_containers(
        w,
        this,
        &|g| object(w, g).biota.weenie_type == r#type,
        &|w, c| get_inventory_items_of_type_weenie_type(w, c, r#type),
    )
}

/// Returns the inventory items matching a weenie class id.
// ACE: Container.GetInventoryItemsOfWCID
#[must_use]
pub fn get_inventory_items_of_wcid(
    w: &World,
    this: ObjectGuid,
    weenie_class_id: u32,
) -> Vec<ObjectGuid> {
    items_then_side_containers(
        w,
        this,
        &|g| object(w, g).biota.weenie_class_id == weenie_class_id,
        &|w, c| get_inventory_items_of_wcid(w, c, weenie_class_id),
    )
}

/// Returns the total # of inventory items matching a wcid.
// ACE: Container.GetNumInventoryItemsOfWCID
#[must_use]
pub fn get_num_inventory_items_of_wcid(w: &World, this: ObjectGuid, weenie_class_id: u32) -> i32 {
    get_inventory_items_of_wcid(w, this, weenie_class_id)
        .into_iter()
        .fold(0i32, |sum, i| {
            sum.wrapping_add(object(w, i).stack_size().unwrap_or(1))
        })
}

/// Returns the inventory items matching a weenie class name.
// ACE: Container.GetInventoryItemsOfWeenieClass
#[must_use]
pub fn get_inventory_items_of_weenie_class(
    w: &World,
    this: ObjectGuid,
    weenie_class_name: &str,
) -> Vec<ObjectGuid> {
    items_then_side_containers(
        w,
        this,
        &|g| equals_ordinal_ignore_case(&weenie_class_name_of(w, g), weenie_class_name),
        &|w, c| get_inventory_items_of_weenie_class(w, c, weenie_class_name),
    )
}

fn weenie_class_name_of(w: &World, item: ObjectGuid) -> String {
    weenie_class_name(w, item)
}

/// Returns the total # of inventory items matching a weenie class name.
// ACE: Container.GetNumInventoryItemsOfWeenieClass
#[must_use]
pub fn get_num_inventory_items_of_weenie_class(
    w: &World,
    this: ObjectGuid,
    weenie_class_name: &str,
) -> i32 {
    get_inventory_items_of_weenie_class(w, this, weenie_class_name)
        .into_iter()
        .fold(0i32, |sum, i| {
            sum.wrapping_add(object(w, i).stack_size().unwrap_or(1))
        })
}

/// Returns all of the trade notes from inventory + side packs.
// ACE: Container.GetTradeNotes
#[must_use]
pub fn get_trade_notes(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    // FIXME: search by classname performance
    // (`StartsWith(string)` is culture-sensitive in .NET; for the ASCII class names it is a prefix test.)
    items_then_side_containers(
        w,
        this,
        &|g| weenie_class_name(w, g).starts_with("tradenote"),
        &get_trade_notes,
    )
}

/// If enough burden is available, this will try to add an item to the main pack. If the main pack
/// is full, it will try to add it to the first side pack with room. It will also increase the
/// EncumbranceVal and Value. ACE's defaults: `placement_position = 0`,
/// `limit_to_main_pack_only = false`, `burden_check = true`. (ACE's `null` check is the caller's:
/// a guid always names an object.)
// ACE: Container.TryAddToInventory
pub fn try_add_to_inventory(
    w: &mut World,
    this: ObjectGuid,
    world_object: ObjectGuid,
    placement_position: i32,
    limit_to_main_pack_only: bool,
    burden_check: bool,
) -> bool {
    try_add_to_inventory_with_container(
        w,
        this,
        world_object,
        placement_position,
        limit_to_main_pack_only,
        burden_check,
    )
    .is_some()
}

/// Returns TRUE if there are enough free inventory slots and burden available to add items.
// ACE: Container.CanAddToInventory
#[must_use]
pub fn can_add_to_inventory_counts(
    w: &World,
    this: ObjectGuid,
    total_container_objects_to_add: i32,
    total_inventory_objects_to_add: i32,
    total_burden_to_add: i32,
) -> bool {
    let o = object(w, this);
    if o.is_player()
        && !player_has_enough_burden_to_add_to_inventory_total(w, o, total_burden_to_add)
    {
        return false;
    }

    (get_free_container_slots(w, this) >= total_container_objects_to_add)
        && (get_free_inventory_slots(w, this, true) >= total_inventory_objects_to_add)
}

/// Returns TRUE if there are enough free inventory slots and burden available to add item.
// ACE: Container.CanAddToInventory
#[must_use]
pub fn can_add_to_inventory(w: &World, this: ObjectGuid, world_object: ObjectGuid) -> bool {
    if object(w, this).is_player()
        && !player_has_enough_burden_to_add_to_inventory(w, this, world_object)
    {
        return false;
    }

    if object(w, world_object).use_backpack_slot() {
        get_free_container_slots(w, this) > 0
    } else {
        get_free_inventory_slots(w, this, true) > 0
    }
}

/// Returns TRUE if there are enough free inventory slots and burden available to add all items.
// ACE: Container.CanAddToInventory
#[must_use]
pub fn can_add_to_inventory_list(
    w: &World,
    this: ObjectGuid,
    world_objects: &[ObjectGuid],
) -> bool {
    can_add_to_inventory_list_with_reasons(w, this, world_objects).0
}

/// `CanAddToInventory(worldObjects, out TooEncumbered, out NotEnoughFreeSlots)`:
/// `(result, too_encumbered, not_enough_free_slots)`.
// ACE: Container.CanAddToInventory
#[must_use]
pub fn can_add_to_inventory_list_with_reasons(
    w: &World,
    this: ObjectGuid,
    world_objects: &[ObjectGuid],
) -> (bool, bool, bool) {
    if world_objects.is_empty() {
        // There are no objects to add (e.g. 1 way trade)
        return (true, false, false);
    }

    if object(w, this).is_player()
        && !player_has_enough_burden_to_add_to_inventory_list(w, this, world_objects)
    {
        return (false, true, false);
    }

    let containers = count_i32(
        world_objects
            .iter()
            .filter(|&&g| object(w, g).use_backpack_slot())
            .count(),
    );
    if containers > 0 && get_free_container_slots(w, this) < containers {
        return (false, false, true);
    }

    let count = count_i32(world_objects.len());
    if get_free_inventory_slots(w, this, true) < (count - containers) {
        return (false, false, true);
    }

    (true, false, false)
}

/// Returns TRUE if there are enough free inventory slots and burden available to add item.
/// ACE's default: `include_side_packs = true`.
// ACE: Container.CanAddToContainer
#[must_use]
pub fn can_add_to_container(
    w: &World,
    this: ObjectGuid,
    world_object: ObjectGuid,
    include_side_packs: bool,
) -> bool {
    if object(w, this).is_player()
        && !player_has_enough_burden_to_add_to_inventory(w, this, world_object)
    {
        return false;
    }

    if object(w, world_object).use_backpack_slot() {
        get_free_container_slots(w, this) > 0
    } else {
        get_free_inventory_slots(w, this, include_side_packs) > 0
    }
}

/// Returns TRUE if there are enough free burden available to merge item and merge target will not
/// exceed maximum stack size.
// ACE: Container.CanMergeToInventory
#[must_use]
pub fn can_merge_to_inventory(
    w: &World,
    this: ObjectGuid,
    world_object: ObjectGuid,
    merge_target: ObjectGuid,
    merge_amout: i32,
) -> bool {
    if object(w, this).is_player()
        && !player_has_enough_burden_to_add_to_inventory(w, this, world_object)
    {
        return false;
    }

    let target = object(w, merge_target);
    let current_stack_size = target.stack_size();
    let max_stack_size = target.max_stack_size();
    let new_stack_size = current_stack_size.map(|s| s.wrapping_add(merge_amout));

    // Lifted `int? <= ushort?`: false when either side is null.
    matches!((new_stack_size, max_stack_size), (Some(n), Some(m)) if n <= i32::from(m))
}

/// `TryAddToInventory(worldObject, out Container container, ..)`: the container the item went
/// into (`this` or a side pack), or `None`. Defaults as [`try_add_to_inventory`].
// ACE: Container.TryAddToInventory
// ACE-BUG: the burden check runs only when this container is itself the Player ("bug: should be root owner"), so an item added straight to a player's side pack skips it.
pub fn try_add_to_inventory_with_container(
    w: &mut World,
    this: ObjectGuid,
    world_object: ObjectGuid,
    placement_position: i32,
    limit_to_main_pack_only: bool,
    burden_check: bool,
) -> Option<ObjectGuid> {
    // bug: should be root owner
    if object(w, this).is_player()
        && burden_check
        && !player_has_enough_burden_to_add_to_inventory(w, this, world_object)
    {
        return None;
    }

    let use_backpack_slot = object(w, world_object).use_backpack_slot();

    let container_items: Vec<ObjectGuid> = inventory_values(w, this)
        .into_iter()
        .filter(|&g| object(w, g).use_backpack_slot() == use_backpack_slot)
        .collect();

    if use_backpack_slot {
        // Retail's signed capacity (V247).
        if !dereth_rules::capacity::has_room(
            object(w, this).container_capacity().unwrap_or(0),
            count_i32(container_items.len()),
        ) {
            return None;
        }
    } else if !dereth_rules::capacity::has_room(
        object(w, this).item_capacity().unwrap_or(0),
        count_i32(container_items.len()),
    ) {
        // Retail's signed capacity (V247).
        // Can we add this to any side pack?
        if !limit_to_main_pack_only {
            let mut containers: Vec<ObjectGuid> = inventory_values(w, this)
                .into_iter()
                .filter(|&g| object(w, g).is_container())
                .collect();
            let placement_of =
                |w: &World, g: ObjectGuid| object(w, g).placement().map_or(0, |p| p.0);
            let mut keyed: Vec<(u32, ObjectGuid)> = containers
                .iter()
                .map(|&g| (placement_of(w, g), g))
                .collect();
            list_sort(&mut keyed, |a, b| compare_to(a.0, b.0));
            containers = keyed.into_iter().map(|(_, g)| g).collect();

            for side_pack in containers {
                if let Some(container) = try_add_to_inventory_with_container(
                    w,
                    side_pack,
                    world_object,
                    placement_position,
                    true,
                    true,
                ) {
                    let (encumbrance, value) = burden_and_value(w, world_object);
                    add_burden_and_value(w, this, encumbrance, value);

                    return Some(container);
                }
            }
        }

        return None;
    }

    if inventory(object(w, this)).contains_key(&world_object) {
        return None;
    }

    {
        let item = object_mut(w, world_object);
        item.set_location(None);
        item.set_placement(Some(Placement::Resting));

        item.set_owner_id(Some(this.full()));
        item.set_container_id(Some(this.full()));
        item.wo.world_object_properties.container = Some(this);
        item.set_placement_position(Some(placement_position)); // Server only variable that we use to remember/restore the order in which items exist in a container
    }

    // Move all the existing items PlacementPosition over.
    for i in container_items {
        let o = object_mut(w, i);
        if o.placement_position()
            .is_some_and(|p| p >= placement_position)
        {
            let moved = o.placement_position().map(|p| p.wrapping_add(1));
            o.set_placement_position(moved);
        }
    }

    inventory_mut(object_mut(w, this)).add(world_object, ());

    let (encumbrance, value) = burden_and_value(w, world_object);
    add_burden_and_value(w, this, encumbrance, value);

    dispatch::on_add_item::on_add_item(w, this);

    Some(this)
}

/// Removes all items from an inventory. TRUE if all items were removed successfully.
/// ACE's default: `force_save = false`.
// ACE: Container.ClearInventory
// ACE-BUG: `success` is never reset, so after the first failed removal no later item is destroyed, though every later item is still removed.
pub fn clear_inventory(w: &mut World, this: ObjectGuid, force_save: bool) -> bool {
    let item_guids = inventory_values(w, this);
    clear_items(w, this, item_guids, force_save)
}

/// The shared loop of `ClearInventory` and `ClearUnmanagedInventory`.
fn clear_items(
    w: &mut World,
    this: ObjectGuid,
    item_guids: Vec<ObjectGuid>,
    force_save: bool,
) -> bool {
    let mut success = true;
    for item_guid in item_guids {
        let item = try_remove_from_inventory_with_item(w, this, item_guid, force_save);
        if item.is_none() {
            success = false;
        }

        if success {
            let item = item.expect("removed above");
            crate::world_objects::world_object::destroy(w, item, true, false);
        }
    }
    if force_save {
        dispatch::save_biota_to_database::save_biota_to_database(w, this, true);
    }

    success
}

/// Removes all items from an inventory that are unmanaged/controlled. TRUE if all unmanaged items
/// were removed successfully. ACE's default: `force_save = false`.
// ACE: Container.ClearUnmanagedInventory
// ACE-BUG: as ClearInventory, `success` is never reset, so after the first failed removal no later item is destroyed.
pub fn clear_unmanaged_inventory(w: &mut World, this: ObjectGuid, force_save: bool) -> bool {
    let o = object(w, this);
    if o.is_storage() || o.biota.weenie_class_id == u32::from(WeenieClassName::W_STORAGE_CLASS.0) {
        return false; // Do not clear storage, ever.
    }

    let item_guids: Vec<ObjectGuid> = inventory_values(w, this)
        .into_iter()
        .filter(|&g| object(w, g).generator_id().is_none())
        .collect();
    clear_items(w, this, item_guids, force_save)
}

/// This will clear the ContainerId and PlacementPosition properties. It will also subtract the
/// EncumbranceVal and Value. ACE's default: `force_save = false`.
// ACE: Container.TryRemoveFromInventory
pub fn try_remove_from_inventory(
    w: &mut World,
    this: ObjectGuid,
    object_guid: ObjectGuid,
    force_save: bool,
) -> bool {
    try_remove_from_inventory_with_item(w, this, object_guid, force_save).is_some()
}

/// `TryRemoveFromInventory(objectGuid, out WorldObject item, forceSave)`: the removed item, from
/// this container or one of its side packs, or `None`.
// ACE: Container.TryRemoveFromInventory
pub fn try_remove_from_inventory_with_item(
    w: &mut World,
    this: ObjectGuid,
    object_guid: ObjectGuid,
    force_save: bool,
) -> Option<ObjectGuid> {
    // first search me / add all items of type.
    if inventory_mut(object_mut(w, this))
        .remove(&object_guid)
        .is_some()
    {
        let item = object_guid;
        let (removed_items_placement_position, item_use_backpack_slot) = {
            let o = object_mut(w, item);
            let removed_items_placement_position = o.placement_position().unwrap_or(0);

            o.set_owner_id(None);
            o.set_container_id(None);
            o.wo.world_object_properties.container = None;
            o.set_placement_position(None);

            (removed_items_placement_position, o.use_backpack_slot())
        };

        // Move all the existing items PlacementPosition over.
        for i in inventory_values(w, this) {
            let o = object_mut(w, i);
            if o.use_backpack_slot() == item_use_backpack_slot
                && o.placement_position()
                    .is_some_and(|p| p > removed_items_placement_position)
            {
                let moved = o.placement_position().map(|p| p.wrapping_sub(1));
                o.set_placement_position(moved);
            }
        }

        let (encumbrance, value) = burden_and_value(w, item);
        subtract_burden_and_value(w, this, encumbrance, value);

        if force_save {
            dispatch::save_biota_to_database::save_biota_to_database(w, item, true);
        }

        dispatch::on_remove_item::on_remove_item(w, this, item);

        return Some(item);
    }

    // next search all containers for item.. run function again for each container.
    let side_containers: Vec<ObjectGuid> = inventory_values(w, this)
        .into_iter()
        .filter(|&g| object(w, g).biota.weenie_type == WeenieType::Container)
        .collect();
    for container in side_containers {
        if let Some(item) = try_remove_from_inventory_with_item(w, container, object_guid, false) {
            let (encumbrance, value) = burden_and_value(w, item);
            subtract_burden_and_value(w, this, encumbrance, value);

            return Some(item);
        }
    }

    None
}

/// "The {Name} is already in use by {currentViewer}!", as the transient error.
// ACE: Container.InUseMessage
#[must_use]
pub fn in_use_message(w: &World, this: ObjectGuid) -> String {
    // verified this message was sent for corpses, instead of WeenieErrorWithString.The_IsCurrentlyInUse
    let mut current_viewer = "someone else".to_owned();

    if property_manager_get_bool(w, "container_opener_name") {
        let o = object(w, this);
        let viewer = ObjectGuid::new(o.viewer());
        let name = o
            .current_landblock
            .and_then(|lb| crate::entity::landblock::get_object(w, lb, viewer, true))
            .and_then(|g| dispatch::name::name(w, g));
        if let Some(name) = name {
            current_viewer = name;
        }
    }
    let name = dispatch::name::name(w, this).unwrap_or_default();
    format!("The {name} is already in use by {current_viewer}!")
}

/// `SendInventory(player)`: CreateObject for every item and every side container's item, then
/// ViewContents for this container and for each sub-container, then the creates.
// ACE: Container.SendInventory
fn send_inventory(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    // send createobject for all objects in this container's inventory to player
    let mut items_to_send: Vec<GameMessage> = Vec::new();

    for item in inventory_values(w, this) {
        // FIXME: only send messages for unknown objects
        items_to_send.push(game_message_create_object(w, item, false, false));

        if object(w, item).is_container() {
            for container_item in inventory_values(w, item) {
                items_to_send.push(game_message_create_object(w, container_item, false, false));
            }
        }
    }

    let Some(session) = player_session(w, player) else {
        return;
    };

    let view_contents = game_event_view_contents(w, session, this);
    enqueue_send(w, session, view_contents);

    // send sub-containers
    let sub_containers: Vec<ObjectGuid> = inventory_values(w, this)
        .into_iter()
        .filter(|&g| object(w, g).is_container())
        .collect();
    for container in sub_containers {
        let view_contents = game_event_view_contents(w, session, container);
        enqueue_send(w, session, view_contents);
    }

    enqueue_send_many(w, session, items_to_send);
}

/// DeleteObject for every item and every side container's item. (Unused in ACE.)
// ACE: Container.SendDeletesForMyInventory
#[allow(dead_code)]
fn send_deletes_for_my_inventory(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    // send deleteobjects for all objects in this container's inventory to player
    let mut items_to_send: Vec<GameMessage> = Vec::new();

    for item in inventory_values(w, this) {
        // FIXME: only send messages for known objects
        items_to_send.push(game_message_delete_object(object_mut(w, item)));

        if object(w, item).is_container() {
            for container_item in inventory_values(w, item) {
                items_to_send.push(game_message_delete_object(object_mut(w, container_item)));
            }
        }
    }

    let Some(session) = player_session(w, player) else {
        return;
    };
    enqueue_send_many(w, session, items_to_send);
}

/// This is raised when player adds item to container... (see [`try_add_to_inventory`]). Places
/// each `Contain`/`ContainTreasure` create-list item in the inventory. Constructors cannot run it
/// (the new objects need guids and the world), so `container_ctor` points here and the caller
/// runs it once the container is in `World.objects`.
// ACE: Container.GenerateContainList
pub fn generate_contain_list(w: &mut World, this: ObjectGuid) {
    let Some(create_list) = object(w, this).biota.properties_create_list.clone() else {
        return;
    };

    for item in create_list.iter().filter(|x| {
        x.destination_type == DestinationType::Contain
            || x.destination_type == DestinationType::ContainTreasure
    }) {
        let wo = crate::world_objects::world_object_equipment::create_new_world_object_by_wcid(
            w,
            item.weenie_class_id,
        );

        let Some(mut wo) = wo else { continue };

        if !this.is_player() {
            wo.set_generator_id(Some(this.full())); // add this to mark item as "managed" so container resets don't delete it.
        }

        if item.palette > 0 {
            wo.set_palette_template(Some(i32::from(item.palette)));
        }
        if item.shade > 0.0 {
            wo.set_shade(Some(f64::from(item.shade)));
        }
        if item.stack_size > 1 {
            wo.set_stack_size(Some(item.stack_size));
        }

        let guid = wo.guid;
        assert!(w.objects.insert(wo).is_ok(), "fresh dynamic guid");
        // the item's own constructor-time work (a sack's own contain list)
        crate::world_objects::creature::post_insert(w, guid);
        if !try_add_to_inventory(w, this, guid, 0, false, true) {
            // Not ACE: the object ACE drops is unreachable (C# collects it).
            w.objects.remove(guid);
        }
    }
}

/// Merges partial stacks of the same weenie, later items into earlier ones.
// ACE: Container.MergeAllStackables
pub fn merge_all_stackables(w: &mut World, this: ObjectGuid) {
    let inventory = inventory_values(w, this);

    for i in (1..inventory.len()).rev() {
        let source_item = inventory[i];

        let source_max = object(w, source_item).max_stack_size();
        if source_max.is_none_or(|m| m <= 1) {
            continue;
        }

        for &destination_item in &inventory[..i] {
            let (d_wcid, d_stack, d_max) = {
                let d = object(w, destination_item);
                (
                    d.biota.weenie_class_id,
                    d.stack_size(),
                    d.max_stack_size().map(i32::from),
                )
            };
            // `destinationItem.StackSize == destinationItem.MaxStackSize`: lifted, so two nulls are equal.
            if d_wcid != object(w, source_item).biota.weenie_class_id || d_stack == d_max {
                continue;
            }

            let source_stack = object(w, source_item).stack_size();
            let room = d_max
                .zip(d_stack)
                .map(|(m, s)| m.wrapping_sub(s))
                .unwrap_or(0);
            let amount = std::cmp::min(source_stack.unwrap_or(0), room);

            object_mut(w, source_item).set_stack_size(source_stack.map(|s| s.wrapping_sub(amount)));

            object_mut(w, destination_item).set_stack_size(d_stack.map(|s| s.wrapping_add(amount)));

            if object(w, source_item).stack_size() == Some(0) {
                try_remove_from_inventory(w, this, source_item, false);
                if !object(w, source_item).wo.world_object.is_destroyed {
                    crate::world_objects::world_object::destroy(w, source_item, true, false);
                }
                break;
            }
        }
    }
}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

/// This is raised by Player.HandleActionUseItem. The item does not exist in the players
/// possession. If the item was outside of range, the player will have been commanded to move
/// using DoMoveTo before ActOnUse is called. When this is called, it should be assumed that the
/// player is within range.
// ACE: Container.ActOnUse
pub fn container_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    // If we have a previous container open, let's close it
    let last_opened_container_id = player_last_opened_container_id(w, player);
    if last_opened_container_id != ObjectGuid::INVALID && last_opened_container_id != this {
        let last_opened_container = object(w, this)
            .current_landblock
            .and_then(|lb| {
                crate::entity::landblock::get_object(w, lb, last_opened_container_id, true)
            })
            .filter(|&g| object(w, g).is_container());

        if let Some(last_opened_container) = last_opened_container {
            let c = object(w, last_opened_container);
            if c.is_open() && c.viewer() == player.full() {
                dispatch::close::close(w, last_opened_container, player);
            }
        }
    }

    let o = object(w, this);
    if o.owner_id().is_some_and(|id| id > 0) || o.container_id().is_some_and(|id| id > 0) {
        return; // Do nothing else if container is owned by something.
    }

    if !o.is_open() {
        dispatch::open::open(w, this, player);
    } else {
        let viewer = o.viewer();
        if viewer == 0 {
            dispatch::close::close(w, this, ObjectGuid::INVALID);
        } else if viewer == player.full() {
            dispatch::close::close(w, this, player);
        } else {
            let message = in_use_message(w, this);
            player_send_transient_error(w, player, &message);
        }
    }
}

// ACE: Container.Open
pub fn container_open(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
) {
    if object(w, this).is_open() {
        let message = in_use_message(w, this);
        player_send_transient_error(w, player, &message);
        return;
    }

    player_set_last_opened_container_id(w, player, this);

    let o = object_mut(w, this);
    o.set_is_open(true);

    o.set_viewer(player.full());

    dispatch::do_on_open_motion_changes::do_on_open_motion_changes(w, this);

    send_inventory(w, this, player);

    let o = object(w, this);
    if !o.is_chest() && !o.reset_message_pending() {
        if let Some(reset_interval) = o.reset_interval() {
            let mut action_chain = ActionChain::new();
            if reset_interval < 15.0 {
                action_chain.add_delay_seconds(w, 15.0);
            } else {
                action_chain.add_delay_seconds(w, reset_interval);
            }
            action_chain.add_action(Actor::Object(this), move |w| {
                dispatch::reset::reset(w, this)
            });
            //actionChain.AddAction(this, () =>
            //{
            //    Close(player);
            //});
            action_chain.enqueue_chain(w);

            object_mut(w, this).set_reset_message_pending(true);
        }
    }
}

// ACE: Container.DoOnOpenMotionChanges
#[allow(unused_variables)]
pub fn container_do_on_open_motion_changes(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> f32 {
    0.0
}

/// `player` is `ObjectGuid::INVALID` (never in `World.objects`) for ACE's `Close(null)`.
// ACE: Container.Close
pub fn container_close(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
) {
    if !object(w, this).is_open() {
        return;
    }

    let anim_time = dispatch::do_on_close_motion_changes::do_on_close_motion_changes(w, this);

    if anim_time <= 0.0 {
        dispatch::finish_close::finish_close(w, this, player);
    } else {
        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, f64::from(anim_time / 2.0f32));
        action_chain.add_action(Actor::Object(this), move |w| {
            dispatch::finish_close::finish_close(w, this, player)
        });
        action_chain.enqueue_chain(w);
    }
}

// ACE: Container.DoOnCloseMotionChanges
#[allow(unused_variables)]
pub fn container_do_on_close_motion_changes(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> f32 {
    0.0
}

/// `player` is `ObjectGuid::INVALID` for ACE's `null`.
// ACE: Container.FinishClose
pub fn container_finish_close(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
) {
    let o = object_mut(w, this);
    o.set_is_open(false);
    o.set_viewer(0);

    if w.objects.contains(player) {
        if let Some(session) = player_session(w, player) {
            if let Some(data) = w.sessions.get_mut(session) {
                let message = game_event_close_ground_container(data, this);
                enqueue_send(w, session, message);
            }
        }

        if player_last_opened_container_id(w, player) == this {
            player_set_last_opened_container_id(w, player, ObjectGuid::INVALID);
        }

        // send deleteobject for all objects in this container's inventory to player
        // this seems logical, but it bugs out the client for re-opening chests w/ respawned items
        /*var itemsToSend = new List<GameMessage>();

        foreach (var item in Inventory.Values)
            itemsToSend.Add(new GameMessageDeleteObject(item));

        player.Session.Network.EnqueueSend(itemsToSend.ToArray());*/
    }
}

/// # Panics
/// When the container is in no landblock (`CurrentLandblock.GetObject` on null).
// ACE: Container.Reset
// ACE-BUG: CurrentLandblock is dereferenced without a null check, so a reset that fires after the container left its landblock throws NullReferenceException.
pub fn container_reset(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    let o = object(w, this);
    let current_landblock = o
        .current_landblock
        .expect("System.NullReferenceException: CurrentLandblock");
    let viewer = ObjectGuid::new(o.viewer());
    let player = crate::entity::landblock::get_object(w, current_landblock, viewer, true)
        .filter(|&g| object(w, g).is_player())
        .unwrap_or(ObjectGuid::INVALID);

    if object(w, this).is_open() {
        dispatch::close::close(w, this, player);
    }

    //if (IsGenerator)
    //{
    //    ResetGenerator();
    //    if (InitCreate > 0)
    //        Generator_Regeneration();
    //}

    clear_unmanaged_inventory(w, this, false);

    object_mut(w, this).set_reset_message_pending(false);
}

/// This event is raised after the containers items have been completely loaded from the database.
// ACE: Container.OnInitialInventoryLoadCompleted
#[allow(unused_variables)]
pub fn container_on_initial_inventory_load_completed(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) {
    // empty base
}

/// This event is raised when player adds item to container.
// ACE: Container.OnAddItem
#[allow(unused_variables)]
pub fn container_on_add_item(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    // empty base
}

/// This event is raised when player removes item from container.
// ACE: Container.OnRemoveItem
#[allow(unused_variables)]
pub fn container_on_remove_item(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    world_object: empyrean_entity::ObjectGuid,
) {
    // empty base
}

// ACE: Container.MotionPickup
#[allow(unused_variables)]
pub fn container_motion_pickup(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> empyrean_entity::enums::MotionCommand {
    empyrean_entity::enums::MotionCommand::Pickup
}

// ACE: Container.IsAttunedOrContainsAttuned
pub fn container_is_attuned_or_contains_attuned(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> bool {
    crate::world_objects::world_object::world_object_is_attuned_or_contains_attuned(w, this)
        || inventory(object(w, this)).keys().any(|&i| {
            dispatch::is_attuned_or_contains_attuned::is_attuned_or_contains_attuned(w, i)
        })
}

// ACE: Container.IsStickyAttunedOrContainsStickyAttuned
pub fn container_is_sticky_attuned_or_contains_sticky_attuned(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> bool {
    crate::world_objects::world_object::world_object_is_sticky_attuned_or_contains_sticky_attuned(w, this)
        || inventory(object(w, this)).keys().any(|&i| {
            dispatch::is_sticky_attuned_or_contains_sticky_attuned::is_sticky_attuned_or_contains_sticky_attuned(w, i)
        })
}

// ACE: Container.IsUniqueOrContainsUnique
pub fn container_is_unique_or_contains_unique(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> bool {
    crate::world_objects::world_object::world_object_is_unique_or_contains_unique(w, this)
        || inventory(object(w, this))
            .keys()
            .any(|&i| dispatch::is_unique_or_contains_unique::is_unique_or_contains_unique(w, i))
}

// ACE: Container.IsBeingTradedOrContainsItemBeingTraded
pub fn container_is_being_traded_or_contains_item_being_traded(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    guid_list: &empyrean_common::dotnet::DotNetHashSet<empyrean_entity::ObjectGuid>,
) -> bool {
    crate::world_objects::world_object::world_object_is_being_traded_or_contains_item_being_traded(w, this, guid_list)
        || inventory(object(w, this)).keys().any(|&i| {
            dispatch::is_being_traded_or_contains_item_being_traded::is_being_traded_or_contains_item_being_traded(w, i, guid_list)
        })
}

// ACE: Container.GetUniqueObjects
pub fn container_get_unique_objects(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> Vec<empyrean_entity::ObjectGuid> {
    let mut unique_objects = Vec::new();

    if object(w, this).unique().is_some() {
        unique_objects.push(this);
    }

    for &item in inventory(object(w, this)).keys() {
        unique_objects.extend(dispatch::get_unique_objects::get_unique_objects(w, item));
    }

    unique_objects
}

/// # Panics
/// When the container is open and has no `ActivationTalk`: ACE writes the null string.
// ACE: Container.OnTalk
// ACE-BUG: an open container with no ActivationTalk builds GameMessageSystemChat(null), whose WriteString16L throws NullReferenceException.
pub fn container_on_talk(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if w.objects.get(activator).is_some_and(WorldObject::is_player) {
        let o = object(w, this);
        if o.is_open() {
            let activation_talk = o
                .activation_talk()
                .expect("System.NullReferenceException: ActivationTalk");
            let message = game_message_system_chat(&activation_talk, ChatMessageType::Broadcast);
            if let Some(session) = player_session(w, activator) {
                enqueue_send(w, session, message);
            }
        }
    }
}

// ---- constructors and SetEphemeralValues ----

/// `new Container(weenie, guid)` / `new Container(biota)`. The biota constructor first repairs
/// stale saved data (`Open`, and `EncumbranceVal`/`Value` from before they were ephemeral), then,
/// for anything but a player's possessions, loads the container's inventory.
// ACE: Container.Container
pub fn container_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    let from_biota = src.is_biota();
    crate::world_objects::world_object::world_object_ctor(o, env, src);

    if !from_biota {
        container_initialize_property_dictionaries(o);
        container_set_ephemeral_values(o, false);

        o.container
            .as_mut()
            .expect("a Container")
            .container
            .inventory_loaded = true;
        return;
    }

    if o.biota
        .try_remove_property(empyrean_entity::enums::PropertyBool::Open)
    {
        o.wo.world_object_database.changes_detected = true;
    }

    // This is a temporary fix for objects that were loaded with this PR when EncumbranceVal was not treated as ephemeral. 2020-03-28
    // This can be removed later.
    container_restore_weenie_int(o, env, empyrean_entity::enums::PropertyInt::EncumbranceVal);

    // This is a temporary fix for objects that were loaded with this PR when Value was not treated as ephemeral. 2020-03-28
    // This can be removed later.
    if !o.is_creature() {
        container_restore_weenie_int(o, env, empyrean_entity::enums::PropertyInt::Value);
    }

    container_initialize_property_dictionaries(o);
    container_set_ephemeral_values(o, true);

    // A player has their possessions passed via the ctor. All other world objects must load their own inventory
    if !o.is_player() && !empyrean_entity::ObjectGuid::is_player_guid(o.container_id().unwrap_or(0))
    {
        // `DatabaseManager.Shard.GetInventoryInParallel(biota.Id, false, biotas =>
        //     EnqueueAction(new ActionEventDelegate(() => SortBiotasIntoInventory(biotas))));`
        // DIVERGE: the object is not in `World.objects` yet, so the load is marked here and started by the insertion hook (`creature::post_insert` -> `post_insert_load_inventory`), before the object is placed anywhere.
        o.container
            .as_mut()
            .expect("a Container")
            .container
            .inventory_load_pending = true;
    }
}

/// One of the `Container(Biota)` data fixes: when the biota saved `property`, it is reset to the
/// weenie's value, or removed if the weenie has none.
// ACE-BUG: a cached weenie with no PropertyInt rows has a null PropertiesInt, so `weenie.PropertiesInt.TryGetValue` throws NullReferenceException when such a weenie's saved biota still carries EncumbranceVal or Value.
fn container_restore_weenie_int(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    property: empyrean_entity::enums::PropertyInt,
) {
    if !o
        .biota
        .properties_int
        .as_ref()
        .is_some_and(|d| d.contains_key(&property))
    {
        return;
    }

    let weenie = (env.get_cached_weenie)(o.biota.weenie_class_id);
    let weenie_value = weenie.as_ref().and_then(|weenie| {
        let ints = weenie
            .properties_int
            .as_ref()
            .expect("System.NullReferenceException: Weenie.PropertiesInt");
        ints.get(&property).copied()
    });

    let ints = o.biota.properties_int.as_mut().expect("checked above");
    match weenie_value {
        Some(value) => {
            if ints.get(&property) != Some(&value) {
                ints.insert(property, value);
                o.wo.world_object_database.changes_detected = true;
            }
        }
        None => {
            ints.remove(&property);
            o.wo.world_object_database.changes_detected = true;
        }
    }
}

// ACE: Container.InitializePropertyDictionaries
fn container_initialize_property_dictionaries(
    o: &mut crate::world_objects::world_object::WorldObject,
) {
    let props = &mut o.wo.world_object_properties;
    if props.ephemeral_property_ints.is_none() {
        props.ephemeral_property_ints = Some(empyrean_common::dotnet::DotNetDict::new());
    }
}

// ACE: Container.SetEphemeralValues
fn container_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    from_biota: bool,
) {
    // Containers are init at 0 burden or their initial value from database. As inventory/equipment is added the burden will be increased
    let encumbrance_val = o.encumbrance_val().unwrap_or(0);
    container_ephemeral_ints(o).try_add(
        empyrean_entity::enums::PropertyInt::EncumbranceVal,
        Some(encumbrance_val),
    );
    if !o.is_creature() && !o.is_corpse() {
        // Creatures/Corpses do not have a value
        let value = o.value().unwrap_or(0);
        container_ephemeral_ints(o)
            .try_add(empyrean_entity::enums::PropertyInt::Value, Some(value));
    }

    //CurrentMotionState = motionStateClosed; // What container defaults to open?

    if !from_biota && !o.is_creature() {
        // `GenerateContainList();`
        // DIVERGE: the new items need guids and the world, so the list is marked here and generated by the insertion hook (`creature::post_insert` -> `post_insert_generate_contain_list`) as soon as the container is in `World.objects`, before it is placed anywhere (V195).
        o.container
            .as_mut()
            .expect("a Container")
            .container
            .generate_contain_list_pending = true;
    }

    if o.container_capacity().is_none() {
        o.set_container_capacity(Some(0));
    }

    if o.use_radius().is_none() {
        o.set_use_radius(Some(0.5));
    }

    o.set_is_open(false);
}

fn container_ephemeral_ints(
    o: &mut crate::world_objects::world_object::WorldObject,
) -> &mut empyrean_common::dotnet::DotNetDict<empyrean_entity::enums::PropertyInt, Option<i32>> {
    o.wo.world_object_properties
        .ephemeral_property_ints
        .as_mut()
        .expect("InitializePropertyDictionaries ran")
}
