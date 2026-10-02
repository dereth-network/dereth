// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Inventory.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Inventory.cs`.
//!
//! The player's inventory actions: pick up, drop, move, give, split, merge and wield, and the
//! helpers the other partials call (`FindObject`, the burden checks,
//! `TryCreateInInventoryWithNetworking`, `TryConsumeFromInventoryWithNetworking`, ...).
//!
//! # Always answer the client
//!
//! The client takes an inventory lock with no timeout for each request, so every failure path
//! sends ACE's `InventoryServerSaveFailed` (or ACE's `WeenieError`) answer, exactly where ACE
//! sends it. Where ACE answers through a member of an unported file (the quest manager's
//! refusals), the shim below answers as ACE does.
//!
//! # References
//!
//! ACE's closures capture `WorldObject` references; here they capture guids and resolve them on
//! use. An object that has left `World.objects` (destroyed, V78) reads as ACE's destroyed object:
//! no landblock, no container.
//!
//! # Shims
//!
//! Members ported in other files, each a private function in [`shims`] marked `SHIM:`
//! (no anchor), calling the real port where one exists and otherwise a `not_ported!` pointer by
//! ACE name answering ACE's default. The `Player_Move.cs` MoveTo chain and the use-radius test
//! are the real ports (`player_move`, `world_object_use`), and `WorldObject.IsBusy` is the
//! `WorldObject.cs` field.

use empyrean_common::dotnet::DotNetHashSet;
use empyrean_entity::enums::{
    AetheriaBitfield, CharacterOptions1, ChatMessageType, CombatMode, CombatStyle, EmoteCategory,
    EquipMask, HeritageGroup, HookGroupType, MotionCommand, MotionStance, PhysicsState,
    PickupState, Placement, PropertyAttribute, PropertyAttribute2nd, PropertyBool,
    PropertyInstanceId, PropertyInt, RegenerationType, Skill, Sound, WeenieError,
    WeenieErrorWithString, WeenieType, WieldRequirement,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::SessionId;

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::items_to_receive::ItemsToReceive;
use crate::entity::landblock;
use crate::entity::put_item_in_container_event::PutItemInContainerEvent;
use crate::entity::unique_table::UniqueTable;
use crate::managers::landblock_manager;
use crate::managers::player_manager::player_session;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_inventory_server_save_failed::game_event_inventory_server_save_failed;
use crate::network::game_event::events::game_event_item_server_says_contain_id::game_event_item_server_says_contain_id;
use crate::network::game_event::events::game_event_item_server_says_move_item::game_event_item_server_says_move_item;
use crate::network::game_event::events::game_event_tell::game_event_tell;
use crate::network::game_event::events::game_event_view_contents::game_event_view_contents;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_event::events::game_event_wield_item::game_event_wield_item;
use crate::network::game_messages::game_message::{enqueue_send, enqueue_send_many, GameMessage};
use crate::network::game_messages::messages::game_message_create_object::game_message_create_object;
use crate::network::game_messages::messages::game_message_delete_object::game_message_delete_object;
use crate::network::game_messages::messages::game_message_inventory_remove_object::game_message_inventory_remove_object;
use crate::network::game_messages::messages::game_message_parent_event::game_message_parent_event;
use crate::network::game_messages::messages::game_message_pickup_event::game_message_pickup_event;
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_public_update_instance_id::game_message_public_update_instance_id;
use crate::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int;
use crate::network::game_messages::messages::game_message_set_stack_size::game_message_set_stack_size;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::game_messages::messages::game_message_update_position::game_message_update_position;
use crate::network::motion::movement_data::Motion;
use crate::physics::phys_ext;
use crate::sessions::SessionData;
use crate::world_objects::container;
use crate::world_objects::creature_equipment;
use crate::world_objects::entity::creature_attribute::{CreatureAttribute, StatCtx};
use crate::world_objects::world_object::{self, WorldObject};
use crate::world_objects::world_object_networking;
use crate::World;

/// `Action<bool>`: a MoveTo chain's completion callback.
pub type MoveToCallback = Box<dyn FnOnce(&mut World, bool) + Send>;

/// `Action`: a deferred call on the world thread.
pub type DeferredAction = Box<dyn FnOnce(&mut World) + Send>;

/// Non-property fields declared in `Player_Inventory.cs`.
#[derive(Default)]
pub struct PlayerInventoryFields {
    // ACE: Player.PickupState
    pub pickup_state: PickupState,
    // ACE: Player.NextPickup
    pub next_pickup: Option<DeferredAction>,
    /// The last two `PutItemInContainer` requests, newest first.
    // ACE: Player.Prev_PutItemInContainer
    pub prev_put_item_in_container: [Option<PutItemInContainerEvent>; 2],
}

impl std::fmt::Debug for PlayerInventoryFields {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayerInventoryFields")
            .field("pickup_state", &self.pickup_state)
            .field("next_pickup", &self.next_pickup.is_some())
            .field(
                "prev_put_item_in_container",
                &self.prev_put_item_in_container,
            )
            .finish_non_exhaustive()
    }
}

/// `Player.RemoveFromInventoryAction`.
// ACE: Player.RemoveFromInventoryAction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveFromInventoryAction {
    None,
    ToWieldedSlot,
    DropItem,
    GiveItem,
    TradeItem,
    SellItem,
    ToCorpseOnDeath,
    ConsumeItem,
    SpendItem,
}

/// `Player.DequipObjectAction`.
// ACE: Player.DequipObjectAction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DequipObjectAction {
    None,
    DequipToPack,
    DequipToOffPlayerContainer,
    DropItem,
    GiveItem,
    TradeItem,
    SellItem,
    ToCorpseOnDeath,
    ConsumeItem,
}

/// `Player.SearchLocations` (`[Flags]`).
// ACE: Player.SearchLocations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchLocations(pub u32);

#[allow(non_upper_case_globals)]
impl SearchLocations {
    pub const None: Self = Self(0x00);
    pub const MyInventory: Self = Self(0x01);
    pub const MyEquippedItems: Self = Self(0x02);
    pub const Landblock: Self = Self(0x04);
    pub const LastUsedContainer: Self = Self(0x08);
    pub const WieldedByOther: Self = Self(0x10);
    pub const TradedByOther: Self = Self(0x20);
    pub const ObjectsKnownByMe: Self = Self(0x40);
    pub const LastUsedHook: Self = Self(0x80);
    pub const LocationsICanMove: Self = Self(0x01 | 0x02 | 0x04 | 0x08);
    pub const Everywhere: Self = Self(0xFF);

    /// `Enum.HasFlag`.
    #[must_use]
    pub const fn has_flag(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }
}

impl std::ops::BitOr for SearchLocations {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// What `FindObject` returns: the object and its `out` parameters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Found {
    pub result: Option<ObjectGuid>,
    pub found_in_container: Option<ObjectGuid>,
    pub root_owner: Option<ObjectGuid>,
    pub was_equipped: bool,
}

// ================================================================================ helpers

/// `this` or an item: panics like the C# `NullReferenceException` when the object is gone.
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

fn fields(w: &World, this: ObjectGuid) -> &PlayerInventoryFields {
    &obj(w, this)
        .player
        .as_ref()
        .expect("InvalidCastException: not a Player")
        .player_inventory
}

fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerInventoryFields {
    &mut obj_mut(w, this)
        .player
        .as_mut()
        .expect("InvalidCastException: not a Player")
        .player_inventory
}

/// `Name` (a null name interpolates as empty).
fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

/// `item.CurrentLandblock`; an object that has left the store has none.
fn current_landblock(w: &World, g: ObjectGuid) -> Option<empyrean_entity::LandblockId> {
    w.objects.get(g).and_then(|o| o.current_landblock)
}

fn weenie_type(w: &World, g: ObjectGuid) -> WeenieType {
    obj(w, g).biota.weenie_type
}

/// `item.WeenieType == Coin || item.WeenieType == Container`.
fn is_coin_or_container(w: &World, g: ObjectGuid) -> bool {
    matches!(weenie_type(w, g), WeenieType::Coin | WeenieType::Container)
}

/// `Player.Session`, the player's session: `None` when it has none (nothing is sent).
fn session(w: &World, this: ObjectGuid) -> Option<SessionId> {
    player_session(w, this)
}

/// `Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    if let Some(s) = session(w, this) {
        enqueue_send(w, s, msg);
    }
}

/// `Session.Network.EnqueueSend(msgs...)`.
fn send_many(w: &mut World, this: ObjectGuid, msgs: Vec<GameMessage>) {
    if let Some(s) = session(w, this) {
        enqueue_send_many(w, s, msgs);
    }
}

/// Builds a game event for the player's session (consuming its `GameEventSequence`).
fn event(
    w: &mut World,
    this: ObjectGuid,
    build: impl FnOnce(&mut SessionData) -> GameMessage,
) -> Option<GameMessage> {
    let s = session(w, this)?;
    let data = w.sessions.get_mut(s)?;
    Some(build(data))
}

/// `Session.Network.EnqueueSend(new GameEventInventoryServerSaveFailed(Session, itemGuid, errorType))`.
fn save_failed(w: &mut World, this: ObjectGuid, item_guid: u32, error_type: WeenieError) {
    if let Some(m) = event(w, this, |d| {
        game_event_inventory_server_save_failed(d, item_guid, error_type)
    }) {
        send(w, this, m);
    }
}

/// `Session.Network.EnqueueSend(new GameEventCommunicationTransientString(Session, message))`.
fn transient(w: &mut World, this: ObjectGuid, message: &str) {
    if let Some(m) = event(w, this, |d| {
        game_event_communication_transient_string(d, message)
    }) {
        send(w, this, m);
    }
}

/// `Session.Network.EnqueueSend(new GameEventWeenieError(Session, errorType))`.
fn weenie_error(w: &mut World, this: ObjectGuid, error_type: WeenieError) {
    if let Some(m) = event(w, this, |d| game_event_weenie_error(d, error_type)) {
        send(w, this, m);
    }
}

/// `Session.Network.EnqueueSend(new GameEventWeenieErrorWithString(Session, errorType, message))`.
fn weenie_error_with_string(
    w: &mut World,
    this: ObjectGuid,
    error_type: WeenieErrorWithString,
    message: &str,
) {
    if let Some(m) = event(w, this, |d| {
        game_event_weenie_error_with_string(d, error_type, message)
    }) {
        send(w, this, m);
    }
}

/// `Session.Network.EnqueueSend(new GameMessageSystemChat(message, type))`.
fn system_chat(w: &mut World, this: ObjectGuid, message: &str, chat_message_type: ChatMessageType) {
    send(
        w,
        this,
        game_message_system_chat(message, chat_message_type),
    );
}

/// `new GameMessagePrivateUpdatePropertyInt(this, PropertyInt.EncumbranceVal, EncumbranceVal ?? 0)`.
fn encumbrance_update(w: &mut World, this: ObjectGuid) -> GameMessage {
    let o = obj_mut(w, this);
    let value = o.encumbrance_val().unwrap_or(0);
    game_message_private_update_property_int(o, PropertyInt::EncumbranceVal, value)
}

/// `Session.Network.EnqueueSend(new GameMessagePrivateUpdatePropertyInt(this, EncumbranceVal, ...))`.
fn send_encumbrance(w: &mut World, this: ObjectGuid) {
    let m = encumbrance_update(w, this);
    send(w, this, m);
}

/// `EnqueueBroadcast(new GameMessageSound(guid, sound))` on `broadcaster`.
fn broadcast_sound(w: &mut World, broadcaster: ObjectGuid, sound: Sound) {
    let m = game_message_sound(broadcaster, sound, 1.0);
    world_object_networking::enqueue_broadcast(w, broadcaster, true, &[m]);
}

/// `new GameMessageSetStackSize(stack)`.
fn set_stack_size_message(w: &mut World, stack: ObjectGuid) -> GameMessage {
    game_message_set_stack_size(obj_mut(w, stack))
}

/// `new GameMessagePublicUpdateInstanceID(item, property, value)`.
fn public_update_instance_id(
    w: &mut World,
    item: ObjectGuid,
    property: PropertyInstanceId,
    value: ObjectGuid,
) -> GameMessage {
    game_message_public_update_instance_id(obj_mut(w, item), property, value)
}

/// `new GameEventItemServerSaysContainId(Session, item, container)`.
fn contain_id(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
    container: ObjectGuid,
) -> Option<GameMessage> {
    let s = session(w, this)?;
    let data = w.sessions.get_mut(s)?;
    let item = w.objects.get(item)?;
    Some(game_event_item_server_says_contain_id(
        data, item, container,
    ))
}

/// `WorldObjectFactory.CreateNewWorldObject(wcid)`, put in `World.objects` (the port's objects
/// live there; one ACE drops unused is removed again by [`forget`]).
fn create_new_world_object(w: &mut World, weenie_class_id: u32) -> Option<ObjectGuid> {
    let o = crate::world_objects::world_object_equipment::create_new_world_object_by_wcid(
        w,
        weenie_class_id,
    )?;
    let guid = o.guid;
    if w.objects.insert(o).is_err() {
        panic!("a new dynamic guid 0x{:08X} is already live", guid.full());
    }
    crate::world_objects::creature::post_insert(w, guid);
    Some(guid)
}

/// An object ACE creates and then drops without destroying (the garbage collector's): removed
/// from `World.objects` (4.5a's rule for unplaced new objects).
fn forget(w: &mut World, g: ObjectGuid) {
    w.objects.remove(g);
}

/// `item.Destroy()` (its defaults).
fn destroy(w: &mut World, g: ObjectGuid) {
    world_object::destroy(w, g, true, false);
}

/// `x += y` on an `int?` (lifted: a null stays null).
fn add_nullable(current: Option<i32>, amount: i32) -> Option<i32> {
    current.map(|c| c.wrapping_add(amount))
}

/// `a * b` on an `int?` operand (lifted).
fn mul_nullable(a: Option<i32>, b: i32) -> Option<i32> {
    a.map(|a| a.wrapping_mul(b))
}

// ================================================================================ Player_Inventory.cs

/// Returns all inventory, side slot items, items in side containers, and all wielded items.
// ACE: Player.GetAllPossessions
#[must_use]
pub fn get_all_possessions(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    let mut results = container::inventory_values(w, this);

    for item in container::inventory_values(w, this) {
        if obj(w, item).is_container() {
            results.extend(container::inventory_values(w, item));
        }
    }

    results.extend(creature_equipment::equipped_objects_values(w, this));

    results
}

/// `(int)((150 * strength) + (AugmentationIncreasedCarryingCapacity * 30 * strength))`, with
/// `Attributes[Strength].Current` (enchanted: the creature attribute read through the enchantment
/// manager). The attribute is read through its handle, which reads the same biota
/// record `Attributes[Strength]` wraps.
// ACE: Player.GetEncumbranceCapacity
#[must_use]
///
/// **Retail's rule (V246, V231):** the one shared implementation,
/// `dereth_rules::burden::encumbrance_capacity`, which the client's burden bar uses: the
/// augmentation bonus capped at five (ACE's had no cap) and saturating (ACE's wrapped).
pub fn get_encumbrance_capacity(w: &World, o: &WorldObject) -> i32 {
    let strength = CreatureAttribute {
        attribute: PropertyAttribute::Strength,
    }
    .current(&mut StatCtx::detached(w, o));
    dereth_rules::burden::encumbrance_capacity(
        i32::try_from(strength).unwrap_or(i32::MAX),
        o.augmentation_increased_carrying_capacity(),
    )
}

/// `EncumbranceVal + totalEncumbranceToCheck <= GetEncumbranceCapacity() * 3`; a null
/// `EncumbranceVal` compares false.
// ACE: Player.HasEnoughBurdenToAddToInventory
#[must_use]
pub fn has_enough_burden_to_add_to_inventory_total(
    w: &World,
    o: &WorldObject,
    total_encumbrance_to_check: i32,
) -> bool {
    let capacity = get_encumbrance_capacity(w, o).wrapping_mul(3);
    o.encumbrance_val()
        .is_some_and(|e| e.wrapping_add(total_encumbrance_to_check) <= capacity)
}

/// We can still pick up null items, for some reason, we have the conditional for that.
// ACE: Player.HasEnoughBurdenToAddToInventory
#[must_use]
pub fn has_enough_burden_to_add_to_inventory(
    w: &World,
    this: ObjectGuid,
    world_object: ObjectGuid,
) -> bool {
    let encumbrance = obj(w, world_object).encumbrance_val().unwrap_or(0);
    has_enough_burden_to_add_to_inventory_total(w, obj(w, this), encumbrance)
}

// ACE: Player.HasEnoughBurdenToAddToInventory
#[must_use]
pub fn has_enough_burden_to_add_to_inventory_list(
    w: &World,
    this: ObjectGuid,
    world_objects: &[ObjectGuid],
) -> bool {
    let mut burden_total = 0i32;

    for &world_object in world_objects {
        burden_total =
            burden_total.wrapping_add(obj(w, world_object).encumbrance_val().unwrap_or(0));
    }

    has_enough_burden_to_add_to_inventory_total(w, obj(w, this), burden_total)
}

/// `(GetEncumbranceCapacity() * 3) - EncumbranceVal ?? 0`: the `??` applies to the whole
/// difference, so a null `EncumbranceVal` answers 0.
// ACE: Player.GetAvailableBurden
#[must_use]
pub fn get_available_burden(w: &World, o: &WorldObject) -> i32 {
    let capacity = get_encumbrance_capacity(w, o).wrapping_mul(3);
    o.encumbrance_val().map_or(0, |e| capacity.wrapping_sub(e))
}

/// If enough burden is available, this will try to add (via create) an item to the main pack.
/// If the main pack is full, it will try to add it to the first side pack with room. Returns the
/// container the item went into (ACE's `out Container container`), or `None`. The item is an
/// object in `World.objects`.
// ACE: Player.TryCreateInInventoryWithNetworking
pub fn try_create_in_inventory_with_networking(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
) -> Option<ObjectGuid> {
    // We don't have enough burden available or no empty pack slot.
    let container = container::try_add_to_inventory_with_container(w, this, item, 0, false, true)?;

    let m = game_message_create_object(w, item, false, false);
    send(w, this, m);

    if obj(w, item).is_container() {
        if let Some(s) = session(w, this) {
            let m = game_event_view_contents(w, s, item);
            send(w, this, m);
        }

        for o in container::inventory_values(w, item) {
            let adminvision = world_object_networking::shims::player_adminvision(w, this);
            let m = game_message_create_object(w, o, adminvision, false);
            send(w, this, m);
        }
    }

    let contain = contain_id(w, this, item, container);
    let encumbrance = encumbrance_update(w, this);
    send_many(
        w,
        this,
        contain
            .into_iter()
            .chain(std::iter::once(encumbrance))
            .collect(),
    );

    if is_coin_or_container(w, item) {
        shims::update_coin_value(w, this);
    }

    dispatch::save_biota_to_database::save_biota_to_database(w, item, true);

    Some(container)
}

/// ACE's default: `amount = int.MaxValue`.
// ACE: Player.TryConsumeFromInventoryWithNetworking
pub fn try_consume_from_inventory_with_networking(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
    amount: i32,
) -> bool {
    let item_weenie_type = weenie_type(w, item);

    if amount >= obj(w, item).stack_size().unwrap_or(1) {
        let Some(item) = container::try_remove_from_inventory_with_item(w, this, item, false)
        else {
            return false;
        };

        let m = game_message_inventory_remove_object(obj(w, item));
        send(w, this, m);

        destroy(w, item);
    } else {
        let found = find_object(
            w,
            this,
            item,
            SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
        );

        let (Some(stack), Some(stack_root_owner)) = (found.result, found.root_owner) else {
            return false;
        };

        if !adjust_stack(
            w,
            this,
            stack,
            amount.wrapping_neg(),
            found.found_in_container,
            Some(stack_root_owner),
        ) {
            return false;
        }

        let m = set_stack_size_message(w, stack);
        send(w, this, m);
    }

    send_encumbrance(w, this);

    if item_weenie_type == WeenieType::Coin {
        shims::update_coin_value(w, this);
    }

    true
}

/// The wcid overload: consumes `amount` across the items of that wcid, in inventory order. ACE's
/// default: `amount = int.MaxValue`.
// ACE: Player.TryConsumeFromInventoryWithNetworking
pub fn try_consume_from_inventory_with_networking_wcid(
    w: &mut World,
    this: ObjectGuid,
    wcid: u32,
    amount: i32,
) -> bool {
    let items = container::get_inventory_items_of_wcid(w, this, wcid);
    //items = items.OrderBy(o => o.Value).ToList();

    let mut left_req = amount;
    for item in items {
        let remove_num = left_req.min(obj(w, item).stack_size().unwrap_or(1));
        if !try_consume_from_inventory_with_networking(w, this, item, remove_num) {
            return false;
        }

        left_req = left_req.wrapping_sub(remove_num);
        if left_req <= 0 {
            break;
        }
    }
    true
}

// ACE: Player.DeepSave
fn deep_save(w: &mut World, item: ObjectGuid) {
    let mut biotas = Vec::new();

    if obj(w, item).wo.world_object_database.changes_detected {
        dispatch::save_biota_to_database::save_biota_to_database(w, item, false);
        biotas.push(item);
    }

    // if the player is dropping a container to the landblock,
    // we must ensure any items within the container also have the correct properties
    if obj(w, item).is_container() {
        for sub_item in container::inventory_values(w, item) {
            if obj(w, sub_item).wo.world_object_database.changes_detected {
                dispatch::save_biota_to_database::save_biota_to_database(w, sub_item, false);
                biotas.push(sub_item);
            }
        }
    }

    shims::shard_save_biotas_in_parallel(w, &biotas);
}

/// Returns the removed item (ACE's `out WorldObject item`), or `None`.
// ACE: Player.TryRemoveFromInventoryWithNetworking
pub fn try_remove_from_inventory_with_networking(
    w: &mut World,
    this: ObjectGuid,
    object_guid: ObjectGuid,
    remove_from_inventory_action: RemoveFromInventoryAction,
) -> Option<ObjectGuid> {
    let item = container::try_remove_from_inventory_with_item(w, this, object_guid, false)?;

    if remove_from_inventory_action != RemoveFromInventoryAction::SellItem
        && remove_from_inventory_action != RemoveFromInventoryAction::GiveItem
    {
        let m =
            public_update_instance_id(w, item, PropertyInstanceId::Container, ObjectGuid::INVALID);
        send(w, this, m);
    }

    if matches!(
        remove_from_inventory_action,
        RemoveFromInventoryAction::GiveItem
            | RemoveFromInventoryAction::SpendItem
            | RemoveFromInventoryAction::ToCorpseOnDeath
    ) {
        let m = game_message_inventory_remove_object(obj(w, item));
        send(w, this, m);
    }

    if remove_from_inventory_action != RemoveFromInventoryAction::ToWieldedSlot {
        // The item has gone off-player, so we must do some additional work

        send_encumbrance(w, this);

        if is_coin_or_container(w, item) {
            shims::update_coin_value(w, this);
        }

        // We must update the database with the latest ContainerId and WielderId properties.
        // If we don't, the player can drop the item, log out, and log back in. If the landblock hasn't queued a database save in that time,
        // the player will end up loading with this object in their inventory even though the landblock is the true owner. This is because
        // when we load player inventory, the database still has the record that shows this player as the ContainerId for the item.
        deep_save(w, item);
    }

    if matches!(
        remove_from_inventory_action,
        RemoveFromInventoryAction::ConsumeItem | RemoveFromInventoryAction::TradeItem
    ) {
        let m = game_message_delete_object(obj_mut(w, item));
        send(w, this, m);
    }

    Some(item)
}

// ACE: Player.TryShuffleStance
pub fn try_shuffle_stance(w: &mut World, this: ObjectGuid, wielded_location: EquipMask) {
    //Console.WriteLine($"{Name}.TryStanceShuffle({wieldedLocation})");

    // do the appropriate combat stance shuffling, based on the item types
    // todo: instead of switching the weapon immediately, the weapon should be swapped in the middle of the animation chain

    // todo: find better / more appropriate logic for this
    // should this be based on something else, such as CombatUse?
    if (wielded_location & EquipMask::Selectable).is_empty() {
        return;
    }

    let combat_mode = shims::combat_mode(w, this);
    if combat_mode != CombatMode::NonCombat && combat_mode != CombatMode::Undef {
        match wielded_location {
            EquipMask::MissileWeapon => {
                shims::handle_action_change_combat_mode(w, this, CombatMode::Missile, false, None)
            }
            EquipMask::Held => {
                shims::handle_action_change_combat_mode(w, this, CombatMode::Magic, false, None)
            }
            EquipMask::Shield => {
                let weapon = creature_equipment::get_equipped_weapon(w, this, true);

                if weapon.is_some_and(|g| {
                    obj(w, g).default_combat_style() == Some(CombatStyle::ThrownWeapon)
                }) {
                    shims::handle_action_change_combat_mode(
                        w,
                        this,
                        CombatMode::Missile,
                        false,
                        None,
                    );
                } else {
                    shims::handle_action_change_combat_mode(
                        w,
                        this,
                        CombatMode::Melee,
                        false,
                        None,
                    );
                }
            }
            _ => shims::handle_action_change_combat_mode(w, this, CombatMode::Melee, false, None),
        }
    }
}

/// This will set the CurrentWieldedLocation property to wieldedLocation and the Wielder property
/// to this guid and will add it to the EquippedObjects dictionary. It will also increase the
/// EncumbranceVal and Value.
// ACE: Player.TryEquipObjectWithNetworking
pub fn try_equip_object_with_networking(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
    wielded_location: EquipMask,
) -> bool {
    if !creature_equipment::try_equip_object_with_broadcasting(w, this, item, wielded_location) {
        return false;
    }

    let m1 = public_update_instance_id(w, item, PropertyInstanceId::Wielder, ObjectGuid::INVALID);
    let m2 = game_message_public_update_property_int(
        obj_mut(w, item),
        PropertyInt::CurrentWieldedLocation,
        0,
    );
    let m3 = event(w, this, |d| {
        game_event_wield_item(d, item.full(), wielded_location)
    });
    let m4 = game_message_sound(this, Sound::WieldObject, 1.0);
    send_many(
        w,
        this,
        [Some(m1), Some(m2), m3, Some(m4)]
            .into_iter()
            .flatten()
            .collect(),
    );

    if obj(w, item).gear_max_health().is_some() {
        crate::world_objects::player_vitals::handle_max_health_update(w, this);
    }

    try_shuffle_stance(w, this, wielded_location);

    // handle item spells
    if obj(w, item).item_cur_mana().is_some_and(|m| m > 0) {
        try_activate_spells(w, this, item);
    }

    // handle equipment sets
    if obj(w, item).equipment_set_id().is_some() {
        shims::equip_item_from_set(w, this, item);
    }

    true
}

