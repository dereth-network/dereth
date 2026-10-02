// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Trade.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Trade.cs`.
//!
//! Player-to-player trade. Each player keeps its own side of the negotiation in
//! [`PlayerTradeFields`]; the partner is a guid resolved through `PlayerManager.GetOnlinePlayer`
//! as ACE does. Other files read `IsTrading`, `TradePartner` and `ItemsInTradeWindow` through
//! [`is_trading`], [`trade_partner`] and [`items_in_trade_window`].

use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};
use empyrean_entity::enums::{
    CharacterOption, ChatMessageType, CombatMode, EndTradeReason, TradeSide, WeenieError,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_accept_trade::game_event_accept_trade;
use crate::network::game_event::events::game_event_add_to_trade::game_event_add_to_trade;
use crate::network::game_event::events::game_event_clear_trade_acceptance::game_event_clear_trade_acceptance;
use crate::network::game_event::events::game_event_close_trade::game_event_close_trade;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_decline_trade::game_event_decline_trade;
use crate::network::game_event::events::game_event_register_trade::game_event_register_trade;
use crate::network::game_event::events::game_event_reset_trade::game_event_reset_trade;
use crate::network::game_event::events::game_event_trade_failure::game_event_trade_failure;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::physics::{object_maint, phys_ext};
use crate::sessions::SessionData;
use crate::world_objects::player_inventory::{DequipObjectAction, RemoveFromInventoryAction};
use crate::world_objects::world_object::LOCAL_BROADCAST_RANGE;
use crate::world_objects::{
    container, creature_combat, creature_equipment, player_character, player_inventory,
    player_magic, player_move, player_tracking,
};
use crate::World;

/// Non-property fields declared in `Player_Trade.cs`.
#[derive(Debug, Default)]
pub struct PlayerTradeFields {
    // ACE: Player.ItemsInTradeWindow
    pub items_in_trade_window: DotNetHashSet<ObjectGuid>,

    // ACE: Player.TradePartner
    pub trade_partner: ObjectGuid,

    // ACE: Player.IsTrading
    /// `{ get; private set; }`.
    pub is_trading: bool,

    // ACE: Player.TradeAccepted
    pub trade_accepted: bool,

    // ACE: Player.TradeTransferInProgress
    pub trade_transfer_in_progress: bool,

    // ACE: Player.KnownTradeObjs
    pub known_trade_objs: DotNetDict<ObjectGuid, DotNetHashSet<ObjectGuid>>,
}

// ================================================================================ helpers

fn fields(w: &World, this: ObjectGuid) -> &PlayerTradeFields {
    &w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .unwrap_or_else(|| {
            panic!(
                "System.NullReferenceException: player 0x{:08X} is not in World.objects",
                this.full()
            )
        })
        .player_trade
}

fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerTradeFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .unwrap_or_else(|| {
            panic!(
                "System.NullReferenceException: player 0x{:08X} is not in World.objects",
                this.full()
            )
        })
        .player_trade
}

/// `Player.IsTrading` (false for anything that is not a player in the store).
#[must_use]
pub fn is_trading(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_trade.is_trading)
}

/// `Player.TradePartner`.
#[must_use]
pub fn trade_partner(w: &World, this: ObjectGuid) -> ObjectGuid {
    w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .map_or(ObjectGuid::INVALID, |p| p.player_trade.trade_partner)
}

/// `Player.ItemsInTradeWindow` (a copy; empty for anything that is not a player in the store).
#[must_use]
pub fn items_in_trade_window(w: &World, this: ObjectGuid) -> DotNetHashSet<ObjectGuid> {
    w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .map(|p| p.player_trade.items_in_trade_window.clone())
        .unwrap_or_default()
}

/// `Player.TradeAccepted`.
#[must_use]
pub fn trade_accepted(w: &World, this: ObjectGuid) -> bool {
    fields(w, this).trade_accepted
}

/// `Player.TradeTransferInProgress`.
#[must_use]
pub fn trade_transfer_in_progress(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_trade.trade_transfer_in_progress)
}

/// `Player.Session`: `None` when it has none (nothing is sent).
fn session(w: &World, this: ObjectGuid) -> Option<SessionId> {
    player_manager::player_session(w, this)
}

