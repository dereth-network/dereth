use super::*;

// ---------------------------------------------------------------------------------------------
// f54: the shop's own latch.
// ---------------------------------------------------------------------------------------------

/// The recorded shopkeeper and the recorded player.
pub(super) const SHOPKEEPER: ObjectId = ObjectId(0x77F0_3064);
pub(super) const RECORDED_PLAYER: ObjectId = ObjectId(0x5000_000A);
/// The recording's own shop-opening message.
const SHOP_OPEN_AT: usize = 4736;
/// The shard's reply to the purchase, and to the sale.
const AFTER_THE_BUY_AT: usize = 4748;
const AFTER_THE_SALE_AT: usize = 4784;
/// The shop's buy button, its sell list and its sell-everything button.
const BTN_BUY: ElementId = ElementId(0x1000_00C2);
pub(super) const SELL_LIST: ElementId = ElementId(0x1000_00CE);
pub(super) const BTN_SELL_ALL: ElementId = ElementId(0x1000_00D3);
/// The page the backpack grid lives on, raised so that it holds live slots.
pub(super) const INVENTORY_PAGE: ElementId = ElementId(0x1000_018B);

/// The recorded session, with the shop open and the backpack page up.
pub(super) fn a_client_at_the_shop() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // The identity is chosen before the description arrives, and a replay with no login has no
    // step that carries it across.
    c.world_mut().player = Some(RECORDED_PLAYER);
    c.when(Inbound::from_corpus("long-solo-play", 0..SHOP_OPEN_AT + 1))
        .tick(4);

    let panel = {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the UI shell");
        let any: &mut dyn std::any::Any = &mut **shell.flow.current_mut().expect("a screen");
        let gp = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");
        let panel = gp
            .panels
            .pages
            .iter()
            .find(|p| p.element == INVENTORY_PAGE)
            .expect("the shipped inventory page")
            .panel_id;
        gp.recv_set_panel_visibility(&mut shell.ui, panel, true);
        panel
    };
    let _ = panel;
    c.tick(4);

    assert_eq!(
        c.view().world().shop.vendor_id,
        Some(SHOPKEEPER),
        "the premise: the recorded message names the shopkeeper"
    );
    assert!(
        c.view().expect_app().hud().panels.vendor.bound(),
        "the premise: the shop panel found its own lists"
    );
    c
}

/// Everything on the strip across the top of the screen, which is where the refusal is drawn and
/// what the player reads.
pub(super) fn bubbles(c: &mut HeadlessClient) -> Vec<String> {
    let list = element_of(c, LIST_BOX);
    let shell = c.app_mut().ui_mut().expect("the UI shell");
    shell
        .ui
        .children(list)
        .into_iter()
        .filter_map(|h| {
            shell
                .ui
                .text_element_mut(h)
                .map(|t| t.glyphs.inq_text(false))
        })
        .collect()
}

/// The `(slot, object)` pairs the shop's own stock list is showing.
pub(super) fn stock_slots(c: &HeadlessClient) -> Vec<(ElemHandle, ObjectId)> {
    c.view()
        .expect_app()
        .hud()
        .panels
        .vendor
        .stock
        .as_ref()
        .expect("the shop panel binds its stock list")
        .slots
        .iter()
        .filter_map(|s| s.item.map(|id| (s.handle, id)))
        .collect()
}

/// A live backpack slot holding something this shop would take.
pub(super) fn sellable(c: &HeadlessClient) -> (ElemHandle, ObjectId) {
    let app = c.view().expect_app();
    let world = &app.objects().world;
    let any: &dyn std::any::Any = app
        .ui()
        .expect("the UI shell")
        .flow
        .current()
        .expect("a screen");
    any.downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen")
        .inventory
        .item_list
        .as_ref()
        .expect("the backpack grid is bound once the page is up")
        .slots
        .iter()
        .find_map(|s| {
            let id = s.item?;
            let w = world.weenie(id)?;
            (world.is_owned_by_player(id)
                && world.shop.profile.inq_acceptability(&w.pwd) == 0
                && world
                    .inventory(id)
                    .is_none_or(|i| i.items.is_empty() && i.containers.is_empty()))
            .then_some((s.handle, id))
        })
        .expect("the recorded pack shows at least one item this shop accepts")
}

/// The player's next action: a drag released on to their own pack, one of the gestures the
/// one-thing-at-a-time gate guards and the one that shows the sentence.
fn try_to_act(c: &mut HeadlessClient, item: ObjectId) -> Vec<Request> {
    c.app_mut().probe_mut().interaction_mut().queue(
        Vec::new(),
        vec![UiRequest::DragDrop {
            item,
            target: DropTarget::Container(RECORDED_PLAYER),
        }],
    );
    pump(c)
}

/// The bytes the one place a request becomes a datagram actually writes. Nothing is transmitted.
fn on_the_wire(r: &Request) -> Vec<u8> {
    let mut s = Session::new(MockTransport::new());
    assert!(
        dereth_client_runtime::requests::send_request(&mut s, r),
        "the sender has an arm for {r:?}"
    );
    s.transport.sent.last().expect("one blob").payload.clone()
}

pub(super) fn dword(b: &[u8], n: usize) -> u32 {
    u32::from_le_bytes([b[n], b[n + 1], b[n + 2], b[n + 3]])
}

/// Whether the move the player wants really reached the wire, and the refusal is not on screen.
fn the_move_reached_the_wire(sent: &[Request], item: ObjectId, strip: &[String]) -> bool {
    if strip.iter().any(|b| b == BUSY) {
        return false;
    }
    let Some(m) = sent.iter().find_map(|r| match r {
        Request::PutItemInContainer(m) if m.item == item => Some(*m),
        _ => None,
    }) else {
        return false;
    };
    // …and the same request, encoded. A gate that answered success after parking the request
    // would satisfy the match above and produce nothing here.
    let b = on_the_wire(&Request::PutItemInContainer(m));
    m.container == RECORDED_PLAYER
        && dword(&b, 0) == 0xF7B1
        && dword(&b, 8) == 0x0019
        && ObjectId(dword(&b, 12)) == item
}

