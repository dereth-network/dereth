// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Vendor.cs
//! Port of `Source/ACE.Server/WorldObjects/Vendor.cs`.
//!
//! ** Buy Data Flow **
//!
//! `Player.HandleActionBuyItem` -> `Vendor.BuyItems_ValidateTransaction` ->
//! `Player.FinalizeBuyTransaction` -> `Vendor.BuyItems_FinalTransaction`
//!
//! # Items for sale
//!
//! ACE's two dictionaries hold `WorldObject`s that are on no landblock. Here each of those objects
//! lives in `World.objects` (the port's only owner of objects) and the
//! dictionaries hold their guids, in .NET's order ([`DotNetDict`]). An item bought from the unique
//! list moves straight into the player's inventory.
//!
//! # Shims
//!
//! `Creature.TurnTo(Position)` is a `not_ported!` pointer; `WorldObject.GetCylinderDistance` is
//! ported.

use std::sync::Arc;

use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_common::time::Time;
use empyrean_entity::enums::{
    AttunedStatus, DestinationType, ItemType, ObjectDescriptionFlag, PropertyBool, PropertyInt,
    PropertyString, RegenLocationType, VendorType, WeenieClassName, WeenieError,
    WeenieErrorWithString,
};
use empyrean_entity::{ObjectGuid, Weenie};

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::item_profile::ItemProfile;
use crate::entity::items_to_receive::ItemsToReceive;
use crate::entity::spell::Spell;
use crate::entity::world_object_info::WorldObjectInfo;
use crate::managers::player_manager::player_session;
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_approach_vendor::game_event_approach_vendor;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_messages::game_message::enqueue_send;
use crate::world_objects::kinds::KindData;
use crate::world_objects::world_object::{CtorEnv, CtorSource, WorldObject};
use crate::world_objects::{
    container, creature_navigation, monster_magic, monster_navigation, player_commerce,
    player_inventory, player_networking, world_object, world_object_database, world_object_magic,
};
use crate::{dispatch, World};

/// Non-property fields declared in `Vendor.cs`.
#[derive(Debug, Default)]
pub struct VendorFields {
    /// The shop's generic items, as guids of objects in `World.objects`.
    // ACE: Vendor.DefaultItemsForSale
    pub default_items_for_sale: DotNetDict<ObjectGuid, ()>,
    /// unique items purchased from other players
    // ACE: Vendor.UniqueItemsForSale
    pub unique_items_for_sale: DotNetDict<ObjectGuid, ()>,
    // ACE: Vendor.inventoryloaded
    pub inventoryloaded: bool,
    /// The last player who used this vendor
    // ACE: Vendor.lastPlayerInfo
    pub last_player_info: Option<WorldObjectInfo>,
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

/// This vendor's fields.
///
/// # Panics
/// When `this` is gone or is not a `Vendor` (ACE's `NullReferenceException`/`InvalidCastException`).
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &VendorFields {
    match &obj(w, this).kind {
        KindData::Vendor(v) => &v.vendor,
        _ => panic!(
            "System.InvalidCastException: 0x{:08X} is not a Vendor",
            this.full()
        ),
    }
}

/// This vendor's fields, mutably.
///
/// # Panics
/// As [`fields`].
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut VendorFields {
    match &mut obj_mut(w, this).kind {
        KindData::Vendor(v) => &mut v.vendor,
        _ => panic!(
            "System.InvalidCastException: 0x{:08X} is not a Vendor",
            this.full()
        ),
    }
}

/// `Name` (a null name interpolates as empty).
fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

/// The shop's default items, in .NET's order.
#[must_use]
pub fn default_items_for_sale(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    fields(w, this)
        .default_items_for_sale
        .keys()
        .copied()
        .collect()
}

/// The unique items (sold to the vendor by players), in .NET's order.
#[must_use]
pub fn unique_items_for_sale(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    fields(w, this)
        .unique_items_for_sale
        .keys()
        .copied()
        .collect()
}

/// `WorldObjectFactory.CreateNewWorldObject(wcid)`, put in `World.objects` at once (the port's
/// objects live there).
fn create_new_world_object(w: &mut World, weenie_class_id: u32) -> Option<ObjectGuid> {
    let o = crate::world_objects::world_object_equipment::create_new_world_object_by_wcid(
        w,
        weenie_class_id,
    )?;
    let guid = o.guid;
    assert!(
        w.objects.insert(o).is_ok(),
        "a new dynamic guid 0x{:08X} is already live",
        guid.full()
    );
    crate::world_objects::creature::post_insert(w, guid);
    Some(guid)
}

/// `item.Destroy()` (its defaults).
fn destroy(w: &mut World, g: ObjectGuid) {
    world_object::destroy(w, g, true, false);
}

/// `Session.Network.EnqueueSend(new GameEventCommunicationTransientString(player.Session, message))`.
fn transient(w: &mut World, player: ObjectGuid, message: &str) {
    let Some(session) = player_session(w, player) else {
        return;
    };
    let Some(data) = w.sessions.get_mut(session) else {
        return;
    };
    let m = game_event_communication_transient_string(data, message);
    enqueue_send(w, session, m);
}

// ================================================================================ Vendor.cs

/// This is raised by Player.HandleActionUseItem.
/// The item does not exist in the players possession.
/// If the item was outside of range, the player will have been commanded to move using DoMoveTo
/// before ActOnUse is called.
/// When this is called, it should be assumed that the player is within range.
// ACE: Vendor.ActOnUse
pub fn vendor_act_on_use(w: &mut World, this: ObjectGuid, activator: ObjectGuid) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    if obj(w, player).wo.world_object.is_busy {
        player_networking::send_weenie_error(w, player, WeenieError::YoureTooBusy);
        return;
    }

