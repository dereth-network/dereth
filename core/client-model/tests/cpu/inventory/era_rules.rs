//! What the model itself does on a world without a system, whatever the character saved: a
//! fellowship shares without a founder's choice, a drop on a player is a gift without trade, a
//! double click on a player does nothing without trade, a weapon stays out of the shield's slot
//! without dual wield, and a trade note is priced by the world's rule.
//! Fixture: synthetic objects in a bare model.

use crate::common::model_replay::*;
use dereth_client_model::inventory::equip::EquipmentDropOutcome;
use dereth_client_model::inventory::use_object::UseResult;
use dereth_client_model::player::options::option::DRAG_ITEM_ON_PLAYER_OPENS_SECURE_TRADE;
use dereth_client_model::vendor::{BuyRefusal, VendorProfile};
use dereth_primitives::EraFeatures;
use dereth_rules::slots::loc;
use dereth_rules::weenie::{bitfield, item_type};

const ME: ObjectId = ObjectId(0x5000_0001);
const ITEM: ObjectId = ObjectId(0x8000_0001);
const OTHER: ObjectId = ObjectId(0x5000_0002);

/// A character holding one item, with another player beside them.
fn a_character_beside_a_player() -> World {
    let mut w = World::new();
    w.player = Some(ME);
    w.tables
        .weenies
        .insert(ME, dereth_client_model::Weenie::new(ME));
    let mut item = dereth_client_model::Weenie::new(ITEM);
    item.pwd.name = "Gem".into();
    item.pwd.container_id = Some(ME);
    w.tables.weenies.insert(ITEM, item);
    let mut other = dereth_client_model::Weenie::new(OTHER);
    other.pwd.name = "Someone".into();
    other.pwd.obj_type = item_type::CREATURE;
    other.pwd.useability = Some(1);
    other.pwd.bitfield |= bitfield::PLAYER | bitfield::STUCK;
    w.tables.weenies.insert(OTHER, other);
    w
}

fn without(f: fn(&mut EraFeatures)) -> EraFeatures {
    let mut features = EraFeatures::ALL;
    f(&mut features);
    features
}

/// The share word of the `Fellowship_Create` (`0x00A2`) the model sends for a saved `share_xp`.
fn share_sent(features: EraFeatures, share_xp: bool) -> i32 {
    let mut w = World::new();
    w.era_features = features;
    let mut req = RecordingRequests::default();
    w.create_fellowship(&mut req, "Band", share_xp);
    req.0
        .iter()
        .find_map(|r| match r {
            Request::FellowshipCreate(m) => Some(m.share_xp),
            _ => None,
        })
        .expect("a fellowship request")
}

/// Behaviour: presentation.era.a-fellowship-always-shares-without-the-founders-choice
#[test]
fn a_world_without_fellowship_options_asks_to_share_whatever_was_saved() {
    let off = without(|f| f.fellowship_options = false);
    assert_eq!(share_sent(off, false), 1);
    assert_eq!(share_sent(off, true), 1);
    assert_eq!(share_sent(EraFeatures::ALL, false), 0);
    assert_eq!(share_sent(EraFeatures::ALL, true), 1);
}

/// Behaviour: presentation.era.fellowship-options-follow-the-world
/// Opening a fellowship asks nothing of a world without open fellowships, plugin or button.
#[test]
fn a_world_without_open_fellowships_sends_no_openness() {
    let toggled = |features| {
        let mut w = World::new();
        w.era_features = features;
        w.fellowship = Some(Default::default());
        let mut req = RecordingRequests::default();
        (w.fellowship_toggle_openness(&mut req), req.0.len())
    };
    assert_eq!(toggled(without(|f| f.fellowship_open = false)), (None, 0));
    assert_eq!(toggled(EraFeatures::ALL), (Some(true), 1));
}

