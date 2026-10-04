use dereth_client_model::{RecordingRequests, RecordingSink, Request, World};
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_primitives::{LocalTime, ObjectId};
use dereth_testkit::HeadlessClient;
use dereth_ui::framework::Screen as _;
use dereth_ui::{ElemHandle, ElementMessage};
use dereth_ui_screens::panels::vendor::{
    Tab, VendorPanel, PAGE_BUY, PAGE_ITEMS, TABS, TYPE_FILTER_MENU,
};
use dereth_ui_screens::view::{ShopView, UiRequest};

use super::{
    a_client_at_the_shop, a_gameplay_client, bound_vendor, centre, clear_requests, click,
    click_shop_tab, element_of, feedback, gameplay_root, gameplay_screen, press, pump, sellable,
    stock_slots, take_requests, the_recorded_grocer, Shop, BTN_SELL_ALL, SELL_LIST, SELL_TAB,
    SHOPKEEPER,
};

// -----------------------------------------------------------------------------------------
// The kinds a thing can be, as the shipped strip names them. Written as literals rather than
// read back through the client's own symbol, for the reason the money lines are: a scenario
// that read the constant it is checking could not notice a wrong constant.
// -----------------------------------------------------------------------------------------
const FOOD: u32 = 0x0000_0020;
const ARMOUR: u32 = 0x0000_0002;
const CONTAINER: u32 = 0x0000_0200;
const SPELL_COMPONENT: u32 = 0x0000_1000;

/// The sentence the client prints when a drag out of a panel that cannot split one is made
/// with less than the whole stack dialled in.
const CANNOT_SPLIT: &str = "You cannot split items from this panel";

// =========================================================================================
// f33: the window the shard's own message put on screen.
// =========================================================================================

/// The `(slot, picture, thing)` triples the live shop window is showing.
fn shop_slots(c: &HeadlessClient) -> Vec<(ElemHandle, Option<ElemHandle>, ObjectId)> {
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
        .filter_map(|s| s.item.map(|id| (s.handle, s.icon, id)))
        .collect()
}

/// Whether a live slot's picture is the finished one -- a plate under an icon -- rather than
/// the bare picture the row was filled from.
fn draws_finished(c: &HeadlessClient, icon: Option<ElemHandle>) -> bool {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the UI shell").ui;
    icon.and_then(|h| ui.node(h))
        .and_then(|n| n.region.image.as_ref().cloned())
        .and_then(|i| i.op)
        .and_then(dereth_ui::region::SurfaceOp::icon_recipe)
        .is_some_and(|r| {
            matches!(
                r,
                dereth_ui::region::IconRecipe::Object {
                    background: Some(_),
                    icon: Some(_),
                    ..
                }
            )
        })
}

/// The release of a drag whose owner is `owner`, over `target`. This is the message the drag
/// manager raises and the one the pack, the box and the trade table already answer.
fn let_go_over(c: &mut HeadlessClient, target: ElemHandle, owner: ElemHandle) {
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
    let _ = pump(c);
}

/// A real pointer click on a live element, through the pointer the player uses.
fn pointer_click(c: &mut HeadlessClient, h: ElemHandle, now: f64) -> Vec<Request> {
    let (x, y) = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        centre(ui, h)
    };
    {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.mouse_move(LocalTime(now), x, y);
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    }
    pump(c)
}

/// The row of the shop's own counter that is holding `item`.
fn counter_row(c: &HeadlessClient, item: ObjectId) -> ElemHandle {
    c.view()
        .expect_app()
        .hud()
        .panels
        .vendor
        .sell
        .as_ref()
        .expect("the shop's counter is bound")
        .slots
        .iter()
        .find(|s| s.item == Some(item))
        .unwrap_or_else(|| panic!("the counter has no row for {item:?}"))
        .handle
}

/// Press on a row and move far enough that the drag really starts.
fn drag_off(c: &mut HeadlessClient, row: ElemHandle, now: f64) {
    let (x, y) = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        centre(ui, row)
    };
    let (ui, _screen) = gameplay_screen(c.app_mut());
    ui.mouse_move(LocalTime(now), x, y);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    ui.mouse_move(LocalTime(now + 0.05), x + 8, y + 8);
    c.tick(1);
}

/// What the icon in the air is carrying, and whether it knows it came out of a shop.
fn what_is_in_the_air(c: &mut HeadlessClient) -> (Option<ObjectId>, bool) {
    let (ui, _screen) = gameplay_screen(c.app_mut());
    let proxy = ui.drag_state().element.expect("the row started a drag");
    let info = dereth_ui_screens::items::widget::inq_drop_icon_info(ui, proxy);
    let from_a_shop = info.flags & dereth_ui_screens::items::widget::drag_flags::IS_VENDOR
        == dereth_ui_screens::items::widget::drag_flags::IS_VENDOR;
    (info.item, from_a_shop)
}

/// The quantity dial the toolbar carries, moved to its bottom, which is one of the stack.
fn dial_down_to_one(c: &mut HeadlessClient) {
    let slider = element_of(c, dereth_ui_screens::toolbar::splitter::SLIDER);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell")
        .ui
        .broadcast_element_message(slider, dereth_ui::msg::element::id::SCROLL_POSITION, 0, 0);
    c.tick(1);
}

fn splitter(c: &mut HeadlessClient) -> dereth_ui_screens::toolbar::splitter::Splitter {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen.splitter
}

/// Every row a shop advertises is a thing the client really holds, and every filled slot of
/// the live window draws the finished picture.
///
/// The premise is the recorded shop being open with stock in it -- that is the world the
/// shard built. The claim is both halves together: the rows are objects, which is why the
/// decoration can be computed at all, and the decoration is on the live element.
pub fn every_row_of_the_shop_is_a_real_thing_and_is_drawn_finished() {
    let mut c = a_client_at_the_shop();

    let (she_stocks_things, every_row_is_held) = {
        let w = c.view().world();
        (
            !w.shop.stock.is_empty(),
            w.shop.stock.iter().all(|p| w.weenie(p.iid).is_some()),
        )
    };

    let slots = shop_slots(&c);
    let the_window_shows_them = !slots.is_empty();
    let all_finished = slots.iter().all(|(_, icon, _)| draws_finished(&c, *icon));

    c.assert_behaviour(
        "vendor.stock.every-row-of-the-shop-is-a-real-thing-and-is-drawn-finished",
        move |_| she_stocks_things && every_row_is_held && the_window_shows_them && all_finished,
    );
    c.shutdown();
}

/// Pressing a stock row picks it.
///
/// The **last** row and not the first: opening the shop already picked the first one, so a
/// scenario that pressed row zero would pass against a client with no press handler at all.
/// What is picked is cleared first for the same reason.
pub fn pressing_a_row_in_the_shops_own_window_picks_it() {
    let mut c = a_client_at_the_shop();
    let (handle, item) = *stock_slots(&c)
        .last()
        .expect("the live stock list holds a row");

    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_selected_object(None, false, &mut RecordingSink::default());
    c.tick(1);
    let nothing_picked_yet = c.view().world().selected != Some(item);

    press(&mut c, handle);
    let the_press_picked_it = c.view().world().selected == Some(item);

    c.assert_behaviour(
        "vendor.stock.pressing-a-row-in-the-shops-own-window-picks-it",
        move |_| nothing_picked_yet && the_press_picked_it,
    );
    c.shutdown();
}

/// Letting something go on the shop's own counter offers it, marks it in the pack, and
/// sell-everything puts it on the wire.
///
/// Nothing leaves this process: what is read is the client's own outbox.
pub fn a_drop_on_the_shops_own_sell_window_offers_it_and_sell_everything_sends_it() {
    let mut c = a_client_at_the_shop();
    let (owner, item) = sellable(&c);
    let target = element_of(&c, SELL_LIST);
    let the_counter_was_empty = c.view().world().shop.sell_list.is_empty();

    let_go_over(&mut c, target, owner);
    let it_is_on_the_counter = c
        .view()
        .world()
        .shop
        .sell_list
        .iter()
        .any(|(id, _)| *id == item);
    let it_is_marked = c
        .view()
        .world()
        .weenie(item)
        .expect("the thing let go")
        .sell_state
        == 1;

    let sent = click(&mut c, BTN_SELL_ALL);
    let it_reached_the_wire = sent.iter().any(|r| {
        matches!(r, Request::VendorSell(m)
                if m.vendor_id == SHOPKEEPER && m.items.iter().any(|i| i.iid == item))
    });

    c.assert_behaviour(
        "vendor.sell.a-drop-on-the-shops-own-sell-window-offers-it-and-sell-everything-sends-it",
        move |_| {
            the_counter_was_empty && it_is_on_the_counter && it_is_marked && it_reached_the_wire
        },
    );
    c.shutdown();
}