/// `Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    if let Some(s) = session(w, this) {
        enqueue_send(w, s, msg);
    }
}

/// Builds a game event on the player's session (consuming its `GameEventSequence`) and sends it.
fn send_event(
    w: &mut World,
    this: ObjectGuid,
    build: impl FnOnce(&mut SessionData) -> GameMessage,
) {
    let Some(s) = session(w, this) else { return };
    let Some(data) = w.sessions.get_mut(s) else {
        return;
    };
    let msg = build(data);
    enqueue_send(w, s, msg);
}

/// `new GameEventCommunicationTransientString(Session, message)`.
fn transient(w: &mut World, this: ObjectGuid, message: &str) {
    send_event(w, this, |d| {
        game_event_communication_transient_string(d, message)
    });
}

/// `new GameEventWeenieError(Session, errorType)`.
fn weenie_error(w: &mut World, this: ObjectGuid, error_type: WeenieError) {
    send_event(w, this, |d| game_event_weenie_error(d, error_type));
}

fn name(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

/// `Player.IsOlthoiPlayer` (`Player_Properties.cs`, the value `SetEphemeralValues` stores).
fn is_olthoi_player(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_properties.is_olthoi_player)
}

/// `Creature.CombatMode`.
fn combat_mode(w: &World, this: ObjectGuid) -> CombatMode {
    creature_combat::combat_mode(w, this)
}

/// `GetInventoryItem(itemGuid) ?? GetEquippedItem(itemGuid)`.
fn get_inventory_or_equipped_item(
    w: &World,
    player: ObjectGuid,
    item_guid: ObjectGuid,
) -> Option<ObjectGuid> {
    container::get_inventory_item(w, player, item_guid)
        .or_else(|| creature_equipment::get_equipped_item(w, player, item_guid))
}

// ================================================================================ Player_Trade

// ACE: Player.HandleActionOpenTradeNegotiations
/// ACE's default: `initiator = false`.
pub fn handle_action_open_trade_negotiations(
    w: &mut World,
    this: ObjectGuid,
    trade_partner_guid: u32,
    initiator: bool,
) {
    // DIVERGE: a world without secure trade (`EraFeatures::trade`) refuses opening one (V419).
    if !crate::world_objects::era_gates::has(w, this, w.era.features.trade, "secure trade") {
        return;
    }

    if is_olthoi_player(w, this) {
        send(w, this, game_message_system_chat("As a mindless engine of destruction an Olthoi cannot participate in trade negotiations!", ChatMessageType::Magic));
        return;
    }

    let Some(trade_partner) = player_manager::get_online_player(w, trade_partner_guid) else {
        return;
    };

    //Check to see if potential trading partner is an Olthoi player
    if initiator && is_olthoi_player(w, trade_partner) {
        send(
            w,
            this,
            game_message_system_chat("The Olthoi's hunger for destruction is too great to understand a request for trade negotiations!", ChatMessageType::Broadcast),
        );
        return;
    }

    //Check to see if partner is not allowing trades
    if initiator
        && player_character::get_character_option(
            w,
            trade_partner,
            CharacterOption::IgnoreAllTradeRequests,
        )
    {
        weenie_error(w, this, WeenieError::TradeIgnoringRequests);
        return;
    }

    //Check to see if either party is already part of an in process trade session
    if is_trading(w, this) || is_trading(w, trade_partner) {
        weenie_error(w, this, WeenieError::TradeAlreadyTrading);
        return;
    }

    //Check to see if either party is in combat mode
    if combat_mode(w, this) != CombatMode::NonCombat
        || combat_mode(w, trade_partner) != CombatMode::NonCombat
    {
        weenie_error(w, this, WeenieError::TradeNonCombatMode);
        return;
    }

    //Check to see if trade partner is in range, if so, rotate and move to
    if initiator {
        let callback: player_move::MoveToCallback =
            Box::new(move |w: &mut World, success: bool| {
                if !success {
                    weenie_error(w, this, WeenieError::TradeMaxDistanceExceeded);
                    return;
                }

                send_event(w, this, |d| {
                    game_event_register_trade(d, this, trade_partner)
                });

                // DIVERGE: ACE's closure holds the partner object; a partner that has since left the
                // store is not called (closures hold guids).
                if w.objects
                    .get(trade_partner)
                    .is_some_and(|o| o.player.is_some())
                {
                    handle_action_open_trade_negotiations(w, trade_partner, this.full(), false);
                }
            });
        player_move::create_move_to_chain(w, this, trade_partner, callback, None, true);
    } else {
        fields_mut(w, this).is_trading = true;
        fields_mut(w, trade_partner).is_trading = true;
        fields_mut(w, this).trade_transfer_in_progress = false;
        fields_mut(w, trade_partner).trade_transfer_in_progress = false;

        fields_mut(w, this).trade_partner = trade_partner;
        fields_mut(w, trade_partner).trade_partner = this;

        fields_mut(w, this).items_in_trade_window.clear();
        fields_mut(w, trade_partner).items_in_trade_window.clear();

        send_event(w, this, |d| {
            game_event_register_trade(d, trade_partner, trade_partner)
        });
    }
}

