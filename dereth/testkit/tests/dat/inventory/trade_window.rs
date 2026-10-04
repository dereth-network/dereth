use dereth_client_model::Request;
use dereth_primitives::{DataId, ObjectId};
use dereth_testkit::player::{ScreenPoint, Target};
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound, Player};
use dereth_ui::ElementId;
use dereth_ui_screens::items::widget::drag_accept_state;
use dereth_ui_screens::panels::trade::{self, status_state, ButtonState};

/// The two players.
const ME: ObjectId = ObjectId(0x5000_0001);
const PARTNER: ObjectId = ObjectId(0x5000_0002);
/// The three things in the player's own pack.
const GEM: ObjectId = ObjectId(0x5000_0021);
const COIN: ObjectId = ObjectId(0x5000_0022);
const PACK: ObjectId = ObjectId(0x5000_0023);
/// What is inside the pack.
const IN_PACK_A: ObjectId = ObjectId(0x5000_0024);
const IN_PACK_B: ObjectId = ObjectId(0x5000_0025);
/// The container the partner puts on the table, and the one thing inside it.
const THEIRS: ObjectId = ObjectId(0x5000_0031);
const THEIR_CONTENT: ObjectId = ObjectId(0x5000_0033);
/// The stack the shard makes when part of one is split off for a trade.
const SPLIT_RESULT: ObjectId = ObjectId(0x5000_0032);
const COIN_WCID: u32 = 0x0ACE;
const ICON: DataId = DataId(0x0600_1234);

/// Which side of the table a message names.
const MY_SIDE: u32 = 1;
const THEIR_SIDE: u32 = 2;

// -----------------------------------------------------------------------------------------
// The fixture
// -----------------------------------------------------------------------------------------

/// The world the shard would have built: the player, three things in their pack, one of them
/// a pack with two things in it, the partner, and a container the partner is holding with one
/// thing inside it.
fn seed(c: &mut HeadlessClient) {
    let w = c.world_mut();
    w.player = Some(ME);
    let mut me = dereth_client_model::Weenie::new(ME);
    me.valid = true;
    me.pwd.name = "Tester".into();
    me.pwd.bitfield |= dereth_client_model::weenie::bitfield::PLAYER;
    w.tables.weenies.insert(ME, me);
    w.tables
        .inventories
        .insert(ME, dereth_client_model::objects::ObjectInventory::new(ME));
    for (i, (id, name)) in [(GEM, "Peridot"), (COIN, "Pyreal"), (PACK, "Belt Pouch")]
        .into_iter()
        .enumerate()
    {
        let mut wn = dereth_client_model::Weenie::new(id);
        wn.valid = true;
        wn.pwd.name = name.into();
        wn.pwd.container_id = Some(ME);
        wn.pwd.stack_size = Some(if id == COIN { 10 } else { 1 });
        wn.pwd.max_stack_size = Some(if id == COIN { 10 } else { 1 });
        if id == COIN {
            wn.pwd.wcid = COIN_WCID;
        }
        wn.pwd.icon_id = ICON.0;
        w.tables.weenies.insert(id, wn);
        w.tables
            .inventories
            .get_mut(ME)
            .expect("seeded")
            .add_content(id, false, i);
    }
    w.tables.inventories.insert(
        PACK,
        dereth_client_model::objects::ObjectInventory::new(PACK),
    );
    for (i, (id, name)) in [(IN_PACK_A, "Lead Scarab"), (IN_PACK_B, "Iron Scarab")]
        .into_iter()
        .enumerate()
    {
        let mut wn = dereth_client_model::Weenie::new(id);
        wn.valid = true;
        wn.pwd.name = name.into();
        wn.pwd.container_id = Some(PACK);
        wn.pwd.stack_size = Some(1);
        wn.pwd.icon_id = ICON.0;
        w.tables.weenies.insert(id, wn);
        w.tables
            .inventories
            .get_mut(PACK)
            .expect("seeded")
            .add_content(id, false, i);
    }
    for (id, name, is_player) in [(PARTNER, "Jorune", true), (THEIRS, "Silver", false)] {
        let mut wn = dereth_client_model::Weenie::new(id);
        wn.valid = true;
        wn.pwd.name = name.into();
        wn.pwd.stack_size = Some(1);
        wn.pwd.icon_id = ICON.0;
        if is_player {
            // The premise, and the world a shard builds: another player is marked as one, is
            // not a thing you can use, and carries a pack of their own. Those three are what
            // send a double-click on them down the trade path rather than the use path.
            wn.pwd.bitfield |= dereth_client_model::weenie::bitfield::PLAYER;
            wn.pwd.useability = Some(dereth_client_model::weenie::item_useable::NO);
            wn.pwd.items_capacity = Some(102);
        }
        w.tables.weenies.insert(id, wn);
    }
    w.tables.inventories.insert(
        THEIRS,
        dereth_client_model::objects::ObjectInventory::new(THEIRS),
    );
    w.tables
        .weenies
        .get_mut(THEIRS)
        .expect("the partner's offer")
        .pwd
        .items_capacity = Some(1);
    let mut content = dereth_client_model::Weenie::new(THEIR_CONTENT);
    content.valid = true;
    content.pwd.name = "Silver Key".into();
    content.pwd.container_id = Some(THEIRS);
    content.pwd.stack_size = Some(1);
    w.tables.weenies.insert(THEIR_CONTENT, content);
    w.tables
        .inventories
        .get_mut(THEIRS)
        .expect("the partner's offer")
        .add_content(THEIR_CONTENT, false, 0);
    let p = w.tables.weenies.get_mut(ME).expect("the player");
    p.pwd.items_capacity = Some(102);
    p.pwd.containers_capacity = Some(7);
    p.pwd.bitfield |= dereth_client_model::weenie::bitfield::OPENABLE;
}

/// A whole client with the shipped gameplay screen up, the pack open, and that world in it.
fn a_client_with_things_to_trade() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    seed(&mut c);
    super::open_pack(&mut c);
    c.tick(3);
    c
}

/// The shard's own "you two are trading now".
fn the_shard_registers_the_trade(c: &mut HeadlessClient) {
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeRegisterTrade {
            initiator: ME,
            partner: PARTNER,
            stamp: 0.0,
        },
    ))
    .tick(3);
}

/// Take the five-metre watch off the fixture partner.
///
/// Registering a trade arms a watch on the partner, and this fixture's partner is an object
/// with no body, which counts as out of range -- so the negotiation would close itself a
/// second in. That is the *subject* of one scenario below and noise in every other, so every
/// other one disarms it as a premise.
fn unwatch_the_partner(c: &mut HeadlessClient) {
    c.world_mut().object_range_checks.unregister(
        dereth_client_model::range::RangeHandler::SecureTrade,
        PARTNER,
    );
}

/// An open negotiation with nothing on the table yet.
fn an_open_negotiation() -> HeadlessClient {
    let mut c = a_client_with_things_to_trade();
    the_shard_registers_the_trade(&mut c);
    assert!(
        c.view().hud().panels.trade.visible,
        "the premise: the window is up"
    );
    unwatch_the_partner(&mut c);
    c
}

/// The shard puts `item` on `side` of the table.
fn the_shard_adds(c: &mut HeadlessClient, item: ObjectId, side: u32) {
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeAddToTradeRecv {
            item,
            side,
            container_properties: 0,
        },
    ))
    .tick(3);
}

// -----------------------------------------------------------------------------------------
// Reading the window
// -----------------------------------------------------------------------------------------

/// What is drawn on one side of the table, in row order.
fn rows(c: &HeadlessClient, partner_side: bool) -> Vec<ObjectId> {
    c.view()
        .hud()
        .panels
        .trade
        .rows(partner_side)
        .iter()
        .map(|r| r.item)
        .collect()
}

/// Whether the window itself is on screen, read off the shipped element and not off the
/// panel's own record of what it wrote.
fn window_is_up(c: &mut HeadlessClient) -> bool {
    let h = c
        .view()
        .hud()
        .panels
        .trade
        .root
        .expect("the trade window is in the tree");
    c.app_mut().ui_mut().expect("the UI shell").ui.is_visible(h)
}

/// How many rows one side already has, which is the tile a player aims a drop at.
fn first_empty(c: &HeadlessClient, partner_side: bool) -> usize {
    let p = &c.view().hud().panels.trade;
    let w = if partner_side {
        p.other_list.as_ref()
    } else {
        p.self_list.as_ref()
    };
    w.expect("both lists are bound")
        .slots
        .iter()
        .filter(|s| s.item.is_some())
        .count()
}

