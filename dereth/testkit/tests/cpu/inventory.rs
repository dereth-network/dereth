//! Things the player carries, buys, sells, gives away or is refused: the use dialog, the ground
//! container, the shop, the drag-split, the trade window, the shortcut bar, and what the client
//! says when the shard refuses an inventory request.
//!
//! Most scenarios drive a model-only client with messages through `Inbound` and requests through
//! `Player`; none opens a retail data file. **Where a claim is driven directly rather than through
//! a gesture**, it is because the harness has no gesture for it -- the pickup-destination probe,
//! the vendor's buttons, the split's three readings, a drop on a list slot or on the figure -- and
//! the claim is booked through `assert_behaviour` on a model client at the end, as
//! `tests/cpu/frame.rs` does.

use dereth_client_contract::UiRequest;
use dereth_client_model::inventory::requests::{InventoryRequest, BUSY_MESSAGE};
use dereth_client_model::inventory::use_object::{
    GroundObjectResult, UsageConfirmation, UseOutcome, UseRefusal, UseResult,
};
use dereth_client_model::inventory::SplitState;
use dereth_client_model::weenie::{bitfield, item_type};
use dereth_client_model::{Notice, RecordingRequests, RecordingSink, Request, World};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::types::weeniedesc::header as pwd_header;
use dereth_protocol::types::{ContentProfile, PublicWeenieDesc};
use dereth_testkit::{Given, HeadlessClient, Inbound, Player, ScenarioView};

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "a_confirmed_use_prints_the_same_line",
        &["use.confirmation.prints-the-same-line-and-the-material-name"],
        a_confirmed_use_prints_the_same_line,
    ),
    (
        "the_trade_window_follows_the_shards_lists",
        &["trade.window.follows-the-shards-own-two-lists"],
        the_trade_window_follows_the_shards_lists,
    ),
    (
        "a_stale_trade_window_accepts_nothing",
        &["trade.controls.a-stale-window-accepts-an-empty-trade"],
        a_stale_trade_window_accepts_nothing,
    ),
    (
        "a_dropped_item_leaves_the_shortcut_bar",
        &["shortcut.bar.drops-an-item-that-leaves-your-pack"],
        a_dropped_item_leaves_the_shortcut_bar,
    ),
    (
        "a_refusal_answers_for_the_item_we_asked_about",
        &["inventory.refusal.answers-for-the-item-the-client-asked-about"],
        a_refusal_answers_for_the_item_we_asked_about,
    ),
    // -- the ground container ---------------------------------------------------------------------
    (
        "a_pickup_follows_the_pack_the_player_has_open",
        &["container.ground.a-pickup-goes-into-whichever-pack-the-player-has-open"],
        a_pickup_follows_the_pack_the_player_has_open,
    ),
    (
        "a_second_ground_container_closes_the_first",
        &["container.ground.a-second-one-closes-the-first-and-tells-the-shard"],
        a_second_ground_container_closes_the_first,
    ),
    (
        "the_contents_reply_fills_the_ground_panel",
        &["container.ground.the-contents-reply-fills-the-panel-unless-it-answers-a-pickup"],
        the_contents_reply_fills_the_ground_panel,
    ),
    (
        "opening_a_ground_container_takes_no_lock",
        &["container.ground.opening-one-takes-no-inventory-lock"],
        opening_a_ground_container_takes_no_lock,
    ),
    (
        "the_contents_reply_frees_a_waiting_request",
        &["container.ground.the-contents-reply-frees-a-waiting-request-unless-it-is-a-pickup"],
        the_contents_reply_frees_a_waiting_request,
    ),
    (
        "your_own_pack_opens_where_it_hangs",
        &["container.own-pack.opens-where-it-is-and-is-not-the-ground-container"],
        your_own_pack_opens_where_it_hangs,
    ),
    (
        "double_clicking_a_player_starts_a_trade",
        &["trade.open.double-clicking-another-player-starts-negotiations"],
        double_clicking_a_player_starts_a_trade,
    ),
    (
        "a_player_killer_altar_asks_first",
        &["use.confirmation.a-player-killer-altar-asks-before-anything-is-sent"],
        a_player_killer_altar_asks_first,
    ),
    (
        "every_refusal_keeps_its_own_reason",
        &["use.refusal.each-refusal-is-named-rather-than-collapsed-into-one"],
        every_refusal_keeps_its_own_reason,
    ),
    // -- the drag-split ---------------------------------------------------------------------------
    (
        "a_split_leaves_the_source_at_its_new_count",
        &["inventory.split.the-source-stack-shows-its-new-count-once-the-shard-answers"],
        a_split_leaves_the_source_at_its_new_count,
    ),
    (
        "a_second_split_waits_for_the_first_answer",
        &["inventory.split.until-that-answer-arrives-every-later-request-is-refused"],
        a_second_split_waits_for_the_first_answer,
    ),
    (
        "a_whole_stack_drop_is_a_move",
        &["inventory.split.dragging-a-whole-stack-is-a-move-and-not-a-split"],
        a_whole_stack_drop_is_a_move,
    ),
    (
        "a_stale_or_oversized_stack_count_is_ignored",
        &["inventory.stack-size.a-stale-or-oversized-update-is-ignored"],
        a_stale_or_oversized_stack_count_is_ignored,
    ),
    // -- the vendor -------------------------------------------------------------------------------
    (
        "the_shards_message_opens_the_shop",
        &["vendor.shop.the-shards-own-message-opens-the-shop-and-replaces-it"],
        the_shards_message_opens_the_shop,
    ),
    (
        "the_button_and_the_basket_each_buy_once",
        &["vendor.buy.the-button-and-the-basket-each-send-one-purchase"],
        the_button_and_the_basket_each_buy_once,
    ),
    (
        "the_quantity_only_moves_a_stackable_row",
        &["vendor.buy.the-quantity-slider-only-moves-a-stackable-row"],
        the_quantity_only_moves_a_stackable_row,
    ),
    (
        "a_broke_or_full_player_buys_nothing",
        &["vendor.buy.a-player-with-no-money-or-no-room-is-refused-and-sends-nothing"],
        a_broke_or_full_player_buys_nothing,
    ),
    (
        "the_sell_list_marks_what_is_in_it",
        &["vendor.sell.the-list-marks-what-is-in-it-and-the-close-button-asks-first"],
        the_sell_list_marks_what_is_in_it,
    ),
    (
        "selling_the_whole_list_sends_one_message",
        &["vendor.sell.selling-the-whole-list-sends-one-message-and-clears-the-marks"],
        selling_the_whole_list_sends_one_message,
    ),
    (
        "an_unwanted_item_is_refused_in_the_shops_words",
        &["vendor.sell.an-item-the-vendor-will-not-take-is-refused-in-its-own-words"],
        an_unwanted_item_is_refused_in_the_shops_words,
    ),
    (
        "a_shop_and_a_ground_container_cannot_share",
        &["vendor.window.the-shop-and-the-ground-container-cannot-both-be-open"],
        a_shop_and_a_ground_container_cannot_share,
    ),
    // -- the trade removal family -----------------------------------------------------------------
    (
        "a_refusal_is_about_one_item_and_does_not_outlive_the_trade",
        &["trade.removal.a-later-confirmation-puts-the-id-back-and-a-reset-forgets-every-refusal"],
        a_refusal_is_about_one_item_and_does_not_outlive_the_trade,
    ),
    // -- where in a list a drop lands -------------------------------------------------------------
    (
        "a_drop_lands_at_the_place_the_player_aimed_at",
        &["inventory.place.a-drop-lands-at-the-place-the-player-aimed-at"],
        a_drop_lands_at_the_place_the_player_aimed_at,
    ),
    (
        "moving_a_thing_up_its_own_list_does_not_shift_it_one_further",
        &["inventory.place.moving-a-thing-up-its-own-list-does-not-shift-it-one-further"],
        moving_a_thing_up_its_own_list_does_not_shift_it_one_further,
    ),
    (
        "a_drop_that_would_change_nothing_is_refused_and_takes_no_lock",
        &["inventory.place.a-drop-that-would-change-nothing-is-refused-and-takes-no-lock"],
        a_drop_that_would_change_nothing_is_refused_and_takes_no_lock,
    ),
    (
        "a_plain_thing_under_the_pointer_is_a_place_and_a_pack_is_a_destination",
        &["inventory.place.a-plain-thing-under-the-pointer-is-a-place-and-a-pack-is-a-destination"],
        a_plain_thing_under_the_pointer_is_a_place_and_a_pack_is_a_destination,
    ),
    (
        "a_dragged_pack_is_looked_up_among_the_packs",
        &["inventory.place.a-dragged-pack-is-looked-up-among-the-packs"],
        a_dragged_pack_is_looked_up_among_the_packs,
    ),
    (
        "a_merge_is_answered_before_any_place_is_worked_out",
        &["inventory.place.a-merge-is-answered-before-any-place-is-worked-out"],
        a_merge_is_answered_before_any_place_is_worked_out,
    ),
    (
        "a_container_with_no_room_refuses_in_the_clients_own_words",
        &["inventory.place.a-container-with-no-room-refuses-in-the-clients-own-words"],
        a_container_with_no_room_refuses_in_the_clients_own_words,
    ),
    (
        "the_lock_and_the_grey_are_both_released_by_the_shards_own_answer",
        &["inventory.place.the-lock-and-the-grey-are-both-released-by-the-shards-own-answer"],
        the_lock_and_the_grey_are_both_released_by_the_shards_own_answer,
    ),
    (
        "a_refused_drop_takes_the_grey_back_off_the_icon",
        &["inventory.place.a-refused-drop-takes-the-grey-back-off-the-icon"],
        a_refused_drop_takes_the_grey_back_off_the_icon,
    ),
    // -- the hint a slot shows under a carried icon -----------------------------------------------
    (
        "which_hint_a_slot_shows_for_what_is_carried_over_it",
        &["inventory.drag-hint.which-hint-a-slot-shows-for-what-is-carried-over-it"],
        which_hint_a_slot_shows_for_what_is_carried_over_it,
    ),
    (
        "the_lists_that_take_no_drop_show_no_hint_rather_than_a_refusal",
        &["inventory.drag-hint.the-lists-that-take-no-drop-show-no-hint-rather-than-a-refusal"],
        the_lists_that_take_no_drop_show_no_hint_rather_than_a_refusal,
    ),
    (
        "setting_the_same_hint_twice_raises_nothing",
        &["inventory.drag-hint.setting-the-same-hint-twice-raises-nothing"],
        setting_the_same_hint_twice_raises_nothing,
    ),
    (
        "every_recorded_placement_can_be_asked_for_and_the_shards_answer_closes_it",
        &["inventory.body.every-recorded-placement-can-be-asked-for-and-the-shards-answer-closes-it"],
        every_recorded_placement_can_be_asked_for_and_the_shards_answer_closes_it,
    ),
    (
        "the_figures_own_picture_is_not_one_of_the_places_on_it",
        &["inventory.body.the-figures-own-picture-is-not-one-of-the-places-on-it"],
        the_figures_own_picture_is_not_one_of_the_places_on_it,
    ),
    (
        "what_can_be_worn_and_what_can_be_wielded_overlap_on_one_place_only",
        &["inventory.body.what-can-be-worn-and-what-can-be-wielded-overlap-on-one-place-only"],
        what_can_be_worn_and_what_can_be_wielded_overlap_on_one_place_only,
    ),
    (
        "every_recorded_equip_let_go_on_the_picture_asks_with_its_whole_list_of_places",
        &["inventory.body.every-recorded-equip-let-go-on-the-picture-asks-with-its-whole-list-of-places"],
        every_recorded_equip_let_go_on_the_picture_asks_with_its_whole_list_of_places,
    ),
    (
        "something_that_could_be_worn_or_wielded_is_still_worn_on_the_picture",
        &["inventory.body.something-that-could-be-worn-or-wielded-is-still-worn-on-the-picture"],
        something_that_could_be_worn_or_wielded_is_still_worn_on_the_picture,
    ),
    (
        "a_place_refuses_in_silence_where_the_picture_refuses_in_words",
        &["inventory.body.a-place-refuses-in-silence-where-the-picture-refuses-in-words"],
        a_place_refuses_in_silence_where_the_picture_refuses_in_words,
    ),
    (
        "use_progress_notice",
        &["use.progress-notice.names-the-object"],
        use_progress_notice,
    ),
    (
        "trade_drop_sends_add_to_trade",
        &["trade.drop.sends-add-to-trade-and-marks-the-item"],
        trade_drop_sends_add_to_trade,
    ),
    (
        "ground_container_contents_become_a_pickup",
        &["container.ground.contents-become-a-pickup"],
        ground_container_contents_become_a_pickup,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

const PLAYER: ObjectId = ObjectId(0x5000_0001);
/// The notice channel the strip across the top of the viewport draws.
const BUBBLE: u32 = 0x1A;
/// What an object lying in the world, or a container, carries.
const USEABLE_REMOTE: u32 = 0x20;

fn put(w: &mut World, id: ObjectId, name: &str, pwd: PublicWeenieDesc) {
    let mut it = dereth_client_model::Weenie::new(id);
    it.pwd = pwd;
    it.pwd.name = name.to_owned();
    it.valid = true;
    w.tables.weenies.insert(id, it);
}

/// The notice-strip lines, in order.
fn bubble_lines<'a>(v: &ScenarioView<'a>) -> Vec<&'a str> {
    v.notices()
        .iter()
        .filter_map(|n| match n {
            Notice::DisplayString {
                channel: BUBBLE,
                text,
            } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// A player with a pack, as every synthesised model scenario starts.
fn a_player(c: &mut HeadlessClient) {
    let w = c.world_mut();
    put(
        w,
        PLAYER,
        "Larktest",
        PublicWeenieDesc {
            bitfield: bitfield::PLAYER,
            obj_type: item_type::CREATURE,
            items_capacity: Some(102),
            containers_capacity: Some(7),
            ..PublicWeenieDesc::default()
        },
    );
    w.player = Some(PLAYER);
    w.tables.inventories.insert(
        PLAYER,
        dereth_client_model::objects::ObjectInventory::new(PLAYER),
    );
}

// =============================================================================================
// 1. use.confirmation.prints-the-same-line-and-the-material-name
// =============================================================================================

/// The dialog's yes prints the ordinary progress line, with no separate wording for a creature,
/// and both lines name the object the way the player sees it named.
pub fn a_confirmed_use_prints_the_same_line() {
    const ALTAR: ObjectId = ObjectId(0x5000_0002);
    const DOOR: ObjectId = ObjectId(0x5000_0003);

    let mut c = HeadlessClient::model();
    a_player(&mut c);
    let stuck_and_useable = |material: Option<u32>| PublicWeenieDesc {
        obj_type: item_type::MISC,
        bitfield: bitfield::STUCK,
        useability: Some(USEABLE_REMOTE),
        material_type: material,
        ..PublicWeenieDesc::default()
    };
    put(
        c.world_mut(),
        ALTAR,
        "Altar of Bael'Zharon",
        stuck_and_useable(None),
    );
    put(c.world_mut(), DOOR, "Door", stuck_and_useable(Some(0x3A)));
    c.world_mut()
        .install_material_names(std::collections::BTreeMap::from([(
            0x3A,
            "Bronze".to_owned(),
        )]));

    // The dialog's yes, which is the arm with no creature branch of its own.
    let mut requests = RecordingRequests::default();
    let mut sink = RecordingSink::default();
    c.world_mut().confirm_usage(
        &mut requests,
        &mut sink,
        ALTAR,
        dereth_client_model::inventory::SplitState::default(),
        ServerTime(2.0),
    );
    // …and the same for an object whose name carries its material.
    let mut door_sink = RecordingSink::default();
    c.world_mut().confirm_usage(
        &mut requests,
        &mut door_sink,
        DOOR,
        dereth_client_model::inventory::SplitState::default(),
        ServerTime(3.0),
    );
    let progress_line = c.view().world().using_notice(DOOR);

    let lines: Vec<String> = sink
        .0
        .iter()
        .chain(door_sink.0.iter())
        .filter_map(|n| match n {
            Notice::DisplayString {
                channel: BUBBLE,
                text,
            } => Some(text.clone()),
            _ => None,
        })
        .collect();

    c.assert_behaviour(
        "use.confirmation.prints-the-same-line-and-the-material-name",
        move |_| {
            lines == ["Using the Altar of Bael'Zharon", "Using the Bronze Door"]
                && progress_line == "Using the Bronze Door"
        },
    );
}

// =============================================================================================
// 2 and 3. The trade window
// =============================================================================================

const PARTNER: ObjectId = ObjectId(0x5000_0002);
const ITEM: ObjectId = ObjectId(0x8000_0A6E);
const ITEM2: ObjectId = ObjectId(0x8000_0A6F);
const ITEM3: ObjectId = ObjectId(0x8000_0A70);
const THEIRS: ObjectId = ObjectId(0x8000_0B00);

/// A client with a player, three carried items and one that is not the player's.
fn trading_client() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    a_player(&mut c);
    for (id, name) in [
        (ITEM, "Sturdy Iron Key"),
        (ITEM2, "Prismatic Taper"),
        (ITEM3, "Lead Scarab"),
        (THEIRS, "Pyreal"),
    ] {
        put(
            c.world_mut(),
            id,
            name,
            PublicWeenieDesc {
                icon_id: 0x0600_1234,
                // Mine are in my pack; theirs is not, and that is what decides whether a drop may
                // mark it.
                container_id: (id != THEIRS).then_some(PLAYER),
                ..PublicWeenieDesc::default()
            },
        );
    }
    put(c.world_mut(), PARTNER, "Alba", PublicWeenieDesc::default());
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeRegisterTrade {
            initiator: PLAYER,
            partner: PARTNER,
            stamp: 4.25,
        },
    ));
    c
}

fn add_row(
    item: ObjectId,
    side: u32,
    position: u32,
) -> dereth_protocol::trade::TradeAddToTradeRecv {
    dereth_protocol::trade::TradeAddToTradeRecv {
        item,
        side,
        container_properties: position,
    }
}

/// The window is filled by the shard: rows arrive at the positions it names, a flag moves only
/// the side it names, a failed add is rolled back, and a close empties the window without
/// taking it off the screen.
pub fn the_trade_window_follows_the_shards_lists() {
    let mut c = trading_client();
    let raised = c.view().world().trade.open;

    // Three adds at positions 0, 0 and 1: the non-zero one is what tells an insert index from a
    // flag, because 0, 0, 0 would give the same answer either way.
    c.when(Inbound::message(&add_row(ITEM, 1, 0)))
        .when(Inbound::message(&add_row(ITEM2, 1, 0)))
        .when(Inbound::message(&add_row(ITEM3, 1, 1)))
        .when(Inbound::message(&add_row(THEIRS, 2, 0)));
    let view = dereth_client::trade_view::trade(c.view().world());
    let ordered = view.self_rows.iter().map(|r| r.item).collect::<Vec<_>>() == [ITEM2, ITEM3, ITEM]
        && view.partner_rows.iter().map(|r| r.item).collect::<Vec<_>>() == [THEIRS]
        && view.self_rows[2].name == "Sturdy Iron Key";

    // A removal naming the wrong side moves nothing; the right one does.
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeRemoveFromTrade {
            item: ITEM,
            side: 2,
        },
    ));
    let wrong_side = dereth_client::trade_view::trade(c.view().world())
        .self_rows
        .len()
        == 3;
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeRemoveFromTrade {
            item: ITEM,
            side: 1,
        },
    ));
    let right_side = dereth_client::trade_view::trade(c.view().world())
        .self_rows
        .len()
        == 2;

    // The flags, each moving only its own side.
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: PLAYER },
    ))
    .when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: PARTNER },
    ));
    let both = c.view().world().trade.trade.both_accepted();
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeDeclineTradeRecv { source: PARTNER },
    ));
    let only_theirs =
        !c.view().world().trade.trade.partner_accepted && c.view().world().trade.trade.accepted;

    // A failed add is rolled back out of the player's own side.
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeTradeFailure {
            item: ITEM2,
            reason: 9,
        },
    ));
    let rolled_back = !c
        .view()
        .world()
        .trade
        .trade
        .self_list
        .iter()
        .any(|p| p.iid == ITEM2);

    // …and a close empties the window and leaves it up with nobody named in it.
    c.when(Inbound::message(&dereth_protocol::trade::TradeCloseTrade {
        reason: 2,
    }));
    let emptied = c.view().world().trade.open
        && c.view().world().trade.partner == ObjectId(0)
        && dereth_client::trade_view::trade(c.view().world())
            .self_rows
            .is_empty();

    c.assert_behaviour("trade.window.follows-the-shards-own-two-lists", move |_| {
        raised
            && ordered
            && wrong_side
            && right_side
            && both
            && only_theirs
            && rolled_back
            && emptied
    });
}