// ACE: Player.HandleActionCloseTradeNegotiations
/// ACE's default: `endTradeReason = EndTradeReason.Normal`.
pub fn handle_action_close_trade_negotiations(
    w: &mut World,
    this: ObjectGuid,
    end_trade_reason: EndTradeReason,
) {
    if fields(w, this).trade_transfer_in_progress {
        return;
    }

    let f = fields_mut(w, this);
    f.is_trading = false;
    f.trade_accepted = false;
    f.trade_transfer_in_progress = false;
    f.items_in_trade_window.clear();
    f.trade_partner = ObjectGuid::INVALID;

    send_event(w, this, |d| game_event_close_trade(d, end_trade_reason));
    weenie_error(w, this, WeenieError::TradeClosed);
}

// ACE: Player.HandleActionAddToTrade
pub fn handle_action_add_to_trade(
    w: &mut World,
    this: ObjectGuid,
    item_guid: u32,
    trade_window_slot_number: u32,
) {
    let _ = trade_window_slot_number;

    if fields(w, this).trade_transfer_in_progress {
        return;
    }

    fields_mut(w, this).trade_accepted = false;

    let target = player_manager::get_online_player(w, fields(w, this).trade_partner.full());

    let Some(target) = target.filter(|_| item_guid != 0) else {
        return;
    };

    fields_mut(w, target).trade_accepted = false;

    let Some(wo) = get_inventory_or_equipped_item(w, this, ObjectGuid::new(item_guid)) else {
        return;
    };

    if crate::dispatch::is_attuned_or_contains_attuned::is_attuned_or_contains_attuned(w, wo) {
        transient(w, this, "You cannot trade that!");
        send_event(w, this, |d| {
            game_event_trade_failure(d, item_guid, WeenieError::AttunedItem)
        });
        return;
    }

    if w.objects
        .get(wo)
        .is_some_and(|o| o.is_pet_device() && o.pet().is_some())
    {
        transient(
            w,
            this,
            "You must unsummon your pet before you can trade this item!",
        );
        send_event(w, this, |d| {
            game_event_trade_failure(d, item_guid, WeenieError::AttunedItem)
        });
        return;
    }

    if crate::dispatch::is_unique_or_contains_unique::is_unique_or_contains_unique(w, wo)
        && !player_inventory::check_uniques(w, target, &[wo], Some(this))
    {
        // WeenieError.TooManyUniqueItems / WeenieErrorWithString._CannotCarryAnymore?
        send_event(w, this, |d| {
            game_event_trade_failure(d, item_guid, WeenieError::None)
        });
        return;
    }

    fields_mut(w, this)
        .items_in_trade_window
        .insert(ObjectGuid::new(item_guid));

    send_event(w, this, |d| {
        game_event_add_to_trade(d, item_guid, TradeSide::Self_)
    });

    add_known_trade_obj(w, target, this, wo);
    player_tracking::track_object(w, target, wo, false);

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, 0.001);
    action_chain.add_action(Actor::Object(target), move |w: &mut World| {
        send_event(w, target, |d| {
            game_event_add_to_trade(d, item_guid, TradeSide::Partner)
        });
    });
    action_chain.enqueue_chain(w);
}