// ACE: Player.TryActivateSpells
fn try_activate_spells(w: &mut World, this: ObjectGuid, item: ObjectGuid) -> bool {
    // check activation requirements
    let (success, message) = shims::check_use_requirements(w, item, this);

    if !success {
        if let Some(message) = message {
            send(w, this, message);
        }

        return false;
    }

    // handle special case
    if obj(w, item).item_cur_mana() == Some(1) {
        obj_mut(w, item).set_item_cur_mana(Some(0));
        return false;
    }

    let mut is_affecting = true; // ??

    let spells: Vec<i32> = obj(w, item)
        .biota
        .properties_spell_book
        .as_ref()
        .map(|b| b.keys().copied().collect())
        .unwrap_or_default();
    for spell in spells {
        #[allow(clippy::cast_sign_loss)] // C#'s (uint)int
        let spell_id = spell as u32;
        if shims::has_proc_spell(w, item, spell_id) {
            continue;
        }

        if obj(w, item).spell_did() == Some(spell_id) {
            continue;
        }

        let success = shims::create_item_spell(w, this, item, spell_id);

        if success {
            is_affecting = true;
        }
    }

    if is_affecting {
        shims::on_spells_activated(w, item);
        let o = obj_mut(w, item);
        let mana = o.item_cur_mana().map(|m| m.wrapping_sub(1));
        o.set_item_cur_mana(mana);
    }

    true
}

/// This will remove the Wielder and CurrentWieldedLocation properties on the item and will remove
/// it from the EquippedObjects dictionary. It does not add it to inventory as you could be
/// unwielding to the ground or a chest. It will also decrease the EncumbranceVal and Value.
/// Returns the dequipped item (ACE's `out WorldObject item`), or `None`.
// ACE: Player.TryDequipObjectWithNetworking
pub fn try_dequip_object_with_networking(
    w: &mut World,
    this: ObjectGuid,
    object_guid: ObjectGuid,
    dequip_object_action: DequipObjectAction,
) -> Option<ObjectGuid> {
    let (item, wielded_location) = creature_equipment::try_dequip_object_with_broadcasting(
        w,
        this,
        object_guid,
        dequip_object_action == DequipObjectAction::DropItem,
    )?;

    let m1 = public_update_instance_id(w, item, PropertyInstanceId::Wielder, ObjectGuid::INVALID);
    let m2 = game_message_public_update_property_int(
        obj_mut(w, item),
        PropertyInt::CurrentWieldedLocation,
        0,
    );
    let m3 = game_message_pickup_event(obj_mut(w, item));
    let m4 = game_message_sound(this, Sound::UnwieldObject, 1.0);
    send_many(w, this, vec![m1, m2, m3, m4]);

    // handle equipment sets
    if obj(w, item).equipment_set_id().is_some() {
        shims::dequip_item_from_set(w, this, item);
    }

    if obj(w, item).gear_max_health().is_some() {
        crate::world_objects::player_vitals::handle_max_health_update(w, this);
    }

    if dequip_object_action == DequipObjectAction::ToCorpseOnDeath
        || dequip_object_action == DequipObjectAction::TradeItem
    {
        let m = game_message_delete_object(obj_mut(w, item));
        send(w, this, m);
    }

    if dequip_object_action == DequipObjectAction::ConsumeItem {
        let m = game_message_delete_object(obj_mut(w, item));
        send(w, this, m);
        destroy(w, item);
    }

    if dequip_object_action != DequipObjectAction::DequipToPack {
        // The item has gone off-player, so we must do some additional work

        send_encumbrance(w, this);

        // We must update the database with the latest ContainerId and WielderId properties.
        // If we don't, the player can drop the item, log out, and log back in. If the landblock hasn't queued a database save in that time,
        // the player will end up loading with this object in their inventory even though the landblock is the true owner. This is because
        // when we load player inventory, the database still has the record that shows this player as the ContainerId for the item.
        if w.objects.contains(item) {
            deep_save(w, item);
        } else {
            // A consumed item has been destroyed (V78: it left the store): ACE's DeepSave of a
            // destroyed object saves no biota.
            shims::shard_save_biotas_in_parallel(w, &[]);
        }
    }

    if dequip_object_action != DequipObjectAction::ToCorpseOnDeath
        && requires_stance_swap(w, this, wielded_location, true)
    {
        let mut new_combat_mode = CombatMode::Melee;

        if shims::combat_mode(w, this) == CombatMode::Missile
            && wielded_location == EquipMask::MissileAmmo
        {
            new_combat_mode = CombatMode::NonCombat;
        }

        let weapon = creature_equipment::get_equipped_weapon(w, this, true);

        if weapon
            .is_some_and(|g| obj(w, g).default_combat_style() == Some(CombatStyle::ThrownWeapon))
        {
            new_combat_mode = CombatMode::Missile;
        }

        shims::handle_action_change_combat_mode(w, this, new_combat_mode, false, None);
    }

    Some(item)
}

/// Returns TRUE if unwielding an item from wieldedLocation should cause the player to switch to a
/// new stance. ACE's default: `check_missile = true`.
// ACE: Player.RequiresStanceSwap
#[must_use]
pub fn requires_stance_swap(
    w: &World,
    this: ObjectGuid,
    wielded_location: EquipMask,
    check_missile: bool,
) -> bool {
    let combat_mode = shims::combat_mode(w, this);
    if combat_mode == CombatMode::NonCombat {
        return false;
    }

    if matches!(
        wielded_location,
        EquipMask::MeleeWeapon
            | EquipMask::MissileWeapon
            | EquipMask::Held
            | EquipMask::Shield
            | EquipMask::TwoHanded
    ) {
        return true;
    }

    if check_missile
        && combat_mode == CombatMode::Missile
        && wielded_location == EquipMask::MissileAmmo
    {
        return true;
    }

    false
}

// =====================================
// Helper Functions - Inventory Movement
// =====================================

/// `FindObject(objectGuid, searchLocations, out foundInContainer, out rootOwner, out
/// wasEquipped)` and its overloads.
// ACE: Player.FindObject
#[must_use]
pub fn find_object(
    w: &World,
    this: ObjectGuid,
    object_guid: ObjectGuid,
    search_locations: SearchLocations,
) -> Found {
    let mut found = Found::default();

    if object_guid == this {
        // Object we're looking for is us
        found.root_owner = Some(this);
        found.result = Some(this);
        return found;
    }

    if search_locations.has_flag(SearchLocations::MyInventory) {
        if let Some((result, found_in_container)) =
            container::get_inventory_item_with_container(w, this, object_guid)
        {
            found.found_in_container = Some(found_in_container);
            found.root_owner = Some(this);
            found.result = Some(result);
            return found;
        }
    }

    if search_locations.has_flag(SearchLocations::MyEquippedItems) {
        if let Some(result) = creature_equipment::get_equipped_item(w, this, object_guid) {
            found.root_owner = Some(this);
            found.was_equipped = true;
            found.result = Some(result);
            return found;
        }
    }

    let current_landblock = obj(w, this).current_landblock;

    if search_locations.has_flag(SearchLocations::Landblock) {
        if let Some(result) =
            current_landblock.and_then(|lb| landblock::get_object(w, lb, object_guid, true))
        {
            found.result = Some(result);
            return found;
        }
    }

    if search_locations.has_flag(SearchLocations::LastUsedContainer) {
        let last_opened_container_id = last_opened_container_id(w, this);
        let last_opened_container = current_landblock
            .and_then(|lb| landblock::get_object(w, lb, last_opened_container_id, true))
            .filter(|&g| obj(w, g).is_container());

        if let Some(last_opened_container) = last_opened_container {
            if obj(w, last_opened_container).is_vendor() {
                if let Some(result) =
                    shims::vendor_try_get_item_for_sale(w, last_opened_container, object_guid)
                {
                    found.root_owner = Some(last_opened_container);
                    found.result = Some(result);
                    return found;
                }
            }
            let c = obj(w, last_opened_container);
            if c.is_open() && c.viewer() == this.full() {
                if let Some((result, found_in_container)) =
                    container::get_inventory_item_with_container(
                        w,
                        last_opened_container,
                        object_guid,
                    )
                {
                    found.found_in_container = Some(found_in_container);
                    found.root_owner = Some(last_opened_container);
                    found.result = Some(result);
                    return found;
                }
            }
        }
    }

    if search_locations.has_flag(SearchLocations::WieldedByOther) {
        if let Some(result) =
            current_landblock.and_then(|lb| shims::landblock_get_wielded_object(w, lb, object_guid))
        {
            // `result.Wielder as Container`
            found.root_owner = obj(w, result)
                .wielder
                .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_container));
            found.result = Some(result);
            return found;
        }
    }

    if search_locations.has_flag(SearchLocations::TradedByOther) {
        let trade_partner = shims::trade_partner(w, this);
        if shims::is_trading(w, this) && trade_partner != ObjectGuid::INVALID {
            let current_trade_partner = current_landblock
                .and_then(|lb| landblock::get_object(w, lb, trade_partner, true))
                .filter(|&g| obj(w, g).is_player());
            if let Some(current_trade_partner) = current_trade_partner {
                if shims::items_in_trade_window(w, current_trade_partner).contains(&object_guid) {
                    let result = creature_equipment::get_equipped_item(
                        w,
                        current_trade_partner,
                        object_guid,
                    )
                    .or_else(|| {
                        container::get_inventory_item(w, current_trade_partner, object_guid)
                    });

                    if result.is_some() {
                        found.result = result;
                        return found;
                    }
                }
            }
        }
    }

    if search_locations.has_flag(SearchLocations::ObjectsKnownByMe) {
        if let Some(result) = shims::get_known_objects(w, this)
            .into_iter()
            .find(|&o| o == object_guid)
        {
            found.result = Some(result);
            return found;
        }
    }

    if search_locations.has_flag(SearchLocations::LastUsedHook) {
        let last_used_hook = current_landblock
            .and_then(|lb| landblock::get_object(w, lb, shims::las_used_hook_id(w, this), true))
            .filter(|&g| obj(w, g).is_hook());
        if let Some(last_used_hook) = last_used_hook {
            if let Some((result, found_in_container)) =
                container::get_inventory_item_with_container(w, last_used_hook, object_guid)
            {
                found.found_in_container = Some(found_in_container);
                found.root_owner = Some(last_used_hook);
                found.result = Some(result);
                return found;
            }
        }
    }

    found
}

/// This would be used if you need to pickup something without a MoveTo action. It will
/// broadcast the pickup motion and add a delay for the animation length.
// ACE: Player.StartPickupChain
fn start_pickup_chain(w: &mut World, this: ObjectGuid) -> ActionChain {
    shims::stop_existing_move_to_chains(w, this);

    let mut pickup_chain = ActionChain::new();

    // start picking up item animation
    let m = game_message_update_position(w, this, false);
    world_object_networking::enqueue_broadcast(w, this, true, &[m]);

    let stance = shims::current_motion_state_stance(w, this);
    let motion_pickup = dispatch::motion_pickup::motion_pickup(w, this);
    let motion = Motion::new(stance, motion_pickup, 1.0);

    world_object_networking::enqueue_broadcast_motion(w, this, &motion, None, None);

    // Wait for animation to progress
    let pickup_animation_length = shims::motion_table_get_animation_length(
        w,
        this,
        stance,
        motion_pickup,
        Some(MotionCommand::Ready),
    );
    pickup_chain.add_delay_seconds(w, f64::from(pickup_animation_length));

    pickup_chain
}

// ACE: Player.GetPickupMotion
pub fn get_pickup_motion(
    w: &World,
    this: ObjectGuid,
    object_were_reaching_toward: ObjectGuid,
) -> MotionCommand {
    let Some(target_location) = w
        .objects
        .get(object_were_reaching_toward)
        .and_then(WorldObject::location)
    else {
        return MotionCommand::Invalid;
    };

    // hack for jump looting --

    // in retail, this bug was a result of actions running on motion callbacks,
    // when the motions exited the animation queue

    // since a 'crouch down' motion cannot be performed while jumping,
    // this crouch down motion exited the animation queue immediately

    // here we are just skipping the animation if the player is jumping
    if shims::is_jumping(w, this) && shims::property_manager_get_bool(w, "allow_jump_loot") {
        return MotionCommand::Invalid;
    }

    let mut item_location_z = target_location.position_z;

    if !obj(w, object_were_reaching_toward).is_corpse() {
        item_location_z += shims::height(w, object_were_reaching_toward) * 0.5f32;
    }

    // `item_location_z >= Location.PositionZ + (Height * 0.90)`: the right side is a double.
    let location_z = f64::from(
        obj(w, this)
            .location()
            .expect("System.NullReferenceException: Location")
            .position_z,
    );
    let height = f64::from(shims::height(w, this));
    let item_location_z = f64::from(item_location_z);

    if item_location_z >= location_z + (height * 0.90) {
        MotionCommand::Pickup20 // Reach up
    } else if item_location_z >= location_z + (height * 0.70) {
        MotionCommand::Pickup15 // Reach over and up just a little bit
    } else if item_location_z >= location_z + (height * 0.50) {
        MotionCommand::Pickup10 // Reach over and down just a little bit
    } else if item_location_z >= location_z + (height * 0.20) {
        MotionCommand::Pickup5 // Bend down a little bit
    } else {
        MotionCommand::Pickup // At foot height or lower
    }
}

/// This would be used if your pickup action first requires a MoveTo action. It will add a chain
/// to broadcast the pickup motion and then add a delay for the animation length.
// ACE: Player.AddPickupChainToMoveToChain
fn add_pickup_chain_to_move_to_chain(
    w: &mut World,
    this: ObjectGuid,
    pickup_motion: MotionCommand,
) -> ActionChain {
    if pickup_motion == MotionCommand::Invalid {
        return ActionChain::new();
    }

    // start picking up item animation
    let m = game_message_update_position(w, this, false);
    world_object_networking::enqueue_broadcast(w, this, true, &[m]);

    let stance = shims::current_motion_state_stance(w, this);
    let motion = Motion::new(stance, pickup_motion, 1.0);

    world_object_networking::enqueue_broadcast_motion(w, this, &motion, None, None);

    // Wait for animation to progress
    let pickup_animation_length = shims::motion_table_get_animation_length(
        w,
        this,
        stance,
        pickup_motion,
        Some(MotionCommand::Ready),
    );

    let mut pickup_chain = ActionChain::new();
    pickup_chain.add_delay_seconds(w, f64::from(pickup_animation_length));

    pickup_chain
}

/// If you want to subtract from a stack, amount should be negative.
// ACE: Player.AdjustStack
pub fn adjust_stack(
    w: &mut World,
    this: ObjectGuid,
    stack: ObjectGuid,
    amount: i32,
    container: Option<ObjectGuid>,
    root_container: Option<ObjectGuid>,
) -> bool {
    let (stack_size, max_stack_size) = {
        let s = obj(w, stack);
        (s.stack_size(), s.max_stack_size())
    };
    let new_size = add_nullable(stack_size, amount);
    // Lifted comparisons: a null operand compares false.
    if new_size.is_some_and(|n| n <= 0)
        || new_size
            .zip(max_stack_size)
            .is_some_and(|(n, m)| n > i32::from(m))
    {
        log::warn!(
            "Player 0x{:08X}:{} tried to adjust stack by an invalid amount amount ({amount}) 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack)
        );
        return false;
    }

    let (unit_encumbrance, unit_value) = {
        let s = obj_mut(w, stack);
        s.set_stack_size_prop(new_size);
        let size = s.stack_size().unwrap_or(1);
        s.set_encumbrance_val(Some(
            s.stack_unit_encumbrance().unwrap_or(0).wrapping_mul(size),
        ));
        s.set_value(Some(s.stack_unit_value().unwrap_or(0).wrapping_mul(size)));
        (
            s.stack_unit_encumbrance().unwrap_or(0),
            s.stack_unit_value().unwrap_or(0),
        )
    };

    if let Some(container) = container {
        // We add to these values because amount will be negative if we're subtracting from a stack, so we want to add a negative number.
        let c = obj_mut(w, container);
        let e = add_nullable(c.encumbrance_val(), unit_encumbrance.wrapping_mul(amount));
        c.set_encumbrance_val(e);
        let v = add_nullable(c.value(), unit_value.wrapping_mul(amount));
        c.set_value(v);
    }

    if let Some(root_container) = root_container.filter(|&r| Some(r) != container) {
        let c = obj_mut(w, root_container);
        let e = add_nullable(c.encumbrance_val(), unit_encumbrance.wrapping_mul(amount));
        c.set_encumbrance_val(e);
        let v = add_nullable(c.value(), unit_value.wrapping_mul(amount));
        c.set_value(v);
    }

    true
}

// ACE: Player.StartPickup
pub fn start_pickup(w: &mut World, this: ObjectGuid) {
    fields_mut(w, this).pickup_state = PickupState::Start;
    shims::set_is_busy(w, this, true);
}

// ACE: Player.EnqueuePickupDone
pub fn enqueue_pickup_done(w: &mut World, this: ObjectGuid, pickup_motion: MotionCommand) {
    fields_mut(w, this).pickup_state = PickupState::Return;

    let stance = shims::current_motion_state_stance(w, this);
    let return_stance = Motion::from_stance(stance);
    world_object_networking::enqueue_broadcast_motion(w, this, &return_stance, None, None);

    let anim_time = shims::motion_table_get_animation_length_of(w, this, pickup_motion);

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(anim_time));
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        if !w.objects.contains(this) {
            return;
        }
        fields_mut(w, this).pickup_state = PickupState::None;
        shims::set_is_busy(w, this, false);

        if let Some(next_pickup) = fields_mut(w, this).next_pickup.take() {
            // `NextPickup(); NextPickup = null;` (taken first: it can only be set again while busy)
            next_pickup(w);
        }
    });
    action_chain.enqueue_chain(w);
}

/// The objects `HandleActionPutItemInContainer_Verify` found (its `out` parameters).
#[derive(Debug, Clone, Copy)]
struct PutVerified {
    item_root_owner: Option<ObjectGuid>,
    item: ObjectGuid,
    container_root_owner: Option<ObjectGuid>,
    container: ObjectGuid,
    item_was_equipped: bool,
}

// ACE: Player.HandleActionPutItemInContainer_Verify
#[allow(clippy::too_many_lines)]
fn handle_action_put_item_in_container_verify(
    w: &mut World,
    this: ObjectGuid,
    item_guid: u32,
    container_guid: u32,
    placement: i32,
) -> Option<PutVerified> {
    if shims::suicide_in_progress(w, this) {
        weenie_error(w, this, WeenieError::YoureTooBusy);
        save_failed(w, this, item_guid, WeenieError::None);
        return None;
    }

    if shims::is_busy(w, this) {
        let f = fields(w, this);
        if f.pickup_state != PickupState::Return || f.next_pickup.is_some() {
            weenie_error(w, this, WeenieError::YoureTooBusy);
            save_failed(w, this, item_guid, WeenieError::None);
        } else {
            fields_mut(w, this).next_pickup = Some(Box::new(move |w: &mut World| {
                handle_action_put_item_in_container(w, this, item_guid, container_guid, placement);
            }));
        }

        return None;
    }

    //OnPutItemInContainer(itemGuid, containerGuid, placement);

    let item_found = find_object(
        w,
        this,
        ObjectGuid::new(item_guid),
        SearchLocations::LocationsICanMove,
    );
    let container_found = find_object(
        w,
        this,
        ObjectGuid::new(container_guid),
        SearchLocations::MyInventory
            | SearchLocations::Landblock
            | SearchLocations::LastUsedContainer,
    );
    let item_root_owner = item_found.root_owner;
    let item_was_equipped = item_found.was_equipped;
    let container = container_found.result.filter(|&g| obj(w, g).is_container());
    let container_root_owner = container_found.root_owner;

    let Some(item) = item_found.result else {
        transient(w, this, "Source item not found!"); // Custom error message
        save_failed(w, this, item_guid, WeenieError::None);
        return None;
    };

    if !item.is_dynamic() || obj(w, item).is_creature() || obj(w, item).stuck() {
        log::warn!(
            "Player 0x{:08X}:{} tried to move item 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            item.full(),
            name(w, item)
        );
        weenie_error(w, this, WeenieError::Stuck);
        save_failed(w, this, item_guid, WeenieError::None);
        return None;
    }

    if item_root_owner != Some(this)
        && container_root_owner == Some(this)
        && !has_enough_burden_to_add_to_inventory(w, this, item)
    {
        transient(w, this, "You are too encumbered to carry that!");
        save_failed(w, this, item_guid, WeenieError::None);
        return None;
    }

    let Some(container) = container else {
        transient(w, this, "Target container not found!"); // Custom error message
        save_failed(w, this, item_guid, WeenieError::None);
        return None;
    };

    if obj(w, container).is_corpse() {
        let message = format!("You cannot put {} in that.", name(w, item));
        transient(w, this, &message); // Custom error message
        save_failed(w, this, item_guid, WeenieError::None);
        return None;
    }

    if shims::is_trading(w, this) && is_being_traded(w, this, item) {
        save_failed(w, this, item_guid, WeenieError::TradeItemBeingTraded);
        return None;
    }

    if container_root_owner != Some(this) {
        // Is our target on the landscape?
        if item_root_owner == Some(this)
            && dispatch::is_attuned_or_contains_attuned::is_attuned_or_contains_attuned(w, item)
        {
            save_failed(w, this, item_guid, WeenieError::AttunedItem);
            return None;
        }

        if item_root_owner == Some(this)
            && obj(w, item).is_pet_device()
            && shims::pet_device_pet(w, item).is_some()
        {
            transient(
                w,
                this,
                "You must unsummon your pet before you can transfer this item!",
            );
            save_failed(w, this, item_guid, WeenieError::None);
            return None;
        }

        if container_root_owner.is_some_and(|r| !obj(w, r).is_open()) {
            save_failed(w, this, item_guid, WeenieError::TheContainerIsClosed);
            return None;
        }
    }

    if let Some(corpse) = container_root_owner.filter(|&r| obj(w, r).is_corpse()) {
        if !shims::corpse_is_monster(w, corpse) {
            save_failed(w, this, item_guid, WeenieError::Dead);
            return None;
        }
    }

    if obj(w, container).is_hook() {
        let hook = container;
        if shims::property_manager_get_bool(w, "house_hook_limit") {
            let (max, current) = (
                shims::house_max_hooks_usable(w, hook),
                shims::house_current_hooks_usable(w, hook),
            );
            if max != -1 && current <= 0 {
                save_failed(w, this, item_guid, WeenieError::YouHaveUsedAllTheHooks);
                return None;
            }
        }

        if shims::property_manager_get_bool(w, "house_hookgroup_limit") {
            let item_hook_group = obj(w, item).hook_group().unwrap_or(HookGroupType::Undef);
            let house_hook_group_max =
                shims::house_get_hook_group_max_count(w, hook, item_hook_group);
            let house_hook_group_current =
                shims::house_get_hook_group_current_count(w, hook, item_hook_group);
            if house_hook_group_max != -1 && house_hook_group_current >= house_hook_group_max {
                save_failed(w, this, item_guid, WeenieError::None);
                crate::world_objects::player_networking::send_weenie_error_with_string(
                    w,
                    this,
                    WeenieErrorWithString::MaxNumberOf_Hooked,
                    &shims::hook_group_to_sentence(item_hook_group),
                );
                return None;
            }
        }
    }

    if obj(w, item).is_container() {
        // Blocking all attempts to put containers in things that aren't Players and Storage. This may not be retail, but at this time appears to be best catch all solution to Quest stamp bypass issue.
        let c = obj(w, container);
        if !c.is_player() && !c.is_storage() {
            //Session.Network.EnqueueSend(new GameEventCommunicationTransientString(Session, $"You cannot put {item.Name} in that.")); // Custom error message
            save_failed(w, this, item_guid, WeenieError::None);
            return None;
        }
    }

    if container_root_owner.is_none() {
        // container is on landscape, so you must have it open
        let c = obj(w, container);
        if !c.is_open() || c.viewer() != this.full() {
            save_failed(w, this, item_guid, WeenieError::TheContainerIsClosed);
            return None;
        }
    }

    Some(PutVerified {
        item_root_owner,
        item,
        container_root_owner,
        container,
        item_was_equipped,
    })
}

/// `item.IsBeingTradedOrContainsItemBeingTraded(ItemsInTradeWindow)`.
fn is_being_traded(w: &World, this: ObjectGuid, item: ObjectGuid) -> bool {
    let items_in_trade_window = shims::items_in_trade_window(w, this);
    dispatch::is_being_traded_or_contains_item_being_traded::is_being_traded_or_contains_item_being_traded(w, item, &items_in_trade_window)
}

// =========================================
// Game Action Handlers - Inventory Movement
// =========================================