/// A shop's stock is bought, never used -- through both of the gestures that could use it.
pub fn what_is_on_the_shops_shelf_is_bought_and_never_used() {
    // (a) the double click on the row itself.
    let mut c = a_client_at_the_shop();
    let (row, item) = *stock_slots(&c).last().expect("a filled stock row");
    let it_is_a_shops_list = c
        .view()
        .expect_app()
        .hud()
        .panels
        .vendor
        .stock
        .as_ref()
        .expect("the stock list")
        .vendor_list;
    c.app_mut()
        .ui_mut()
        .expect("the UI shell")
        .ui
        .broadcast_element_message(
            row,
            dereth_ui::msg::element::id::MOUSE_PRESS,
            // The left **double** click, which is the arm that would otherwise use a thing.
            0x0A,
            0,
        );
    let sent = pump(&mut c);
    let the_double_click_used_nothing = !sent
        .iter()
        .any(|r| matches!(r, Request::UseEvent(m) if m.object == item));
    c.shutdown();

    // (b) picking the row and then pressing the toolbar's own use button, which is a
    // different gate entirely.
    let mut c = a_client_at_the_shop();
    let (row, item) = *stock_slots(&c).last().expect("a filled stock row");
    let it_belongs_to_the_shop = c
        .view()
        .world()
        .weenie(item)
        .and_then(|w| w.pwd.container_id)
        == Some(SHOPKEEPER);

    let _ = pointer_click(&mut c, row, 1.0);
    let the_row_is_picked = c.view().world().selected == Some(item);

    let use_button = element_of(&c, dereth_ui_screens::toolbar::target_mode::USE_BUTTON);
    let the_button_is_the_pointer_target = {
        let at = {
            let (ui, _screen) = gameplay_screen(c.app_mut());
            centre(ui, use_button)
        };
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell")
            .ui
            .hit_test_screen(at.0, at.1)
            == Some(use_button)
    };
    let sent = pointer_click(&mut c, use_button, 2.0);
    let the_toolbar_used_nothing = !sent
        .iter()
        .any(|r| matches!(r, Request::UseEvent(m) if m.object == item));

    c.assert_behaviour(
        "vendor.stock.what-is-on-the-shops-shelf-is-bought-and-never-used",
        move |_| {
            it_is_a_shops_list
                && the_double_click_used_nothing
                && it_belongs_to_the_shop
                && the_row_is_picked
                && the_button_is_the_pointer_target
                && the_toolbar_used_nothing
        },
    );
    c.shutdown();
}

/// Part of a stack let go on the counter is split off and that part is what is offered.
///
/// The dial is driven the way a player drives it: the stack is picked with a real press on
/// its own pack slot, which is what seeds the dial's top, and the amount is moved with a real
/// message on the toolbar's own slider. The drop asks the shard for the split, beside the
/// stack in its own container, and holds the row with the stack until the shard's new object
/// arrives; the object arriving here is the one the shard would make.
pub fn part_of_a_stack_let_go_on_the_sell_window_is_split_off_and_offered() {
    let mut c = a_client_at_the_shop();
    let (owner, item) = sellable(&c);
    let target = element_of(&c, SELL_LIST);
    let (it_is_a_stack, container, wcid) = {
        let w = c.view().world().weenie(item).expect("the pack item");
        (
            w.pwd.stack_size.unwrap_or(0) > 1,
            w.pwd.container_id.expect("carried"),
            w.pwd.wcid,
        )
    };

    press(&mut c, owner);
    let the_stack_is_picked = c.view().world().selected == Some(item);
    let the_dial_took_the_stack = splitter(&mut c).max_split_size > 1;

    dial_down_to_one(&mut c);
    let split = splitter(&mut c);
    let the_dial_is_off_the_whole = split.split_size < split.max_split_size;

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
    let sent = pump(&mut c);
    let the_split_was_asked_for = sent.iter().any(|r| {
        matches!(r, Request::StackableSplitToContainer(m)
                if m.stack == item && m.container == container && m.amount == 1)
    });
    let the_stack_holds_the_row = c.view().world().shop.sell_list == vec![(item, 1)];

    // The shard's answer: a new object of the same class, one in the stack, beside it.
    let part = ObjectId(0x8000_7777);
    {
        let core = &mut **c.app_mut();
        let w = &mut core.objects.world;
        let mut new = dereth_client_model::Weenie::new(part);
        new.pwd = w.weenie(item).expect("the source").pwd.clone();
        new.pwd.stack_size = Some(1);
        new.valid = true;
        w.tables.weenies.insert(part, new);
        if let Some(inv) = w.tables.inventories.get_mut(container) {
            inv.items.push(part);
        }
        core.interaction.apply_object_notices(
            w,
            vec![dereth_client_model::Notice::ItemAttributesChanged {
                object: part,
                kind: 1,
            }],
        );
    }
    let _ = pump(&mut c);
    let w = c.view().world();
    let the_part_took_the_row = w.shop.sell_list == vec![(part, 1)];
    let the_part_is_marked = w.weenie(part).is_some_and(|x| x.sell_state == 1);
    let the_stack_is_not = w.weenie(item).is_some_and(|x| x.sell_state == 0);
    let same_class = w.weenie(part).map(|x| x.pwd.wcid) == Some(wcid);

    c.assert_behaviour(
        "vendor.sell.part-of-a-stack-let-go-on-the-sell-window-is-split-off-and-offered",
        move |_| {
            it_is_a_stack
                && the_stack_is_picked
                && the_dial_took_the_stack
                && the_dial_is_off_the_whole
                && the_split_was_asked_for
                && the_stack_holds_the_row
                && the_part_took_the_row
                && the_part_is_marked
                && the_stack_is_not
                && same_class
        },
    );
    c.shutdown();
}

/// Dragging a row back out of the counter is a withdrawal and not a sale, and the two amounts
/// the dial can be at are the two arms of the same rule.
pub fn taking_a_row_back_out_of_the_window_is_a_withdrawal_and_not_a_sale() {
    // ---- the partial arm: the amount is put back and the player is told why.
    let mut c = a_client_at_the_shop();
    let (pack_row, item) = sellable(&c);
    let target = element_of(&c, SELL_LIST);

    press(&mut c, pack_row);
    let whole = splitter(&mut c).max_split_size;
    let it_is_a_stack = whole > 1;

    let_go_over(&mut c, target, pack_row);
    let it_is_on_the_counter = c
        .view()
        .world()
        .shop
        .sell_list
        .iter()
        .any(|(id, _)| *id == item);
    click_shop_tab(&mut c, SELL_TAB);

    // A press and release that never moves is not a drag, and must take nothing off.
    let row = counter_row(&c, item);
    let _ = pointer_click(&mut c, row, 10.0);
    let a_still_press_takes_nothing_off = c
        .view()
        .world()
        .shop
        .sell_list
        .iter()
        .any(|(id, _)| *id == item);

    dial_down_to_one(&mut c);
    let the_dial_is_at_one =
        splitter(&mut c).split_size == 1 && c.view().world().split.split_size == 1;

    let row = counter_row(&c, item);
    drag_off(&mut c, row, 11.0);
    let (carried, out_of_a_shop) = what_is_in_the_air(&mut c);
    let the_row_is_in_the_air = carried == Some(item) && out_of_a_shop;
    c.tick(2);

    let it_left_the_counter = c
        .view()
        .world()
        .shop
        .sell_list
        .iter()
        .all(|(id, _)| *id != item);
    let the_mark_came_off = c.view().world().weenie(item).expect("the thing").sell_state == 0;
    let the_amount_was_put_back =
        splitter(&mut c).split_size == whole && c.view().world().split.split_size == whole;
    let the_player_was_told = feedback(&c).iter().any(|l| l == CANNOT_SPLIT);
    let nothing_was_sold = !c
        .view()
        .expect_app()
        .interaction()
        .last_sent
        .iter()
        .any(|r| matches!(r, Request::VendorSell(_)));
    c.shutdown();

    // ---- the whole arm, which is the directly adjacent branch: the offer still comes back,
    // with no line and no change to the amount.
    let mut c = a_client_at_the_shop();
    let (pack_row, item) = sellable(&c);
    let target = element_of(&c, SELL_LIST);
    press(&mut c, pack_row);
    let dial = splitter(&mut c);
    let the_dial_is_at_the_whole = dial.split_size == dial.max_split_size;
    let_go_over(&mut c, target, pack_row);
    click_shop_tab(&mut c, SELL_TAB);

    let row = counter_row(&c, item);
    drag_off(&mut c, row, 20.0);
    let (carried, out_of_a_shop) = what_is_in_the_air(&mut c);
    let the_whole_row_is_in_the_air = carried == Some(item) && out_of_a_shop;
    c.tick(2);

    let it_left_the_counter_too = c
        .view()
        .world()
        .shop
        .sell_list
        .iter()
        .all(|(id, _)| *id != item);
    let the_mark_came_off_too = c.view().world().weenie(item).expect("the thing").sell_state == 0;
    let the_amount_was_left_alone = splitter(&mut c) == dial;
    let no_line_this_time = feedback(&c).iter().all(|l| l != CANNOT_SPLIT);

    c.assert_behaviour(
        "vendor.sell.taking-a-row-back-out-of-the-window-is-a-withdrawal-and-not-a-sale",
        move |_| {
            it_is_a_stack
                && it_is_on_the_counter
                && a_still_press_takes_nothing_off
                && the_dial_is_at_one
                && the_row_is_in_the_air
                && it_left_the_counter
                && the_mark_came_off
                && the_amount_was_put_back
                && the_player_was_told
                && nothing_was_sold
                && the_dial_is_at_the_whole
                && the_whole_row_is_in_the_air
                && it_left_the_counter_too
                && the_mark_came_off_too
                && the_amount_was_left_alone
                && no_line_this_time
        },
    );
    c.shutdown();
}

// =========================================================================================
// o650 and o740: the strip of categories, and what happens between the category and the row.
//
// These drive the shipped shop window bound to the live element tree, with the shop snapshot
// the client's own seam builds out of a recorded shop message. The message is the shard's;
// where a scenario needs a supply or a container the recordings do not carry, only the
// *message* is rewritten and everything from the client's own handler down is production.
// =========================================================================================

/// A view answering one shop snapshot and nothing else.
fn a_view(s: &ShopView) -> Shop {
    Shop(s.clone())
}