// ACE: Player.HandleActionResetTrade
pub fn handle_action_reset_trade(w: &mut World, this: ObjectGuid, who_reset: ObjectGuid) {
    if fields(w, this).trade_transfer_in_progress {
        return;
    }

    let f = fields_mut(w, this);
    f.items_in_trade_window.clear();
    f.trade_accepted = false;

    send_event(w, this, |d| game_event_reset_trade(d, who_reset));
}

// ACE: Player.ClearTradeAcceptance
pub fn clear_trade_acceptance(w: &mut World, this: ObjectGuid) {
    let f = fields_mut(w, this);
    f.items_in_trade_window.clear();
    f.trade_accepted = false;

    send_event(w, this, game_event_clear_trade_acceptance);
}

// ACE: Player.HandleActionAcceptTrade
pub fn handle_action_accept_trade(w: &mut World, this: ObjectGuid) {
    if fields(w, this).trade_transfer_in_progress {
        return;
    }

    fields_mut(w, this).trade_accepted = true;

    send_event(w, this, |d| game_event_accept_trade(d, this));
    transient(w, this, "You have accepted the offer");

    let target = player_manager::get_online_player(w, fields(w, this).trade_partner.full());

    let Some(target) = target else { return };

    send_event(w, target, |d| game_event_accept_trade(d, this));
    let text = format!("{} has accepted the offer", name(w, this));
    transient(w, target, &text);

    if fields(w, target).trade_accepted {
        finalize_trade(w, this, target);
    }
}

// ACE: Player.FinalizeTrade
fn finalize_trade(w: &mut World, this: ObjectGuid, target: ObjectGuid) {
    if !verify_trade_busy_state(w, this, target) || !verify_trade_inventory(w, this, target) {
        return;
    }

    player_magic::set_is_busy(w, this, true);
    player_magic::set_is_busy(w, target, true);

    fields_mut(w, this).trade_transfer_in_progress = true;
    fields_mut(w, target).trade_transfer_in_progress = true;

    transient(w, this, "The items are being traded");
    transient(w, target, "The items are being traded");

    // `tradedItems`: the biotas saved once the items have moved (ACE keeps the Biota references,
    // so the save sees their state after the move).
    let mut traded_items: Vec<ObjectGuid> = Vec::new();

    let mut my_escrow: Vec<ObjectGuid> = Vec::new();
    let mut target_escrow: Vec<ObjectGuid> = Vec::new();

    let mine: Vec<ObjectGuid> = fields(w, this)
        .items_in_trade_window
        .iter()
        .copied()
        .collect();
    for item_guid in mine {
        let wo = player_inventory::try_remove_from_inventory_with_networking(
            w,
            this,
            item_guid,
            RemoveFromInventoryAction::TradeItem,
        )
        .or_else(|| {
            player_inventory::try_dequip_object_with_networking(
                w,
                this,
                item_guid,
                DequipObjectAction::TradeItem,
            )
        });
        if let Some(wo) = wo {
            target_escrow.push(wo);

            traded_items.push(wo);
        }
    }

    let theirs: Vec<ObjectGuid> = fields(w, target)
        .items_in_trade_window
        .iter()
        .copied()
        .collect();
    for item_guid in theirs {
        let wo = player_inventory::try_remove_from_inventory_with_networking(
            w,
            target,
            item_guid,
            RemoveFromInventoryAction::TradeItem,
        )
        .or_else(|| {
            player_inventory::try_dequip_object_with_networking(
                w,
                target,
                item_guid,
                DequipObjectAction::TradeItem,
            )
        });
        if let Some(wo) = wo {
            my_escrow.push(wo);

            traded_items.push(wo);
        }
    }

    let current_landblock = w
        .objects
        .get(this)
        .and_then(|o| o.current_landblock)
        .expect("ACE: CurrentLandblock is null (NullReferenceException in ActionChain)");

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, 0.5);
    action_chain.add_action(Actor::Landblock(current_landblock), move |w: &mut World| {
        for wo in my_escrow {
            let _ = player_inventory::try_create_in_inventory_with_networking(w, this, wo);
        }

        for wo in target_escrow {
            let _ = player_inventory::try_create_in_inventory_with_networking(w, target, wo);
        }

        weenie_error(w, this, WeenieError::TradeComplete);
        weenie_error(w, target, WeenieError::TradeComplete);

        fields_mut(w, this).trade_transfer_in_progress = false;
        fields_mut(w, target).trade_transfer_in_progress = false;

        player_magic::set_is_busy(w, this, false);
        player_magic::set_is_busy(w, target, false);

        crate::entity::landblock::shard_save_biotas_in_parallel(w, &traded_items);

        handle_action_reset_trade(w, this, this);
        handle_action_reset_trade(w, target, target);
    });

    action_chain.enqueue_chain(w);
}

