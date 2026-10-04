//! The unnamed vendor child is the stock list's scrollbar named in the layout and resolved by the
//! scrollable; it is live only on overflow; every scrollbar message moves the list; holding the
//! arrow scrolls one slot at a time.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use crate::common::*;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_ui::widgets::listbox::scroll_offset_of;
use dereth_ui::{Delivery, ElemHandle, ElementId, MessageId, Screen, UiSystem};
use dereth_ui_screens::panels::vendor::{self, VendorPanel, PAGE_ITEMS, STOCK_LIST, TABS};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{GameView, ShopRow, ShopView};

/// The subject: the stock list's horizontal scrollbar.
const STOCK_SCROLLBAR: ElementId = ElementId(0x1000_00BE);
/// The buy list's, and the sell list's — the same pair one page over.
const BUY_SCROLLBAR: ElementId = ElementId(0x1000_00C6);
const SELL_SCROLLBAR: ElementId = ElementId(0x1000_00CF);
/// The bar's own parts, from the shipped scrollbar template.
const THUMB: ElementId = ElementId(0x0000_0001);
const ARROW_INCREMENT: ElementId = ElementId(0x1000_036B);
const ARROW_DECREMENT: ElementId = ElementId(0x1000_036C);
/// A scrollbar, `dereth_ui::factory::engine::SCROLLBAR`.
const SCROLLBAR: u32 = 0x0B;
/// An item-list widget.
const ITEM_LIST: u32 = 0x1000_0031;
/// One `ItemSlot` pitch: the shipped stock list's slots are 32 px wide, so one arrow step is 32.
const SLOT_PITCH: i32 = 32;

// ---------------------------------------------------------------------------------------------
// Harness — `lamp_panels.rs`'s.
// ---------------------------------------------------------------------------------------------

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

fn gameplay() -> (UiSystem, GamePlayScreen) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    (ui, s)
}

fn find(ui: &UiSystem, s: &GamePlayScreen, id: ElementId) -> ElemHandle {
    let root = s.root().expect("the gameplay root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{:#010X} is not in the shipped tree", id.0))
}

/// The `H_SCROLLBAR` / `V_SCROLLBAR` attribute a scrollable names its bar with, as an element id.
fn bar_attribute(ui: &UiSystem, h: ElemHandle, attr: u32) -> Option<u32> {
    ui.node(h)?.merged_properties().get_enum(attr)
}

/// The list's own `ListBox` behaviour, so the production `Scrollable::scrollbar` accessor can be
/// asked directly and its returned scrollbar can be checked through the Rust element accessors.
fn list_box(ui: &UiSystem, h: ElemHandle) -> &dereth_ui::widgets::listbox::ListBox {
    ui.node(h)
        .expect("list node")
        .behaviour
        .as_ref()
        .expect("behaviour")
        .as_any()
        .expect("typed behaviour")
        .downcast_ref()
        .expect("list widget inherits ListBox")
}

// ---------------------------------------------------------------------------------------------
// A shop with enough stock to overflow one row of slots
// ---------------------------------------------------------------------------------------------

const MISC: u32 = 0x0000_0200;

#[derive(Debug, Default)]
struct Shop(ShopView);
impl GameView for Shop {
    fn shop(&self) -> ShopView {
        self.0.clone()
    }
}

fn shop(rows: usize) -> Shop {
    Shop(ShopView {
        open: true,
        vendor: Some(ObjectId(0x8000_0001)),
        stock: (0..rows)
            .map(|i| ShopRow {
                item: ObjectId(0x8000_0100 + u32::try_from(i).expect("small")),
                name: format!("Thing {i}"),
                amount: -1,
                obj_type: MISC,
                price: 10,
                ..ShopRow::default()
            })
            .collect(),
        type_filters: vec![("Miscellaneous", MISC)],
        ..ShopView::default()
    })
}