    if !obj(w, this).open_for_business()
        || !validate_vendor_requirements(obj(w, this), &|wcid| w.content.get_cached_weenie(wcid))
    {
        // should there be some sort of feedback to player here?
        return;
    }

    let rotate_time = dispatch::rotate::rotate(w, this, player); // vendor rotates towards player

    // TODO: remove this when DelayManager is not forward propagating current tick time

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(0.001_f32)); // force to run after rotate.EnqueueBroadcastAction
    action_chain.add_action(Actor::Object(this), move |w| load_inventory(w, this));
    action_chain.add_delay_seconds(w, f64::from(rotate_time));
    action_chain.add_action(Actor::Object(this), move |w| {
        approach_vendor(w, this, player, VendorType::Open, 0)
    });
    action_chain.enqueue_chain(w);

    if fields(w, this).last_player_info.is_none() {
        let mut close_chain = ActionChain::new();
        close_chain.add_delay_seconds(w, f64::from(CLOSE_INTERVAL));
        close_chain.add_action(Actor::Object(this), move |w| check_close(w, this));
        close_chain.enqueue_chain(w);
    }

    let info = WorldObjectInfo::new(w, player);
    fields_mut(w, this).last_player_info = Some(info);
}

/// `new Vendor(weenie, guid)` / `new Vendor(biota)`: the `Creature` constructor, then
/// Vendor's `SetEphemeralValues`.
// ACE: Vendor.Vendor
pub fn vendor_ctor(o: &mut WorldObject, env: &CtorEnv<'_>, src: CtorSource) {
    crate::world_objects::creature::creature_ctor(o, env, src);
    vendor_set_ephemeral_values(o, env);
}

// ACE: Vendor.SetEphemeralValues
fn vendor_set_ephemeral_values(o: &mut WorldObject, env: &CtorEnv<'_>) {
    o.wo.world_object.object_description_flags |= ObjectDescriptionFlag::Vendor;

    if !property_manager::get_bool(env.w, "vendor_shop_uses_generator", false, true).item {
        o.wo.world_object_generators
            .generator_profiles
            .retain(|p| !p.biota.where_create.contains(RegenLocationType::Shop));
    }

    let open_for_business = validate_vendor_requirements(o, env.get_cached_weenie);
    o.set_open_for_business(open_for_business);
}

/// `ValidateVendorRequirements()`; `get_cached_weenie` is `DatabaseManager.World.GetCachedWeenie`
/// (the constructor's environment, or the world's content).
// ACE: Vendor.ValidateVendorRequirements
fn validate_vendor_requirements(
    o: &WorldObject,
    get_cached_weenie: &dyn Fn(u32) -> Option<Arc<Weenie>>,
) -> bool {
    let mut success = true;

    let name = o.get_property(PropertyString::Name).unwrap_or_default();
    let (guid, wcid) = (o.guid, o.biota.weenie_class_id);

    let currency_wcid = o
        .alternate_currency()
        .unwrap_or(u32::from(WeenieClassName::W_COINSTACK_CLASS.0));
    let currency_weenie = get_cached_weenie(currency_wcid);
    if currency_weenie.is_none() {
        let error_msg = format!(
            "WCID {currency_wcid}{} is not found in the database, Vendor has been disabled as a result!",
            if o.alternate_currency().is_some() { ", which comes from PropertyDataId.AlternateCurrency," } else { "" }
        );
        log::error!("[VENDOR] {name} (0x{guid}:{wcid}) Currency {error_msg}");
        success = false;
    }

    if o.merchandise_item_types().is_none() {
        log::error!("[VENDOR] {name} (0x{guid}:{wcid}) MerchandiseItemTypes is NULL, Vendor has been disabled as a result!");
        success = false;
    }

    if o.merchandise_min_value().is_none() {
        log::error!("[VENDOR] {name} (0x{guid}:{wcid}) MerchandiseMinValue is NULL, Vendor has been disabled as a result!");
        success = false;
    }

    if o.merchandise_max_value().is_none() {
        log::error!("[VENDOR] {name} (0x{guid}:{wcid}) MerchandiseMaxValue is NULL, Vendor has been disabled as a result!");
        success = false;
    }

    if o.buy_price().is_none() {
        log::error!("[VENDOR] {name} (0x{guid}:{wcid}) BuyPrice is NULL, Vendor has been disabled as a result!");
        success = false;
    }

    if o.sell_price().is_none() {
        log::error!("[VENDOR] {name} (0x{guid}:{wcid}) SellPrice is NULL, Vendor has been disabled as a result!");
        success = false;
    }

    success
}