/// Accepting while the window disagrees with the shard sends an empty trade; the three control
/// buttons each send their own ask, and closing takes the traded marks back off.
pub fn a_stale_trade_window_accepts_nothing() {
    let mut c = trading_client();
    // One row the shard has confirmed.
    c.when(Inbound::message(&add_row(ITEM, 1, 0)));

    // The window shows two. That is the desync the client is looking for.
    c.when(Player::ui(UiRequest::TradeAccept {
        displayed_self: 2,
        displayed_partner: 0,
    }));
    let stale = matches!(
        c.outbound().last(),
        Some(Request::TradeAcceptTrade(m))
            if m.0.self_list.is_empty()
                && m.0.partner_list.is_empty()
                && m.0.partner == ObjectId(0)
    ) && !c.view().world().trade.trade.accepted
        && c.view().interaction().stats.trade_out_of_sync_sent == 1
        && c.view().interaction().stats.trade_accepts_sent == 0;

    // …and an agreeing window sends the real one.
    c.when(Player::ui(UiRequest::TradeAccept {
        displayed_self: 1,
        displayed_partner: 0,
    }));
    let agreeing = matches!(
        c.outbound().last(),
        Some(Request::TradeAcceptTrade(m))
            if m.0.self_list.len() == 1
                && m.0.self_list[0].iid == ITEM
                && m.0.partner == PARTNER
                && m.0.accepted == 1
    ) && c.view().world().trade.trade.accepted
        && c.view().interaction().stats.trade_accepts_sent == 1;

    // The three control buttons.
    c.when(Player::Ui(vec![
        UiRequest::TradeDecline,
        UiRequest::TradeReset,
    ]));
    let declined = !c.view().world().trade.trade.accepted
        && c.view().interaction().stats.trade_control_sent == 2;

    // An item the player drops on the table is marked as being traded -- which is the seam the
    // inventory guards read, and the reason the close below has anything to clear.
    c.when(Player::ui(UiRequest::TradeAddItem {
        item: ITEM2,
        position: 0,
    }));
    let marked = c
        .view()
        .world()
        .weenie(ITEM2)
        .is_some_and(|w| w.trade_state == 1)
        && c.view()
            .world()
            .weenie(THEIRS)
            .is_some_and(|w| w.trade_state == 0);
    c.when(Player::ui(UiRequest::TradeClose));
    let closed = !c.view().world().trade.open
        && c.view()
            .world()
            .weenie(ITEM2)
            .is_some_and(|w| w.trade_state == 0)
        && c.view().interaction().stats.trade_control_sent == 3;

    let opcodes: Vec<&'static str> = c
        .outbound()
        .iter()
        .filter_map(|r| match r {
            Request::TradeDeclineTrade(_) => Some("decline"),
            Request::TradeResetTrade(_) => Some("reset"),
            Request::TradeCloseTradeNegotiations(_) => Some("close"),
            _ => None,
        })
        .collect();

    c.assert_behaviour(
        "trade.controls.a-stale-window-accepts-an-empty-trade",
        move |_| {
            stale
                && agreeing
                && declined
                && marked
                && closed
                && opcodes == ["decline", "reset", "close"]
        },
    );
}

// =============================================================================================
// 4. shortcut.bar.drops-an-item-that-leaves-your-pack
// =============================================================================================

/// A side pack of the player's -- the destination of the ordinary move.
const PACK: ObjectId = ObjectId(0x5000_0020);
const SWORD: ObjectId = ObjectId(0x5000_0030);
const ROCK: ObjectId = ObjectId(0x5000_0031);
const CORPSE: ObjectId = ObjectId(0x8000_0A96);
const SWORD_SLOT: i32 = 3;
const ROCK_SLOT: i32 = 5;

/// A player carrying two shortcut-bound items, a side pack, and a corpse to drop into.
fn a_bar_with_two_shortcuts() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    {
        let w = c.world_mut();
        w.player = Some(PLAYER);
        w.tables.inventories.insert(
            PLAYER,
            dereth_client_model::objects::ObjectInventory::new(PLAYER),
        );
        for id in [PLAYER, PACK, SWORD, ROCK, CORPSE] {
            let mut wn = dereth_client_model::Weenie::new(id);
            wn.valid = true;
            wn.pwd.name = format!("obj{:X}", id.0 & 0xFFFF);
            w.tables.weenies.insert(id, wn);
        }
        let p = w.tables.weenies.get_mut(PLAYER).expect("seeded");
        p.pwd.items_capacity = Some(102);
        p.pwd.containers_capacity = Some(7);
        p.pwd.bitfield |= bitfield::OPENABLE;
        let pack = w.tables.weenies.get_mut(PACK).expect("seeded");
        pack.pwd.items_capacity = Some(24);
        pack.pwd.bitfield |= bitfield::OPENABLE;
        pack.pwd.container_id = Some(PLAYER);
        let corpse = w.tables.weenies.get_mut(CORPSE).expect("seeded");
        corpse.pwd.items_capacity = Some(24);
        corpse.pwd.bitfield |= bitfield::OPENABLE | bitfield::CORPSE;
        w.tables.inventories.insert(
            PACK,
            dereth_client_model::objects::ObjectInventory::new(PACK),
        );
        w.tables.inventories.insert(
            CORPSE,
            dereth_client_model::objects::ObjectInventory::new(CORPSE),
        );
        for id in [SWORD, ROCK] {
            w.tables
                .weenies
                .get_mut(id)
                .expect("seeded")
                .pwd
                .container_id = Some(PLAYER);
        }
        let inv = w.tables.inventories.get_mut(PLAYER).expect("seeded");
        inv.add_content(PACK, true, 0);
        inv.add_content(SWORD, false, 0);
        inv.add_content(ROCK, false, 1);
        w.remake_character_inventory();
        for (slot, item) in [(SWORD_SLOT, SWORD), (ROCK_SLOT, ROCK)] {
            assert!(
                w.player_system
                    .add_shortcut(dereth_protocol::login::ShortCutData {
                        index: slot,
                        object_id: item,
                        spell_id: 0,
                    }),
                "the bar takes slot {slot}"
            );
        }
    }
    c
}

/// The numeral any tile showing `item` would draw, read at the seam the panels read.
fn drawn_shortcut_num(c: &HeadlessClient, item: ObjectId) -> Option<u32> {
    use dereth_ui_screens::view::GameView as _;
    let v = c.view();
    v.hud()
        .view(v.objects())
        .slot_decoration(item)
        .and_then(|d| d.shortcut_num)
}

/// An item that leaves the player's own possession leaves the shortcut bar with it.
pub fn a_dropped_item_leaves_the_shortcut_bar() {
    let mut c = a_bar_with_two_shortcuts();
    let loaded = c.view().world().player_system.shortcut_slot_of(SWORD)
        == Some(SWORD_SLOT as usize)
        && drawn_shortcut_num(&c, SWORD) == Some(SWORD_SLOT as u32);

    // The removal the shard sends when an item drops to the corpse.
    c.when(Inbound::message(
        &dereth_protocol::objects::ItemServerSaysRemove { object: SWORD },
    ));
    let off_the_bar = c
        .view()
        .world()
        .player_system
        .shortcut_slot_of(SWORD)
        .is_none();
    let told_the_shard = c
        .outbound()
        .iter()
        .filter_map(|r| match r {
            Request::RemoveShortCut(m) => Some(m.index),
            _ => None,
        })
        .collect::<Vec<_>>()
        == [SWORD_SLOT as u32];

    // The corpse window opens on it, and the tile there carries no numeral.
    c.when(Inbound::message(
        &dereth_protocol::objects::ItemOnViewContents {
            container: CORPSE,
            contents: vec![ContentProfile {
                iid: SWORD,
                container_properties: 0,
            }],
        },
    ));
    let in_the_corpse = {
        use dereth_ui_screens::view::GameView as _;
        let v = c.view();
        v.hud()
            .view(v.objects())
            .container_contents(CORPSE)
            .contains(&SWORD)
    };
    let no_numeral = drawn_shortcut_num(&c, SWORD).is_none();

    // The other shortcut is untouched.
    let survivor = c.view().world().player_system.shortcut_slot_of(ROCK)
        == Some(ROCK_SLOT as usize)
        && drawn_shortcut_num(&c, ROCK) == Some(ROCK_SLOT as u32);

    // …and a move between the player's own containers keeps its shortcut and sends nothing.
    let mut own = a_bar_with_two_shortcuts();
    own.when(Inbound::message(
        &dereth_protocol::objects::ItemServerSaysContainId {
            item: ROCK,
            container: PACK,
            slot: 0,
            container_properties: 0,
        },
    ));
    let kept = own.view().world().is_owned_by_player(ROCK)
        && own.view().world().player_system.shortcut_slot_of(ROCK) == Some(ROCK_SLOT as usize)
        && !own
            .outbound()
            .iter()
            .any(|r| matches!(r, Request::RemoveShortCut(_)));

    c.assert_behaviour(
        "shortcut.bar.drops-an-item-that-leaves-your-pack",
        move |_| {
            loaded
                && off_the_bar
                && told_the_shard
                && in_the_corpse
                && no_numeral
                && survivor
                && kept
        },
    );
}

// =============================================================================================
// 5. inventory.refusal.answers-for-the-item-the-client-asked-about
// =============================================================================================

const LOCKED: ObjectId = ObjectId(0x8000_0A22);
const OTHER: ObjectId = ObjectId(0x8000_09A2);

fn ghosted(w: &mut World, id: ObjectId, name: &str) {
    let mut row = dereth_client_model::Weenie::new(id);
    row.pwd.name = name.to_owned();
    row.waiting = true;
    w.tables.weenies.insert(id, row);
}

/// A client holding a request lock on one of two ghosted items.
fn two_ghosted_items(lock_held: bool) -> HeadlessClient {
    let mut c = HeadlessClient::model();
    let w = c.world_mut();
    ghosted(w, LOCKED, "Sturdy Iron Key");
    ghosted(w, OTHER, "Prismatic Taper");
    if lock_held {
        w.request_lock
            .record(LOCKED, InventoryRequest::Move, ServerTime(12.5));
    }
    c
}

fn a_refusal(
    object: ObjectId,
    reason: u32,
) -> dereth_protocol::objects::CharacterServerSaysAttemptFailed {
    dereth_protocol::objects::CharacterServerSaysAttemptFailed { object, reason }
}

/// The refusal is answered for the item the client is waiting on, not the one it names.
pub fn a_refusal_answers_for_the_item_we_asked_about() {
    let mut c = two_ghosted_items(true);
    c.when(Inbound::message(&a_refusal(OTHER, 0x1D)));
    let substituted = c.view().interaction().stats.attempts_failed == 1
        && c.view().world().weenie(LOCKED).is_some_and(|w| !w.waiting)
        && c.view().world().weenie(OTHER).is_some_and(|w| w.waiting)
        && c.view().interaction().last_refusal.as_deref()
            == Some("The Sturdy Iron Key can't be moved - you're too busy")
        && c.view().world().request_lock.is_idle();

    // With nothing pending the refusal keeps its own item.
    let mut idle = two_ghosted_items(false);
    idle.when(Inbound::message(&a_refusal(OTHER, 0x1D)));
    let kept_its_own = idle.view().interaction().stats.attempts_failed == 1
        && idle
            .view()
            .world()
            .weenie(OTHER)
            .is_some_and(|w| !w.waiting)
        && idle
            .view()
            .world()
            .weenie(LOCKED)
            .is_some_and(|w| w.waiting)
        && idle.view().world().request_lock.is_idle();

    // An item the client has never seen makes nothing happen, and says so rather than counting
    // nothing at all.
    let mut stranger = two_ghosted_items(false);
    stranger.when(Inbound::message(&a_refusal(ObjectId(0x5000_1234), 0x1D)));
    let nothing_ran = stranger.view().interaction().stats.attempts_failed == 0
        && stranger
            .view()
            .interaction()
            .stats
            .attempts_failed_unknown_object
            == 1
        && stranger.view().interaction().last_refusal.is_none();

    // …and a lock held on an item that has since been destroyed stays held: the client waits,
    // which is what retail does.
    let mut wedged = HeadlessClient::model();
    wedged
        .world_mut()
        .request_lock
        .record(LOCKED, InventoryRequest::Move, ServerTime(12.5));
    wedged.when(Inbound::message(&a_refusal(OTHER, 0x1D)));
    let still_held = wedged
        .view()
        .interaction()
        .stats
        .attempts_failed_unknown_object
        == 1
        && wedged.view().world().request_lock.pending == InventoryRequest::Move
        && wedged.view().world().request_lock.object == Some(LOCKED);

    // The second reader of the substituted item: an interrupted auto-equip is abandoned for it.
    const BLOCKED: ObjectId = ObjectId(0x8000_0500);
    let mut unblock = two_ghosted_items(true);
    ghosted(unblock.world_mut(), BLOCKED, "Silk Shirt");
    unblock.world_mut().unblock = dereth_client_model::inventory::equip::UnblockState {
        blocking_id: Some(LOCKED),
        blocked_id: Some(BLOCKED),
        unblock_attempt_num: 1,
        ..dereth_client_model::inventory::equip::UnblockState::default()
    };
    unblock.when(Inbound::message(&a_refusal(OTHER, 0x1D)));
    let abandoned = unblock.view().interaction().stats.unblocks_abandoned == 1
        && unblock.view().world().unblock.unblock_attempt_num == 0
        && unblock
            .view()
            .world()
            .weenie(BLOCKED)
            .is_some_and(|w| !w.waiting);

    c.assert_behaviour(
        "inventory.refusal.answers-for-the-item-the-client-asked-about",
        move |_| substituted && kept_its_own && nothing_ran && still_held && abandoned,
    );
}

// -------------------------------------------------------------------------------------------

#[test]
fn scenario_a_confirmed_use_prints_the_same_line() {
    scenario("a_confirmed_use_prints_the_same_line");
}

#[test]
fn scenario_the_trade_window_follows_the_shards_lists() {
    scenario("the_trade_window_follows_the_shards_lists");
}

#[test]
fn scenario_a_stale_trade_window_accepts_nothing() {
    scenario("a_stale_trade_window_accepts_nothing");
}

#[test]
fn scenario_a_dropped_item_leaves_the_shortcut_bar() {
    scenario("a_dropped_item_leaves_the_shortcut_bar");
}

#[test]
fn scenario_a_refusal_answers_for_the_item_we_asked_about() {
    scenario("a_refusal_answers_for_the_item_we_asked_about");
}

// =============================================================================================
// The ground container, the drag-split, the stow and the shop.
// =============================================================================================

/// Non-usable flag `0x01` -- what a creature carries, and the reason a player falls past the
/// "is this usable?" line to the trade test.
const USEABLE_NO: u32 = 0x01;

const CORPSE_A: ObjectId = ObjectId(0x7000_0001);
const CORPSE_B: ObjectId = ObjectId(0x7000_0002);
const LOOT_A: ObjectId = ObjectId(0x7000_0003);
const LOOT_B: ObjectId = ObjectId(0x7000_0004);
const SIDE_PACK: ObjectId = ObjectId(0x7000_0005);
const FULL_PACK: ObjectId = ObjectId(0x7000_0006);
const WEDGE: ObjectId = ObjectId(0x7000_0007);
const STRANGER: ObjectId = ObjectId(0x7000_0008);
const ALTAR: ObjectId = ObjectId(0x7000_0009);

/// A container lying loose in the world: in nothing, wielded by nobody, with a capacity of its
/// own, openable, and useable from a distance.
fn ground_container(w: &mut World, id: ObjectId, name: &str, corpse: bool) {
    put(
        w,
        id,
        name,
        PublicWeenieDesc {
            obj_type: item_type::CONTAINER,
            bitfield: bitfield::OPENABLE | if corpse { bitfield::CORPSE } else { 0 },
            items_capacity: Some(10),
            useability: Some(USEABLE_REMOTE),
            ..PublicWeenieDesc::default()
        },
    );
}

/// One loose item inside `container`.
fn item_in(w: &mut World, id: ObjectId, name: &str, container: ObjectId) {
    put(
        w,
        id,
        name,
        PublicWeenieDesc {
            obj_type: item_type::MISC,
            container_id: Some(container),
            value: Some(50),
            ..PublicWeenieDesc::default()
        },
    );
}

/// A side pack of the player's own, with `capacity` item slots and `filled` of them taken.
fn side_pack(w: &mut World, id: ObjectId, capacity: u8, filled: &[ObjectId]) {
    put(
        w,
        id,
        "Belt Pouch",
        PublicWeenieDesc {
            obj_type: item_type::CONTAINER,
            bitfield: bitfield::OPENABLE,
            items_capacity: Some(capacity),
            container_id: Some(PLAYER),
            useability: Some(USEABLE_REMOTE),
            ..PublicWeenieDesc::default()
        },
    );
    w.tables
        .inventories
        .insert(id, dereth_client_model::objects::ObjectInventory::new(id));
    for (slot, item) in filled.iter().enumerate() {
        put(
            w,
            *item,
            "Ballast",
            PublicWeenieDesc {
                container_id: Some(id),
                ..PublicWeenieDesc::default()
            },
        );
        let inv = w.tables.inventories.get_mut(id).expect("just inserted");
        inv.add_content(*item, false, slot);
    }
    let inv = w
        .tables
        .inventories
        .get_mut(PLAYER)
        .expect("a_player made one");
    inv.add_content(id, true, 0);
    w.remake_character_inventory();
}

/// Drive the use entry point directly and hand back everything it produced.
///
/// The gesture step cannot be used where the *outcome* is the claim -- a refusal is a value, not
/// a request -- so the three refusal scenarios below call the model's own entry point and book
/// what it answered.
fn use_it(w: &mut World, id: ObjectId, t: f64) -> (UseOutcome, RecordingRequests, RecordingSink) {
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let outcome = w.use_object(&mut req, &mut out, id, SplitState::default(), ServerTime(t));
    (outcome, req, out)
}

// ---------------------------------------------------------------------------------------------
// 6. container.ground.a-pickup-goes-into-whichever-pack-the-player-has-open
// ---------------------------------------------------------------------------------------------

/// Where a pickup out of the open ground container would actually go, probed with nothing open,
/// with a pack the loot fits in open, and with one it does not.
///
/// **The lock is put back after each probe, deliberately.** A pickup records the player's
/// one-request-at-a-time lock and only the shard's own answer clears it; a scenario's invented
/// request can never be answered, so without the restore the second probe would be refused by
/// the first probe's lock and the measurement would read as a finding.
fn pickup_destination(w: &mut World, item: ObjectId) -> Option<ObjectId> {
    let saved = w.request_lock;
    let mut req = RecordingRequests::default();
    let mut sink = RecordingSink::default();
    w.place_in_backpack(
        &mut req,
        &mut sink,
        item,
        false,
        SplitState::default(),
        ServerTime(9.0),
    );
    // The ghost the pending mark raised comes off too: the reply that would have taken it off is
    // not coming either.
    w.set_waiting_state(item, false);
    w.request_lock = saved;
    req.0.iter().find_map(|r| match r {
        Request::PutItemInContainer(m) => Some(m.container),
        Request::StackableSplitToContainer(m) => Some(m.container),
        _ => None,
    })
}