/// Buying and letting the shard answer leaves the player able to act.
pub(super) fn a_purchase_leaves_the_player_able_to_act() {
    let mut c = a_client_at_the_shop();

    // The positive control: with nothing outstanding the same gesture goes through. Without it
    // every measurement below is about the wrong thing.
    let (_, item) = sellable(&c);
    let sent = try_to_act(&mut c, item);
    let strip = bubbles(&mut c);
    let control = the_move_reached_the_wire(&sent, item, &strip);
    assert!(
        control,
        "the premise: with nothing outstanding the player can act"
    );

    let mut c = a_client_at_the_shop();
    let (handle, stock) = *stock_slots(&c)
        .last()
        .expect("the live stock list holds a row");
    press(&mut c, handle);
    assert_eq!(
        c.view().world().selected,
        Some(stock),
        "the premise: the press picked the row"
    );

    let sent = click(&mut c, BTN_BUY);
    let buy = sent
        .iter()
        .find_map(|r| match r {
            Request::VendorBuy(m) if m.vendor_id == SHOPKEEPER => Some(m.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the premise: the buy button puts a purchase on the wire"));
    let b = on_the_wire(&Request::VendorBuy(buy));
    let went_out =
        dword(&b, 0) == 0xF7B1 && dword(&b, 8) == 0x005F && ObjectId(dword(&b, 12)) == SHOPKEEPER;

    // The shard answers, with the recording's own datagram.
    c.when(Inbound::from_corpus(
        "long-solo-play",
        SHOP_OPEN_AT + 1..AFTER_THE_BUY_AT + 1,
    ))
    .tick(4);
    assert_eq!(
        c.view().world().shop.vendor_id,
        Some(SHOPKEEPER),
        "the premise: it is the same shopkeeper re-advertising itself"
    );

    let (_, item) = sellable(&c);
    let sent = try_to_act(&mut c, item);
    let strip = bubbles(&mut c);
    let free = the_move_reached_the_wire(&sent, item, &strip);

    c.assert_behaviour("vendor.busy.a-purchase-leaves-the-player-able-to-act", {
        move |_| control && went_out && free
    });
    c.shutdown();
}

/// Selling and letting the shard answer leaves the player able to act.
pub(super) fn a_sale_leaves_the_player_able_to_act() {
    let mut c = a_client_at_the_shop();
    let (owner, offered) = sellable(&c);
    let target = element_of(&c, SELL_LIST);

    c.app_mut()
        .ui_mut()
        .expect("the UI shell")
        .ui
        .broadcast_element_message(
            target,
            dereth_ui::msg::element::id::DROP_FAILED,
            0,
            owner.raw(),
        );
    let _ = pump(&mut c);
    assert!(
        c.view()
            .world()
            .shop
            .sell_list
            .iter()
            .any(|(id, _)| *id == offered),
        "the premise: the item is in the sell basket"
    );

    let sent = click(&mut c, BTN_SELL_ALL);
    let sell = sent
        .iter()
        .find_map(|r| match r {
            Request::VendorSell(m) if m.vendor_id == SHOPKEEPER => Some(m.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the premise: sell-everything puts a sale on the wire"));
    let b = on_the_wire(&Request::VendorSell(sell));
    let went_out =
        dword(&b, 0) == 0xF7B1 && dword(&b, 8) == 0x0060 && ObjectId(dword(&b, 12)) == SHOPKEEPER;

    c.when(Inbound::from_corpus(
        "long-solo-play",
        SHOP_OPEN_AT + 1..AFTER_THE_SALE_AT + 1,
    ))
    .tick(4);
    assert_eq!(c.view().world().shop.vendor_id, Some(SHOPKEEPER));

    let (_, item) = sellable(&c);
    let sent = try_to_act(&mut c, item);
    let strip = bubbles(&mut c);
    let free = the_move_reached_the_wire(&sent, item, &strip);

    c.assert_behaviour(
        "vendor.busy.a-sale-leaves-the-player-able-to-act",
        move |_| went_out && free,
    );
    c.shutdown();
}

/// While the shop really has not answered, the refusal still fires -- and a different merchant's
/// reply does not count as the answer.
///
/// **This is the point of the pair above**: a change that simply deleted the gate would pass both
/// of them and fail here.
pub(super) fn while_the_shop_has_not_answered_the_refusal_still_fires() {
    let mut c = a_client_at_the_shop();
    let (handle, _) = *stock_slots(&c).last().expect("a filled row");
    press(&mut c, handle);
    assert!(
        !click(&mut c, BTN_BUY).is_empty(),
        "the premise: the buy went out"
    );

    let (_, item) = sellable(&c);
    let sent = try_to_act(&mut c, item);
    let refused = !sent
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(_)))
        && bubbles(&mut c).iter().any(|b| b == BUSY);

    // The recorded reply, addressed to somebody else: the bytes are still the recording's.
    let mut row = Corpus::load("long-solo-play")
        .expect("the locked corpus decodes")
        .expect("long-solo-play is locked")
        .blobs
        .into_iter()
        .find(|r| r.idx == AFTER_THE_BUY_AT)
        .expect("the recorded reply to the purchase");
    row.payload[16..20].copy_from_slice(&0x77F0_9999_u32.to_le_bytes());
    c.when(Inbound::event(dereth_testkit::inbound::event_of(&row)))
        .tick(4);

    let (_, item) = sellable(&c);
    let sent = try_to_act(&mut c, item);
    let still_latched = !sent
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(_)))
        && bubbles(&mut c).iter().any(|b| b == BUSY);

    c.assert_behaviour(
        "vendor.busy.while-the-shop-has-not-answered-the-refusal-still-fires",
        { move |_| refused && still_latched },
    );
    c.shutdown();
}

/// The purse the recorded session carries when the shop opens, and after the purchase and the
/// sale. The literals are the recording's own.
const COIN_AT_OPEN: i32 = 9_995;
const COIN_AFTER_BUY: i32 = 9_930;
const COIN_AFTER_SALE: i32 = 9_988;
/// Where in the recording the shard's answers to each of those land.
const AFTER_THE_BUY_COIN_AT: usize = 4_744;
const AFTER_THE_SALE_COIN_AT: usize = 4_788;

/// The name and the cost line of the row the player has picked, and the two buttons that act on
/// it.
const ITEM_NAME_TEXT: ElementId = ElementId(0x1000_00C0);
const ITEM_COST_TEXT: ElementId = ElementId(0x1000_00C1);
const BTN_ADD_TO_LIST: ElementId = ElementId(0x1000_00C3);
const BTN_CLOSE: ElementId = ElementId(0x1000_00D6);
/// A live button and a sleeping one.
const STATE_NORMAL: dereth_ui::StateId = dereth_ui::StateId(1);
const STATE_DISABLED: dereth_ui::StateId = dereth_ui::StateId(0x0D);

fn state_of(c: &HeadlessClient, id: ElementId) -> dereth_ui::StateId {
    let h = element_of(c, id);
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell")
        .ui
        .node(h)
        .expect("a live element")
        .state
}

fn click_button(c: &mut HeadlessClient, id: ElementId) {
    let h = element_of(c, id);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell")
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
    c.tick(4);
}

// ---------------------------------------------------------------------------------------------
// o604: the four money lines, the tabs, and the purse.
// ---------------------------------------------------------------------------------------------

fn panel_text(ui: &mut UiSystem, h: Option<ElemHandle>) -> String {
    let h = h.expect("the element is bound");
    ui.text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

/// The four money strings a player would be reading, in writer order.
fn money(ui: &mut UiSystem, p: &VendorPanel) -> [String; 4] {
    [
        panel_text(ui, p.buy_list_text),
        panel_text(ui, p.buy_purse_text),
        panel_text(ui, p.sell_list_text),
        panel_text(ui, p.sell_purse_text),
    ]
}

/// A view answering one shop snapshot and nothing else.
#[derive(Debug, Default, Clone)]
pub(super) struct Shop(pub(super) ShopView);

impl GameView for Shop {
    fn shop(&self) -> ShopView {
        self.0.clone()
    }
}

/// A shop whose three numbers are **all different**, which is the premise: a shop whose basket
/// equalled its purse could not separate a line that computes from one that copies.
fn three_distinct_numbers() -> Shop {
    let mut s = shop_with_stack(1, -1);
    s.shop.buy_transaction = 137;
    s.shop.sell_transaction = 42;
    s.shop.buy_items = 3;
    s.shop.sell_items = 1;
    s.shop.total_value = 1_000_000;
    Shop(s.shop)
}

/// The four money lines and the tab strip are five different places in the shipped screen.
pub(super) fn the_money_lines_are_five_different_places() {
    let mut c = a_gameplay_client();
    let p = bound_vendor(c.app_mut());

    let bound = [
        p.buy_list_text,
        p.buy_purse_text,
        p.sell_list_text,
        p.sell_purse_text,
        p.type_menu,
    ];
    let all_bound = bound.iter().all(Option::is_some);
    let mut handles: Vec<ElemHandle> = bound.iter().filter_map(|h| *h).collect();
    let n = handles.len();
    handles.sort_unstable();
    handles.dedup();
    let distinct = handles.len() == n;

    // The ids as numbers, beside the symbols that carry them: a scenario that read a constant
    // through the symbol it is written through could not detect a wrong one.
    let ids = BUY_LIST_TEXT.0 == 0x1000_00C7
        && BUY_PURSE_TEXT.0 == 0x1000_00C8
        && SELL_LIST_TEXT.0 == 0x1000_00D0
        && SELL_PURSE_TEXT.0 == 0x1000_00D1
        && TYPE_FILTER_MENU.0 == 0x1000_00BF
        && ATTR_SHOP_FILTER == 0x1000_0039
        && dereth_client_model::vendor::COIN_VALUE == 20;

    c.assert_behaviour(
        "vendor.money.the-four-lines-and-the-filter-strip-are-five-different-things-in-the-shipped-tree",
        move |_| all_bound && distinct && ids,
    );
    c.shutdown();
}

/// Each money line is written by its own source, and moving one number moves exactly the lines
/// that read it.
pub(super) fn each_money_line_follows_its_own_source() {
    let mut c = a_gameplay_client();
    let mut p = bound_vendor(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());

    let base = three_distinct_numbers();
    assert!(p.update(ui, &base), "the first drive is always a redraw");
    assert_eq!(
        p.money_writes, 4,
        "the premise: all four lines were written"
    );
    let before = money(ui, &p);
    let each_line = before[0] == "Buying 3 items worth 137p"
        && before[1] == "You have 1,000,000p"
        && before[2] == "Selling 1 item worth 42p"
        && before[3] == "You have 1,000,000p"
        // The two purses are the same string from the same number in two places, and the two
        // basket lines are not.
        && before[1] == before[3]
        && before[0] != before[2];

    // (a) the buying basket alone.
    let mut v = base.clone();
    v.0.buy_transaction = 999;
    assert!(p.update(ui, &v));
    let after = money(ui, &p);
    let buy_alone = after[0] == "Buying 3 items worth 999p" && after[1..] == before[1..];

    // (b) the selling basket alone.
    let mut v = base.clone();
    v.0.sell_transaction = 7;
    assert!(p.update(ui, &v));
    let after = money(ui, &p);
    let sell_alone = after[2] == "Selling 1 item worth 7p"
        && [&after[0], &after[1], &after[3]] == [&before[0], &before[1], &before[3]];

    // (c) the purse alone -- and it moves **two** lines, because two of them render one number.
    let mut v = base.clone();
    v.0.total_value = 12_345;
    assert!(p.update(ui, &v));
    let after = money(ui, &p);
    let purse_alone = after[1] == "You have 12,345p"
        && after[3] == "You have 12,345p"
        && [&after[0], &after[2]] == [&before[0], &before[2]];

    c.assert_behaviour(
        "vendor.money.each-of-the-four-lines-is-written-by-its-own-source",
        { move |_| each_line && buy_alone && sell_alone && purse_alone },
    );
    c.shutdown();
}

/// A player with a pack and, optionally, some money.
///
/// `None` is not "poor": it is the shard having said nothing about the player's coin at all,
/// which is a different state.
fn a_player_with_coin(coin: Option<i32>) -> World {
    let mut w = World::new();
    let player = ObjectId(0x5000_000A);
    w.player = Some(player);
    let mut wn = dereth_client_model::Weenie::new(player);
    wn.pwd = dereth_protocol::types::PublicWeenieDesc {
        name: "Lark".into(),
        obj_type: dereth_client_model::weenie::item_type::CREATURE,
        bitfield: dereth_client_model::weenie::bitfield::OPENABLE,
        items_capacity: Some(10),
        containers_capacity: Some(2),
        ..dereth_protocol::types::PublicWeenieDesc::default()
    };
    let mut q = dereth_client_model::qualities::Qualities::new();
    if let Some(v) = coin {
        // **The literal 20, not the symbol the code under test reads.**
        q.ints
            .get_or_insert_with(std::collections::BTreeMap::new)
            .insert(20, v);
    }
    wn.qualities = Some(q);
    w.tables.weenies.insert(player, wn);
    w
}

/// The recording's own grocer, chosen by **what it sells** rather than by an index, so that it
/// does not go stale as the corpus grows.
pub(super) fn the_recorded_grocer() -> dereth_protocol::trade::VendorInfo {
    let blobs = dereth_client_net::client_session::testing::Corpus::load("long-solo-play")
        .expect("the locked corpus decodes")
        .expect("long-solo-play is locked")
        .blobs;
    let mut infos: Vec<dereth_protocol::trade::VendorInfo> = Vec::new();
    for b in &blobs {
        if b.dir != dereth_client_net::client_session::testing::Direction::ServerToClient
            || b.opcode != 0xF7B0
            || b.payload.len() < 16
        {
            continue;
        }
        let sub = u32::from_le_bytes([b.payload[12], b.payload[13], b.payload[14], b.payload[15]]);
        if sub != 0x0062 {
            continue;
        }
        if let Ok(m) =
            dereth_protocol::read_body::<dereth_protocol::trade::VendorInfo>(&b.payload[16..])
        {
            infos.push(m);
        }
    }
    assert!(
        infos.len() >= 2,
        "the premise: the recording carries the vendor traffic; {} decoded",
        infos.len()
    );
    infos
        .into_iter()
        .find(|m| {
            let mut probe = World::new();
            probe.handle_vendor_info(
                m,
                &mut RecordingSink::default(),
                &mut RecordingRequests::default(),
                dereth_primitives::ServerTime(0.0),
            );
            probe.shop.list_contains_type(0x0000_0020)
                && !probe.shop.list_contains_type(0x0000_0002)
        })
        .expect("a recorded shop that sells food and no armour")
}

fn open_the_grocer(w: &mut World) {
    w.handle_vendor_info(
        &the_recorded_grocer(),
        &mut RecordingSink::default(),
        &mut RecordingRequests::default(),
        dereth_primitives::ServerTime(0.0),
    );
}

/// A basket is counted in things, not in rows, and the noun follows the count.
pub(super) fn a_basket_is_counted_in_things_and_not_in_rows() {
    let mut c = a_retail_client();

    // The noun fork, on both sides of the boundary and on the boundary itself.
    let nouns = vendor::transaction_line("Buying", 0, 0) == "Buying 0 items worth 0p"
        && vendor::transaction_line("Buying", 1, 5) == "Buying 1 item worth 5p"
        && vendor::transaction_line("Buying", 2, 10) == "Buying 2 items worth 10p"
        && vendor::transaction_line("Selling", 1, 1) == "Selling 1 item worth 1p"
        && vendor::transaction_line("Selling", 250, 1_234_567)
            == "Selling 250 items worth 1,234,567p"
        && vendor::purse_line(0) == "You have 0p"
        && vendor::purse_line(999) == "You have 999p"
        && vendor::purse_line(1_000) == "You have 1,000p";

    // …and the count itself, through the seam, which is the only place the stack sizes are.
    let mut w = a_player_with_coin(Some(1_000_000));
    open_the_grocer(&mut w);
    let stock = w.shop.stock[0].iid;
    let empty = dereth_client::vendor_view::shop(&w);
    let mut counts = empty.buy_items == 0 && empty.sell_items == 0;

    w.shop.stock[0].pwd.stack_size = Some(1);
    w.shop.buy_list = vec![(stock, 3)];
    counts &= dereth_client::vendor_view::shop(&w).buy_items == 3;

    // The selected quantity is already in the basket, independent of the stock stack size.
    w.shop.stock[0].pwd.stack_size = Some(20);
    counts &= dereth_client::vendor_view::shop(&w).buy_items == 3;

    // A thing with no count of its own, or a count of nothing, counts as one.
    w.shop.stock[0].pwd.stack_size = None;
    counts &= dereth_client::vendor_view::shop(&w).buy_items == 3;
    w.shop.stock[0].pwd.stack_size = Some(0);
    counts &= dereth_client::vendor_view::shop(&w).buy_items == 3;

    // A basket row naming something the client does not hold contributes nothing.
    w.shop.buy_list = vec![(stock, 3), (ObjectId(0xDEAD_BEEF), 5)];
    counts &= dereth_client::vendor_view::shop(&w).buy_items == 3;

    // …and the selling side reads the thing's own count.
    let arrow = ObjectId(0x8000_0A6E);
    let mut wn = dereth_client_model::Weenie::new(arrow);
    wn.pwd = dereth_protocol::types::PublicWeenieDesc {
        name: "Arrow".into(),
        stack_size: Some(250),
        ..dereth_protocol::types::PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(arrow, wn);
    w.shop.sell_list = vec![(arrow, 1)];
    counts &= dereth_client::vendor_view::shop(&w).sell_items == 250;

    c.assert_behaviour(
        "vendor.money.a-basket-is-counted-in-things-and-not-in-rows",
        move |_| nouns && counts,
    );
    c.shutdown();
}

/// The tab strip is built from the stock that is actually there.
pub(super) fn the_filter_strip_follows_the_stock() {
    let mut c = a_gameplay_client();

    let mut w = World::new();
    open_the_grocer(&mut w);
    let view = dereth_client::vendor_view::shop(&w);
    assert!(
        view.open && !view.stock.is_empty(),
        "the premise: the recorded shop has stock"
    );

    let names: Vec<String> = view
        .type_filters
        .iter()
        .map(|(n, _)| (*n).to_owned())
        .collect();
    // What it sells and what it does not, rather than a count -- a count would go stale with the
    // recording, and this is a statement about the mechanism.
    let sells_food = names.iter().any(|n| n == "Food");
    let no_armour = !names.iter().any(|n| n == "Armor");
    let not_everything = view.type_filters.len() < 18;
    let masks_agree = view
        .type_filters
        .iter()
        .all(|(_, mask)| *mask != 0 && w.shop.list_contains_type(*mask));

    let mut p = bound_vendor(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());
    assert!(p.update(ui, &Shop(view.clone())));
    let menu = p.type_menu.expect("the tab strip");
    let mut rows_match = p.num_type_filters == view.type_filters.len()
        && dereth_ui::widgets::menu::num_items(ui, menu) == view.type_filters.len();
    for (i, (name, mask)) in view.type_filters.iter().enumerate() {
        let row = dereth_ui::widgets::menu::get_item(ui, menu, i).expect("a row");
        #[allow(clippy::cast_possible_wrap)]
        let want = *mask as i32;
        rows_match &= &panel_text(ui, Some(row)) == name
            && ui
                .node(row)
                .and_then(|n| n.instance_properties.get(ATTR_SHOP_FILTER).cloned())
                == Some(dereth_ui::PropertyValue::Integer(want));
    }

    // The positive control: two tabs really do make two rows through this exact path.
    let two = Shop(ShopView {
        open: true,
        vendor: Some(ObjectId(0x8000_0001)),
        type_filters: vec![("Food", 0x0000_0020), ("Gems", 0x0000_0800)],
        ..ShopView::default()
    });
    assert!(p.update(ui, &two));
    let control = p.num_type_filters == 2 && dereth_ui::widgets::menu::num_items(ui, menu) == 2;

    // A shop with nothing in it earns no tabs.
    let none = Shop(ShopView {
        open: true,
        vendor: Some(ObjectId(0x8000_0001)),
        ..ShopView::default()
    });
    assert!(p.update(ui, &none));
    let empty = p.num_type_filters == 0
        && dereth_ui::widgets::menu::num_items(ui, menu) == 0
        && dereth_ui::widgets::menu::selected_index(ui, menu) == -1;

    // …and a shop that has been closed keeps none, whatever it was showing. The snapshot here is
    // made up on purpose: a closed shop still carrying tabs is a state the seam never produces,
    // and it is the only one that can tell the panel's own guard from an empty snapshot.
    let closed_with_stale_tabs = Shop(ShopView {
        open: false,
        type_filters: vec![("Food", 0x0000_0020), ("Gems", 0x0000_0800)],
        ..ShopView::default()
    });
    assert!(p.update(ui, &closed_with_stale_tabs));
    let closed =
        p.num_type_filters == 0 && dereth_ui::widgets::menu::num_items(ui, menu) == 0 && !p.visible;

    assert!(p.update(ui, &Shop(ShopView::default())));
    let plainly_closed = p.num_type_filters == 0 && !p.visible;

    c.assert_behaviour(
        "vendor.stock.the-filter-strip-is-built-from-the-stock-that-is-actually-there",
        move |_| {
            sells_food
                && no_armour
                && not_everything
                && masks_agree
                && rows_match
                && control
                && empty
                && closed
                && plainly_closed
        },
    );
    c.shutdown();
}

/// Opening a shop fills its purse from the player's own money, and a player the shard has said
/// nothing about is refused every purchase.
///
/// The first half is the finding and is asserted first: a scenario that only checked the second
/// would pass on a client that made every player rich.
pub(super) fn the_purse_is_filled_from_the_players_own_coin() {
    let mut c = a_retail_client();

    let mut probe = World::new();
    open_the_grocer(&mut probe);
    let item = probe.shop.stock.first().expect("the shop has stock").iid;
    let price = {
        let pwd = probe.shop.stock[0].pwd.clone();
        probe.shop.profile.vendor_sell_price(&pwd, 1)
    };
    assert!(price > 0, "the premise: the first row costs something");

    // (a) A player the shard has said nothing about.
    let mut w = a_player_with_coin(None);
    let mut out = RecordingSink::default();
    let mut req = RecordingRequests::default();
    open_the_grocer(&mut w);
    let penniless = w.shop.total_value == 0;
    req.0.clear();
    let refused = w.buy_single_item(
        item,
        1,
        &mut req,
        &mut out,
        dereth_primitives::ServerTime(1.0),
    ) == Err(dereth_client_model::vendor::BuyRefusal::NotEnoughMoney)
        && req.0.is_empty();

    // (b) The same player, with money.
    let mut w = a_player_with_coin(Some(1_000_000));
    let mut out = RecordingSink::default();
    let mut req = RecordingRequests::default();
    open_the_grocer(&mut w);
    let filled = w.shop.total_value == 1_000_000;
    req.0.clear();
    let bought = w.buy_single_item(
        item,
        1,
        &mut req,
        &mut out,
        dereth_primitives::ServerTime(1.0),
    ) == Ok(true)
        && req.0.len() == 1
        && dereth_client::vendor_view::shop(&w).total_value == 1_000_000;

    c.assert_behaviour(
        "vendor.purse.is-filled-from-the-players-own-coin-when-the-shop-opens",
        { move |_| penniless && refused && filled && bought },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// f47: the purse on screen, the picked row's two lines, and the close that asks.
// ---------------------------------------------------------------------------------------------

/// Both places the shop says what the player has are carrying it, and they follow it afterwards.
pub(super) fn the_shop_shows_the_players_own_coin_and_follows_it() {
    let mut c = a_client_at_the_shop();
    let want = format!(
        "You have {}p",
        dereth_ui_screens::panels::examination::insert_commas(COIN_AT_OPEN)
    );
    let at_open = c.view().world().shop.total_value == COIN_AT_OPEN
        && text_of(&mut c, BUY_PURSE_TEXT) == want
        && text_of(&mut c, SELL_PURSE_TEXT) == want;

    // The recorded purchase.
    c.when(Inbound::from_corpus(
        "long-solo-play",
        SHOP_OPEN_AT + 1..AFTER_THE_BUY_COIN_AT + 1,
    ))
    .tick(4);
    let after_buy = format!(
        "You have {}p",
        dereth_ui_screens::panels::examination::insert_commas(COIN_AFTER_BUY)
    );
    let bought = c.view().world().shop.is_open()
        && c.view().world().shop.total_value == COIN_AFTER_BUY
        && text_of(&mut c, BUY_PURSE_TEXT) == after_buy
        && text_of(&mut c, SELL_PURSE_TEXT) == after_buy;

    // …and the recorded sale, on a fresh run so the window is the one the recording re-opens.
    let mut c2 = a_client_at_the_shop();
    c2.when(Inbound::from_corpus(
        "long-solo-play",
        SHOP_OPEN_AT + 1..AFTER_THE_SALE_COIN_AT + 1,
    ))
    .tick(4);
    let after_sale = format!(
        "You have {}p",
        dereth_ui_screens::panels::examination::insert_commas(COIN_AFTER_SALE)
    );
    let sold = c2.view().world().shop.is_open()
        && c2.view().world().shop.total_value == COIN_AFTER_SALE
        && text_of(&mut c2, BUY_PURSE_TEXT) == after_sale
        && text_of(&mut c2, SELL_PURSE_TEXT) == after_sale;
    c2.shutdown();

    c.assert_behaviour(
        "vendor.purse.the-open-shop-shows-the-players-own-coin-and-follows-it",
        { move |_| at_open && bought && sold },
    );
    c.shutdown();
}

/// The picked row writes its name and its cost line, and both buttons wake up.
///
/// The press lands on the **last** filled row: the shop already picked the first one for itself
/// when it opened, so a scenario that pressed row zero could pass on a client with no arm at all.
pub(super) fn the_picked_stock_row_writes_its_name_and_its_cost() {
    let mut c = a_client_at_the_shop();
    let (handle, item) = *stock_slots(&c)
        .last()
        .expect("the live stock list holds a filled row");

    // The oracle for both strings is the seam, which is a different path from the panel's own.
    let (name, price) = {
        let view = dereth_client::vendor_view::shop(c.view().world());
        let row = view
            .stock
            .iter()
            .find(|r| r.item == item)
            .expect("the pressed slot is a row of this shop")
            .clone();
        (row.name, row.price)
    };
    assert!(
        !name.is_empty() && price > 0,
        "the premise: the row has a name and a price"
    );

    press(&mut c, handle);
    assert_eq!(
        c.view().world().selected,
        Some(item),
        "the premise: the press picked it"
    );

    let want_cost = format!(
        "costs {}p (you have {}p)",
        dereth_ui_screens::panels::examination::insert_commas(price),
        dereth_ui_screens::panels::examination::insert_commas(COIN_AT_OPEN)
    );
    let picked = text_of(&mut c, ITEM_NAME_TEXT) == name
        && text_of(&mut c, ITEM_COST_TEXT) == want_cost
        && state_of(&c, BTN_BUY) == STATE_NORMAL
        && state_of(&c, BTN_ADD_TO_LIST) == STATE_NORMAL;

    // …and the control: picking nothing clears both and puts both buttons back to sleep.
    c.world_mut()
        .set_selected_object(None, false, &mut RecordingSink::default());
    c.tick(3);
    let unpicked = text_of(&mut c, ITEM_NAME_TEXT).is_empty()
        && text_of(&mut c, ITEM_COST_TEXT).is_empty()
        && state_of(&c, BTN_BUY) == STATE_DISABLED
        && state_of(&c, BTN_ADD_TO_LIST) == STATE_DISABLED;

    c.assert_behaviour(
        "vendor.stock.the-picked-row-writes-its-name-and-its-cost-and-wakes-both-buttons",
        move |_| picked && unpicked,
    );
    c.shutdown();
}

/// Closing with something in a basket asks first, in a dialog; with the baskets empty it does not.
pub(super) fn closing_with_a_basket_asks_first() {
    let mut c = a_client_at_the_shop();
    let (handle, item) = *stock_slots(&c).last().expect("a filled stock row");
    press(&mut c, handle);
    click_button(&mut c, BTN_ADD_TO_LIST);
    assert!(
        c.view()
            .world()
            .shop
            .buy_list
            .iter()
            .any(|(id, _)| *id == item),
        "the premise: the basket holds it"
    );
    let scroll_before = c.view().world().scroll.added;

    click_button(&mut c, BTN_CLOSE);
    let still_open = c.view().world().shop.is_open();

    let root = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell")
        .ui
        .dialogs
        .open_on(dereth_ui::dialog::factory::DEFAULT_QUEUE)
        .expect("the client asks with a dialog rather than a line across the top of the screen")
        .element
        .expect("the dialog's own element is created and bound");
    let shown = {
        let shell = c.app_mut().ui_mut().expect("the UI shell");
        let h = shell
            .ui
            .get_child_recursive(root, dereth_ui::dialog::base::child::TEXT)
            .expect("a dialog binds its text");
        shell
            .ui
            .text_element_mut(h)
            .map(|t| t.glyphs.inq_text(false))
            .unwrap_or_default()
    };
    let asked = still_open
        && shown.contains(dereth_client_model::vendor::UNFINISHED_TRANSACTIONS)
        && c.view().world().scroll.added == scroll_before;

    // …and the control: with both baskets empty it closes at once and asks nothing.
    let mut empty = a_client_at_the_shop();
    assert!(
        empty.view().world().shop.buy_list.is_empty()
            && empty.view().world().shop.sell_list.is_empty(),
        "the premise: a freshly opened shop's baskets are empty"
    );
    click_button(&mut empty, BTN_CLOSE);
    let closed_at_once = !empty.view().world().shop.is_open()
        && empty
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell")
            .ui
            .dialogs
            .open_on(dereth_ui::dialog::factory::DEFAULT_QUEUE)
            .is_none();
    empty.shutdown();

    c.assert_behaviour(
        "vendor.close.leaving-with-something-in-a-basket-asks-first",
        move |_| asked && closed_at_once,
    );
    c.shutdown();
}

// =============================================================================================
// The three things that tell a player whether a drop will be taken, and why not.
//
// Three failures, all about the same absence: the client took the drop or refused it and said
// nothing either way. What the shop says while something is carried over it, the mark a thing wears
// once it has been offered, and the sentence the player reads when the shard says no.
//
// The two cursor answers are the two arms of one decision and are one row; the two world-drop
// refusals are one claim measured twice -- the sentence, and the name inside it -- and are one
// row that asserts both sentences.
// =============================================================================================

/// The recorded shop, her sell list and the tab that raises it.
pub(super) const SELL_TAB: ElementId = ElementId(0x1000_00BB);
/// The two answers the shop's own hover gives.
const HINT_ACCEPT: StateId = StateId(0x1000_0040);
const HINT_REFUSE: StateId = StateId(0x1000_0041);

/// Everything the feedback strip has been given, drawn or not.
pub(super) fn feedback(c: &HeadlessClient) -> Vec<String> {
    let m = &c.view().expect_app().hud().panels.spew.model;
    m.pending.iter().chain(m.items.iter()).cloned().collect()
}

/// The shop open on its **Selling** tab, with the player's own pack page up behind it.
fn a_client_at_the_shop_selling() -> HeadlessClient {
    let mut c = a_client_at_the_shop();
    let tab = element_of(&c, SELL_TAB);
    {
        let (ui, _root) = gameplay_root(c.app_mut());
        ui.broadcast_element_message(
            tab,
            dereth_ui::msg::element::id::BUTTON_CLICKED,
            dereth_ui::focus::action::PRIMARY_CLICK,
            0,
        );
    }
    c.tick(3);
    assert!(
        c.view().expect_app().hud().panels.vendor.sell.is_some(),
        "the premise: the shop's sell list is bound"
    );
    c
}

/// A thing in the player's pack the shop would buy, and one she would not.
fn something_she_takes_and_something_she_will_not(
    c: &mut HeadlessClient,
) -> (
    Option<(ElemHandle, ObjectId)>,
    Option<(ElemHandle, ObjectId)>,
) {
    let app = c.view().expect_app();
    let world = &app.objects().world;
    let shell = app.ui().expect("the UI shell");
    let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
    let screen = any
        .downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen");
    let (mut good, mut bad) = (None, None);
    for w in screen.inventory.item_list.iter() {
        for s in &w.slots {
            let Some(id) = s.item else { continue };
            let Some(o) = world.weenie(id) else { continue };
            if !world.is_owned_by_player(id) {
                continue;
            }
            let empty = world
                .inventory(id)
                .is_none_or(|i| i.items.is_empty() && i.containers.is_empty());
            if !empty {
                continue;
            }
            if world.shop.profile.inq_acceptability(&o.pwd) == 0 {
                good.get_or_insert((s.handle, id));
            } else {
                bad.get_or_insert((s.handle, id));
            }
        }
    }
    (good, bad)
}

/// The shop's first sell tile, and what its own hint element is showing.
fn sell_tile(c: &HeadlessClient) -> (ElemHandle, Option<StateId>) {
    let list = c
        .view()
        .expect_app()
        .hud()
        .panels
        .vendor
        .sell
        .as_ref()
        .expect("the shop's sell list is bound");
    let s = list.slots.first().expect("the list keeps its empty tiles");
    (s.handle, s.drag_accept_state)
}

/// Pick a pack tile up and carry the icon over `over`.
///
/// **Not driven through the pointer, and the reason is the layout and not the gesture.** At this
/// screen size the shop sits over the player's pack, so with the shop up no tile of the pack grid
/// is the element under its own centre -- a pointer aimed at one reaches the shop's own buttons.
/// That is a fact about where two windows are, which a player changes by moving one, and driving
/// the pointer here would be asserting the layout. Every handler on the path below is still the
/// production one: the drag is begun by the list's own producer at the point the tile is at, and
/// the hover is the message the element manager raises when the pointer enters something.
fn carry_icon_over(c: &mut HeadlessClient, from: ElemHandle, over: ElemHandle) {
    let (x, y) = {
        let (ui, _root) = gameplay_root(c.app_mut());
        centre(ui, from)
    };
    {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the UI shell");
        let any: &mut dyn std::any::Any = &mut **shell.flow.current_mut().expect("a screen");
        let screen = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");
        screen
            .begin_item_drag(&mut shell.ui, from, x, y)
            .expect("the tile starts a drag");
    }
    c.tick(1);
    assert!(
        {
            let (ui, _root) = gameplay_root(c.app_mut());
            ui.drag_state().element.is_some()
        },
        "the premise: the tile made a proxy, or there is nothing to hint about"
    );
    {
        let (ui, _root) = gameplay_root(c.app_mut());
        ui.broadcast_element_message(over, dereth_ui::msg::element::id::DRAG_CURSOR_OVER, 1, 0);
    }
    c.tick(3);
}

// ---------------------------------------------------------------------------------------------
// vendor.sell.the-window-says-whether-it-would-buy-what-is-carried-over-it
// ---------------------------------------------------------------------------------------------

/// Carrying something over a shop's sell window shows whether she would buy it: one answer for
/// something she takes and a different one for something she will not.
///
/// Both arms are one scenario because they are the two answers to one question, and a client that
/// painted the same thing for both would be no better than the one that painted nothing. The
/// control is asserted first: the tile is not already showing the accepting answer before anything
/// is carried over it.
pub(super) fn the_shop_says_whether_it_would_buy_what_is_carried_over_it() {
    // **One client at a time.** Two live ones in this process share the shell's request globals,
    // which is why this binary runs serially at all, so the first arm is shut down before the
    // second is built.
    let takes_it = {
        let mut c = a_client_at_the_shop_selling();
        let (good, _) = something_she_takes_and_something_she_will_not(&mut c);
        let (from, _) = good.expect("the recorded pack carries something this shop accepts");
        let (tile, before) = sell_tile(&c);
        assert_ne!(
            before,
            Some(HINT_ACCEPT),
            "the premise: the answer is not already showing"
        );
        carry_icon_over(&mut c, from, tile);
        let answer = sell_tile(&c).1 == Some(HINT_ACCEPT);
        c.shutdown();
        answer
    };

    let mut c = a_client_at_the_shop_selling();
    let (_, bad) = something_she_takes_and_something_she_will_not(&mut c);
    let (from_bad, _) = bad.expect("the recorded pack carries something this shop refuses");
    let (tile, _) = sell_tile(&c);
    carry_icon_over(&mut c, from_bad, tile);
    let will_not = sell_tile(&c).1 == Some(HINT_REFUSE);

    c.assert_behaviour(
        "vendor.sell.the-window-says-whether-it-would-buy-what-is-carried-over-it",
        move |_| takes_it && will_not,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// vendor.tabs.something-carried-over-the-window-turns-it-to-the-selling-tab
// ---------------------------------------------------------------------------------------------

/// Carrying something over the shop window turns it to the Selling tab by itself -- over any
/// part of the window, here the stock list on the Items page -- and the pointer over the same
/// spot with nothing carried leaves the page alone.
pub(super) fn something_carried_over_the_window_turns_it_to_the_selling_tab() {
    use dereth_ui_screens::panels::vendor::{PAGE_ITEMS, PAGE_SELL, STOCK_LIST};
    let page = |c: &HeadlessClient| {
        c.view()
            .expect_app()
            .hud()
            .panels
            .vendor
            .open_page(&c.view().expect_app().ui().expect("the UI shell").ui)
    };
    let mut c = a_client_at_the_shop();
    let starts_on_items = page(&c) == Some(PAGE_ITEMS);
    let (from, _item) = sellable(&c);
    let stock = element_of(&c, STOCK_LIST);
    let (sx, sy) = {
        let (ui, _root) = gameplay_root(c.app_mut());
        centre(ui, stock)
    };

    // The pointer over the stock list with nothing in hand.
    {
        let (ui, _root) = gameplay_root(c.app_mut());
        ui.mouse_move(LocalTime(1.0), sx, sy);
    }
    c.tick(3);
    let empty_handed_stays = page(&c) == Some(PAGE_ITEMS);

    // The same spot with the stack in hand.
    let (fx, fy) = {
        let (ui, _root) = gameplay_root(c.app_mut());
        centre(ui, from)
    };
    {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the UI shell");
        let any: &mut dyn std::any::Any = &mut **shell.flow.current_mut().expect("a screen");
        let screen = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");
        screen
            .begin_item_drag(&mut shell.ui, from, fx, fy)
            .expect("the tile starts a drag");
    }
    c.tick(1);
    let carrying = {
        let (ui, _root) = gameplay_root(c.app_mut());
        ui.is_dragging()
    };
    {
        let (ui, _root) = gameplay_root(c.app_mut());
        ui.mouse_move(LocalTime(2.0), sx, sy);
    }
    c.tick(3);
    let turned_to_selling = page(&c) == Some(PAGE_SELL);

    c.assert_behaviour(
        "vendor.tabs.something-carried-over-the-window-turns-it-to-the-selling-tab",
        move |_| starts_on_items && empty_handed_stays && carrying && turned_to_selling,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// vendor.sell.something-offered-wears-the-mark-in-the-pack-as-well-as-in-the-window
// ---------------------------------------------------------------------------------------------

/// A thing put into the shop's sell list wears the offered mark **in the player's own pack as
/// well as in the window**.
///
/// One thing is being drawn in two places and the mark belongs to the thing, not to either list,
/// which is the whole of the claim. The mark is read off the live element and not off the list's
/// record of it, because a list that remembered the flag and never raised the element is exactly
/// the failure this guards against.
pub(super) fn something_offered_to_the_shop_wears_the_mark_in_both_windows() {
    let mut c = a_client_at_the_shop_selling();
    let (good, _) = something_she_takes_and_something_she_will_not(&mut c);
    let (from, item) = good.expect("the recorded pack carries something this shop accepts");
    let list = element_of(&c, SELL_LIST);

    {
        let (ui, _root) = gameplay_root(c.app_mut());
        ui.broadcast_element_message(
            list,
            dereth_ui::msg::element::id::DROP_FAILED,
            0,
            from.raw(),
        );
    }
    c.tick(4);

    let offered = c
        .view()
        .world()
        .weenie(item)
        .expect("the offered thing")
        .sell_state
        == 1;

    let marks = |c: &HeadlessClient, slot: &dereth_ui_screens::items::widget::ItemSlot| {
        let h = slot
            .sell_state_elem
            .expect("every tile binds the offered mark");
        let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
        (
            slot.sell_state,
            ui.node(h).expect("live").region.flags.visible,
        )
    };
    let in_the_pack = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the UI shell");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
        any.downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen")
            .inventory
            .item_list
            .iter()
            .flat_map(|w| w.slots.iter())
            .find(|s| s.item == Some(item) && s.handle == from)
            .map(|s| marks(&c, s))
            .expect("the pack tile the drag started from still holds the thing")
    };
    let in_the_window = {
        let app = c.view().expect_app();
        app.hud()
            .panels
            .vendor
            .sell
            .as_ref()
            .expect("the sell list")
            .slots
            .iter()
            .find(|s| s.item == Some(item))
            .map(|s| marks(&c, s))
            .expect("the sell list now holds the thing")
    };

    c.assert_behaviour(
        "vendor.sell.something-offered-wears-the-mark-in-the-pack-as-well-as-in-the-window",
        move |_| offered && in_the_pack == (true, true) && in_the_window == (true, true),
    );
    c.shutdown();
}

// =============================================================================================
// Three more things the shop window does.
//
// The drag hints come down when a drag stops; a whole backpack can be dragged as a proxy for
// every single item in it; and the items list, tabbed off and back to, draws with its
// background and borders before any item is selected.
//
// The fixture is the recorded Holtburg shopkeeper, opened by the recording's own message, with the
// player's pack page up behind her. What each of her answers is belongs to her recorded profile and
// is never chosen here.
// =============================================================================================

/// The shop's own tabs and the two pages behind them.
const ITEMS_TAB: ElementId = ElementId(0x1000_00B9);
const ITEMS_PAGE: ElementId = ElementId(0x1000_00BC);
const SELLING_PAGE: ElementId = ElementId(0x1000_00CD);

/// A real click on one of the shop's tabs.
///
/// **A plain click and not a button press**, and the difference is the gesture: the panel opens a
/// tab on a click and on nothing else, so a button press here would reach the window's own button
/// handler and leave the live panel showing the page it already had -- which is not the gesture
/// a player makes.
pub(super) fn click_shop_tab(c: &mut HeadlessClient, id: ElementId) {
    let h = element_of(c, id);
    {
        let (ui, _root) = gameplay_root(c.app_mut());
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    }
    c.tick(3);
}

/// Which of the shop's pages is open.
fn shop_page(c: &HeadlessClient) -> Option<ElementId> {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the UI shell").ui;
    app.hud().panels.vendor.open_page(ui)
}

/// The state the first sell tile's own hint overlay is in -- the live element, not the panel's
/// mirror of it.
fn sell_tile_overlay(c: &mut HeadlessClient) -> Option<StateId> {
    let h = {
        let list = c
            .view()
            .expect_app()
            .hud()
            .panels
            .vendor
            .sell
            .as_ref()
            .expect("the shop's sell list is bound");
        list.slots
            .first()
            .expect("the list keeps its empty tiles")
            .drag_accept
    }?;
    let (ui, _root) = gameplay_root(c.app_mut());
    ui.node(h).map(|n| n.state)
}

/// Let the carried icon go on `target`: the message the element manager raises on the thing the
/// drag cursor was last over, carrying the drag's owner.
fn let_the_icon_go_on(c: &mut HeadlessClient, target: ElemHandle, owner: ElemHandle) {
    {
        let (ui, _root) = gameplay_root(c.app_mut());
        ui.broadcast_element_message(
            target,
            dereth_ui::msg::element::id::DROP_FAILED,
            0,
            owner.raw(),
        );
    }
    c.tick(4);
    clear_requests(c.ui_outbox());
}

// ---------------------------------------------------------------------------------------------
// vendor.sell.letting-go-takes-the-hint-down-whichever-answer-it-was-showing
// ---------------------------------------------------------------------------------------------

/// Letting the icon go on the shop's own tile takes that tile's hint down at once, and it does
/// so whichever of the two answers the tile was showing.
///
/// Both answers are one scenario because the clear does not look at the outcome at all: it
/// happens before the drop is decided. A client that cleared only the accepting answer would
/// leave a red circle sitting on the window for the rest of the session.
pub(super) fn letting_go_takes_the_shop_hint_down_whichever_answer_it_was_showing() {
    let was_accepting = {
        let mut c = a_client_at_the_shop_selling();
        let (good, _bad) = something_she_takes_and_something_she_will_not(&mut c);
        let (from, _id) = good.expect("the recorded pack carries something this shop buys");
        let (tile, _) = sell_tile(&c);
        carry_icon_over(&mut c, from, tile);
        let up = sell_tile_overlay(&mut c) == Some(drag_accept_state::ACCEPT);
        let_the_icon_go_on(&mut c, tile, from);
        let down = sell_tile_overlay(&mut c) == Some(drag_accept_state::NONE);
        c.shutdown();
        up && down
    };

    let mut c = a_client_at_the_shop_selling();
    let (_good, bad) = something_she_takes_and_something_she_will_not(&mut c);
    let (from, _id) = bad.expect("the recorded pack carries something this shop refuses");
    let (tile, _) = sell_tile(&c);
    carry_icon_over(&mut c, from, tile);
    let was_refusing_up = sell_tile_overlay(&mut c) == Some(drag_accept_state::REFUSE);
    let_the_icon_go_on(&mut c, tile, from);
    let was_refusing_down = sell_tile_overlay(&mut c) == Some(drag_accept_state::NONE);

    c.assert_behaviour(
        "vendor.sell.letting-go-takes-the-hint-down-whichever-answer-it-was-showing",
        move |_| was_accepting && was_refusing_up && was_refusing_down,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// vendor.stock.coming-back-to-the-stock-tab-draws-every-row-decorated-again
// ---------------------------------------------------------------------------------------------

/// Tabbing away from the shop's stock and back draws every row with its own finished picture
/// again, and not with the bare one the row was filled from.
///
/// The control is the first open, in the same run: a scenario that only looked after the tab
/// return could not tell "it never drew properly" from "it stopped drawing properly".
pub(super) fn coming_back_to_the_stock_tab_draws_every_row_decorated_again() {
    /// Every filled stock tile and whether the picture it is drawing is the finished one.
    fn stock(c: &HeadlessClient) -> Vec<(ObjectId, bool)> {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell").ui;
        app.hud()
            .panels
            .vendor
            .stock
            .as_ref()
            .expect("the shop's stock list is bound")
            .slots
            .iter()
            .filter_map(|s| {
                let id = s.item?;
                let icon = s.icon.expect("every tile draws a picture");
                let finished = ui
                    .node(icon)
                    .and_then(|n| n.region.image.as_ref().cloned())
                    .and_then(|i| i.op)
                    .and_then(dereth_ui::region::SurfaceOp::icon_recipe)
                    .is_some();
                Some((id, finished))
            })
            .collect()
    }

    let mut c = a_client_at_the_shop();
    let opens_on_the_stock = shop_page(&c) == Some(ITEMS_PAGE);
    let first = stock(&c);
    let she_stocks_rows = !first.is_empty();
    let all_finished_at_first = first.iter().all(|(_, finished)| *finished);

    click_shop_tab(&mut c, SELL_TAB);
    let really_left = shop_page(&c) == Some(SELLING_PAGE);
    click_shop_tab(&mut c, ITEMS_TAB);
    let really_back = shop_page(&c) == Some(ITEMS_PAGE);

    let again = stock(&c);
    let same_rows = again.iter().map(|(id, _)| *id).collect::<Vec<_>>()
        == first.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    let all_finished_again = again.iter().all(|(_, finished)| *finished);

    c.assert_behaviour(
        "vendor.stock.coming-back-to-the-stock-tab-draws-every-row-decorated-again",
        move |_| {
            opens_on_the_stock
                && she_stocks_rows
                && all_finished_at_first
                && really_left
                && really_back
                && same_rows
                && all_finished_again
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// A pack carried onto the sell window stands for what is inside it.
// ---------------------------------------------------------------------------------------------

/// The sack the recording's player carries. Its container is the player, so it is his to sell.
const SACK: ObjectId = ObjectId(0x8000_0997);
/// Four of that player's own recorded things, put inside the sack. Two of them this shopkeeper
/// buys and two she does not, so the per-thing gate has something to decide.
const SACK_BREAD: ObjectId = ObjectId(0x8000_099A);
const SACK_SHORTBOW: ObjectId = ObjectId(0x8000_0A74);
const SACK_LETTER: ObjectId = ObjectId(0x8000_099C);
const SACK_GUIDE: ObjectId = ObjectId(0x8000_0A76);

/// Move four of the player's things into the sack.
///
/// **Put there, and the reason is measured**: no container in any of the seven recordings ever
/// holds anything -- a scan of this one finds not one list of contents outside the player's own
/// -- so nothing recorded can make this shape. Everything downstream is still the recording's:
/// these are four things the recorded description created, with their own recorded facts, and
/// what the shopkeeper thinks of each of them is her own recorded profile's answer.
fn fill_the_sack(c: &mut HeadlessClient) {
    let inside = [SACK_BREAD, SACK_SHORTBOW, SACK_LETTER, SACK_GUIDE];
    let world = &mut c.app_mut().probe_mut().objects_mut().world;
    let player = world.player.expect("the recorded player");
    if let Some(p) = world.tables.inventories.get_mut(player) {
        p.items.retain(|i| !inside.contains(i));
    }
    for id in inside {
        if let Some(w) = world.weenie_mut(id) {
            w.pwd.container_id = Some(SACK);
        }
    }
    world.tables.inventories.insert(
        SACK,
        dereth_client_model::objects::ObjectInventory {
            container: SACK,
            items: inside.to_vec(),
            containers: Vec::new(),
            placements: Vec::new(),
        },
    );
    c.tick(3);
}

/// Whichever of the player's lists is drawing the sack.
fn sack_tile(c: &HeadlessClient) -> ElemHandle {
    let app = c.view().expect_app();
    let shell = app.ui().expect("the UI shell");
    let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
    let inv = &any
        .downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen")
        .inventory;
    for w in inv
        .top_container
        .iter()
        .chain(inv.container_list.iter())
        .chain(inv.item_list.iter())
    {
        for s in &w.slots {
            if s.item == Some(SACK) {
                return s.handle;
            }
        }
    }
    panic!("the recorded player carries the sack and one of his lists must be drawing it")
}

/// What the shop's basket holds, in order.
fn basket(c: &HeadlessClient) -> Vec<ObjectId> {
    c.view()
        .world()
        .shop
        .sell_list
        .iter()
        .map(|(id, _)| *id)
        .collect()
}

/// What the live sell list is drawing, in list order.
fn sell_rows(c: &HeadlessClient) -> Vec<ObjectId> {
    c.view()
        .expect_app()
        .hud()
        .panels
        .vendor
        .sell
        .as_ref()
        .expect("the shop's sell list is bound")
        .slots
        .iter()
        .filter_map(|s| s.item)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// vendor.sell.a-pack-let-go-on-the-window-offers-what-is-inside-it-and-not-itself
// ---------------------------------------------------------------------------------------------

/// A pack let go on the shop's sell window offers every thing inside it the shop would buy, in
/// the pack's own order, and does **not** offer the pack.
///
/// The pack is accepted for the gesture whatever the shop thinks of packs, and with no message;
/// each thing inside is then judged on its own. The line the player reads names the pack, which
/// is how they know a whole container was emptied onto the counter rather than one thing added.
pub(super) fn a_pack_let_go_on_the_window_offers_what_is_inside_it_and_not_itself() {
    let mut c = a_client_at_the_shop_selling();
    fill_the_sack(&mut c);

    let his_own = c.view().world().is_owned_by_player(SACK);
    let the_gesture_is_allowed = c.view().world().drag_item_acceptable(SACK).is_none();

    let from = sack_tile(&c);
    let target = element_of(&c, SELL_LIST);
    let_the_icon_go_on(&mut c, target, from);

    let offered = basket(&c);
    let drawn = sell_rows(&c);
    let marked = c
        .view()
        .world()
        .weenie(SACK_BREAD)
        .expect("the recorded loaf")
        .sell_state
        == 1;
    let said_so = feedback(&c).iter().any(|l| l == "Selling contents of Sack");

    c.assert_behaviour(
        "vendor.sell.a-pack-let-go-on-the-window-offers-what-is-inside-it-and-not-itself",
        move |_| {
            his_own
                && the_gesture_is_allowed
                && offered == vec![SACK_BREAD, SACK_LETTER]
                && drawn == vec![SACK_BREAD, SACK_LETTER]
                && marked
                && said_so
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// vendor.sell.an-empty-pack-is-offered-as-itself
// ---------------------------------------------------------------------------------------------

/// The half of the same walk a client could get wrong in the generous direction: an **empty**
/// pack has no contents to stand for, so it is offered as itself and its own acceptability
/// decides -- and the line about selling a container's contents is not printed.
pub(super) fn an_empty_pack_is_offered_as_itself() {
    let mut c = a_client_at_the_shop_selling();
    let really_empty = c
        .view()
        .world()
        .inventory(SACK)
        .is_none_or(|i| i.items.is_empty() && i.containers.is_empty());

    let from = sack_tile(&c);
    let target = element_of(&c, SELL_LIST);
    let_the_icon_go_on(&mut c, target, from);

    let offered = basket(&c);
    let said_nothing_about_contents = !feedback(&c)
        .iter()
        .any(|l| l.starts_with("Selling contents of"));

    c.assert_behaviour(
        "vendor.sell.an-empty-pack-is-offered-as-itself",
        move |_| really_empty && offered == vec![SACK] && said_nothing_about_contents,
    );
    c.shutdown();
}

// =============================================================================================
// One store for the player's coin, because there is exactly one.
//
// The player's pyreals are stored in a single place, as in retail: the pack can hold any number
// of stacks of them, and there is one source of truth for the total.
//
// **The recording is unusually sharp here.** Its login description carries the player's coin,
// and the first update to that number does not arrive for another five hundred seconds -- but a
// shop opens twelve blobs before it. So for eight and a half minutes of recorded play the login
// description is the *only* thing that could have told this client what the player was carrying,
// and a client that dropped it opens that shop on nothing.
//
// The client never counts the coin for itself: nothing anywhere adds up what is in the pack. The
// shard sums it and sends the total, and it **suppresses the send when the total has not
// changed** -- so a client that drops one never gets a spontaneous correction.
// =============================================================================================

/// The recording's **first** shop, and the last.
const FIRST_SHOP_AT: usize = 4152;
const LAST_SHOP_AT: usize = 4784;
/// What the recording's login description says the player is carrying.
const LOGIN_COIN: i32 = 10_000;
/// The number the shard put on the last coin update of the recording, and its own counter.
const SEQ_OF_THE_SALE: u8 = 5;
/// The coin property, as a number. Never the symbol the code under test reads it through.
const COIN_PROPERTY: u32 = 20;

/// The recording, replayed as far as its **first** shop, with the pack page up.
fn a_client_at_the_first_shop() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.world_mut().player = Some(RECORDED_PLAYER);
    c.when(Inbound::from_corpus("long-solo-play", 0..FIRST_SHOP_AT + 1))
        .tick(4);
    open_pack(&mut c);
    c.tick(4);
    assert!(
        c.view().world().shop.is_open(),
        "the premise: the recording's first shop message is at this blob"
    );
    c
}

/// Whether the recording has sent any coin update before `through`.
fn the_shard_has_said_nothing_about_coin_before(through: usize) -> bool {
    let corpus = Corpus::load("long-solo-play")
        .expect("the locked corpus decodes")
        .expect("long-solo-play is locked");
    !corpus.blobs.iter().any(|b| {
        b.dir == dereth_client_net::client_session::testing::Direction::ServerToClient
            && b.idx <= through
            && b.opcode == 0x02CD
            && b.payload.len() >= 13
            && u32::from_le_bytes(b.payload[5..9].try_into().unwrap_or_default()) == COIN_PROPERTY
    })
}

/// How the shop writes what the player has.
fn purse_line(coin: i32) -> String {
    format!(
        "You have {}p",
        dereth_ui_screens::panels::examination::insert_commas(coin)
    )
}

/// One coin update for the player, in the public form, carrying `seq` as its own counter.
fn the_shard_says_the_player_has(seq: u8, value: i32) -> Inbound {
    let mut p = Vec::with_capacity(17);
    p.extend_from_slice(&0x02CE_u32.to_le_bytes());
    p.push(seq);
    p.extend_from_slice(&RECORDED_PLAYER.0.to_le_bytes());
    p.extend_from_slice(&COIN_PROPERTY.to_le_bytes());
    p.extend_from_slice(&value.to_le_bytes());
    Inbound::event(dereth_client_net::client_session::SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode(0x02CE),
        blob: p,
    })
}

// ---------------------------------------------------------------------------------------------
// vendor.purse.the-first-recorded-shop-opens-on-the-purse-the-login-description-carried
// ---------------------------------------------------------------------------------------------

/// The first shop of the recording opens on the purse the **login description** carried, and the
/// same number appears in every place the shop says what the player has.
///
/// The premise that makes this a statement about the login description and not about anything
/// else is asserted from the recording itself: the shard has said nothing about the player's
/// coin at this point, so there is no other source the number could have come from.
///
/// Three places, reached three ways: the two purse lines the window writes when it opens, and
/// the picked row's cost line, which is written by a different call with the same number as its
/// trailing argument.
pub(super) fn the_first_recorded_shop_opens_on_the_purse_the_login_description_carried() {
    let nothing_said_yet = the_shard_has_said_nothing_about_coin_before(FIRST_SHOP_AT);

    let mut c = a_client_at_the_first_shop();
    let want = purse_line(LOGIN_COIN);
    let the_model_has_it = c.view().world().shop.total_value == LOGIN_COIN;
    let both_lines =
        text_of(&mut c, BUY_PURSE_TEXT) == want && text_of(&mut c, SELL_PURSE_TEXT) == want;

    // The third place, on the picked row's own line.
    let (handle, item) = *stock_slots(&c)
        .first()
        .expect("the recorded shopkeeper advertises stock");
    press(&mut c, handle);
    let picked = c.view().world().selected == Some(item);
    let on_the_cost_line = text_of(&mut c, ITEM_COST_TEXT).contains(&format!(
        "(you have {}p)",
        dereth_ui_screens::panels::examination::insert_commas(LOGIN_COIN)
    ));

    c.assert_behaviour(
        "vendor.purse.the-first-recorded-shop-opens-on-the-purse-the-login-description-carried",
        move |_| nothing_said_yet && the_model_has_it && both_lines && picked && on_the_cost_line,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// vendor.purse.a-purse-that-came-only-from-the-login-can-buy-what-it-can-afford
// ---------------------------------------------------------------------------------------------

/// A purse whose only source is the login description can actually buy what it can afford.
///
/// This is the less obvious half of the failure: the shop's money test is a plain comparison
/// against that number, so losing it does not merely mis-draw a label -- it refuses every purchase
/// however rich the player is. The premises are the recorded shop's own: its first row costs
/// something, and that something is less than what the player is carrying.
pub(super) fn a_purse_that_came_only_from_the_login_can_buy_what_it_can_afford() {
    let c = a_client_at_the_first_shop();
    let world = c.view().world();
    let row = world
        .shop
        .stock
        .first()
        .expect("the recorded shopkeeper has stock")
        .clone();
    let price = world.shop.profile.vendor_sell_price(&row.pwd, 1);
    let it_costs_something = price > 0;
    let and_he_can_afford_it = price < LOGIN_COIN;
    let the_shop_agrees = world.shop.total_value >= price;

    let mut c = c;
    c.assert_behaviour(
        "vendor.purse.a-purse-that-came-only-from-the-login-can-buy-what-it-can-afford",
        move |_| it_costs_something && and_he_can_afford_it && the_shop_agrees,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// vendor.purse.a-stale-coin-update-for-the-player-is-dropped-by-the-same-counter
// ---------------------------------------------------------------------------------------------

/// The two forms a coin update arrives in -- the one addressed to the player and the one that
/// leaves the player implied -- share **one** counter, so an update older than the last one
/// already taken is dropped, and a newer one lands.
///
/// That is the sentence "there is one store" made falsifiable: two stores, each with a counter
/// of its own, would both have accepted the stale update. The shop is re-opened with the
/// recording's own last shop message between the update and the reading, so what is measured is
/// the **store** and not a number the window was already holding.
pub(super) fn a_stale_coin_update_for_the_player_is_dropped_by_the_same_counter() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.world_mut().player = Some(RECORDED_PLAYER);
    c.when(Inbound::from_corpus(
        "long-solo-play",
        0..AFTER_THE_SALE_COIN_AT + 5,
    ))
    .tick(4);
    open_pack(&mut c);
    c.tick(4);
    let the_last_word = c.view().world().shop.total_value == COIN_AFTER_SALE;

    let reopen = || Inbound::from_corpus("long-solo-play", LAST_SHOP_AT..LAST_SHOP_AT + 1);

    c.when(the_shard_says_the_player_has(SEQ_OF_THE_SALE - 1, 1))
        .tick(1);
    c.when(reopen()).tick(3);
    let the_stale_one_was_dropped = text_of(&mut c, BUY_PURSE_TEXT) == purse_line(COIN_AFTER_SALE)
        && c.view().world().shop.total_value == COIN_AFTER_SALE;

    c.when(the_shard_says_the_player_has(SEQ_OF_THE_SALE + 1, 12_345))
        .tick(1);
    c.when(reopen()).tick(3);
    let the_newer_one_landed = text_of(&mut c, BUY_PURSE_TEXT) == purse_line(12_345)
        && c.view().world().shop.total_value == 12_345;

    c.assert_behaviour(
        "vendor.purse.a-stale-coin-update-for-the-player-is-dropped-by-the-same-counter",
        move |_| the_last_word && the_stale_one_was_dropped && the_newer_one_landed,
    );
    c.shutdown();
}