/// Populates this vendor's DefaultItemsForSale
// ACE: Vendor.LoadInventory
pub fn load_inventory(w: &mut World, this: ObjectGuid) {
    if fields(w, this).inventoryloaded {
        return;
    }

    // `itemsForSale` is only handed to LoadInventoryItem, whose uses of it are commented out.

    let shop_items: Vec<_> = obj(w, this)
        .biota
        .properties_create_list
        .iter()
        .flat_map(|l| l.iter())
        .filter(|x| x.destination_type == DestinationType::Shop)
        .map(|x| {
            (
                x.weenie_class_id,
                i32::from(x.palette),
                x.shade,
                x.stack_size,
            )
        })
        .collect();

    for (weenie_class_id, palette, shade, stack_size) in shop_items {
        load_inventory_item(
            w,
            this,
            weenie_class_id,
            Some(palette),
            Some(shade),
            Some(stack_size),
        );
    }

    //if (Biota.PropertiesGenerator != null && !PropertyManager.GetBool("vendor_shop_uses_generator").Item)
    //{
    //    foreach (var item in Biota.PropertiesGenerator.Where(x => x.WhereCreate.HasFlag(RegenLocationType.Shop)))
    //        LoadInventoryItem(itemsForSale, item.WeenieClassId, (int?)item.PaletteId, item.Shade, item.StackSize);
    //}

    fields_mut(w, this).inventoryloaded = true;
}

// ACE: Vendor.LoadInventoryItem
fn load_inventory_item(
    w: &mut World,
    this: ObjectGuid,
    weenie_class_id: u32,
    palette: Option<i32>,
    shade: Option<f32>,
    stack_size: Option<i32>,
) {
    //var itemProfile = (weenieClassId, palette ?? 0, shade ?? 0);

    // let's skip dupes if there are any
    //if (itemsForSale.ContainsKey(itemProfile))
    //    return;

    let Some(wo) = create_new_world_object(w, weenie_class_id) else {
        return;
    };

    // Lifted comparisons: a null operand compares false.
    if palette.is_some_and(|p| p > 0) {
        obj_mut(w, wo).set_palette_template(palette);
    }

    if shade.is_some_and(|s| s > 0.0) {
        obj_mut(w, wo).set_shade(shade.map(f64::from));
    }

    obj_mut(w, wo).set_container_id(Some(this.full()));

    let _ = dispatch::calculate_obj_desc::calculate_obj_desc(w, wo);

    //itemsForSale.Add(itemProfile, wo.Guid.Full);

    obj_mut(w, wo)
        .wo
        .world_object_properties
        .vendor_shop_create_list_stack_size = Some(stack_size.unwrap_or(-1));

    fields_mut(w, this).default_items_for_sale.add(wo, ());
}

// ACE: Vendor.AddDefaultItem
pub fn add_default_item(w: &mut World, this: ObjectGuid, item: ObjectGuid) {
    let item_wcid = obj(w, item).biota.weenie_class_id;
    let existing = get_default_items_by_wcid(w, this, item_wcid);

    // add to existing stack?
    if !existing.is_empty() {
        let stack_left = existing.into_iter().find(|&i| {
            let o = obj(w, i);
            o.stack_size().unwrap_or(1) < o.max_stack_size().map_or(1, i32::from)
        });
        if let Some(stack_left) = stack_left {
            let size = obj(w, stack_left).stack_size().unwrap_or(1).wrapping_add(1);
            obj_mut(w, stack_left).set_stack_size(Some(size));
            return;
        }
    }

    // create new item
    obj_mut(w, item).set_container_id(Some(this.full()));

    let _ = dispatch::calculate_obj_desc::calculate_obj_desc(w, item);

    fields_mut(w, this).default_items_for_sale.add(item, ());
}

/// Helper function to replace the previous 'AllItemsForSale' combiner
/// While AllItemsForSale was a useful concept, it was only used in 2 places, and was inefficient
///
/// The items in `forEachItem`'s order (the default items, then the unique ones), for the caller to
/// run its action over.
// ACE: Vendor.forEachItem
#[must_use]
pub fn for_each_item(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    let f = fields(w, this);
    f.default_items_for_sale
        .keys()
        .chain(f.unique_items_for_sale.keys())
        .copied()
        .collect()
}

// ACE: Vendor.GetDefaultItemsByWcid
#[must_use]
pub fn get_default_items_by_wcid(w: &World, this: ObjectGuid, wcid: u32) -> Vec<ObjectGuid> {
    fields(w, this)
        .default_items_for_sale
        .keys()
        .copied()
        .filter(|&i| obj(w, i).biota.weenie_class_id == wcid)
        .collect()
}

/// Searches the vendor's inventory for an item
// ACE: Vendor.TryGetItemForSale
#[must_use]
pub fn try_get_item_for_sale(
    w: &World,
    this: ObjectGuid,
    item_guid: ObjectGuid,
) -> Option<ObjectGuid> {
    let f = fields(w, this);
    (f.default_items_for_sale.contains_key(&item_guid)
        || f.unique_items_for_sale.contains_key(&item_guid))
    .then_some(item_guid)
}