/// This method processes the Game Action (F7B1) Put Item In Container (0x0019). This is raised
/// when we:
/// - move an item around in our inventory.
/// - dequip an item.
/// - Pickup an item off of the landblock or a container on the landblock
/// - Put an item into a container on the landblock
/// - Move an item between containers on a landblock
///
/// ACE's default: `placement = 0`.
// ACE: Player.HandleActionPutItemInContainer
pub fn handle_action_put_item_in_container(
    w: &mut World,
    this: ObjectGuid,
    item_guid: u32,
    container_guid: u32,
    placement: i32,
) {
    //Console.WriteLine($"{Name}.HandleActionPutItemInContainer({itemGuid:X8}, {containerGuid:X8}, {placement})");

    let Some(v) =
        handle_action_put_item_in_container_verify(w, this, item_guid, container_guid, placement)
    else {
        return;
    };
    let PutVerified {
        item_root_owner,
        item,
        container_root_owner,
        container,
        ..
    } = v;

    if (item_root_owner == Some(this) && container_root_owner != Some(this))
        || (item_root_owner != Some(this) && container_root_owner == Some(this))
    {
        // Movement is between the player and the world
        if item_root_owner.is_some_and(|r| obj(w, r).is_vendor()) {
            weenie_error(w, this, WeenieError::NotAllTheItemsAreAvailable);
            save_failed(w, this, item_guid, WeenieError::None);
            return;
        }

        let item_as_container = Some(item).filter(|&g| obj(w, g).is_container());

        // Checking to see if item to pick is an container itself and IsOpen
        if item_as_container.is_some()
            && !verify_container_open_status(w, this, item_as_container, item)
        {
            return;
        }

        let move_to_target = if item_root_owner == Some(this) {
            container_root_owner.unwrap_or(container) // Movement is from player
        } else {
            item_root_owner.unwrap_or(item) // Movement is too player
        };

        shims::create_pickup_move_to_chain(
            w,
            this,
            move_to_target,
            Box::new(move |w: &mut World, success: bool| {
                if current_landblock(w, this).is_none() {
                    // Maybe we were teleported as we were motioning to pick up the item
                    save_failed(w, this, item_guid, WeenieError::ActionCancelled);
                    return;
                }

                if !success {
                    save_failed(w, this, item_guid, WeenieError::ActionCancelled);
                    return;
                }

                // Was this item picked up by someone else?
                if item_root_owner.is_none() && current_landblock(w, item).is_none() {
                    save_failed(w, this, item_guid, WeenieError::ActionCancelled);
                    return;
                }

                if item_root_owner != Some(this)
                    && container_root_owner == Some(this)
                    && dispatch::is_unique_or_contains_unique::is_unique_or_contains_unique(w, item)
                    && !check_uniques(w, this, &[item], None)
                {
                    // moving from world container to player
                    save_failed(w, this, item_guid, WeenieError::None);
                    return;
                }

                start_pickup(w, this);

                let pickup_motion = get_pickup_motion(w, this, move_to_target);
                let mut pickup_chain = add_pickup_chain_to_move_to_chain(w, this, pickup_motion);

                pickup_chain.add_action(Actor::Object(this), move |w: &mut World| {
                    put_item_in_container_pickup_done(
                        w,
                        this,
                        v,
                        item_as_container,
                        placement,
                        item_guid,
                        pickup_motion,
                    );
                });

                pickup_chain.enqueue_chain(w);
            }),
            None,
            false, // if player is within UseRadius of moveToTarget, do not perform rotation
        );
    } else {
        // This is a self-contained movement
        let _container_root_owner = container_root_owner.unwrap_or(container);

        if _container_root_owner != this && Some(_container_root_owner) != item_root_owner {
            // this *should* be a self-contained movement..
            // duplicated check/message from client
            save_failed(w, this, item_guid, WeenieError::None);
            let message = format!("You must first pick up the {}", name(w, item));
            shims::send_transient_error(w, this, &message);
            return;
        }

        let wielded_location = obj(w, item)
            .current_wielded_location()
            .unwrap_or(EquipMask::None);

        // note that special sequence for swapping arrows while in missile combat
        // is not handled here, but is still handled downstream
        if requires_stance_swap(w, this, wielded_location, false) {
            shims::handle_action_change_combat_mode(
                w,
                this,
                CombatMode::Melee,
                true,
                Some(Box::new(move |w: &mut World| {
                    let Some(v) = handle_action_put_item_in_container_verify(
                        w,
                        this,
                        item_guid,
                        container_guid,
                        placement,
                    ) else {
                        return;
                    };
                    do_handle_action_put_item_in_container(w, this, v, placement);
                })),
            );
        } else {
            do_handle_action_put_item_in_container(w, this, v, placement);
        }
    }
}

/// The pickup chain's last action in `HandleActionPutItemInContainer`.
#[allow(clippy::too_many_lines)]
fn put_item_in_container_pickup_done(
    w: &mut World,
    this: ObjectGuid,
    v: PutVerified,
    item_as_container: Option<ObjectGuid>,
    placement: i32,
    item_guid: u32,
    pickup_motion: MotionCommand,
) {
    let PutVerified {
        item_root_owner,
        item,
        container_root_owner,
        container,
        ..
    } = v;

    // Was this item picked up by someone else?
    if item_root_owner.is_none() && current_landblock(w, item).is_none() {
        save_failed(w, this, item_guid, WeenieError::ActionCancelled);
        enqueue_pickup_done(w, this, pickup_motion);
        return;
    }

    // Checking to see if item to pick is an container itself and IsOpen
    if !verify_container_open_status(w, this, item_as_container, item) {
        enqueue_pickup_done(w, this, pickup_motion);
        return;
    }

    let Some((quest_solve, is_from_a_player_corpse)) = verify_quest(w, this, item, item_root_owner)
    else {
        // InventoryServerSaveFailed previously sent in QuestManager
        enqueue_pickup_done(w, this, pickup_motion);
        return;
    };

    if do_handle_action_put_item_in_container(w, this, v, placement) {
        send_encumbrance(w, this);

        if is_coin_or_container(w, item) {
            shims::update_coin_value(w, this);
        }

        if item_root_owner == Some(this) {
            shims::emote_manager_on_drop(w, item, this);
            broadcast_sound(w, this, Sound::DropItem);
        } else if container_root_owner == Some(this) {
            if let Some(item_as_container) = item_as_container {
                // We're picking up a pack
                if let Some(s) = session(w, this) {
                    let m = game_event_view_contents(w, s, item_as_container);
                    send(w, this, m);
                }

                for pack_item in container::inventory_values(w, item_as_container) {
                    let m = game_message_create_object(w, pack_item, false, false);
                    send(w, this, m);
                }
            }

            broadcast_sound(w, this, Sound::PickUpItem);

            shims::emote_manager_on_pickup(w, item, this);
            crate::world_objects::world_object_generators::notify_of_event(
                w,
                item,
                RegenerationType::PickUp,
            );

            if quest_solve {
                shims::emote_manager_on_quest(w, item, this);
            }

            if is_from_a_player_corpse {
                let root = item_root_owner.expect("a corpse root owner");
                log::info!(
                    "[CORPSE] {} (0x{}) picked up {} (0x{}) from {} (0x{})",
                    name(w, this),
                    this,
                    name(w, item),
                    item,
                    name(w, root),
                    root
                );
                dispatch::save_biota_to_database::save_biota_to_database(w, item, true);
            }
        }

        if shims::property_manager_get_bool(w, "house_hook_limit") {
            let to_hook = Some(container).filter(|&c| obj(w, c).is_hook());
            let from_hook = item_root_owner.filter(|&r| obj(w, r).is_hook());
            if to_hook.is_some_and(|h| {
                shims::house_max_hooks_usable(w, h) != -1
                    && shims::house_current_hooks_usable(w, h) <= 0
            }) {
                crate::world_objects::player_networking::send_weenie_error(
                    w,
                    this,
                    WeenieError::YouAreNowUsingMaxHooks,
                );
            } else if from_hook.is_some_and(|h| {
                shims::house_max_hooks_usable(w, h) != -1
                    && shims::house_current_hooks_usable(w, h) == 1
            }) {
                crate::world_objects::player_networking::send_weenie_error(
                    w,
                    this,
                    WeenieError::YouAreNoLongerUsingMaxHooks,
                );
            }
        }

        if shims::property_manager_get_bool(w, "house_hookgroup_limit") {
            if obj(w, container).is_hook() {
                let to_hook = container;
                let item_hook_group = obj(w, item).hook_group().unwrap_or(HookGroupType::Undef);
                let house_hook_group_max =
                    shims::house_get_hook_group_max_count(w, to_hook, item_hook_group);
                let house_hook_group_current =
                    shims::house_get_hook_group_current_count(w, to_hook, item_hook_group);

                if house_hook_group_max != -1 && house_hook_group_current >= house_hook_group_max {
                    crate::world_objects::player_networking::send_weenie_error_with_string(
                        w,
                        this,
                        WeenieErrorWithString::MaxNumberOf_HookedUntilOneIsRemoved,
                        &shims::hook_group_to_sentence(item_hook_group),
                    );
                }
            } else if let Some(from_hook) = item_root_owner.filter(|&r| obj(w, r).is_hook()) {
                let item_hook_group = obj(w, item).hook_group().unwrap_or(HookGroupType::Undef);
                let house_hook_group_max =
                    shims::house_get_hook_group_max_count(w, from_hook, item_hook_group);
                let house_hook_group_current =
                    shims::house_get_hook_group_current_count(w, from_hook, item_hook_group);

                if house_hook_group_max != -1
                    && house_hook_group_current == house_hook_group_max.wrapping_sub(1)
                {
                    crate::world_objects::player_networking::send_weenie_error_with_string(
                        w,
                        this,
                        WeenieErrorWithString::NoLongerMaxNumberOf_Hooked,
                        &shims::hook_group_to_sentence(item_hook_group),
                    );
                }
            }
        }
    }
    enqueue_pickup_done(w, this, pickup_motion);
}

// ACE: Player.VerifyContainerOpenStatus
fn verify_container_open_status(
    w: &mut World,
    this: ObjectGuid,
    item_as_container: Option<ObjectGuid>,
    item: ObjectGuid,
) -> bool {
    // Checking to see if item to pick is an container itself and IsOpen
    if let Some(item_as_container) = item_as_container.filter(|&c| obj(w, c).is_open()) {
        if obj(w, item_as_container).viewer() == this.full() {
            // We're the one that has it open. Close it before picking it up
            dispatch::close::close(w, item_as_container, this);
        } else {
            // We're not who has it open. Can't pick up something someone else is viewing!
            let item_name = name(w, item);
            weenie_error_with_string(
                w,
                this,
                WeenieErrorWithString::The_IsCurrentlyInUse,
                &item_name,
            );
            save_failed(w, this, item.full(), WeenieError::None);
            return false;
        }
    }
    true
}

/// Returns `Some((questSolve, isFromAPlayerCorpse))` where ACE returns true.
// ACE: Player.VerifyQuest
fn verify_quest(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
    item_root_owner: Option<ObjectGuid>,
) -> Option<(bool, bool)> {
    let mut quest_solve = false;
    let mut is_from_a_player_corpse = false;

    if let Some(quest_restriction) = obj(w, item).quest_restriction() {
        if !shims::quest_manager_has_quest(w, this, &quest_restriction) {
            shims::quest_manager_handle_no_quest_error(w, this, item);
            return None;
        }
    }

    let item_found_on_corpse = item_root_owner.is_some_and(|r| obj(w, r).is_corpse());

    if item_found_on_corpse
        && item_root_owner
            .and_then(|r| obj(w, r).level())
            .is_some_and(|l| l > 0)
    {
        is_from_a_player_corpse = true;
    }

    if let Some(quest) = obj(w, item).quest() {
        // We're picking up an item with a quest stamp that can also be a timer/limiter
        let item_found_on_my_corpse = item_found_on_corpse
            && item_root_owner.and_then(|r| obj(w, r).victim_id()) == Some(this.full());
        if obj(w, item).generator_id().is_some()
            || (item_found_on_corpse && !item_found_on_my_corpse)
        {
            // item is controlled by a generator or is on a corpse that is not my own
            if shims::quest_manager_can_solve(w, this, &quest) {
                quest_solve = true;
            } else {
                shims::quest_manager_handle_solve_error(w, this, &quest);
                return None;
            }
        }
    }
    Some((quest_solve, is_from_a_player_corpse))
}

// ACE: Player.DoHandleActionPutItemInContainer
fn do_handle_action_put_item_in_container(
    w: &mut World,
    this: ObjectGuid,
    v: PutVerified,
    placement: i32,
) -> bool {
    let PutVerified {
        item_root_owner,
        item,
        container_root_owner,
        container,
        item_was_equipped,
    } = v;
    //Console.WriteLine($"-> DoHandleActionPutItemInContainer({item.Name}, {itemRootOwner?.Name}, {itemWasEquipped}, {container?.Name}, {containerRootOwner?.Name}, {placement})");

    let mut prev_location: Option<Position> = None;
    let mut prev_landblock = None;

    let prev_container = obj(w, item).wo.world_object_properties.container;

    on_put_item_in_container(w, this, item.full(), container.full(), placement);

    if let Some(item_landblock) = current_landblock(w, item) {
        // Movement is an item pickup off the landblock
        prev_location = obj(w, item).location();
        prev_landblock = Some(item_landblock);

        landblock::remove_world_object(w, item_landblock, item, false, true, true);
        obj_mut(w, item).set_location(None);
    } else if item_was_equipped {
        // Movement is an equipped item to a container on the landblock
        let dequip_object_action = if container_root_owner == Some(this) {
            DequipObjectAction::DequipToPack
        } else {
            DequipObjectAction::DequipToOffPlayerContainer
        };

        if try_dequip_object_with_networking(w, this, item, dequip_object_action).is_none() {
            transient(w, this, "TryDequipObjectWithNetworking failed!"); // Custom error message
            save_failed(w, this, item.full(), WeenieError::None);
            return false;
        }
    } else {
        // Movement is within the same pack or between packs in a container on the landblock
        let item_root_creature = item_root_owner.filter(|&r| obj(w, r).is_creature());

        if let Some(item_root_owner) = item_root_owner {
            if !container::try_remove_from_inventory(w, item_root_owner, item, false)
                && item_root_creature
                    .is_none_or(|c| creature_equipment::try_dequip_object(w, c, item).is_none())
            {
                transient(w, this, "TryRemoveFromInventory failed!"); // Custom error message
                save_failed(w, this, item.full(), WeenieError::None);
            }
        }

        if item_root_owner == Some(this) && container_root_owner != Some(this) {
            // We must update the database with the latest ContainerId and WielderId properties.
            // If we don't, the player can drop the item, log out, and log back in. If the landblock hasn't queued a database save in that time,
            // the player will end up loading with this object in their inventory even though the landblock is the true owner. This is because
            // when we load player inventory, the database still has the record that shows this player as the ContainerId for the item.
            deep_save(w, item);
        }
    }

    let burden_check = item_root_owner != Some(this) && container_root_owner == Some(this);

    if !container::try_add_to_inventory(w, container, item, placement, true, burden_check) {
        let message = format!("Unable to put {} into container", name(w, item));
        transient(w, this, &message); // Custom error message
        save_failed(w, this, item.full(), WeenieError::None);

        if let (Some(prev_location), Some(prev_landblock)) = (prev_location, prev_landblock) {
            let mut landblock_return = ActionChain::new();

            landblock_return.add_delay_seconds(w, 1.0);
            landblock_return.add_action(Actor::Landblock(prev_landblock), move |w: &mut World| {
                shims::remove_tracked_object(w, this, item, false)
            });
            landblock_return.add_delay_seconds(w, 1.0);
            landblock_return.add_action(Actor::Landblock(prev_landblock), move |w: &mut World| {
                if let Some(o) = w.objects.get_mut(item) {
                    o.set_location(Some(Position::from_position(&prev_location)));
                }
                landblock_manager::add_object(w, item, false);
            });
            landblock_return.enqueue_chain(w);
        } else if item_root_owner
            .is_none_or(|r| !container::try_add_to_inventory(w, r, item, 0, false, true))
        {
            log::error!(
                "{}.DoHandleActionPutItemInContainer({} ({}), {:?} ({:?}), {item_was_equipped}, {} ({}), {:?} ({:?}), {placement}) - removed item from original location, failed to add to new container, failed to re-add to original location",
                name(w, this),
                name(w, item),
                item,
                item_root_owner.map(|r| name(w, r)),
                item_root_owner,
                name(w, container),
                container,
                container_root_owner.map(|r| name(w, r)),
                container_root_owner
            );
        }

        return false;
    }

    if let Some(container_root_owner) = container_root_owner.filter(|&r| r != container) {
        let (encumbrance, value) = (
            obj(w, item).encumbrance_val().unwrap_or(0),
            obj(w, item).value().unwrap_or(0),
        );
        let r = obj_mut(w, container_root_owner);
        let e = add_nullable(r.encumbrance_val(), encumbrance);
        r.set_encumbrance_val(e);
        let v = add_nullable(r.value(), value);
        r.set_value(v);
    }

    // when moving from a non-stuck container to a different container,
    // the database must be synced immediately
    if let Some(prev_container) = prev_container {
        if !w
            .objects
            .get(prev_container)
            .is_some_and(WorldObject::stuck)
            && container != prev_container
        {
            dispatch::save_biota_to_database::save_biota_to_database(w, item, true);
        }
    }

    let m1 = public_update_instance_id(w, item, PropertyInstanceId::Container, container);
    let m2 = contain_id(w, this, item, container);
    send_many(w, this, std::iter::once(m1).chain(m2).collect());

    true
}

/// This method processes the Game Action (F7B1) Drop Item (0x001B). This is raised when we:
/// - drop an equipped item
/// - drop an item from inventory
// ACE: Player.HandleActionDropItem
pub fn handle_action_drop_item(w: &mut World, this: ObjectGuid, item_guid: u32) {
    if shims::is_busy(w, this) || shims::teleporting(w, this) || shims::suicide_in_progress(w, this)
    {
        weenie_error(w, this, WeenieError::YoureTooBusy);
        save_failed(w, this, item_guid, WeenieError::None);
        return;
    }

    let found = find_object(
        w,
        this,
        ObjectGuid::new(item_guid),
        SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
    );
    let was_equipped = found.was_equipped;

    let Some(item) = found.result else {
        transient(w, this, "Item not found!"); // Custom error message
        save_failed(w, this, item_guid, WeenieError::None);
        return;
    };

    if dispatch::is_attuned_or_contains_attuned::is_attuned_or_contains_attuned(w, item) {
        save_failed(w, this, item_guid, WeenieError::AttunedItem);
        return;
    }

    if obj(w, item).is_pet_device() && shims::pet_device_pet(w, item).is_some() {
        transient(
            w,
            this,
            "You must unsummon your pet before you can drop this item!",
        );
        save_failed(w, this, item_guid, WeenieError::None);
        return;
    }

    if shims::is_trading(w, this) && is_being_traded(w, this, item) {
        save_failed(w, this, item.full(), WeenieError::TradeItemBeingTraded);
        return;
    }

    let mut action_chain = start_pickup_chain(w, this);

    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        if current_landblock(w, this).is_none() {
            // Maybe we were teleported as we were motioning to drop the item
            save_failed(w, this, item.full(), WeenieError::ActionCancelled);
            return;
        }

        if was_equipped {
            if try_dequip_object_with_networking(w, this, item, DequipObjectAction::DropItem)
                .is_none()
            {
                transient(w, this, "Failed to dequip item!"); // Custom error message
                save_failed(w, this, item.full(), WeenieError::None);
                return;
            }
        } else if try_remove_from_inventory_with_networking(
            w,
            this,
            item,
            RemoveFromInventoryAction::DropItem,
        )
        .is_none()
        {
            transient(w, this, "Failed to remove item from inventory!"); // Custom error message
            save_failed(w, this, item.full(), WeenieError::None);
            return;
        }

        if try_drop_item(w, this, item) {
            // drop success
            let m1 = public_update_instance_id(
                w,
                item,
                PropertyInstanceId::Container,
                ObjectGuid::INVALID,
            );
            let m2 = event(w, this, |d| game_event_item_server_says_move_item(d, item));
            let m3 = game_message_update_position(w, item, false);
            send_many(
                w,
                this,
                [Some(m1), m2, Some(m3)].into_iter().flatten().collect(),
            );

            broadcast_sound(w, this, Sound::DropItem);

            shims::emote_manager_on_drop(w, item, this);
        } else {
            // drop failed, re-add to inventory
            if container::try_add_to_inventory(w, this, item, 0, false, true) {
                send_encumbrance(w, this);

                if is_coin_or_container(w, item) {
                    shims::update_coin_value(w, this);
                }

                save_failed(w, this, item.full(), WeenieError::None);
            } else {
                log::warn!(
                    "0x{}:{} for player {} lost from HandleActionDropItem failure.",
                    item,
                    name(w, item),
                    name(w, this)
                );
            }
        }

        let stance = shims::current_motion_state_stance(w, this);
        let return_stance = Motion::from_stance(stance);
        world_object_networking::enqueue_broadcast_motion(w, this, &return_stance, None, None);
    });

    action_chain.enqueue_chain(w);
}

// ACE: Player.TryDropItem
fn try_drop_item(w: &mut World, this: ObjectGuid, item: ObjectGuid) -> bool {
    let location = obj(w, this)
        .location()
        .expect("System.NullReferenceException: Location");
    {
        let o = obj_mut(w, item);
        o.set_location(Some(Position::from_position(&location)));
        o.set_placement(Some(Placement::Resting)); // This is needed to make items lay flat on the ground.
    }

    // increased precision for non-ethereal objects
    let ethereal = obj(w, item).ethereal();
    phys_ext::set_physics_property_state(
        w,
        item,
        PropertyBool::Ethereal,
        PhysicsState::Ethereal,
        Some(true),
    );

    let current_landblock = obj(w, this)
        .current_landblock
        .expect("System.NullReferenceException: CurrentLandblock");
    if !landblock::add_world_object(w, current_landblock, item) {
        return false;
    }

    // use radius?
    let mut target_pos = location.in_front_of(f64::from(1.1f32), false);
    let cell = crate::entity::position_extensions::get_cell(w, &target_pos);
    target_pos.set_landblock_id(empyrean_entity::LandblockId::new(cell));

    // try slide to new position
    shims::physics_obj_slide_to(w, item, &target_pos);

    phys_ext::set_physics_property_state(
        w,
        item,
        PropertyBool::Ethereal,
        PhysicsState::Ethereal,
        ethereal,
    );

    if obj(w, item).ethereal().is_none() {
        let default_physics_state = PhysicsState(
            obj(w, item)
                .get_property(PropertyInt::PhysicsState)
                .unwrap_or(0),
        );

        let value = default_physics_state.contains(PhysicsState::Ethereal);
        phys_ext::set_physics_property_state(
            w,
            item,
            PropertyBool::Ethereal,
            PhysicsState::Ethereal,
            Some(value),
        );
    }

    world_object::enqueue_broadcast_physics_state(w, item);

    true
}

/// This method processes the Game Action (F7B1) Get And Wield Item (0x001A). This is raised when
/// we:
/// - try to wield an item in from inventory
/// - try to wield an item in a chest
/// - try to wield an item on the landscape
/// - try to transfer a wielded item to another wield location
// ACE: Player.HandleActionGetAndWieldItem
pub fn handle_action_get_and_wield_item(
    w: &mut World,
    this: ObjectGuid,
    item_guid: u32,
    wielded_location: EquipMask,
) {
    //Console.WriteLine($"{Name}.HandleActionGetAndWieldItem({itemGuid:X8}, {wieldedLocation})");

    // todo fix this, it seems IsAnimating is always true for a player
    // todo we need to know when a player is busy to avoid additional actions during that time
    /*if (IsAnimating)
    {
        Session.Network.EnqueueSend(new GameEventInventoryServerSaveFailed(Session, WeenieError.YoureTooBusy));
        return;
    }*/

    let found = find_object(
        w,
        this,
        ObjectGuid::new(item_guid),
        SearchLocations::LocationsICanMove,
    );
    let (from_container, root_owner, was_equipped) = (
        found.found_in_container,
        found.root_owner,
        found.was_equipped,
    );

    let Some(item) = found.result else {
        transient(w, this, "Item not found!"); // Custom error message
        save_failed(w, this, item_guid, WeenieError::None);
        return;
    };

    if !item.is_dynamic() || obj(w, item).is_creature() || obj(w, item).stuck() {
        log::warn!(
            "Player 0x{:08X}:{} tried to move item 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            item.full(),
            name(w, item)
        );
        weenie_error(w, this, WeenieError::Stuck);
        save_failed(w, this, item_guid, WeenieError::None);
        return;
    }

    if root_owner != Some(this) && !has_enough_burden_to_add_to_inventory(w, this, item) {
        transient(w, this, "You are too encumbered to carry that!");
        save_failed(w, this, item_guid, WeenieError::None);
        return;
    }

    let valid_locations = obj(w, item).valid_locations();
    if valid_locations.is_none_or(|v| v == EquipMask::None) {
        log::warn!(
            "Player 0x{:08X}:{} tried to wield item 0x{:08X}:{} to {wielded_location:?} (0x{:X}), not in item's validlocatiions {:?} (0x{:X}).",
            this.full(),
            name(w, this),
            item.full(),
            name(w, item),
            wielded_location.0,
            valid_locations.unwrap_or(EquipMask::None),
            valid_locations.unwrap_or(EquipMask::None).0
        );
        weenie_error(w, this, WeenieError::InvalidInventoryLocation);
        save_failed(w, this, item_guid, WeenieError::None);
        return;
    }

    if root_owner != Some(this) {
        // Item is on the landscape, or in a landblock chest
        if shims::combat_mode(w, this) != CombatMode::NonCombat {
            transient(
                w,
                this,
                "Cannot pick that up and wield it while not at peace!",
            ); // Custom error message
            save_failed(w, this, item_guid, WeenieError::None);
            return;
        }

        let move_to_target = root_owner.unwrap_or(item);
        shims::create_pickup_move_to_chain(
            w,
            this,
            move_to_target,
            Box::new(move |w: &mut World, success: bool| {
                if current_landblock(w, this).is_none() {
                    // Maybe we were teleported as we were motioning to pick up the item
                    save_failed(w, this, item_guid, WeenieError::ActionCancelled);
                    return;
                }

                if !success {
                    save_failed(w, this, item_guid, WeenieError::ActionCancelled);
                    return;
                }

                // Was this item picked up by someone else?
                if root_owner.is_none() && current_landblock(w, item).is_none() {
                    save_failed(w, this, item_guid, WeenieError::ActionCancelled);
                    return;
                }

                start_pickup(w, this);

                let pickup_motion = get_pickup_motion(w, this, move_to_target);
                let mut pickup_chain = add_pickup_chain_to_move_to_chain(w, this, pickup_motion);

                pickup_chain.add_action(Actor::Object(this), move |w: &mut World| {
                    // Was this item picked up by someone else?
                    if root_owner.is_none() && current_landblock(w, item).is_none() {
                        save_failed(w, this, item_guid, WeenieError::ActionCancelled);
                        enqueue_pickup_done(w, this, pickup_motion);
                        return;
                    }

                    let Some((quest_solve, is_from_a_player_corpse)) =
                        verify_quest(w, this, item, root_owner)
                    else {
                        // InventoryServerSaveFailed previously sent in QuestManager
                        enqueue_pickup_done(w, this, pickup_motion);
                        return;
                    };

                    if do_handle_action_get_and_wield_item(
                        w,
                        this,
                        item,
                        from_container,
                        root_owner,
                        was_equipped,
                        wielded_location,
                        false,
                    ) {
                        send_encumbrance(w, this);

                        broadcast_sound(w, this, Sound::PickUpItem);

                        shims::emote_manager_on_pickup(w, item, this);
                        crate::world_objects::world_object_generators::notify_of_event(
                            w,
                            item,
                            RegenerationType::PickUp,
                        );

                        if quest_solve {
                            shims::emote_manager_on_quest(w, item, this);
                        }

                        if is_from_a_player_corpse {
                            let root = root_owner.expect("a corpse root owner");
                            log::info!(
                                "[CORPSE] {} (0x{}) picked up and wielded {} (0x{}) from {} (0x{})",
                                name(w, this),
                                this,
                                name(w, item),
                                item,
                                name(w, root),
                                root
                            );
                            dispatch::save_biota_to_database::save_biota_to_database(w, item, true);
                        }
                    }
                    enqueue_pickup_done(w, this, pickup_motion);
                });

                pickup_chain.enqueue_chain(w);
            }),
            None,
            false, // if player is within UseRadius of moveToTarget, do not perform rotation
        );
    } else {
        do_handle_action_get_and_wield_item(
            w,
            this,
            item,
            from_container,
            root_owner,
            was_equipped,
            wielded_location,
            false,
        );
    }
}