/// The recorded grocer's own world: the shop message that sells food and no armour, handled
/// by the client's own handler.
fn a_shop_that_sells_food() -> World {
    a_shop_that_sells_food_with(|_| {})
}

/// The same, with the shop's own message rewritten before the client sees it.
fn a_shop_that_sells_food_with(edit: impl Fn(&mut dereth_protocol::trade::VendorInfo)) -> World {
    let mut info = the_recorded_grocer();
    edit(&mut info);
    let mut w = World::new();
    w.handle_vendor_info(
        &info,
        &mut RecordingSink::default(),
        &mut RecordingRequests::default(),
        dereth_primitives::ServerTime(0.0),
    );
    w
}

/// Every shop message the recordings carry, decoded. The recordings are searched by what the
/// message *says* rather than by an index, so nothing here goes stale as they grow.
fn every_recorded_shop() -> Vec<dereth_protocol::trade::VendorInfo> {
    let mut out = Vec::new();
    for corpus in Corpus::load_all() {
        for b in &corpus.blobs {
            if b.dir != Direction::ServerToClient || b.opcode != 0xF7B0 || b.payload.len() < 16 {
                continue;
            }
            let sub =
                u32::from_le_bytes([b.payload[12], b.payload[13], b.payload[14], b.payload[15]]);
            if sub != 0x0062 {
                continue;
            }
            if let Ok(m) =
                dereth_protocol::read_body::<dereth_protocol::trade::VendorInfo>(&b.payload[16..])
            {
                out.push(m);
            }
        }
    }
    out
}

/// The one recorded shop that advertises a **countable** supply of something, and that row.
/// Found by that property rather than by an index.
fn a_recorded_shop_with_a_countable_supply() -> (World, ObjectId) {
    let info = every_recorded_shop()
        .into_iter()
        .find(|m| m.items.iter().any(|p| p.amount != -1 && p.pwd.is_some()))
        .expect("a recorded shop message advertising a countable supply");
    let row = info
        .items
        .iter()
        .find(|p| p.amount != -1 && p.pwd.is_some())
        .map(|p| p.iid)
        .expect("that row");
    let mut w = World::new();
    w.handle_vendor_info(
        &info,
        &mut RecordingSink::default(),
        &mut RecordingRequests::default(),
        dereth_primitives::ServerTime(0.0),
    );
    (w, row)
}

/// Give `id` some contents, which is what the containment gate counts.
fn put_inside(w: &mut World, id: ObjectId, items: &[ObjectId], containers: &[ObjectId]) {
    w.tables.inventories.insert(
        id,
        dereth_client_model::objects::ObjectInventory {
            container: id,
            items: items.to_vec(),
            containers: containers.to_vec(),
            placements: Vec::new(),
        },
    );
}

/// One of the shop's food rows.
fn a_food_row(w: &World) -> ObjectId {
    w.shop
        .stock
        .iter()
        .find(|p| p.pwd.obj_type & FOOD != 0)
        .map(|p| p.iid)
        .expect("the recorded grocer sells food")
}

/// What the live list is holding, read off its slots and not off the panel's own record.
fn on_the_shelf(p: &VendorPanel) -> Vec<ObjectId> {
    p.stock
        .as_ref()
        .expect("the shelf")
        .slots
        .iter()
        .filter_map(|s| s.item)
        .collect()
}

/// Which category covers `mask`, in the strip the open built.
fn category_for(view: &ShopView, mask: u32) -> usize {
    view.type_filters
        .iter()
        .position(|(_, m)| *m & mask != 0)
        .expect("the strip has a category covering this kind")
}

/// How many times the strip has said a category was chosen since the last drain.
fn choices(ui: &mut dereth_ui::UiSystem) -> usize {
    ui.drain_outbox()
        .into_iter()
        .filter(|d| match d {
            dereth_ui::Delivery::Element { msg, .. } => {
                msg.id == dereth_ui::msg::element::id::MENU_CHOSEN
                    && msg.source_id == TYPE_FILTER_MENU
            }
            _ => false,
        })
        .count()
}

/// Drive one category of the strip the way a player does, and leave the live list holding
/// whatever that choice left on it.
///
/// The open's own refill has already run by the time this returns from its first line -- that
/// is the client's own opening and it is production, not setup -- so the counters are zeroed
/// and what follows is one pass, the player's.
fn choose_the_category(c: &mut HeadlessClient, p: &mut VendorPanel, view: &ShopView, tab: usize) {
    let (ui, _root) = gameplay_root(c.app_mut());
    assert!(
        p.update(ui, &a_view(view)),
        "the first drive is always a redraw"
    );
    p.basket_drops = 0;
    p.container_drops = 0;
    p.stack_size_writes = 0;
    p.scroll_restores = 0;
    let menu = p.type_menu.expect("the strip of categories");
    let row = dereth_ui::widgets::menu::get_item(ui, menu, tab).expect("a category row");
    let _ = ui.drain_outbox();
    dereth_ui::widgets::menu::set_selected_item(ui, menu, Some(row), true);
    let msgs: Vec<ElementMessage> = ui
        .drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            dereth_ui::Delivery::Element { msg, .. } => Some(msg),
            _ => None,
        })
        .filter(|m| {
            m.id == dereth_ui::msg::element::id::MENU_CHOSEN && m.source_id == TYPE_FILTER_MENU
        })
        .collect();
    assert!(!msgs.is_empty(), "the premise: choosing a category says so");
    clear_requests(&mut ui.requests);
    let before = p.filter_applications;
    assert!(
        p.on_element_message(ui, &msgs[0], &a_view(view)),
        "the shop owns that message"
    );
    assert_eq!(
        p.filter_applications,
        before + 1,
        "...and the filtering arm really ran"
    );
}

/// Choosing a category leaves exactly the things of that kind, and moves the pick to the
/// first of them.
pub fn choosing_a_category_leaves_exactly_the_things_of_that_kind() {
    let w = a_shop_that_sells_food();
    let view = dereth_client::vendor_view::shop(&w);
    assert!(
        view.open && !view.stock.is_empty(),
        "the premise: a recorded shop with stock"
    );
    assert!(
        view.type_filters.len() >= 2,
        "the premise: at least two categories, or there is no differential to take"
    );

    let mut c = a_gameplay_client();
    // The opening, on a window of its own: the strip the shop built is its own, and the
    // opening has already chosen the first category rather than showing everything.
    let mut opened = bound_vendor(c.app_mut());
    let strip_is_the_shops_own = {
        let (ui, _root) = gameplay_root(c.app_mut());
        assert!(
            opened.update(ui, &a_view(&view)),
            "the first drive is always a redraw"
        );
        dereth_ui::widgets::menu::num_items(ui, opened.type_menu.expect("the strip"))
            == view.type_filters.len()
    };
    let it_opens_on_the_first_category = opened.last_mask == Some(view.type_filters[0].1);

    let mut every_category_filters = true;
    let mut every_category_picks_the_first = true;
    let mut kept: Vec<usize> = Vec::new();
    for (i, (_, mask)) in view.type_filters.iter().enumerate() {
        // A window of its own per category, so that each pass is an opening followed by one
        // choice and never a second choice on a shelf the last one left.
        let (p, _) = shelf_after_choosing(&mut c, &view, i);
        let want: Vec<ObjectId> = view
            .stock
            .iter()
            .filter(|r| r.obj_type & *mask != 0)
            .map(|r| r.item)
            .collect();
        assert!(
            !want.is_empty(),
            "the premise: a category exists because something is of it"
        );
        every_category_filters &= p.last_mask == Some(*mask)
            && on_the_shelf(&p) == want
            && p.rows(Tab::Items)
                .iter()
                .map(|r| r.item)
                .collect::<Vec<_>>()
                == want;
        let reqs = take_requests(c.ui_outbox());
        every_category_picks_the_first &= reqs.len() >= 2
            && reqs.first() == Some(&UiRequest::VendorFilter(i))
            && reqs.last() == Some(&UiRequest::Select(want[0]))
            && reqs[1..reqs.len() - 1].iter().all(|r| {
                matches!(r, UiRequest::VendorSetObjectStackSize { item, size }
                        if want.contains(item) && *size > 1)
            });
        kept.push(want.len());
    }
    // Without this the loop above is satisfied by a shop whose every category happens to
    // cover everything, which is no filtering at all.
    let at_least_one_is_a_shorter_list = kept.iter().any(|n| *n < view.stock.len());

    c.assert_behaviour(
        "vendor.filter.choosing-a-category-leaves-exactly-the-things-of-that-kind",
        move |_| {
            strip_is_the_shops_own
                && it_opens_on_the_first_category
                && every_category_filters
                && every_category_picks_the_first
                && at_least_one_is_a_shorter_list
        },
    );
    c.shutdown();
}