// ACE: Vendor.PrepareResetToHome
fn prepare_reset_to_home(w: &mut World, this: ObjectGuid) {
    // Reset to Home position
    let reset_interval = obj(w, this).reset_interval().unwrap_or(300.0);
    let reset_timestamp = Time::get_unix_time_at(w.now.utc.add_seconds(reset_interval));
    obj_mut(w, this).set_reset_timestamp(Some(reset_timestamp));

    let mut auto_reset_timer = ActionChain::new();
    auto_reset_timer.add_delay_seconds(w, reset_interval);
    auto_reset_timer.add_action(Actor::Object(this), move |w| check_reset_to_home(w, this));
    auto_reset_timer.enqueue_chain(w);
}

/// Sends the latest vendor inventory list to player, rotates vendor towards player, and performs
/// the appropriate emote. ACE's defaults: `action = VendorType.Undef`, `altCurrencySpent = 0`.
// ACE: Vendor.ApproachVendor
pub fn approach_vendor(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
    action: VendorType,
    alt_currency_spent: u32,
) {
    rot_uniques(w, this);

    let session = player_session(w, player).expect("System.NullReferenceException: player.Session");
    let m = game_event_approach_vendor(w, session, this, alt_currency_spent);
    enqueue_send(w, session, m);

    let _rotate_time = dispatch::rotate::rotate(w, this, player); // vendor rotates to player

    if action != VendorType::Undef {
        do_vendor_emote(w, this, action, player);
    }

    obj_mut(w, player)
        .player
        .as_mut()
        .expect("System.InvalidCastException: not a Player")
        .player_use
        .last_opened_container_id = this;

    prepare_reset_to_home(w, this);
}

// ACE: Vendor.DoVendorEmote
pub fn do_vendor_emote(
    w: &mut World,
    this: ObjectGuid,
    vendor_type: VendorType,
    player: ObjectGuid,
) {
    use crate::world_objects::managers::emote_manager;

    match vendor_type {
        VendorType::Open => emote_manager::do_vendor_emote(w, this, vendor_type, Some(player)),

        // player buys item from vendor
        VendorType::Buy => emote_manager::do_vendor_emote(w, this, vendor_type, Some(player)),

        // player sells item to vendor
        VendorType::Sell => emote_manager::do_vendor_emote(w, this, vendor_type, Some(player)),

        _ => log::warn!(
            "Vendor.DoVendorEmote - Encountered Unhandled VendorType {vendor_type:?} for {} ({})",
            name(w, this),
            obj(w, this).biota.weenie_class_id
        ),
    }
}

// ACE: Vendor.closeInterval
const CLOSE_INTERVAL: f32 = 1.5;

/// After a player approaches a vendor, this is called every closeInterval seconds
/// to see if the player is still within the UseRadius of the vendor.
///
/// If the player has moved away, the vendor Close emote is called (waving goodbye, saying farewell)
// ACE: Vendor.CheckClose
pub fn check_close(w: &mut World, this: ObjectGuid) {
    let Some(info) = fields(w, this).last_player_info.clone() else {
        return;
    };

    let last_player = info
        .try_get_world_object(w)
        .filter(|&g| obj(w, g).is_player());

    let Some(last_player) = last_player else {
        fields_mut(w, this).last_player_info = None;
        return;
    };

    // handles player logging out at vendor
    if obj(w, last_player).current_landblock.is_none() {
        fields_mut(w, this).last_player_info = None;
        return;
    }

    let dist = shims::get_cylinder_distance(w, this, last_player);

    // Lifted comparison: a null UseRadius compares false.
    if obj(w, this).use_radius().is_some_and(|r| dist > r) {
        let p = obj_mut(w, last_player)
            .player
            .as_mut()
            .expect("System.InvalidCastException: not a Player");
        if p.player_use.last_opened_container_id == this {
            p.player_use.last_opened_container_id = ObjectGuid::INVALID;
        }

        crate::world_objects::managers::emote_manager::do_vendor_emote(
            w,
            this,
            VendorType::Close,
            Some(last_player),
        );
        fields_mut(w, this).last_player_info = None;

        return;
    }

    let mut close_chain = ActionChain::new();
    close_chain.add_delay_seconds(w, f64::from(CLOSE_INTERVAL));
    close_chain.add_action(Actor::Object(this), move |w| check_close(w, this));
    close_chain.enqueue_chain(w);
}

// ACE: Vendor.CheckResetToHome
pub fn check_reset_to_home(w: &mut World, this: ObjectGuid) {
    // `Time.GetUnixTime() >= ResetTimestamp`: a null timestamp compares false.
    if obj(w, this)
        .reset_timestamp()
        .is_some_and(|t| w.now.unix_time >= t)
    {
        let o = obj(w, this);
        let location = o
            .location()
            .expect("System.NullReferenceException: Location");
        let home = o.home().expect("System.NullReferenceException: Home");

        // are we already at home origin?
        if location.pos() == home.pos() {
            // just turnto if required?
            if location.rotation() != home.rotation() {
                shims::turn_to_position(w, this, &home);
            }
        } else {
            let run_rate = monster_navigation::get_run_rate(w, this);
            creature_navigation::creature_move_to_position(
                w,
                this,
                &home,
                run_rate,
                true,
                None,
                Some(1.0),
            );
        }
    }
}