/// ACE's default: `from_split = false`.
// ACE: Player.DoHandleActionGetAndWieldItem
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::fn_params_excessive_bools
)]
fn do_handle_action_get_and_wield_item(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
    from_container: Option<ObjectGuid>,
    item_root_owner: Option<ObjectGuid>,
    was_equipped: bool,
    mut wielded_location: EquipMask,
    from_split: bool,
) -> bool {
    // Console.WriteLine($"-> DoHandleActionGetAndWieldItem({item.Name}, {itemRootOwner?.Name}, {wasEquipped}, {wieldedLocation})");

    let wield_error = check_wield_requirements(w, this, item);

    if wield_error != WeenieError::None {
        // client doesnt show specific wieldError here, just '<item> can't be wielded'?
        save_failed(w, this, item.full(), wield_error);
        return false;
    }

    // the client handles dequipping a lot of conflicting items automatically,
    // but there are some cases it misses that must be handled specifically here:
    if (obj(w, item)
        .current_wielded_location()
        .unwrap_or(EquipMask::None)
        & EquipMask::SelectablePlusAmmo)
        .is_empty()
        && !check_weapon_collision(w, this, Some(item), Some(wielded_location), None)
    {
        // Is this generic message good enough? -- '<item> can't be wielded'?
        save_failed(w, this, item.full(), wield_error);
        return false;
    }

    // Unwield wand/missile launcher/two-handed if dual wielding
    if wielded_location == EquipMask::Shield && !obj(w, item).is_shield() {
        // DIVERGE: an era without dual wield (`EraFeatures::dual_wield`) refuses a weapon in the
        // off hand (ClassicACE's `HandleActionGetAndWieldItem` at its Infiltration ruleset).
        // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Inventory.cs
        if !w.era.features.dual_wield {
            save_failed(w, this, item.full(), WeenieError::None);
            return false;
        }
        let mut main_weapon = creature_equipment::get_equipped_melee_weapon(w, this, true);

        if main_weapon.is_some_and(|g| !obj(w, g).is_two_handed()) {
            main_weapon = None;
        }

        let main_weapon = main_weapon
            .or_else(|| creature_equipment::get_equipped_missile_weapon(w, this))
            .or_else(|| creature_equipment::get_equipped_wand(w, this));

        // special case: instead of sending the typical DequipItem -> GetAndWieldItem here,
        // the client just sends GetAndWieldItem, and the server is responsible for detecting if DequipItem is needed

        if main_weapon.is_some() {
            // this wasn't a thing in retail, and can bug out the client during laggy conditions

            // if main-hand slot is filled with anything other than a 1-handed melee weapon, send error
            save_failed(
                w,
                this,
                item.full(),
                WeenieError::ConflictingInventoryLocation,
            );
            return false;
        }
    }

    // Unwield dual weapon if equipping thrown weapon
    if wielded_location == EquipMask::MissileWeapon {
        let dual_wield = creature_equipment::get_dual_wield_weapon(w, this);

        // special case: instead of sending the typical DequipItem -> GetAndWieldItem here,
        // the client just sends GetAndWieldItem, and the server is responsible for detecting if DequipItem is needed

        if dual_wield.is_some() {
            // this wasn't a thing in retail, and can bug out the client during laggy conditions

            // if wielding an off-hand weapon, send error
            save_failed(
                w,
                this,
                item.full(),
                WeenieError::ConflictingInventoryLocation,
            );
            return false;
        }
    }

    let valid_locations = obj(w, item).valid_locations();
    if (valid_locations.unwrap_or(EquipMask::None) & wielded_location).is_empty() {
        if valid_locations == Some(EquipMask::MeleeWeapon) && wielded_location == EquipMask::Shield
        {
            // allow dual wielding
        } else {
            log::warn!(
                "{} tried to wield {} ({}) in slot {wielded_location:?}, which doesn't match valid slots {valid_locations:?}",
                name(w, this),
                name(w, item),
                item
            );
            save_failed(w, this, item.full(), WeenieError::None);
            return false;
        }
    }

    // client bug: equip wand or bow
    // then equip melee weapon instead, then swap melee weapon to offhand slot
    // client automatically sends a request to wield the wand/bow again, only this time with EquipMask.MeleeWeapon
    // this client bug will still exist for melee weapons
    if wielded_location == EquipMask::MeleeWeapon
        && (valid_locations.unwrap_or(EquipMask::None) & wielded_location).is_empty()
    {
        save_failed(w, this, item.full(), WeenieError::None);
        return false;
    }

    // verify Aetheria slot, client doesn't handle this
    if !(wielded_location & EquipMask::Sigil).is_empty() {
        let aetheria_flags = obj(w, this).aetheria_flags();
        if wielded_location.contains(EquipMask::SigilOne)
            && !aetheria_flags.contains(AetheriaBitfield::Blue)
            || wielded_location.contains(EquipMask::SigilTwo)
                && !aetheria_flags.contains(AetheriaBitfield::Yellow)
            || wielded_location.contains(EquipMask::SigilThree)
                && !aetheria_flags.contains(AetheriaBitfield::Red)
        {
            save_failed(w, this, item.full(), WeenieError::None);
            return false;
        }
    }

    // TODO: this handles armor slots,
    // trinkets and weapons would need to be handled a bit differently

    // TODO: slots view is bugged here
    // for both slots view and non-slots view, the client is oddly sending 2 packets, similar to dual wielding weapon swapping
    // for non-slots view, the 2 packets it sends both have the full coverage slots in wieldedLocation
    // for slots view, it sends the correct packet first, with the full coverage, and then it sends a packet with coverage for just 1 slot
    // this bugs out CurrentWieldedLocation, as it won't be covering all of the slots... so for armor/clothing we set wieldedLocation to item.ValidLocations here
    if obj(w, item).is_clothing() {
        wielded_location = valid_locations.unwrap_or(EquipMask::None);
    }

    // verify item slot is valid
    // restricting this to two-handed for now, as without that clamp, it bugs out dual wielding and possibly other things
    // (`wieldedLocation & item.ValidLocations` is lifted: a null ValidLocations is null, which is not 0)
    if obj(w, item).weapon_skill() == Skill::TwoHandedCombat
        && valid_locations.is_some_and(|v| (wielded_location & v).is_empty())
    {
        save_failed(w, this, item.full(), WeenieError::None);
        return false;
    }

    if !creature_equipment::wielded_location_is_available(w, this, item, wielded_location) {
        // filtering to just armor here, or else trinkets and dual wielding breaks
        //var existing = GetEquippedClothingArmor(item.ClothingPriority ?? 0).FirstOrDefault();
        let existing = creature_equipment::get_equipped_items(w, this, item, wielded_location)
            .first()
            .copied();

        let message = format!(
            "You must remove your {} to wield {}",
            existing.map(|e| name(w, e)).unwrap_or_default(),
            name(w, item)
        );
        transient(w, this, &message);
        save_failed(w, this, item.full(), WeenieError::None);
        return false;
    }

    if was_equipped {
        // Movement is an equipped item to another equipped item slot
        let prev_location = obj(w, item).current_wielded_location();

        obj_mut(w, item).set_current_wielded_location(Some(wielded_location));
        #[allow(clippy::cast_possible_wrap)] // C#'s (int)EquipMask
        let m = game_message_public_update_property_int(
            obj_mut(w, item),
            PropertyInt::CurrentWieldedLocation,
            wielded_location.0 as i32,
        );
        send(w, this, m);

        if let Some(m) = event(w, this, |d| {
            game_event_wield_item(d, item.full(), wielded_location)
        }) {
            send(w, this, m);
        }

        // handle swapping melee weapon between hands
        if creature_equipment::is_in_child_location(w, this, item) {
            creature_equipment::reset_child(w, this, item);
            let m = {
                let (creature, wielded) = w
                    .objects
                    .get2_mut(this, item)
                    .expect("the player and its item");
                game_message_parent_event(creature, wielded, None, None)
            };
            world_object_networking::enqueue_broadcast(w, this, true, &[m]);

            // handle swapping dual-wielded weapons
            if is_double_send(w, this) {
                let prev_item_guid = fields(w, this).prev_put_item_in_container[0]
                    .as_ref()
                    .map_or(0, |e| e.item_guid);
                // `(EquipMask)prevLocation`: a null location throws InvalidOperationException in ACE.
                let prev_location = prev_location
                    .expect("System.InvalidOperationException: Nullable object must have a value.");
                handle_action_get_and_wield_item(w, this, prev_item_guid, prev_location);
            } else {
                send(w, this, game_message_sound(this, Sound::WieldObject, 1.0));
            }
        }

        // perform stance swapping if necessary
        try_shuffle_stance(w, this, wielded_location);

        return true;
    }

    if let Some(item_landblock) = current_landblock(w, item) {
        // Movement is an item pickup off the landblock
        landblock::remove_world_object(w, item_landblock, item, false, true, true);
        obj_mut(w, item).set_location(None);
    } else {
        // Movement is within the same pack or between packs in a container on the landblock
        // (`itemRootOwner.TryRemoveFromInventory`: a null root owner throws in ACE)
        if !from_split
            && !container::try_remove_from_inventory(
                w,
                item_root_owner.expect("System.NullReferenceException: itemRootOwner"),
                item,
                false,
            )
        {
            transient(w, this, "TryRemoveFromInventory failed!"); // Custom error message
            save_failed(w, this, item.full(), WeenieError::None);
        }
    }

    if !try_equip_object_with_networking(w, this, item, wielded_location) {
        transient(w, this, "TryEquipObjectWithNetworking failed!"); // Custom error message
        save_failed(w, this, item.full(), WeenieError::None);

        // todo: So the item isn't lost, we should try to put the item in the players inventory, or if that's full, on the landblock.
        log::warn!(
            "Item 0x{:08X}:{} for player {} lost from DoHandleActionGetAndWieldItem failure.",
            item.full(),
            name(w, item),
            name(w, this)
        );

        return false;
    }

    // if wielding from a loose container, we must save immediately
    if from_container.is_some_and(|c| !w.objects.get(c).is_some_and(WorldObject::stuck)) {
        dispatch::save_biota_to_database::save_biota_to_database(w, item, true);
    }

    true
}

/// Client will automatically send any unequip (PutItemInContainer) message before the
/// GetAndWield, but misses some instances and can be memory hacked to ignore others. Let's just
/// make sure our status is accurate before actually equipping an item! Returns true if the items
/// were successfuly remove and the new item can attempt to be equipped, otherwise false.
// ACE: Player.CheckWeaponCollision
#[allow(clippy::too_many_lines)]
pub fn check_weapon_collision(
    w: &World,
    this: ObjectGuid,
    item: Option<ObjectGuid>,
    wielded_location: Option<EquipMask>,
    combat_mode: Option<CombatMode>,
) -> bool {
    // Client actually allows these equip scenarios:
    // Shield with a Two-Handed weapon.

    let combat_mode = combat_mode.unwrap_or_else(|| shims::combat_mode(w, this));

    let player_name = name(w, this);
    let conflicts = |g: ObjectGuid| {
        let o = obj(w, g);
        o.is_two_handed() || o.is_caster_weapon() || o.is_ammo_launcher()
    };

    if let (Some(item), Some(wielded_location)) = (item, wielded_location) {
        let item_name = name(w, item);
        let i = obj(w, item);
        // Equipping a new item
        match wielded_location {
            EquipMask::Shield => {
                // Remove any items in the shield/offhand slot, two-handed weapons, missile weapons or casters
                if let Some(offhand) = creature_equipment::get_equipped_off_hand(w, this) {
                    log::warn!("'{player_name}' tried to wield '{item_name}' ({item}) in slot {wielded_location:?}, but is occupied by '{}'", name(w, offhand));
                    return false;
                }

                // Remove any Two Handed, Caster (magic), or Missile Weapons
                if let Some(mainhand) =
                    creature_equipment::get_equipped_main_hand(w, this).filter(|&m| conflicts(m))
                {
                    log::warn!("'{player_name}' tried to wield '{item_name}' ({item}) in slot {wielded_location:?}, which conflicts with '{}'", name(w, mainhand));
                    return false;
                }
            }
            EquipMask::MissileWeapon => {
                // Should not have any items in either hand for ammo launchers (bows, atlatls)
                // Thrown weapons (ie. phials) can have a shield
                if let Some(offhand) = creature_equipment::get_equipped_off_hand(w, this)
                    .filter(|_| i.is_ammo_launcher())
                {
                    log::warn!("'{player_name}' tried to wield '{item_name}' ({item}) in slot {wielded_location:?}, which conflicts with '{}'", name(w, offhand));
                    return false;
                }

                if let Some(mainhand) = creature_equipment::get_equipped_main_hand(w, this) {
                    log::warn!("'{player_name}' tried to wield '{item_name}' ({item}) in slot {wielded_location:?}, which conflicts with '{}'", name(w, mainhand));
                    return false;
                }

                // Ensure our ammo types align properly
                let ammo = creature_equipment::get_equipped_ammo(w, this);
                if let Some(ammo) = ammo
                    .filter(|&a| i.ammo_type().is_some() && obj(w, a).ammo_type() != i.ammo_type())
                {
                    log::warn!(
                        "'{player_name}' tried to wield '{item_name}' ({item}), AmmoType: {:?} in slot {wielded_location:?}, which conflicts with ammo of '{}' ({:?})",
                        i.ammo_type(),
                        name(w, ammo),
                        obj(w, ammo).ammo_type()
                    );
                    return false;
                }
            }
            EquipMask::MissileAmmo => {
                // Ensure our ammo types align properly
                let mainhand = creature_equipment::get_equipped_main_hand(w, this);
                if let Some(mainhand) = mainhand.filter(|&m| {
                    let m = obj(w, m);
                    m.ammo_type().is_some()
                        && i.ammo_type().is_some()
                        && m.ammo_type() != i.ammo_type()
                }) {
                    log::warn!(
                        "'{player_name}' tried to wield '{item_name}' ({item}), AmmoType: {:?} in slot {wielded_location:?}, which conflicts with AmmoType of '{}' ({:?})",
                        i.ammo_type(),
                        name(w, mainhand),
                        obj(w, mainhand).ammo_type()
                    );
                    return false;
                }
            }
            EquipMask::TwoHanded => {
                // Should not have any items in the shield/offhand slot, two-handed weapons, missile weapons or casters
                if let Some(offhand) = creature_equipment::get_equipped_off_hand(w, this) {
                    log::warn!("'{player_name}' tried to wield '{item_name}' ({item}) in slot {wielded_location:?}, which conflicts with '{}'", name(w, offhand));
                    return false;
                }

                // Remove anything in the main hand!
                if let Some(mainhand) = creature_equipment::get_equipped_main_hand(w, this) {
                    log::warn!("'{player_name}' tried to wield '{item_name}' ({item}) in slot {wielded_location:?}, which conflicts with '{}'", name(w, mainhand));
                    return false;
                }
            }
            EquipMask::MeleeWeapon => {
                // Should not have any Caster, Missile, or TwoHanders equipped
                if let Some(offhand) =
                    creature_equipment::get_equipped_off_hand(w, this).filter(|&g| conflicts(g))
                {
                    log::warn!("'{player_name}' tried to wield '{item_name}' ({item}) in slot {wielded_location:?}, which conflicts with '{}'", name(w, offhand));
                    return false;
                }

                if let Some(mainhand) =
                    creature_equipment::get_equipped_main_hand(w, this).filter(|&g| conflicts(g))
                {
                    log::warn!("'{player_name}' tried to wield '{item_name}' ({item}) in slot {wielded_location:?}, which conflicts with '{}'", name(w, mainhand));
                    return false;
                }
            }
            EquipMask::Held => {
                // Should not have any items in offhand slot for casters only
                if i.is_caster_weapon() {
                    if let Some(offhand) = creature_equipment::get_equipped_off_hand(w, this) {
                        log::warn!("'{player_name}' tried to wield '{item_name}' ({item}) in slot {wielded_location:?}, which conflicts with '{}'", name(w, offhand));
                        return false;
                    }
                }

                // Should not have any items still in mainhand slot
                if let Some(mainhand) = creature_equipment::get_equipped_main_hand(w, this) {
                    log::warn!("'{player_name}' tried to wield '{item_name}' ({item}) in slot {wielded_location:?}, which conflicts with '{}'", name(w, mainhand));
                    return false;
                }

                // items such as 23307 - Ball of Gunk have EquipMask.Held and no DefaultCombatMode
                // can only be wielded in NonCombat mode
                if combat_mode != CombatMode::NonCombat && i.default_combat_style().is_none() {
                    log::warn!("'{player_name}' tried to wield '{item_name}' ({item}) in slot {wielded_location:?}, which conflicts with {combat_mode:?} combat mode");
                    return false;
                }
            }
            _ => {}
        }
    } else {
        // changing combat mode

        // Just do a quick sanity check to ensure the player isn't wielding two weapons they shouldn't
        let mainhand = creature_equipment::get_equipped_main_hand(w, this);
        let offhand = creature_equipment::get_equipped_off_hand(w, this);

        // Wielding just one item is perfectly fine...its when they have two if might be suspect
        if let Some(mainhand) = mainhand {
            let m = obj(w, mainhand);
            if let Some(offhand) = offhand {
                // Can't wield these with anything else!
                if m.is_two_handed() || m.is_ammo_launcher() || m.is_caster_weapon() {
                    log::warn!("'{player_name}' is illegally wielding '{}' ({mainhand}) and {}' ({offhand})", name(w, mainhand), name(w, offhand));
                    return false;
                }
            }

            // Ensure our ammo matches up properly
            if m.is_ammo_launcher() {
                let ammo = creature_equipment::get_equipped_ammo(w, this);
                if let Some(ammo) = ammo.filter(|&a| {
                    obj(w, a).ammo_type().is_some()
                        && m.ammo_type().is_some()
                        && obj(w, a).ammo_type() != m.ammo_type()
                }) {
                    log::warn!("'{player_name}' is illegally wielding '{}' ({mainhand}) with ammo {}' ({:?})", name(w, mainhand), name(w, ammo), obj(w, ammo).ammo_type());
                    return false;
                }
            }

            // items such as 23307 - Ball of Gunk have EquipMask.Held and no DefaultCombatMode
            // they can be placed in main hand in NonCombat mode, but trying to wield them in combat mode results in the client-side error
            // 'You can't enter combat mode while wielding the <item>'
            // however, this client-side check can be bypassed with vtank

            if m.default_combat_style().is_none() {
                log::warn!("'{player_name}' is illegally wielding '{}' ({mainhand}) in {combat_mode:?} combat mode", name(w, mainhand));
                return false;
            }
        }
    }

    // All good at this point
    true
}

// ACE: Player.DoHandleActionGetAndWieldItem_DequipItemToInventory
#[allow(dead_code)] // ACE's only callers are commented out
fn do_handle_action_get_and_wield_item_dequip_item_to_inventory(
    w: &mut World,
    this: ObjectGuid,
    main_weapon: ObjectGuid,
    item: ObjectGuid,
) -> bool {
    let Some(dequipped_item) =
        try_dequip_object_with_networking(w, this, main_weapon, DequipObjectAction::DequipToPack)
    else {
        transient(w, this, "Failed to dequip existing weapon!"); // Custom error message
        save_failed(w, this, item.full(), WeenieError::None);
        return false;
    };

    if try_create_in_inventory_with_networking(w, this, dequipped_item).is_none() {
        transient(w, this, "Failed to add dequip back into inventory!"); // Custom error message
        save_failed(w, this, item.full(), WeenieError::None);

        // todo: if this happens, we should just put back the dequipped item to where it was

        return false;
    }
    true
}

// ACE: Player.CheckWieldRequirements
pub fn check_wield_requirements(w: &mut World, this: ObjectGuid, item: ObjectGuid) -> WeenieError {
    if !shims::property_manager_get_bool(w, "use_wield_requirements") {
        return WeenieError::None;
    }

    let heritage_specific_armor = obj(w, item).get_property(PropertyInt::HeritageSpecificArmor);
    let heritage_group = obj(w, this).heritage_group();
    if shims::is_olthoi_player(w, this) {
        if heritage_specific_armor.is_none_or(|h| HeritageGroup(h) != heritage_group) {
            return WeenieError::HeritageRequiresSpecificArmor;
        }
    } else if shims::is_gear_knight_player(w, this) {
        // `(item.ValidLocations & (Clothing | Armor)) != 0` is lifted: a null ValidLocations compares true.
        // ACE's composites, as the named set they amount to: clothing and armour but not the cloak,
        // which retail let a Gear Knight wear unintegrated (V229).
        let plated = crate::entity::core_plating::GEAR_KNIGHT_PLATED_LOCATIONS;
        let covers = obj(w, item)
            .valid_locations()
            .is_none_or(|v| !(v & plated).is_empty());
        if covers && heritage_specific_armor.is_none_or(|h| HeritageGroup(h) != heritage_group) {
            return WeenieError::HeritageRequiresSpecificArmor;
        }
    } else if heritage_specific_armor.is_some_and(|h| HeritageGroup(h) != heritage_group) {
        return WeenieError::ArmorRequiresSpecificHeritage;
    }

    let allowed_wielder = obj(w, item).get_property(PropertyInstanceId::AllowedWielder);
    if allowed_wielder.is_some_and(|a| a != this.full()) {
        return WeenieError::YouDoNotOwnThatItem; // Unsure of the exact message
    }

    let requirements = {
        let i = obj(w, item);
        [
            (
                i.wield_requirements(),
                i.wield_skill_type(),
                i.wield_difficulty(),
            ),
            (
                i.wield_requirements2(),
                i.wield_skill_type2(),
                i.wield_difficulty2(),
            ),
            (
                i.wield_requirements3(),
                i.wield_skill_type3(),
                i.wield_difficulty3(),
            ),
            (
                i.wield_requirements4(),
                i.wield_skill_type4(),
                i.wield_difficulty4(),
            ),
        ]
    };
    for (wield_requirement, wield_skill_type, wield_difficulty) in requirements {
        let result = check_wield_requirement(
            w,
            this,
            wield_requirement,
            wield_skill_type,
            wield_difficulty,
        );
        if result != WeenieError::None {
            return result;
        }
    }

    WeenieError::None
}

/// `int` versus `uint` comparisons promote both sides to `long` in C#.
// ACE: Player.CheckWieldRequirement
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::collapsible_match
)] // ACE's switch of ifs
pub fn check_wield_requirement(
    w: &mut World,
    this: ObjectGuid,
    wield_requirement: WieldRequirement,
    wield_skill_type: Option<i32>,
    wield_difficulty: Option<i32>,
) -> WeenieError {
    let skill_or_attribute = wield_skill_type.unwrap_or(0);
    let difficulty = wield_difficulty.unwrap_or(0) as u32; // C#'s (uint)int

    let skill = |w: &mut World| {
        let moa = world_object::convert_to_mo_a_skill(w, this, Skill(skill_or_attribute));
        obj_mut(w, this)
            .get_creature_skill(moa, false)
            .expect("System.NullReferenceException: skill")
    };
    let attribute = CreatureAttribute {
        attribute: PropertyAttribute(skill_or_attribute as u16),
    };
    let vital = |w: &World| {
        let key = PropertyAttribute2nd(skill_or_attribute as u16);
        obj(w, this)
            .vitals()
            .get(&key)
            .copied()
            .expect("System.NullReferenceException: vital")
    };

    match wield_requirement {
        WieldRequirement::Skill => {
            // verify skill level - current / buffed
            let s = skill(w);
            if s.current(w, this) < difficulty {
                return WeenieError::SkillTooLow;
            }
        }
        WieldRequirement::RawSkill => {
            // verify skill level - base
            let s = skill(w);
            if s.base(w, obj(w, this)) < difficulty {
                return WeenieError::SkillTooLow;
            }
        }
        WieldRequirement::Attrib => {
            // verify primary attribute - current / buffed
            if attribute.current(&mut StatCtx::in_world(w, this)) < difficulty {
                return WeenieError::SkillTooLow;
            }
        }
        WieldRequirement::RawAttrib => {
            // verify primary attribute - base
            if attribute.base(obj(w, this)) < difficulty {
                return WeenieError::SkillTooLow;
            }
        }
        WieldRequirement::SecondaryAttrib => {
            // verify vital - current maxvalue
            let v = vital(w);
            if v.max_value(&mut StatCtx::in_world(w, this)) < difficulty {
                return WeenieError::SkillTooLow;
            }
        }
        WieldRequirement::RawSecondaryAttrib => {
            // verify vital - base
            let v = vital(w);
            if v.base(w, obj(w, this)) < difficulty {
                return WeenieError::SkillTooLow;
            }
        }
        WieldRequirement::Level => {
            // verify player level
            if i64::from(obj(w, this).level().unwrap_or(1)) < i64::from(difficulty) {
                return WeenieError::LevelTooLow;
            }
        }
        WieldRequirement::Training => {
            // verify skill is trained / specialized
            let s = skill(w);
            if i64::from(s.advancement_class(obj(w, this)).0) < i64::from(difficulty) {
                return WeenieError::SkillTooLow;
            }
        }
        WieldRequirement::IntStat => {
            // unused in PY16
            // verify PropertyInt minimum
            let prop_int = obj(w, this)
                .get_property(PropertyInt(skill_or_attribute as u16))
                .unwrap_or(0);
            if i64::from(prop_int) < i64::from(difficulty) {
                return WeenieError::SkillTooLow;
            }
        }
        WieldRequirement::BoolStat => {
            // unused in PY16
            // verify PropertyBool equal
            let prop_bool = obj(w, this)
                .get_property(PropertyBool(skill_or_attribute as u16))
                .unwrap_or(false);
            if prop_bool != (difficulty != 0) {
                return WeenieError::SkillTooLow;
            }
        }
        WieldRequirement::CreatureType => {
            // verify creature type
            let creature_type = obj(w, this)
                .creature_type()
                .unwrap_or(empyrean_entity::enums::CreatureType::Invalid);
            if i64::from(creature_type.0) != i64::from(difficulty) {
                return WeenieError::SkillTooLow;
            }
        }
        WieldRequirement::HeritageType => {
            // verify heritage type
            if i64::from(obj(w, this).heritage_group().0) != i64::from(difficulty) {
                return WeenieError::ArmorRequiresSpecificHeritage;
            }
        }
        _ => {}
    }

    WeenieError::None
}