/// Every category is a row the player can actually reach with the pointer, and pressing one
/// replaces what is on the shelf.
///
/// Two categories first, because a popup of two is where a second row gets clipped away under
/// the first; then three, so the answer is not about that shape alone.
pub fn every_category_is_a_row_the_player_can_actually_click() {
    let full = dereth_client::vendor_view::shop(&a_shop_that_sells_food());
    let pair = full
        .type_filters
        .iter()
        .enumerate()
        .flat_map(|(i, a)| full.type_filters.iter().skip(i + 1).map(move |b| (*a, *b)))
        .find(|((_, a), (_, b))| {
            let aa: Vec<_> = full
                .stock
                .iter()
                .filter(|r| r.obj_type & *a != 0)
                .map(|r| r.item)
                .collect();
            let bb: Vec<_> = full
                .stock
                .iter()
                .filter(|r| r.obj_type & *b != 0)
                .map(|r| r.item)
                .collect();
            !aa.is_empty() && !bb.is_empty() && aa != bb
        })
        .expect("the recorded shop supplies two categories whose things differ");
    let mut view = full.clone();
    view.type_filters = vec![pair.0, pair.1];
    view.stock
        .retain(|r| r.obj_type & (pair.0 .1 | pair.1 .1) != 0);

    let mut c = a_gameplay_client();
    let mut p = bound_vendor(c.app_mut());
    let (ui, root) = gameplay_root(c.app_mut());
    assert!(p.update(ui, &a_view(&view)));
    let env = ui
        .get_child_recursive(
            root,
            dereth_ui_screens::screens::gameplay::window::ENV_PANEL,
        )
        .expect("the window that hosts the shop");
    ui.set_visible(env, true);
    ui.bring_to_front(env);
    let menu = p.type_menu.expect("the strip");
    let popup = dereth_ui::widgets::menu::popup_handle(ui, menu).expect("the strip's popup");
    let two_rows = dereth_ui::widgets::menu::num_items(ui, menu) == 2;
    let first_drawn = on_the_shelf(&p);

    // The player's own click on the face of the strip.
    let mb = ui.screen_box(menu);
    let at = ((mb.x0 + mb.x1) / 2, (mb.y0 + mb.y1) / 2);
    let the_strip_is_the_pointer_target = ui.hit_test_screen(at.0, at.1) == Some(menu);
    let _ = ui.drain_outbox();
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1, false);
    let the_click_opened_it = ui.node(popup).expect("the popup").region.flags.visible;

    let rows: Vec<ElemHandle> = (0..2)
        .map(|i| dereth_ui::widgets::menu::get_item(ui, menu, i).expect("a category row"))
        .collect();
    let both_are_reachable = rows.iter().copied().all(|row| {
        let b = ui.screen_box(row);
        ui.hit_test_screen((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2) == Some(row)
    });

    let second = ui.screen_box(rows[1]);
    let second_at = ((second.x0 + second.x1) / 2, (second.y0 + second.y1) / 2);
    let _ = ui.drain_outbox();
    ui.mouse_down(
        dereth_ui::focus::action::PRIMARY_CLICK,
        second_at.0,
        second_at.1,
    );
    let chosen = ui
        .drain_outbox()
        .into_iter()
        .find_map(|d| match d {
            dereth_ui::Delivery::Element { msg, .. }
                if msg.id == dereth_ui::msg::element::id::MENU_CHOSEN
                    && msg.source_id == TYPE_FILTER_MENU =>
            {
                Some(msg)
            }
            _ => None,
        })
        .expect("the real row press says a category was chosen");
    let the_shop_took_it = p.on_element_message(ui, &chosen, &a_view(&view));
    ui.mouse_up(
        dereth_ui::focus::action::PRIMARY_CLICK,
        second_at.0,
        second_at.1,
        false,
    );

    let want: Vec<ObjectId> = view
        .stock
        .iter()
        .filter(|r| r.obj_type & pair.1 .1 != 0)
        .map(|r| r.item)
        .collect();
    let the_two_differ = want != first_drawn;
    let the_shelf_followed = p.last_mask == Some(pair.1 .1) && on_the_shelf(&p) == want;
    let the_choice_closed_it = !ui.node(popup).expect("the popup").region.flags.visible;

    // The neighbour: three rows exercise the same placement without the special two-row shape.
    let mut three = full.clone();
    assert!(
        three.type_filters.len() > 2,
        "the premise: the recorded shop has a third"
    );
    three.type_filters.truncate(3);
    let all_three = three.type_filters.iter().fold(0, |all, (_, m)| all | *m);
    three.stock.retain(|r| r.obj_type & all_three != 0);
    assert!(p.update(ui, &a_view(&three)));
    let _ = ui.drain_outbox();
    let three_rows = dereth_ui::widgets::menu::num_items(ui, menu) == 3;

    let mb = ui.screen_box(menu);
    let at = ((mb.x0 + mb.x1) / 2, (mb.y0 + mb.y1) / 2);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1, false);
    let the_neighbour_opened = ui.node(popup).expect("the popup").region.flags.visible;
    let rows: Vec<ElemHandle> = (0..3)
        .map(|i| dereth_ui::widgets::menu::get_item(ui, menu, i).expect("a category row"))
        .collect();
    let all_three_are_reachable = rows.iter().copied().all(|row| {
        let b = ui.screen_box(row);
        ui.hit_test_screen((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2) == Some(row)
    });
    let third = ui.screen_box(rows[2]);
    let third_at = ((third.x0 + third.x1) / 2, (third.y0 + third.y1) / 2);
    let _ = ui.drain_outbox();
    ui.mouse_down(
        dereth_ui::focus::action::PRIMARY_CLICK,
        third_at.0,
        third_at.1,
    );
    let chosen = ui
        .drain_outbox()
        .into_iter()
        .find_map(|d| match d {
            dereth_ui::Delivery::Element { msg, .. }
                if msg.id == dereth_ui::msg::element::id::MENU_CHOSEN
                    && msg.source_id == TYPE_FILTER_MENU =>
            {
                Some(msg)
            }
            _ => None,
        })
        .expect("the third real row says a category was chosen");
    assert!(p.on_element_message(ui, &chosen, &a_view(&three)));
    ui.mouse_up(
        dereth_ui::focus::action::PRIMARY_CLICK,
        third_at.0,
        third_at.1,
        false,
    );
    let third_mask = three.type_filters[2].1;
    let third_want: Vec<ObjectId> = three
        .stock
        .iter()
        .filter(|r| r.obj_type & third_mask != 0)
        .map(|r| r.item)
        .collect();
    let the_neighbour_followed = p.last_mask == Some(third_mask) && on_the_shelf(&p) == third_want;

    c.assert_behaviour(
        "vendor.filter.every-category-is-a-row-the-player-can-actually-click",
        move |_| {
            two_rows
                && the_strip_is_the_pointer_target
                && the_click_opened_it
                && both_are_reachable
                && the_shop_took_it
                && the_two_differ
                && the_shelf_followed
                && the_choice_closed_it
                && three_rows
                && the_neighbour_opened
                && all_three_are_reachable
                && the_neighbour_followed
        },
    );
    c.shutdown();
}

/// Nothing chosen is an empty shelf, and asking for one kind by name never consults the strip.
pub fn a_shop_with_no_category_chosen_shows_nothing() {
    let w = a_shop_that_sells_food();
    let view = dereth_client::vendor_view::shop(&w);
    let mut c = a_gameplay_client();
    let mut p = bound_vendor(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());
    assert!(p.update(ui, &a_view(&view)));
    let the_open_drew_something = !on_the_shelf(&p).is_empty();

    let menu = p.type_menu.expect("the strip");
    dereth_ui::widgets::menu::set_selected_item(ui, menu, None, false);
    let nothing_is_chosen = p.selected_type_filter(ui).is_none();

    let kept = p.update_items_list(ui, &view, 0, false);
    let the_shelf_is_empty = kept == 0 && p.last_mask == Some(0) && on_the_shelf(&p).is_empty();

    // The other arm: a kind asked for by name bypasses the strip entirely.
    let comps: Vec<ObjectId> = view
        .stock
        .iter()
        .filter(|r| r.obj_type & SPELL_COMPONENT != 0)
        .map(|r| r.item)
        .collect();
    #[allow(clippy::cast_possible_wrap)]
    let by_name = p.update_items_list(ui, &view, SPELL_COMPONENT as i32, false);
    let a_named_kind_is_kept =
        by_name == comps.len() && p.last_mask == Some(SPELL_COMPONENT) && on_the_shelf(&p) == comps;

    c.assert_behaviour(
        "vendor.filter.a-shop-with-no-category-chosen-shows-nothing",
        move |_| {
            the_open_drew_something
                && nothing_is_chosen
                && the_shelf_is_empty
                && a_named_kind_is_kept
        },
    );
    c.shutdown();
}