/// The centre of one tile of one of the window's two lists.
///
/// A list builds every tile from one template, so all of them carry one shipped id and a tile
/// has to be named by its own centre.
fn tile(c: &mut HeadlessClient, partner_side: bool, i: usize) -> Target {
    let h = {
        let p = &c.view().hud().panels.trade;
        let w = if partner_side {
            p.other_list.as_ref()
        } else {
            p.self_list.as_ref()
        };
        w.expect("both lists are bound")
            .slots
            .get(i)
            .unwrap_or_else(|| panic!("the list has no tile {i}"))
            .handle
    };
    let (ui, _s) = super::gameplay_screen(c.app_mut());
    let (x, y) = super::centre(ui, h);
    Target::Point(ScreenPoint::new(x, y))
}

/// The centre of the pack tile `item` is drawn in.
fn in_the_pack(c: &mut HeadlessClient, item: ObjectId) -> Target {
    let h = super::pack_slot(c, item);
    let (ui, _s) = super::gameplay_screen(c.app_mut());
    let (x, y) = super::centre(ui, h);
    Target::Point(ScreenPoint::new(x, y))
}

/// Whether the thing under the pointer at `t` really is inside the list `id`.
///
/// Every negative claim below needs this: a drop that landed somewhere else would satisfy
/// "nothing happened" for the wrong reason.
fn points_into(c: &mut HeadlessClient, t: Target, id: ElementId) -> bool {
    let Target::Point(p) = t else {
        panic!("a tile is named by its own centre")
    };
    let (ui, _s) = super::gameplay_screen(c.app_mut());
    let Some(hit) = ui.hit_test_screen(p.x, p.y) else {
        return false;
    };
    let mut h = hit;
    loop {
        if ui.node(h).map(dereth_ui::ElementNode::element_id) == Some(id) {
            return true;
        }
        match ui.parent(h) {
            Some(q) => h = q,
            None => return false,
        }
    }
}

/// What the window last answered about one tile: the answer the panel recorded, and the state
/// the overlay element is really in. Two readings, because the record alone cannot catch a
/// panel that decided correctly and drew nothing, and the element alone cannot tell "never
/// answered" from "answered no".
fn hint(
    c: &mut HeadlessClient,
    partner_side: bool,
    i: usize,
) -> (Option<dereth_ui::StateId>, Option<dereth_ui::StateId>) {
    let (recorded, overlay) = {
        let p = &c.view().hud().panels.trade;
        let w = if partner_side {
            p.other_list.as_ref()
        } else {
            p.self_list.as_ref()
        };
        let s = &w.expect("both lists are bound").slots[i];
        (s.drag_accept_state, s.drag_accept)
    };
    let (ui, _s) = super::gameplay_screen(c.app_mut());
    (recorded, overlay.and_then(|h| ui.node(h)).map(|n| n.state))
}

// -----------------------------------------------------------------------------------------
// What the client sent
// -----------------------------------------------------------------------------------------

/// Everything the client asked the shard for since `mark`, less the two appraisals a pointer
/// crossing a pack tile raises on its own and that have nothing to do with trading.
fn asked_since(c: &HeadlessClient, mark: usize) -> Vec<Request> {
    c.outbound()[mark..]
        .iter()
        .filter(|r| !matches!(r, Request::QueryHealth(_) | Request::QueryItemMana(_)))
        .cloned()
        .collect()
}

/// Everything the client has put in front of the player on the strip across the top of the
/// screen, drawn or waiting to be drawn -- which is the surface the refusals and the
/// running commentary of a trade appear on.
fn said(c: &HeadlessClient) -> Vec<String> {
    super::feedback(c)
}

/// One whole drag out of the pack on to the first free tile of your own half of the table,
/// and everything it asked the shard for.
fn offer(c: &mut HeadlessClient, item: ObjectId) -> Vec<Request> {
    let from = in_the_pack(c, item);
    let to = tile(c, false, first_empty(c, false));
    assert!(
        points_into(c, to, trade::SELF_LIST),
        "the gesture must really land inside your own half of the table"
    );
    let mark = c.outbound().len();
    c.when(Player::Grab(from))
        .when(Player::Over(to))
        .when(Player::Drop(to));
    asked_since(c, mark)
}

/// The item and position every add-to-trade named, in the order they were asked for.
fn added(sent: &[Request]) -> Vec<(ObjectId, u32)> {
    sent.iter()
        .filter_map(|r| match r {
            Request::TradeAddToTrade(m) => Some((m.item, m.slot)),
            _ => None,
        })
        .collect()
}

// -----------------------------------------------------------------------------------------
// trade.window.the-shards-registration-raises-it-and-names-the-partner
// -----------------------------------------------------------------------------------------