/// An open side pack takes the loot; a full one is passed over and the loot lands where it would
/// have landed anyway.
pub fn a_pickup_follows_the_pack_the_player_has_open() {
    let mut c = HeadlessClient::model();
    a_player(&mut c);
    {
        let w = c.world_mut();
        ground_container(w, CORPSE_A, "Drudge Slave", true);
        item_in(w, LOOT_A, "Lead Scarab", CORPSE_A);
        side_pack(w, SIDE_PACK, 24, &[]);
        side_pack(w, FULL_PACK, 1, &[WEDGE]);
    }

    c.when(Player::DoubleClick(CORPSE_A));
    let opened = c.view().world().ground_object == Some(CORPSE_A);

    let w = c.world_mut();
    w.open_container = None;
    let to_the_player = pickup_destination(w, LOOT_A);

    w.open_container = None;
    let took_the_notice = w.on_new_parent_container(SIDE_PACK);
    let fits = w.will_item_fit_in_container(LOOT_A, SIDE_PACK, SplitState::default());
    let into_the_open_pack = pickup_destination(w, LOOT_A);

    w.open_container = None;
    assert!(
        w.on_new_parent_container(FULL_PACK),
        "the full pack is the player's too"
    );
    let does_not_fit = !w.will_item_fit_in_container(LOOT_A, FULL_PACK, SplitState::default());
    let spilled_past_it = pickup_destination(w, LOOT_A);

    c.assert_behaviour(
        "container.ground.a-pickup-goes-into-whichever-pack-the-player-has-open",
        move |_| {
            opened
                && to_the_player == Some(PLAYER)
                && took_the_notice
                && fits
                && into_the_open_pack == Some(SIDE_PACK)
                && does_not_fit
                && spilled_past_it == to_the_player
        },
    );
}

// ---------------------------------------------------------------------------------------------
// 7. container.ground.a-second-one-closes-the-first-and-tells-the-shard
// ---------------------------------------------------------------------------------------------

/// The second corpse closes the first and names it on the wire; the shard closing one is not
/// answered at all.
pub fn a_second_ground_container_closes_the_first() {
    let mut c = HeadlessClient::model();
    a_player(&mut c);
    {
        let w = c.world_mut();
        ground_container(w, CORPSE_A, "Drudge Slave", true);
        ground_container(w, CORPSE_B, "Mosswart Corpse", true);
    }

    let w = c.world_mut();
    let (_, _, _) = use_it(w, CORPSE_A, 1.0);
    let first_open = w.ground_object == Some(CORPSE_A);
    let (_, req, out) = use_it(w, CORPSE_B, 2.0);
    let second_open = w.ground_object == Some(CORPSE_B);
    let first_panel_closed = out.0.contains(&Notice::SetGroundObject(ObjectId(0)));
    let named_the_one_being_left = req
        .0
        .iter()
        .filter_map(|r| match r {
            Request::NoLongerViewingContents(m) => Some(m.container),
            _ => None,
        })
        .collect::<Vec<_>>()
        == vec![CORPSE_A];

    // …and when the shard is the one that ends the view, nothing goes back for it.
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let was_ground =
        w.handle_stop_viewing_object_contents(&mut req, &mut out, CORPSE_B, ServerTime(3.0));
    let shard_closed_it = was_ground
        && w.ground_object.is_none()
        && out.0.contains(&Notice::SetGroundObject(ObjectId(0)))
        && req.0.is_empty();

    c.assert_behaviour(
        "container.ground.a-second-one-closes-the-first-and-tells-the-shard",
        move |_| {
            first_open
                && second_open
                && first_panel_closed
                && named_the_one_being_left
                && shard_closed_it
        },
    );
}

// ---------------------------------------------------------------------------------------------
// 8. container.ground.the-contents-reply-fills-the-panel-unless-it-answers-a-pickup
// ---------------------------------------------------------------------------------------------

/// A client that has just double-clicked a corpse lying in the world.
fn a_corpse_the_player_asked_to_view() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    a_player(&mut c);
    {
        let w = c.world_mut();
        ground_container(w, CORPSE_A, "Drudge Slave", true);
        ground_container(w, CORPSE_B, "Mosswart Corpse", true);
    }
    c.when(Player::DoubleClick(CORPSE_A));
    c
}

fn contents(container: ObjectId, ids: &[ObjectId]) -> dereth_protocol::objects::ItemOnViewContents {
    dereth_protocol::objects::ItemOnViewContents {
        container,
        contents: ids
            .iter()
            .map(|id| ContentProfile {
                iid: *id,
                container_properties: 0,
            })
            .collect(),
    }
}

/// What the panel would draw is exactly what the shard listed -- and two replies that are not
/// the requested container's open nothing.
pub fn the_contents_reply_fills_the_ground_panel() {
    let mut c = a_corpse_the_player_asked_to_view();
    c.when(Inbound::message(&contents(CORPSE_A, &[LOOT_A, LOOT_B])));
    let drawn: Vec<ObjectId> = {
        use dereth_ui_screens::view::GameView as _;
        let v = c.view();
        let panels = v.hud().view(v.objects());
        panels.container_contents(CORPSE_A).to_vec()
    };
    let opened = c.view().interaction().stats.ground_panels_opened == 1;
    let exactly_what_was_listed = drawn == vec![LOOT_A, LOOT_B];

    // A reply for a container the player never asked about raises nothing.
    let mut other = a_corpse_the_player_asked_to_view();
    other.when(Inbound::message(&contents(CORPSE_B, &[LOOT_A])));
    let wrong_container = other.view().interaction().stats.ground_panels_opened == 0;

    // …and neither does the reply that is only the answer to picking this container up.
    let mut pickup = a_corpse_the_player_asked_to_view();
    pickup
        .world_mut()
        .request_lock
        .record(CORPSE_A, InventoryRequest::PickUp, ServerTime(2.0));
    pickup.when(Inbound::message(&contents(CORPSE_A, &[LOOT_A])));
    let answering_a_pickup = pickup.view().interaction().stats.ground_panels_opened == 0;

    c.assert_behaviour(
        "container.ground.the-contents-reply-fills-the-panel-unless-it-answers-a-pickup",
        move |_| opened && exactly_what_was_listed && wrong_container && answering_a_pickup,
    );
}

// ---------------------------------------------------------------------------------------------
// 9. container.ground.opening-one-takes-no-inventory-lock
// ---------------------------------------------------------------------------------------------

/// Opening a corpse asks the shard for its contents and takes no lock at all, so a second one is
/// never refused for the first being busy.
pub fn opening_a_ground_container_takes_no_lock() {
    let mut c = HeadlessClient::model();
    a_player(&mut c);
    {
        let w = c.world_mut();
        ground_container(w, CORPSE_A, "Drudge Slave", true);
        ground_container(w, CORPSE_B, "Mosswart Corpse", true);
    }

    let w = c.world_mut();
    let (_, req, _) = use_it(w, CORPSE_A, 1.0);
    let asked = w.requested_ground_object == Some(CORPSE_A) && !req.0.is_empty();
    let idle = w.request_lock.is_idle();
    // Three more opens, because the claim is that *no* request kind is ever taken here and one
    // open could miss a kind set on a later pass.
    for t in 2..5 {
        let _ = use_it(w, CORPSE_A, f64::from(t));
    }
    let still_idle = w.request_lock.is_idle()
        && w.request_lock.pending != InventoryRequest::ViewAsGroundContainer;

    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let second_not_refused =
        w.attempt_set_ground_object(&mut req, &mut out, CORPSE_B, ServerTime(6.0))
            != GroundObjectResult::Busy;

    c.assert_behaviour(
        "container.ground.opening-one-takes-no-inventory-lock",
        move |_| asked && idle && still_idle && second_not_refused,
    );
}

// ---------------------------------------------------------------------------------------------
// 10. container.ground.the-contents-reply-frees-a-waiting-request-unless-it-is-a-pickup
// ---------------------------------------------------------------------------------------------

/// The reply releases whatever was waiting on that container -- except a pickup of the container
/// itself, and except a reply for some other container.
pub fn the_contents_reply_frees_a_waiting_request() {
    let mut c = HeadlessClient::model();
    a_player(&mut c);
    {
        let w = c.world_mut();
        ground_container(w, CORPSE_A, "Drudge Slave", true);
        ground_container(w, CORPSE_B, "Mosswart Corpse", true);
    }

    let w = c.world_mut();
    let _ = use_it(w, CORPSE_A, 1.0);
    w.request_lock
        .record(CORPSE_A, InventoryRequest::PickUp, ServerTime(2.0));

    // The negative first: the lock really is engaged, and a player can see that it is.
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let held_is_visible =
        w.attempt_set_ground_object(&mut req, &mut out, CORPSE_B, ServerTime(2.5))
            == GroundObjectResult::Busy
            && out
                .0
                .iter()
                .any(|n| matches!(n, Notice::DisplayString { text, .. } if text == BUSY_MESSAGE))
            && req.0.is_empty();

    let mut out = RecordingSink::default();
    let pickup_keeps_it = !w.on_view_contents(CORPSE_A, &[], &mut out, ServerTime(3.0))
        && w.request_lock.pending == InventoryRequest::PickUp
        && w.request_lock.object == Some(CORPSE_A);

    // Anything else waiting on that container is released, and the client is usable again.
    let mut released = true;
    for kind in [
        InventoryRequest::Move,
        InventoryRequest::Merge,
        InventoryRequest::Wield,
        InventoryRequest::PutInContainer,
    ] {
        let mut k = HeadlessClient::model();
        a_player(&mut k);
        {
            let w = k.world_mut();
            ground_container(w, CORPSE_A, "Drudge Slave", true);
            ground_container(w, CORPSE_B, "Mosswart Corpse", true);
        }
        let w = k.world_mut();
        let _ = use_it(w, CORPSE_A, 1.0);
        w.request_lock.record(CORPSE_A, kind, ServerTime(2.0));
        let mut out = RecordingSink::default();
        let opened = w.on_view_contents(CORPSE_A, &[], &mut out, ServerTime(3.0));
        let mut req = RecordingRequests::default();
        let mut out2 = RecordingSink::default();
        let usable_again =
            w.attempt_set_ground_object(&mut req, &mut out2, CORPSE_B, ServerTime(4.0))
                != GroundObjectResult::Busy;
        released &=
            opened && w.request_lock.is_idle() && w.request_lock.object.is_none() && usable_again;
    }

    // …and a reply for a container that is not the requested one releases nothing.
    let mut elsewhere = HeadlessClient::model();
    a_player(&mut elsewhere);
    {
        let w = elsewhere.world_mut();
        ground_container(w, CORPSE_A, "Drudge Slave", true);
        ground_container(w, CORPSE_B, "Mosswart Corpse", true);
    }
    let w = elsewhere.world_mut();
    let _ = use_it(w, CORPSE_A, 1.0);
    w.request_lock
        .record(CORPSE_A, InventoryRequest::Move, ServerTime(2.0));
    let mut out = RecordingSink::default();
    let another_container_releases_nothing =
        !w.on_view_contents(CORPSE_B, &[], &mut out, ServerTime(3.0))
            && w.request_lock.pending == InventoryRequest::Move
            && w.request_lock.object == Some(CORPSE_A);

    c.assert_behaviour(
        "container.ground.the-contents-reply-frees-a-waiting-request-unless-it-is-a-pickup",
        move |_| {
            held_is_visible && pickup_keeps_it && released && another_container_releases_nothing
        },
    );
}

// ---------------------------------------------------------------------------------------------
// 11. container.own-pack.opens-where-it-is-and-is-not-the-ground-container
// ---------------------------------------------------------------------------------------------

/// A belt pouch of the player's own opens where it hangs, and does not become the ground
/// container.
pub fn your_own_pack_opens_where_it_hangs() {
    let mut c = HeadlessClient::model();
    a_player(&mut c);
    side_pack(c.world_mut(), SIDE_PACK, 24, &[]);

    c.when(Player::DoubleClick(SIDE_PACK));
    let raised = c
        .view()
        .notices()
        .contains(&Notice::OpenContainedContainer(SIDE_PACK));
    let not_the_ground_container = c.view().world().ground_object.is_none();

    c.assert_behaviour(
        "container.own-pack.opens-where-it-is-and-is-not-the-ground-container",
        move |_| raised && not_the_ground_container,
    );
}

// ---------------------------------------------------------------------------------------------
// 12. trade.open.double-clicking-another-player-starts-negotiations
// ---------------------------------------------------------------------------------------------

/// Double-clicking somebody else asks them to trade, and a drawn weapon asks nobody.
pub fn double_clicking_a_player_starts_a_trade() {
    let mut c = HeadlessClient::model();
    a_player(&mut c);
    put(
        c.world_mut(),
        STRANGER,
        "Alba",
        PublicWeenieDesc {
            bitfield: bitfield::PLAYER,
            obj_type: item_type::CREATURE,
            // A capacity of his own, which is why another player is not stuffed into your pack.
            items_capacity: Some(102),
            useability: Some(USEABLE_NO),
            ..PublicWeenieDesc::default()
        },
    );
    let reads_as_a_trade = c.view().world().determine_use_result(STRANGER) == UseResult::Trade;

    let w = c.world_mut();
    let (outcome, req, _) = use_it(w, STRANGER, 1.0);
    let asked = outcome
        == UseOutcome::Dispatched {
            result: UseResult::Trade,
            sent: true,
        }
        && matches!(req.0.as_slice(), [Request::OpenTradeNegotiations(m)] if m.partner == STRANGER);

    w.combat.combat_mode = dereth_client_model::combat::CombatMode::Melee;
    let (outcome, req, _) = use_it(w, STRANGER, 2.0);
    let refused_in_combat = outcome
        == UseOutcome::Dispatched {
            result: UseResult::Trade,
            sent: false,
        }
        && req.0.is_empty();

    c.assert_behaviour(
        "trade.open.double-clicking-another-player-starts-negotiations",
        move |_| reads_as_a_trade && asked && refused_in_combat,
    );
}

// ---------------------------------------------------------------------------------------------
// 13. use.confirmation.a-player-killer-altar-asks-before-anything-is-sent
// ---------------------------------------------------------------------------------------------

/// The altar asks first, sends nothing while the player thinks, and then uses exactly as any
/// other use does.
pub fn a_player_killer_altar_asks_first() {
    let mut c = HeadlessClient::model();
    a_player(&mut c);
    put(
        c.world_mut(),
        ALTAR,
        "Altar of Bael'Zharon",
        PublicWeenieDesc {
            obj_type: item_type::MISC,
            // The player-killer switch, and stuck in the world: without the second an altar reads
            // as a pickup and is never confirmed at all.
            bitfield: 0x0000_0400 | bitfield::STUCK,
            useability: Some(USEABLE_REMOTE),
            ..PublicWeenieDesc::default()
        },
    );

    c.when(Player::DoubleClick(ALTAR));
    let asked = c.view().notices().contains(&Notice::UsageConfirmation {
        object: ALTAR,
        kind: UsageConfirmation::PkAltar,
    }) && c.outbound().is_empty()
        && UsageConfirmation::PkAltar
            .prompt()
            .starts_with("Using this altar will make you a pl");

    let w = c.world_mut();
    let busy_before = w.magic.busy_count;
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    w.confirm_usage(
        &mut req,
        &mut out,
        ALTAR,
        SplitState::default(),
        ServerTime(4.0),
    );
    let then_used = matches!(req.0.as_slice(), [Request::UseEvent(m)] if m.object == ALTAR)
        && w.magic.busy_count == busy_before + 1;

    c.assert_behaviour(
        "use.confirmation.a-player-killer-altar-asks-before-anything-is-sent",
        move |_| asked && then_used,
    );
}

// ---------------------------------------------------------------------------------------------
// 14. use.refusal.each-refusal-is-named-rather-than-collapsed-into-one
// ---------------------------------------------------------------------------------------------

/// Five refusals, each keeping its own reason, and none of them sending anything.
pub fn every_refusal_keeps_its_own_reason() {
    const SEALED: ObjectId = ObjectId(0x7000_0010);
    const BEAST: ObjectId = ObjectId(0x7000_0011);
    const TRADED: ObjectId = ObjectId(0x7000_0012);
    const SWORD_IN_THE_PACK: ObjectId = ObjectId(0x7000_0013);
    const NEVER_HEARD_OF: ObjectId = ObjectId(0x7000_0014);

    let mut c = HeadlessClient::model();
    a_player(&mut c);
    let w = c.world_mut();
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();

    // A container that is not openable.
    put(
        w,
        SEALED,
        "Strongbox",
        PublicWeenieDesc {
            obj_type: item_type::CONTAINER,
            items_capacity: Some(4),
            useability: Some(USEABLE_REMOTE),
            ..PublicWeenieDesc::default()
        },
    );
    let sealed = w.attempt_set_ground_object(&mut req, &mut out, SEALED, ServerTime(1.0))
        == GroundObjectResult::NotOpenable { creature: false };

    // A creature, which is refused without a word.
    put(
        w,
        BEAST,
        "Drudge Slave",
        PublicWeenieDesc {
            obj_type: item_type::CREATURE,
            ..PublicWeenieDesc::default()
        },
    );
    let before = out.0.len();
    let creature = w.attempt_set_ground_object(&mut req, &mut out, BEAST, ServerTime(1.0))
        == GroundObjectResult::NotOpenable { creature: true }
        && out.0.len() == before;

    // An object the client has never heard of, which says so.
    let stranger = w.attempt_set_ground_object(&mut req, &mut out, NEVER_HEARD_OF, ServerTime(1.0))
        == GroundObjectResult::NoRepresentation
        && out.0.iter().any(|n| {
            matches!(
                n,
                Notice::DisplayString { text, .. }
                    if text == "The object has no representation on the client."
            )
        });
    let nothing_opened = w.ground_object.is_none() && req.0.is_empty();

    // An item already on the trade table.
    put(
        w,
        TRADED,
        "Sturdy Iron Key",
        PublicWeenieDesc {
            obj_type: item_type::MISC,
            useability: Some(USEABLE_REMOTE),
            container_id: Some(PLAYER),
            ..PublicWeenieDesc::default()
        },
    );
    w.weenie_mut(TRADED).expect("just created").trade_state = 1;
    let (outcome, req_traded, _) = use_it(w, TRADED, 2.0);
    let being_traded =
        outcome == UseOutcome::Refused(UseRefusal::BeingTraded) && req_traded.0.is_empty();

    // …and a weapon that has to be in hand and is not.
    put(
        w,
        SWORD_IN_THE_PACK,
        "Shortsword",
        PublicWeenieDesc {
            obj_type: item_type::MISC,
            container_id: Some(PLAYER),
            useability: Some(dereth_client_model::weenie::item_useable::WIELDED),
            ..PublicWeenieDesc::default()
        },
    );
    let (outcome, req_sword, _) = use_it(w, SWORD_IN_THE_PACK, 3.0);
    let must_be_wielded =
        outcome == UseOutcome::Refused(UseRefusal::MustBeWielded) && req_sword.0.is_empty();

    c.assert_behaviour(
        "use.refusal.each-refusal-is-named-rather-than-collapsed-into-one",
        move |_| {
            sealed && creature && stranger && nothing_opened && being_traded && must_be_wielded
        },
    );
}

#[test]
fn scenario_a_pickup_follows_the_pack_the_player_has_open() {
    scenario("a_pickup_follows_the_pack_the_player_has_open");
}

#[test]
fn scenario_a_second_ground_container_closes_the_first() {
    scenario("a_second_ground_container_closes_the_first");
}

#[test]
fn scenario_the_contents_reply_fills_the_ground_panel() {
    scenario("the_contents_reply_fills_the_ground_panel");
}

#[test]
fn scenario_opening_a_ground_container_takes_no_lock() {
    scenario("opening_a_ground_container_takes_no_lock");
}

#[test]
fn scenario_the_contents_reply_frees_a_waiting_request() {
    scenario("the_contents_reply_frees_a_waiting_request");
}