/// A bound vendor panel with `rows` stock rows on screen, and the tree made visible down to the
/// bar so the pointer can reach it (the vendor is a page of the `<ENVP>` window, which opens
/// hidden with the rest of the stack).
fn open_shop(rows: usize) -> (UiSystem, GamePlayScreen, VendorPanel, Shop) {
    let (mut ui, s) = gameplay();
    let root = s.root().expect("the gameplay root");
    let mut p = VendorPanel::default();
    p.post_init(&mut ui, root);
    assert!(
        p.bound(),
        "the stock list is the binding without which nothing can be drawn"
    );
    let view = shop(rows);
    p.open_vendor_type_filters(&mut ui, &view.0);
    p.update(&mut ui, &view);
    assert_eq!(
        p.rows(vendor::Tab::Items).len(),
        rows,
        "the fixture's filter keeps every row: a mask of 0 would empty the list"
    );
    // Every ancestor **above** the bar, so the pointer can reach it. The bar's own visibility is
    // left exactly as the panel left it: it is the thing under measurement.
    let bar = find(&ui, &s, STOCK_SCROLLBAR);
    let mut chain = Vec::new();
    let mut cur = ui.parent(bar);
    while let Some(h) = cur {
        chain.push(h);
        cur = ui.parent(h);
    }
    for h in chain.into_iter().rev() {
        ui.set_visible(h, true);
    }
    (ui, s, p, view)
}

fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen, p: &mut VendorPanel, view: &Shop) {
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
                p.on_element_message(ui, &msg, view);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 1. What the element is, and the attribute that reaches it
// ---------------------------------------------------------------------------------------------

/// `0x100000BE` is a scrollbar beside the stock list on the Items page, with the
/// shipped bar template's thumb and two arrows under it.
#[test]
fn the_unnamed_vendor_child_is_the_stock_lists_scrollbar() {
    let (ui, s) = gameplay();
    let page = find(&ui, &s, PAGE_ITEMS);
    let list = find(&ui, &s, STOCK_LIST);
    let bar = find(&ui, &s, STOCK_SCROLLBAR);

    assert_eq!(
        ty_of(&ui, bar),
        SCROLLBAR,
        "0x100000BE is engine type 0x0B, scrollbar element"
    );
    assert_eq!(ty_of(&ui, list), ITEM_LIST, "0x100000BD is a list widget");
    assert_eq!(
        ui.parent(bar),
        Some(page),
        "it sits under the Items page 0x100000BC"
    );
    assert_eq!(
        ui.parent(list),
        Some(page),
        "beside the stock list, not inside it"
    );
    assert!(
        ui.is_ancestor_of(find(&ui, &s, TABS), bar),
        "and the page is inside the vendor panel 0x100000B8, post-init's one binding"
    );
    // Authored invisible: retail shows it only when the content overflows, which is what
    // `update_scrollbar_size` decides. This is the state a freshly built tree is in.
    assert!(
        !ui.node(bar).expect("alive").region.flags.visible,
        "the shipped layout authors the bar hidden"
    );
    let kids: Vec<u32> = ui.children(bar).iter().map(|c| id_of(&ui, *c)).collect();
    assert_eq!(
        kids,
        vec![THUMB.0, ARROW_INCREMENT.0, ARROW_DECREMENT.0],
        "the bar's own parts: the thumb and the two arrow buttons"
    );
}

/// **The route both sides take, and the reason no id literal exists on either.** The stock list
/// names its bar in layout data, through attribute `0x71`.
#[test]
fn the_stock_list_names_its_bar_in_the_layout_and_the_scrollable_resolves_it() {
    let (ui, s) = gameplay();
    let list = find(&ui, &s, STOCK_LIST);
    let bar = find(&ui, &s, STOCK_SCROLLBAR);

    assert_eq!(
        bar_attribute(&ui, list, dereth_ui::scrollable::attr::H_SCROLLBAR),
        Some(STOCK_SCROLLBAR.0),
        "0x100000BD's H_SCROLLBAR (0x71) is 0x100000BE"
    );
    assert_eq!(
        bar_attribute(&ui, list, dereth_ui::scrollable::attr::V_SCROLLBAR),
        None,
        "and it has no vertical bar: the stock strip is one row of slots, scrolled sideways"
    );
    // The identity round trip the scrollbar lookup performs, through the production accessor.
    let resolved = list_box(&ui, list).scroll.scrollbar(&ui, list, true);
    assert_eq!(
        resolved,
        Some(bar),
        "Scrollable::scrollbar(horizontal) lands on 0x100000BE"
    );

    // The same pair on the other two pages, so the finding is the shape of the window and not one
    // element's accident.
    for (list_id, bar_id) in [
        (vendor::BUY_LIST, BUY_SCROLLBAR),
        (vendor::SELL_LIST, SELL_SCROLLBAR),
    ] {
        let l = find(&ui, &s, list_id);
        assert_eq!(
            bar_attribute(&ui, l, dereth_ui::scrollable::attr::H_SCROLLBAR),
            Some(bar_id.0),
            "{:#010X}'s H_SCROLLBAR is {:#010X}",
            list_id.0,
            bar_id.0
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 2. It is driven — the bar comes alive with the stock, and moves the list
// ---------------------------------------------------------------------------------------------

/// Behaviour: vendor.stock.the-strips-scrollbar-comes-alive-only-when-the-stock-overflows-and-its-arrow-scrolls-one-slot
/// An empty shop leaves the bar hidden, disabled and full-proportion; a shop whose stock overflows
/// the strip shows it, enables it and gives it a real thumb proportion. Both halves, because a bar
/// that was always visible would pass the second on its own.
#[test]
fn the_bar_comes_alive_only_when_the_stock_overflows_the_strip() {
    /// `(visible, disabled, horizontal, proportion)` of the bar, after a shop of `rows` rows.
    fn measure(rows: usize) -> (bool, Option<bool>, Option<bool>, Option<f32>) {
        let (ui, s, p, _view) = open_shop(rows);
        assert_eq!(p.rows(vendor::Tab::Items).len(), rows);
        let bar = find(&ui, &s, STOCK_SCROLLBAR);
        let n = ui.node(bar).expect("alive");
        let mp = n.merged_properties();
        (
            n.region.flags.visible,
            mp.get_bool(dereth_ui::widgets::scrollbar::attr::DISABLED),
            mp.get_bool(dereth_ui::widgets::scrollbar::attr::HORIZONTAL),
            mp.get_float(dereth_ui::widgets::scrollbar::attr::PROPORTION),
        )
    }

    let (vis, disabled, horizontal, proportion) = measure(0);
    assert!(
        !vis,
        "nothing in stock, nothing to scroll: the bar stays hidden"
    );
    assert_eq!(disabled, Some(true), "and disabled");
    assert_eq!(
        horizontal,
        Some(true),
        "the vendor's bar is the horizontal one either way"
    );
    assert_eq!(
        proportion,
        Some(1.0),
        "a full-width thumb is the empty-content proportion"
    );

    // Forty rows in a strip 509 px wide at 32 px a slot — about sixteen fit.
    let (vis, disabled, horizontal, proportion) = measure(40);
    assert!(vis, "forty rows overflow the strip, so the bar is shown");
    assert_eq!(disabled, Some(false), "and enabled");
    assert_eq!(horizontal, Some(true), "still the horizontal bar");
    let proportion = proportion.expect("a thumb proportion");
    assert!(
        proportion > 0.0 && proportion < 1.0,
        "the thumb is a real fraction of the track: {proportion}"
    );
}

/// The four scrollbar messages, in the direction gives them:
/// `0x0E` a step forward, `0x0D` a step back, `0x10` a page forward, `0x0F` a page back. Driven at
/// the bar so the directions can be asserted; the pointer test below is what proves a player can
/// reach them at all.
#[test]
fn every_scrollbar_message_from_0x100000be_moves_the_stock_list() {
    let (mut ui, s, _p, _view) = open_shop(40);
    let list = find(&ui, &s, STOCK_LIST);
    let bar = find(&ui, &s, STOCK_SCROLLBAR);
    let x = |ui: &UiSystem| scroll_offset_of(ui, list).expect("the list scrolls").0;
    assert_eq!(x(&ui), 0, "a freshly filled list starts at the left");

    ui.broadcast_element_message(bar, MessageId(0x0E), 0, 0);
    assert_eq!(
        x(&ui),
        SLOT_PITCH,
        "0x0E is one step forward, and a step is one slot"
    );
    ui.broadcast_element_message(bar, MessageId(0x0E), 0, 0);
    assert_eq!(x(&ui), 2 * SLOT_PITCH, "and it accumulates");
    ui.broadcast_element_message(bar, MessageId(0x0D), 0, 0);
    assert_eq!(x(&ui), SLOT_PITCH, "0x0D is the step back");

    let before = x(&ui);
    ui.broadcast_element_message(bar, MessageId(0x10), 0, 0);
    let paged = x(&ui);
    assert!(
        paged > before + SLOT_PITCH,
        "0x10 is a page, which is more than a step: {before} -> {paged}"
    );
    ui.broadcast_element_message(bar, MessageId(0x0F), 0, 0);
    assert!(x(&ui) < paged, "0x0F pages back: {paged} -> {}", x(&ui));

    // The bar's own position attribute follows the list, which is what draws the thumb.
    let pos = ui
        .node(bar)
        .expect("alive")
        .merged_properties()
        .get_float(dereth_ui::widgets::scrollbar::attr::POSITION)
        .expect("a position");
    assert!(pos > 0.0, "the thumb moved with the content: {pos}");
}

/// **From a pointer position.** Press and hold the bar's increment arrow at its own screen
/// coordinates and let the repeat clock run; the stock list walks one slot per repeat and stops
/// when the button is released.
#[test]
fn holding_the_bars_arrow_scrolls_the_stock_list_one_slot_at_a_time() {
    let (mut ui, mut s, mut p, view) = open_shop(40);
    let list = find(&ui, &s, STOCK_LIST);
    let bar = find(&ui, &s, STOCK_SCROLLBAR);
    let arrow = ui
        .get_child(bar, ARROW_INCREMENT)
        .expect("the increment arrow");
    let b = ui.screen_box(arrow);
    assert!(b.is_valid(), "the arrow has a box to point at");
    let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    let offset = |ui: &UiSystem| scroll_offset_of(ui, list).expect("the list scrolls").0;

    let mut t = 100.0_f64;
    ui.use_time(LocalTime(t), &mut dereth_ui::NullInputPump);
    ui.mouse_move(LocalTime(t), x, y);
    assert_eq!(offset(&ui), 0, "the pointer alone has not moved it");
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);

    let mut steps = offset(&ui) / SLOT_PITCH;
    for _ in 0..6 {
        t += 0.25;
        ui.use_time(LocalTime(t), &mut dereth_ui::NullInputPump);
        pump(&mut ui, &mut s, &mut p, &view);
        let now = offset(&ui);
        if now == (steps + 1) * SLOT_PITCH {
            steps += 1;
        }
    }
    assert!(steps >= 4, "a held arrow repeats: {steps} slots reached");
    assert_eq!(
        offset(&ui),
        steps * SLOT_PITCH,
        "and every repeat is exactly one slot pitch, never a partial one"
    );

    let held = offset(&ui);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    for _ in 0..4 {
        t += 0.25;
        ui.use_time(LocalTime(t), &mut dereth_ui::NullInputPump);
        pump(&mut ui, &mut s, &mut p, &view);
    }
    assert_eq!(offset(&ui), held, "release stops the repeat");
}

/// Behaviour: vendor.slots.resize-padding-does-not-extend-scroll
#[test]
fn resizing_refills_blank_slots_but_only_filled_slots_extend_scroll() {
    let (mut ui, s, mut panel, mut view) = open_shop(1);
    view.0.buy_list = shop(3).0.stock;
    view.0.sell_list = view.0.buy_list.clone();
    panel.update(&mut ui, &view);
    for (list_id, which, bar_id) in [
        (STOCK_LIST, vendor::Tab::Items, STOCK_SCROLLBAR),
        (vendor::BUY_LIST, vendor::Tab::Buying, BUY_SCROLLBAR),
        (vendor::SELL_LIST, vendor::Tab::Selling, SELL_SCROLLBAR),
    ] {
        let list = find(&ui, &s, list_id);
        for width in [192, 640, 96] {
            ui.resize_to(list, width, 32);
            assert!(panel.update(&mut ui, &view));
            let widget = match which {
                vendor::Tab::Items => panel.stock.as_ref(),
                vendor::Tab::Buying => panel.buy.as_ref(),
                vendor::Tab::Selling => panel.sell.as_ref(),
            }
            .unwrap();
            assert!(
                widget.slots.len() >= usize::try_from(width / 32).unwrap(),
                "resized visible cells are filled"
            );
            assert!(widget
                .slots
                .iter()
                .skip(panel.rows(which).len())
                .all(|slot| slot.item.is_none()));
            let b = find(&ui, &s, bar_id);
            assert!(
                !ui.node(b).unwrap().region.flags.visible,
                "blank padding does not scroll"
            );
            assert_eq!(
                list_box(&ui, list).scroll_item_count,
                Some(panel.rows(which).len())
            );
        }
    }
    let many = shop(40);
    panel.update(&mut ui, &many);
    let list = find(&ui, &s, STOCK_LIST);
    assert!(
        ui.node(find(&ui, &s, STOCK_SCROLLBAR))
            .unwrap()
            .region
            .flags
            .visible
    );
    assert_eq!(list_box(&ui, list).scroll_item_count, Some(40));
    panel.update(&mut ui, &view);
    assert!(
        !ui.node(find(&ui, &s, STOCK_SCROLLBAR))
            .unwrap()
            .region
            .flags
            .visible
    );
}