/// The pick moves to the first row only when a shop opens.
pub fn what_is_picked_moves_only_when_a_shop_opens() {
    let w = a_shop_that_sells_food();
    let view = dereth_client::vendor_view::shop(&w);
    assert!(
        view.open && !view.stock.is_empty(),
        "the premise: a recorded shop with stock"
    );

    let mut c = a_gameplay_client();
    let mut p = bound_vendor(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());

    // No shop at all: the shelf is refilled, and nothing is picked -- there was no opening.
    clear_requests(&mut ui.requests);
    let _ = choices(ui);
    assert!(
        p.update(ui, &Shop(ShopView::default())),
        "the first drive is always a redraw"
    );
    let a_closed_shop_refills = p.last_mask == Some(0);
    let a_closed_shop_picks_nothing =
        take_requests(&mut ui.requests).is_empty() && choices(ui) == 0;

    // The opening.
    clear_requests(&mut ui.requests);
    assert!(p.update(ui, &a_view(&view)), "an opening is a new snapshot");
    let mask = p.last_mask.expect("the opening chose the first category");
    let want: Vec<ObjectId> = view
        .stock
        .iter()
        .filter(|r| r.obj_type & mask != 0)
        .map(|r| r.item)
        .collect();
    assert!(
        !want.is_empty(),
        "the premise: the first category keeps a row"
    );
    let the_opening_picks_the_first =
        take_requests(&mut ui.requests) == vec![UiRequest::Select(want[0])] && choices(ui) > 0;

    // An ordinary redraw with the same shop still open.
    let mut later = view.clone();
    later.total_value += 1;
    clear_requests(&mut ui.requests);
    assert!(
        p.update(ui, &a_view(&later)),
        "the snapshot changed, so this is another redraw"
    );
    let a_redraw_picks_nothing =
        p.rebuilds == 3 && take_requests(&mut ui.requests).is_empty() && choices(ui) == 0;
    // ...and it is "did not move the pick" rather than "did not run", which are the two a
    // scenario has to be able to tell apart.
    let a_redraw_still_refills = p.last_mask == Some(mask) && on_the_shelf(&p) == want;

    // Walking straight from one merchant to the next, without either being closed.
    let mut other = view.clone();
    other.vendor = Some(ObjectId(
        view.vendor.expect("the shop names its merchant").0 ^ 1,
    ));
    clear_requests(&mut ui.requests);
    assert!(
        p.update(ui, &a_view(&other)),
        "a different merchant is a different snapshot"
    );
    let a_different_merchant_picks_the_first =
        take_requests(&mut ui.requests) == vec![UiRequest::Select(want[0])] && choices(ui) > 0;

    // A stock refresh from the same merchant retains the current selection.
    let mut refreshed = other.clone();
    assert!(
        refreshed.stock.len() >= 2,
        "the premise: there is a row to drop"
    );
    refreshed.stock.pop();
    clear_requests(&mut ui.requests);
    let _ = choices(ui);
    assert!(
        p.update(ui, &a_view(&refreshed)),
        "different stock is a new snapshot"
    );
    let mask4 = p.last_mask.expect("the refresh refilled");
    let want4: Vec<ObjectId> = refreshed
        .stock
        .iter()
        .filter(|r| r.obj_type & mask4 != 0)
        .map(|r| r.item)
        .collect();
    assert!(
        !want4.is_empty(),
        "the premise: the shortened stock still keeps a row"
    );
    let a_refresh_keeps_the_pick_and_rebuilds_stock =
        take_requests(&mut ui.requests).is_empty() && choices(ui) == 0 && on_the_shelf(&p) == want4;

    // Walking away: the strip is emptied and the pick is left alone.
    clear_requests(&mut ui.requests);
    let _ = choices(ui);
    assert!(
        p.update(ui, &Shop(ShopView::default())),
        "the close is a new snapshot"
    );
    // Six drives, so every one of the five above really was a redraw and not a no-op.
    let walking_away_empties_the_strip = p.rebuilds == 6 && p.num_type_filters == 0;
    let walking_away_picks_nothing = choices(ui) == 0 && take_requests(&mut ui.requests).is_empty();

    c.assert_behaviour(
        "vendor.filter.what-is-picked-moves-only-when-a-shop-opens",
        move |_| {
            a_closed_shop_refills
                && a_closed_shop_picks_nothing
                && the_opening_picks_the_first
                && a_redraw_picks_nothing
                && a_redraw_still_refills
                && a_different_merchant_picks_the_first
                && a_refresh_keeps_the_pick_and_rebuilds_stock
                && walking_away_empties_the_strip
                && walking_away_picks_nothing
        },
    );
    c.shutdown();
}

/// The shelf keeps its empty cells and never has anywhere to scroll to.
pub fn the_list_that_keeps_nothing_still_has_no_room_to_scroll() {
    let w = a_shop_that_sells_food();
    let view = dereth_client::vendor_view::shop(&w);
    let mut c = a_gameplay_client();
    let mut p = bound_vendor(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());
    assert!(p.update(ui, &a_view(&view)));
    let list = p.stock.as_ref().expect("the shelf").handle;
    let the_open_drew_rows = !on_the_shelf(&p).is_empty();
    let the_open_counted_once = p.scroll_restores == 1;
    let it_starts_at_the_top =
        dereth_ui::widgets::listbox::scroll_offset_of(ui, list) == Some((0, 0));

    // A kind no row of this shop carries: the things go, the padding stays.
    let before = p.scroll_restores;
    let kept = p.update_items_list(ui, &view, 0x0000_0001, false);
    let (padding, still_at_the_top, no_travel, counted) = {
        let shelf = p.stock.as_ref().expect("the shelf");
        let width = ui.node(list).expect("a live element").region.box_.width();
        let cells = usize::try_from(width / shelf.cell.0).expect("a positive cell width");
        (
            shelf.max_columns == -1 && shelf.slots.len() == cells,
            shelf.scroll(ui) == (0, 0),
            !dereth_ui::widgets::listbox::set_scroll_offset(ui, list, i32::MAX, i32::MAX),
            p.scroll_restores == before + u32::from(cells != 0),
        )
    };
    let nothing_kept = kept == 0 && on_the_shelf(&p).is_empty();

    // ...and a kind that does keep rows counts too.
    let before = p.scroll_restores;
    let (_, mask) = view.type_filters[0];
    #[allow(clippy::cast_possible_wrap)]
    let m = mask as i32;
    let a_full_list_counts =
        p.update_items_list(ui, &view, m, false) > 0 && p.scroll_restores == before + 1;

    c.assert_behaviour(
        "vendor.filter.the-list-that-keeps-nothing-still-has-no-room-to-scroll",
        move |_| {
            the_open_drew_rows
                && the_open_counted_once
                && it_starts_at_the_top
                && nothing_kept
                && padding
                && still_at_the_top
                && no_travel
                && counted
                && a_full_list_counts
        },
    );
    c.shutdown();
}

/// Coming back to the stock page redraws it without moving the pick, and the buying page does
/// neither.
pub fn coming_back_to_the_stock_page_redraws_it_without_moving_what_is_picked() {
    let w = a_shop_that_sells_food();
    let view = dereth_client::vendor_view::shop(&w);
    let mut c = a_gameplay_client();
    let mut p = bound_vendor(c.app_mut());
    let (ui, _root) = gameplay_root(c.app_mut());
    assert!(p.update(ui, &a_view(&view)));
    let it_opens_on_the_stock = p.open_page(ui) == Some(PAGE_ITEMS);

    let tabs = p.tabs.expect("the shop's own tab strip");
    let page_changed = ElementMessage {
        source_id: TABS,
        source: tabs,
        id: dereth_ui::msg::element::id::TAB_PAGE_CHANGED,
        p1: 0,
        p2: 0,
        point: dereth_ui::msg::MessagePoint::default(),
        serial: 0,
    };
    let before = on_the_shelf(&p);
    clear_requests(&mut ui.requests);
    let the_shop_owns_it = p.on_element_message(ui, &page_changed, &a_view(&view));
    let the_same_rows_came_back = p.tab == Tab::Items && on_the_shelf(&p) == before;
    let the_pick_did_not_move = take_requests(&mut ui.requests).is_empty();

    // The same message with the buying page open: the shelf is not that page's business.
    // Opening it the way a click does, which is the gesture and not a poke.
    let buying = ui
        .get_child_recursive(tabs, dereth_ui_screens::panels::vendor::TAB_BUYING)
        .expect("the buying tab");
    ui.broadcast_element_message(buying, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    let the_click_opened_the_buying_page = p.open_page(ui) == Some(PAGE_BUY);
    let _ = ui.drain_outbox();
    clear_requests(&mut ui.requests);
    let the_shop_owns_it_there_too = p.on_element_message(ui, &page_changed, &a_view(&view));
    let the_shelf_was_left_alone = p.tab == Tab::Buying
        && on_the_shelf(&p) == before
        && take_requests(&mut ui.requests).is_empty();

    c.assert_behaviour(
        "vendor.filter.coming-back-to-the-stock-page-redraws-it-without-moving-what-is-picked",
        move |_| {
            it_opens_on_the_stock
                && the_shop_owns_it
                && the_same_rows_came_back
                && the_pick_did_not_move
                && the_click_opened_the_buying_page
                && the_shop_owns_it_there_too
                && the_shelf_was_left_alone
        },
    );
    c.shutdown();
}

/// The screen carries the two messages the shop window listens for, and carries nothing it
/// owns to nothing.
///
/// The negative is asserted with the positives: a gate written against the windows that
/// existed goes **quiet** when a new one arrives rather than failing, which is exactly what an
/// unwired strip looked like.
pub fn the_screen_carries_a_click_on_the_strip_and_a_change_of_page_to_the_shop() {
    use dereth_ui::msg::element::id as mid;

    let mut c = a_gameplay_client();
    let got = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let _ = screen.take_panel_messages();
        let root = screen.root().expect("the gameplay root");
        let mk = |id| ElementMessage {
            source_id: TYPE_FILTER_MENU,
            source: root,
            id,
            p1: 0,
            p2: 0,
            point: dereth_ui::msg::MessagePoint::default(),
            serial: 0,
        };
        for id in [mid::MENU_CHOSEN, mid::TAB_PAGE_CHANGED, mid::BUTTON_CLICKED] {
            screen.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &mk(id));
        }
        // A message no window on the screen switches on.
        screen.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(ui),
            &mk(mid::MENU_OPENED),
        );
        screen
            .take_panel_messages()
            .iter()
            .map(|m| m.id.0)
            .collect::<Vec<u32>>()
    };
    let carried_in_order = got
        == vec![
            mid::MENU_CHOSEN.0,
            mid::TAB_PAGE_CHANGED.0,
            mid::BUTTON_CLICKED.0,
        ];

    c.assert_behaviour(
        "vendor.filter.the-screen-carries-a-click-on-the-strip-and-a-change-of-page-to-the-shop",
        move |_| carried_in_order,
    );
    c.shutdown();
}