/// Creates world objects for generic items
// ACE: Vendor.ItemProfileToWorldObjects
pub fn item_profile_to_world_objects(w: &mut World, item_profile: &ItemProfile) -> Vec<ObjectGuid> {
    let mut results = Vec::new();

    let mut remaining = item_profile.amount;

    while remaining > 0 {
        let wo = create_new_world_object(w, item_profile.weenie_class_id)
            .expect("System.NullReferenceException: wo");

        if item_profile.palette.is_some() {
            obj_mut(w, wo).set_palette_template(item_profile.palette);
        }

        if item_profile.shade.is_some() {
            obj_mut(w, wo).set_shade(item_profile.shade);
        }

        let max_stack_size = obj(w, wo).max_stack_size().map(i32::from);
        if let Some(max_stack_size) = max_stack_size.filter(|&m| m > 0) {
            // stackable
            let current_stack_size = remaining.min(max_stack_size);

            obj_mut(w, wo).set_stack_size(Some(current_stack_size));
            results.push(wo);
            remaining = remaining.wrapping_sub(current_stack_size);
        } else {
            // non-stackable
            obj_mut(w, wo).set_stack_size_prop(None);
            results.push(wo);
            remaining = remaining.wrapping_sub(1);
        }
    }
    results
}

/// Handles validation for player buying items from vendor
// ACE: Vendor.BuyItems_ValidateTransaction
pub fn buy_items_validate_transaction(
    w: &mut World,
    this: ObjectGuid,
    item_profiles: &mut [ItemProfile],
    player: ObjectGuid,
) -> bool {
    // one difference between buy and sell currently
    // is that if *any* items in the buy transactions are detected as invalid,
    // we reject the entire transaction.
    // this seems to be the "safest" route, however in terms of player convenience
    // where only 1 item has an error from a large purchase set,
    // this might not be the most convenient for the player.

    let mut default_item_profiles: Vec<ItemProfile> = Vec::new();
    let mut unique_items: Vec<ObjectGuid> = Vec::new();

    // find item profiles in default and unique items
    for item_profile in item_profiles.iter_mut() {
        if !item_profile.is_valid_amount() {
            // reject entire transaction immediately
            player_networking::send_transient_error(w, player, "Invalid amount");
            return false;
        }

        let item_guid = ObjectGuid::new(item_profile.object_guid);

        // check default items
        if fields(w, this)
            .default_items_for_sale
            .contains_key(&item_guid)
        {
            let default_item_for_sale = obj(w, item_guid);
            item_profile.weenie_class_id = default_item_for_sale.biota.weenie_class_id;
            item_profile.palette = default_item_for_sale.palette_template();
            item_profile.shade = default_item_for_sale.shade();

            default_item_profiles.push(item_profile.clone());
        }
        // check unique items
        else if fields(w, this)
            .unique_items_for_sale
            .contains_key(&item_guid)
        {
            unique_items.push(item_guid);
        }
    }

    // ensure player has enough free inventory slots / container slots / available burden to receive items
    let mut items_to_receive = ItemsToReceive::new(w, player);

    for default_item_profile in &default_item_profiles {
        items_to_receive.add(
            w,
            default_item_profile.weenie_class_id,
            default_item_profile.amount,
        );

        if items_to_receive.player_exceeds_limits() {
            break;
        }
    }

    if !items_to_receive.player_exceeds_limits() {
        for &unique_item in &unique_items {
            let (wcid, stack_size) = {
                let o = obj(w, unique_item);
                (o.biota.weenie_class_id, o.stack_size().unwrap_or(1))
            };
            items_to_receive.add(w, wcid, stack_size);

            if items_to_receive.player_exceeds_limits() {
                break;
            }
        }
    }

    if items_to_receive.player_exceeds_limits() {
        if items_to_receive.player_exceeds_available_burden() {
            transient(w, player, "You are too encumbered to buy that!");
        } else if items_to_receive.player_out_of_inventory_slots() {
            transient(w, player, "You do not have enough pack space to buy that!");
        } else if items_to_receive.player_out_of_container_slots() {
            transient(
                w,
                player,
                "You do not have enough container slots to buy that!",
            );
        }

        return false;
    }

    // ideally the creation of the wo's would be delayed even further,
    // and all validations would be performed on weenies beforehand
    // this would require:
    // - a forEach helper function to iterate through both defaultItemProfiles (ItemProfiles) and uniqueItems (WorldObjects),
    //   so that 2 foreach iterators don't have to be written each time
    // - weenie to have more functions that mimic the functionality of WorldObject

    // create world objects for default items
    let mut default_items: Vec<ObjectGuid> = Vec::new();

    for default_item_profile in &default_item_profiles {
        default_items.extend(item_profile_to_world_objects(w, default_item_profile));
    }

    let purchase_items: Vec<ObjectGuid> = default_items
        .iter()
        .chain(unique_items.iter())
        .copied()
        .collect();

    if obj(w, this).wo.world_object.is_busy
        && purchase_items
            .iter()
            .any(|&i| obj(w, i).get_property(PropertyBool::VendorService) == Some(true))
    {
        let vendor_name = name(w, this);
        player_networking::send_weenie_error_with_string(
            w,
            player,
            WeenieErrorWithString::_IsTooBusyToAcceptGifts,
            &vendor_name,
        );
        cleanup_created_items(w, &default_items);
        return false;
    }

    // check uniques
    if !player_inventory::check_uniques(w, player, &purchase_items, Some(this)) {
        cleanup_created_items(w, &default_items);
        return false;
    }

    // calculate price
    let mut total_price: u32 = 0;

    for &item in &purchase_items {
        let cost = get_sell_cost(w, this, item);

        // detect rollover?
        total_price = total_price.wrapping_add(cost);
    }

    // verify player has enough currency
    if let Some(alternate_currency) = obj(w, this).alternate_currency() {
        let player_alt_currency =
            container::get_num_inventory_items_of_wcid(w, player, alternate_currency);

        // `int < uint`: both widened to long.
        if i64::from(player_alt_currency) < i64::from(total_price) {
            cleanup_created_items(w, &default_items);
            return false;
        }
    } else {
        // `player.CoinValue < totalPrice`: lifted over long, so a null CoinValue compares false.
        if obj(w, player)
            .coin_value()
            .is_some_and(|c| i64::from(c) < i64::from(total_price))
        {
            cleanup_created_items(w, &default_items);
            return false;
        }
    }

    // everything is verified at this point

    // send transaction to player for further processing
    player_commerce::finalize_buy_transaction(
        w,
        player,
        this,
        &default_items,
        &unique_items,
        total_price,
    );

    true
}

