// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Commerce.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Commerce.cs`.
//!
//! The player's side of a vendor transaction (buy, sell) and the coin helpers
//! (`UpdateCoinValue`, `SpendCurrency`, `CollectCurrencyStacks`, `CreatePayoutCoinStacks`).
//!
//! New objects (bought stacks, payout coins, split coin stacks) are put in `World.objects` as they
//! are created, as `player_inventory.rs` does. Members of unowned files are in [`shims`].

use empyrean_common::dotnet::{CsCast, DotNetDict, DotNetHashSet};
use empyrean_entity::enums::{
    ItemType, PropertyBool, PropertyInt, Sound, VendorType, WeenieClassName, WeenieError,
    WeenieType,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;

use crate::entity::item_profile::ItemProfile;
use crate::entity::items_to_receive::ItemsToReceive;
use crate::managers::player_manager::player_session;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_inventory_server_save_failed::game_event_inventory_server_save_failed;
use crate::network::game_event::events::game_event_item_server_says_contain_id::game_event_item_server_says_contain_id;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_set_stack_size::game_message_set_stack_size;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::sessions::SessionData;
use crate::world_objects::player_inventory::{
    self, DequipObjectAction, RemoveFromInventoryAction, SearchLocations,
};
use crate::world_objects::world_object::{self, WorldObject};
use crate::world_objects::{container, player_networking, vendor};
use crate::{dispatch, World};

/// Non-property fields declared in `Player_Commerce.cs`.
#[derive(Debug, Default)]
pub struct PlayerCommerceFields {}

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

/// `Name` (a null name interpolates as empty).
fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

/// `Player.Session`: `None` when it has none (nothing is sent).
fn session(w: &World, this: ObjectGuid) -> Option<SessionId> {
    player_session(w, this)
}

/// `Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    if let Some(s) = session(w, this) {
        enqueue_send(w, s, msg);
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

/// `Session.Network.EnqueueSend(new GameEventInventoryServerSaveFailed(Session, Guid.Full))` (the
/// default `WeenieError.None`).
fn save_failed(w: &mut World, this: ObjectGuid) {
    if let Some(m) = event(w, this, |d| {
        game_event_inventory_server_save_failed(d, this.full(), WeenieError::None)
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

/// `new GameMessagePrivateUpdatePropertyInt(this, property, value)`, sent.
fn send_private_int(w: &mut World, this: ObjectGuid, property: PropertyInt, value: i32) {
    let m = game_message_private_update_property_int(obj_mut(w, this), property, value);
    send(w, this, m);
}

/// `WorldObjectFactory.CreateNewWorldObject(wcid)`, put in `World.objects` at once.
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

/// `CurrentLandblock?.GetObject(vendorGuid) as Vendor`.
fn vendor_on_landblock(w: &World, this: ObjectGuid, vendor_guid: u32) -> Option<ObjectGuid> {
    let lb = obj(w, this).current_landblock?;
    crate::entity::landblock::get_object(w, lb, ObjectGuid::new(vendor_guid), true)
        .filter(|&g| obj(w, g).is_vendor())
}

/// Not ACE's (fix, V315): whether the player may trade with `vendor` now: the
/// vendor is open for business and the player stands within its use radius (the reach the client
/// keeps its vendor window open for). Either missing, the vendor is treated as not there.
fn vendor_open_and_in_reach(w: &World, this: ObjectGuid, vendor: ObjectGuid) -> bool {
    if !obj(w, vendor).open_for_business() {
        return false;
    }
    let has_body = |g: ObjectGuid| crate::physics::phys_ext::physics_obj(w, g).is_some();
    has_body(this)
        && has_body(vendor)
        && crate::world_objects::world_object_use::is_within_use_radius_of(w, this, vendor, None)
}

/// Not ACE's (fix, V315): the vendor's value range, as the client's sell
/// check reads it. The unit value (the stack's value over its size) must not be above
/// `MerchandiseMaxValue` (a promissory note is exempt) nor below `MerchandiseMinValue`; -1 or no
/// value is no limit.
fn within_vendor_value_range(w: &World, vendor: ObjectGuid, item: ObjectGuid) -> bool {
    let (v, o) = (obj(w, vendor), obj(w, item));
    let value = i64::from(o.value().unwrap_or(0));
    let unit = match o.stack_size() {
        Some(n) if n != 0 => value / i64::from(n),
        _ => value,
    };
    let limit = |l: Option<i32>| l.filter(|&l| l != -1).map(i64::from);
    if let Some(max) = limit(v.merchandise_max_value()) {
        if unit > max && o.item_type().0 & ItemType::PromissoryNote.0 == 0 {
            return false;
        }
    }
    if let Some(min) = limit(v.merchandise_min_value()) {
        if unit < min {
            return false;
        }
    }
    true
}

// ================================================================================ Player_Commerce.cs

// player buying items from vendor

/// Called when player clicks 'Buy Items'
///
/// Not ACE's (fix, V315): a vendor that is not open for business or that the
/// player is not within the use radius of is refused as a vendor that is not there
/// (InventoryServerSaveFailed, then UseDone NoObject), in the buy and the sell handler both; ACE
/// checked neither, so a crafted Buy or Sell traded with any vendor on the player's landblock or
/// an adjacent one.
// ACE: Player.HandleActionBuyItem
pub fn handle_action_buy_item(
    w: &mut World,
    this: ObjectGuid,
    vendor_guid: u32,
    mut items: Vec<ItemProfile>,
) {
    if obj(w, this).wo.world_object.is_busy {
        save_failed(w, this);
        shims::send_use_done_event(w, this, WeenieError::YoureTooBusy);
        return;
    }

    if shims::is_trading(w, this) {
        save_failed(w, this);
        shims::send_use_done_event(w, this, WeenieError::CantDoThatTradeInProgress);
        return;
    }

    let Some(vendor) =
        vendor_on_landblock(w, this, vendor_guid).filter(|&v| vendor_open_and_in_reach(w, this, v))
    else {
        save_failed(w, this);
        shims::send_use_done_event(w, this, WeenieError::NoObject);
        return;
    };

    // if this succeeds, it automatically calls player.FinalizeBuyTransaction()
    if !vendor::buy_items_validate_transaction(w, vendor, &mut items, this) {
        save_failed(w, this);
    }

    shims::send_use_done_event(w, this, WeenieError::None);
}

// ACE: Player.coinStackWcid
const COIN_STACK_WCID: u32 = WeenieClassName::W_COINSTACK_CLASS.0 as u32;

/// Vendor has validated the transactions and sent a list of items for processing.
// ACE: Player.FinalizeBuyTransaction
pub fn finalize_buy_transaction(
    w: &mut World,
    this: ObjectGuid,
    vendor: ObjectGuid,
    generic_items: &[ObjectGuid],
    unique_items: &[ObjectGuid],
    cost: u32,
) {
    // transaction has been validated by this point

    let currency_wcid = obj(w, vendor)
        .alternate_currency()
        .unwrap_or(COIN_STACK_WCID);

    spend_currency(w, this, currency_wcid, cost, true);

    let income = obj(w, vendor)
        .money_income()
        .wrapping_add(cost.cast_signed());
    obj_mut(w, vendor).set_money_income(income);

    for &item in generic_items {
        let service = obj(w, item)
            .get_property(PropertyBool::VendorService)
            .unwrap_or(false);

        if service {
            vendor::apply_service(w, vendor, item, this);

            // DIVERGE: ACE drops the service item unreferenced (the garbage collector's); it leaves
            // `World.objects` here (4.5a's rule for unclaimed new objects).
            w.objects.remove(item);
        } else {
            // errors shouldn't be possible here, since the items were pre-validated, but just in case...
            if player_inventory::try_create_in_inventory_with_networking(w, this, item).is_none() {
                log::error!(
                    "[VENDOR] {}.FinalizeBuyTransaction({}) - couldn't add {} ({item}) to player inventory after validation, this shouldn't happen!",
                    name(w, this),
                    name(w, vendor),
                    name(w, item)
                );

                destroy(w, item); // cleanup for guid manager
            }

            let n = obj(w, vendor).num_items_sold().wrapping_add(1);
            obj_mut(w, vendor).set_num_items_sold(n);
        }
    }

    for &item in unique_items {
        if player_inventory::try_create_in_inventory_with_networking(w, this, item).is_some() {
            vendor::fields_mut(w, vendor)
                .unique_items_for_sale
                .remove(&item);

            // this was only for when the unique item was sold to the vendor,
            // to determine when the item should rot on the vendor. it gets removed now
            obj_mut(w, item).set_sold_timestamp(None);

            let n = obj(w, vendor).num_items_sold().wrapping_add(1);
            obj_mut(w, vendor).set_num_items_sold(n);
        } else {
            log::error!(
                "[VENDOR] {}.FinalizeBuyTransaction({}) - couldn't add {} ({item}) to player inventory after validation, this shouldn't happen!",
                name(w, this),
                name(w, vendor),
                name(w, item)
            );
        }
    }

    send(w, this, game_message_sound(this, Sound::PickUpItem, 1.0));

    if crate::managers::property_manager::get_bool(w, "player_receive_immediate_save", false, true)
        .item
    {
        shims::rush_next_player_save(w, this, 5);
    }

    let alt_currency_spent = if obj(w, vendor).alternate_currency().is_some() {
        cost
    } else {
        0
    };

    vendor::approach_vendor(w, vendor, this, VendorType::Buy, alt_currency_spent);
}

// player selling items to vendor

// whereas most of the logic for buying items is in vendor,
// most of the logic for selling items is located in player_commerce
// the functions have similar structure, just in different places
// there's really no point in there being differences in location,
// and it might be better to move them all to vendor for consistency.

/// Called when player clicks 'Sell Items'
// ACE: Player.HandleActionSellItem
pub fn handle_action_sell_item(
    w: &mut World,
    this: ObjectGuid,
    vendor_guid: u32,
    item_profiles: Vec<ItemProfile>,
) {
    if obj(w, this).wo.world_object.is_busy {
        save_failed(w, this);
        shims::send_use_done_event(w, this, WeenieError::YoureTooBusy);
        return;
    }

    let Some(vendor) =
        vendor_on_landblock(w, this, vendor_guid).filter(|&v| vendor_open_and_in_reach(w, this, v))
    else {
        save_failed(w, this);
        shims::send_use_done_event(w, this, WeenieError::NoObject);
        return;
    };

    // perform validations on requested sell items,
    // and filter to list of validated items

    // one difference between sell and buy is here.
    // when an itemProfile is invalid in buy, the entire transaction is failed immediately.
    // when an itemProfile is invalid in sell, we just remove the invalid itemProfiles, and continue onwards
    // this might not be the best for safety, and it's a tradeoff between safety and player convenience
    // should we fail the entire transaction (similar to buy), if there are any invalids in the transaction request?

    let sell_list = verify_sell_items(w, this, &item_profiles, vendor);
    let sell_items: Vec<ObjectGuid> = sell_list.values().copied().collect();

    if sell_list.is_empty() {
        save_failed(w, this);
        shims::send_use_done_event(w, this, WeenieError::None);
        return;
    }

    // calculate pyreals to receive
    let payout_coin_amount = vendor::calculate_payout_coin_amount(w, vendor, &sell_items);

    if payout_coin_amount < 0 {
        log::warn!(
            "[VENDOR] {} (0x({this}) tried to sell something to {} (0x{vendor}) resulting in a payout of {payout_coin_amount} pyreals.",
            name(w, this),
            name(w, vendor)
        );

        player_networking::send_transient_error(w, this, "Transaction failed.");
        save_failed(w, this);

        shims::send_use_done_event(w, this, WeenieError::None);

        return;
    }

    // verify player has enough pack slots / burden to receive these pyreals
    let mut items_to_receive = ItemsToReceive::new(w, this);

    items_to_receive.add(w, COIN_STACK_WCID, payout_coin_amount);

    if items_to_receive.player_exceeds_limits() {
        if items_to_receive.player_exceeds_available_burden() {
            transient(w, this, "You are too encumbered to sell that!");
        } else if items_to_receive.player_out_of_inventory_slots() {
            transient(
                w,
                this,
                "You do not have enough free pack space to sell that!",
            );
        }

        save_failed(w, this);
        shims::send_use_done_event(w, this, WeenieError::None); // WeenieError.FullInventoryLocation?
        return;
    }

    let payout_coin_stacks = create_payout_coin_stacks(w, payout_coin_amount);

    let outflow = obj(w, vendor)
        .money_outflow()
        .wrapping_add(payout_coin_amount);
    obj_mut(w, vendor).set_money_outflow(outflow);

    // remove sell items from player inventory
    for &item in &sell_items {
        if player_inventory::try_remove_from_inventory_with_networking(
            w,
            this,
            item,
            RemoveFromInventoryAction::SellItem,
        )
        .is_some()
            || player_inventory::try_dequip_object_with_networking(
                w,
                this,
                item,
                DequipObjectAction::SellItem,
            )
            .is_some()
        {
            if let Some(s) = session(w, this) {
                let item_o = w
                    .objects
                    .get(item)
                    .expect("System.NullReferenceException: item");
                if let Some(data) = w.sessions.get_mut(s) {
                    let m = game_event_item_server_says_contain_id(data, item_o, vendor);
                    enqueue_send(w, s, m);
                }
            }
        } else {
            log::warn!(
                "[VENDOR] Item 0x{:08X}:{} for player {} not found in HandleActionSellItem.",
                item.full(),
                name(w, item),
                name(w, this)
            ); // This shouldn't happen
        }
    }

    // send the list of items to the vendor
    // for the vendor to determine what to do with each item (resell, destroy)
    vendor::process_items_for_purchase(w, vendor, this, &sell_items);

    // add coins to player inventory
    for item in payout_coin_stacks {
        if player_inventory::try_create_in_inventory_with_networking(w, this, item).is_none() {
            // this shouldn't happen because of pre-validations in itemsToReceive
            log::warn!(
                "[VENDOR] Payout 0x{:08X}:{} for player {} failed to add to inventory HandleActionSellItem.",
                item.full(),
                name(w, item),
                name(w, this)
            );
            destroy(w, item);
        }
    }

    // UpdateCoinValue removed -- already handled in TryCreateInInventoryWithNetworking

    send(w, this, game_message_sound(this, Sound::PickUpItem, 1.0));

    shims::send_use_done_event(w, this, WeenieError::None);
}

/// Filters the list of ItemProfiles the player is attempting to sell to the vendor
/// to the list of verified WorldObjects in the player's inventory w/ validations
///
/// ACE's `Dictionary<uint, WorldObject>`: guid -> item, in insertion order.
///
/// Not ACE's (fix, V315): an item outside the vendor's value range
/// ([`within_vendor_value_range`]) is refused as an unsellable one; ACE never checked the range,
/// which the client's sell list enforces. The requested amount stays a range check only: the
/// client sends amount 1 for a whole stack (it splits a part stack off before listing it), so the
/// object named is what is sold.
// ACE: Player.VerifySellItems
pub fn verify_sell_items(
    w: &mut World,
    this: ObjectGuid,
    sell_items: &[ItemProfile],
    vendor: ObjectGuid,
) -> DotNetDict<u32, ObjectGuid> {
    let mut all_possessions: DotNetDict<u32, ObjectGuid> = DotNetDict::new();
    for i in player_inventory::get_all_possessions(w, this) {
        // `ToDictionary` throws on a duplicate key
        assert!(
            all_possessions.try_add(i.full(), i),
            "System.ArgumentException: An item with the same key has already been added."
        );
    }

    let accepted_item_types = obj(w, vendor).merchandise_item_types().unwrap_or(0);

    let mut verified: DotNetDict<u32, ObjectGuid> = DotNetDict::new();

    let player_name = name(w, this);
    let vendor_name = name(w, vendor);

    for sell_item in sell_items {
        let Some(&wo) = all_possessions.get(&sell_item.object_guid) else {
            log::warn!("[VENDOR] {player_name} tried to sell item {:08X} not in their inventory to {vendor_name}", sell_item.object_guid);
            continue;
        };

        let (wo_name, stack_size) = (name(w, wo), obj(w, wo).stack_size());

        // verify item profile (unique guids, amount)
        if verified.contains_key(&wo.full()) {
            log::warn!("[VENDOR] {player_name} tried to sell duplicate item {wo_name} ({wo}) to {vendor_name}");
            continue;
        }

        if !sell_item.is_valid_amount() {
            log::warn!(
                "[VENDOR] {player_name} tried to sell {}x {wo_name} ({wo}) to {vendor_name}",
                sell_item.amount
            );
            continue;
        }

        if sell_item.amount > stack_size.unwrap_or(1) {
            log::warn!(
                "[VENDOR] {player_name} tried to sell {}x {wo_name} ({wo}) to {vendor_name}, but they only have {}x",
                sell_item.amount,
                stack_size.unwrap_or(1)
            );
            continue;
        }

        let item_name = |w: &World| {
            if stack_size.unwrap_or(1) > 1 {
                world_object::get_plural_name(w, wo)
            } else {
                name(w, wo)
            }
        };

        // verify wo / vendor / player properties
        let o = obj(w, wo);
        if accepted_item_types.cast_unsigned() & o.item_type().0 == 0
            || !o.is_sellable()
            || o.retained()
        {
            let item_name = item_name(w);
            transient(w, this, &format!("The {item_name} is unsellable.")); // retail message did not include item name, leaving in that for now.
            continue;
        }

        // `wo.Value < 1`: a null Value compares false.
        if obj(w, wo).value().is_some_and(|v| v < 1) {
            let item_name = item_name(w);
            transient(
                w,
                this,
                &format!("The {item_name} has no value and cannot be sold."),
            ); // retail message did not include item name, leaving in that for now.
            continue;
        }

        if !within_vendor_value_range(w, vendor, wo) {
            let item_name = item_name(w);
            transient(w, this, &format!("The {item_name} is unsellable."));
            continue;
        }

        if shims::is_trading(w, this)
            && dispatch::is_being_traded_or_contains_item_being_traded::is_being_traded_or_contains_item_being_traded(w, wo, &shims::items_in_trade_window(w, this))
        {
            let item_name = item_name(w);
            transient(w, this, &format!("You cannot sell that! The {item_name} is currently being traded.")); // custom message?
            continue;
        }

        if obj(w, wo).is_container() && !container::inventory(obj(w, wo)).is_empty() {
            let item_name = item_name(w);
            transient(
                w,
                this,
                &format!("You cannot sell that! The {item_name} must be empty."),
            ); // custom message?
            continue;
        }

        verified.add(wo.full(), wo);
    }

    verified
}

// ACE: Player.CreatePayoutCoinStacks
pub fn create_payout_coin_stacks(w: &mut World, mut amount: i32) -> Vec<ObjectGuid> {
    let mut coin_stacks = Vec::new();

    while amount > 0 {
        let currency_stack = shims::create_new_world_object_by_name(w, "coinstack")
            .expect("System.NullReferenceException: currencyStack");

        let max_stack_size = obj(w, currency_stack)
            .max_stack_size()
            .expect("System.InvalidOperationException: Nullable object must have a value.");
        let current_stack_amount = amount.min(i32::from(max_stack_size));

        obj_mut(w, currency_stack).set_stack_size(Some(current_stack_amount));
        coin_stacks.push(currency_stack);
        amount = amount.wrapping_sub(current_stack_amount);
    }
    coin_stacks
}

/// ACE's default: `sendUpdateMessageIfChanged = true`.
// ACE: Player.UpdateCoinValue
pub fn update_coin_value(w: &mut World, this: ObjectGuid, send_update_message_if_changed: bool) {
    let mut send_update_message_if_changed = send_update_message_if_changed;

    let mut coins: i32 = 0;

    for coin_stack in container::get_inventory_items_of_type_weenie_type(w, this, WeenieType::Coin)
    {
        coins = coins.wrapping_add(obj(w, coin_stack).value().unwrap_or(0));
    }

    if send_update_message_if_changed && obj(w, this).coin_value() == Some(coins) {
        send_update_message_if_changed = false;
    }

    obj_mut(w, this).set_coin_value(Some(coins));

    if send_update_message_if_changed {
        let value = obj(w, this).coin_value().unwrap_or(0);
        send_private_int(w, this, PropertyInt::CoinValue, value);
    }
}

/// ACE's default: `destroy = false`. `None` is ACE's `null`.
// ACE: Player.SpendCurrency
pub fn spend_currency(
    w: &mut World,
    this: ObjectGuid,
    current_wcid: u32,
    amount: u32,
    destroy: bool,
) -> Option<Vec<ObjectGuid>> {
    if current_wcid == 0 || amount == 0 {
        return None;
    }

    let mut cost = Vec::new();

    // `amount > CoinValue`: uint against int?, lifted over long, so a null CoinValue compares false.
    if current_wcid == COIN_STACK_WCID
        && obj(w, this)
            .coin_value()
            .is_some_and(|c| i64::from(amount) > i64::from(c))
    {
        return None;
    }
    if destroy {
        player_inventory::try_consume_from_inventory_with_networking_wcid(
            w,
            this,
            current_wcid,
            amount.cast_signed(),
        );
    } else {
        cost = collect_currency_stacks(w, this, current_wcid, amount);

        for &stack in &cost {
            if player_inventory::try_remove_from_inventory_with_networking(
                w,
                this,
                stack,
                RemoveFromInventoryAction::SpendItem,
            )
            .is_none()
            {
                update_coin_value(w, this, true); // this coinstack was created by spliting up an existing one, and not actually added to the players inventory. The existing stack was already adjusted down but we need to update the player's CoinValue, so we do that now.
            }
        }
    }
    Some(cost)
}

// ACE: Player.CollectCurrencyStacks
pub fn collect_currency_stacks(
    w: &mut World,
    this: ObjectGuid,
    currency_wcid: u32,
    amount: u32,
) -> Vec<ObjectGuid> {
    let mut currency_stacks_collected = Vec::new();

    let currency_stacks_in_inventory =
        container::get_inventory_items_of_wcid(w, this, currency_wcid);
    //currencyStacksInInventory = currencyStacksInInventory.OrderBy(o => o.Value).ToList();

    let mut remaining: i32 = amount.cs_cast();

    for stack in currency_stacks_in_inventory {
        let stack_size = obj(w, stack).stack_size();
        let amount_to_remove = remaining.min(stack_size.unwrap_or(1));

        if stack_size == Some(amount_to_remove) {
            currency_stacks_collected.push(stack);
        } else {
            // create new stack
            let new_stack = create_new_world_object(w, currency_wcid)
                .expect("System.NullReferenceException: newStack");
            obj_mut(w, new_stack).set_stack_size(Some(amount_to_remove));
            currency_stacks_collected.push(new_stack);

            let found = player_inventory::find_object(w, this, stack, SearchLocations::MyInventory);

            // adjust existing stack
            if let Some(stack_to_adjust) = found.result {
                player_inventory::adjust_stack(
                    w,
                    this,
                    stack_to_adjust,
                    amount_to_remove.wrapping_neg(),
                    found.found_in_container,
                    found.root_owner,
                );
                let m = game_message_set_stack_size(obj_mut(w, stack_to_adjust));
                send(w, this, m);
                let encumbrance = obj(w, this).encumbrance_val().unwrap_or(0);
                send_private_int(w, this, PropertyInt::EncumbranceVal, encumbrance);
            }
            // UpdateCoinValue removed -- already called upstream
        }

        remaining = remaining.wrapping_sub(amount_to_remove);

        if remaining <= 0 {
            break;
        }
    }
    currency_stacks_collected
}

/// Members ported in other files, each marked `SHIM:` (no anchor).
mod shims {
    use super::{DotNetHashSet, ObjectGuid, WeenieError, World};

    /// `Player.SendUseDoneEvent(errorType)` (`Player_Use.cs`).
    pub(super) fn send_use_done_event(w: &mut World, this: ObjectGuid, error_type: WeenieError) {
        crate::world_objects::player_use::send_use_done_event(w, this, error_type);
    }

    /// SHIM: `Player.IsTrading` (`Player_Trade.cs`).
    pub(super) fn is_trading(w: &World, this: ObjectGuid) -> bool {
        crate::world_objects::player_trade::is_trading(w, this)
    }

    /// SHIM: `Player.ItemsInTradeWindow` (`Player_Trade.cs`).
    pub(super) fn items_in_trade_window(w: &World, this: ObjectGuid) -> DotNetHashSet<ObjectGuid> {
        crate::world_objects::player_trade::items_in_trade_window(w, this)
    }

    /// `Player.RushNextPlayerSave(seconds)` (`Player_Database.cs`).
    pub(super) fn rush_next_player_save(w: &mut World, this: ObjectGuid, seconds: i32) {
        crate::world_objects::player_database::rush_next_player_save(w, this, seconds);
    }

    /// `WorldObjectFactory.CreateNewWorldObject(string weenieClassName)`, put in `World.objects`.
    pub(super) fn create_new_world_object_by_name(
        w: &mut World,
        weenie_class_name: &str,
    ) -> Option<ObjectGuid> {
        crate::factories::world_object_factory::create_new_world_object_by_name_in_world(
            w,
            weenie_class_name,
        )
    }
}