#[test]
fn scenario_your_own_pack_opens_where_it_hangs() {
    scenario("your_own_pack_opens_where_it_hangs");
}

#[test]
fn scenario_double_clicking_a_player_starts_a_trade() {
    scenario("double_clicking_a_player_starts_a_trade");
}

#[test]
fn scenario_a_player_killer_altar_asks_first() {
    scenario("a_player_killer_altar_asks_first");
}

#[test]
fn scenario_every_refusal_keeps_its_own_reason() {
    scenario("every_refusal_keeps_its_own_reason");
}

// =============================================================================================
// The drag-split: what the stack it came off looks like, and what the player may do next.
// =============================================================================================

const PYREAL: ObjectId = ObjectId(0x7000_0020);
const NEW_STACK: ObjectId = ObjectId(0x7000_0021);
const SPLIT_PACK: ObjectId = ObjectId(0x7000_0022);
/// How many are in the stack to begin with.
const WHOLE_STACK: u32 = 20;

/// The three readings each scenario below takes of the stack a split came off: how many the
/// player sees in it, whether its icon is ghosted, and what the one-request-at-a-time lock holds.
#[derive(Debug, PartialEq, Eq)]
struct Stack {
    count: Option<u16>,
    ghosted: bool,
    lock: InventoryRequest,
}

fn stack_of(w: &World, id: ObjectId) -> Stack {
    Stack {
        count: w.weenie(id).and_then(|x| x.pwd.stack_size),
        ghosted: w.weenie(id).is_some_and(|x| x.waiting),
        lock: w.request_lock.pending,
    }
}

/// A player carrying a stack of twenty and a pack to drag part of it into.
fn a_player_with_a_stack() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    a_player(&mut c);
    {
        let w = c.world_mut();
        side_pack(w, SPLIT_PACK, 24, &[]);
        let n = u16::try_from(WHOLE_STACK).expect("twenty fits");
        put(
            w,
            PYREAL,
            "Pyreal",
            PublicWeenieDesc {
                wcid: 0x0111,
                stack_size: Some(n),
                max_stack_size: Some(25_000),
                value: Some(u32::from(n)),
                container_id: Some(PLAYER),
                ..PublicWeenieDesc::default()
            },
        );
    }
    c
}

/// The shard saying what is left of a stack.
fn new_count(
    sequence: u8,
    item: ObjectId,
    amount: u32,
    worth: u32,
) -> dereth_protocol::items::ItemUpdateStackSize {
    dereth_protocol::items::ItemUpdateStackSize {
        sequence,
        item,
        amount,
        new_value: worth,
    }
}

/// The source stack keeps its old count until the shard answers, then takes the new one.
pub fn a_split_leaves_the_source_at_its_new_count() {
    let mut c = a_player_with_a_stack();
    let at_rest = stack_of(c.view().world(), PYREAL)
        == Stack {
            count: Some(20),
            ghosted: false,
            lock: InventoryRequest::None,
        };

    let w = c.world_mut();
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    w.attempt_split_to_container(
        &mut req,
        &mut out,
        PYREAL,
        SPLIT_PACK,
        0,
        5,
        ServerTime(1.0),
        false,
    )
    .expect("nothing is pending, so the split is accepted");
    let asked = matches!(
        req.0.as_slice(),
        [Request::StackableSplitToContainer(m)]
            if m.stack == PYREAL && m.container == SPLIT_PACK && m.amount == 5
    );
    // The client predicts nothing: the stack still shows twenty, ghosted, with the lock on it.
    let waiting = stack_of(w, PYREAL)
        == Stack {
            count: Some(20),
            ghosted: true,
            lock: InventoryRequest::Split,
        }
        && w.request_lock.object == Some(PYREAL);

    c.when(Inbound::message(&new_count(1, PYREAL, 15, 4_242)));
    let settled = stack_of(c.view().world(), PYREAL)
        == Stack { count: Some(15), ghosted: false, lock: InventoryRequest::None }
        // The count and the worth are two numbers, and a fixture where they are equal could not
        // tell one being written into the other.
        && c.view().world().weenie(PYREAL).and_then(|x| x.pwd.value) == Some(4_242);

    c.assert_behaviour(
        "inventory.split.the-source-stack-shows-its-new-count-once-the-shard-answers",
        move |_| at_rest && asked && waiting && settled,
    );
}

/// Until the shard says what is left, the player is held to one request at a time.
pub fn a_second_split_waits_for_the_first_answer() {
    let mut c = a_player_with_a_stack();
    {
        let w = c.world_mut();
        let mut req = RecordingRequests::default();
        let mut out = RecordingSink::default();
        w.attempt_split_to_container(
            &mut req,
            &mut out,
            PYREAL,
            SPLIT_PACK,
            0,
            5,
            ServerTime(1.0),
            false,
        )
        .expect("the first split is accepted");
    }

    // The reply that places the newly split stack is about **that** stack, so it lets nobody go.
    c.when(Inbound::message(
        &dereth_protocol::objects::ItemServerSaysContainId {
            item: NEW_STACK,
            container: SPLIT_PACK,
            slot: 0,
            container_properties: 0,
        },
    ));
    let still_held = c.view().world().request_lock.pending == InventoryRequest::Split;

    let w = c.world_mut();
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let refused = w.attempt_split_to_container(
        &mut req,
        &mut out,
        PYREAL,
        SPLIT_PACK,
        1,
        5,
        ServerTime(2.0),
        false,
    ) == Err(BUSY_MESSAGE)
        && req.0.is_empty()
        && out
            .0
            .iter()
            .any(|n| matches!(n, Notice::DisplayString { text, .. } if text == BUSY_MESSAGE));

    // Now the stack's own answer arrives, which is what lets go.
    c.when(Inbound::message(&new_count(1, PYREAL, 15, 15)));
    let freed = c.view().world().request_lock.pending == InventoryRequest::None;

    let w = c.world_mut();
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let accepted = w
        .attempt_split_to_container(
            &mut req,
            &mut out,
            PYREAL,
            SPLIT_PACK,
            1,
            5,
            ServerTime(3.0),
            false,
        )
        .is_ok()
        && matches!(req.0.as_slice(), [Request::StackableSplitToContainer(_)]);

    c.assert_behaviour(
        "inventory.split.until-that-answer-arrives-every-later-request-is-refused",
        move |_| still_held && refused && freed && accepted,
    );
}

/// The whole of a stack is a move; part of it is a split.
pub fn a_whole_stack_drop_is_a_move() {
    let mut c = a_player_with_a_stack();
    let whole = SplitState::whole_stack(WHOLE_STACK).is_whole_stack();

    let w = c.world_mut();
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    w.attempt_put_in_container(
        &mut req,
        &mut out,
        PYREAL,
        SPLIT_PACK,
        0,
        ServerTime(1.0),
        false,
    )
    .expect("nothing is pending, so the move is accepted");
    let moved = matches!(req.0.as_slice(), [Request::PutItemInContainer(m)] if m.item == PYREAL);

    // …and the contrast, on a client of its own so the lock the move took cannot decide it.
    let mut part = a_player_with_a_stack();
    let w = part.world_mut();
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    w.attempt_split_to_container(
        &mut req,
        &mut out,
        PYREAL,
        SPLIT_PACK,
        0,
        5,
        ServerTime(1.0),
        false,
    )
    .expect("accepted");
    let split = matches!(req.0.as_slice(), [Request::StackableSplitToContainer(_)]);

    c.assert_behaviour(
        "inventory.split.dragging-a-whole-stack-is-a-move-and-not-a-split",
        move |_| whole && moved && split,
    );
}

/// A stack's count only moves forward, and an update it cannot hold is refused.
pub fn a_stale_or_oversized_stack_count_is_ignored() {
    let count = |c: &HeadlessClient| {
        c.view()
            .world()
            .weenie(PYREAL)
            .and_then(|x| x.pwd.stack_size)
    };

    let mut c = a_player_with_a_stack();
    c.when(Inbound::message(&new_count(5, PYREAL, 15, 15)));
    let applied = count(&c) == Some(15);
    c.when(Inbound::message(&new_count(4, PYREAL, 9, 9)));
    let stale_dropped = count(&c) == Some(15);
    c.when(Inbound::message(&new_count(6, PYREAL, 7, 7)));
    let still_moving = count(&c) == Some(7);

    c.world_mut()
        .weenie_mut(PYREAL)
        .expect("seeded")
        .pwd
        .max_stack_size = Some(10);
    c.when(Inbound::message(&new_count(7, PYREAL, 11, 11)));
    let too_big_refused = count(&c) == Some(7);
    c.when(Inbound::message(&new_count(8, PYREAL, 10, 10)));
    let exactly_full_applies = count(&c) == Some(10);

    // An object the client does not have changes nothing at all.
    let before = stack_of(c.view().world(), PYREAL);
    c.when(Inbound::message(&new_count(9, ObjectId(0x0BAD_0BAD), 3, 3)));
    let unknown_dropped = stack_of(c.view().world(), PYREAL) == before;

    // The shared order: a quality change to the same count makes an older stack message stale.
    // A private counter would have taken it, because on a private counter it would be the first.
    let mut shared = a_player_with_a_stack();
    {
        let key = dereth_client_model::qualities::StatKey::new(
            dereth_client_model::qualities::StatType::Int,
            12,
        );
        let mut out = RecordingSink::default();
        assert!(shared.world_mut().apply_stat_update(
            PYREAL,
            key,
            dereth_client_model::qualities::StatValue::Int(11),
            9,
            &mut out
        ));
    }
    let quality_count = shared
        .view()
        .world()
        .weenie(PYREAL)
        .and_then(|x| x.pwd.stack_size);
    shared.when(Inbound::message(&new_count(8, PYREAL, 4, 4)));
    let ordered_against_the_quality = shared
        .view()
        .world()
        .weenie(PYREAL)
        .and_then(|x| x.pwd.stack_size)
        == quality_count;
    shared.when(Inbound::message(&new_count(10, PYREAL, 4, 4)));
    let and_the_next_one_lands = shared
        .view()
        .world()
        .weenie(PYREAL)
        .and_then(|x| x.pwd.stack_size)
        == Some(4);

    c.assert_behaviour(
        "inventory.stack-size.a-stale-or-oversized-update-is-ignored",
        move |_| {
            applied
                && stale_dropped
                && still_moving
                && too_big_refused
                && exactly_full_applies
                && unknown_dropped
                && ordered_against_the_quality
                && and_the_next_one_lands
        },
    );
}

#[test]
fn scenario_a_split_leaves_the_source_at_its_new_count() {
    scenario("a_split_leaves_the_source_at_its_new_count");
}

#[test]
fn scenario_a_second_split_waits_for_the_first_answer() {
    scenario("a_second_split_waits_for_the_first_answer");
}

#[test]
fn scenario_a_whole_stack_drop_is_a_move() {
    scenario("a_whole_stack_drop_is_a_move");
}

#[test]
fn scenario_a_stale_or_oversized_stack_count_is_ignored() {
    scenario("a_stale_or_oversized_stack_count_is_ignored");
}

// =============================================================================================
// The shop.
// =============================================================================================

const MERCHANT: ObjectId = ObjectId(0x7000_0040);
const MERCHANT_TOO: ObjectId = ObjectId(0x7000_0041);
const OIL: ObjectId = ObjectId(0x7000_0042);
const TAPER: ObjectId = ObjectId(0x7000_0043);
const SCARAB: ObjectId = ObjectId(0x7000_0044);
const MY_KEY: ObjectId = ObjectId(0x7000_0045);
const MY_RING: ObjectId = ObjectId(0x7000_0046);
const TREASURE: ObjectId = ObjectId(0x7000_0047);
const SOMEWHERE_ELSE: ObjectId = ObjectId(0x7000_0048);
const MY_POUCH: ObjectId = ObjectId(0x7000_0049);
const IN_THE_POUCH: ObjectId = ObjectId(0x7000_004A);
const MY_TAPERS: ObjectId = ObjectId(0x7000_004B);
const BALLAST: ObjectId = ObjectId(0x7000_004C);

/// One shop, as the shard describes it: a general store that deals in odds and ends, pays nine
/// tenths and charges the full price, and will not take anything worth more than ten thousand.
///
/// **Synthesised**, and the counts a recording would have carried are deliberately absent: the
/// claim is what the client does with a shop, and how many shops the recordings hold is the
/// corpus's own question.
fn a_shop(
    merchant: ObjectId,
    rows: &[(ObjectId, &str, u32)],
) -> dereth_protocol::trade::VendorInfo {
    dereth_protocol::trade::VendorInfo {
        merchant_id: merchant,
        profile: dereth_protocol::trade::VendorProfile {
            item_types: item_type::MISC,
            min_value: -1,
            max_value: 10_000,
            magic: 1,
            buy_price: 0.9,
            sell_price: 1.0,
            trade_id: 0,
            trade_num: 0,
            trade_name: String::new(),
        },
        items: rows
            .iter()
            .map(|(id, name, value)| dereth_protocol::trade::ItemProfile {
                // A vendor's stock is unlimited, which is what every recorded stock row but one
                // carries.
                amount: -1,
                iid: *id,
                pwd: Some(PublicWeenieDesc {
                    // The descriptor's own header says which of its optional fields are on the
                    // wire; a row that set the fields and not the header would encode bytes the
                    // reader is not looking for and silently swallow the row after it.
                    header: pwd_header::VALUE | pwd_header::STACK_SIZE | pwd_header::MAX_STACK_SIZE,
                    name: (*name).to_owned(),
                    obj_type: item_type::MISC,
                    value: Some(*value),
                    stack_size: Some(1),
                    max_stack_size: Some(1),
                    ..PublicWeenieDesc::default()
                }),
            })
            .collect(),
    }
}

/// A player with a purse. The coin is a property of the character, not a poke at the panel's
/// cached total: opening a shop refills that total from the property, so a purse with nothing
/// behind it is wiped the moment a window appears.
fn a_solvent_player(c: &mut HeadlessClient) {
    a_player(c);
    let w = c.world_mut();
    {
        let me = w.weenie_mut(PLAYER).expect("a_player made one");
        me.pwd.bitfield |= bitfield::OPENABLE;
        let mut q = dereth_client_model::qualities::Qualities::new();
        q.ints
            .get_or_insert_with(std::collections::BTreeMap::new)
            .insert(dereth_client_model::vendor::COIN_VALUE, 1_000_000);
        me.qualities = Some(q);
    }
    w.shop.total_value = 1_000_000;
}

/// Create an object in `container`'s contents list, which is what the capacity checks walk.
fn add_carried(w: &mut World, container: ObjectId, id: ObjectId, obj_type: u32, value: u32) {
    put(
        w,
        id,
        "thing",
        PublicWeenieDesc {
            obj_type,
            value: Some(value),
            container_id: Some(container),
            ..PublicWeenieDesc::default()
        },
    );
    let held: Vec<ContentProfile> = w
        .tables
        .weenies
        .iter()
        .filter(|(_, x)| x.pwd.container_id == Some(container))
        .map(|(k, x)| ContentProfile {
            iid: k,
            container_properties: u32::from(x.is_container()),
        })
        .collect();
    w.view_object_contents(container, &held, &mut RecordingSink::default());
}

/// A solvent player standing at an open general store.
fn at_a_shop(rows: &[(ObjectId, &str, u32)]) -> HeadlessClient {
    let mut c = HeadlessClient::model();
    a_solvent_player(&mut c);
    c.when(Inbound::message(&a_shop(MERCHANT, rows)));
    c
}

/// The shard's own message is what puts a shop on screen, and a second one replaces it.
pub fn the_shards_message_opens_the_shop() {
    let mut c = HeadlessClient::model();
    a_solvent_player(&mut c);
    let nothing_yet =
        c.view().world().vendor_id().is_none() && c.view().interaction().stats.vendor_opens == 0;

    c.when(Inbound::message(&a_shop(
        MERCHANT,
        &[(OIL, "Oil of Rendering", 100)],
    )));
    let opened = {
        let v = c.view();
        v.interaction().stats.vendor_opens == 1
            && v.world().vendor_id() == Some(MERCHANT)
            && v.world().shop.mode == dereth_client_model::vendor::ShopMode::Buy
            && v.world().shop.stock.len() == 1
            && v.world().shop.stock[0].iid == OIL
            && v.world().shop.stock[0].pwd.name == "Oil of Rendering"
            && v.world().shop.stock[0].amount == -1
    };

    c.when(Inbound::message(&a_shop(
        MERCHANT_TOO,
        &[(TAPER, "Prismatic Taper", 12), (SCARAB, "Lead Scarab", 40)],
    )));
    let replaced = {
        let v = c.view();
        let names: Vec<&str> = v
            .world()
            .shop
            .stock
            .iter()
            .map(|r| r.pwd.name.as_str())
            .collect();
        v.interaction().stats.vendor_opens == 2
            && v.world().vendor_id() == Some(MERCHANT_TOO)
            && names == ["Prismatic Taper", "Lead Scarab"]
    };

    c.assert_behaviour(
        "vendor.shop.the-shards-own-message-opens-the-shop-and-replaces-it",
        move |_| nothing_yet && opened && replaced,
    );
}

/// The buy button sends at once; the basket merges two presses into one purchase.
pub fn the_button_and_the_basket_each_buy_once() {
    let mut c = at_a_shop(&[(OIL, "Oil of Rendering", 100)]);
    c.when(Player::ui(UiRequest::VendorBuySingle {
        item: OIL,
        split: 1,
    }));
    let straight_away = matches!(
        c.outbound().last(),
        Some(Request::VendorBuy(m))
            if m.vendor_id == MERCHANT && m.items.len() == 1 && m.items[0].iid == OIL
                && m.items[0].amount == 1
    ) && c.view().interaction().stats.vendor_buys == 1;

    let mut basket = at_a_shop(&[(OIL, "Oil of Rendering", 100)]);
    basket.when(Player::Ui(vec![
        UiRequest::VendorAddToBuyList {
            item: OIL,
            split: 1,
        },
        UiRequest::VendorAddToBuyList {
            item: OIL,
            split: 1,
        },
    ]));
    let filled = basket.view().world().shop.buy_list.len() == 2 && basket.outbound().is_empty();
    basket.when(Player::ui(UiRequest::VendorBuyAll));
    let one_purchase_for_two = matches!(
        basket.outbound(),
        [Request::VendorBuy(m)]
            if m.vendor_id == MERCHANT && m.items.len() == 1 && m.items[0].iid == OIL
                && m.items[0].amount == 2
    );

    c.assert_behaviour(
        "vendor.buy.the-button-and-the-basket-each-send-one-purchase",
        move |_| straight_away && filled && one_purchase_for_two,
    );
}

/// The quantity dialled in is honoured only for a row that stacks.
pub fn the_quantity_only_moves_a_stackable_row() {
    let mut c = at_a_shop(&[(OIL, "Oil of Rendering", 100)]);

    /// How many of it the purchase the client built asks for.
    fn bought(w: &mut World, want: i32, at: f64) -> Option<i32> {
        let mut req = RecordingRequests::default();
        let mut out = RecordingSink::default();
        w.buy_single_item(OIL, want, &mut req, &mut out, ServerTime(at))
            .ok()?;
        req.0.iter().find_map(|r| match r {
            Request::VendorBuy(m) => m.items.first().map(|i| i.amount),
            _ => None,
        })
    }

    let w = c.world_mut();
    let as_the_shard_sold_it = w.shop.stock[0].pwd.stack_size.unwrap_or(0) < 2;
    let one_at_a_time = bought(w, 7, 1.0) == Some(1);

    // **Synthesised from here**: the same row with a stack behind it. The shard's own rows are
    // sold one at a time, so nothing a recording carries can tell the two arms apart.
    w.shop.stock[0].pwd.stack_size = Some(20);
    let in_the_number_asked_for = bought(w, 7, 2.0) == Some(7);
    // The boundary from below, which is the value a real stock row carries: a stack of one is
    // not a stack.
    w.shop.stock[0].pwd.stack_size = Some(1);
    let a_stack_of_one_is_not_a_stack = bought(w, 9, 3.0) == Some(1);
    w.shop.stock[0].pwd.stack_size = Some(2);
    let and_a_stack_of_two_is = bought(w, 5, 4.0) == Some(5);

    // …and the price the money check runs against grows with the number.
    w.shop.stock[0].pwd.stack_size = Some(20);
    w.shop.stock[0].pwd.value = Some(2_000);
    let pwd = w.shop.stock[0].pwd.clone();
    let priced = (
        w.shop.profile.vendor_sell_price(&pwd, 1),
        w.shop.profile.vendor_sell_price(&pwd, 7),
    ) == (100, 700);

    c.assert_behaviour(
        "vendor.buy.the-quantity-slider-only-moves-a-stackable-row",
        move |_| {
            as_the_shard_sold_it
                && one_at_a_time
                && in_the_number_asked_for
                && a_stack_of_one_is_not_a_stack
                && and_a_stack_of_two_is
                && priced
        },
    );
}