/// `GetSellCost(WorldObject item)`: `GetSellCost(item.Value, item.ItemType)`.
// ACE: Vendor.GetSellCost
#[must_use]
pub fn get_sell_cost(w: &World, this: ObjectGuid, item: ObjectGuid) -> u32 {
    let (value, stack_size, item_type) = {
        let o = obj(w, item);
        (o.value(), o.stack_size(), o.item_type())
    };
    get_sell_cost_of(
        obj(w, this).sell_price(),
        value,
        stack_size,
        Some(item_type),
    )
}

/// `GetSellCost(Weenie item)`: `GetSellCost(item.GetValue(), item.GetItemType())`.
// ACE: Vendor.GetSellCost
#[must_use]
pub fn get_sell_cost_weenie(w: &World, this: ObjectGuid, item: &Weenie) -> u32 {
    get_sell_cost_of(
        obj(w, this).sell_price(),
        item.get_value(),
        item.get_property(PropertyInt::StackSize),
        Some(item.get_item_type()),
    )
}

/// The private `GetSellCost(int? value, ItemType? itemType)`, over the vendor's `SellPrice`.
///
/// **Retail's price, not ACE's (V225).** ACE rounds
/// `(float)rate * value` to single precision before the 0.1 fudge, so at rounding boundaries it
/// charged one pyreal more or less than the vendor window shows. The server now prices exactly
/// as the client does (`dereth_rules::vendor::sell_price`): the unit value (the stack's value over
/// its size, integer division) times the count, in double precision. The trade-note rate and
/// ACE's `max(1)` floor are unchanged (the client's result is never below 1 for a non-negative
/// value). A price past `i32::MAX` is out of the client's range (its formula returns -1 there);
/// the server then keeps ACE's saturating result rather than let an item sell for 1 pyreal.
// ACE: Vendor.GetSellCost
#[must_use]
pub fn get_sell_cost_of(
    sell_price: Option<f64>,
    value: Option<i32>,
    stack_size: Option<i32>,
    item_type: Option<ItemType>,
) -> u32 {
    let mut sell_rate = sell_price.unwrap_or(1.0);
    if item_type == Some(ItemType::PromissoryNote) {
        sell_rate = 1.15;
    }
    let rate: f32 = sell_rate.cs_cast();
    let (unit, count) = unit_and_count(value.unwrap_or(0), stack_size);
    if !in_client_range(rate, unit, count) {
        // ACE's formula: `(uint)Math.Ceiling(((float)sellRate * (value ?? 0)) - 0.1)`, then `max(1)`.
        let value: f32 = value.unwrap_or(0).cs_cast();
        let cost: u32 = (f64::from(rate * value) - 0.1).ceil().cs_cast();
        return cost.max(1);
    }
    let cost = dereth_rules::vendor::sell_price(unit, item_type_bits(item_type), rate, count);
    u32::try_from(cost).unwrap_or(0).max(1)
}

/// Whether the client's price chain stays inside `i32` for this rate, unit value and count.
fn in_client_range(rate: f32, unit: i32, count: i32) -> bool {
    (f64::from(rate) * f64::from(unit) * f64::from(count)).abs() + 1.0 < f64::from(i32::MAX)
}