// ACE: Player.GetItemsInTradeWindow
/// `itemsToBeTraded`, or `None` (ACE's `false`) when an item is neither in `player`'s inventory nor
/// equipped.
fn get_items_in_trade_window(w: &World, player: ObjectGuid) -> Option<Vec<ObjectGuid>> {
    let mut results = Vec::new();

    for &item_guid in fields(w, player).items_in_trade_window.iter() {
        // look in inventory for item; if item is equipped, it won't be found above, so if not found, look in equipped objects
        let wo = get_inventory_or_equipped_item(w, player, item_guid);

        // item was not found in inventory or equipped: `return false`
        results.push(wo?);
    }

    Some(results)
}

// ACE: Player.HandleActionDeclineTrade
/// `session.Player` is `this`.
pub fn handle_action_decline_trade(w: &mut World, this: ObjectGuid, session: SessionId) {
    let _ = session;
    decline_trade(w, this);
}

/// The body of `HandleActionDeclineTrade(session)` for `session.Player`.
fn decline_trade(w: &mut World, this: ObjectGuid) {
    if fields(w, this).trade_transfer_in_progress {
        return;
    }

    fields_mut(w, this).trade_accepted = false;

    send_event(w, this, |d| game_event_decline_trade(d, this));
    transient(w, this, "Trade confirmation failed...");

    let target = player_manager::get_online_player(w, fields(w, this).trade_partner.full());

    if let Some(target) = target {
        send_event(w, target, |d| game_event_decline_trade(d, this));
        transient(w, target, "Trade confirmation failed...");
    }
}

// ACE: Player.HandleActionTradeSwitchToCombatMode
/// `session.Player` is `this`.
pub fn handle_action_trade_switch_to_combat_mode(w: &mut World, this: ObjectGuid) {
    if combat_mode(w, this) != CombatMode::NonCombat && is_trading(w, this) {
        let target = player_manager::get_online_player(w, fields(w, this).trade_partner.full());

        weenie_error(w, this, WeenieError::TradeNonCombatMode);
        handle_action_close_trade_negotiations(w, this, EndTradeReason::EnteredCombat);

        if let Some(target) = target {
            weenie_error(w, target, WeenieError::TradeNonCombatMode);
            handle_action_close_trade_negotiations(w, target, EndTradeReason::EnteredCombat);
        }
    }
}

// ACE: Player.VerifyTrade_BusyState
fn verify_trade_busy_state(w: &mut World, this: ObjectGuid, partner: ObjectGuid) -> bool {
    let (self_is_busy, partner_is_busy) = (
        player_magic::is_busy(w, this),
        player_magic::is_busy(w, partner),
    );
    if !self_is_busy && !partner_is_busy {
        return true;
    }

    let self_busy = "You are too busy to complete the trade!";
    let other_busy = "Your trading partner is too busy to complete the trade!";

    let self_msg = if self_is_busy { self_busy } else { other_busy };
    let partner_msg = if self_is_busy { other_busy } else { self_busy };

    transient(w, this, self_msg);
    transient(w, partner, partner_msg);

    clear_trade_acceptance(w, this);
    clear_trade_acceptance(w, partner);

    false
}