/// Behaviour: presentation.era.fellowship-options-follow-the-world
/// Ignoring invitations clears a saved Auto-Accept only where the option exists: a world that
/// hides it keeps the value for a world that has it.
#[test]
fn a_hidden_auto_accept_survives_ignoring_invitations() {
    use dereth_client_model::player::options::option;
    let saved_after_ignoring = |features| {
        let mut w = World::new();
        w.set_era_features(features);
        w.player_system
            .options
            .set(option::IGNORE_FELLOWSHIP_REQUESTS, false);
        w.player_system
            .options
            .set(option::FELLOWSHIP_AUTO_ACCEPT_REQUESTS, true);
        w.player_system
            .set_option(option::IGNORE_FELLOWSHIP_REQUESTS, true, ServerTime(1.0));
        w.player_system
            .options
            .get(option::FELLOWSHIP_AUTO_ACCEPT_REQUESTS)
    };
    assert!(saved_after_ignoring(without(|f| f
        .fellowship_auto_accept =
        false)));
    assert!(!saved_after_ignoring(EraFeatures::ALL));
}

/// Behaviour: presentation.era.allegiance-chat-follows-the-world
/// A world without the allegiance chat room has no such talk focus, whatever the server's room
/// tracker said.
#[test]
fn a_world_without_allegiance_chat_has_no_such_talk_focus() {
    use dereth_client_model::chat::TalkFocus;
    let mut w = World::new();
    w.chat.set_talk_focus_enabled(TalkFocus::Allegiance, true);
    w.chat.set_talk_focus(TalkFocus::Allegiance);
    w.set_era_features(EraFeatures::ALL);
    assert!(w.chat.is_talk_focus_enabled(TalkFocus::Allegiance));
    w.set_era_features(without(|f| f.allegiance_chat = false));
    assert!(!w.chat.is_talk_focus_enabled(TalkFocus::Allegiance));
    assert_ne!(w.chat.talk_focus, TalkFocus::Allegiance);
}

/// Dropping the item on the other player, with the saved option on, and what was asked of the
/// game.
fn drop_on_the_player(features: EraFeatures) -> (Vec<Request>, ObjectId) {
    let mut w = a_character_beside_a_player();
    w.era_features = features;
    w.player_system
        .options
        .set(DRAG_ITEM_ON_PLAYER_OPENS_SECURE_TRADE, true);
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    w.attempt_place_in_3d(
        &mut req,
        &mut out,
        ITEM,
        Some(OTHER),
        false,
        true,
        SplitState::default(),
        ServerTime(1.0),
    );
    let saved = w
        .player_system
        .options
        .get(DRAG_ITEM_ON_PLAYER_OPENS_SECURE_TRADE);
    assert!(
        saved,
        "the saved option is kept for the world that has trade"
    );
    (req.0, w.trade.attempt_to_player)
}

/// Behaviour: presentation.era.a-drop-on-a-player-gives-without-trade
#[test]
fn a_drop_on_a_player_gives_without_trade_whatever_the_saved_option_says() {
    let (sent, attempt) = drop_on_the_player(without(|f| f.trade = false));
    assert!(
        sent.iter()
            .any(|r| matches!(r, Request::GiveObjectRequest(m) if m.target == OTHER)),
        "{sent:?}"
    );
    assert_eq!(attempt, ObjectId(0));
    let (sent, attempt) = drop_on_the_player(EraFeatures::ALL);
    assert!(!sent
        .iter()
        .any(|r| matches!(r, Request::GiveObjectRequest(_))));
    assert_eq!(attempt, OTHER);
}

/// Behaviour: presentation.era.a-double-click-on-a-player-does-nothing-without-trade
#[test]
fn a_double_click_on_a_player_asks_for_no_trade_without_trade() {
    let mut w = a_character_beside_a_player();
    assert_eq!(w.determine_use_result(OTHER), UseResult::Trade);
    w.era_features = without(|f| f.trade = false);
    assert_eq!(w.determine_use_result(OTHER), UseResult::Nothing);
}

/// A sword in the pack, as the drop on an equipment slot reads it.
fn a_world_with_a_sword(features: EraFeatures) -> World {
    let mut w = a_character_beside_a_player();
    w.era_features = features;
    let sword = w.tables.weenies.get_mut(ITEM).expect("just made");
    sword.pwd.valid_locations = Some(loc::MELEE_WEAPON);
    w
}