/// Neither an empty purse nor a full pack reaches the shard.
pub fn a_broke_or_full_player_buys_nothing() {
    let mut c = at_a_shop(&[(OIL, "Oil of Rendering", 100)]);
    let w = c.world_mut();
    w.shop.total_value = 0;
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let broke = w.buy_single_item(OIL, 1, &mut req, &mut out, ServerTime(1.0))
        == Err(dereth_client_model::vendor::BuyRefusal::NotEnoughMoney)
        && req.0.is_empty()
        && out.0.iter().any(|n| {
            matches!(
                n,
                Notice::DisplayString { channel: BUBBLE, text }
                    if text == "You don't have enough money"
            )
        });

    // A pack with one slot and something already in it.
    let mut full = HeadlessClient::model();
    a_solvent_player(&mut full);
    {
        let w = full.world_mut();
        w.weenie_mut(PLAYER)
            .expect("a_player made one")
            .pwd
            .items_capacity = Some(1);
        add_carried(w, PLAYER, BALLAST, item_type::MISC, 5);
    }
    full.when(Inbound::message(&a_shop(
        MERCHANT,
        &[(OIL, "Oil of Rendering", 100)],
    )));
    let w = full.world_mut();
    w.shop.total_value = 1_000_000;
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let no_room = w.buy_single_item(OIL, 1, &mut req, &mut out, ServerTime(1.0))
        == Err(dereth_client_model::vendor::BuyRefusal::NoFreeSlot)
        && req.0.is_empty()
        && out.0.iter().any(|n| {
            matches!(
                n,
                Notice::DisplayString { text, .. }
                    if text == "You must empty some slots in your backpack first"
            )
        });

    c.assert_behaviour(
        "vendor.buy.a-player-with-no-money-or-no-room-is-refused-and-sends-nothing",
        move |_| broke && no_room,
    );
}

/// The sell list marks what is in it, and the close button asks before walking away from a
/// basket.
pub fn the_sell_list_marks_what_is_in_it() {
    let mut c = at_a_shop(&[(OIL, "Oil of Rendering", 100)]);
    {
        let w = c.world_mut();
        add_carried(w, PLAYER, MY_KEY, item_type::MISC, 100);
        add_carried(w, PLAYER, MY_RING, item_type::MISC, 100);
    }

    let w = c.world_mut();
    let mut out = RecordingSink::default();
    let marked = w.add_item_to_sell(MY_KEY, &mut out)
        && w.add_item_to_sell(MY_RING, &mut out)
        && w.weenie(MY_KEY).expect("carried").sell_state == 1
        && w.weenie(MY_RING).expect("carried").sell_state == 1
        && w.shop.sell_list.len() == 2
        && out.0.contains(&Notice::AddItemToSell(MY_KEY));

    let taken_back_out = w.remove_from_sell_list(MY_KEY)
        && w.weenie(MY_KEY).expect("carried").sell_state == 0
        && w.weenie(MY_RING).expect("carried").sell_state == 1;

    // The close button, with something still in each basket in turn.
    let mut out = RecordingSink::default();
    let asks = w.add_to_buy_list(OIL, 1)
        && w.close_vendor_button(&mut out)
            == Err(
                "You have not completed all transactions. Are you sure you want to leave this \
                 vendor?",
            )
        && w.vendor_id().is_some();
    w.shop.buy_list.clear();
    let asks_for_the_other_basket_too = w.close_vendor_button(&mut out).is_err();

    let mut out = RecordingSink::default();
    let closed = w.close_vendor(&mut out)
        && w.weenie(MY_RING).expect("carried").sell_state == 0
        && w.vendor_id().is_none()
        && out.0.contains(&Notice::CloseVendor);

    // …and with both baskets empty the button itself closes it.
    let mut empty = at_a_shop(&[(OIL, "Oil of Rendering", 100)]);
    let w = empty.world_mut();
    let mut out = RecordingSink::default();
    let button_closes = w.close_vendor_button(&mut out) == Ok(true) && w.vendor_id().is_none();

    c.assert_behaviour(
        "vendor.sell.the-list-marks-what-is-in-it-and-the-close-button-asks-first",
        move |_| {
            marked
                && taken_back_out
                && asks
                && asks_for_the_other_basket_too
                && closed
                && button_closes
        },
    );
}

/// Selling everything sends one sale carrying every row, and clears the marks.
pub fn selling_the_whole_list_sends_one_message() {
    let mut c = at_a_shop(&[(OIL, "Oil of Rendering", 100)]);
    {
        let w = c.world_mut();
        add_carried(w, PLAYER, MY_KEY, item_type::MISC, 100);
        add_carried(w, PLAYER, MY_RING, item_type::MISC, 100);
    }

    let w = c.world_mut();
    let mut out = RecordingSink::default();
    let in_the_list = w.add_item_to_sell(MY_KEY, &mut out) && w.add_item_to_sell(MY_RING, &mut out);

    let mut req = RecordingRequests::default();
    let sold = w.sell_all(&mut req, &mut out, ServerTime(1.0));
    let one_message = matches!(
        req.0.as_slice(),
        [Request::VendorSell(m)]
            if m.vendor_id == MERCHANT
                && m.items.len() == 2
                && m.items.iter().all(|i| i.amount == 1)
                && m.items.iter().any(|i| i.iid == MY_KEY)
                && m.items.iter().any(|i| i.iid == MY_RING)
    );
    let marks_gone = w.weenie(MY_KEY).expect("carried").sell_state == 0
        && w.weenie(MY_RING).expect("carried").sell_state == 0
        && w.shop.sell_list.is_empty();

    c.assert_behaviour(
        "vendor.sell.selling-the-whole-list-sends-one-message-and-clears-the-marks",
        move |_| in_the_list && sold && one_message && marks_gone,
    );
}

/// Every reason a shop will not take a thing keeps its own words.
pub fn an_unwanted_item_is_refused_in_the_shops_words() {
    let mut c = at_a_shop(&[(OIL, "Oil of Rendering", 100)]);

    // The four the profile itself answers, read through the shop the shard described.
    let thing = |value: u32, obj_type: u32| PublicWeenieDesc {
        value: Some(value),
        obj_type,
        ..PublicWeenieDesc::default()
    };
    let w = c.world_mut();
    let wrong_sort = w
        .shop
        .profile
        .refusal_message(&thing(100, item_type::ARMOR))
        == Some("That item cannot be sold here");
    let worth_nothing = w.shop.profile.refusal_message(&thing(0, item_type::MISC))
        == Some("That item has no value and cannot be sold");
    w.shop.profile.min_value = 50;
    let too_cheap = w.shop.profile.refusal_message(&thing(10, item_type::MISC))
        == Some("That item is too cheap to sell here");
    w.shop.profile.min_value = -1;
    let too_valuable = w
        .shop
        .profile
        .refusal_message(&thing(20_000, item_type::MISC))
        == Some("That item is too valuable to sell here");

    // …and the same refusal reached by dragging a real item onto the list.
    add_carried(w, PLAYER, TREASURE, item_type::MISC, 20_000);
    let mut out = RecordingSink::default();
    let dragged = w.drag_item_acceptable(TREASURE)
        == Some("That item is too valuable to sell here")
        && !w.add_item_to_sell(TREASURE, &mut out)
        && w.weenie(TREASURE).expect("carried").sell_state == 0
        && w.shop.sell_list.is_empty();

    // An item that is nowhere near the player takes the containment arm, which runs first.
    put(
        w,
        SOMEWHERE_ELSE,
        "Someone else's Pyreal",
        PublicWeenieDesc {
            obj_type: item_type::MISC,
            value: Some(10),
            ..PublicWeenieDesc::default()
        },
    );
    let not_carried =
        w.drag_item_acceptable(SOMEWHERE_ELSE) == Some("You can only sell items you are carrying");

    // A pack with something still in it.
    put(
        w,
        MY_POUCH,
        "Belt Pouch",
        PublicWeenieDesc {
            obj_type: item_type::CONTAINER,
            value: Some(100),
            container_id: Some(PLAYER),
            bitfield: bitfield::OPENABLE,
            items_capacity: Some(24),
            ..PublicWeenieDesc::default()
        },
    );
    add_carried(w, MY_POUCH, IN_THE_POUCH, item_type::MISC, 1);
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let not_empty = w.needs_pack_slot(MY_POUCH)
        && w.sell_single_item(
            MY_POUCH,
            SplitState::default(),
            &mut req,
            &mut out,
            ServerTime(1.0),
        ) == Err("Cannot sell container that isn't empty")
        && req.0.is_empty();

    // Part of a stack, and then the whole of it.
    add_carried(w, PLAYER, MY_TAPERS, item_type::MISC, 100);
    w.weenie_mut(MY_TAPERS).expect("carried").pwd.stack_size = Some(10);
    let half = SplitState {
        split_size: 5,
        max_split_size: 10,
    };
    let part_of_a_stack = !half.is_whole_stack()
        && w.sell_single_item(MY_TAPERS, half, &mut req, &mut out, ServerTime(2.0))
            == Err("Cannot sell part of a stack")
        && req.0.is_empty();
    let whole = SplitState {
        split_size: 10,
        max_split_size: 10,
    };
    let the_whole_stack_goes =
        w.sell_single_item(MY_TAPERS, whole, &mut req, &mut out, ServerTime(3.0)) == Ok(true)
            && matches!(
                req.0.as_slice(),
                [Request::VendorSell(m)] if m.items.len() == 1 && m.items[0].amount == 1
            );

    c.assert_behaviour(
        "vendor.sell.an-item-the-vendor-will-not-take-is-refused-in-its-own-words",
        move |_| {
            wrong_sort
                && worth_nothing
                && too_cheap
                && too_valuable
                && dragged
                && not_carried
                && not_empty
                && part_of_a_stack
                && the_whole_stack_goes
        },
    );
}

/// The shop and the ground container close each other, and re-opening the one already open
/// closes neither.
pub fn a_shop_and_a_ground_container_cannot_share() {
    let shop = a_shop(MERCHANT, &[(OIL, "Oil of Rendering", 100)]);

    let mut c = HeadlessClient::model();
    a_solvent_player(&mut c);
    ground_container(c.world_mut(), CORPSE_A, "Drudge Slave", true);
    let w = c.world_mut();
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    w.set_ground_object(&mut req, &mut out, Some(CORPSE_A), true, ServerTime(0.0));
    let corpse_open = w.ground_object == Some(CORPSE_A);
    req.0.clear();
    out.0.clear();
    w.handle_vendor_info(&shop, &mut out, &mut req, ServerTime(1.0));
    let the_shop_closed_it = w.ground_object.is_none()
        && w.vendor_id().is_some()
        && req
            .0
            .iter()
            .any(|r| matches!(r, Request::NoLongerViewingContents(_)));

    // The other edge: a different ground container closes the shop before its contents arrive.
    let mut back = HeadlessClient::model();
    a_solvent_player(&mut back);
    ground_container(back.world_mut(), CORPSE_B, "Mosswart Corpse", true);
    let w = back.world_mut();
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    w.handle_vendor_info(&shop, &mut out, &mut req, ServerTime(0.0));
    let shop_open = w.vendor_id().is_some();
    out.0.clear();
    req.0.clear();
    let the_corpse_closed_the_shop =
        w.attempt_set_ground_object(&mut req, &mut out, CORPSE_B, ServerTime(1.0))
            == GroundObjectResult::Opened
            && w.vendor_id().is_none()
            && (w.ground_object, w.requested_ground_object) == (Some(CORPSE_B), Some(CORPSE_B))
            && out.0 == [Notice::CloseVendor]
            && req.0.is_empty();

    // …and re-opening the container that is already open leaves an independent shop alone.
    w.handle_vendor_info(&shop, &mut out, &mut req, ServerTime(2.0));
    w.ground_object = Some(CORPSE_B);
    w.requested_ground_object = Some(CORPSE_B);
    out.0.clear();
    req.0.clear();
    let the_same_one_changes_neither =
        !w.set_ground_object(&mut req, &mut out, Some(CORPSE_B), true, ServerTime(3.0))
            && w.vendor_id().is_some()
            && out.0.is_empty()
            && req.0.is_empty();

    c.assert_behaviour(
        "vendor.window.the-shop-and-the-ground-container-cannot-both-be-open",
        move |_| {
            corpse_open
                && the_shop_closed_it
                && shop_open
                && the_corpse_closed_the_shop
                && the_same_one_changes_neither
        },
    );
}

#[test]
fn scenario_the_shards_message_opens_the_shop() {
    scenario("the_shards_message_opens_the_shop");
}

#[test]
fn scenario_the_button_and_the_basket_each_buy_once() {
    scenario("the_button_and_the_basket_each_buy_once");
}

#[test]
fn scenario_the_quantity_only_moves_a_stackable_row() {
    scenario("the_quantity_only_moves_a_stackable_row");
}

#[test]
fn scenario_a_broke_or_full_player_buys_nothing() {
    scenario("a_broke_or_full_player_buys_nothing");
}

#[test]
fn scenario_the_sell_list_marks_what_is_in_it() {
    scenario("the_sell_list_marks_what_is_in_it");
}

#[test]
fn scenario_selling_the_whole_list_sends_one_message() {
    scenario("selling_the_whole_list_sends_one_message");
}

#[test]
fn scenario_an_unwanted_item_is_refused_in_the_shops_words() {
    scenario("an_unwanted_item_is_refused_in_the_shops_words");
}

#[test]
fn scenario_a_shop_and_a_ground_container_cannot_share() {
    scenario("a_shop_and_a_ground_container_cannot_share");
}

// ---------------------------------------------------------------------------------------------
// trade.removal.a-later-confirmation-puts-the-id-back-and-a-reset-forgets-every-refusal
//
// This claim never touches an element: it reads the client's own record of the negotiation, so it
// holds on a model client and needs no retail data file.
// ---------------------------------------------------------------------------------------------

/// The two players and the two items the removal scenarios use.
const TRADE_ME: ObjectId = ObjectId(0x5000_0001);
const TRADE_PARTNER: ObjectId = ObjectId(0x5000_0002);
const TRADE_CONFIRMED: ObjectId = ObjectId(0x8000_0A6E);
const TRADE_REFUSED: ObjectId = ObjectId(0x8000_0A6F);

/// Put the player and the two items into the client's own world.
fn a_trader_with_two_things(c: &mut HeadlessClient) {
    let w = c.world_mut();
    w.player = Some(TRADE_ME);
    for (id, name) in [
        (TRADE_CONFIRMED, "Sturdy Iron Key"),
        (TRADE_REFUSED, "Prismatic Taper"),
    ] {
        let mut o = dereth_client_model::Weenie::new(id);
        o.pwd.name = name.into();
        o.pwd.icon_id = 0x0600_103F;
        // Both are in the player's pack, so the table will take them.
        o.pwd.container_id = Some(TRADE_ME);
        o.valid = true;
        w.tables.weenies.insert(id, o);
    }
    let mut partner = dereth_client_model::Weenie::new(TRADE_PARTNER);
    partner.pwd.name = "Alba".into();
    partner.valid = true;
    w.tables.weenies.insert(TRADE_PARTNER, partner);
}

/// Register, open, confirm one row and agree on both sides -- the shard's own messages, through
/// the production decode.
fn an_open_accepted_trade(c: &mut HeadlessClient) {
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeRegisterTrade {
            initiator: TRADE_ME,
            partner: TRADE_PARTNER,
            stamp: 4.25,
        },
    ))
    .when(Inbound::message(&dereth_protocol::trade::TradeOpenTrade {
        source: TRADE_PARTNER,
    }))
    .when(Inbound::message(
        &dereth_protocol::trade::TradeAddToTradeRecv {
            item: TRADE_CONFIRMED,
            side: 1,
            container_properties: 0,
        },
    ))
    .when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: TRADE_ME },
    ))
    .when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv {
            source: TRADE_PARTNER,
        },
    ));
    let t = &c.view().world().trade.trade;
    assert!(
        t.accepted && t.partner_accepted,
        "the premise: both sides have agreed"
    );
    assert_eq!(
        t.self_list.len(),
        1,
        "the premise: one confirmed row on your side"
    );
}

/// A refusal is about one item, a later confirmation of that item undoes it, and ending the
/// negotiation forgets every refusal it was carrying.
pub fn a_refusal_is_about_one_item_and_does_not_outlive_the_trade() {
    let mut c = HeadlessClient::model();
    a_trader_with_two_things(&mut c);
    an_open_accepted_trade(&mut c);

    c.when(Inbound::message(
        &dereth_protocol::trade::TradeTradeFailure {
            item: TRADE_REFUSED,
            reason: 9,
        },
    ));
    let refused = c.view().world().trade.self_removed == vec![TRADE_REFUSED];

    c.when(Inbound::message(
        &dereth_protocol::trade::TradeAddToTradeRecv {
            item: TRADE_REFUSED,
            side: 1,
            container_properties: 0,
        },
    ));
    let put_back = {
        let v = dereth_client::trade_view::trade(c.view().world());
        v.self_removed.is_empty() && v.self_rows.iter().any(|r| r.item == TRADE_REFUSED)
    };

    // …and each of the two ways a negotiation ends forgets every refusal it was carrying.
    let mut cleared = Vec::new();
    for which in 0..2u8 {
        let mut fresh = HeadlessClient::model();
        a_trader_with_two_things(&mut fresh);
        an_open_accepted_trade(&mut fresh);
        fresh.when(Inbound::message(
            &dereth_protocol::trade::TradeTradeFailure {
                item: TRADE_REFUSED,
                reason: 9,
            },
        ));
        assert_eq!(
            fresh.view().world().trade.self_removed,
            vec![TRADE_REFUSED],
            "the premise: the refusal is being carried"
        );
        if which == 0 {
            fresh.when(Inbound::message(
                &dereth_protocol::trade::TradeResetTradeRecv {
                    source: TRADE_PARTNER,
                },
            ));
        } else {
            fresh.when(Inbound::message(&dereth_protocol::trade::TradeCloseTrade {
                reason: 2,
            }));
        }
        cleared.push(fresh.view().world().trade.self_removed.is_empty());
    }

    c.assert_behaviour(
        "trade.removal.a-later-confirmation-puts-the-id-back-and-a-reset-forgets-every-refusal",
        move |_| refused && put_back && cleared == vec![true, true],
    );
}

#[test]
fn scenario_a_refusal_is_about_one_item_and_does_not_outlive_the_trade() {
    scenario("a_refusal_is_about_one_item_and_does_not_outlive_the_trade");
}

// =============================================================================================
// Where in a list a drop lands.
//
// These claims need no retail data file: they are about the arithmetic between the slot the pointer
// was over and the place the move asks the shard for, and they are driven here through the
// interaction layer's own drop arm -- a `UiRequest::DragDrop` carrying a
// `DropTarget::ItemListSlot`, which is exactly what the shipped pack grid raises when a player lets
// an icon go. The same claim measured through a real pointer drag across the live grid is in the
// dat tier, beside the other drag scenarios.
//
// **A front-insert and a correct insert agree at the head of any list**, and they agree in a
// list holding one thing. So every placement claim here is measured at the head, at a middle
// slot and past the end of the same five-thing pack, and the middle one in both directions --
// because the adjustment fires only when a thing moves *down* its own list.
// =============================================================================================