// ACE: Player.VerifyTrade_Inventory
fn verify_trade_inventory(w: &mut World, this: ObjectGuid, partner: ObjectGuid) -> bool {
    let self_items = get_items_in_trade_window(w, this);
    let partner_items = get_items_in_trade_window(w, partner);

    let Some(self_items) = self_items else {
        decline_trade(w, this);
        return false;
    };

    let Some(partner_items) = partner_items else {
        decline_trade(w, partner);
        return false;
    };

    let (player_a_can_add_to_inventory, self_encumbered, self_pack_space) =
        container::can_add_to_inventory_list_with_reasons(w, this, &partner_items);
    let (player_b_can_add_to_inventory, partner_encumbered, partner_pack_space) =
        container::can_add_to_inventory_list_with_reasons(w, partner, &self_items);

    if player_a_can_add_to_inventory && player_b_can_add_to_inventory {
        return true;
    }

    let mut self_reason = String::new();
    let mut partner_reason = String::new();

    if !player_a_can_add_to_inventory {
        self_reason = "You ".to_owned();
        partner_reason = "Your trading partner ".to_owned();

        if self_encumbered {
            self_reason += "are too encumbered to complete the trade!";
            partner_reason += "is too encumbered to complete the trade!";
        } else if self_pack_space {
            self_reason += "do not have enough free slots to complete the trade!";
            partner_reason += "does not have enough free slots to complete the trade!";
        }
    } else if !player_b_can_add_to_inventory {
        self_reason = "Your trading partner ".to_owned();
        partner_reason = "You ".to_owned();

        if partner_encumbered {
            self_reason += "is too encumbered to complete the trade!";
            partner_reason += "are too encumbered to complete the trade!";
        } else if partner_pack_space {
            self_reason += "does not have enough free slots to complete the trade!";
            partner_reason += "do not have enough free slots to complete the trade!";
        }
    }

    transient(w, this, &self_reason);
    transient(w, partner, &partner_reason);

    clear_trade_acceptance(w, this);
    clear_trade_acceptance(w, partner);

    false
}

// ACE: Player.AddKnownTradeObj
pub fn add_known_trade_obj(
    w: &mut World,
    this: ObjectGuid,
    player_guid: ObjectGuid,
    item_guid: ObjectGuid,
) {
    let f = fields_mut(w, this);
    f.known_trade_objs
        .get_or_insert_with(player_guid, DotNetHashSet::new)
        .insert(item_guid);
}

/// `ObjMaint.GetKnownObject(guid)`: the known physics object.
fn obj_maint_get_known_object(
    w: &World,
    this: ObjectGuid,
    guid: ObjectGuid,
) -> Option<dereth_physics::PhysHandle> {
    let h = w
        .objects
        .get(this)
        .and_then(|o| o.phys)
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    object_maint::get_known_object(w, h, guid.full())
}

// ACE: Player.GetKnownTradeObj
/// The trade partner who offered `item_guid`, while it is still known and within local broadcast
/// range; else `None`.
pub fn get_known_trade_obj(
    w: &mut World,
    this: ObjectGuid,
    item_guid: ObjectGuid,
) -> Option<ObjectGuid> {
    if fields(w, this).known_trade_objs.is_empty() {
        return None;
    }

    prune_known_trade_objs(w, this);

    let found = fields(w, this)
        .known_trade_objs
        .iter()
        .find(|(_, items)| items.contains(&item_guid))
        .map(|(&player_guid, _)| player_guid);
    let player_guid = found?;

    let player = obj_maint_get_known_object(w, this, player_guid)
        .and_then(|known| phys_ext::weenie_obj(w, known).world_object(w))
        .filter(|&g| w.objects.get(g).is_some_and(|o| o.is_player()));
    let location = w.objects.get(this).and_then(|o| o.location());
    match player {
        Some(player) => {
            let player_location = w.objects.get(player).and_then(|o| o.location());
            match player_location {
                Some(player_location) => {
                    let location =
                        location.expect("ACE: Location is null (NullReferenceException)");
                    (location.distance_to(&player_location) <= LOCAL_BROADCAST_RANGE)
                        .then_some(player)
                }
                None => None,
            }
        }
        None => None,
    }
}

// ACE: Player.PruneKnownTradeObjs
pub fn prune_known_trade_objs(w: &mut World, this: ObjectGuid) {
    let keys: Vec<ObjectGuid> = fields(w, this).known_trade_objs.keys().copied().collect();
    for player_guid in keys {
        if obj_maint_get_known_object(w, this, player_guid).is_none() {
            fields_mut(w, this).known_trade_objs.remove(&player_guid);
        }
    }
}