/// What the client passes its price formulas: the value of one unit and the number of units.
/// A stack's value is the whole stack's, so it is divided by the stack size first, as the client
/// does; an item with no stack size is one unit of its whole value.
fn unit_and_count(value: i32, stack_size: Option<i32>) -> (i32, i32) {
    match stack_size {
        Some(n) if n != 0 => (value.wrapping_div(n), n),
        _ => (value, 1),
    }
}

/// The client's price formulas test only for trade notes; any other type prices at the rate.
fn item_type_bits(item_type: Option<ItemType>) -> u32 {
    if item_type == Some(ItemType::PromissoryNote) {
        dereth_rules::weenie::item_type::PROMISSORY_NOTE
    } else {
        0
    }
}

/// `GetBuyCost(WorldObject item)`: `GetBuyCost(item.Value, item.ItemType)`.
// ACE: Vendor.GetBuyCost
#[must_use]
pub fn get_buy_cost(w: &World, this: ObjectGuid, item: ObjectGuid) -> i32 {
    let (value, stack_size, item_type) = {
        let o = obj(w, item);
        (o.value(), o.stack_size(), o.item_type())
    };
    get_buy_cost_of(obj(w, this).buy_price(), value, stack_size, Some(item_type))
}

/// `GetBuyCost(Weenie item)`: `GetBuyCost(item.GetValue(), item.GetItemType())`.
// ACE: Vendor.GetBuyCost
#[must_use]
pub fn get_buy_cost_weenie(w: &World, this: ObjectGuid, item: &Weenie) -> i32 {
    get_buy_cost_of(
        obj(w, this).buy_price(),
        item.get_value(),
        item.get_property(PropertyInt::StackSize),
        Some(item.get_item_type()),
    )
}

/// The private `GetBuyCost(int? value, ItemType? itemType)`, over the vendor's `BuyPrice`.
///
/// **Retail's price, not ACE's (V225)**: what the vendor pays
/// is `dereth_rules::vendor::buy_price` over the unit value and count, in double precision, as the
/// client computes the price it shows. See [`get_sell_cost_of`], including the out-of-range case.
/// ACE's `max(1)` is kept.
// ACE: Vendor.GetBuyCost
#[must_use]
pub fn get_buy_cost_of(
    buy_price: Option<f64>,
    value: Option<i32>,
    stack_size: Option<i32>,
    item_type: Option<ItemType>,
) -> i32 {
    let mut buy_rate = buy_price.unwrap_or(1.0);
    if item_type == Some(ItemType::PromissoryNote) {
        buy_rate = 1.0;
    }
    let rate: f32 = buy_rate.cs_cast();
    let (unit, count) = unit_and_count(value.unwrap_or(0), stack_size);
    if !in_client_range(rate, unit, count) {
        // ACE's formula: `(int)Math.Floor(((float)buyRate * (value ?? 0)) + 0.1)`, then `max(1)`.
        let value: f32 = value.unwrap_or(0).cs_cast();
        let cost: i32 = (f64::from(rate * value) + 0.1).floor().cs_cast();
        return cost.max(1);
    }
    dereth_rules::vendor::buy_price(unit, item_type_bits(item_type), rate, count).max(1)
}

/// `items` are the values of ACE's `Dictionary<uint, WorldObject>`, in its order.
// ACE: Vendor.CalculatePayoutCoinAmount
#[must_use]
pub fn calculate_payout_coin_amount(w: &World, this: ObjectGuid, items: &[ObjectGuid]) -> i32 {
    let mut payout: i32 = 0;

    for &item in items {
        payout = payout.wrapping_add(get_buy_cost(w, this, item));
    }

    payout
}

/// This will either add the item to the vendors temporary sellables, or destroy it.
/// In both cases, the item will be removed from the database.
/// The item should already have been removed from the players inventory
///
/// `items` are the values of ACE's `Dictionary<uint, WorldObject>`, in its order.
// ACE: Vendor.ProcessItemsForPurchase
pub fn process_items_for_purchase(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
    items: &[ObjectGuid],
) {
    for &item in items {
        let mut resell_item = true;

        let o = obj(w, item);

        // don't resell DestroyOnSell
        if o.get_property(PropertyBool::DestroyOnSell).unwrap_or(false) {
            resell_item = false;
        }

        // don't resell Attuned items that can be sold
        if o.attuned() == Some(AttunedStatus::Attuned) {
            resell_item = false;
        }

        // don't resell stackables?
        if o.max_stack_size().is_some() || o.max_structure().is_some() {
            resell_item = false;
        }

        if resell_item {
            obj_mut(w, item).set_container_id(Some(this.full()));

            if !fields_mut(w, this).unique_items_for_sale.try_add(item, ()) {
                let sell_items = items
                    .iter()
                    .map(|&i| format!("{} ({i})", name(w, i)))
                    .collect::<Vec<_>>()
                    .join(", ");
                log::error!("[VENDOR] {}.ProcessItemsForPurchase({}): duplicate item found, sell list: {sell_items}", name(w, this), name(w, player));
            }

            let now = w.now.unix_time;
            obj_mut(w, item).set_sold_timestamp(Some(now));

            // verify no gap: even though the guid is technically free in the database at this point,
            // is it still marked as consumed in guid manager, and not marked as freed here?
            // if player repurchases item sometime later, we must ensure the guid is still marked as consumed for re-add

            // remove object from shard db, but keep a reference to it in memory
            // for DestroyOnSell items, these will effectively be destroyed immediately
            // for other items, if a player re-purchases, it will be added to the shard db again
            world_object_database::remove_biota_from_database(w, item, true);
        } else {
            destroy(w, item);
        }

        let n = obj(w, this).num_items_bought().wrapping_add(1);
        obj_mut(w, this).set_num_items_bought(n);
    }

    approach_vendor(w, this, player, VendorType::Sell, 0);
}