/// The shard saying two people are trading is what puts the window on screen, and the partner
/// is named in it at the same moment.
///
/// The control is the frame before: with no negotiation the window is down, so this is a
/// change and not a value the panel writes anyway.
pub fn the_registration_raises_the_window_and_names_the_partner() {
    let mut c = a_client_with_things_to_trade();
    let down_first = !window_is_up(&mut c) && !c.view().hud().panels.trade.visible;

    the_shard_registers_the_trade(&mut c);

    let up = window_is_up(&mut c) && c.view().hud().panels.trade.visible;
    let named = c.view().hud().panels.trade.partner_name_text.clone();
    let one_registration = c.view().interaction().stats.trade_registers == 1;
    // The other message the shard could have sent is not what raises it, and the shard this
    // client is rebuilt against never sends it at all.
    let no_other_opener = c.view().interaction().stats.trade_opens == 0;

    c.assert_behaviour(
        "trade.window.the-shards-registration-raises-it-and-names-the-partner",
        { move |_| down_first && up && named == "Jorune" && one_registration && no_other_opener },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.table.a-real-drag-from-the-pack-offers-the-item-and-draws-it-at-once
// -----------------------------------------------------------------------------------------

/// A drag out of the pack on to the table asks the shard for the item once, and the row is on
/// screen before the shard has answered. The shard's answer then leaves one row, not two.
pub fn a_real_drag_offers_the_item_and_draws_it_at_once() {
    let mut c = an_open_negotiation();

    let sent = offer(&mut c, GEM);
    let one_ask = added(&sent) == vec![(GEM, 0)] && sent.len() == 1;
    let drawn_at_once = rows(&c, false) == vec![GEM];
    let named = c.view().hud().panels.trade.rows(false)[0].name == "Peridot";

    the_shard_adds(&mut c, GEM, MY_SIDE);
    let not_doubled = rows(&c, false) == vec![GEM];
    let still_up = window_is_up(&mut c);

    c.assert_behaviour(
        "trade.table.a-real-drag-from-the-pack-offers-the-item-and-draws-it-at-once",
        move |_| one_ask && drawn_at_once && named && not_doubled && still_up,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.drag-hint.your-own-half-lights-for-something-you-are-carrying
// -----------------------------------------------------------------------------------------

/// Carrying something you own over your own half of the table lights it as accepted, and
/// letting go there really does offer it -- so the light and the drop cannot disagree.
pub fn your_own_half_lights_for_something_you_are_carrying() {
    let mut c = an_open_negotiation();
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);

    let slot = first_empty(&c, false);
    let never_answered = hint(&mut c, false, slot).0.is_none();

    let from = in_the_pack(&mut c, GEM);
    let to = tile(&mut c, false, slot);
    assert!(
        points_into(&mut c, to, trade::SELF_LIST),
        "the carry lands on your own half"
    );
    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Over(to));
    let lit = hint(&mut c, false, slot)
        == (
            Some(drag_accept_state::ACCEPT),
            Some(drag_accept_state::ACCEPT),
        );

    c.when(Player::Drop(to));
    let offered = added(&asked_since(&c, mark)) == vec![(GEM, 0)];

    c.assert_behaviour(
        "trade.drag-hint.your-own-half-lights-for-something-you-are-carrying",
        { move |_| never_answered && lit && offered },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.close.the-close-button-is-the-only-gesture-that-takes-the-window-down
// -----------------------------------------------------------------------------------------

/// The window's own close button is the one thing that takes it down, and taking it down is
/// what tells the shard the negotiation is over. What the shard has confirmed survives that:
/// the window empties, the shard's own record does not, and only the shard's own close
/// retires it.
pub fn the_close_button_is_the_only_gesture_that_takes_the_window_down() {
    let mut c = an_open_negotiation();
    let _ = offer(&mut c, GEM);
    the_shard_adds(&mut c, GEM, MY_SIDE);
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: ME },
    ))
    .when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: PARTNER },
    ))
    .tick(3);
    assert!(
        c.view().world().trade.trade.both_accepted(),
        "the premise: both sides agreed"
    );

    let mark = c.outbound().len();
    c.when(Player::click(trade::BTN_CLOSE));

    let sent = asked_since(&c, mark);
    let one_close = matches!(sent.as_slice(), [Request::TradeCloseTradeNegotiations(_)]);
    let down = !window_is_up(&mut c);
    let emptied = rows(&c, false).is_empty() && rows(&c, true).is_empty();
    let record_survives = {
        let t = &c.view().world().trade;
        t.trade.self_list.iter().map(|p| p.iid).collect::<Vec<_>>() == vec![GEM]
                && t.trade.partner_list.iter().map(|p| p.iid).collect::<Vec<_>>() == vec![THEIRS]
                && t.trade.both_accepted()
                // The emptying is an override the window puts over that record, and the
                // agreement it was showing is put out with it.
                && t.display_lists == Some([Vec::new(), Vec::new()])
                && t.acceptance_darkened
    };

    c.when(Inbound::message(&dereth_protocol::trade::TradeCloseTrade {
        reason: 1,
    }))
    .tick(3);
    let the_shards_close_retires_it = {
        let t = &c.view().world().trade;
        t.trade.self_list.is_empty()
            && t.trade.partner_list.is_empty()
            && !t.trade.both_accepted()
            && t.display_lists.is_none()
    };

    c.assert_behaviour(
        "trade.close.the-close-button-is-the-only-gesture-that-takes-the-window-down",
        move |_| one_close && down && emptied && record_survives && the_shards_close_retires_it,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.table.part-of-a-stack-is-split-first-and-only-the-shards-own-new-stack-is-offered
// -----------------------------------------------------------------------------------------

/// Dragging part of a stack on to the table splits it first, says so, and offers only the
/// stack the shard makes -- never the one the player dragged.
///
/// The whole of it is driven through the shipped grid and the shipped slider: the stack is
/// picked by clicking its own tile and the amount is set through the notice the slider
/// raises, so nothing here bypasses the quantity a player would have chosen.
pub fn part_of_a_stack_is_split_first_and_only_the_shards_stack_is_offered() {
    let mut c = an_open_negotiation();

    // Pick the stack and ask for three of the ten.
    let tile_of_the_stack = in_the_pack(&mut c, COIN);
    c.when(Player::Click(tile_of_the_stack)).tick(4);
    let picked = c.view().world().selected == Some(COIN);
    ask_for(&mut c, 3);
    let three_in_hand = c.view().world().split.split_size == 3;

    let sent = offer(&mut c, COIN);
    let asked_to_split = matches!(
        sent.as_slice(),
        [Request::StackableSplitToContainer(m)]
            if m.stack == COIN && m.container == ME && m.slot == 0 && m.amount == 3
    );
    let nothing_on_the_table_yet = rows(&c, false).is_empty();
    let remembered = c.view().world().trade.pending_split.map(|p| p.stack_size) == Some(3);
    let told = said(&c)
        .iter()
        .any(|l| l == "Splitting the Pyreals before trading them");

    // The shard refusing the split must leave nothing behind that a later object could be
    // mistaken for.
    c.when(Inbound::message(
        &dereth_protocol::objects::CharacterServerSaysAttemptFailed {
            object: COIN,
            reason: 0,
        },
    ))
    .tick(3);
    let refusal_forgets_it =
        c.view().world().trade.pending_split.is_none() && rows(&c, false).is_empty();

    // Try again, and let the shard answer properly this time.
    let sent = offer(&mut c, COIN);
    let asked_again = matches!(sent.as_slice(), [Request::StackableSplitToContainer(_)]);
    c.when(Inbound::message(
        &dereth_protocol::items::ItemUpdateStackSize {
            sequence: 1,
            item: COIN,
            amount: 7,
            new_value: 0,
        },
    ))
    .tick(2);
    let the_sources_new_count_is_not_the_answer = rows(&c, false).is_empty();

    let mark = c.outbound().len();
    c.when(Inbound::world_view(&the_shards_new_stack())).tick(4);
    let offered = added(&asked_since(&c, mark)) == vec![(SPLIT_RESULT, 0)];
    let only_the_new_one = rows(&c, false) == vec![SPLIT_RESULT];
    let done = c.view().world().trade.pending_split.is_none();

    c.assert_behaviour(
        "trade.table.part-of-a-stack-is-split-first-and-only-the-shards-own-new-stack-is-offered",
        move |_| {
            picked
                && three_in_hand
                && asked_to_split
                && nothing_on_the_table_yet
                && remembered
                && told
                && refusal_forgets_it
                && asked_again
                && the_sources_new_count_is_not_the_answer
                && offered
                && only_the_new_one
                && done
        },
    );
    c.shutdown();
}

/// Ask for `amount` of the picked stack through the toolbar splitter's own text handler --
/// the path a player typing a number into the stack box takes, and the one place in this
/// build that moves both copies of the quantity a drop then reads.
fn ask_for(c: &mut HeadlessClient, amount: u32) {
    let notice = {
        let (_ui, screen) = super::gameplay_screen(c.app_mut());
        screen.splitter.on_text(&amount.to_string()).0
    };
    c.ui_outbox().emit(notice);
    c.tick(2);
}

/// The stack the shard makes out of the split: three of the ten, in the player's own pack.
fn the_shards_new_stack() -> dereth_protocol::objects::ItemCreateObject {
    let mut p = dereth_protocol::objects::ObjectCreatePayload {
        id: SPLIT_RESULT,
        ..Default::default()
    };
    p.physicsdesc.timestamps.instance = 2;
    p.wdesc.header = dereth_protocol::types::weeniedesc::header::STACK_SIZE
        | dereth_protocol::types::weeniedesc::header::MAX_STACK_SIZE
        | dereth_protocol::types::weeniedesc::header::CONTAINER_ID;
    p.wdesc.name = "Pyreal".to_owned();
    p.wdesc.wcid = COIN_WCID;
    p.wdesc.icon_id = ICON.0;
    p.wdesc.stack_size = Some(3);
    p.wdesc.max_stack_size = Some(10);
    p.wdesc.container_id = Some(ME);
    dereth_protocol::objects::ItemCreateObject(p)
}

// -----------------------------------------------------------------------------------------
// trade.table.anything-either-side-changes-puts-both-agreements-out
// -----------------------------------------------------------------------------------------

/// The partner agreeing lights their side; the partner then putting something else on the
/// table puts that light straight out again and offers you the choice afresh -- even though
/// the shard has said nothing to take the agreement back.
///
/// The control is that the shard's own record still says the partner agreed, so what is
/// measured is the window refusing to keep showing it and not a value arriving from outside.
pub fn anything_either_side_changes_puts_both_agreements_out() {
    let mut c = an_open_negotiation();
    let _ = offer(&mut c, GEM);
    the_shard_adds(&mut c, GEM, MY_SIDE);

    c.when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: PARTNER },
    ))
    .tick(3);
    let lit = c.view().hud().panels.trade.partner_status == status_state::ACCEPTED;

    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);
    let the_record_still_agrees = c.view().world().trade.trade.partner_accepted;
    let out_again = c.view().hud().panels.trade.partner_status == status_state::NOT_ACCEPTED;
    let offered_afresh = c.view().hud().panels.trade.button == ButtonState::Enabled;
    let their_row_is_there = rows(&c, true) == vec![THEIRS];

    c.assert_behaviour(
        "trade.table.anything-either-side-changes-puts-both-agreements-out",
        {
            move |_| {
                lit && the_record_still_agrees && out_again && offered_afresh && their_row_is_there
            }
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.table.the-shard-voiding-both-agreements-empties-the-window-without-closing-it
// -----------------------------------------------------------------------------------------

/// The shard voiding both agreements clears the table the player is looking at, takes the
/// traded marks back off, and leaves the window open -- and it does not touch the shard's own
/// record of the trade.
///
/// Then the two things that follow from that: pressing accept on a window that disagrees with
/// that record offers an empty trade rather than the one the shard still holds, and a fresh
/// drop on to the emptied window is taken rather than refused for something the record still
/// remembers.
pub fn the_shard_voiding_both_agreements_empties_the_window_without_closing_it() {
    let mut c = an_open_negotiation();
    let _ = offer(&mut c, GEM);
    the_shard_adds(&mut c, GEM, MY_SIDE);
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: PARTNER },
    ))
    .tick(3);
    let _ = offer(&mut c, COIN); // drawn, and deliberately never confirmed
    let marked = c
        .view()
        .world()
        .weenie(COIN)
        .expect("the stack")
        .trade_state
        != 0;
    let record_before = c.view().world().trade.trade.clone();

    let mark = c.outbound().len();
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeClearTradeAcceptance,
    ))
    .tick(3);

    let record_untouched = c.view().world().trade.trade == record_before;
    let still_up = window_is_up(&mut c);
    let emptied = rows(&c, false).is_empty() && rows(&c, true).is_empty();
    let unmarked = c
        .view()
        .world()
        .weenie(COIN)
        .expect("the stack")
        .trade_state
        == 0;
    let nothing_to_accept = c.view().hud().panels.trade.button == ButtonState::Disabled
        && c.view().hud().panels.trade.partner_status == status_state::NOT_ACCEPTED;
    let said_nothing_back = asked_since(&c, mark).is_empty();
    c.tick(3);
    let stays_empty = rows(&c, false).is_empty() && rows(&c, true).is_empty();

    // The shard putting the partner's row back fills their side alone.
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);
    let only_theirs_came_back = rows(&c, true) == vec![THEIRS] && rows(&c, false).is_empty();

    // Pressing accept now, with the window and the record disagreeing, offers nothing.
    let mark = c.outbound().len();
    c.when(Player::click(trade::BTN_TRADE));
    let empty_agreement = match asked_since(&c, mark).as_slice() {
        [Request::TradeAcceptTrade(m)] => {
            m.0 == dereth_client_model::trade::Trade::default().to_wire()
        }
        _ => false,
    };

    // And a fresh drop on to the emptied window is taken.
    let sent = offer(&mut c, GEM);
    let taken_again = added(&sent) == vec![(GEM, 0)] && rows(&c, false) == vec![GEM];
    the_shard_adds(&mut c, GEM, MY_SIDE);
    let one_row = rows(&c, false) == vec![GEM];

    c.assert_behaviour(
        "trade.table.the-shard-voiding-both-agreements-empties-the-window-without-closing-it",
        move |_| {
            marked
                && record_untouched
                && still_up
                && emptied
                && unmarked
                && nothing_to_accept
                && said_nothing_back
                && stays_empty
                && only_theirs_came_back
                && empty_agreement
                && taken_again
                && one_row
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.table.the-partners-container-keeps-what-is-inside-it-alive-only-while-it-is-on-show
// -----------------------------------------------------------------------------------------

/// What is inside a container the partner offers is only shown to you while the row is on the
/// table: taking the row away puts the contents on the list of things to forget, the row
/// coming back cancels that, and the container itself is never forgotten either way.
pub fn the_partners_container_keeps_its_contents_alive_only_while_shown() {
    let mut c = an_open_negotiation();
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);
    let shown = rows(&c, true) == vec![THEIRS];

    c.when(Inbound::message(
        &dereth_protocol::trade::TradeClearTradeAcceptance,
    ))
    .tick(3);
    let flushed = rows(&c, true).is_empty();
    let first_deadline = c
        .view()
        .world()
        .tables
        .doomed
        .get(THEIR_CONTENT)
        .copied()
        .expect("the contents of a flushed partner container are queued to be forgotten");
    let the_container_stays = !c.view().world().tables.doomed.contains_key(THEIRS);

    // The row coming back cancels it; the row going away again puts it back.
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);
    let reoffer_cancels = !c.view().world().tables.doomed.contains_key(THEIR_CONTENT);
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeClearTradeAcceptance,
    ))
    .tick(3);
    let flush_queues_again = c.view().world().tables.doomed.contains_key(THEIR_CONTENT);

    // A move that puts the container anywhere but with the partner restores no row and
    // cancels nothing.
    c.when(Inbound::message(
        &dereth_protocol::objects::ItemServerSaysContainId {
            item: THEIRS,
            container: ME,
            slot: 0,
            container_properties: 0,
        },
    ))
    .tick(3);
    let a_move_elsewhere_changes_nothing =
        rows(&c, true).is_empty() && c.view().world().tables.doomed.contains_key(THEIR_CONTENT);

    // The partner's own move restores the row and cancels the forgetting outright.
    c.when(Inbound::message(
        &dereth_protocol::objects::ItemServerSaysContainId {
            item: THEIRS,
            container: PARTNER,
            slot: 0,
            container_properties: 0,
        },
    ))
    .tick(3);
    let restored = rows(&c, true) == vec![THEIRS]
        && !c.view().world().tables.doomed.contains_key(THEIR_CONTENT);
    run_the_clock_to(&mut c, first_deadline.0 + 0.001);
    let cancelled_stays_cancelled = c.view().world().weenie(THEIR_CONTENT).is_some();

    // Taking the row off for good queues the contents again, and this time they go.
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeRemoveFromTrade {
            item: THEIRS,
            side: THEIR_SIDE,
        },
    ))
    .tick(3);
    let removal_deadline = c
        .view()
        .world()
        .tables
        .doomed
        .get(THEIR_CONTENT)
        .copied()
        .expect("taking the row off queues the contents again");
    run_the_clock_to(&mut c, removal_deadline.0 + 0.001);
    let forgotten = c.view().world().weenie(THEIR_CONTENT).is_none();
    let the_container_remains = c.view().world().weenie(THEIRS).is_some();

    c.assert_behaviour(
        "trade.table.the-partners-container-keeps-what-is-inside-it-alive-only-while-it-is-on-show",
        move |_| {
            shown
                && flushed
                && the_container_stays
                && reoffer_cancels
                && flush_queues_again
                && a_move_elsewhere_changes_nothing
                && restored
                && cancelled_stays_cancelled
                && forgotten
                && the_container_remains
        },
    );
    c.shutdown();
}