/// A thing the player owns keeps its own kind when a shop advertises it.
///
/// All three arms, with the thing's kind and the shop's word for it deliberately different,
/// so a client that read only one of them fails on one arm. Both readers of the same answer
/// are checked: the row's own kind and the strip the categories are built from.
pub fn a_thing_the_player_owns_keeps_its_own_kind_when_the_shop_advertises_it() {
    let mut c = a_gameplay_client();
    let mut w = a_shop_that_sells_food();
    let stock = w.shop.stock[0].iid;
    // The shop says Food; the thing we are about to plant says Armour. Nothing a shard could
    // send would produce this pair, which is the point.
    w.shop.stock[0].pwd.obj_type = FOOD;
    let no_other_row_is_armour = w.shop.stock[1..]
        .iter()
        .all(|p| p.pwd.obj_type & ARMOUR == 0);

    // Arm 1 -- the client has never seen the thing.
    let a = dereth_client::vendor_view::shop(&w);
    let unseen = a
        .stock
        .iter()
        .find(|r| r.item == stock)
        .expect("the row")
        .obj_type
        == FOOD
        && a.type_filters.iter().any(|(n, _)| *n == "Food")
        && !a.type_filters.iter().any(|(n, _)| *n == "Armor");

    // Arm 2 -- the client holds it and the player does not own it.
    let player = ObjectId(0x5000_000A);
    w.player = Some(player);
    let mut wn = dereth_client_model::weenie::Weenie::new(stock);
    wn.pwd = dereth_protocol::types::PublicWeenieDesc {
        name: "Fried Chicken".into(),
        obj_type: ARMOUR,
        ..dereth_protocol::types::PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(stock, wn);
    let nobody_owns_it = !w.is_owned_by_player(stock);
    let b = dereth_client::vendor_view::shop(&w);
    let unowned = b
        .stock
        .iter()
        .find(|r| r.item == stock)
        .expect("the row")
        .obj_type
        == FOOD
        && b.type_filters.iter().any(|(n, _)| *n == "Food")
        && !b.type_filters.iter().any(|(n, _)| *n == "Armor");

    // Arm 3 -- the player owns it, so it keeps its own kind.
    w.tables
        .weenies
        .get_mut(stock)
        .expect("the planted thing")
        .pwd
        .container_id = Some(player);
    let the_player_owns_it = w.is_owned_by_player(stock);
    let d = dereth_client::vendor_view::shop(&w);
    let owned = d
        .stock
        .iter()
        .find(|r| r.item == stock)
        .expect("the row")
        .obj_type
        == ARMOUR
        && d.type_filters.iter().any(|(n, _)| *n == "Armor");

    // The three arms really are three answers and not three restatements of one.
    let names = |v: &ShopView| v.type_filters.iter().map(|(n, _)| *n).collect::<Vec<_>>();
    let the_arms_differ = names(&a) == names(&b) && names(&b) != names(&d);

    c.assert_behaviour(
        "vendor.filter.a-thing-the-player-owns-keeps-its-own-kind-when-the-shop-advertises-it",
        move |_| {
            no_other_row_is_armour
                && unseen
                && nobody_owns_it
                && unowned
                && the_player_owns_it
                && owned
                && the_arms_differ
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// o740: the supply the basket has taken, the stack a row is offered in, and the container.
// -----------------------------------------------------------------------------------------

/// A shop whose named row advertises a countable supply of `amount`.
fn a_shop_advertising(target: ObjectId, amount: i32) -> World {
    let mut w = a_shop_that_sells_food();
    for p in &mut w.shop.stock {
        if p.iid == target {
            p.amount = amount;
        }
    }
    w
}

/// What the shelf is holding once the given category has been chosen on a fresh window.
fn shelf_after_choosing(
    c: &mut HeadlessClient,
    view: &ShopView,
    tab: usize,
) -> (VendorPanel, Vec<ObjectId>) {
    let mut p = bound_vendor(c.app_mut());
    choose_the_category(c, &mut p, view, tab);
    let shelf = on_the_shelf(&p);
    (p, shelf)
}

/// A fully basketed finite row remains on the shelf until its purchase is received.
pub fn a_fully_basketed_row_stays_on_the_shelf_until_purchase() {
    let mut c = a_gameplay_client();
    let target = a_food_row(&a_shop_that_sells_food());
    let tab = category_for(
        &dereth_client::vendor_view::shop(&a_shop_that_sells_food()),
        FOOD,
    );

    // Twelve advertised, nothing basketed: the row is there.
    let view = dereth_client::vendor_view::shop(&a_shop_advertising(target, 12));
    let (p, with_an_empty_basket) = shelf_after_choosing(&mut c, &view, tab);
    let it_is_listed = with_an_empty_basket.contains(&target) && p.basket_drops == 0;

    // Twelve advertised, twelve basketed: membership and order stay unchanged.
    let mut w = a_shop_advertising(target, 12);
    w.shop.buy_list.push((target, 12));
    let view = dereth_client::vendor_view::shop(&w);
    let (p, with_a_full_basket) = shelf_after_choosing(&mut c, &view, tab);
    let it_stays = with_a_full_basket == with_an_empty_basket && p.basket_drops == 0;

    // Neither a partial basket nor an overfilled saved basket hides advertised stock.
    let mut w = a_shop_advertising(target, 12);
    w.shop.buy_list.push((target, 11));
    let view = dereth_client::vendor_view::shop(&w);
    let (p, shelf) = shelf_after_choosing(&mut c, &view, tab);
    let one_left_keeps_it = shelf.contains(&target) && p.basket_drops == 0;

    let mut w = a_shop_advertising(target, 12);
    w.shop.buy_list.push((target, 13));
    let view = dereth_client::vendor_view::shop(&w);
    let (p, shelf) = shelf_after_choosing(&mut c, &view, tab);
    let more_than_all_keeps_it = shelf.contains(&target) && p.basket_drops == 0;

    // The same thing basketed twice counts as both.
    let mut w = a_shop_advertising(target, 12);
    w.shop.buy_list.push((target, 6));
    w.shop.buy_list.push((target, 6));
    let view = dereth_client::vendor_view::shop(&w);
    let really_two_entries = view.buy_list.iter().filter(|r| r.item == target).count() == 2;
    let (p, shelf) = shelf_after_choosing(&mut c, &view, tab);
    let two_helpings_keep_it = shelf.contains(&target) && p.basket_drops == 0;

    // The same retained membership in a shop the recordings really carry: the message, the
    // row, its supply and its kind are all the recording's, and the basket entry is put there
    // by the client's own add-to-list.
    let (w, sack) = a_recorded_shop_with_a_countable_supply();
    let recorded_supply = w
        .shop
        .stock
        .iter()
        .find(|p| p.iid == sack)
        .expect("the row")
        .amount;
    let view = dereth_client::vendor_view::shop(&w);
    let sack_tab = category_for(&view, CONTAINER);
    let it_is_in_containers = view.type_filters[sack_tab].0 == "Containers";
    let (_, shelf) = shelf_after_choosing(&mut c, &view, sack_tab);
    let the_recorded_row_is_listed = shelf.contains(&sack);

    let mut w2 = w;
    let the_button_basketed_it = w2.add_to_buy_list(sack, 1) && w2.shop.buy_list == vec![(sack, 1)];
    let view = dereth_client::vendor_view::shop(&w2);
    let (p, shelf) = shelf_after_choosing(&mut c, &view, sack_tab);
    let the_recorded_row_stays = shelf.contains(&sack) && p.basket_drops == 0;

    c.assert_behaviour(
        "vendor.shelf.a-fully-basketed-row-stays-on-the-shelf-until-purchase",
        move |_| {
            it_is_listed
                && it_stays
                && one_left_keeps_it
                && more_than_all_keeps_it
                && really_two_entries
                && two_helpings_keep_it
                && recorded_supply == 1
                && it_is_in_containers
                && the_recorded_row_is_listed
                && the_button_basketed_it
                && the_recorded_row_stays
        },
    );
    c.shutdown();
}

/// Basket membership does not change which rows the merchant advertises.
pub fn basket_quantities_do_not_change_advertised_shelf_membership() {
    let mut c = a_gameplay_client();
    let target = a_food_row(&a_shop_that_sells_food());
    let tab = category_for(
        &dereth_client::vendor_view::shop(&a_shop_that_sells_food()),
        FOOD,
    );

    // (advertised supply, what the basket holds of it, is it on the shelf)
    let mut all_four = true;
    for (amount, basketed, listed) in [
        // Zero advertised supply, with and without a saved basket entry.
        (0_i32, None, true),
        // The merchant still advertises the same identity.
        (0, Some(1_i32), true),
        // A negative finite quantity does not alter membership either.
        (-2, None, true),
        // Basketed finite stock remains visible.
        (5, Some(5), true),
    ] {
        let mut w = a_shop_advertising(target, amount);
        if let Some(n) = basketed {
            w.shop.buy_list.push((target, n));
        }
        let view = dereth_client::vendor_view::shop(&w);
        let (p, shelf) = shelf_after_choosing(&mut c, &view, tab);
        all_four &= shelf.contains(&target) == listed && p.basket_drops == u32::from(!listed);
    }

    c.assert_behaviour(
        "vendor.shelf.basket-quantities-do-not-change-advertised-shelf-membership",
        move |_| all_four,
    );
    c.shutdown();
}

/// The stack sizes the shelf asks for, in the order it asked.
fn stacks_asked_for(c: &mut HeadlessClient) -> Vec<(ObjectId, i32)> {
    take_requests(c.ui_outbox())
        .into_iter()
        .filter_map(|r| match r {
            UiRequest::VendorSetObjectStackSize { item, size } => Some((item, size)),
            _ => None,
        })
        .collect()
}

/// A stackable row is offered in the biggest stack the shop can sell.
pub fn a_stackable_row_is_offered_in_the_biggest_stack_the_shop_can_sell() {
    let mut c = a_gameplay_client();
    let target = a_food_row(&a_shop_that_sells_food());
    let tab = category_for(
        &dereth_client::vendor_view::shop(&a_shop_that_sells_food()),
        FOOD,
    );

    // The endless-supply arm: the whole stack, and nothing at all for a row that does not
    // stack. A stack of one is the same answer as no stack.
    let mut endless = true;
    for (max, want) in [(0_u16, None), (1, None), (2, Some(2_i32)), (100, Some(100))] {
        let mut w = a_shop_that_sells_food();
        let supply_is_endless = w.shop.stock.iter().all(|p| p.amount == -1);
        for p in &mut w.shop.stock {
            // Every other row unstackable, so what was asked for is unambiguous.
            p.pwd.max_stack_size = Some(if p.iid == target { max } else { 0 });
        }
        let view = dereth_client::vendor_view::shop(&w);
        let carried_across = view
            .stock
            .iter()
            .find(|r| r.item == target)
            .expect("the row")
            .max_stack_size
            == u32::from(max);
        let (p, shelf) = shelf_after_choosing(&mut c, &view, tab);
        endless &= supply_is_endless
            && carried_across
            && shelf.contains(&target)
            && stacks_asked_for(&mut c) == want.map(|s| vec![(target, s)]).unwrap_or_default()
            && p.stack_size_writes == u32::from(want.is_some());
    }

    // The countable-supply arm reaches the same place from different arithmetic: what is
    // advertised, held down to what the thing can stack to.
    let mut countable = true;
    for (amount, basketed, max, want) in [
        (200_i32, 50_i32, 100_u16, Some(100_i32)),
        (200, 150, 100, Some(100)),
        // A row that does not stack is still left alone, however much is left.
        (200, 150, 1, None),
        (200, 0, 100, Some(100)),
    ] {
        let mut w = a_shop_that_sells_food();
        for p in &mut w.shop.stock {
            if p.iid == target {
                p.amount = amount;
                p.pwd.max_stack_size = Some(max);
            } else {
                p.pwd.max_stack_size = Some(0);
            }
        }
        if basketed != 0 {
            w.shop.buy_list.push((target, basketed));
        }
        let view = dereth_client::vendor_view::shop(&w);
        let (_, shelf) = shelf_after_choosing(&mut c, &view, tab);
        countable &= shelf.contains(&target)
            && stacks_asked_for(&mut c) == want.map(|s| vec![(target, s)]).unwrap_or_default();
    }

    c.assert_behaviour(
        "vendor.shelf.a-stackable-row-is-offered-in-the-biggest-stack-the-shop-can-sell",
        move |_| endless && countable,
    );
    c.shutdown();
}

/// A container with anything in it is not on the shelf, and it was still set up first.
pub fn a_container_with_anything_in_it_is_not_on_the_shelf() {
    let mut c = a_gameplay_client();
    let target = a_food_row(&a_shop_that_sells_food());
    let tab = category_for(
        &dereth_client::vendor_view::shop(&a_shop_that_sells_food()),
        FOOD,
    );

    // Each of the two counts on its own, and both together.
    let mut each_count_fires = true;
    for (items, containers, listed) in [
        (0_usize, 0_usize, true),
        (1, 0, false),
        (0, 1, false),
        (2, 3, false),
    ] {
        let mut w = a_shop_that_sells_food();
        let ids = |n: usize, base: u32| {
            (0..n)
                .map(|i| ObjectId(base + u32::try_from(i).expect("a small count")))
                .collect::<Vec<_>>()
        };
        put_inside(
            &mut w,
            target,
            &ids(items, 0x8000_1000),
            &ids(containers, 0x8000_2000),
        );
        let view = dereth_client::vendor_view::shop(&w);
        let row = view
            .stock
            .iter()
            .find(|r| r.item == target)
            .expect("the row");
        let carried_across = (row.contained_items, row.contained_containers)
            == (
                u32::try_from(items).expect("a small count"),
                u32::try_from(containers).expect("a small count"),
            );
        let (p, shelf) = shelf_after_choosing(&mut c, &view, tab);
        each_count_fires &= carried_across
                && shelf.contains(&target) == listed
                && p.container_drops == u32::from(!listed)
                // The containment is the only reason it went.
                && p.basket_drops == 0;
    }

    // ...and the stack was still set up before it was dropped, which is only observable
    // because that write outlives the list: it is a change to the thing and not to a widget.
    let mut w = a_shop_that_sells_food();
    for p in &mut w.shop.stock {
        p.pwd.max_stack_size = Some(if p.iid == target { 100 } else { 0 });
    }
    put_inside(&mut w, target, &[ObjectId(0x8000_BEEF)], &[]);
    let view = dereth_client::vendor_view::shop(&w);
    let (p, shelf) = shelf_after_choosing(&mut c, &view, tab);
    let set_up_before_it_was_dropped = !shelf.contains(&target)
        && p.container_drops == 1
        && stacks_asked_for(&mut c) == vec![(target, 100)];

    c.assert_behaviour(
        "vendor.shelf.a-container-with-anything-in-it-is-not-on-the-shelf",
        move |_| each_count_fires && set_up_before_it_was_dropped,
    );
    c.shutdown();
}

/// Setting a row's stack keeps its unit price and lets the quantity dial mean something.
pub fn setting_a_rows_stack_keeps_its_unit_price_and_lets_the_quantity_dial_mean_something() {
    let mut c = a_gameplay_client();
    let target = ObjectId(0x8000_0A6E);
    let mut info = the_recorded_grocer();
    info.items.truncate(1);
    {
        let p = &mut info.items[0];
        p.iid = target;
        p.amount = -1;
        let pwd = p
            .pwd
            .as_mut()
            .expect("the recorded row describes its thing");
        pwd.stack_size = Some(1);
        pwd.max_stack_size = Some(100);
        pwd.value = Some(50);
    }
    let mut w = World::new();
    w.handle_vendor_info(
        &info,
        &mut RecordingSink::default(),
        &mut RecordingRequests::default(),
        dereth_primitives::ServerTime(0.0),
    );

    let one_costs = w.shop.profile.vendor_sell_price(&w.shop.stock[0].pwd, 1);

    // Before: the stack is one, so the add-to-list button ignores the dial.
    let before = w.add_to_buy_list(target, 7) && w.shop.buy_list == vec![(target, 1)];
    w.shop.buy_list.clear();

    let it_was_set = w.set_object_stack_size(target, 100);
    let the_stack_and_the_price_moved_together = {
        let pwd = &w.shop.stock[0].pwd;
        pwd.stack_size == Some(100) && pwd.value == Some(50 * 100)
    };
    let one_still_costs_the_same =
        w.shop.profile.vendor_sell_price(&w.shop.stock[0].pwd, 1) == one_costs;

    // After: the same press takes the dialled number.
    let after = w.add_to_buy_list(target, 7) && w.shop.buy_list == vec![(target, 7)];

    // Doing it again changes nothing, which matters because the shelf is refilled on every
    // redraw.
    let again = w.set_object_stack_size(target, 100)
        && w.shop.stock[0].pwd.value == Some(50 * 100)
        && w.shop.stock[0].pwd.stack_size == Some(100);

    // A thing with no stack of its own at all is one of it, not none.
    w.shop.stock[0].pwd.stack_size = None;
    w.shop.stock[0].pwd.value = Some(9);
    let no_stack_is_one =
        w.set_object_stack_size(target, 4) && w.shop.stock[0].pwd.value == Some(36);

    c.assert_behaviour(
            "vendor.shelf.setting-a-rows-stack-keeps-its-unit-price-and-lets-the-quantity-dial-mean-something",
            move |_| {
                before
                    && it_was_set
                    && the_stack_and_the_price_moved_together
                    && one_still_costs_the_same
                    && after
                    && again
                    && no_stack_is_one
            },
        );
    c.shutdown();
}

/// How big a stack a row can be sold in comes from the same place its kind does.
pub fn what_a_row_stacks_to_comes_from_the_same_place_its_kind_does() {
    let mut c = a_gameplay_client();
    let mut w = a_shop_that_sells_food();
    let stock = w.shop.stock[0].iid;
    w.shop.stock[0].pwd.max_stack_size = Some(100);
    let row_max = |w: &World| {
        dereth_client::vendor_view::shop(w)
            .stock
            .iter()
            .find(|r| r.item == stock)
            .expect("the row")
            .max_stack_size
    };

    // Arm 1 -- the client has never seen the thing.
    let unseen = row_max(&w) == 100;

    // Arm 2 -- it holds it and the player does not own it.
    let player = ObjectId(0x5000_000A);
    w.player = Some(player);
    let mut wn = dereth_client_model::weenie::Weenie::new(stock);
    wn.pwd = dereth_protocol::types::PublicWeenieDesc {
        max_stack_size: Some(7),
        ..dereth_protocol::types::PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(stock, wn);
    let nobody_owns_it = !w.is_owned_by_player(stock);
    let unowned = row_max(&w) == 100;

    // Arm 3 -- the player owns it, so its own answer stands.
    w.tables
        .weenies
        .get_mut(stock)
        .expect("the planted thing")
        .pwd
        .container_id = Some(player);
    let the_player_owns_it = w.is_owned_by_player(stock);
    let owned = row_max(&w) == 7;

    c.assert_behaviour(
        "vendor.shelf.what-a-row-stacks-to-comes-from-the-same-place-its-kind-does",
        move |_| unseen && nobody_owns_it && unowned && the_player_owns_it && owned,
    );
    c.shutdown();
}

/// The picked row can be one that is no longer on the shelf.
pub fn the_picked_row_can_be_one_that_is_no_longer_on_the_shelf() {
    let mut c = a_gameplay_client();
    let w0 = a_shop_that_sells_food();
    let food: Vec<ObjectId> = w0
        .shop
        .stock
        .iter()
        .filter(|p| p.pwd.obj_type & FOOD != 0)
        .map(|p| p.iid)
        .collect();
    assert!(
        food.len() >= 2,
        "the premise: two food rows, or there is no first: {food:?}"
    );
    let first_row = food[0];

    let mut w = a_shop_advertising(first_row, 5);
    put_inside(&mut w, first_row, &[ObjectId(0x8000_BEEF)], &[]);
    let view = dereth_client::vendor_view::shop(&w);
    let tab = category_for(&view, FOOD);
    let (_, shelf) = shelf_after_choosing(&mut c, &view, tab);
    let the_first_row_is_gone = !shelf.contains(&first_row);
    let picks: Vec<ObjectId> = take_requests(c.ui_outbox())
        .into_iter()
        .filter_map(|r| match r {
            UiRequest::Select(id) => Some(id),
            _ => None,
        })
        .collect();
    let it_picked_the_dropped_row = picks == vec![first_row];
    let and_that_is_not_the_first_left = picks.first().copied() != shelf.first().copied();

    c.assert_behaviour(
        "vendor.shelf.the-picked-row-can-be-one-that-is-no-longer-on-the-shelf",
        move |_| {
            the_first_row_is_gone && it_picked_the_dropped_row && and_that_is_not_the_first_left
        },
    );
    c.shutdown();
}

/// The shelf never has anywhere to scroll to, however little is left on it.
pub fn the_shelf_never_scrolls_however_little_is_left_on_it() {
    let mut c = a_gameplay_client();
    let w0 = a_shop_that_sells_food();
    let food: Vec<ObjectId> = w0
        .shop
        .stock
        .iter()
        .filter(|p| p.pwd.obj_type & FOOD != 0)
        .map(|p| p.iid)
        .collect();
    let tab = category_for(&dereth_client::vendor_view::shop(&w0), FOOD);

    // Filled containers are excluded independently of basket membership.
    let mut w = a_shop_that_sells_food();
    for id in &food {
        put_inside(&mut w, *id, &[ObjectId(0x8000_BEEF)], &[]);
    }
    let view = dereth_client::vendor_view::shop(&w);
    let (p, shelf) = shelf_after_choosing(&mut c, &view, tab);
    let every_row_was_dropped = shelf.is_empty()
        && usize::try_from(p.container_drops).expect("a small count") == food.len();
    // It is the padding that is being counted, so the pass still takes the branch.
    let it_still_counted = p.scroll_restores == 1;

    // The companion, so this is not simply "it always fires": the same category with an empty
    // basket keeps its rows and takes the branch for the ordinary reason.
    let view = dereth_client::vendor_view::shop(&a_shop_that_sells_food());
    let (mut p, shelf) = shelf_after_choosing(&mut c, &view, tab);
    let a_full_shelf_counts_too = !shelf.is_empty() && p.scroll_restores == 1;

    // ...and there is still nowhere to scroll to.
    let nowhere_to_scroll = {
        let list = p.stock.as_ref().expect("the shelf").handle;
        let (ui, _root) = gameplay_root(c.app_mut());
        !dereth_ui::widgets::listbox::set_scroll_offset(ui, list, 0, 1)
    };
    let _ = &mut p;

    c.assert_behaviour(
        "vendor.shelf.the-shelf-never-scrolls-however-little-is-left-on-it",
        move |_| {
            every_row_was_dropped
                && it_still_counted
                && a_full_shelf_counts_too
                && nowhere_to_scroll
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// The tests that run the scenarios above and check their declarations.
// -----------------------------------------------------------------------------------------

dereth_testkit::scenarios! {
    scenario_every_row_of_the_shop_is_a_real_thing_and_is_drawn_finished => every_row_of_the_shop_is_a_real_thing_and_is_drawn_finished ["vendor.stock.every-row-of-the-shop-is-a-real-thing-and-is-drawn-finished"],
    scenario_pressing_a_row_in_the_shops_own_window_picks_it => pressing_a_row_in_the_shops_own_window_picks_it ["vendor.stock.pressing-a-row-in-the-shops-own-window-picks-it"],
    scenario_a_drop_on_the_shops_own_sell_window_offers_it_and_sell_everything_sends_it => a_drop_on_the_shops_own_sell_window_offers_it_and_sell_everything_sends_it ["vendor.sell.a-drop-on-the-shops-own-sell-window-offers-it-and-sell-everything-sends-it"],
    scenario_what_is_on_the_shops_shelf_is_bought_and_never_used => what_is_on_the_shops_shelf_is_bought_and_never_used ["vendor.stock.what-is-on-the-shops-shelf-is-bought-and-never-used"],
    scenario_part_of_a_stack_let_go_on_the_sell_window_is_split_off_and_offered => part_of_a_stack_let_go_on_the_sell_window_is_split_off_and_offered ["vendor.sell.part-of-a-stack-let-go-on-the-sell-window-is-split-off-and-offered"],
    scenario_taking_a_row_back_out_of_the_window_is_a_withdrawal_and_not_a_sale => taking_a_row_back_out_of_the_window_is_a_withdrawal_and_not_a_sale ["vendor.sell.taking-a-row-back-out-of-the-window-is-a-withdrawal-and-not-a-sale"],
    scenario_choosing_a_category_leaves_exactly_the_things_of_that_kind => choosing_a_category_leaves_exactly_the_things_of_that_kind ["vendor.filter.choosing-a-category-leaves-exactly-the-things-of-that-kind"],
    scenario_every_category_is_a_row_the_player_can_actually_click => every_category_is_a_row_the_player_can_actually_click ["vendor.filter.every-category-is-a-row-the-player-can-actually-click"],
    scenario_a_shop_with_no_category_chosen_shows_nothing => a_shop_with_no_category_chosen_shows_nothing ["vendor.filter.a-shop-with-no-category-chosen-shows-nothing"],
    scenario_what_is_picked_moves_only_when_a_shop_opens => what_is_picked_moves_only_when_a_shop_opens ["vendor.filter.what-is-picked-moves-only-when-a-shop-opens"],
    scenario_the_list_that_keeps_nothing_still_has_no_room_to_scroll => the_list_that_keeps_nothing_still_has_no_room_to_scroll ["vendor.filter.the-list-that-keeps-nothing-still-has-no-room-to-scroll"],
    scenario_coming_back_to_the_stock_page_redraws_it_without_moving_what_is_picked => coming_back_to_the_stock_page_redraws_it_without_moving_what_is_picked ["vendor.filter.coming-back-to-the-stock-page-redraws-it-without-moving-what-is-picked"],
    scenario_the_screen_carries_a_click_on_the_strip_and_a_change_of_page_to_the_shop => the_screen_carries_a_click_on_the_strip_and_a_change_of_page_to_the_shop ["vendor.filter.the-screen-carries-a-click-on-the-strip-and-a-change-of-page-to-the-shop"],
    scenario_a_thing_the_player_owns_keeps_its_own_kind_when_the_shop_advertises_it => a_thing_the_player_owns_keeps_its_own_kind_when_the_shop_advertises_it ["vendor.filter.a-thing-the-player-owns-keeps-its-own-kind-when-the-shop-advertises-it"],
    scenario_a_fully_basketed_row_stays_on_the_shelf_until_purchase => a_fully_basketed_row_stays_on_the_shelf_until_purchase ["vendor.shelf.a-fully-basketed-row-stays-on-the-shelf-until-purchase"],
    scenario_basket_quantities_do_not_change_advertised_shelf_membership => basket_quantities_do_not_change_advertised_shelf_membership ["vendor.shelf.basket-quantities-do-not-change-advertised-shelf-membership"],
    scenario_a_stackable_row_is_offered_in_the_biggest_stack_the_shop_can_sell => a_stackable_row_is_offered_in_the_biggest_stack_the_shop_can_sell ["vendor.shelf.a-stackable-row-is-offered-in-the-biggest-stack-the-shop-can-sell"],
    scenario_a_container_with_anything_in_it_is_not_on_the_shelf => a_container_with_anything_in_it_is_not_on_the_shelf ["vendor.shelf.a-container-with-anything-in-it-is-not-on-the-shelf"],
    scenario_setting_a_rows_stack_keeps_its_unit_price_and_lets_the_quantity_dial_mean_something => setting_a_rows_stack_keeps_its_unit_price_and_lets_the_quantity_dial_mean_something ["vendor.shelf.setting-a-rows-stack-keeps-its-unit-price-and-lets-the-quantity-dial-mean-something"],
    scenario_what_a_row_stacks_to_comes_from_the_same_place_its_kind_does => what_a_row_stacks_to_comes_from_the_same_place_its_kind_does ["vendor.shelf.what-a-row-stacks-to-comes-from-the-same-place-its-kind-does"],
    scenario_the_picked_row_can_be_one_that_is_no_longer_on_the_shelf => the_picked_row_can_be_one_that_is_no_longer_on_the_shelf ["vendor.shelf.the-picked-row-can-be-one-that-is-no-longer-on-the-shelf"],
    scenario_the_shelf_never_scrolls_however_little_is_left_on_it => the_shelf_never_scrolls_however_little_is_left_on_it ["vendor.shelf.the-shelf-never-scrolls-however-little-is-left-on-it"],
}