// ACE: Vendor.ApplyService
pub fn apply_service(w: &mut World, this: ObjectGuid, item: ObjectGuid, target: ObjectGuid) {
    // verify -- players purchasing multiple services in 1 transaction, and IsBusy state?
    let spell_did = obj(w, item).spell_did().unwrap_or(0);
    let spell = Spell::new(w, spell_did, true);

    if spell.not_found() {
        return;
    }

    obj_mut(w, this).wo.world_object.is_busy = true;

    let pre_cast_time = monster_magic::pre_cast_motion(w, this, target, false);

    let mut cast_chain = ActionChain::new();
    cast_chain.add_delay_seconds(w, f64::from(pre_cast_time));

    let post_cast_time = monster_magic::get_post_cast_time(w, this, &spell, false);

    cast_chain.add_action(Actor::Object(this), move |w| {
        world_object_magic::try_cast_spell(
            w,
            this,
            &spell,
            Some(target),
            Some(this),
            None,
            false,
            false,
            true,
        );
        monster_magic::post_cast_motion(w, this);
    });

    cast_chain.add_delay_seconds(w, f64::from(post_cast_time));
    cast_chain.add_action(Actor::Object(this), move |w| {
        if let Some(o) = w.objects.get_mut(this) {
            o.wo.world_object.is_busy = false;
        }
    });

    cast_chain.enqueue_chain(w);

    let n = obj(w, this).num_services_sold().wrapping_add(1);
    obj_mut(w, this).set_num_services_sold(n);
}

/// Unique items in the vendor's inventory sold to the vendor by players
/// expire after vendor_unique_rot_time seconds
// ACE: Vendor.RotUniques
pub fn rot_uniques(w: &mut World, this: ObjectGuid) {
    let mut items_to_remove: Option<Vec<ObjectGuid>> = None;

    for unique_item in unique_items_for_sale(w, this) {
        let Some(sold_time) = obj(w, unique_item).sold_timestamp() else {
            log::warn!(
                "[VENDOR] Vendor {} has unique item {} ({unique_item}) without a SoldTimestamp -- this shouldn't happen",
                name(w, this),
                name(w, unique_item)
            );
            continue; // keep in list?
        };

        let mut rot_time = Time::get_date_time_from_timestamp(sold_time);

        rot_time = rot_time.add_seconds(
            property_manager::get_double(w, "vendor_unique_rot_time", 300.0, true).item,
        );

        if w.now.utc >= rot_time {
            items_to_remove
                .get_or_insert_with(Vec::new)
                .push(unique_item);
        }
    }
    if let Some(items_to_remove) = items_to_remove {
        for item_to_remove in items_to_remove {
            log::debug!(
                "[VENDOR] Vendor {} has discontinued sale of {} and removed it from its UniqueItemsForSale list.",
                name(w, this),
                name(w, item_to_remove)
            );
            fields_mut(w, this)
                .unique_items_for_sale
                .remove(&item_to_remove);

            destroy(w, item_to_remove); // even though it has already been removed from the db at this point, we want to mark as freed in guid manager now
        }
    }
}

// ACE: Vendor.CleanupCreatedItems
fn cleanup_created_items(w: &mut World, created_items: &[ObjectGuid]) {
    for &created_item in created_items {
        destroy(w, created_item);
    }
}

/// Members of other ACE files, each marked `SHIM:` (no anchor).
pub(crate) mod shims {
    use empyrean_entity::{ObjectGuid, Position};

    use super::World;

    /// SHIM: `WorldObject.GetCylinderDistance(wo)` (`WorldObject_Use.cs`, 4.8b's port), answering
    /// +infinity when either object has no physics body (5.7's recorded divergence; ACE throws).
    pub(crate) fn get_cylinder_distance(w: &World, this: ObjectGuid, wo: ObjectGuid) -> f32 {
        let has_body = |g: ObjectGuid| crate::physics::phys_ext::physics_obj(w, g).is_some();
        if !has_body(this) || !has_body(wo) {
            return f32::INFINITY;
        }
        crate::world_objects::world_object_use::get_cylinder_distance(w, this, wo)
    }

    /// SHIM: `Creature.TurnTo(Position)` (`Creature_Navigation.cs`).
    pub(crate) fn turn_to_position(w: &mut World, this: ObjectGuid, position: &Position) {
        let _ = crate::world_objects::creature_navigation::turn_to_position(w, this, position);
    }
}