/// Let the client's own clock reach `at`, which is how a thing queued to be forgotten is
/// forgotten.
fn run_the_clock_to(c: &mut HeadlessClient, at: f64) {
    let mut out = dereth_client_model::RecordingSink::default();
    let mut req = dereth_client_model::RecordingRequests::default();
    c.world_mut()
        .use_time(dereth_primitives::ServerTime(at), &mut out, &mut req);
}

// -----------------------------------------------------------------------------------------
// trade.controls.the-one-button-agrees-and-then-takes-it-back-without-ending-anything
// -----------------------------------------------------------------------------------------

/// The window's single agreement button is both answers: pressing it agrees, pressing it
/// again takes the agreement back, and neither press ends the negotiation or clears the
/// table.
pub fn the_one_button_agrees_and_then_takes_it_back() {
    let mut c = an_open_negotiation();
    let _ = offer(&mut c, GEM);
    the_shard_adds(&mut c, GEM, MY_SIDE);
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);
    let awake = c.view().hud().panels.trade.button == ButtonState::Enabled;

    let mark = c.outbound().len();
    c.when(Player::click(trade::BTN_TRADE));
    let agreed = matches!(
        asked_since(&c, mark).as_slice(),
        [Request::TradeAcceptTrade(_)]
    );
    let showing_agreement = c.view().hud().panels.trade.button == ButtonState::Accepted;

    let mark = c.outbound().len();
    c.when(Player::click(trade::BTN_TRADE));
    let took_it_back = match asked_since(&c, mark).as_slice() {
        [Request::TradeDeclineTrade(m)] => dereth_protocol::write_body(m)
            .expect("it encodes")
            .is_empty(),
        _ => false,
    };
    let offering_again = c.view().hud().panels.trade.button == ButtonState::Enabled;
    let nothing_ended =
        window_is_up(&mut c) && rows(&c, false) == vec![GEM] && rows(&c, true) == vec![THEIRS];

    c.assert_behaviour(
        "trade.controls.the-one-button-agrees-and-then-takes-it-back-without-ending-anything",
        move |_| {
            awake && agreed && showing_agreement && took_it_back && offering_again && nothing_ended
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.close.whatever-ends-a-trade-says-so-and-leaves-the-empty-window-up
// -----------------------------------------------------------------------------------------

/// Whatever reason the shard gives for a trade ending, the player is told the same sentence,
/// both halves of the table are emptied, the things that were on it stop being marked as
/// traded, the name across the table is blanked, the window stays on screen, and nothing goes
/// back out.
///
/// Four reasons are driven, because a client that answered only one of them would look
/// identical on a single arm.
pub fn whatever_ends_a_trade_says_so_and_leaves_the_empty_window_up() {
    let mut every_reason = true;
    let mut last: Option<HeadlessClient> = None;
    for reason in [0_u32, 1, 2, 5] {
        let mut c = an_open_negotiation();
        let _ = offer(&mut c, GEM);
        the_shard_adds(&mut c, GEM, MY_SIDE);
        the_shard_adds(&mut c, THEIRS, THEIR_SIDE);
        assert!(
            c.view().world().weenie(GEM).expect("the gem").trade_state != 0,
            "the premise: the gem is marked as being traded"
        );

        let mark = c.outbound().len();
        c.when(Inbound::message(&dereth_protocol::trade::TradeCloseTrade {
            reason,
        }))
        .tick(3);

        let told = said(&c)
            .iter()
            .any(|l| l == "The trade has been cancelled.");
        let emptied = rows(&c, false).is_empty() && rows(&c, true).is_empty();
        let unmarked = c.view().world().weenie(GEM).expect("the gem").trade_state == 0;
        let unnamed = c.view().hud().panels.trade.partner_name_text.is_empty();
        let still_up = window_is_up(&mut c);
        let nothing_to_accept = c.view().hud().panels.trade.button == ButtonState::Disabled;
        let silent = asked_since(&c, mark).is_empty();

        every_reason &=
            told && emptied && unmarked && unnamed && still_up && nothing_to_accept && silent;
        if let Some(previous) = last.take() {
            previous.shutdown();
        }
        last = Some(c);
    }
    let mut c = last.expect("four reasons were driven");
    c.assert_behaviour(
        "trade.close.whatever-ends-a-trade-says-so-and-leaves-the-empty-window-up",
        move |_| every_reason,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.open.a-combat-stance-refuses-the-request-in-the-clients-own-words
// -----------------------------------------------------------------------------------------

/// Asking another player to trade while standing in a combat stance is refused in words and
/// asks nobody anything; the same gesture in peace mode opens the negotiation, so the refusal
/// is a refusal and not a path that never worked.
///
/// The two halves are two clients because the client refuses a second use of anything within
/// a fifth of a second of the first, which is its own behaviour and not this claim's.
pub fn a_combat_stance_refuses_the_request_in_the_clients_own_words() {
    let mut c = a_client_with_things_to_trade();
    c.world_mut().combat.combat_mode = dereth_client_model::combat::CombatMode::Melee;
    let mark = c.outbound().len();
    c.when(Player::DoubleClick(PARTNER)).tick(3);
    let asked_nobody = !asked_since(&c, mark)
        .iter()
        .any(|r| matches!(r, Request::OpenTradeNegotiations(_)));
    let refused_in_words = said(&c)
        .iter()
        .any(|l| l == "You need to be in peace mode to trade.");
    c.shutdown();

    let mut c = a_client_with_things_to_trade();
    let at_peace =
        c.view().world().combat.combat_mode == dereth_client_model::combat::CombatMode::NonCombat;
    let mark = c.outbound().len();
    c.when(Player::DoubleClick(PARTNER)).tick(3);
    let asked = asked_since(&c, mark)
        .iter()
        .any(|r| matches!(r, Request::OpenTradeNegotiations(m) if m.partner == PARTNER));

    c.assert_behaviour(
        "trade.open.a-combat-stance-refuses-the-request-in-the-clients-own-words",
        move |_| asked_nobody && refused_in_words && at_peace && asked,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.table.an-item-waiting-that-needs-splitting-is-refused-in-the-clients-own-words
// -----------------------------------------------------------------------------------------

/// An item dropped on another player and waiting for them to answer is refused in words, and
/// never reaches the table, when it is the stack the player has part of in hand.
pub fn an_item_waiting_that_needs_splitting_is_refused_in_the_clients_own_words() {
    let mut c = a_client_with_things_to_trade();
    c.world_mut().selected = Some(GEM);
    // The screen's own selection handler re-seeds the slider, so let it settle before the
    // part-stack is asked for; otherwise it would be overwritten by a whole stack.
    c.tick(4);
    c.when(Player::ui(
        dereth_client_contract::UiRequest::StackSliderChanged { split: 1, max: 5 },
    ))
    .tick(1);
    let part_of_a_stack_in_hand = !c.view().world().split.is_whole_stack();
    park_for_trade(&mut c, GEM);

    let mark = c.outbound().len();
    the_shard_registers_the_trade(&mut c);
    unwatch_the_partner(&mut c);

    let offered_nothing = !asked_since(&c, mark)
        .iter()
        .any(|r| matches!(r, Request::TradeAddToTrade(_)));
    let refused_in_words = said(&c)
        .iter()
        .any(|l| l == "You must split the stack before trading it.");
    let table_is_empty = rows(&c, false).is_empty();

    c.assert_behaviour(
        "trade.table.an-item-waiting-that-needs-splitting-is-refused-in-the-clients-own-words",
        move |_| part_of_a_stack_in_hand && offered_nothing && refused_in_words && table_is_empty,
    );
    c.shutdown();
}

/// The state a drag of `item` on to another player leaves behind while it waits for them to
/// answer: the client remembers who and what, and nothing else.
fn park_for_trade(c: &mut HeadlessClient, item: ObjectId) {
    let w = c.world_mut();
    w.trade.attempt_to_player = PARTNER;
    w.trade.attempt_object = item;
}

// -----------------------------------------------------------------------------------------
// trade.table.an-item-waiting-for-a-partner-reaches-the-table-when-the-negotiation-opens
// -----------------------------------------------------------------------------------------

/// An item dropped on another player goes on to the table by itself the moment they accept
/// the window, both on the wire and on screen -- and it is only offered once.
pub fn an_item_waiting_for_a_partner_reaches_the_table_when_the_negotiation_opens() {
    let mut c = a_client_with_things_to_trade();
    park_for_trade(&mut c, GEM);

    let mark = c.outbound().len();
    the_shard_registers_the_trade(&mut c);
    unwatch_the_partner(&mut c);
    c.tick(3);

    let offered = added(&asked_since(&c, mark))
        .iter()
        .any(|(item, _)| *item == GEM);
    let drawn = rows(&c, false) == vec![GEM];
    let once = c.view().interaction().stats.trade_for_dummies_offered == 1;
    let forgotten = c.view().world().trade.attempt_object == ObjectId(0);

    c.assert_behaviour(
        "trade.table.an-item-waiting-for-a-partner-reaches-the-table-when-the-negotiation-opens",
        move |_| offered && drawn && once && forgotten,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.table.a-pack-put-on-the-table-offers-what-is-inside-it-and-says-so
// -----------------------------------------------------------------------------------------

/// Letting a pack with things in it go on the trade table offers the things inside it, one by
/// one and in the pack's own order, tells the player that is what is happening, and does not
/// put the pack itself on the table.
pub fn a_pack_put_on_the_table_offers_what_is_inside_it_and_says_so() {
    let mut c = an_open_negotiation();

    let sent = offer(&mut c, PACK);
    let the_contents = added(&sent) == vec![(IN_PACK_A, 0), (IN_PACK_B, 0)];
    let not_the_pack = !added(&sent).iter().any(|(item, _)| *item == PACK);
    let told = said(&c)
        .iter()
        .any(|l| l == "Trading contents of Belt Pouch");

    c.assert_behaviour(
        "trade.table.a-pack-put-on-the-table-offers-what-is-inside-it-and-says-so",
        move |_| the_contents && not_the_pack && told,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.table.what-is-on-the-table-can-be-picked-and-examined-but-not-used
// -----------------------------------------------------------------------------------------

/// A row on either half of the trade table can be picked and can be examined, and cannot be
/// used: neither an ordinary press nor a double click on one asks the shard for anything at
/// all, and a right click asks about that very thing.
pub fn what_is_on_the_table_can_be_picked_and_examined_but_not_used() {
    let mut c = an_open_negotiation();
    c.world_mut()
        .object_range_checks
        .unregister_all(dereth_client_model::range::RangeHandler::SecureTrade);
    the_shard_adds(&mut c, GEM, MY_SIDE);
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);

    let mut every_row = true;
    for (n, (item, partner_side)) in [(GEM, false), (THEIRS, true)].into_iter().enumerate() {
        let row = index_of(&c, partner_side, item);
        let at = tile(&mut c, partner_side, row);
        let list = if partner_side {
            trade::OTHER_LIST
        } else {
            trade::SELF_LIST
        };
        every_row &= points_into(&mut c, at, list);
        let Target::Point(p) = at else {
            unreachable!("a tile is a point")
        };
        for (k, action) in [
            dereth_ui::focus::action::PRIMARY_CLICK,
            dereth_ui::focus::action::SECONDARY_CLICK,
            DOUBLE_CLICK,
        ]
        .into_iter()
        .enumerate()
        {
            let mark = c.outbound().len();
            press_and_release(&mut c, (p.x, p.y), action, 40.0 + (n * 8 + k) as f64);
            every_row &= c.view().world().selected == Some(item);
            let sent = asked_since(&c, mark);
            every_row &= if action == dereth_ui::focus::action::SECONDARY_CLICK {
                matches!(sent.as_slice(), [Request::Appraise(m)] if m.target == item)
            } else {
                sent.is_empty()
            };
        }
    }

    c.assert_behaviour(
        "trade.table.what-is-on-the-table-can-be-picked-and-examined-but-not-used",
        move |_| every_row,
    );
    c.shutdown();
}

/// The action a second click inside the double-click window arrives as.
const DOUBLE_CLICK: u32 = 10;

/// Which row of a list `item` is drawn in.
fn index_of(c: &HeadlessClient, partner_side: bool, item: ObjectId) -> usize {
    let p = &c.view().hud().panels.trade;
    let w = if partner_side {
        p.other_list.as_ref()
    } else {
        p.self_list.as_ref()
    };
    w.expect("both lists are bound")
        .slots
        .iter()
        .position(|s| s.item == Some(item))
        .unwrap_or_else(|| panic!("{item:?} is not drawn on that half of the table"))
}

/// One press and release of `action` at a screen point.
fn press_and_release(c: &mut HeadlessClient, at: (i32, i32), action: u32, t: f64) {
    {
        let (ui, _s) = super::gameplay_screen(c.app_mut());
        ui.mouse_move(dereth_primitives::LocalTime(t), at.0, at.1);
        ui.mouse_down(action, at.0, at.1);
        ui.mouse_up(action, at.0, at.1, false);
    }
    c.tick(3);
}

// -----------------------------------------------------------------------------------------
// trade.drag-hint.something-already-on-the-table-is-shown-as-refused
// -----------------------------------------------------------------------------------------

/// The same thing, the same carry, a different answer: once it is on the table, carrying it
/// over the table again is shown as refused, and letting go there really does nothing.
pub fn something_already_on_the_table_is_shown_as_refused() {
    let mut c = an_open_negotiation();

    let slot = first_empty(&c, false);
    let from = in_the_pack(&mut c, GEM);
    let to = tile(&mut c, false, slot);
    c.when(Player::Grab(from)).when(Player::Over(to));
    let green_first = hint(&mut c, false, slot).0 == Some(drag_accept_state::ACCEPT);
    c.when(Player::Drop(to));
    let on_the_table = rows(&c, false) == vec![GEM];

    let slot = first_empty(&c, false);
    let from = in_the_pack(&mut c, GEM);
    let to = tile(&mut c, false, slot);
    let mark = c.outbound().len();
    c.when(Player::Grab(from)).when(Player::Over(to));
    let refused = hint(&mut c, false, slot)
        == (
            Some(drag_accept_state::REFUSE),
            Some(drag_accept_state::REFUSE),
        );
    c.when(Player::Drop(to));
    let nothing_asked = asked_since(&c, mark).is_empty();
    let still_one_row = rows(&c, false) == vec![GEM];

    c.assert_behaviour(
        "trade.drag-hint.something-already-on-the-table-is-shown-as-refused",
        { move |_| green_first && on_the_table && refused && nothing_asked && still_one_row },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.drag-hint.something-you-no-longer-carry-is-refused-without-a-word-until-you-let-go
// -----------------------------------------------------------------------------------------

/// Something that has stopped being yours in the middle of the carry is shown as refused and
/// **nothing is said**; only letting go of it says why.
pub fn something_you_no_longer_carry_is_refused_without_a_word() {
    let mut c = an_open_negotiation();

    let slot = first_empty(&c, false);
    let from = in_the_pack(&mut c, COIN);
    let to = tile(&mut c, false, slot);
    c.when(Player::Grab(from));
    let said_before = said(&c).len();
    // The shard moves it into the partner's hands while the icon is in the air, which is the
    // one way a live carry can be holding something that is no longer the player's.
    c.world_mut()
        .tables
        .weenies
        .get_mut(COIN)
        .expect("the stack")
        .pwd
        .container_id = Some(PARTNER);

    let mark = c.outbound().len();
    c.when(Player::Over(to));
    let refused = hint(&mut c, false, slot)
        == (
            Some(drag_accept_state::REFUSE),
            Some(drag_accept_state::REFUSE),
        );
    let silent = said(&c).len() == said_before;

    c.when(Player::Drop(to));
    let nothing_asked = asked_since(&c, mark).is_empty();
    let and_now_it_says_why = said(&c)
        .iter()
        .any(|l| l.contains("only trade items you are carrying"));

    c.assert_behaviour(
        "trade.drag-hint.something-you-no-longer-carry-is-refused-without-a-word-until-you-let-go",
        move |_| refused && silent && nothing_asked && and_now_it_says_why,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.drag-hint.the-half-you-cannot-drop-on-answers-nothing-and-swallows-the-drop
// -----------------------------------------------------------------------------------------

/// The partner's half of the table answers a carried icon with nothing at all -- it never
/// lights green over a drop it is going to throw away -- and letting go there does nothing
/// and says nothing.
///
/// The scenario is arranged to be able to fail in both directions: a place that was never
/// answered looks the same as one a client with no answer at all would leave, so a real drop
/// is made first to move it to a known state, and the carry after that must leave it exactly
/// there.
pub fn the_half_you_cannot_drop_on_answers_nothing() {
    let mut c = an_open_negotiation();
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);
    let both_refuse_a_hint_of_their_own = {
        let p = &c.view().hud().panels.trade;
        let flags = |w: &dereth_ui_screens::items::widget::ItemListWidget| {
            (
                w.container_list,
                w.vendor_list,
                w.shortcut_list,
                w.salvage_list,
            )
        };
        let mine = flags(p.self_list.as_ref().expect("your own half"));
        let theirs = flags(p.other_list.as_ref().expect("the partner's half"));
        mine == (false, true, false, false) && theirs == (false, true, false, false)
    };

    let slot = first_empty(&c, true);
    let from = in_the_pack(&mut c, GEM);
    let to = tile(&mut c, true, slot);
    let lands_there = points_into(&mut c, to, trade::OTHER_LIST);
    c.when(Player::Grab(from));
    let said_before = said(&c).len();
    let mark = c.outbound().len();
    c.when(Player::Over(to));
    let never_answered = hint(&mut c, true, slot).0.is_none();

    c.when(Player::Drop(to));
    let nothing_asked = asked_since(&c, mark).is_empty();
    let nothing_reached_your_half = rows(&c, false).is_empty();
    let silent = said(&c).len() == said_before;
    let cleared =
        hint(&mut c, true, slot) == (Some(drag_accept_state::NONE), Some(drag_accept_state::NONE));

    // The second carry, from a known state.
    let from = in_the_pack(&mut c, COIN);
    let to = tile(&mut c, true, slot);
    c.when(Player::Grab(from)).when(Player::Over(to));
    let still_nothing =
        hint(&mut c, true, slot) == (Some(drag_accept_state::NONE), Some(drag_accept_state::NONE));
    c.when(Player::Release);

    c.assert_behaviour(
        "trade.drag-hint.the-half-you-cannot-drop-on-answers-nothing-and-swallows-the-drop",
        move |_| {
            both_refuse_a_hint_of_their_own
                && lands_there
                && never_answered
                && nothing_asked
                && nothing_reached_your_half
                && silent
                && cleared
                && still_nothing
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.drag-hint.the-answer-comes-down-however-the-carry-ends
// -----------------------------------------------------------------------------------------

/// However a carry over the trade table ends -- letting go on it, carrying off it on to the
/// window's own furniture, or carrying off the window altogether -- the place stops claiming
/// it would take the drop.
pub fn the_answer_comes_down_however_the_carry_ends() {
    let mut c = an_open_negotiation();

    // 1. letting go on it.
    let slot = first_empty(&c, false);
    let from = in_the_pack(&mut c, GEM);
    let to = tile(&mut c, false, slot);
    c.when(Player::Grab(from)).when(Player::Over(to));
    let up_first = hint(&mut c, false, slot).0 == Some(drag_accept_state::ACCEPT);
    c.when(Player::Drop(to));
    let down_on_the_drop =
        hint(&mut c, false, slot) == (Some(drag_accept_state::NONE), Some(drag_accept_state::NONE));

    // 2. carrying off it on to the window's own furniture -- the partner's name.
    let slot = first_empty(&c, false);
    let from = in_the_pack(&mut c, COIN);
    let to = tile(&mut c, false, slot);
    c.when(Player::Grab(from)).when(Player::Over(to));
    let up_again = hint(&mut c, false, slot).0 == Some(drag_accept_state::ACCEPT);
    let furniture = {
        let h = c
            .view()
            .hud()
            .panels
            .trade
            .other_name
            .expect("the partner's name");
        let (ui, _s) = super::gameplay_screen(c.app_mut());
        let (x, y) = super::centre(ui, h);
        Target::Point(ScreenPoint::new(x, y))
    };
    let mark = c.outbound().len();
    c.when(Player::Over(furniture));
    let down_on_the_furniture =
        hint(&mut c, false, slot) == (Some(drag_accept_state::NONE), Some(drag_accept_state::NONE));
    c.when(Player::Drop(furniture));
    let the_furniture_takes_nothing = asked_since(&c, mark).is_empty();

    // 3. carrying off the window altogether -- back on to the pack.
    let slot = first_empty(&c, false);
    let from = in_the_pack(&mut c, COIN);
    let to = tile(&mut c, false, slot);
    c.when(Player::Grab(from)).when(Player::Over(to));
    let up_a_third_time = hint(&mut c, false, slot).0 == Some(drag_accept_state::ACCEPT);
    let away = in_the_pack(&mut c, PACK);
    c.when(Player::Over(away));
    let down_off_the_window =
        hint(&mut c, false, slot) == (Some(drag_accept_state::NONE), Some(drag_accept_state::NONE));
    c.when(Player::Release);

    c.assert_behaviour(
        "trade.drag-hint.the-answer-comes-down-however-the-carry-ends",
        {
            move |_| {
                up_first
                    && down_on_the_drop
                    && up_again
                    && down_on_the_furniture
                    && the_furniture_takes_nothing
                    && up_a_third_time
                    && down_off_the_window
            }
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.drag-hint.a-shortcut-carried-over-the-table-is-given-no-answer-at-all
// -----------------------------------------------------------------------------------------

/// A tile dragged off the bar of shortcuts and carried over the trade table gets no answer
/// from it at all: the place keeps whatever it was showing rather than turning red.
///
/// A real drop is made first so the place is in a known state, which is what lets this fail
/// in both directions.
pub fn a_shortcut_carried_over_the_table_is_given_no_answer_at_all() {
    let mut c = an_open_negotiation();

    let slot = first_empty(&c, false);
    let from = in_the_pack(&mut c, GEM);
    let to = tile(&mut c, false, slot);
    c.when(Player::Grab(from))
        .when(Player::Over(to))
        .when(Player::Drop(to));
    let before = hint(&mut c, false, slot);
    let known_state = before == (Some(drag_accept_state::NONE), Some(drag_accept_state::NONE));

    c.world_mut()
        .player_system
        .add_shortcut(dereth_protocol::login::ShortCutData {
            index: 0,
            object_id: COIN,
            spell_id: 0,
        });
    c.tick(2);
    let from = {
        let (ui, screen) = super::gameplay_screen(c.app_mut());
        let h = screen
            .shortcuts
            .slots
            .first()
            .expect("the bar has a tile")
            .handle;
        let (x, y) = super::centre(ui, h);
        Target::Point(ScreenPoint::new(x, y))
    };
    let to = tile(&mut c, false, slot);
    c.when(Player::Grab(from)).when(Player::Over(to));
    let unchanged = hint(&mut c, false, slot) == before;
    c.when(Player::Release);

    c.assert_behaviour(
        "trade.drag-hint.a-shortcut-carried-over-the-table-is-given-no-answer-at-all",
        move |_| known_state && unchanged,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.close.the-partner-walking-away-ends-it-and-leaves-the-window-on-screen
// -----------------------------------------------------------------------------------------

/// The partner walking more than five metres away ends the negotiation with the shard and
/// leaves the window where it is -- emptied, but still naming them, because nothing has yet
/// said they are gone.
pub fn the_partner_walking_away_ends_it_and_leaves_the_window_on_screen() {
    let mut c = a_client_with_things_to_trade();
    the_shard_registers_the_trade(&mut c);
    assert!(
        c.view().hud().panels.trade.visible,
        "the premise: the window is up"
    );
    let _ = offer(&mut c, GEM);
    the_shard_adds(&mut c, GEM, MY_SIDE);
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: ME },
    ))
    .when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: PARTNER },
    ))
    .tick(3);

    let armed = c
        .view()
        .world()
        .object_range_checks
        .live()
        .find(|e| e.handler == dereth_client_model::range::RangeHandler::SecureTrade)
        .copied()
        .expect("registering a trade watches how far away the partner is");
    let watching_the_partner = armed.object == PARTNER && (armed.range - 5.0).abs() < 1e-6;

    let mut where_they_are = Distances(std::collections::BTreeMap::new());
    where_they_are.0.insert(PARTNER.0, 5.1);
    let mut out = dereth_client_model::RecordingSink::default();
    let mut req = dereth_client_model::RecordingRequests::default();
    let stats = c.world_mut().calculate_object_range_checks(
        dereth_primitives::ServerTime(30.0),
        &where_they_are,
        75.0,
        &mut out,
        &mut req,
    );
    c.tick(3);

    let one_leaving = stats.exits == 1;
    let ended_with_the_shard = req
        .0
        .iter()
        .any(|r| matches!(r, Request::TradeCloseTradeNegotiations(_)));
    let still_up = window_is_up(&mut c);
    let emptied_and_still_named = rows(&c, false).is_empty()
        && rows(&c, true).is_empty()
        && c.view().hud().panels.trade.partner_name_text == "Jorune";
    let the_record_survives = {
        let t = &c.view().world().trade;
        t.trade.self_list.iter().map(|p| p.iid).collect::<Vec<_>>() == vec![GEM]
            && t.trade
                .partner_list
                .iter()
                .map(|p| p.iid)
                .collect::<Vec<_>>()
                == vec![THEIRS]
            && t.trade.both_accepted()
            && t.display_lists == Some([Vec::new(), Vec::new()])
            && t.acceptance_darkened
    };
    let nothing_to_accept = c.view().hud().panels.trade.button == ButtonState::Disabled;

    c.assert_behaviour(
        "trade.close.the-partner-walking-away-ends-it-and-leaves-the-window-on-screen",
        move |_| {
            watching_the_partner
                && one_leaving
                && ended_with_the_shard
                && still_up
                && emptied_and_still_named
                && the_record_survives
                && nothing_to_accept
        },
    );
    c.shutdown();
}

/// How far away each object is, for the one scenario whose subject is the distance.
struct Distances(std::collections::BTreeMap<u32, f32>);

impl dereth_client_model::range::ObjectRangeGeometry for Distances {
    fn distance(&self, object: ObjectId, _player: ObjectId, _r: bool, _z: bool) -> Option<f32> {
        self.0.get(&object.0).copied()
    }
}

// -----------------------------------------------------------------------------------------
// trade.close.clearing-every-item-blanks-the-table-without-ending-the-negotiation
// -----------------------------------------------------------------------------------------

/// The window's "clear all" button asks for the table to be blanked and nothing else: the
/// window stays on screen and the person across it is still the person across it.
pub fn clearing_every_item_blanks_the_table_without_ending_the_negotiation() {
    let mut c = an_open_negotiation();
    let _ = offer(&mut c, GEM);
    the_shard_adds(&mut c, GEM, MY_SIDE);
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);

    let mark = c.outbound().len();
    c.when(Player::click(trade::BTN_CLEAR_ALL));

    let asked_to_clear = match asked_since(&c, mark).as_slice() {
        [Request::TradeResetTrade(m)] => dereth_protocol::write_body(m)
            .expect("it encodes")
            .is_empty(),
        _ => false,
    };
    let still_up = window_is_up(&mut c);
    let still_the_same_partner = c.view().world().trade.partner == PARTNER;

    c.assert_behaviour(
        "trade.close.clearing-every-item-blanks-the-table-without-ending-the-negotiation",
        move |_| asked_to_clear && still_up && still_the_same_partner,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.close.a-completed-trade-empties-the-window-and-leaves-it-up
// -----------------------------------------------------------------------------------------

/// A trade both sides agree to finishes without ending the window: the shard swaps the goods
/// and blanks both tables, the window stays on screen still naming the partner, and the
/// client says nothing more about it.
pub fn a_completed_trade_empties_the_window_and_leaves_it_up() {
    let mut c = an_open_negotiation();
    let _ = offer(&mut c, GEM);
    the_shard_adds(&mut c, GEM, MY_SIDE);
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);

    let mark = c.outbound().len();
    c.when(Player::click(trade::BTN_TRADE));
    let agreed = matches!(
        asked_since(&c, mark).as_slice(),
        [Request::TradeAcceptTrade(_)]
    );

    c.when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: ME },
    ))
    .when(Inbound::message(
        &dereth_protocol::trade::TradeAcceptTradeRecv { source: PARTNER },
    ))
    .tick(3);
    let mark = c.outbound().len();
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeResetTradeRecv { source: PARTNER },
    ))
    .tick(3);

    let nothing_more = asked_since(&c, mark).is_empty();
    let still_up = window_is_up(&mut c);
    let emptied_and_still_named = rows(&c, false).is_empty()
        && rows(&c, true).is_empty()
        && c.view().hud().panels.trade.partner_name_text == "Jorune";
    let still_the_same_partner = c.view().world().trade.partner == PARTNER;

    c.assert_behaviour(
        "trade.close.a-completed-trade-empties-the-window-and-leaves-it-up",
        move |_| {
            agreed && nothing_more && still_up && emptied_and_still_named && still_the_same_partner
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.close.logging-off-tells-the-shard-nothing-about-the-trade
// -----------------------------------------------------------------------------------------

/// Leaving the game in the middle of a trade sends nothing at all about it: the shard learns
/// the trade is over from the player leaving, and the client forgets who it was trading with.
pub fn logging_off_tells_the_shard_nothing_about_the_trade() {
    let mut c = an_open_negotiation();
    let _ = offer(&mut c, GEM);
    the_shard_adds(&mut c, GEM, MY_SIDE);
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);

    let mark = c.outbound().len();
    c.world_mut().trade.end_character_session();
    c.tick(3);

    let said_nothing = !asked_since(&c, mark)
        .iter()
        .any(|r| matches!(r, Request::TradeCloseTradeNegotiations(_)));
    let forgotten = c.view().world().trade.partner == ObjectId(0);

    c.assert_behaviour(
        "trade.close.logging-off-tells-the-shard-nothing-about-the-trade",
        move |_| said_nothing && forgotten,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// trade.close.no-gesture-but-the-close-button-ends-the-negotiation
// -----------------------------------------------------------------------------------------

/// Clearing the table, offering something, agreeing, taking the agreement back and the shard
/// blanking both sides are five things that are not the end of a negotiation: none of them
/// ends it and none of them takes the window down. The same negotiation's close button then
/// does both.
///
/// Each of those gestures asks the shard for its own thing, which is what says the instrument
/// can see at all.
pub fn no_gesture_but_the_close_button_ends_the_negotiation() {
    let mut c = an_open_negotiation();
    let _ = offer(&mut c, GEM);
    the_shard_adds(&mut c, GEM, MY_SIDE);
    the_shard_adds(&mut c, THEIRS, THEIR_SIDE);

    let mark = c.outbound().len();
    let mut stayed_up = true;

    c.when(Player::click(trade::BTN_CLEAR_ALL));
    stayed_up &= window_is_up(&mut c);
    let _ = offer(&mut c, COIN);
    the_shard_adds(&mut c, COIN, MY_SIDE);
    stayed_up &= window_is_up(&mut c);
    c.when(Player::click(trade::BTN_TRADE));
    stayed_up &= window_is_up(&mut c);
    c.when(Player::click(trade::BTN_TRADE));
    stayed_up &= window_is_up(&mut c);
    c.when(Inbound::message(
        &dereth_protocol::trade::TradeResetTradeRecv { source: PARTNER },
    ))
    .tick(3);
    stayed_up &= window_is_up(&mut c);

    let seen = asked_since(&c, mark);
    let nothing_ended_it = !seen
        .iter()
        .any(|r| matches!(r, Request::TradeCloseTradeNegotiations(_)));
    let the_instrument_can_see = seen
        .iter()
        .any(|r| matches!(r, Request::TradeResetTrade(_)))
        && seen
            .iter()
            .any(|r| matches!(r, Request::TradeAcceptTrade(_)))
        && seen
            .iter()
            .any(|r| matches!(r, Request::TradeDeclineTrade(_)))
        && seen
            .iter()
            .any(|r| matches!(r, Request::TradeAddToTrade(_)));

    let mark = c.outbound().len();
    c.when(Player::click(trade::BTN_CLOSE));
    let the_close_button_does = matches!(
        asked_since(&c, mark).as_slice(),
        [Request::TradeCloseTradeNegotiations(_)]
    );
    let and_takes_it_down = !window_is_up(&mut c);

    c.assert_behaviour(
        "trade.close.no-gesture-but-the-close-button-ends-the-negotiation",
        move |_| {
            stayed_up
                && nothing_ended_it
                && the_instrument_can_see
                && the_close_button_does
                && and_takes_it_down
        },
    );
    c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_the_registration_raises_the_window_and_names_the_partner => the_registration_raises_the_window_and_names_the_partner ["trade.window.the-shards-registration-raises-it-and-names-the-partner"],
    scenario_a_real_drag_offers_the_item_and_draws_it_at_once => a_real_drag_offers_the_item_and_draws_it_at_once ["trade.table.a-real-drag-from-the-pack-offers-the-item-and-draws-it-at-once"],
    scenario_your_own_half_lights_for_something_you_are_carrying => your_own_half_lights_for_something_you_are_carrying ["trade.drag-hint.your-own-half-lights-for-something-you-are-carrying"],
    scenario_the_close_button_is_the_only_gesture_that_takes_the_window_down => the_close_button_is_the_only_gesture_that_takes_the_window_down ["trade.close.the-close-button-is-the-only-gesture-that-takes-the-window-down"],
    scenario_part_of_a_stack_is_split_first_and_only_the_shards_stack_is_offered => part_of_a_stack_is_split_first_and_only_the_shards_stack_is_offered ["trade.table.part-of-a-stack-is-split-first-and-only-the-shards-own-new-stack-is-offered"],
    scenario_anything_either_side_changes_puts_both_agreements_out => anything_either_side_changes_puts_both_agreements_out ["trade.table.anything-either-side-changes-puts-both-agreements-out"],
    scenario_the_shard_voiding_both_agreements_empties_the_window_without_closing_it => the_shard_voiding_both_agreements_empties_the_window_without_closing_it ["trade.table.the-shard-voiding-both-agreements-empties-the-window-without-closing-it"],
    scenario_the_partners_container_keeps_its_contents_alive_only_while_shown => the_partners_container_keeps_its_contents_alive_only_while_shown ["trade.table.the-partners-container-keeps-what-is-inside-it-alive-only-while-it-is-on-show"],
    scenario_the_one_button_agrees_and_then_takes_it_back => the_one_button_agrees_and_then_takes_it_back ["trade.controls.the-one-button-agrees-and-then-takes-it-back-without-ending-anything"],
    scenario_whatever_ends_a_trade_says_so_and_leaves_the_empty_window_up => whatever_ends_a_trade_says_so_and_leaves_the_empty_window_up ["trade.close.whatever-ends-a-trade-says-so-and-leaves-the-empty-window-up"],
    scenario_a_combat_stance_refuses_the_request_in_the_clients_own_words => a_combat_stance_refuses_the_request_in_the_clients_own_words ["trade.open.a-combat-stance-refuses-the-request-in-the-clients-own-words"],
    scenario_an_item_waiting_that_needs_splitting_is_refused_in_the_clients_own_words => an_item_waiting_that_needs_splitting_is_refused_in_the_clients_own_words ["trade.table.an-item-waiting-that-needs-splitting-is-refused-in-the-clients-own-words"],
    scenario_an_item_waiting_for_a_partner_reaches_the_table_when_the_negotiation_opens => an_item_waiting_for_a_partner_reaches_the_table_when_the_negotiation_opens ["trade.table.an-item-waiting-for-a-partner-reaches-the-table-when-the-negotiation-opens"],
    scenario_a_pack_put_on_the_table_offers_what_is_inside_it_and_says_so => a_pack_put_on_the_table_offers_what_is_inside_it_and_says_so ["trade.table.a-pack-put-on-the-table-offers-what-is-inside-it-and-says-so"],
    scenario_what_is_on_the_table_can_be_picked_and_examined_but_not_used => what_is_on_the_table_can_be_picked_and_examined_but_not_used ["trade.table.what-is-on-the-table-can-be-picked-and-examined-but-not-used"],
    scenario_something_already_on_the_table_is_shown_as_refused => something_already_on_the_table_is_shown_as_refused ["trade.drag-hint.something-already-on-the-table-is-shown-as-refused"],
    scenario_something_you_no_longer_carry_is_refused_without_a_word => something_you_no_longer_carry_is_refused_without_a_word ["trade.drag-hint.something-you-no-longer-carry-is-refused-without-a-word-until-you-let-go"],
    scenario_the_half_you_cannot_drop_on_answers_nothing => the_half_you_cannot_drop_on_answers_nothing ["trade.drag-hint.the-half-you-cannot-drop-on-answers-nothing-and-swallows-the-drop"],
    scenario_the_answer_comes_down_however_the_carry_ends => the_answer_comes_down_however_the_carry_ends ["trade.drag-hint.the-answer-comes-down-however-the-carry-ends"],
    scenario_a_shortcut_carried_over_the_table_is_given_no_answer_at_all => a_shortcut_carried_over_the_table_is_given_no_answer_at_all ["trade.drag-hint.a-shortcut-carried-over-the-table-is-given-no-answer-at-all"],
    scenario_the_partner_walking_away_ends_it_and_leaves_the_window_on_screen => the_partner_walking_away_ends_it_and_leaves_the_window_on_screen ["trade.close.the-partner-walking-away-ends-it-and-leaves-the-window-on-screen"],
    scenario_clearing_every_item_blanks_the_table_without_ending_the_negotiation => clearing_every_item_blanks_the_table_without_ending_the_negotiation ["trade.close.clearing-every-item-blanks-the-table-without-ending-the-negotiation"],
    scenario_a_completed_trade_empties_the_window_and_leaves_it_up => a_completed_trade_empties_the_window_and_leaves_it_up ["trade.close.a-completed-trade-empties-the-window-and-leaves-it-up"],
    scenario_logging_off_tells_the_shard_nothing_about_the_trade => logging_off_tells_the_shard_nothing_about_the_trade ["trade.close.logging-off-tells-the-shard-nothing-about-the-trade"],
    scenario_no_gesture_but_the_close_button_ends_the_negotiation => no_gesture_but_the_close_button_ends_the_negotiation ["trade.close.no-gesture-but-the-close-button-ends-the-negotiation"],
}