/// Behaviour: presentation.era.a-weapon-stays-out-of-the-shield-slot-without-dual-wield
#[test]
fn only_a_world_with_dual_wield_takes_a_weapon_in_the_shields_slot() {
    let drop_it = |w: &mut World| {
        let mut req = RecordingRequests::default();
        let mut out = RecordingSink::default();
        let outcome = w.drop_equipment_at_location(
            &mut req,
            &mut out,
            ITEM,
            loc::SHIELD,
            0,
            SplitState::default(),
            ServerTime(1.0),
        );
        (outcome, req.0.len(), out.0.len())
    };
    let mut w = a_world_with_a_sword(without(|f| f.dual_wield = false));
    assert_eq!(
        drop_it(&mut w),
        (EquipmentDropOutcome::Refused, 0, 0),
        "refused, with nothing sent and nothing said"
    );
    assert_eq!(
        w.equipment_hover(ITEM).slot_mask.unwrap_or(0) & loc::SHIELD,
        0,
        "the slot does not light for it"
    );
    let w = a_world_with_a_sword(EraFeatures::ALL);
    assert_ne!(
        w.equipment_hover(ITEM).slot_mask.unwrap_or(0) & loc::SHIELD,
        0
    );
    let mut w = a_world_with_a_sword(without(|f| f.dual_wield = false));
    w.tables
        .weenies
        .get_mut(ITEM)
        .expect("just made")
        .pwd
        .valid_locations = Some(loc::SHIELD);
    assert_ne!(
        drop_it(&mut w).0,
        EquipmentDropOutcome::Refused,
        "a shield still goes there"
    );
}

/// A vendor with a thousand-pyreal trade note in stock.
fn at_a_note_vendor(features: EraFeatures, purse: i32) -> World {
    let mut w = World::new();
    w.era_features = features;
    w.player = Some(ME);
    let mut me = dereth_client_model::Weenie::new(ME);
    me.pwd.items_capacity = Some(10);
    w.tables.weenies.insert(ME, me);
    w.shop.vendor_id = Some(ObjectId(0x7000_0001));
    w.shop.profile = VendorProfile {
        item_types: item_type::PROMISSORY_NOTE,
        min_value: -1,
        max_value: -1,
        buy_price: 0.5,
        sell_price: 1.5,
        ..VendorProfile::default()
    };
    w.shop.total_value = purse;
    w.shop.stock.push(dereth_client_model::vendor::ItemProfile {
        amount: -1,
        iid: ITEM,
        pwd: dereth_protocol::types::PublicWeenieDesc {
            value: Some(1000),
            obj_type: item_type::PROMISSORY_NOTE,
            ..Default::default()
        },
    });
    w
}

/// Behaviour: presentation.era.trade-note-prices-follow-the-world
/// Who can afford a note follows the world's rule: 1100 buys one where notes know their issuer,
/// and not where a note costs 1150.
#[test]
fn the_client_refuses_a_note_purchase_by_the_worlds_price() {
    let buy = |features, purse| {
        let mut w = at_a_note_vendor(features, purse);
        let mut req = RecordingRequests::default();
        let mut out = RecordingSink::default();
        w.buy_single_item(ITEM, 1, &mut req, &mut out, ServerTime(1.0))
    };
    let early = without(|f| f.trade_notes_cash_anywhere = false);
    assert_eq!(buy(early, 1100), Ok(true));
    assert_eq!(buy(early, 1099), Err(BuyRefusal::NotEnoughMoney));
    assert_eq!(buy(EraFeatures::ALL, 1100), Err(BuyRefusal::NotEnoughMoney));
    assert_eq!(buy(EraFeatures::ALL, 1150), Ok(true));

    let w = at_a_note_vendor(early, 0);
    let note = w.shop.stock[0].pwd.clone();
    assert_eq!(w.shop_sell_price(&note, 1), 1100);
    assert_eq!(w.shop_buy_price(&note), 800);
    let w = at_a_note_vendor(EraFeatures::ALL, 0);
    assert_eq!(w.shop_sell_price(&note, 1), 1150);
    assert_eq!(w.shop_buy_price(&note), 1000);
}