use dereth_client_contract::view::DropTarget;

/// The five loose things every placement claim below is measured against, in list order.
const LOOSE: [ObjectId; 5] = [
    ObjectId(0x5000_0010),
    ObjectId(0x5000_0011),
    ObjectId(0x5000_0012),
    ObjectId(0x5000_0013),
    ObjectId(0x5000_0014),
];

/// A side pack, so that the player's list of packs has a place of its own.
const PLACE_PACK: ObjectId = ObjectId(0x5000_00FF);

/// A player carrying five loose things and one side pack.
///
/// **Nothing here is a stack**: no `max_stack_size` is set, so no merge is legal and every drop
/// falls through to the placement arithmetic, which is what these scenarios are about. The merge
/// scenario makes its own stacks, which is the other side of that fork.
fn a_player_with_five_things(c: &mut HeadlessClient) {
    a_player(c);
    let w = c.world_mut();
    for (i, id) in LOOSE.iter().enumerate() {
        put(
            w,
            *id,
            &format!("obj{:X}", id.0 & 0xFF),
            PublicWeenieDesc {
                container_id: Some(PLAYER),
                ..PublicWeenieDesc::default()
            },
        );
        w.tables
            .inventories
            .get_mut(PLAYER)
            .expect("the player's own list")
            .add_content(*id, false, i);
    }
    put(
        w,
        PLACE_PACK,
        "Sack",
        PublicWeenieDesc {
            container_id: Some(PLAYER),
            items_capacity: Some(24),
            bitfield: bitfield::OPENABLE,
            ..PublicWeenieDesc::default()
        },
    );
    w.tables.inventories.insert(
        PLACE_PACK,
        dereth_client_model::objects::ObjectInventory::new(PLACE_PACK),
    );
    w.tables
        .inventories
        .get_mut(PLAYER)
        .expect("the player's own list")
        .add_content(PLACE_PACK, true, 0);
}

/// The drop the shipped pack grid resolves when the pointer is over slot `index` of the
/// five-thing list.
///
/// These are the four facts the grid reads off its own element tree before anything that needs
/// the object table happens: the list's container, the thing under the pointer, the slot number
/// and how many things the list is showing. `tests/dat`'s
/// `a_real_drag_across_the_grid_asks_for_the_place_the_player_aimed_at` is the same four taken
/// from a real pointer drag instead of written down.
fn over_grid_slot(index: u32) -> DropTarget {
    DropTarget::ItemListSlot {
        container: PLAYER,
        under: LOOSE.get(index as usize).copied(),
        index,
        num_ui_items: 5,
        dragged_is_container: false,
        container_list: false,
    }
}

/// Let `item` go on `target`, and answer the container and the place of the move that went out,
/// if one did.
fn let_go(c: &mut HeadlessClient, item: ObjectId, target: DropTarget) -> Option<(ObjectId, u32)> {
    let mark = c.outbound().len();
    c.when(Player::ui(UiRequest::DragDrop { item, target }));
    c.outbound()[mark..].iter().find_map(|r| match r {
        Request::PutItemInContainer(m) => Some((m.container, m.slot)),
        _ => None,
    })
}

/// Whether the client is holding its one inventory request lock.
fn holding_the_lock(c: &HeadlessClient) -> bool {
    !c.view().world().request_lock.is_idle()
}

/// What the shard's answer does to the model, minus the move itself: it releases the request
/// lock and takes the provisional row off the list.
///
/// Without it the **second** drop of a scenario is refused by the client's own "already
/// attempting to place" gate, which is a different claim with a row of its own.
fn the_shard_answers(c: &mut HeadlessClient) {
    let w = c.world_mut();
    w.request_lock.clear();
    w.pending_row = None;
}