// =========================================
// Game Action Handlers - Inventory Stacking
// =========================================

/// The stack handlers' "Split amount not valid!" answer: the transient plus the save failure.
fn split_amount_not_valid(w: &mut World, this: ObjectGuid, stack_id: u32) {
    transient(w, this, "Split amount not valid!"); // Custom error message
    save_failed(w, this, stack_id, WeenieError::None);
}

/// This method processes the Game Action (F7B1) Stackable Split To Container (0x0055). This is
/// raised when we:
/// - try to split a stack into the same container
/// - try to split a stack off of the landblock into a container
/// - try to split a stack into a different container that doesn't already have a stack that can support a merge
// ACE: Player.HandleActionStackableSplitToContainer
#[allow(clippy::too_many_lines)]
pub fn handle_action_stackable_split_to_container(
    w: &mut World,
    this: ObjectGuid,
    stack_id: u32,
    container_id: u32,
    placement_position: i32,
    amount: i32,
) {
    //Console.WriteLine($"{Name}.HandleActionStackableSplitToContainer({stackId:X8}, {containerId:X8}, {placementPosition}, {amount})");

    if amount <= 0 {
        log::warn!("Player 0x{:08X}:{} tried to split item with invalid amount ({amount}) 0x{stack_id:08X}.", this.full(), name(w, this));
        split_amount_not_valid(w, this, stack_id);
        return;
    }

    let stack_found = find_object(
        w,
        this,
        ObjectGuid::new(stack_id),
        SearchLocations::LocationsICanMove,
    );
    let container_found = find_object(
        w,
        this,
        ObjectGuid::new(container_id),
        SearchLocations::MyInventory
            | SearchLocations::Landblock
            | SearchLocations::LastUsedContainer,
    );
    let (stack_found_in_container, stack_root_owner) =
        (stack_found.found_in_container, stack_found.root_owner);
    let container = container_found.result.filter(|&g| obj(w, g).is_container());
    let container_root_owner = container_found.root_owner;

    let Some(stack) = stack_found.result else {
        transient(w, this, "Source stack not found!"); // Custom error message
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    };

    if !stack.is_dynamic() || obj(w, stack).stuck() {
        log::warn!(
            "Player 0x{:08X}:{} tried to move item 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack)
        );
        weenie_error(w, this, WeenieError::Stuck);
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    let is_stackable = obj(w, stack).is_stackable();
    if !is_stackable {
        log::warn!(
            "Player 0x{:08X}:{} tried to split an item 0x{:08X}:{} that is not stackable.",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack)
        );
        //Session.Network.EnqueueSend(new GameEventWeenieError(Session, WeenieError.Stuck));
        transient(w, this, "You cannot split that!"); // Custom error message
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    if stack_root_owner != Some(this)
        && container_root_owner == Some(this)
        && !has_enough_burden_to_add_to_inventory(w, this, stack)
    {
        transient(w, this, "You are too encumbered to carry that!");
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    let Some(container) = container else {
        transient(w, this, "Target container not found!"); // Custom error message
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    };

    if obj(w, container).is_corpse() {
        let message = format!("You cannot put {} in that.", name(w, stack));
        transient(w, this, &message); // Custom error message
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    let stack_size = obj(w, stack).stack_size();
    if stack_size.is_none_or(|s| s == 0) {
        log::warn!(
            "Player 0x{:08X}:{} tried to split invalid item 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack)
        );
        transient(w, this, "Stack not valid!"); // Custom error message
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    if stack_size.is_some_and(|s| s <= amount) {
        log::warn!(
            "Player 0x{:08X}:{} tried to split item with invalid amount ({amount}) 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack)
        );
        split_amount_not_valid(w, this, stack_id);
        return;
    }

    if shims::is_trading(w, this) && is_being_traded(w, this, stack) {
        save_failed(w, this, stack_id, WeenieError::TradeItemBeingTraded);
        return;
    }

    if dispatch::is_attuned_or_contains_attuned::is_attuned_or_contains_attuned(w, stack)
        && stack_root_owner == Some(this)
        && container_root_owner != Some(this)
    {
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    if (stack_root_owner == Some(this) && container_root_owner != Some(this))
        || (stack_root_owner != Some(this) && container_root_owner == Some(this))
    {
        // Movement is between the player and the world
        if stack_root_owner.is_some_and(|r| obj(w, r).is_vendor()) {
            transient(w, this, "You cannot merge from vendor"); // Custom error message
            save_failed(w, this, stack_id, WeenieError::None);
            return;
        }

        let move_to_object = if stack_root_owner == Some(this) {
            container_root_owner.unwrap_or(container)
        } else {
            stack_root_owner.unwrap_or(stack)
        };

        let stack_original_container = obj(w, stack).container_id();

        // `stackOriginalContainer != stack.ContainerId || stack.StackSize <= amount` negated.
        let stack_still_valid = move |w: &World| {
            let s = w.objects.get(stack);
            s.and_then(WorldObject::container_id) == stack_original_container
                && !s
                    .and_then(WorldObject::stack_size)
                    .is_some_and(|size| size <= amount)
        };

        shims::create_pickup_move_to_chain(
            w,
            this,
            move_to_object,
            Box::new(move |w: &mut World, success: bool| {
                if current_landblock(w, this).is_none() {
                    // Maybe we were teleported as we were motioning to split the item
                    save_failed(w, this, stack_id, WeenieError::ActionCancelled);
                    return;
                }

                if !success {
                    save_failed(w, this, stack_id, WeenieError::ActionCancelled);
                    return;
                }

                // We make sure the stack is still valid. It could have changed during our movement
                if !stack_still_valid(w) {
                    log::debug!("Player 0x{:08X}:{} tried to split an item that's no longer valid 0x{:08X}.", this.full(), name(w, this), stack.full());
                    transient(w, this, "Split failed!"); // Custom error message
                    save_failed(w, this, stack_id, WeenieError::ActionCancelled);
                    return;
                }

                start_pickup(w, this);

                let pickup_motion = get_pickup_motion(w, this, move_to_object);
                let mut pickup_chain = add_pickup_chain_to_move_to_chain(w, this, pickup_motion);

                pickup_chain.add_action(Actor::Object(this), move |w: &mut World| {
                    // We make sure the stack is still valid. It could have changed during our pickup animation
                    if !stack_still_valid(w) {
                        log::debug!("Player 0x{:08X}:{} tried to split an item that's no longer valid 0x{:08X}.", this.full(), name(w, this), stack.full());
                        transient(w, this, "Split failed!"); // Custom error message
                        save_failed(w, this, stack_id, WeenieError::ActionCancelled);
                        enqueue_pickup_done(w, this, pickup_motion);
                        return;
                    }

                    let Some(new_stack) = create_new_world_object(w, obj(w, stack).biota.weenie_class_id) else {
                        // this should never happen under normal circumstances,
                        // but can happen if the player has an item in their inventory that is no longer in the world database
                        save_failed(w, this, stack_id, WeenieError::ActionCancelled);
                        enqueue_pickup_done(w, this, pickup_motion);
                        return;
                    };

                    obj_mut(w, new_stack).set_stack_size(Some(amount));

                    let targets = SplitTargets { stack, stack_found_in_container, stack_root_owner, container, container_root_owner };
                    if do_handle_action_stackable_split_to_container(w, this, targets, new_stack, placement_position, amount) {
                        send_encumbrance(w, this);

                        if weenie_type(w, stack) == WeenieType::Coin {
                            shims::update_coin_value(w, this);
                        }

                        if stack_root_owner == Some(this) {
                            broadcast_sound(w, this, Sound::DropItem);
                        } else if container_root_owner == Some(this) {
                            broadcast_sound(w, this, Sound::PickUpItem);
                        }
                    }
                    enqueue_pickup_done(w, this, pickup_motion);
                });

                pickup_chain.enqueue_chain(w);
            }),
            None,
            false, // if player is within UseRadius of moveToTarget, do not perform rotation
        );
    } else {
        // This is a self-contained movement
        let Some(new_stack) = create_new_world_object(w, obj(w, stack).biota.weenie_class_id)
        else {
            // this should never happen under normal circumstances,
            // but can happen if the player has an item in their inventory that is no longer in the world database
            save_failed(w, this, stack_id, WeenieError::None);
            return;
        };

        obj_mut(w, new_stack).set_stack_size(Some(amount));

        let targets = SplitTargets {
            stack,
            stack_found_in_container,
            stack_root_owner,
            container,
            container_root_owner,
        };
        do_handle_action_stackable_split_to_container(
            w,
            this,
            targets,
            new_stack,
            placement_position,
            amount,
        );
    }
}

/// The source stack and destination container of a split (and where each was found).
#[derive(Debug, Clone, Copy)]
struct SplitTargets {
    stack: ObjectGuid,
    stack_found_in_container: Option<ObjectGuid>,
    stack_root_owner: Option<ObjectGuid>,
    container: ObjectGuid,
    container_root_owner: Option<ObjectGuid>,
}

// ACE: Player.DoHandleActionStackableSplitToContainer
fn do_handle_action_stackable_split_to_container(
    w: &mut World,
    this: ObjectGuid,
    targets: SplitTargets,
    new_stack: ObjectGuid,
    placement_position: i32,
    amount: i32,
) -> bool {
    let SplitTargets {
        stack,
        stack_found_in_container,
        stack_root_owner,
        container,
        container_root_owner,
    } = targets;
    //Console.WriteLine($"{Name}.DoHandleActionStackableSplitToContainer({stack?.Name}, {stackFoundInContainer?.Name}, {stackRootOwner?.Name}, {container?.Name}, {containerRootOwner?.Name}, {newStack?.Name}, {placementPosition}, {amount})");

    // Before we modify the original stack, we make sure we can add the new stack
    if !container::try_add_to_inventory(w, container, new_stack, placement_position, true, true) {
        transient(w, this, "TryAddToInventory failed!"); // Custom error message
        save_failed(w, this, stack.full(), WeenieError::None);
        forget(w, new_stack);
        return false;
    }

    if let Some(container_root_owner) = container_root_owner.filter(|&r| r != container) {
        let (unit_encumbrance, unit_value) = (
            obj(w, stack).stack_unit_encumbrance(),
            obj(w, stack).stack_unit_value(),
        );
        let r = obj_mut(w, container_root_owner);
        // `containerRootOwner.EncumbranceVal += (stack.StackUnitEncumbrance * amount)`: all lifted.
        let e = r
            .encumbrance_val()
            .zip(mul_nullable(unit_encumbrance, amount))
            .map(|(a, b)| a.wrapping_add(b));
        r.set_encumbrance_val(e);
        let v = r
            .value()
            .zip(mul_nullable(unit_value, amount))
            .map(|(a, b)| a.wrapping_add(b));
        r.set_value(v);
    }

    let m = game_message_create_object(w, new_stack, false, false);
    send(w, this, m);
    if let Some(m) = contain_id(w, this, new_stack, container) {
        send(w, this, m);
    }

    if !adjust_stack(
        w,
        this,
        stack,
        amount.wrapping_neg(),
        stack_found_in_container,
        stack_root_owner,
    ) {
        return false;
    }

    let m = set_stack_size_message(w, stack);
    if stack_root_owner.is_none() {
        world_object_networking::enqueue_broadcast(w, this, true, &[m]);
    } else {
        send(w, this, m);
    }

    true
}

/// This method processes the Game Action (F7B1) Stackable Split To 3D (0x0056). This is raised
/// when we:
/// - try to split a stack onto the landblock
// ACE: Player.HandleActionStackableSplitTo3D
pub fn handle_action_stackable_split_to3_d(
    w: &mut World,
    this: ObjectGuid,
    stack_id: u32,
    amount: i32,
) {
    if amount <= 0 {
        log::warn!("Player 0x{:08X}:{} tried to split item with invalid amount ({amount}) 0x{stack_id:08X}.", this.full(), name(w, this));
        split_amount_not_valid(w, this, stack_id);
        return;
    }

    let found = find_object(
        w,
        this,
        ObjectGuid::new(stack_id),
        SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
    );
    let (stack_found_in_container, stack_root_owner) = (found.found_in_container, found.root_owner);

    let Some(stack) = found.result else {
        transient(w, this, "Stack not found!"); // Custom error message
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    };

    let is_stackable = obj(w, stack).is_stackable();
    if !is_stackable {
        log::warn!(
            "Player 0x{:08X}:{} tried to split an item 0x{:08X}:{} that is not stackable.",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack)
        );
        //Session.Network.EnqueueSend(new GameEventWeenieError(Session, WeenieError.Stuck));
        transient(w, this, "You cannot split that!"); // Custom error message
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    let stack_size = obj(w, stack).stack_size();
    if stack_size.is_none_or(|s| s == 0) {
        log::warn!(
            "Player 0x{:08X}:{} tried to split invalid item 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack)
        );
        transient(w, this, "Stack not valid!"); // Custom error message
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    if stack_size.is_some_and(|s| s <= amount) {
        log::warn!(
            "Player 0x{:08X}:{} tried to split item with invalid amount ({amount}) 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack)
        );
        split_amount_not_valid(w, this, stack_id);
        return;
    }

    if dispatch::is_attuned_or_contains_attuned::is_attuned_or_contains_attuned(w, stack) {
        save_failed(w, this, stack_id, WeenieError::AttunedItem);
        return;
    }

    if shims::is_trading(w, this) && is_being_traded(w, this, stack) {
        save_failed(w, this, stack_id, WeenieError::TradeItemBeingTraded);
        return;
    }

    let mut action_chain = start_pickup_chain(w, this);

    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        if current_landblock(w, this).is_none() {
            // Maybe we were teleported as we were motioning to drop the item
            save_failed(w, this, stack_id, WeenieError::ActionCancelled);
            return;
        }

        if !adjust_stack(w, this, stack, amount.wrapping_neg(), stack_found_in_container, stack_root_owner) {
            save_failed(w, this, stack_id, WeenieError::ActionCancelled);
            return;
        }

        let m = set_stack_size_message(w, stack);
        send(w, this, m);

        let Some(new_stack) = create_new_world_object(w, obj(w, stack).biota.weenie_class_id) else {
            // this should never happen under normal circumstances,
            // but can happen if the player has an item in their inventory that is no longer in the world database
            save_failed(w, this, stack_id, WeenieError::ActionCancelled);
            return;
        };

        obj_mut(w, new_stack).set_stack_size(Some(amount));

        send_encumbrance(w, this);

        if weenie_type(w, stack) == WeenieType::Coin {
            shims::update_coin_value(w, this);
        }

        if try_drop_item(w, this, new_stack) {
            broadcast_sound(w, this, Sound::DropItem);
        } else {
            // restore original stack
            if adjust_stack(w, this, stack, amount, stack_found_in_container, stack_root_owner) {
                send_encumbrance(w, this);

                if weenie_type(w, stack) == WeenieType::Coin {
                    shims::update_coin_value(w, this);
                }
            } else {
                log::warn!("Partial stack 0x{:08X}:{} for player {} lost from HandleActionStackableSplitTo3D failure.", stack.full(), name(w, stack), name(w, this));
            }

            destroy(w, new_stack);
        }

        let stance = shims::current_motion_state_stance(w, this);
        let return_stance = Motion::from_stance(stance);
        world_object_networking::enqueue_broadcast_motion(w, this, &return_stance, None, None);
    });

    action_chain.enqueue_chain(w);
}

// ACE: Player.HandleActionStackableSplitToWield
#[allow(clippy::too_many_lines)]
pub fn handle_action_stackable_split_to_wield(
    w: &mut World,
    this: ObjectGuid,
    stack_id: u32,
    wielded_location: EquipMask,
    amount: i32,
) {
    //Console.WriteLine($"{Name}.HandleActionStackableSplitToWield({stackId:X8}, {wieldedLocation}, {amount})");

    if amount <= 0 {
        log::warn!("Player 0x{:08X}:{} tried to split item with invalid amount ({amount}) 0x{stack_id:08X}.", this.full(), name(w, this));
        split_amount_not_valid(w, this, stack_id);
        return;
    }

    let found = find_object(
        w,
        this,
        ObjectGuid::new(stack_id),
        SearchLocations::LocationsICanMove,
    );
    let (stack_found_in_container, stack_root_owner) = (found.found_in_container, found.root_owner);

    let Some(stack) = found.result else {
        transient(w, this, "Source stack not found!"); // Custom error message
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    };

    if !stack.is_dynamic() || obj(w, stack).stuck() {
        log::warn!(
            "Player 0x{:08X}:{} tried to move item 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack)
        );
        weenie_error(w, this, WeenieError::Stuck);
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    let is_stackable = obj(w, stack).is_stackable();
    if !is_stackable {
        log::warn!(
            "Player 0x{:08X}:{} tried to split an item 0x{:08X}:{} that is not stackable.",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack)
        );
        //Session.Network.EnqueueSend(new GameEventWeenieError(Session, WeenieError.Stuck));
        transient(w, this, "You cannot split that!"); // Custom error message
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    if stack_root_owner != Some(this) && !has_enough_burden_to_add_to_inventory(w, this, stack) {
        transient(w, this, "You are too encumbered to carry that!");
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    let stack_size = obj(w, stack).stack_size();
    if stack_size.is_none_or(|s| s == 0) {
        log::warn!(
            "Player 0x{:08X}:{} tried to split invalid item 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack)
        );
        transient(w, this, "Stack not valid!"); // Custom error message
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    if stack_size.is_some_and(|s| s <= amount) {
        log::warn!(
            "Player 0x{:08X}:{} tried to split item with invalid amount ({amount}) 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack)
        );
        split_amount_not_valid(w, this, stack_id);
        return;
    }

    if shims::is_trading(w, this) && is_being_traded(w, this, stack) {
        save_failed(w, this, stack_id, WeenieError::TradeItemBeingTraded);
        return;
    }

    if dispatch::is_attuned_or_contains_attuned::is_attuned_or_contains_attuned(w, stack) {
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    let valid_locations = obj(w, stack).valid_locations();
    if valid_locations.is_none_or(|v| v == EquipMask::None) {
        log::warn!(
            "Player 0x{:08X}:{} tried to wield item 0x{:08X}:{} to {wielded_location:?} (0x{:X}), not in item's validlocatiions {:?} (0x{:X}).",
            this.full(),
            name(w, this),
            stack.full(),
            name(w, stack),
            wielded_location.0,
            valid_locations.unwrap_or(EquipMask::None),
            valid_locations.unwrap_or(EquipMask::None).0
        );
        weenie_error(w, this, WeenieError::InvalidInventoryLocation);
        save_failed(w, this, stack_id, WeenieError::None);
        return;
    }

    if stack_root_owner != Some(this) {
        // Movement is between the player and the world
        if stack_root_owner.is_some_and(|r| obj(w, r).is_vendor()) {
            transient(w, this, "You cannot merge from vendor"); // Custom error message
            save_failed(w, this, stack_id, WeenieError::None);
            return;
        }

        let move_to_object = stack_root_owner.unwrap_or(stack);

        let stack_original_container = obj(w, stack).container_id();
        let stack_still_valid = move |w: &World| {
            let s = w.objects.get(stack);
            s.and_then(WorldObject::container_id) == stack_original_container
                && !s
                    .and_then(WorldObject::stack_size)
                    .is_some_and(|size| size <= amount)
        };

        shims::create_pickup_move_to_chain(
            w,
            this,
            move_to_object,
            Box::new(move |w: &mut World, success: bool| {
                if current_landblock(w, this).is_none() {
                    // Maybe we were teleported as we were motioning to split the item
                    save_failed(w, this, stack_id, WeenieError::ActionCancelled);
                    return;
                }

                if !success {
                    save_failed(w, this, stack_id, WeenieError::ActionCancelled);
                    return;
                }

                // We make sure the stack is still valid. It could have changed during our movement
                if !stack_still_valid(w) {
                    log::debug!("Player 0x{:08X}:{} tried to split an item that's no longer valid 0x{:08X}.", this.full(), name(w, this), stack.full());
                    transient(w, this, "Split failed!"); // Custom error message
                    save_failed(w, this, stack_id, WeenieError::ActionCancelled);
                    return;
                }

                start_pickup(w, this);

                let pickup_motion = get_pickup_motion(w, this, move_to_object);
                let mut pickup_chain = add_pickup_chain_to_move_to_chain(w, this, pickup_motion);

                pickup_chain.add_action(Actor::Object(this), move |w: &mut World| {
                    split_to_wield_pickup_done(
                        w,
                        this,
                        SplitToWield {
                            stack,
                            stack_found_in_container,
                            stack_root_owner,
                            stack_id,
                            wielded_location,
                            amount,
                            pickup_motion,
                        },
                        stack_still_valid(w),
                    );
                });

                pickup_chain.enqueue_chain(w);
            }),
            None,
            false, // if player is within UseRadius of moveToTarget, do not perform rotation
        );
    } else {
        // This is a self-contained movement
        if !adjust_stack(
            w,
            this,
            stack,
            amount.wrapping_neg(),
            stack_found_in_container,
            stack_root_owner,
        ) {
            save_failed(w, this, stack_id, WeenieError::ActionCancelled);
            return;
        }

        let m = set_stack_size_message(w, stack);
        send(w, this, m);

        let Some(new_stack) = create_new_world_object(w, obj(w, stack).biota.weenie_class_id)
        else {
            // this should never happen under normal circumstances,
            // but can happen if the player has an item in their inventory that is no longer in the world database
            save_failed(w, this, stack_id, WeenieError::None);
            return;
        };

        obj_mut(w, new_stack).set_stack_size(Some(amount));

        if do_handle_action_get_and_wield_item(
            w,
            this,
            new_stack,
            stack_found_in_container,
            stack_root_owner,
            false,
            wielded_location,
            true,
        ) {
            shims::track_object(w, this, new_stack);
        } else {
            // restore original stack
            if adjust_stack(
                w,
                this,
                stack,
                amount,
                stack_found_in_container,
                stack_root_owner,
            ) {
                send_encumbrance(w, this);

                if weenie_type(w, stack) == WeenieType::Coin {
                    shims::update_coin_value(w, this);
                }

                let m = set_stack_size_message(w, stack);
                send(w, this, m);
            } else {
                log::warn!("Partial stack 0x{:08X}:{} for player {} lost from HandleActionStackableSplitToWield failure.", stack.full(), name(w, stack), name(w, this));
            }

            destroy(w, new_stack);
        }
    }
}

/// What the pickup action of `HandleActionStackableSplitToWield` captured.
#[derive(Debug, Clone, Copy)]
struct SplitToWield {
    stack: ObjectGuid,
    stack_found_in_container: Option<ObjectGuid>,
    stack_root_owner: Option<ObjectGuid>,
    stack_id: u32,
    wielded_location: EquipMask,
    amount: i32,
    pickup_motion: MotionCommand,
}

/// The pickup chain's last action in `HandleActionStackableSplitToWield`.
fn split_to_wield_pickup_done(
    w: &mut World,
    this: ObjectGuid,
    s: SplitToWield,
    stack_still_valid: bool,
) {
    let SplitToWield {
        stack,
        stack_found_in_container,
        stack_root_owner,
        stack_id,
        wielded_location,
        amount,
        pickup_motion,
    } = s;

    // We make sure the stack is still valid. It could have changed during our pickup animation
    if !stack_still_valid {
        log::debug!(
            "Player 0x{:08X}:{} tried to split an item that's no longer valid 0x{:08X}.",
            this.full(),
            name(w, this),
            stack.full()
        );
        transient(w, this, "Split failed!"); // Custom error message
        save_failed(w, this, stack_id, WeenieError::ActionCancelled);
        enqueue_pickup_done(w, this, pickup_motion);
        return;
    }

    if !adjust_stack(
        w,
        this,
        stack,
        amount.wrapping_neg(),
        stack_found_in_container,
        stack_root_owner,
    ) {
        // ACE-BUG: this failure path never calls EnqueuePickupDone, so the player stays IsBusy (PickupState Start) and every later pickup, drop or give is refused as too busy.
        save_failed(w, this, stack_id, WeenieError::ActionCancelled);
        return;
    }

    let m = set_stack_size_message(w, stack);
    if stack_root_owner.is_none() {
        world_object_networking::enqueue_broadcast(w, this, true, &[m]);
    } else {
        send(w, this, m);
    }

    let Some(new_stack) = create_new_world_object(w, obj(w, stack).biota.weenie_class_id) else {
        // this should never happen under normal circumstances,
        // but can happen if the player has an item in their inventory that is no longer in the world database
        save_failed(w, this, stack_id, WeenieError::ActionCancelled);
        enqueue_pickup_done(w, this, pickup_motion);
        return;
    };

    obj_mut(w, new_stack).set_stack_size(Some(amount));

    if do_handle_action_get_and_wield_item(
        w,
        this,
        new_stack,
        stack_found_in_container,
        stack_root_owner,
        false,
        wielded_location,
        true,
    ) {
        send_encumbrance(w, this);

        if weenie_type(w, stack) == WeenieType::Coin {
            shims::update_coin_value(w, this);
        }

        broadcast_sound(w, this, Sound::PickUpItem);

        shims::track_object(w, this, new_stack);
    } else {
        // restore original stack
        if adjust_stack(
            w,
            this,
            stack,
            amount,
            stack_found_in_container,
            stack_root_owner,
        ) {
            send_encumbrance(w, this);

            if weenie_type(w, stack) == WeenieType::Coin {
                shims::update_coin_value(w, this);
            }

            let m = set_stack_size_message(w, stack);
            if stack_root_owner.is_none() {
                world_object_networking::enqueue_broadcast(w, this, true, &[m]);
            } else {
                send(w, this, m);
            }
        } else {
            log::warn!("Partial stack 0x{:08X}:{} for player {} lost from HandleActionStackableSplitToWield failure.", stack.full(), name(w, stack), name(w, this));
        }

        destroy(w, new_stack);
    }
    enqueue_pickup_done(w, this, pickup_motion);
}

/// This method processes the Game Action (F7B1) Stackable Merge (0x0054). This is raised when
/// we:
/// - try to merge two stacks stack in the same container
/// - try to merge two stacks stack in different container
/// - try to merge a stack from the landblock into a container
/// - try to split a stack into a different container that has a stack that can support a merge
// ACE: Player.HandleActionStackableMerge
#[allow(clippy::too_many_lines)]
pub fn handle_action_stackable_merge(
    w: &mut World,
    this: ObjectGuid,
    merge_from_guid: u32,
    merge_to_guid: u32,
    amount: i32,
) {
    //Console.WriteLine($"HandleActionStackableMerge({mergeFromGuid:X8}, {mergeToGuid:X8}, {amount})");

    if amount <= 0 {
        log::warn!("Player 0x{}:{} tried to merge item with invalid amount ({amount}) 0x{merge_from_guid:08X}.", this.full(), name(w, this));
        transient(w, this, "Merge amount not valid!"); // Custom error message
        save_failed(w, this, merge_from_guid, WeenieError::None);
        return;
    }

    let source_found = find_object(
        w,
        this,
        ObjectGuid::new(merge_from_guid),
        SearchLocations::LocationsICanMove,
    );
    let target_found = find_object(
        w,
        this,
        ObjectGuid::new(merge_to_guid),
        SearchLocations::LocationsICanMove,
    );
    let (source_stack_root_owner, target_stack_root_owner) =
        (source_found.root_owner, target_found.root_owner);

    let Some(source_stack) = source_found.result else {
        transient(w, this, "Source stack not found!"); // Custom error message
        save_failed(w, this, merge_from_guid, WeenieError::None);
        return;
    };

    if !source_stack.is_dynamic() || obj(w, source_stack).stuck() {
        log::warn!(
            "Player 0x{:08X}:{} tried to move item 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            source_stack.full(),
            name(w, source_stack)
        );
        weenie_error(w, this, WeenieError::Stuck);
        save_failed(w, this, merge_from_guid, WeenieError::None);
        return;
    }

    if source_stack_root_owner != Some(this)
        && target_stack_root_owner == Some(this)
        && !has_enough_burden_to_add_to_inventory(w, this, source_stack)
    {
        transient(w, this, "You are too encumbered to carry that!");
        save_failed(w, this, merge_from_guid, WeenieError::None);
        return;
    }

    let Some(target_stack) = target_found.result else {
        transient(w, this, "Target stack not found!"); // Custom error message
        save_failed(w, this, merge_from_guid, WeenieError::None);
        return;
    };

    if !target_stack.is_dynamic() || obj(w, target_stack).stuck() {
        log::warn!(
            "Player 0x{:08X}:{} tried to move item 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            target_stack.full(),
            name(w, target_stack)
        );
        weenie_error(w, this, WeenieError::Stuck);
        save_failed(w, this, merge_to_guid, WeenieError::None);
        return;
    }

    if target_stack_root_owner.is_some_and(|r| obj(w, r).is_corpse()) {
        let message = format!("You cannot put {} in that.", name(w, source_stack));
        transient(w, this, &message); // Custom error message
        save_failed(w, this, merge_from_guid, WeenieError::None);
        return;
    }

    let source_is_stackable = obj(w, source_stack).is_stackable();
    let target_is_stackable = obj(w, target_stack).is_stackable();
    if !source_is_stackable || !target_is_stackable {
        transient(w, this, "You cannot merge those items!"); // Custom error message
        if source_is_stackable {
            log::warn!(
                "Player 0x{:08X}:{} tried to merge an item 0x{:08X}:{} that is not stackable.",
                this.full(),
                name(w, this),
                target_stack.full(),
                name(w, target_stack)
            );
            save_failed(w, this, merge_to_guid, WeenieError::None);
        } else {
            log::warn!(
                "Player 0x{:08X}:{} tried to merge an item 0x{:08X}:{} that is not stackable.",
                this.full(),
                name(w, this),
                source_stack.full(),
                name(w, source_stack)
            );
            save_failed(w, this, merge_from_guid, WeenieError::None);
        }
        return;
    }

    if obj(w, source_stack).biota.weenie_class_id != obj(w, target_stack).biota.weenie_class_id {
        log::warn!(
            "Player 0x{:08X}:{} tried to merge different items 0x{:08X}:{} and 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            source_stack.full(),
            name(w, source_stack),
            target_stack.full(),
            name(w, target_stack)
        );
        transient(w, this, "Stacks not compatible!"); // Custom error message
        save_failed(
            w,
            this,
            merge_from_guid,
            WeenieError::YouCannotMergeDifferentStacks,
        );
        return;
    }

    let source_size = obj(w, source_stack).stack_size();
    if source_size.is_none_or(|s| s == 0) {
        log::warn!(
            "Player 0x{:08X}:{} tried to merge invalid source item 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            source_stack.full(),
            name(w, source_stack)
        );
        transient(w, this, "Stack not valid!"); // Custom error message
        save_failed(w, this, merge_from_guid, WeenieError::None);
        return;
    }

    let (target_size, target_max) = (
        obj(w, target_stack).stack_size(),
        obj(w, target_stack).max_stack_size(),
    );
    // `targetStack.StackSize == targetStack.MaxStackSize` is lifted: equal when both are null.
    let target_full = match (target_size, target_max) {
        (Some(s), Some(m)) => s == i32::from(m),
        (None, None) => true,
        _ => false,
    };
    if target_size.is_none_or(|s| s == 0) || target_full {
        log::warn!(
            "Player 0x{:08X}:{} tried to merge invalid target item 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            target_stack.full(),
            name(w, target_stack)
        );
        transient(w, this, "Target not valid!"); // Custom error message
        save_failed(w, this, merge_from_guid, WeenieError::None);
        return;
    }

    if source_size.is_some_and(|s| s < amount) {
        log::warn!(
            "Player 0x{}:{} tried to merge item with invalid amount ({amount}) 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            source_stack.full(),
            name(w, source_stack)
        );
        transient(w, this, "Merge amount not valid!"); // Custom error message
        save_failed(w, this, merge_from_guid, WeenieError::None);
        return;
    }

    if target_stack_root_owner == Some(this)
        && !container::can_merge_to_inventory(w, this, source_stack, target_stack, amount)
    {
        save_failed(w, this, source_stack.full(), WeenieError::None);
        return;
    }

    if shims::is_trading(w, this) {
        if is_being_traded(w, this, source_stack) {
            save_failed(w, this, merge_from_guid, WeenieError::TradeItemBeingTraded);
            return;
        }
        if is_being_traded(w, this, target_stack) {
            save_failed(w, this, merge_to_guid, WeenieError::TradeItemBeingTraded);
            return;
        }
    }

    if dispatch::is_attuned_or_contains_attuned::is_attuned_or_contains_attuned(w, source_stack)
        && source_stack_root_owner == Some(this)
        && target_stack_root_owner != Some(this)
    {
        save_failed(w, this, source_stack.full(), WeenieError::None);
        return;
    }

    if (source_stack_root_owner == Some(this) && target_stack_root_owner != Some(this))
        || (source_stack_root_owner != Some(this) && target_stack_root_owner == Some(this))
    {
        // Movement is between the player and the world
        if source_stack_root_owner.is_some_and(|r| obj(w, r).is_vendor()) {
            transient(w, this, "You cannot merge from vendor"); // Custom error message
            save_failed(w, this, source_stack.full(), WeenieError::None);
            return;
        }

        let move_to_object = if source_stack_root_owner == Some(this) {
            target_stack_root_owner.unwrap_or(target_stack)
        } else {
            source_stack_root_owner.unwrap_or(source_stack)
        };

        // The source's WeenieType, which ACE still reads after a whole-stack merge destroyed it.
        let source_weenie_type = weenie_type(w, source_stack);
        let source_stack_original_container = obj(w, source_stack).container_id();
        let source_still_valid = move |w: &World| {
            let s = w.objects.get(source_stack);
            s.and_then(WorldObject::container_id) == source_stack_original_container
                && !s
                    .and_then(WorldObject::stack_size)
                    .is_some_and(|size| size < amount)
        };

        shims::create_pickup_move_to_chain(
            w,
            this,
            move_to_object,
            Box::new(move |w: &mut World, success: bool| {
                if current_landblock(w, this).is_none() {
                    // Maybe we were teleported as we were motioning to split the item
                    save_failed(w, this, merge_from_guid, WeenieError::ActionCancelled);
                    return;
                }

                if !success {
                    save_failed(w, this, merge_from_guid, WeenieError::ActionCancelled);
                    return;
                }

                // We make sure the stack is still valid. It could have changed during our movement
                if !source_still_valid(w) {
                    log::debug!(
                        "Player 0x{}:{} tried to merge an item that's no longer valid 0x{:08X}.",
                        this.full(),
                        name(w, this),
                        source_stack.full()
                    );
                    transient(w, this, "Merge Failed!"); // Custom error message
                    save_failed(w, this, merge_from_guid, WeenieError::ActionCancelled);
                    return;
                }

                start_pickup(w, this);

                let pickup_motion = get_pickup_motion(w, this, move_to_object);
                let mut pickup_chain = add_pickup_chain_to_move_to_chain(w, this, pickup_motion);

                pickup_chain.add_action(Actor::Object(this), move |w: &mut World| {
                    // We make sure the stack is still valid. It could have changed during our pickup animation
                    if !source_still_valid(w) {
                        log::debug!("Player 0x{}:{} tried to merge an item that's no longer valid 0x{:08X}.", this.full(), name(w, this), source_stack.full());
                        transient(w, this, "Merge Failed!"); // Custom error message
                        save_failed(w, this, merge_from_guid, WeenieError::ActionCancelled);
                        enqueue_pickup_done(w, this, pickup_motion);
                        return;
                    }

                    if do_handle_action_stackable_merge(w, this, source_stack, target_stack, amount) {
                        // If the client used the R key to merge a partial stack from the landscape, it also tries to add the "ghosted" item of the picked up stack to the inventory as well.
                        // DIVERGE: after a whole-stack merge ACE sends CreateObject for the destroyed source (its StackSize is still > 0); a destroyed object has left World.objects (V78) and cannot be serialised, so nothing is sent then.
                        if source_stack_root_owner != Some(this) && w.objects.get(source_stack).and_then(WorldObject::stack_size).is_some_and(|s| s > 0) {
                            let m = game_message_create_object(w, source_stack, false, false);
                            send(w, this, m);
                        }

                        send_encumbrance(w, this);

                        if source_weenie_type == WeenieType::Coin {
                            shims::update_coin_value(w, this);
                        }

                        if source_stack_root_owner == Some(this) {
                            broadcast_sound(w, this, Sound::DropItem);
                        } else if target_stack_root_owner == Some(this) {
                            broadcast_sound(w, this, Sound::PickUpItem);
                        }
                    }
                    enqueue_pickup_done(w, this, pickup_motion);
                });

                pickup_chain.enqueue_chain(w);
            }),
            None,
            false, // if player is within UseRadius of moveToTarget, do not perform rotation
        );
    } else {
        // This is a self-contained movement
        do_handle_action_stackable_merge(w, this, source_stack, target_stack, amount);
    }
}

// ACE: Player.DoHandleActionStackableMerge
#[allow(clippy::too_many_lines)]
fn do_handle_action_stackable_merge(
    w: &mut World,
    this: ObjectGuid,
    source_stack: ObjectGuid,
    target_stack: ObjectGuid,
    amount: i32,
) -> bool {
    //Console.WriteLine($"DoHandleActionStackableMerge({sourceStack?.Name}, {targetStack?.Name}, {amount})");

    let mut previous_source_stack_check = source_stack;
    //var previousTargetStackCheck = targetStack;

    let source_found = find_object(w, this, source_stack, SearchLocations::LocationsICanMove);
    let target_found = find_object(w, this, target_stack, SearchLocations::LocationsICanMove);
    let mut source_stack_root_owner = source_found.root_owner;
    let (mut target_stack_found_in_container, mut target_stack_root_owner) =
        (target_found.found_in_container, target_found.root_owner);

    let (Some(source_stack), Some(target_stack)) = (source_found.result, target_found.result)
    else {
        save_failed(
            w,
            this,
            previous_source_stack_check.full(),
            WeenieError::None,
        );
        return false;
    };

    // `targetStack.MaxStackSize < targetStack.StackSize + amount`: lifted, false on a null.
    let target_overflows = |w: &World, target_stack: ObjectGuid| {
        let t = obj(w, target_stack);
        t.max_stack_size()
            .zip(add_nullable(t.stack_size(), amount))
            .is_some_and(|(m, s)| i32::from(m) < s)
    };

    if target_overflows(w, target_stack) {
        save_failed(
            w,
            this,
            previous_source_stack_check.full(),
            WeenieError::None,
        );
        return false;
    }

    let (source_size, target_size, target_max) = {
        let (s, t) = (obj(w, source_stack), obj(w, target_stack));
        (s.stack_size(), t.stack_size(), t.max_stack_size())
    };
    // `amount == sourceStack.StackSize && sourceStack.StackSize + targetStack.StackSize <= targetStack.MaxStackSize`
    let consumes_whole_source = source_size == Some(amount)
        && source_size
            .zip(target_size)
            .map(|(a, b)| a.wrapping_add(b))
            .zip(target_max)
            .is_some_and(|(sum, m)| sum <= i32::from(m));

    if consumes_whole_source {
        // The merge will consume the entire source stack
        let m = game_message_inventory_remove_object(obj(w, source_stack));
        send(w, this, m);

        if let Some(source_root) = source_stack_root_owner {
            // item is contained and not on a landblock
            let source_stack_root_player = Some(source_root).filter(|&r| obj(w, r).is_player());

            let stack_to_destroy =
                container::try_remove_from_inventory_with_item(w, source_root, source_stack, true)
                    .or_else(|| {
                        // test case: merge equipped phials with another stack in inventory
                        source_stack_root_player.and_then(|p| {
                            try_dequip_object_with_networking(
                                w,
                                p,
                                source_stack,
                                DequipObjectAction::DequipToPack,
                            )
                        })
                    });
            if let Some(stack_to_destroy) = stack_to_destroy {
                destroy(w, stack_to_destroy);
            } else {
                save_failed(
                    w,
                    this,
                    previous_source_stack_check.full(),
                    WeenieError::None,
                );
                return false;
            }
        } else {
            // item is on the landblock and not contained
            destroy(w, source_stack);
        }

        if !adjust_stack(
            w,
            this,
            target_stack,
            amount,
            target_stack_found_in_container,
            target_stack_root_owner,
        ) {
            return false;
        }

        let m = set_stack_size_message(w, target_stack);
        if current_landblock(w, target_stack).is_some() {
            world_object_networking::enqueue_broadcast(w, target_stack, true, &[m]);
        } else {
            send(w, this, m);
        }
    } else {
        // The merge will reduce the size of the source stack
        previous_source_stack_check = source_stack;
        //previousTargetStackCheck = targetStack;
        let source_found = find_object(w, this, source_stack, SearchLocations::LocationsICanMove);
        let target_found = find_object(w, this, target_stack, SearchLocations::LocationsICanMove);
        let source_stack_found_in_container = source_found.found_in_container;
        source_stack_root_owner = source_found.root_owner;
        target_stack_found_in_container = target_found.found_in_container;
        target_stack_root_owner = target_found.root_owner;

        let source_stack = match source_found.result {
            Some(s) if !obj(w, s).stack_size().is_some_and(|size| size < amount) => s,
            _ => {
                save_failed(
                    w,
                    this,
                    previous_source_stack_check.full(),
                    WeenieError::None,
                );
                return false;
            }
        };

        let target_stack = match target_found.result {
            Some(t) if !target_overflows(w, t) => t,
            _ => {
                save_failed(
                    w,
                    this,
                    previous_source_stack_check.full(),
                    WeenieError::None,
                );
                return false;
            }
        };

        if !adjust_stack(
            w,
            this,
            source_stack,
            amount.wrapping_neg(),
            source_stack_found_in_container,
            source_stack_root_owner,
        ) {
            save_failed(
                w,
                this,
                previous_source_stack_check.full(),
                WeenieError::None,
            );
            return false;
        }

        let m = set_stack_size_message(w, source_stack);
        if current_landblock(w, source_stack).is_some() {
            world_object_networking::enqueue_broadcast(w, source_stack, true, &[m]);
        } else {
            send(w, this, m);
        }

        if !adjust_stack(
            w,
            this,
            target_stack,
            amount,
            target_stack_found_in_container,
            target_stack_root_owner,
        ) {
            return false;
        }

        let m = set_stack_size_message(w, target_stack);
        if current_landblock(w, target_stack).is_some() {
            world_object_networking::enqueue_broadcast(w, target_stack, true, &[m]);
        } else {
            send(w, this, m);
        }
    }

    let item_found_on_corpse = source_stack_root_owner.is_some_and(|r| obj(w, r).is_corpse());

    let mut is_from_a_player_corpse = false;
    if item_found_on_corpse
        && source_stack_root_owner
            .and_then(|r| obj(w, r).level())
            .is_some_and(|l| l > 0)
    {
        is_from_a_player_corpse = true;
    }

    if is_from_a_player_corpse {
        let root = source_stack_root_owner.expect("a corpse root owner");
        let outcome = match w.objects.get(source_stack) {
            Some(o) if !o.wo.world_object.is_destroyed => {
                format!("leaving behind {}", format_n0(o.stack_size().unwrap_or(0)))
            }
            _ => "which resulted in the destruction".to_owned(),
        };
        log::info!(
            "[CORPSE] {} (0x{}) merged {} {outcome} of {} (0x{}) to {} (0x{}) from {} (0x{})",
            name(w, this),
            this,
            format_n0(amount),
            w.objects
                .get(source_stack)
                .map(|_| name(w, source_stack))
                .unwrap_or_default(),
            source_stack,
            name(w, target_stack),
            target_stack,
            name(w, root),
            root
        );
        dispatch::save_biota_to_database::save_biota_to_database(w, target_stack, true);
    }

    true
}

// =============================================
// Game Action Handlers - Inventory Give/Receive
// =============================================

/// This method processes the Game Action (F7B1) Give Object Request (0x00CD). This is raised
/// when we:
/// - try to give an object to another player
/// - try to give an object to an NPC
// ACE: Player.HandleActionGiveObjectRequest
pub fn handle_action_give_object_request(
    w: &mut World,
    this: ObjectGuid,
    target_guid: u32,
    item_guid: u32,
    amount: i32,
) {
    if shims::is_busy(w, this) || shims::teleporting(w, this) || shims::suicide_in_progress(w, this)
    {
        weenie_error(w, this, WeenieError::YoureTooBusy);
        save_failed(w, this, item_guid, WeenieError::None);
        return;
    }

    if amount <= 0 {
        log::warn!("Player 0x{:08X}:{} tried to give item with invalid amount ({amount}) 0x{item_guid:08X}.", this.full(), name(w, this));
        transient(w, this, "Give amount not valid!"); // Custom error message
        save_failed(w, this, item_guid, WeenieError::None);
        return;
    }

    let target = find_object(
        w,
        this,
        ObjectGuid::new(target_guid),
        SearchLocations::Landblock,
    )
    .result
    .filter(|&g| obj(w, g).is_container());
    let found = find_object(
        w,
        this,
        ObjectGuid::new(item_guid),
        SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
    );
    let give = GiveFrom {
        item_found_in_container: found.found_in_container,
        item_root_owner: found.root_owner,
        item_was_equipped: found.was_equipped,
    };

    let Some(target) = target else {
        transient(w, this, "Target not found!"); // Custom error message
        save_failed(w, this, item_guid, WeenieError::None);
        return;
    };

    let Some(item) = found.result else {
        transient(w, this, "Item not found!"); // Custom error message
        save_failed(w, this, item_guid, WeenieError::None);
        return;
    };

    if obj(w, item).stack_size().is_some_and(|s| s < amount) {
        log::warn!(
            "Player 0x{:08X}:{} tried to give item with invalid amount ({amount}) 0x{:08X}:{}.",
            this.full(),
            name(w, this),
            item.full(),
            name(w, item)
        );
        transient(w, this, "Give amount not valid!"); // Custom error message
        save_failed(w, this, item_guid, WeenieError::None);
        return;
    }

    if shims::is_trading(w, this) && is_being_traded(w, this, item) {
        save_failed(w, this, item.full(), WeenieError::TradeItemBeingTraded);
        return;
    }

    shims::create_move_to_chain(
        w,
        this,
        target,
        Box::new(move |w: &mut World, success: bool| {
            if current_landblock(w, this).is_none() {
                // Maybe we were teleported as we were motioning to pick up the item
                save_failed(w, this, item_guid, WeenieError::ActionCancelled);
                return;
            }

            if !success {
                save_failed(w, this, item_guid, WeenieError::ActionCancelled);
                return;
            }

            if w.objects.get(target).is_some_and(WorldObject::is_player) {
                give_object_to_player(w, this, target, item, give, amount);
            } else {
                give_object_to_npc(w, this, target, item, give, amount);
            }
        }),
        None,
        true,
    ); // if player is within UseRadius of moveToTarget, perform rotation?
}

/// Where the item being given was found (`FindObject`'s `out` parameters).
#[derive(Debug, Clone, Copy)]
struct GiveFrom {
    item_found_in_container: Option<ObjectGuid>,
    item_root_owner: Option<ObjectGuid>,
    item_was_equipped: bool,
}

/// `$"{Name} tries to give you {(amount > 1 ? $"{amount} " : "")}{item.GetNameWithMaterial(amount)}."`.
fn tries_to_give_you(w: &World, this: ObjectGuid, item: ObjectGuid, amount: i32) -> String {
    let count = if amount > 1 {
        format!("{amount} ")
    } else {
        String::new()
    };
    format!(
        "{} tries to give you {count}{}.",
        name(w, this),
        shims::get_name_with_material(w, item, Some(amount))
    )
}

// ACE: Player.GiveObjectToPlayer
fn give_object_to_player(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    item: ObjectGuid,
    from: GiveFrom,
    amount: i32,
) {
    if dispatch::is_attuned_or_contains_attuned::is_attuned_or_contains_attuned(w, item) {
        save_failed(w, this, item.full(), WeenieError::AttunedItem);
        return;
    }

    if obj(w, item).is_pet_device() && shims::pet_device_pet(w, item).is_some() {
        transient(
            w,
            this,
            "You must unsummon your pet before you can transfer this item!",
        );
        save_failed(w, this, item.full(), WeenieError::AttunedItem);
        return;
    }

    if shims::is_trading(w, this) && is_being_traded(w, this, item) {
        save_failed(w, this, item.full(), WeenieError::TradeItemBeingTraded);
        return;
    }

    if shims::is_olthoi_player(w, target) || shims::is_olthoi_player(w, this) {
        transient(w, this, "Olthoi cannot trade items with other players!"); // Custom error message
        save_failed(w, this, item.full(), WeenieError::None);
        return;
    }

    let allow_give = CharacterOptions1::AllowGive.0;
    #[allow(clippy::cast_sign_loss)]
    // `(target.Character.CharacterOptions1 & (int)AllowGive) != (int)AllowGive` on the raw bits
    let options1 = world_object_networking::shims::player_character(obj(w, target))
        .expect("System.NullReferenceException: target.Character")
        .character_options_1 as u32;
    if options1 & allow_give != allow_give {
        let target_name = name(w, target);
        weenie_error_with_string(
            w,
            this,
            WeenieErrorWithString::_IsNotAcceptingGiftsRightNow,
            &target_name,
        );
        save_failed(w, this, item.full(), WeenieError::None);
        let msg = game_message_system_chat(
            &tries_to_give_you(w, this, item, amount),
            ChatMessageType::Broadcast,
        );
        send(w, target, msg);
        return;
    }

    if shims::is_logging_out(w, target) {
        save_failed(w, this, item.full(), WeenieError::None);
        return;
    }

    if shims::is_busy(w, target) {
        let target_name = name(w, target);
        weenie_error_with_string(
            w,
            this,
            WeenieErrorWithString::_IsTooBusyToAcceptGifts,
            &target_name,
        );
        save_failed(w, this, item.full(), WeenieError::None);
        let msg = game_message_system_chat(
            &tries_to_give_you(w, this, item, amount),
            ChatMessageType::Broadcast,
        );
        send(w, target, msg);
        return;
    }

    // TODO: this seems a bit backwards here...
    // the item is removed from the source player's inventory,
    // and it tries to add to target player's inventory (which does the slot/burden checks, and can also independently fail)
    // these slot/burden checks should be done beforehand, before it tries to remove the item from source player

    if !container::can_add_to_inventory(w, target, item) {
        save_failed(w, this, item.full(), WeenieError::None);
        return;
    }

    if dispatch::is_unique_or_contains_unique::is_unique_or_contains_unique(w, item)
        && !check_uniques(w, target, &[item], Some(this))
    {
        save_failed(w, this, item.full(), WeenieError::None);
        return;
    }

    let Some(Some(item_to_give)) = remove_item_for_give(w, this, item, from, amount, false) else {
        return;
    };

    let mut action_chain = ActionChain::new();
    if from.item_was_equipped {
        action_chain.add_delay_seconds(w, f64::from(0.5f32));
    }

    // This is a hack because our Player_Tracking->RemoveTrackedEquippedObject() is doing GameMessageDeleteObject, not GameMessagePickupEvent
    // Without this, when you give an equipped item to a player, the player won't see it appear in their inventory
    // A bug still exists in the following scenario:
    // Player A equips weapon, gives weapon (while equipped) to player B.
    // Player B then gives weapon back to A. Player B is now bugged. The fix is to fix RemoveTrackedEquippedObject

    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        if !w.objects.contains(target)
            || try_create_in_inventory_with_networking(w, target, item_to_give).is_none()
        {
            transient(w, this, "TryCreateInInventoryWithNetworking failed!"); // Custom error message

            // todo: So the item isn't lost, we should try to put the item in the players inventory, or if that's full, on the landblock.

            save_failed(w, this, item_to_give.full(), WeenieError::None);

            if try_create_in_inventory_with_networking(w, this, item_to_give).is_none() {
                log::warn!(
                    "Item 0x{:08X}:{} for player {} lost from GiveObjecttoPlayer failure.",
                    item.full(),
                    name(w, item),
                    name(w, this)
                );
            }

            return;
        }

        if item == item_to_give {
            if let Some(m) = contain_id(w, this, item, target) {
                send(w, this, m);
            }
        }

        let stack_size = obj(w, item_to_give).stack_size().unwrap_or(1);

        let stack_msg = if stack_size != 1 {
            format!("{} ", format_n0(stack_size))
        } else {
            String::new()
        };
        let item_name = shims::get_name_with_material(w, item_to_give, Some(stack_size));

        let message = format!("You give {} {stack_msg}{item_name}.", name(w, target));
        system_chat(w, this, &message, ChatMessageType::Broadcast);

        // send DO to source player if not splitting a stack
        if item == item_to_give {
            let m = game_message_delete_object(obj_mut(w, item));
            send(w, this, m);
        }

        let message = format!("{} gives you {stack_msg}{item_name}.", name(w, this));
        system_chat(w, target, &message, ChatMessageType::Broadcast);

        broadcast_sound(w, target, Sound::ReceiveItem);
    });

    action_chain.enqueue_chain(w);
}

/// C#'s `{n:N0}` for an int in the culture ACE runs under (en-US): thousands separated by commas.
fn format_n0(n: i32) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

// ACE: Player.GiveObjectToNPC
#[allow(clippy::too_many_lines)]
fn give_object_to_npc(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    item: ObjectGuid,
    from: GiveFrom,
    amount: i32,
) {
    if !w.objects.contains(target) || !w.objects.contains(item) {
        return;
    }

    if name(w, item) == "IOU"
        && weenie_type(w, item) == WeenieType::Book
        && name(w, target) == "Town Crier"
    {
        handle_iou_turn_in(w, this, target, item);
        return;
    }

    if obj(w, item).is_pet_device() && shims::pet_device_pet(w, item).is_some() {
        transient(
            w,
            this,
            "You must unsummon your pet before you can transfer this item!",
        );
        save_failed(w, this, item.full(), WeenieError::AttunedItem);
        return;
    }

    if shims::is_olthoi_player(w, this)
        && obj(w, target).creature_type() != Some(empyrean_entity::enums::CreatureType::Olthoi)
    {
        save_failed(w, this, item.full(), WeenieError::None);
        let target_name = name(w, target);
        weenie_error_with_string(w, this, WeenieErrorWithString::_CowersFromYou, &target_name);
        return;
    }

    if shims::emote_manager_is_busy(w, target) {
        save_failed(w, this, item.full(), WeenieError::None);
        let target_name = name(w, target);
        weenie_error_with_string(
            w,
            this,
            WeenieErrorWithString::AiRefuseItemDuringEmote,
            &target_name,
        );
        return;
    }

    if shims::is_trading(w, this) && is_being_traded(w, this, item) {
        save_failed(w, this, item.full(), WeenieError::TradeItemBeingTraded);
        return;
    }

    let accept_all = obj(w, target).ai_accept_everything()
        && !dispatch::is_sticky_attuned_or_contains_sticky_attuned::is_sticky_attuned_or_contains_sticky_attuned(w, item);

    let emote_result = world_object::has_give_or_refuse_emote_for_item(w, target, item);
    if emote_result.is_some() || accept_all {
        let category = emote_result.as_ref().map(|e| e.category);
        if accept_all || (category == Some(EmoteCategory::Give) && obj(w, target).allow_give()) {
            // for NPCs that accept items with EmoteCategory.Give,
            // if stacked item, only give 1, ignoring amount indicated, unless they are AiAcceptEverything in which case, take full amount indicated
            if let Some(Some(item_to_give)) = remove_item_for_give(
                w,
                this,
                item,
                from,
                if accept_all { amount } else { 1 },
                false,
            ) {
                if item == item_to_give {
                    if let Some(m) = contain_id(w, this, item, target) {
                        send(w, this, m);
                    }
                }

                let stack_size = obj(w, item_to_give).stack_size().unwrap_or(1);

                let stack_msg = if stack_size != 1 {
                    format!("{} ", format_n0(stack_size))
                } else {
                    String::new()
                };
                let item_name = shims::get_name_with_material(w, item_to_give, Some(stack_size));

                let message = format!("You give {} {stack_msg}{item_name}.", name(w, target));
                system_chat(w, this, &message, ChatMessageType::Broadcast);
                broadcast_sound(w, target, Sound::ReceiveItem);

                // With AiAcceptEverything and no Give emote, emoteResult is null (EmoteManager returns early on it).
                shims::emote_manager_execute_emote_set(w, target, emote_result.as_ref(), this);

                destroy(w, item_to_give);
            }
        } else if category == Some(EmoteCategory::Refuse) {
            // Item rejected by npc
            let message = format!(
                "You allow {} to examine your {}.",
                name(w, target),
                shims::get_name_with_material(w, item, None)
            );
            system_chat(w, this, &message, ChatMessageType::Broadcast);
            save_failed(w, this, item.full(), WeenieError::TradeAiRefuseEmote);

            shims::emote_manager_execute_emote_set(w, target, emote_result.as_ref(), this);
        } else {
            save_failed(w, this, item.full(), WeenieError::None);
            let target_name = name(w, target);
            weenie_error_with_string(
                w,
                this,
                WeenieErrorWithString::_IsNotAcceptingGiftsRightNow,
                &target_name,
            );
        }
    } else {
        if weenie_type(w, item) == WeenieType::Deed
            && obj(w, target).allow_give()
            && obj(w, target).ai_accept_everything()
        {
            // http://acpedia.org/wiki/Housing_FAQ#House_deeds
            let stack_size = obj(w, item).stack_size().unwrap_or(1);

            let stack_msg = if stack_size != 1 {
                format!("{stack_size} ")
            } else {
                String::new()
            };
            let item_name = shims::get_name_with_material(w, item, Some(stack_size));

            let message = format!("You give {} {stack_msg}{item_name}.", name(w, target));
            system_chat(w, this, &message, ChatMessageType::Broadcast);
            broadcast_sound(w, target, Sound::ReceiveItem);

            crate::world_objects::player_house::handle_action_abandon_house(w, this);

            return;
        }

        let target_name = name(w, target);
        // C#'s `(WeenieErrorWithString)WeenieError.TradeAiDoesntWant`
        weenie_error_with_string(
            w,
            this,
            WeenieErrorWithString(WeenieError::TradeAiDoesntWant.0),
            &target_name,
        );
        save_failed(w, this, item.full(), WeenieError::None);
    }
}

/// Whether an IOU's author (scribe or page author) is one the Town Crier redeems: ours, or
/// ACE's (IOUs issued before the rename, or on a shard brought over from ACE).
// DIVERGE: ACE redeems only IOUs signed "ACEmulator"; ours also redeems those signed with our name, which is how it issues them (brand).
#[must_use]
pub fn is_iou_author(name: Option<&str>) -> bool {
    matches!(
        name,
        Some(empyrean_common::brand::ACE_PRODUCT | empyrean_common::brand::PRODUCT)
    )
}

// ACE: Player.HandleIOUTurnIn
fn handle_iou_turn_in(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    iou_to_turn_in: ObjectGuid,
) {
    let message = format!(
        "You allow {} to examine your {}.",
        name(w, target),
        shims::get_name_with_material(w, iou_to_turn_in, None)
    );
    system_chat(w, this, &message, ChatMessageType::Broadcast);
    save_failed(
        w,
        this,
        iou_to_turn_in.full(),
        WeenieError::TradeAiRefuseEmote,
    );

    let tell = |w: &mut World, text: &str| {
        if let Some(s) = session(w, this) {
            let m = game_event_tell(w, target, text, this, s, ChatMessageType::Tell);
            send(w, this, m);
        }
    };

    if !shims::property_manager_get_bool(w, "iou_trades") {
        tell(w, "Sorry! I'm not taking IOUs right now, but if you do wish to discard them, drop them in to the garbage barrels found at the Mana Forges in Hebian-To, Zaikhal, and Cragstone.");
        //Session.Network.EnqueueSend(new GameEventWeenieErrorWithString(Session, (WeenieErrorWithString)WeenieError.TradeAiDoesntWant, target.Name));
        //var barrel = WorldObjectFactory.CreateNewWorldObject("ace34726-garbagebarrel");
        //barrel.TimeToRot = 180;
        //barrel.Location = target.Location.InFrontOf(2f);
        //barrel.Location.LandblockId = new LandblockId(barrel.Location.GetCell());
        //barrel.EnterWorld();
        return;
    }

    let is_prewritten = {
        let i = obj(w, iou_to_turn_in);
        i.is_book()
            && is_iou_author(i.scribe_name().as_deref())
            && i.scribe_account().as_deref() == Some("prewritten")
    };
    if is_prewritten {
        if let Some(page) = shims::book_get_page(w, iou_to_turn_in, 0) {
            if is_iou_author(page.author_name.as_deref())
                && page.author_account.as_deref() == Some("prewritten")
                && page.author_id == u32::MAX
            {
                let page_text = page
                    .page_text
                    .clone()
                    .expect("System.NullReferenceException: PageText");

                let split: Vec<&str> = page_text.split('\n').collect();

                if !split.is_empty() {
                    let wcid = shims::uint_try_parse(split[0]);

                    //Console.WriteLine($"{success} {wcid}");

                    tell(w, "Ahh, an IOU! You know, I collect these for some reason. Let me see if I have something for it somewhere in my pack...");

                    if let Some(wcid) = wcid {
                        if let Some(item) = create_new_world_object(w, wcid) {
                            let text = format!(
                                "You're in luck! This {} was just left here the other day.",
                                name(w, item)
                            );
                            tell(w, &text);
                            tell(w, "I'll trade it to you for this IOU.");
                            let message = format!(
                                "You give {} {}.",
                                name(w, target),
                                name(w, iou_to_turn_in)
                            );
                            system_chat(w, this, &message, ChatMessageType::Broadcast);
                            broadcast_sound(w, target, Sound::ReceiveItem);

                            let from = GiveFrom {
                                item_found_in_container: None,
                                item_root_owner: None,
                                item_was_equipped: false,
                            };
                            remove_item_for_give(w, this, iou_to_turn_in, from, 1, true);
                            let success =
                                try_create_in_inventory_with_networking(w, this, item).is_some();

                            if success {
                                tell(w, "Here you go.");
                                let message =
                                    format!("{} gives you {}.", name(w, target), name(w, item));
                                system_chat(w, this, &message, ChatMessageType::Broadcast);
                                broadcast_sound(w, target, Sound::ReceiveItem);

                                if shims::property_manager_get_bool(
                                    w,
                                    "player_receive_immediate_save",
                                ) {
                                    shims::rush_next_player_save(w, this, 5);
                                }

                                log::debug!("[IOU] {} (0x{}) traded in a IOU (0x{}) for {wcid} which became {} (0x{}).", name(w, this), this, iou_to_turn_in, name(w, item), item);
                            } else {
                                forget(w, item);
                            }
                            return;
                        }

                        tell(w, "Sorry, doesn't look I've got one of those yet. Check back again later.");
                        return;
                    }
                }
            }
        }
    }

    tell(w, "Hmm... Something isn't quite right with this IOU. I can't seem to make out what its for. I'm sorry!");
}

/// Returns `Some(itemToGive)` where ACE returns true (`itemToGive` is `None` when `destroy`
/// destroyed it), `None` where ACE returns false. ACE's default: `destroy = false`.
// ACE: Player.RemoveItemForGive
#[allow(clippy::option_option)]
fn remove_item_for_give(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
    from: GiveFrom,
    amount: i32,
    destroy_item: bool,
) -> Option<Option<ObjectGuid>> {
    let stack_size = obj(w, item).stack_size();
    if stack_size.is_some_and(|s| s > 1 && amount < s) {
        // We're splitting a stack
        if !adjust_stack(
            w,
            this,
            item,
            amount.wrapping_neg(),
            from.item_found_in_container,
            from.item_root_owner,
        ) {
            save_failed(w, this, item.full(), WeenieError::None);
            return None;
        }

        let m = set_stack_size_message(w, item);
        send(w, this, m);

        let Some(new_stack) = create_new_world_object(w, obj(w, item).biota.weenie_class_id) else {
            // this should never happen under normal circumstances,
            // but can happen if the player has an item in their inventory that is no longer in the world database
            save_failed(w, this, item.full(), WeenieError::None);
            return None;
        };

        obj_mut(w, new_stack).set_stack_size(Some(amount));

        send_encumbrance(w, this);

        if weenie_type(w, item) == WeenieType::Coin {
            shims::update_coin_value(w, this);
        }

        return Some(Some(new_stack));
    }

    // We're giving the whole object
    if from.item_was_equipped {
        if try_dequip_object_with_networking(w, this, item, DequipObjectAction::GiveItem).is_none()
        {
            transient(w, this, "TryDequipObjectWithNetworking failed!"); // Custom error message
            save_failed(w, this, item.full(), WeenieError::None);
            return None;
        }
    } else if try_remove_from_inventory_with_networking(
        w,
        this,
        item,
        RemoveFromInventoryAction::GiveItem,
    )
    .is_none()
    {
        transient(w, this, "TryRemoveFromInventoryWithNetworking failed!"); // Custom error message
        save_failed(w, this, item.full(), WeenieError::None);
        return None;
    }

    if destroy_item {
        destroy(w, item);
        Some(None)
    } else {
        Some(Some(item))
    }
}

// ===========================
// Game Action Handlers - Misc
// ===========================

/// This method processes the Game Action (F7B1) Set Inscription (0x00BF). This is raised when
/// we:
/// - try to inscribe an item
// ACE: Player.HandleActionSetInscription
pub fn handle_action_set_inscription(
    w: &mut World,
    this: ObjectGuid,
    item_guid: u32,
    inscription_text: &str,
) {
    let mut item = find_object(
        w,
        this,
        ObjectGuid::new(item_guid),
        SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
    )
    .result;

    let is_sentinel_or_admin = obj(w, this).is_sentinel() || obj(w, this).is_admin();

    if item.is_none() {
        if is_sentinel_or_admin {
            item = find_object(
                w,
                this,
                ObjectGuid::new(item_guid),
                SearchLocations::Everywhere,
            )
            .result;
        }

        if item.is_none() {
            log::error!("Player_Inventory HandleActionSetInscription failed");
            return;
        }
    }
    let Some(item) = item else { return };

    if obj(w, item).inscribable() {
        let mut do_inscribe = false;
        let (scribe_account, scribe_name, scribe_iid) = {
            let i = obj(w, item);
            (i.scribe_account(), i.scribe_name(), i.scribe_iid())
        };
        let is_null_or_white_space = |s: &Option<String>| {
            s.as_deref()
                .is_none_or(|s| s.chars().all(char::is_whitespace))
        };
        let account_name = shims::account_name(w, this);
        let player_name = dispatch::name::name(w, this);
        if is_null_or_white_space(&scribe_account) && is_null_or_white_space(&scribe_name) {
            do_inscribe = true;
        } else {
            #[allow(clippy::if_same_then_else)] // ACE's three separate branches
            if is_sentinel_or_admin {
                do_inscribe = true;
            } else if scribe_iid == Some(this.full())
                && scribe_name == player_name
                && scribe_account == account_name
            {
                do_inscribe = true;
            } else if scribe_name == player_name && scribe_account == account_name {
                do_inscribe = true;
            }
        }

        if do_inscribe {
            let i = obj_mut(w, item);
            if inscription_text.is_empty() {
                i.set_inscription(None);
                i.set_scribe_name(None);
                i.set_scribe_account(None);
                i.set_scribe_iid(None);
            } else {
                i.set_inscription(Some(inscription_text.to_owned()));
                i.set_scribe_name(player_name);
                i.set_scribe_account(account_name);
                i.set_scribe_iid(Some(this.full()));
            }

            // this response was never recorded occuring from retail servers
            // Session.Network.EnqueueSend(new GameEventInscriptionResponse(Session, item));

            // There was no direct response from the servers for this event, client just sent it and moved on.
        }
    } else {
        // Send some cool you cannot inscribe that item message. Not sure how that was handled live, I could not find a pcap of a failed inscription. Og II
        // `ChatPacket.SendServerMessage(Session, message, ChatMessageType.System)`
        system_chat(
            w,
            this,
            "Target item cannot be inscribed.",
            ChatMessageType::System,
        );
    }
}

/// This handles a peculiar sequence sent by the client in certain scenarios: the client will
/// double-send 0x19 PutItemInContainer for the same object (swapping dual wield weapons,
/// swapping ammo types in combat). Reading it clears the older entry.
// ACE: Player.IsDoubleSend
fn is_double_send(w: &mut World, this: ObjectGuid) -> bool {
    let now = w.now.utc;
    let f = fields_mut(w, this);
    let (Some(newest), Some(older)) = (
        &f.prev_put_item_in_container[0],
        &f.prev_put_item_in_container[1],
    ) else {
        return false;
    };

    let is_double_send = newest.is_double_send(older, now);

    f.prev_put_item_in_container[1] = None;

    is_double_send
}

// ACE: Player.OnPutItemInContainer
fn on_put_item_in_container(
    w: &mut World,
    this: ObjectGuid,
    item_guid: u32,
    container_guid: u32,
    placement: i32,
) {
    let now = w.now.utc;
    let f = fields_mut(w, this);
    f.prev_put_item_in_container[1] = f.prev_put_item_in_container[0].take();
    f.prev_put_item_in_container[0] = Some(PutItemInContainerEvent::new(
        item_guid,
        container_guid,
        placement,
        now,
    ));
}

/// ACE's defaults: `amount = 1`, `palette = 0`, `shade = 0`. `emoter` is `None` for ACE's null.
// ACE: Player.GiveFromEmote
pub fn give_from_emote(
    w: &mut World,
    this: ObjectGuid,
    emoter: Option<ObjectGuid>,
    weenie_class_id: u32,
    amount: i32,
    palette: i32,
    shade: f32,
) {
    let Some(emoter) = emoter.filter(|_| weenie_class_id != 0) else {
        log::warn!(
            "Player.GiveFromEmote: Emoter is null: {} | weenieClassId == 0: {}",
            emoter.is_none(),
            weenie_class_id == 0
        );

        if let Some(emoter) = emoter {
            log::warn!(
                "Player.GiveFromEmote: Emoter is {} (0x{}) | WCID: {}",
                name(w, emoter),
                emoter,
                obj(w, emoter).biota.weenie_class_id
            );
        }

        return;
    };
    let mut items_to_receive = ItemsToReceive::new(w, this);

    items_to_receive.add(w, weenie_class_id, amount);

    let item_stacks = items_to_receive.required_slots();

    if items_to_receive.player_exceeds_limits() {
        //if (itemsToReceive.PlayerExceedsAvailableBurden)
        //    Session.Network.EnqueueSend(new GameEventCommunicationTransientString(Session, "You are too encumbered to use that!"));
        //else if (itemsToReceive.PlayerOutOfInventorySlots)
        //    Session.Network.EnqueueSend(new GameEventCommunicationTransientString(Session, "You do not have enough pack space to use that!"));
        //else if (itemsToReceive.PlayerOutOfContainerSlots)
        //    Session.Network.EnqueueSend(new GameEventCommunicationTransientString(Session, "You do not have enough container slots to use that!"));

        // Font of Enlightenment and Rebirth tries to give you Attribute Reset Certificate.
        let item_being_given = w
            .content
            .get_cached_weenie(weenie_class_id)
            .expect("System.NullReferenceException: itemBeingGiven");
        let count = if item_stacks > 1 {
            format!("{item_stacks} ")
        } else {
            String::new()
        };
        let item_name = if item_stacks > 1 {
            item_being_given.get_plural_name()
        } else {
            item_being_given.get_name().unwrap_or_default()
        };
        let message = format!("{} tries to give you {count}{item_name}.", name(w, emoter));
        system_chat(w, this, &message, ChatMessageType::Broadcast);

        return;
    }

    if item_stacks > 0 {
        let mut remaining = amount;

        while remaining > 0 {
            let Some(item) = create_new_world_object(w, weenie_class_id) else {
                log::warn!(
                    "Player.GiveFromEmote: Emoter is {} (0x{}) | WCID: {} is not able to be created.",
                    name(w, emoter),
                    emoter,
                    obj(w, emoter).biota.weenie_class_id
                );
                return;
            };

            if obj(w, item).is_stackable() {
                let stack_size = remaining.min(obj(w, item).max_stack_size().map_or(1, i32::from));

                obj_mut(w, item).set_stack_size(Some(stack_size));
                remaining = remaining.wrapping_sub(stack_size);
            } else {
                remaining = remaining.wrapping_sub(1);
            }

            if palette > 0 {
                obj_mut(w, item).set_palette_template(Some(palette));
            }
            if shade > 0.0 {
                obj_mut(w, item).set_shade(Some(f64::from(shade)));
            }

            if !try_create_for_give(w, this, emoter, item) {
                forget(w, item);
            }
        }
    } else {
        log::warn!(
            "Player.GiveFromEmote: itemStacks <= 0: emoter: {} (0x{}) - {} | weenieClassId: {weenie_class_id} | amount: {amount}",
            name(w, emoter),
            emoter,
            obj(w, emoter).biota.weenie_class_id
        );

        if shims::property_manager_get_bool(w, "iou_trades") {
            let item = crate::factories::player_factory::create_iou(w, weenie_class_id)
                .expect("System.NullReferenceException: PlayerFactory.CreateIOU");
            let guid = item.guid;
            if w.objects.insert(item).is_err() {
                panic!("a new dynamic guid 0x{:08X} is already live", guid.full());
            }
            if !try_create_for_give(w, this, emoter, guid) {
                forget(w, guid);
            }
        }
    }
}

// ACE: Player.TryCreateForGive
pub fn try_create_for_give(
    w: &mut World,
    this: ObjectGuid,
    giver: ObjectGuid,
    item_being_given: ObjectGuid,
) -> bool {
    let gives_message = |w: &World, verb: &str| {
        let stack_size = obj(w, item_being_given).stack_size();
        let count = if stack_size.is_some_and(|s| s > 1) {
            format!("{} ", stack_size.unwrap_or(0))
        } else {
            String::new()
        };
        format!(
            "{} {verb} {count}{}.",
            name(w, giver),
            shims::get_name_with_material(w, item_being_given, stack_size)
        )
    };

    if dispatch::is_unique_or_contains_unique::is_unique_or_contains_unique(w, item_being_given)
        && !check_uniques(w, this, &[item_being_given], Some(giver))
    {
        return false;
    }

    if try_create_in_inventory_with_networking(w, this, item_being_given).is_none() {
        let message = gives_message(w, "tries to give you");
        system_chat(w, this, &message, ChatMessageType::Broadcast);
        return false;
    }

    if !obj(w, giver)
        .get_property(PropertyBool::NpcInteractsSilently)
        .unwrap_or(false)
    {
        let message = gives_message(w, "gives you");
        system_chat(w, this, &message, ChatMessageType::Broadcast);

        broadcast_sound(w, this, Sound::ReceiveItem);
    }

    if shims::property_manager_get_bool(w, "player_receive_immediate_save") {
        shims::rush_next_player_save(w, this, 5);
    }

    true
}

/// Verifies a player can pick up an object that is unique, or contains uniques (both of ACE's
/// overloads; the single-object one is a one-element list). ACE's default: `giver = null`.
// ACE: Player.CheckUniques
// Not ACE's (retail, V296): the refusal is the client's own "too many unique items" error
// (WeenieError TooManyUniqueItems, which the client shows as "You cannot pick up more of that item!"), sent to the
// giving player when a player gave the item, else to the receiver. ACE sent a chat line that named the giver.
pub fn check_uniques(
    w: &mut World,
    this: ObjectGuid,
    objs: &[ObjectGuid],
    giver: Option<ObjectGuid>,
) -> bool {
    let mut unique_objects = Vec::new();

    for &o in objs {
        unique_objects.extend(dispatch::get_unique_objects::get_unique_objects(w, o));
    }

    // build dictionary of wcid => count
    let unique_table = UniqueTable::new(w, &unique_objects);

    // ensure player can add this obj to their inventory
    for (&wcid, entry) in unique_table.entries.iter() {
        let current = container::get_num_inventory_items_of_wcid(w, this, wcid);

        if current.wrapping_add(entry.count) > entry.max {
            let msg_target = giver.filter(|&g| obj(w, g).is_player()).unwrap_or(this);

            weenie_error(w, msg_target, WeenieError::TooManyUniqueItems);

            return false;
        }
    }
    true
}

// ACE: Player.AuditEquippedItems
pub fn audit_equipped_items(w: &mut World, this: ObjectGuid) {
    // fixes any 'invisible' equipped items, where CurrentWieldedLocation is None
    // not sure how items could have gotten into this state, possibly from legacy bugs

    let dequip_items: Vec<ObjectGuid> = creature_equipment::equipped_objects_values(w, this)
        .into_iter()
        .filter(|&i| obj(w, i).current_wielded_location() == Some(EquipMask::None))
        .collect();

    for dequip_item in dequip_items {
        log::warn!(
            "{}.AuditEquippedItems() - dequipping {} ({})",
            name(w, this),
            name(w, dequip_item),
            dequip_item
        );
        handle_action_put_item_in_container(w, this, dequip_item.full(), this.full(), 0);
    }
}

/// Returns the Equipped items matching a weenie class id.
// ACE: Player.GetEquippedObjectsOfWCID
#[must_use]
pub fn get_equipped_objects_of_wcid(
    w: &World,
    this: ObjectGuid,
    weenie_class_id: u32,
) -> Vec<ObjectGuid> {
    creature_equipment::equipped_objects_values(w, this)
        .into_iter()
        .filter(|&i| obj(w, i).biota.weenie_class_id == weenie_class_id)
        .collect()
}

/// The wcid overload. ACE's default: `amount = int.MaxValue`.
// ACE: Player.TryConsumeFromEquippedObjectsWithNetworking
pub fn try_consume_from_equipped_objects_with_networking_wcid(
    w: &mut World,
    this: ObjectGuid,
    wcid: u32,
    amount: i32,
) -> bool {
    let items = get_equipped_objects_of_wcid(w, this, wcid);

    let mut left_req = amount;
    for item in items {
        let remove_num = left_req.min(obj(w, item).stack_size().unwrap_or(1));
        if !try_consume_from_equipped_objects_with_networking(w, this, item, remove_num) {
            return false;
        }

        left_req = left_req.wrapping_sub(remove_num);
        if left_req <= 0 {
            break;
        }
    }
    true
}

/// ACE's default: `amount = int.MaxValue`.
// ACE: Player.TryConsumeFromEquippedObjectsWithNetworking
pub fn try_consume_from_equipped_objects_with_networking(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
    amount: i32,
) -> bool {
    let item_weenie_type = weenie_type(w, item);

    if amount >= obj(w, item).stack_size().unwrap_or(1) {
        if try_dequip_object_with_networking(w, this, item, DequipObjectAction::ConsumeItem)
            .is_none()
        {
            return false;
        }
    } else {
        let found = find_object(
            w,
            this,
            item,
            SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
        );

        let (Some(stack), Some(stack_root_owner)) = (found.result, found.root_owner) else {
            return false;
        };

        if !adjust_stack(
            w,
            this,
            stack,
            amount.wrapping_neg(),
            found.found_in_container,
            Some(stack_root_owner),
        ) {
            return false;
        }

        let m = set_stack_size_message(w, stack);
        send(w, this, m);
    }

    send_encumbrance(w, this);

    if item_weenie_type == WeenieType::Coin {
        shims::update_coin_value(w, this);
    }

    true
}

/// Used with UpdateObject to maintain container placement sync on server.
// ACE: Player.MoveItemToFirstContainerSlot
pub fn move_item_to_first_container_slot(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
) -> bool {
    let Some(container) = obj(w, target)
        .wo
        .world_object_properties
        .container
        .filter(|&c| obj(w, c).is_container())
    else {
        log::error!(
            "{}.Player_Inventory.MoveItemToFirstContainerSlot() - failed to find target item {} ({}) in player inventory",
            name(w, this),
            name(w, target),
            target
        );
        return false;
    };

    if !container::try_remove_from_inventory(w, this, target, false) {
        log::error!(
            "{}.Player_Inventory.MoveItemToFirstContainerSlot() - failed to remove target item {} ({}) from player inventory",
            name(w, this),
            name(w, target),
            target
        );
        return false;
    }

    if !container::try_add_to_inventory(w, container, target, 0, true, false) {
        log::error!(
            "{}.Player_Inventory.MoveItemToFirstContainerSlot() - failed to re-add target item {} ({}) to player inventory",
            name(w, this),
            name(w, target),
            target
        );
        return false;
    }

    if container != this {
        // container is sidepack - update EncumbranceVal and Value for Player
        let (encumbrance, value) = (
            obj(w, target).encumbrance_val().unwrap_or(0),
            obj(w, target).value().unwrap_or(0),
        );
        let p = obj_mut(w, this);
        let e = add_nullable(p.encumbrance_val(), encumbrance);
        p.set_encumbrance_val(e);
        let v = add_nullable(p.value(), value);
        p.set_value(v);
    }

    true
}

/// Returns the total # of equipped objects matching a wcid.
// ACE: Player.GetNumEquippedObjectsOfWCID
#[must_use]
pub fn get_num_equipped_objects_of_wcid(w: &World, this: ObjectGuid, weenie_class_id: u32) -> i32 {
    get_equipped_objects_of_wcid(w, this, weenie_class_id)
        .into_iter()
        .fold(0i32, |sum, i| {
            sum.wrapping_add(obj(w, i).stack_size().unwrap_or(1))
        })
}

/// `Player.LastOpenedContainerId` (a `Player_Use.cs` auto-property, stored in its fields).
#[must_use]
pub fn last_opened_container_id(w: &World, this: ObjectGuid) -> ObjectGuid {
    obj(w, this)
        .player
        .as_ref()
        .map_or(ObjectGuid::INVALID, |p| {
            p.player_use.last_opened_container_id
        })
}

// ================================================================================ shims

/// Members ported in other files (see the module docs).
mod shims {
    use super::{creature_equipment, dispatch, phys_ext, world_object, world_object_networking};
    use super::{
        name, obj, transient, CombatMode, DeferredAction, DotNetHashSet, EquipMask, GameMessage,
        HookGroupType, MotionCommand, MotionStance, MoveToCallback, ObjectGuid, Position, World,
        WorldObject,
    };

    /// `WorldObject.IsBusy` (get): the `WorldObject.cs` field.
    pub(super) fn is_busy(w: &World, this: ObjectGuid) -> bool {
        w.objects
            .get(this)
            .is_some_and(|o| o.wo.world_object.is_busy)
    }

    /// `WorldObject.IsBusy` (set).
    pub(super) fn set_is_busy(w: &mut World, this: ObjectGuid, value: bool) {
        w.objects
            .get_mut(this)
            .expect("System.NullReferenceException: this")
            .wo
            .world_object
            .is_busy = value;
    }

    /// `WorldObject.Teleporting` (`WorldObject.cs` field).
    pub(super) fn teleporting(w: &World, this: ObjectGuid) -> bool {
        obj(w, this).wo.world_object.teleporting
    }

    /// `Player.suicideInProgress` (`Player_Death.cs`).
    pub(super) fn suicide_in_progress(w: &World, this: ObjectGuid) -> bool {
        obj(w, this)
            .player
            .as_ref()
            .is_some_and(|p| p.player_death.suicide_in_progress)
    }

    /// SHIM: `Player.IsTrading` (`Player_Trade.cs`).
    pub(super) fn is_trading(w: &World, this: ObjectGuid) -> bool {
        crate::world_objects::player_trade::is_trading(w, this)
    }

    /// SHIM: `Player.ItemsInTradeWindow` (`Player_Trade.cs`).
    pub(super) fn items_in_trade_window(w: &World, this: ObjectGuid) -> DotNetHashSet<ObjectGuid> {
        crate::world_objects::player_trade::items_in_trade_window(w, this)
    }

    /// SHIM: `Player.TradePartner` (`Player_Trade.cs`).
    pub(super) fn trade_partner(w: &World, this: ObjectGuid) -> ObjectGuid {
        crate::world_objects::player_trade::trade_partner(w, this)
    }

    /// `Player.LasUsedHookId` (`Player_Use.cs` field; `Hook.ActOnUse` sets it).
    pub(super) fn las_used_hook_id(w: &World, this: ObjectGuid) -> ObjectGuid {
        crate::world_objects::player_use::fields(w, this).las_used_hook_id
    }

    /// `Creature.CombatMode` (`Creature_Combat.cs` field).
    pub(super) fn combat_mode(w: &World, this: ObjectGuid) -> CombatMode {
        crate::world_objects::creature_combat::fields(obj(w, this)).combat_mode
    }

    /// SHIM: `Player.HandleActionChangeCombatMode(newCombatMode, forceHandCombat, callback)`
    /// (`Player_Combat.cs`), the virtual-dispatch target.
    pub(super) fn handle_action_change_combat_mode(
        w: &mut World,
        this: ObjectGuid,
        new_combat_mode: CombatMode,
        force_hand_combat: bool,
        callback: Option<DeferredAction>,
    ) {
        crate::world_objects::player_combat::handle_action_change_combat_mode(
            w,
            this,
            new_combat_mode,
            force_hand_combat,
            callback,
        );
    }

    /// SHIM: `WorldObject.CurrentMotionState.Stance`: the stored motion state's stance, or
    /// `NonCombat` with no motion state.
    pub(super) fn current_motion_state_stance(w: &World, this: ObjectGuid) -> MotionStance {
        w.objects
            .get(this)
            .and_then(world_object_networking::shims::current_motion_state)
            .map_or(MotionStance::NonCombat, |m| m.stance)
    }

    /// SHIM: `DatManager.PortalDat.ReadFromDat<MotionTable>(MotionTableId).GetAnimationLength(stance,
    /// motion, currentMotion)`.
    pub(super) fn motion_table_get_animation_length(
        w: &World,
        this: ObjectGuid,
        stance: MotionStance,
        motion: MotionCommand,
        current_motion: Option<MotionCommand>,
    ) -> f32 {
        let id = obj(w, this).motion_table_id();
        let mt = w
            .dats
            .portal_dat()
            .read_from_dat::<empyrean_dat::file_types::MotionTable>(id);
        crate::physics::motion_table::get_animation_length_in(
            w,
            mt.as_deref(),
            stance,
            motion,
            current_motion,
        )
    }

    /// SHIM: `DatManager.PortalDat.ReadFromDat<MotionTable>(MotionTableId).GetAnimationLength(motion)`.
    pub(super) fn motion_table_get_animation_length_of(
        w: &World,
        this: ObjectGuid,
        motion: MotionCommand,
    ) -> f32 {
        let id = obj(w, this).motion_table_id();
        let mt = w
            .dats
            .portal_dat()
            .read_from_dat::<empyrean_dat::file_types::MotionTable>(id);
        crate::physics::motion_table::get_animation_length_of(w, mt.as_deref(), motion)
    }

    /// `Player.IsJumping` (`Player.cs`).
    pub(super) fn is_jumping(w: &World, this: ObjectGuid) -> bool {
        crate::world_objects::player::is_jumping(w, this)
    }

    /// SHIM: `WorldObject.Height` (`WorldObject_Properties.cs`): `PhysicsObj.GetHeight()`, or 0
    /// without a body.
    pub(super) fn height(w: &World, g: ObjectGuid) -> f32 {
        w.objects
            .get(g)
            .and_then(|o| o.phys)
            .and_then(|h| w.physics.get(h))
            .map_or(0.0, |o| o.height())
    }

    /// SHIM: `PropertyManager.GetBool(key).Item`.
    pub(super) fn property_manager_get_bool(w: &World, key: &str) -> bool {
        crate::managers::property_manager::get_bool(w, key, false, true).item
    }

    /// SHIM: `Player.UpdateCoinValue()` (`Player_Commerce.cs`).
    pub(super) fn update_coin_value(w: &mut World, this: ObjectGuid) {
        crate::world_objects::player_commerce::update_coin_value(w, this, true);
    }

    /// SHIM: `DatabaseManager.Shard.SaveBiotasInParallel(biotas, null)`.
    pub(super) fn shard_save_biotas_in_parallel(w: &mut World, biotas: &[ObjectGuid]) {
        crate::entity::landblock::shard_save_biotas_in_parallel(w, biotas);
    }

    /// `Player.EquipItemFromSet(item)` (`Player_Spells.cs`).
    pub(super) fn equip_item_from_set(w: &mut World, this: ObjectGuid, item: ObjectGuid) {
        crate::world_objects::player_spells::equip_item_from_set(w, this, item);
    }

    /// `Player.DequipItemFromSet(item)` (`Player_Spells.cs`).
    pub(super) fn dequip_item_from_set(w: &mut World, this: ObjectGuid, item: ObjectGuid) {
        crate::world_objects::player_spells::dequip_item_from_set(w, this, item);
    }

    /// `item.CheckUseRequirements(activator)` (`WorldObject_Use.cs`): `(Success, Message)`.
    pub(super) fn check_use_requirements(
        w: &mut World,
        item: ObjectGuid,
        activator: ObjectGuid,
    ) -> (bool, Option<GameMessage>) {
        let result = dispatch::check_use_requirements::check_use_requirements(w, item, activator);
        (result.success, result.message)
    }

    /// `WorldObject.HasProcSpell(spellId)` (`WorldObject_Weapon.cs`).
    pub(super) fn has_proc_spell(w: &World, item: ObjectGuid, spell_id: u32) -> bool {
        crate::world_objects::world_object_weapon::has_proc_spell(obj(w, item), spell_id)
    }

    /// SHIM: `Creature.CreateItemSpell(item, spellId)` (`Creature_Magic.cs`).
    pub(super) fn create_item_spell(
        w: &mut World,
        this: ObjectGuid,
        item: ObjectGuid,
        spell_id: u32,
    ) -> bool {
        crate::world_objects::creature_magic::create_item_spell(w, this, item, spell_id)
    }

    /// `WorldObject.OnSpellsActivated()`.
    pub(super) fn on_spells_activated(w: &mut World, item: ObjectGuid) {
        crate::world_objects::world_object_magic::on_spells_activated(w, item);
    }

    /// SHIM: `Vendor.TryGetItemForSale(itemGuid, out item)` (`Vendor.cs`).
    pub(super) fn vendor_try_get_item_for_sale(
        w: &World,
        vendor: ObjectGuid,
        item_guid: ObjectGuid,
    ) -> Option<ObjectGuid> {
        crate::world_objects::vendor::try_get_item_for_sale(w, vendor, item_guid)
    }

    /// SHIM: `CurrentLandblock.GetWieldedObject(guid)` (searchAdjacents true), whose port takes
    /// `&mut World` though it only reads: the first creature of this landblock, then of each
    /// adjacent one, wielding `guid`; a non-selectable wielded item answers null.
    pub(super) fn landblock_get_wielded_object(
        w: &World,
        lb: empyrean_entity::LandblockId,
        guid: ObjectGuid,
    ) -> Option<ObjectGuid> {
        let mut blocks = vec![lb];
        if let Some(l) = w.landblock_manager.landblocks.get(lb) {
            blocks.extend(l.adjacents.iter().copied());
        }
        for block in blocks {
            let Some(l) = w.landblock_manager.landblocks.get(block) else {
                continue;
            };
            for &creature in l.world_object_guids() {
                if !w
                    .objects
                    .get(creature)
                    .is_some_and(WorldObject::is_creature)
                {
                    continue;
                }
                if let Some(item) = creature_equipment::get_equipped_item(w, creature, guid) {
                    let selectable = w
                        .objects
                        .get(item)
                        .and_then(WorldObject::current_wielded_location)
                        .is_some_and(|l| !(l & EquipMask::Selectable).is_empty());
                    return selectable.then_some(item);
                }
            }
        }
        None
    }

    /// `Player.GetKnownObjects()` (`Player_Tracking.cs`).
    pub(super) fn get_known_objects(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
        crate::world_objects::player_tracking::get_known_objects(w, this)
    }

    /// `PetDevice.Pet` (`PetDevice.cs`).
    pub(super) fn pet_device_pet(w: &World, item: ObjectGuid) -> Option<ObjectGuid> {
        w.objects
            .get(item)
            .and_then(WorldObject::pet)
            .map(ObjectGuid::new)
    }

    /// `Corpse.IsMonster` (`Corpse.cs` field).
    pub(super) fn corpse_is_monster(w: &World, corpse: ObjectGuid) -> bool {
        crate::world_objects::corpse::fields(w, corpse).is_monster
    }

    /// SHIM: `hook.House.HouseMaxHooksUsable` (`House.cs`).
    pub(super) fn house_max_hooks_usable(w: &World, hook: ObjectGuid) -> i32 {
        let house = crate::world_objects::hook::house(w, hook)
            .expect("System.NullReferenceException: hook.House");
        obj(w, house).house_max_hooks_usable()
    }

    /// SHIM: `hook.House.HouseCurrentHooksUsable` (`House.cs`).
    pub(super) fn house_current_hooks_usable(w: &World, hook: ObjectGuid) -> i32 {
        let house = crate::world_objects::hook::house(w, hook)
            .expect("System.NullReferenceException: hook.House");
        obj(w, house).house_current_hooks_usable()
    }

    /// SHIM: `hook.House.GetHookGroupMaxCount(group)` (`House.cs`).
    pub(super) fn house_get_hook_group_max_count(
        w: &World,
        hook: ObjectGuid,
        group: HookGroupType,
    ) -> i32 {
        let house = crate::world_objects::hook::house(w, hook)
            .expect("System.NullReferenceException: hook.House");
        crate::world_objects::house::get_hook_group_max_count(w, house, group)
    }

    /// SHIM: `hook.House.GetHookGroupCurrentCount(group)` (`House.cs`).
    pub(super) fn house_get_hook_group_current_count(
        w: &World,
        hook: ObjectGuid,
        group: HookGroupType,
    ) -> i32 {
        let house = crate::world_objects::hook::house(w, hook)
            .expect("System.NullReferenceException: hook.House");
        crate::world_objects::house::get_hook_group_current_count(w, house, group)
    }

    /// SHIM: `HookGroupType.ToSentence()` (`HookGroupTypeExtensions`, ACE.Entity): the enum name.
    pub(super) fn hook_group_to_sentence(group: HookGroupType) -> String {
        group.to_sentence()
    }

    /// SHIM: `Player.SendTransientError(msg)`: the transient string event.
    pub(super) fn send_transient_error(w: &mut World, this: ObjectGuid, msg: &str) {
        transient(w, this, msg);
    }

    /// `item.EmoteManager.OnDrop(player)` (`EmoteManager.cs`).
    pub(super) fn emote_manager_on_drop(w: &mut World, item: ObjectGuid, player: ObjectGuid) {
        crate::world_objects::managers::emote_manager::on_drop(w, item, player);
    }

    /// `item.EmoteManager.OnPickup(player)`.
    pub(super) fn emote_manager_on_pickup(w: &mut World, item: ObjectGuid, player: ObjectGuid) {
        crate::world_objects::managers::emote_manager::on_pickup(w, item, player);
    }

    /// `item.EmoteManager.OnQuest(player)`.
    pub(super) fn emote_manager_on_quest(w: &mut World, item: ObjectGuid, player: ObjectGuid) {
        crate::world_objects::managers::emote_manager::on_quest(w, item, player);
    }

    /// `target.EmoteManager.IsBusy`.
    pub(super) fn emote_manager_is_busy(w: &World, target: ObjectGuid) -> bool {
        crate::world_objects::managers::emote_manager::is_busy(w, target)
    }

    /// `target.EmoteManager.ExecuteEmoteSet(emoteSet, player)`.
    pub(super) fn emote_manager_execute_emote_set(
        w: &mut World,
        target: ObjectGuid,
        emote_set: Option<&empyrean_entity::models::properties_emote::PropertiesEmote>,
        player: ObjectGuid,
    ) {
        crate::world_objects::managers::emote_manager::execute_emote_set(
            w,
            target,
            emote_set,
            Some(player),
            false,
        );
    }

    /// `QuestManager.HasQuest(questFormat)` (`QuestManager.cs`).
    pub(super) fn quest_manager_has_quest(w: &World, this: ObjectGuid, quest: &str) -> bool {
        crate::managers::quest_manager::has_quest(
            w,
            &crate::managers::quest_manager::QuestOwner::Creature(this),
            quest,
        )
    }

    /// `QuestManager.CanSolve(questFormat)`.
    pub(super) fn quest_manager_can_solve(w: &World, this: ObjectGuid, quest: &str) -> bool {
        crate::managers::quest_manager::can_solve(
            w,
            &crate::managers::quest_manager::QuestOwner::Creature(this),
            quest,
        )
    }

    /// `QuestManager.HandleNoQuestError(wo)`.
    pub(super) fn quest_manager_handle_no_quest_error(
        w: &mut World,
        this: ObjectGuid,
        wo: ObjectGuid,
    ) {
        crate::managers::quest_manager::handle_no_quest_error(
            w,
            &crate::managers::quest_manager::QuestOwner::Creature(this),
            wo,
        );
    }

    /// `QuestManager.HandleSolveError(questName)`.
    pub(super) fn quest_manager_handle_solve_error(w: &mut World, this: ObjectGuid, quest: &str) {
        crate::managers::quest_manager::handle_solve_error(
            w,
            &crate::managers::quest_manager::QuestOwner::Creature(this),
            quest,
        );
    }

    /// `Player.RemoveTrackedObject(wo, fromPickup)` (`Player_Tracking.cs`).
    pub(super) fn remove_tracked_object(
        w: &mut World,
        this: ObjectGuid,
        wo: ObjectGuid,
        from_pickup: bool,
    ) {
        crate::world_objects::player_tracking::remove_tracked_object(w, this, wo, from_pickup);
    }

    /// `Player.TrackObject(wo)` (`Player_Tracking.cs`).
    pub(super) fn track_object(w: &mut World, this: ObjectGuid, wo: ObjectGuid) {
        crate::world_objects::player_tracking::track_object(w, this, wo, false);
    }

    /// `Player.IsOlthoiPlayer` (`Player_Properties.cs`), as `SetEphemeralValues` set it.
    pub(super) fn is_olthoi_player(w: &World, this: ObjectGuid) -> bool {
        obj(w, this)
            .player
            .as_ref()
            .is_some_and(|p| p.player_properties.is_olthoi_player)
    }

    /// `Player.IsGearKnightPlayer` (`Player_Properties.cs`), as `SetEphemeralValues` set it.
    pub(super) fn is_gear_knight_player(w: &World, this: ObjectGuid) -> bool {
        obj(w, this)
            .player
            .as_ref()
            .is_some_and(|p| p.player_properties.is_gear_knight_player)
    }

    /// `Player.IsLoggingOut` (`Player.cs`).
    pub(super) fn is_logging_out(w: &World, this: ObjectGuid) -> bool {
        obj(w, this)
            .player
            .as_ref()
            .is_some_and(|p| p.player.is_logging_out)
    }

    /// `Player.RushNextPlayerSave(seconds)` (`Player_Database.cs`).
    pub(super) fn rush_next_player_save(w: &mut World, this: ObjectGuid, seconds: i32) {
        crate::world_objects::player_database::rush_next_player_save(w, this, seconds);
    }

    /// SHIM: `Player.Account.AccountName`: the session's account.
    pub(super) fn account_name(w: &World, this: ObjectGuid) -> Option<String> {
        world_object_networking::shims::player_session_account(w, this)
    }

    /// SHIM: `WorldObject.GetNameWithMaterial(stackSize)` (`WorldObject_Properties.cs`)
    /// over `RecipeManager.GetMaterialName` for the material.
    pub(super) fn get_name_with_material(
        w: &World,
        item: ObjectGuid,
        stack_size: Option<i32>,
    ) -> String {
        let mut item_name = if stack_size.is_some_and(|s| s != 1) {
            world_object::get_plural_name(w, item)
        } else {
            name(w, item)
        };

        let Some(material_type) = obj(w, item).material_type() else {
            return item_name;
        };

        let material = crate::managers::recipe_manager::get_material_name(w, material_type);

        if item_name.contains(&material) {
            item_name = item_name.replace(&material, "");
        }

        format!("{material} {item_name}")
    }

    /// SHIM: `book.GetPage(index)` (`Book.cs`).
    pub(super) fn book_get_page(
        w: &World,
        book: ObjectGuid,
        index: i32,
    ) -> Option<empyrean_entity::models::properties_book_page_data::PropertiesBookPageData> {
        crate::world_objects::book::get_page(obj(w, book), index).cloned()
    }

    /// `uint.TryParse(s, out var wcid)` (invariant digits, surrounding white space and a leading
    /// `+` allowed).
    pub(super) fn uint_try_parse(s: &str) -> Option<u32> {
        let t = s.trim_matches(char::is_whitespace);
        let t = t.strip_prefix('+').unwrap_or(t);
        if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        t.parse().ok()
    }

    /// SHIM: `item.PhysicsObj.transition(PhysicsObj.Position, targetPos, false)`, then, when it
    /// found a cell, `SetPositionInternal(transit)`, `SyncLocation()` and
    /// `SendUpdatePosition(true)`.
    pub(super) fn physics_obj_slide_to(w: &mut World, item: ObjectGuid, target_pos: &Position) {
        let Some(h) = obj(w, item).phys else { return };
        let Some(from) = w.physics.get(h).map(|o| o.position) else {
            return;
        };
        let to = phys_ext::to_physics_position(target_pos);
        let transit = w.physics.transition(h, &from, &to, false);

        if let Some(transit) = transit {
            if transit.sphere_path.curr_cell.is_some() {
                phys_ext::set_position_internal(w, h, &transit);

                world_object::sync_location(w, item);

                world_object_networking::send_update_position(w, item, true);
            }
        }
    }

    // ---------------------------------------------------------------- Player_Move.cs

    /// `Player.StopExistingMoveToChains()` (`Player_Move.cs`).
    pub(super) fn stop_existing_move_to_chains(w: &mut World, this: ObjectGuid) {
        crate::world_objects::player_move::stop_existing_move_to_chains(w, this);
    }

    /// `Player.CreateMoveToChain(target, callback, useRadius, rotate)` (`Player_Move.cs`).
    pub(super) fn create_move_to_chain(
        w: &mut World,
        this: ObjectGuid,
        target: ObjectGuid,
        callback: MoveToCallback,
        use_radius: Option<f32>,
        rotate: bool,
    ) {
        crate::world_objects::player_move::create_move_to_chain(
            w, this, target, callback, use_radius, rotate,
        );
    }

    /// [`create_move_to_chain`] for an item move (a pickup, or an item to or from a container
    /// on the landscape): not ACE's (retail captures, V257), the MoveTo carries the plain
    /// defaults (0x1EE0F), not a use-move's UseFinalHeading.
    pub(super) fn create_pickup_move_to_chain(
        w: &mut World,
        this: ObjectGuid,
        target: ObjectGuid,
        callback: MoveToCallback,
        use_radius: Option<f32>,
        rotate: bool,
    ) {
        use crate::network::motion::move_to_parameters::RetailMoveTo;
        crate::world_objects::player_move::create_move_to_chain_as(
            w,
            this,
            target,
            callback,
            use_radius,
            rotate,
            RetailMoveTo::Plain,
        );
    }
}