/// Every line the client has put on the scroll since it was last read.
fn said(c: &mut HeadlessClient) -> Vec<String> {
    c.world_mut()
        .scroll
        .drain()
        .into_iter()
        .map(|s| s.body)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// inventory.place.a-drop-lands-at-the-place-the-player-aimed-at
// ---------------------------------------------------------------------------------------------

/// The move asks for the place the pointer was over, and not for the head of the list.
///
/// Four aims at the same five-thing pack with the second thing in hand: the head, a middle slot
/// below it, the first empty cell, and an empty cell further out than the list has things.
pub fn a_drop_lands_at_the_place_the_player_aimed_at() {
    let mut c = HeadlessClient::model();
    a_player_with_five_things(&mut c);
    let dragged = LOOSE[1];

    // The head. A front-insert and a correct insert agree here, which is exactly why this cannot
    // be the only place the claim is measured.
    let head = let_go(&mut c, dragged, over_grid_slot(0));
    the_shard_answers(&mut c);

    // A middle slot, moving **down** the list: taking the thing out of place 1 shifts everything
    // after it down one, so the place asked for is one less than the slot aimed at.
    let middle = let_go(&mut c, dragged, over_grid_slot(3));
    the_shard_answers(&mut c);

    // The first empty cell. The list is showing five things, so this is the tail.
    let tail = let_go(&mut c, dragged, over_grid_slot(5));
    the_shard_answers(&mut c);

    // An empty cell further out is the same answer: the slot is clamped to what the list holds,
    // or a player who let go over the bottom of a half-empty grid would be asking for a place
    // past the end of the container.
    let far = let_go(&mut c, dragged, over_grid_slot(9));

    c.assert_behaviour(
        "inventory.place.a-drop-lands-at-the-place-the-player-aimed-at",
        move |_| {
            head == Some((PLAYER, 0))
                && middle == Some((PLAYER, 2))
                && tail == Some((PLAYER, 4))
                && far == Some((PLAYER, 4))
        },
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.place.moving-a-thing-up-its-own-list-does-not-shift-it-one-further
// ---------------------------------------------------------------------------------------------

/// The other direction, which is the half the shift must **not** touch.
///
/// The same three aims with the fourth thing in hand -- something at place 3 moving *up* -- so
/// the head and a middle slot are both above it and neither may be adjusted. Asserting the
/// downward direction alone would let an unconditional shift pass: the head would become the
/// place before the head, and the second cell the first.
pub fn moving_a_thing_up_its_own_list_does_not_shift_it_one_further() {
    let mut c = HeadlessClient::model();
    a_player_with_five_things(&mut c);
    let dragged = LOOSE[3];

    let head = let_go(&mut c, dragged, over_grid_slot(0));
    the_shard_answers(&mut c);
    let up_one = let_go(&mut c, dragged, over_grid_slot(1));
    the_shard_answers(&mut c);
    // ...and going down from 3 to the end still is adjusted, in the same run, so the two halves
    // cannot be told apart by anything but the direction.
    let down_to_the_end = let_go(&mut c, dragged, over_grid_slot(5));

    c.assert_behaviour(
        "inventory.place.moving-a-thing-up-its-own-list-does-not-shift-it-one-further",
        move |_| {
            head == Some((PLAYER, 0))
                && up_one == Some((PLAYER, 1))
                && down_to_the_end == Some((PLAYER, 4))
        },
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.place.a-drop-that-would-change-nothing-is-refused-and-takes-no-lock
// ---------------------------------------------------------------------------------------------

/// Two drops that would move nothing are refused outright, and a refusal asks the shard for
/// nothing -- so nothing can be waiting for an answer and the one inventory lock stays free.
///
/// The second of the two is the subtle one: the cell immediately after a thing's own place is
/// where that thing already is once it has been taken out of the list, so with the whole stack
/// in hand it is the same no-op as dropping it on itself. **With only part of a stack in hand it
/// is a real move**, and that pair is what makes the guard a guard rather than a blanket refusal.
pub fn a_drop_that_would_change_nothing_is_refused_and_takes_no_lock() {
    let mut c = HeadlessClient::model();
    a_player_with_five_things(&mut c);
    let dragged = LOOSE[2];

    let on_itself = let_go(&mut c, dragged, over_grid_slot(2));
    let no_lock_after_that = !holding_the_lock(&c);
    let on_the_next_cell = let_go(&mut c, dragged, over_grid_slot(3));
    let still_no_lock = !holding_the_lock(&c);

    // The same aim with part of a stack in hand. The slider is the player's own: the shipped
    // split dialog raises it as the stack is picked up, and it is what tells the drop how much
    // of the stack is being carried.
    let mut split = HeadlessClient::model();
    a_player_with_five_things(&mut split);
    {
        let w = split.world_mut();
        let wn = w.tables.weenies.get_mut(dragged).expect("seeded");
        wn.pwd.wcid = 273;
        wn.pwd.stack_size = Some(5);
        wn.pwd.max_stack_size = Some(25);
    }
    split.when(Player::ui(UiRequest::StackSliderChanged {
        split: 1,
        max: 5,
    }));
    let mark = split.outbound().len();
    split.when(Player::ui(UiRequest::DragDrop {
        item: dragged,
        target: over_grid_slot(3),
    }));
    let a_part_stack_is_a_real_move = split.outbound().len() > mark;
    split.shutdown();

    c.assert_behaviour(
        "inventory.place.a-drop-that-would-change-nothing-is-refused-and-takes-no-lock",
        move |_| {
            on_itself.is_none()
                && no_lock_after_that
                && on_the_next_cell.is_none()
                && still_no_lock
                && a_part_stack_is_a_real_move
        },
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.place.a-plain-thing-under-the-pointer-is-a-place-and-a-pack-is-a-destination
// ---------------------------------------------------------------------------------------------

/// Letting something go on another **thing** moves it to that thing's place in the list they are
/// both in; letting it go on a **pack** puts it inside the pack, at the pack's own head.
///
/// The first half is the one a player notices when it is wrong: a move aimed at a plain thing is
/// a request no shard can answer, and the inventory lock has no timeout, so the icon sits greyed
/// until the player does something else with their pack.
pub fn a_plain_thing_under_the_pointer_is_a_place_and_a_pack_is_a_destination() {
    let mut c = HeadlessClient::model();
    a_player_with_five_things(&mut c);

    let onto_a_thing = let_go(&mut c, LOOSE[0], over_grid_slot(3));
    the_shard_answers(&mut c);

    // The same grid, the same slot number, with the side pack drawn in that cell instead.
    let onto_a_pack = let_go(
        &mut c,
        LOOSE[0],
        DropTarget::ItemListSlot {
            container: PLAYER,
            under: Some(PLACE_PACK),
            index: 3,
            num_ui_items: 5,
            dragged_is_container: false,
            container_list: false,
        },
    );

    c.assert_behaviour(
        "inventory.place.a-plain-thing-under-the-pointer-is-a-place-and-a-pack-is-a-destination",
        move |_| {
            onto_a_thing.map(|(container, _)| container) == Some(PLAYER)
                && onto_a_pack == Some((PLACE_PACK, 0))
        },
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.place.a-dragged-pack-is-looked-up-among-the-packs
// ---------------------------------------------------------------------------------------------

/// A player's packs and a player's loose things are two lists, and which one a drag is looked up
/// in comes off the **thing being carried**, not off the list it is let go on.
///
/// Asking the wrong list for a dragged pack finds whatever happens to be at that place among the
/// loose things, and the two answers differ silently: here the second pack sits at place 1 among
/// the packs and at no place at all among the things.
pub fn a_dragged_pack_is_looked_up_among_the_packs() {
    const POUCH: ObjectId = ObjectId(0x5000_00FE);

    let mut c = HeadlessClient::model();
    a_player_with_five_things(&mut c);
    {
        let w = c.world_mut();
        put(
            w,
            POUCH,
            "Pouch",
            PublicWeenieDesc {
                container_id: Some(PLAYER),
                items_capacity: Some(24),
                bitfield: bitfield::OPENABLE,
                ..PublicWeenieDesc::default()
            },
        );
        w.tables.inventories.insert(
            POUCH,
            dereth_client_model::objects::ObjectInventory::new(POUCH),
        );
        w.tables
            .inventories
            .get_mut(PLAYER)
            .expect("the player's own list")
            .add_content(POUCH, true, 1);
    }

    // The strip of packs: two cells, the sack at 0 and the pouch at 1. The pouch is carried onto
    // the sack's cell.
    let on_the_strip = |under: ObjectId| DropTarget::ItemListSlot {
        container: PLAYER,
        under: Some(under),
        index: 0,
        num_ui_items: 2,
        dragged_is_container: true,
        container_list: true,
    };
    let moved = let_go(&mut c, POUCH, on_the_strip(PLACE_PACK));
    the_shard_answers(&mut c);

    // And the same drop with the sack itself in hand is the refused no-op -- which is the proof
    // that the lookup is against the packs: the sack really is at place 0 among them.
    let already_there = let_go(&mut c, PLACE_PACK, on_the_strip(PLACE_PACK));

    c.assert_behaviour(
        "inventory.place.a-dragged-pack-is-looked-up-among-the-packs",
        move |_| moved == Some((PLAYER, 0)) && already_there.is_none(),
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.place.a-merge-is-answered-before-any-place-is-worked-out
// ---------------------------------------------------------------------------------------------

/// Letting one stack go on another of the same kind is a merge, and the merge is answered before
/// any place is worked out at all: nothing is repositioned as well.
pub fn a_merge_is_answered_before_any_place_is_worked_out() {
    let mut c = HeadlessClient::model();
    a_player_with_five_things(&mut c);
    {
        let w = c.world_mut();
        for (id, n) in [(LOOSE[1], 5u16), (LOOSE[3], 7)] {
            let wn = w.tables.weenies.get_mut(id).expect("seeded");
            wn.pwd.wcid = 273;
            wn.pwd.stack_size = Some(n);
            wn.pwd.max_stack_size = Some(25);
        }
    }
    // The whole source stack is in hand, which is what picking a stack up without touching the
    // split dialog leaves.
    c.when(Player::ui(UiRequest::StackSliderChanged {
        split: 5,
        max: 5,
    }));

    let mark = c.outbound().len();
    c.when(Player::ui(UiRequest::DragDrop {
        item: LOOSE[1],
        target: over_grid_slot(3),
    }));
    let sent: Vec<Request> = c.outbound()[mark..].to_vec();
    let merged = sent.iter().any(|r| {
        matches!(r, Request::StackableMerge(m)
            if m.merge_from == LOOSE[1] && m.merge_to == LOOSE[3] && m.amount == 5)
    });
    let nothing_repositioned = !sent
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(_)));

    c.assert_behaviour(
        "inventory.place.a-merge-is-answered-before-any-place-is-worked-out",
        move |_| merged && nothing_repositioned,
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.place.a-container-with-no-room-refuses-in-the-clients-own-words
// ---------------------------------------------------------------------------------------------

/// A thing that says it is a container but has no room for anything refuses the drop in the
/// client's own sentence, naming itself, and asks the shard for nothing.
///
/// Without that arm the drop quietly becomes a reposition in the list the container is sitting
/// in, which looks to the player like the thing bouncing back for no reason.
pub fn a_container_with_no_room_refuses_in_the_clients_own_words() {
    let mut c = HeadlessClient::model();
    a_player_with_five_things(&mut c);
    let full = LOOSE[4];
    {
        let w = c.world_mut();
        let wn = w.tables.weenies.get_mut(full).expect("seeded");
        wn.pwd.name = "Chest".into();
        wn.pwd.bitfield |=
            dereth_client_model::inventory::use_object::use_bitfield::REQUIRES_PACK_SLOT;
        wn.pwd.items_capacity = Some(0);
        wn.pwd.containers_capacity = Some(0);
    }
    let _ = said(&mut c);

    let sent = let_go(&mut c, LOOSE[0], over_grid_slot(4));
    let lines = said(&mut c);
    let refused_in_words = lines.iter().any(|l| l == "The Chest cannot accept items");

    c.assert_behaviour(
        "inventory.place.a-container-with-no-room-refuses-in-the-clients-own-words",
        move |_| sent.is_none() && refused_in_words,
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.place.the-lock-and-the-grey-are-both-released-by-the-shards-own-answer
// ---------------------------------------------------------------------------------------------

/// A legal reposition takes the one inventory lock and greys the icon, and the shard's answer
/// releases both with nothing else asked of the player.
///
/// The lock has no timeout -- which is correct -- so the answer is the only thing that can
/// release it, and a move aimed somewhere no shard can answer holds it for the rest of the
/// session.
pub fn the_lock_and_the_grey_are_both_released_by_the_shards_own_answer() {
    let mut c = HeadlessClient::model();
    a_player_with_five_things(&mut c);
    let dragged = LOOSE[1];

    let sent = let_go(&mut c, dragged, over_grid_slot(3));
    let taken = holding_the_lock(&c);
    let greyed = c.view().world().weenie(dragged).expect("seeded").waiting;

    // The shard's own answer for a reposition inside one container.
    c.when(Inbound::message(
        &dereth_protocol::objects::ItemServerSaysContainId {
            item: dragged,
            container: PLAYER,
            slot: 2,
            container_properties: 0,
        },
    ));

    let released = !holding_the_lock(&c);
    let ungreyed = !c.view().world().weenie(dragged).expect("seeded").waiting;
    let where_the_player_put_it = c
        .view()
        .world()
        .inventory(PLAYER)
        .expect("the player's own list")
        .place_in_list(dragged, false);

    c.assert_behaviour(
        "inventory.place.the-lock-and-the-grey-are-both-released-by-the-shards-own-answer",
        move |_| {
            sent == Some((PLAYER, 2))
                && taken
                && greyed
                && released
                && ungreyed
                && where_the_player_put_it == Some(2)
        },
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.place.a-refused-drop-takes-the-grey-back-off-the-icon
// ---------------------------------------------------------------------------------------------

/// A drop the client refuses for itself un-greys the icon it greyed when it was picked up.
///
/// The grid greys the source cell the moment the icon leaves it. A refusal sends nothing, so
/// there is nothing on the wire that could ever clear that grey -- the inventory wait has no
/// timeout and the panel's own refresh does nothing on an unchanged list -- and it would survive
/// until something else moved in the pack.
pub fn a_refused_drop_takes_the_grey_back_off_the_icon() {
    let mut c = HeadlessClient::model();
    a_player_with_five_things(&mut c);
    let dragged = LOOSE[2];

    // What the grid does before the drop is routed anywhere: the icon leaves the cell greyed.
    c.world_mut().set_waiting_state(dragged, true);
    let greyed = c.view().world().weenie(dragged).expect("seeded").waiting;

    let mark = c.outbound().len();
    c.when(Player::ui(UiRequest::DragDrop {
        item: dragged,
        target: over_grid_slot(2),
    }));
    let nothing_sent = c.outbound().len() == mark;
    let back = !c.view().world().weenie(dragged).expect("seeded").waiting;
    let no_lock = !holding_the_lock(&c);

    c.assert_behaviour(
        "inventory.place.a-refused-drop-takes-the-grey-back-off-the-icon",
        move |_| greyed && nothing_sent && back && no_lock,
    );
}

#[test]
fn scenario_a_drop_lands_at_the_place_the_player_aimed_at() {
    scenario("a_drop_lands_at_the_place_the_player_aimed_at");
}

#[test]
fn scenario_moving_a_thing_up_its_own_list_does_not_shift_it_one_further() {
    scenario("moving_a_thing_up_its_own_list_does_not_shift_it_one_further");
}

#[test]
fn scenario_a_drop_that_would_change_nothing_is_refused_and_takes_no_lock() {
    scenario("a_drop_that_would_change_nothing_is_refused_and_takes_no_lock");
}

#[test]
fn scenario_a_plain_thing_under_the_pointer_is_a_place_and_a_pack_is_a_destination() {
    scenario("a_plain_thing_under_the_pointer_is_a_place_and_a_pack_is_a_destination");
}

#[test]
fn scenario_a_dragged_pack_is_looked_up_among_the_packs() {
    scenario("a_dragged_pack_is_looked_up_among_the_packs");
}

#[test]
fn scenario_a_merge_is_answered_before_any_place_is_worked_out() {
    scenario("a_merge_is_answered_before_any_place_is_worked_out");
}

#[test]
fn scenario_a_container_with_no_room_refuses_in_the_clients_own_words() {
    scenario("a_container_with_no_room_refuses_in_the_clients_own_words");
}

#[test]
fn scenario_the_lock_and_the_grey_are_both_released_by_the_shards_own_answer() {
    scenario("the_lock_and_the_grey_are_both_released_by_the_shards_own_answer");
}

#[test]
fn scenario_a_refused_drop_takes_the_grey_back_off_the_icon() {
    scenario("a_refused_drop_takes_the_grey_back_off_the_icon");
}

// =============================================================================================
// Which hint a list slot shows for what is being carried over it.
//
// These three claims are about one list widget answering one hover and need no retail data file;
// the drag-hint claims that need the live element tree are in the dat tier.
//
// **Why this is a table and not a picture.** The hint is one small overlay on one slot under a
// moving pointer. A wrong one is invisible in a frame and *actively misinforms*: saying "this
// will be taken" over a slot whose drop will be refused tells the player the move will work.
// Every yes below is paired with the no of the same shape, because the whole value of the hint
// is that the two are different.
// =============================================================================================

use dereth_ui::{StateId, UiSystem};
use dereth_ui_screens::items::widget::{
    child, drag_accept_state, drag_flags, DropIconInfo, ItemListWidget, ItemSlot,
};

/// A bare list with one slot per entry of `items`, built on a real (headless) element system so
/// that the state writes go through the widget's own cache rather than round it.
fn a_bare_list(
    ui: &mut UiSystem,
    container_list: bool,
    items: &[Option<ObjectId>],
) -> ItemListWidget {
    let root = ui.create_hollow(None);
    let mut w = ItemListWidget {
        element: dereth_ui::ElementId(0x1000_0031),
        handle: root,
        slot_id: dereth_ui::ElementId(0x1000_033A),
        container_list,
        shortcut_list: false,
        vendor_list: false,
        salvage_list: false,
        allow_dragging: true,
        horizontal: false,
        at_least_one_empty: false,
        single_selection: false,
        open_item_id: None,
        max_columns: 1,
        fixed_list_size: i32::try_from(items.len()).unwrap_or(0),
        cell: (32, 32),
        slots: Vec::new(),
        cached_slots: std::collections::VecDeque::new(),
        parent_container: None,
        created: 0,
        create_failures: 0,
        drag_icon_failures: 0,
    };
    for it in items {
        let h = ui.create_hollow(Some(root));
        let overlay = ui.create_hollow(Some(h));
        let mut s = ItemSlot::bare(h);
        s.drag_accept = Some(overlay);
        s.item = *it;
        s.is_container = false;
        w.slots.push(s);
    }
    w
}

/// What the icon in the air says about itself.
fn carrying(is_container: bool) -> DropIconInfo {
    DropIconInfo {
        item: Some(ObjectId(0x5000_0010)),
        spell: None,
        flags: if is_container {
            drag_flags::IS_CONTAINER
        } else {
            0
        },
    }
}

/// The four hint states and the four kinds of drag, as numbers.
///
/// This is the one place they are written down. Every other assertion in this subject reads them
/// through the symbols, and a claim that reads a constant through the symbol that writes it
/// cannot detect a wrong constant -- so the numbers are asserted here as the premise of the
/// table below rather than left to a guard of their own.
fn the_shipped_numbers_are_what_they_are() {
    assert_eq!(drag_accept_state::NONE, StateId(0x1000_003F), "no hint");
    assert_eq!(
        drag_accept_state::ACCEPT,
        StateId(0x1000_0040),
        "this slot will take the drop"
    );
    assert_eq!(
        drag_accept_state::REFUSE,
        StateId(0x1000_0041),
        "this slot will refuse it"
    );
    assert_eq!(
        drag_accept_state::INTO_CONTAINER,
        StateId(0x1000_0046),
        "into this container"
    );
    assert_eq!(
        child::DRAG_ACCEPT,
        0x1000_045A,
        "the overlay the hint is drawn on"
    );
    assert_eq!(drag_flags::IS_CONTAINER, 1);
    assert_eq!(drag_flags::IS_VENDOR, 2);
    assert_eq!(drag_flags::IS_SHORTCUT, 4);
    assert_eq!(drag_flags::IS_SALVAGE, 8);
    assert_eq!(
        drag_flags::NOT_AN_INVENTORY_MOVE,
        0xE,
        "the three that are not an inventory move"
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.which-hint-a-slot-shows-for-what-is-carried-over-it
// ---------------------------------------------------------------------------------------------

/// Seven hovers over the same two lists, each paired with its opposite.
///
/// | list | carried | cell under the pointer | the pack's room | hint |
/// |---|---|---|---|---|
/// | things | a plain thing | anything | -- | it will be taken |
/// | things | a pack | anything | -- | it will be refused |
/// | packs | a pack | anything | -- | it will be taken |
/// | packs | a plain thing | an **empty** cell | -- | it will be refused |
/// | packs | a plain thing | a pack | none | it will be refused |
/// | packs | a plain thing | a pack | some | into this container |
/// | packs | a plain thing | a pack | endless | into this container |
///
/// The endless pack has a row of its own because "how much room is left" answers a negative
/// number for a container with no limit, and reading that as "no room" refuses every drop into a
/// player's own main pack -- which is the commonest drop there is.
pub fn which_hint_a_slot_shows_for_what_is_carried_over_it() {
    the_shipped_numbers_are_what_they_are();

    let mut ui = UiSystem::new((800, 600));
    let pack = ObjectId(0x5000_00FF);
    let plain = ObjectId(0x5000_0020);
    let unknown_room = |_: ObjectId| -> Option<i64> { None };

    // --- the list of loose things --------------------------------------------------------------
    let mut things = a_bare_list(&mut ui, false, &[Some(plain), None]);
    let a_thing_over_a_filled_cell = things.drag_over(&mut ui, 0, carrying(false), &unknown_room);
    let a_thing_over_an_empty_cell = things.drag_over(&mut ui, 1, carrying(false), &unknown_room);
    let a_pack_over_the_things = things.drag_over(&mut ui, 0, carrying(true), &unknown_room);

    // --- the strip of packs --------------------------------------------------------------------
    let mut packs = a_bare_list(&mut ui, true, &[Some(pack), None]);
    let a_pack_over_the_packs = packs.drag_over(&mut ui, 0, carrying(true), &unknown_room);
    let a_thing_over_an_empty_pack_cell =
        packs.drag_over(&mut ui, 1, carrying(false), &unknown_room);

    let no_room = |id: ObjectId| -> Option<i64> { (id == pack).then_some(0) };
    let over_a_full_pack = packs.drag_over(&mut ui, 0, carrying(false), &no_room);
    let some_room = |id: ObjectId| -> Option<i64> { (id == pack).then_some(3) };
    let over_a_pack_with_room = packs.drag_over(&mut ui, 0, carrying(false), &some_room);

    // The widget skips a set that would change nothing, so the cache is moved off the answer
    // being measured first -- otherwise the assertion would be about the previous hover.
    packs.drag_over(&mut ui, 0, carrying(false), &no_room);
    let endless = |id: ObjectId| -> Option<i64> { (id == pack).then_some(-1) };
    let over_an_endless_pack = packs.drag_over(&mut ui, 0, carrying(false), &endless);

    packs.drag_over(&mut ui, 0, carrying(false), &some_room);
    let over_something_the_client_does_not_know =
        packs.drag_over(&mut ui, 0, carrying(false), &unknown_room);

    let accept = Some(drag_accept_state::ACCEPT);
    let refuse = Some(drag_accept_state::REFUSE);
    let into = Some(drag_accept_state::INTO_CONTAINER);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "inventory.drag-hint.which-hint-a-slot-shows-for-what-is-carried-over-it",
        move |_| {
            a_thing_over_a_filled_cell == accept
                && a_thing_over_an_empty_cell == accept
                && a_pack_over_the_things == refuse
                && a_pack_over_the_packs == accept
                && a_thing_over_an_empty_pack_cell == refuse
                && over_a_full_pack == refuse
                && over_a_pack_with_room == into
                && over_an_endless_pack == into
                && over_something_the_client_does_not_know == refuse
        },
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.the-lists-that-take-no-drop-show-no-hint-rather-than-a-refusal
// ---------------------------------------------------------------------------------------------

/// A shop's stock, a salvage list and the shortcut bar answer a hover with **nothing at all**,
/// and so does a drag that is not an inventory move -- the difference between "no hint" and "the
/// refusal hint", which is what a player sees painted on every cell the pointer crosses.
///
/// The pair that makes it a claim: a drag carrying a **pack** is not one of the three, and it
/// really does get the refusal.
pub fn the_lists_that_take_no_drop_show_no_hint_rather_than_a_refusal() {
    let mut ui = UiSystem::new((800, 600));
    let unknown_room = |_: ObjectId| -> Option<i64> { None };
    let plain = ObjectId(0x5000_0020);

    let mut lists_that_answer_nothing = Vec::new();
    for which in 0..3usize {
        let mut w = a_bare_list(&mut ui, false, &[Some(plain)]);
        match which {
            0 => w.vendor_list = true,
            1 => w.salvage_list = true,
            _ => w.shortcut_list = true,
        }
        let answer = w.drag_over(&mut ui, 0, carrying(false), &unknown_room);
        lists_that_answer_nothing.push((answer, w.slots[0].drag_accept_state));
    }

    // An icon in the air carrying no thing at all.
    let mut w = a_bare_list(&mut ui, false, &[Some(plain)]);
    let nothing_carried = DropIconInfo {
        item: None,
        spell: None,
        flags: 0,
    };
    let empty_hand = (
        w.drag_over(&mut ui, 0, nothing_carried, &unknown_room),
        w.slots[0].drag_accept_state,
    );

    // The three kinds of drag that are not an inventory move.
    let mut not_a_move = Vec::new();
    for flag in [
        drag_flags::IS_VENDOR,
        drag_flags::IS_SHORTCUT,
        drag_flags::IS_SALVAGE,
    ] {
        let mut w = a_bare_list(&mut ui, false, &[Some(plain)]);
        let info = DropIconInfo {
            item: Some(plain),
            spell: None,
            flags: flag,
        };
        not_a_move.push((
            w.drag_over(&mut ui, 0, info, &unknown_room),
            w.slots[0].drag_accept_state,
        ));
    }

    // ...and carrying a pack is **not** one of them, which is the pair that makes the line above
    // mean something: it gets a real answer.
    let mut w = a_bare_list(&mut ui, false, &[Some(plain)]);
    let info = DropIconInfo {
        item: Some(plain),
        spell: None,
        flags: drag_flags::IS_CONTAINER,
    };
    let a_pack_is_still_answered = w.drag_over(&mut ui, 0, info, &unknown_room);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "inventory.drag-hint.the-lists-that-take-no-drop-show-no-hint-rather-than-a-refusal",
        move |_| {
            lists_that_answer_nothing.iter().all(|a| *a == (None, None))
                && empty_hand == (None, None)
                && not_a_move.iter().all(|a| *a == (None, None))
                && a_pack_is_still_answered == Some(drag_accept_state::REFUSE)
        },
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.drag-hint.setting-the-same-hint-twice-raises-nothing
// ---------------------------------------------------------------------------------------------

/// Putting the hint a slot already has on it again changes nothing, and a slot with no overlay
/// to draw it on does nothing at all.
///
/// Both are observable: without the first, a pointer sitting still over one cell would restate
/// the overlay's state every frame, which is a redraw and a media step per frame for as long as
/// the player holds the icon there.
pub fn setting_the_same_hint_twice_raises_nothing() {
    let mut ui = UiSystem::new((800, 600));
    let root = ui.create_hollow(None);
    let overlay = ui.create_hollow(Some(root));
    let mut slot = ItemSlot::bare(root);
    slot.drag_accept = Some(overlay);

    let first = slot.set_drag_accept_state(&mut ui, drag_accept_state::ACCEPT);
    let second = slot.set_drag_accept_state(&mut ui, drag_accept_state::ACCEPT);
    let different = slot.set_drag_accept_state(&mut ui, drag_accept_state::REFUSE);

    let mut bare = ItemSlot::bare(root);
    let with_no_overlay = bare.set_drag_accept_state(&mut ui, drag_accept_state::ACCEPT);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "inventory.drag-hint.setting-the-same-hint-twice-raises-nothing",
        move |_| first && !second && different && !with_no_overlay,
    );
}

#[test]
fn scenario_which_hint_a_slot_shows_for_what_is_carried_over_it() {
    scenario("which_hint_a_slot_shows_for_what_is_carried_over_it");
}

#[test]
fn scenario_the_lists_that_take_no_drop_show_no_hint_rather_than_a_refusal() {
    scenario("the_lists_that_take_no_drop_show_no_hint_rather_than_a_refusal");
}

#[test]
fn scenario_setting_the_same_hint_twice_raises_nothing() {
    scenario("setting_the_same_hint_twice_raises_nothing");
}

// =============================================================================================
// Every place a recorded character's clothes are worn in, asked for again.
//
// The recording says its character wears thing *X* in place *L*, nine times over; this asks the
// client to put each of them on by letting it go on the place on the figure that covers *L*, and
// reads what it asks the shard for. Nothing here opens a retail data file: the recording is
// committed, the places on the figure are the shipped map the client already carries, and the drop
// goes in at the request the figure raises.
//
// **The shard's answer closes each one**, in the same loop, because the hold an inventory
// request takes has no timeout: a client that asked and was never answered would sit on it for
// the rest of the session, and a scenario that never answered would be measuring the first drop
// nine times.
// =============================================================================================

/// The player of the recording these placements belong to.
const DRESSED_PLAYER: ObjectId = ObjectId(0x5000_0003);

/// What the recording says its character was wearing, and where.
fn what_the_recording_says_he_wore() -> Vec<(ObjectId, u32)> {
    let mut out: Vec<(ObjectId, u32)> = Vec::new();
    dereth_testkit::adapters_inventory::first_blob_where("short-second-connection", |e| {
        if let dereth_client_net::client_session::SessionEvent::PlayerDescription(d) = e {
            out = d
                .inventory_placements
                .iter()
                .map(|p| (p.iid, p.location))
                .collect();
            true
        } else {
            false
        }
    })
    .expect("the recording carries the player's description");
    out
}

// ---------------------------------------------------------------------------------------------
// inventory.body.every-recorded-placement-can-be-asked-for-and-the-shards-answer-closes-it
// ---------------------------------------------------------------------------------------------

/// For every place the recording's character wears something in: letting that thing go on the
/// place on the figure that covers it asks the shard to put it on, naming a place that is one of
/// the thing's own and one that place on screen stands for -- never a move into a container --
/// and the hold and the grey are taken by the ask and released by the shard's answer, which also
/// records the placement the figure is drawn from.
///
/// Several of the recorded places are combinations that no single place on the figure equals,
/// which is why what is asked for is required to be a **part** of the thing's own list rather
/// than the whole of it.
pub fn every_recorded_placement_can_be_asked_for_and_the_shards_answer_closes_it() {
    use dereth_client_contract::view::DropTarget;
    use dereth_ui_screens::panels::inventory::{InventoryPanels, PAPER_DOLL_SLOTS};

    let worn = what_the_recording_says_he_wore();
    let the_recording_dresses_him = worn.len() >= 9;

    let mut c = HeadlessClient::model();
    {
        let w = c.world_mut();
        w.player = Some(DRESSED_PLAYER);
        w.tables.inventories.insert(
            DRESSED_PLAYER,
            dereth_client_model::objects::ObjectInventory::new(DRESSED_PLAYER),
        );
        put(
            w,
            DRESSED_PLAYER,
            "the recorded character",
            PublicWeenieDesc {
                bitfield: bitfield::PLAYER,
                obj_type: item_type::CREATURE,
                items_capacity: Some(102),
                containers_capacity: Some(7),
                ..PublicWeenieDesc::default()
            },
        );
        for (item, places) in &worn {
            put(
                w,
                *item,
                "a recorded garment",
                PublicWeenieDesc {
                    valid_locations: Some(*places),
                    ..PublicWeenieDesc::default()
                },
            );
        }
    }

    let mut asked = 0usize;
    let mut every_one_holds = true;
    for (item, places) in &worn {
        // The place on the figure a drop lands on: the one whose own mask the recorded place
        // picks out.
        let Some(element) = PAPER_DOLL_SLOTS
            .iter()
            .find(|(_, mask)| mask & places != 0)
            .map(|(id, _)| dereth_ui::ElementId(*id))
        else {
            every_one_holds = false;
            continue;
        };
        let mask = InventoryPanels::location_of_slot(element).expect("a place on the figure");

        let mark = c.outbound().len();
        c.when(Player::ui(UiRequest::DragDrop {
            item: *item,
            target: DropTarget::EquipSlot(element),
        }));
        let sent: Vec<Request> = c.outbound()[mark..].to_vec();

        let wears: Vec<u32> = sent
            .iter()
            .filter_map(|r| match r {
                Request::GetAndWieldItem(m) if m.item == *item => Some(m.slot),
                _ => None,
            })
            .collect();
        every_one_holds &= wears.len() == 1;
        every_one_holds &= wears.first().is_some_and(|l| *l != 0 && l & !places == 0);
        every_one_holds &= wears.first().is_some_and(|l| l & mask != 0);
        every_one_holds &= !sent
            .iter()
            .any(|r| matches!(r, Request::PutItemInContainer(_)));

        // The hold and the grey, taken by the ask.
        every_one_holds &= c.view().world().request_lock.object == Some(*item);
        every_one_holds &= c.view().world().request_lock.pending
            == dereth_client_model::inventory::requests::InventoryRequest::Wield;
        every_one_holds &= c.view().world().weenie(*item).expect("seeded").waiting;

        // The shard's answer, with the pair this drop asked for.
        c.when(Inbound::message(&dereth_protocol::objects::ItemWearItem {
            item: *item,
            slot: *places,
        }));
        let w = c.view().world();
        every_one_holds &= !w.weenie(*item).expect("seeded").waiting;
        every_one_holds &= w.weenie(*item).expect("seeded").pwd.wielder_id == Some(DRESSED_PLAYER);
        every_one_holds &= w.request_lock.is_idle();
        every_one_holds &= w
            .inventory(DRESSED_PLAYER)
            .expect("his own list")
            .placements
            .iter()
            .any(|p| p.iid == *item && p.loc == *places);
        asked += 1;
    }
    let all_nine = asked == worn.len();

    c.assert_behaviour(
        "inventory.body.every-recorded-placement-can-be-asked-for-and-the-shards-answer-closes-it",
        move |_| the_recording_dresses_him && all_nine && every_one_holds,
    );
}

#[test]
fn scenario_every_recorded_placement_can_be_asked_for_and_the_shards_answer_closes_it() {
    scenario("every_recorded_placement_can_be_asked_for_and_the_shards_answer_closes_it");
}

// =============================================================================================
// The figure's own picture, as a place to let something go.
//
// A drop on the picture of the character is not silently nothing. The picture is the one thing on
// the figure that is **not** one of its places, which is exactly why a drop on it is answered by a
// different arm: what goes out is the thing's whole list of places, with the shard choosing, rather
// than the one place a walk would have picked.
//
// Which things can be worn and which cannot is the **recordings'**: every equip a recorded
// client asked for is joined to the list of places the shard described that thing with, so no
// mask under test is written here.
// =============================================================================================

use dereth_client_model::inventory::equip::WIELD_SLOT_ORDER;
use dereth_client_model::inventory::slots::{
    loc, location_info_from_element_id, PAPERDOLL_REGIONS,
};
use dereth_testkit::adapters_inventory::{all_recorded_equips, RecordedEquip};
use dereth_ui_screens::panels::inventory::{
    InventoryPanels, CANNOT_PUT_THAT_ITEM_THERE, PAPER_DOLL_DRAG_MASK, PAPER_DOLL_SLOTS,
};

/// The figure's viewport, its own answer overlay and the checkbox that asks for the grid of
/// places: three more things the window binds, and none of them a place to let something go.
const FIGURE_VIEWPORT: u32 = 0x1000_01D5;
const FIGURE_ANSWER_OVERLAY: u32 = 0x1000_046D;
const PLACES_CHECKBOX: u32 = 0x1000_05BE;

/// The player these drops are made by.
const FIGURE_PLAYER: ObjectId = ObjectId(0x5000_0001);

/// A client holding one thing, greyed as the grid greys an icon the moment it leaves its cell.
///
/// The grey matters: a scenario that asserts a refused drop puts the icon back is measuring
/// nothing unless the icon was greyed first.
fn a_client_holding(item: ObjectId, places: u32) -> HeadlessClient {
    let mut c = HeadlessClient::model();
    {
        let w = c.world_mut();
        w.player = Some(FIGURE_PLAYER);
        w.tables.inventories.insert(
            FIGURE_PLAYER,
            dereth_client_model::objects::ObjectInventory::new(FIGURE_PLAYER),
        );
        put(
            w,
            FIGURE_PLAYER,
            "the player",
            PublicWeenieDesc {
                bitfield: bitfield::PLAYER,
                obj_type: item_type::CREATURE,
                items_capacity: Some(102),
                containers_capacity: Some(7),
                ..PublicWeenieDesc::default()
            },
        );
        put(
            w,
            item,
            "a thing being carried",
            PublicWeenieDesc {
                valid_locations: Some(places),
                ..PublicWeenieDesc::default()
            },
        );
        w.set_waiting_state(item, true);
    }
    c
}

/// Put `item` back where a fresh drop starts: greyed, with nothing held and nothing said.
fn ready_for_another_drop(c: &mut HeadlessClient, item: ObjectId, places: u32) {
    {
        let w = c.world_mut();
        w.request_lock = dereth_client_model::inventory::requests::RequestLock::default();
        let wn = w.tables.weenies.get_mut(item);
        if wn.is_none() {
            put(
                w,
                item,
                "a thing being carried",
                PublicWeenieDesc {
                    valid_locations: Some(places),
                    ..PublicWeenieDesc::default()
                },
            );
        } else {
            let wn = w.tables.weenies.get_mut(item).expect("checked");
            wn.pwd.valid_locations = Some(places);
        }
        w.set_waiting_state(item, true);
    }
    c.interaction_mut().last_refusal = None;
}

/// Let `item` go on the figure's own picture, and answer everything that went out.
fn let_go_on_the_picture(c: &mut HeadlessClient, item: ObjectId) -> Vec<Request> {
    use dereth_client_contract::view::DropTarget;

    let mark = c.outbound().len();
    c.when(Player::ui(UiRequest::DragDrop {
        item,
        target: DropTarget::EquipSlot(PAPER_DOLL_DRAG_MASK),
    }));
    c.outbound()[mark..].to_vec()
}

/// Let `item` go on one of the figure's **places**, named by its own element.
fn let_go_on_a_place(c: &mut HeadlessClient, item: ObjectId, element: u32) -> Vec<Request> {
    use dereth_client_contract::view::DropTarget;

    let mark = c.outbound().len();
    c.when(Player::ui(UiRequest::DragDrop {
        item,
        target: DropTarget::EquipSlot(dereth_ui::ElementId(element)),
    }));
    c.outbound()[mark..].to_vec()
}

/// What the client asked to put on, out of a batch of requests.
fn wears(sent: &[Request], item: ObjectId) -> Vec<u32> {
    sent.iter()
        .filter_map(|r| match r {
            Request::GetAndWieldItem(m) if m.item == item => Some(m.slot),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// inventory.body.the-figures-own-picture-is-not-one-of-the-places-on-it
// ---------------------------------------------------------------------------------------------

/// The picture of the character is not one of the twenty-four places on the body, and neither is
/// anything else the window binds beside it.
///
/// That is the whole reason a drop on the picture is answered by a different arm at all: if the
/// picture ever became a place, a drop on it would take the place gate instead and what went out
/// would be one bit rather than the whole list.
pub fn the_figures_own_picture_is_not_one_of_the_places_on_it() {
    let twenty_four = PAPERDOLL_REGIONS.len() == 24 && PAPER_DOLL_SLOTS.len() == 24;
    let the_picture_is_not_a_place = location_info_from_element_id(PAPER_DOLL_DRAG_MASK.0)
        .is_none()
        && InventoryPanels::location_of_slot(PAPER_DOLL_DRAG_MASK).is_none()
        && !PAPERDOLL_REGIONS
            .iter()
            .any(|(element, _, _)| *element == PAPER_DOLL_DRAG_MASK.0);
    let nor_are_the_others = [FIGURE_VIEWPORT, FIGURE_ANSWER_OVERLAY, PLACES_CHECKBOX]
        .into_iter()
        .all(|id| location_info_from_element_id(id).is_none() && id != PAPER_DOLL_DRAG_MASK.0);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "inventory.body.the-figures-own-picture-is-not-one-of-the-places-on-it",
        move |_| twenty_four && the_picture_is_not_a_place && nor_are_the_others,
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.body.what-can-be-worn-and-what-can-be-wielded-overlap-on-one-place-only
// ---------------------------------------------------------------------------------------------

/// What can be **worn** and what can be **wielded** are two lists of places that overlap on one
/// place only, and between them they do not cover everything.
///
/// Both exceptions are the interesting behaviour of the gate a drop on the picture makes: the
/// one place in both lists is a thing that is worn when it is let go on the picture and wielded
/// when it is let go on that place, and a thing that can only be **held** is in neither list and
/// is refused on the picture in words. The recordings exercise both sides of the gate, which is
/// asserted here so that a one-sided measurement reads as a broken premise.
pub fn what_can_be_worn_and_what_can_be_wielded_overlap_on_one_place_only() {
    let one_place_in_both = loc::WEARABLE & loc::WIELDABLE == loc::CLOAK;
    let held_is_in_neither = loc::WEARABLE | loc::WIELDABLE == loc::ALL & !loc::HELD;
    // The sentence the client says, written out once here rather than only compared with itself.
    let the_sentence = CANNOT_PUT_THAT_ITEM_THERE == "You can't put that item there";

    let equips = all_recorded_equips();
    let wearable = equips
        .iter()
        .filter(|e| e.places & loc::WEARABLE != 0)
        .count();
    let not_wearable = equips
        .iter()
        .filter(|e| e.places & loc::WEARABLE == 0)
        .count();
    let both_sides = wearable > 0 && not_wearable > 0 && wearable + not_wearable == equips.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "inventory.body.what-can-be-worn-and-what-can-be-wielded-overlap-on-one-place-only",
        move |_| one_place_in_both && held_is_in_neither && the_sentence && both_sides,
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.body.every-recorded-equip-let-go-on-the-picture-asks-with-its-whole-list-of-places
// ---------------------------------------------------------------------------------------------

/// Every thing the recordings record being put on, let go on the picture of the character, asks
/// the shard to put it on with **the whole of that thing's own list of places** -- the shard
/// chooses which one -- and the hold and the grey are taken while it waits.
///
/// The falsifiable half is the comparison with the other answer: a walk over the places would
/// send exactly one of them, so for any recorded thing whose list names more than one place the
/// two answers cannot coincide. The scenario requires at least one such thing, because without
/// one the two are indistinguishable and this measures nothing.
pub fn every_recorded_equip_let_go_on_the_picture_asks_with_its_whole_list_of_places() {
    let wearable: Vec<RecordedEquip> = all_recorded_equips()
        .into_iter()
        .filter(|e| e.places & loc::WEARABLE != 0)
        .collect();
    let the_recordings_carry_some = !wearable.is_empty();

    let mut c = a_client_holding(wearable[0].item, wearable[0].places);
    let mut asked = 0usize;
    let mut composite = 0usize;
    let mut every_one_holds = true;
    for e in &wearable {
        ready_for_another_drop(&mut c, e.item, e.places);
        let sent = let_go_on_the_picture(&mut c, e.item);
        let out = wears(&sent, e.item);
        every_one_holds &= out == vec![e.places];
        every_one_holds &= c.view().world().request_lock.object == Some(e.item);
        every_one_holds &= c.view().world().request_lock.pending
            == dereth_client_model::inventory::requests::InventoryRequest::Wield;
        every_one_holds &= c.view().world().weenie(e.item).expect("seeded").waiting;
        every_one_holds &= c.view().interaction().last_refusal.is_none();

        if e.places.count_ones() > 1 {
            composite += 1;
            // Not any one of the places a walk would have picked, and more than one bit: the
            // comparison is by place in the walk's own order rather than by naming a constant,
            // so re-ordering that order cannot make it vacuous.
            every_one_holds &= WIELD_SLOT_ORDER.iter().all(|(bit, _)| out != vec![*bit]);
            every_one_holds &= out.first().is_some_and(|l| l.count_ones() > 1);
        }
        asked += 1;
    }
    let all_of_them = asked == wearable.len();
    let at_least_one_composite = composite > 0;

    c.assert_behaviour(
        "inventory.body.every-recorded-equip-let-go-on-the-picture-asks-with-its-whole-list-of-places",
        move |_| the_recordings_carry_some && all_of_them && at_least_one_composite && every_one_holds,
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.body.something-that-could-be-worn-or-wielded-is-still-worn-on-the-picture
// ---------------------------------------------------------------------------------------------

/// The gate a drop on the picture makes asks whether the thing could be worn **anywhere**, not
/// whether it could be worn everywhere: something whose list of places names a place to wear and
/// a place to wield passes it, and the whole list goes out with the weapon place still in it --
/// the shard chooses.
///
/// Written by hand because the recordings carry no such thing, and said so rather than left
/// implicit.
pub fn something_that_could_be_worn_or_wielded_is_still_worn_on_the_picture() {
    const BOTH: ObjectId = ObjectId(0x5100_0001);
    let places = loc::CLOAK | loc::MELEE_WEAPON;
    // The fixture must not be the thing it is measuring.
    let a_real_mixture =
        places & loc::WEARABLE != 0 && places & loc::WIELDABLE != 0 && places != loc::WEARABLE;

    let mut c = a_client_holding(BOTH, places);
    let sent = let_go_on_the_picture(&mut c, BOTH);
    let asked_with_the_whole_list = wears(&sent, BOTH) == vec![places];
    let said_nothing = c.view().interaction().last_refusal.is_none();

    c.assert_behaviour(
        "inventory.body.something-that-could-be-worn-or-wielded-is-still-worn-on-the-picture",
        move |_| a_real_mixture && asked_with_the_whole_list && said_nothing,
    );
}

// ---------------------------------------------------------------------------------------------
// inventory.body.a-place-refuses-in-silence-where-the-picture-refuses-in-words
// ---------------------------------------------------------------------------------------------

/// The same thing, refused two ways: let go on a **place** it does not belong in, it is refused
/// without a word; let go on the **picture**, it is refused in the client's own sentence.
///
/// One thing, two elements, two behaviours -- which is what makes the sentence the picture's own
/// and not a general property of a refused drop. Both refusals put the grey back and neither
/// takes the one inventory hold, which has no timeout.
pub fn a_place_refuses_in_silence_where_the_picture_refuses_in_words() {
    let e = all_recorded_equips()
        .into_iter()
        .find(|e| e.places & loc::WEARABLE == 0)
        .expect("the recordings carry something that can only be wielded");

    // A place the thing does not belong in, taken from the client's own table by position rather
    // than written down as an element id.
    let (element, mask, _) = PAPERDOLL_REGIONS
        .iter()
        .copied()
        .find(|(_, m, _)| m & e.places == 0)
        .expect("some place on the figure this thing is not valid for");
    let really_not_its_place = mask & e.places == 0;

    let mut c = a_client_holding(e.item, e.places);
    let sent = let_go_on_a_place(&mut c, e.item, element);
    let the_place_sent_nothing = sent.is_empty();
    let the_place_said_nothing = c.view().interaction().last_refusal.is_none();
    let the_place_put_the_grey_back = !c.view().world().weenie(e.item).expect("seeded").waiting;
    let the_place_took_no_hold = c.view().world().request_lock.is_idle();

    ready_for_another_drop(&mut c, e.item, e.places);
    let sent = let_go_on_the_picture(&mut c, e.item);
    let the_picture_sent_nothing = sent.is_empty();
    let the_picture_said_so =
        c.view().interaction().last_refusal.as_deref() == Some(CANNOT_PUT_THAT_ITEM_THERE);
    let the_picture_put_the_grey_back = !c.view().world().weenie(e.item).expect("seeded").waiting;
    let the_picture_took_no_hold = c.view().world().request_lock.is_idle();

    c.assert_behaviour(
        "inventory.body.a-place-refuses-in-silence-where-the-picture-refuses-in-words",
        move |_| {
            really_not_its_place
                && the_place_sent_nothing
                && the_place_said_nothing
                && the_place_put_the_grey_back
                && the_place_took_no_hold
                && the_picture_sent_nothing
                && the_picture_said_so
                && the_picture_put_the_grey_back
                && the_picture_took_no_hold
        },
    );
}

#[test]
fn scenario_the_figures_own_picture_is_not_one_of_the_places_on_it() {
    scenario("the_figures_own_picture_is_not_one_of_the_places_on_it");
}

#[test]
fn scenario_what_can_be_worn_and_what_can_be_wielded_overlap_on_one_place_only() {
    scenario("what_can_be_worn_and_what_can_be_wielded_overlap_on_one_place_only");
}

#[test]
fn scenario_every_recorded_equip_let_go_on_the_picture_asks_with_its_whole_list_of_places() {
    scenario("every_recorded_equip_let_go_on_the_picture_asks_with_its_whole_list_of_places");
}

#[test]
fn scenario_something_that_could_be_worn_or_wielded_is_still_worn_on_the_picture() {
    scenario("something_that_could_be_worn_or_wielded_is_still_worn_on_the_picture");
}

#[test]
fn scenario_a_place_refuses_in_silence_where_the_picture_refuses_in_words() {
    scenario("a_place_refuses_in_silence_where_the_picture_refuses_in_words");
}

// -------------------------------------------------------------------------------------------
// 1. use.progress-notice.names-the-object
// -------------------------------------------------------------------------------------------

/// Using something prints a progress line that names it, and a creature gets the other wording.
pub fn use_progress_notice() {
    const CHEST: ObjectId = ObjectId(0x5000_0002);
    const NPC: ObjectId = ObjectId(0x5000_0003);

    let mut c = HeadlessClient::model();
    c.given(Given::APlayer(PLAYER));
    put(
        c.world_mut(),
        CHEST,
        "Chest",
        PublicWeenieDesc {
            obj_type: item_type::MISC,
            bitfield: bitfield::STUCK,
            useability: Some(USEABLE_REMOTE),
            ..PublicWeenieDesc::default()
        },
    );
    put(
        c.world_mut(),
        NPC,
        "Ulgrim the Unpleasant",
        PublicWeenieDesc {
            obj_type: item_type::CREATURE,
            bitfield: bitfield::STUCK,
            useability: Some(USEABLE_REMOTE),
            ..PublicWeenieDesc::default()
        },
    );

    // **Ten frames between the two clicks.** The client refuses a second use within a fifth of a
    // second of the first, which is how it stops one double-click from being sent twice; two
    // gestures in the same frame would leave this scenario asserting over one of them.
    c.when(Player::DoubleClick(CHEST))
        .tick(10)
        .when(Player::DoubleClick(NPC))
        .assert_behaviour("use.progress-notice.names-the-object", |v| {
            bubble_lines(v) == ["Using the Chest", "Approaching Ulgrim the Unpleasant"]
        });
}

#[test]
fn scenario_use_progress_notice() {
    scenario("use_progress_notice");
}

// -------------------------------------------------------------------------------------------
// 8. trade.drop.sends-add-to-trade-and-marks-the-item
// -------------------------------------------------------------------------------------------

/// A drop on the trade table sends one add naming the item and its position, and marks it.
pub fn trade_drop_sends_add_to_trade() {
    const PARTNER: ObjectId = ObjectId(0x5000_0002);
    const ITEM: ObjectId = ObjectId(0x8000_0A6E);
    const ITEM2: ObjectId = ObjectId(0x8000_0A6F);
    const THEIRS: ObjectId = ObjectId(0x8000_0B00);

    let mut c = HeadlessClient::model();
    c.world_mut().player = Some(PLAYER);
    for (id, name) in [
        (ITEM, "Sturdy Iron Key"),
        (ITEM2, "Prismatic Taper"),
        (THEIRS, "Pyreal"),
    ] {
        put(
            c.world_mut(),
            id,
            name,
            PublicWeenieDesc {
                icon_id: 0x0600_1234,
                // Mine are in my pack; theirs is not, and that is what decides whether a drop may
                // mark it.
                container_id: (id != THEIRS).then_some(PLAYER),
                ..PublicWeenieDesc::default()
            },
        );
    }
    put(c.world_mut(), PARTNER, "Alba", PublicWeenieDesc::default());

    c.when(Inbound::message(
        &dereth_protocol::trade::TradeRegisterTrade {
            initiator: PLAYER,
            partner: PARTNER,
            stamp: 4.25,
        },
    ))
    .when(Player::Ui(vec![
        UiRequest::TradeAddItem {
            item: ITEM,
            position: 0,
        },
        UiRequest::TradeAddItem {
            item: ITEM2,
            position: 1,
        },
    ]))
    .assert_behaviour("trade.drop.sends-add-to-trade-and-marks-the-item", |v| {
        let sent = matches!(
            v.outbound(),
            [Request::TradeAddToTrade(a), Request::TradeAddToTrade(b)]
                if (a.item, a.slot) == (ITEM, 0) && (b.item, b.slot) == (ITEM2, 1)
        );
        let marked = v.world().weenie(ITEM).is_some_and(|w| w.trade_state == 1)
            && v.world().weenie(ITEM2).is_some_and(|w| w.trade_state == 1)
            && v.world().weenie(THEIRS).is_some_and(|w| w.trade_state == 0);
        sent && marked
    });
}

#[test]
fn scenario_trade_drop_sends_add_to_trade() {
    scenario("trade_drop_sends_add_to_trade");
}

// -------------------------------------------------------------------------------------------
// 9. container.ground.contents-become-a-pickup
// -------------------------------------------------------------------------------------------

/// Opening a container on the ground makes its contents pickups rather than uses.
pub fn ground_container_contents_become_a_pickup() {
    const CORPSE: ObjectId = ObjectId(0x5000_0002);
    const LOOT: ObjectId = ObjectId(0x5000_0003);

    let mut c = HeadlessClient::model();
    c.given(Given::APlayer(PLAYER));
    put(
        c.world_mut(),
        CORPSE,
        "Drudge Slave",
        PublicWeenieDesc {
            obj_type: item_type::CONTAINER,
            bitfield: bitfield::OPENABLE | bitfield::CORPSE,
            items_capacity: Some(10),
            useability: Some(USEABLE_REMOTE),
            ..PublicWeenieDesc::default()
        },
    );
    put(
        c.world_mut(),
        LOOT,
        "Lead Scarab",
        PublicWeenieDesc {
            obj_type: item_type::MISC,
            container_id: Some(CORPSE),
            ..PublicWeenieDesc::default()
        },
    );

    c.when(Player::DoubleClick(CORPSE));
    let opened = c.view().world().ground_object == Some(CORPSE);

    // Ten frames: the use throttle again, as in `use_progress_notice`.
    c.tick(10).when(Player::DoubleClick(LOOT)).assert_behaviour(
        "container.ground.contents-become-a-pickup",
        move |v| {
            let picked_up = matches!(
                v.outbound().last(),
                Some(Request::PutItemInContainer(m)) if m.item == LOOT
            );
            let used_the_corpse = v
                .outbound()
                .iter()
                .any(|r| matches!(r, Request::UseEvent(m) if m.object == CORPSE));
            opened && used_the_corpse && picked_up
        },
    );
}

#[test]
fn scenario_ground_container_contents_become_a_pickup() {
    scenario("ground_container_contents_become_a_pickup");
}
