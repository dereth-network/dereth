//! Inventory bar/wheel scrolling moves slots and hit-testing resolves the displayed item; vendor
//! filter scroll-to-show, repaint preserves scroll, refilter shrinks extent ignoring hidden cached
//! slots.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

#![allow(clippy::pedantic)]

use crate::common::layout::RegistrationOrder;
use dereth_primitives::ObjectId;
use dereth_ui::framework::LayoutEnum;
use dereth_ui::widgets::listbox::{set_scroll_offset, ListBox};
use dereth_ui::{ElemHandle, ElementId, MessageId, UiSystem};
use dereth_ui_screens::panels::{inventory::InventoryPanels, vendor::VendorPanel};
use dereth_ui_screens::view::{GameView, ShopRow, ShopView};

fn env() -> (UiSystem, ElemHandle) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let root = dereth_ui_screens::env::create_and_add_root_element(
        &mut ui,
        LayoutEnum(0x1000_0006),
        ElementId(0x1000_0495),
    )
    .expect("shipped gameplay root");
    (ui, root)
}

fn list(ui: &UiSystem, h: ElemHandle) -> &ListBox {
    ui.node(h)
        .expect("list node")
        .behaviour
        .as_ref()
        .expect("behavior")
        .as_any()
        .expect("typed behavior")
        .downcast_ref()
        .expect("ItemList inherits ListBox")
}

fn show_ancestors(ui: &mut UiSystem, h: ElemHandle) {
    let mut current = Some(h);
    while let Some(h) = current {
        ui.set_visible(h, true);
        current = ui.parent(h);
    }
}

#[derive(Debug)]
struct View {
    items: Vec<ObjectId>,
    packs: Vec<ObjectId>,
    shop: ShopView,
    selected: Option<ObjectId>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            items: (0..102).map(|i| ObjectId(0x5000 + i)).collect(),
            packs: vec![ObjectId(0x6000)],
            shop: ShopView::default(),
            selected: None,
        }
    }
}
impl GameView for View {
    fn player(&self) -> Option<ObjectId> {
        Some(ObjectId(0x1000))
    }
    fn container_contents(&self, _id: ObjectId) -> &[ObjectId] {
        &self.items
    }
    fn contained_containers(&self, id: ObjectId) -> &[ObjectId] {
        if id == ObjectId(0x1000) {
            &self.packs
        } else {
            &[]
        }
    }
    fn items_capacity(&self, _id: ObjectId) -> Option<i32> {
        Some(102)
    }
    fn containers_capacity(&self, _id: ObjectId) -> Option<i32> {
        Some(7)
    }
    fn shop(&self) -> ShopView {
        self.shop.clone()
    }
    fn selected_object(&self) -> Option<ObjectId> {
        self.selected
    }
}

/// Behaviour: inventory.pack.the-bar-and-wheel-scroll-the-grid-and-a-scrolled-hit-resolves-the-shown-item
#[test]
fn inventory_bar_and_wheel_move_slots_and_the_scrolled_hit_resolves_the_displayed_item() {
    let (mut ui, root) = env();
    let page = ui
        .get_child_recursive(root, ElementId(0x1000_018B))
        .expect("inventory page");
    let mut stack = dereth_ui_screens::panels::panel_stack::PanelStack::default();
    stack.setup_children(&mut ui, root);
    let page_id = stack
        .pages
        .iter()
        .find(|p| p.handle == page)
        .expect("inventory tab")
        .panel_id;
    stack.recv_set_panel_visibility(&mut ui, page_id, true);
    let mut panel = InventoryPanels::default();
    panel.post_init(&mut ui, page);
    let mut view = View::default();
    assert!(panel.update(&mut ui, &view));
    let w = panel.item_list.as_ref().expect("inventory grid");
    let h = w.handle;
    show_ancestors(&mut ui, h);
    let bar = list(&ui, h)
        .scroll
        .scrollbar(&ui, h, false)
        .expect("shipped vertical bar");
    let ch = w.cell.1;
    assert_eq!(list(&ui, h).items.len(), 102);
    assert!(list(&ui, h).scroll.height > ui.node(h).unwrap().region.box_.height());
    assert_eq!(w.scroll(&ui), (0, 0));

    // The bottom-arrow message reaches the list through its actual sibling-bar registration.
    ui.broadcast_element_message(bar, MessageId(0x0E), 0, 0);
    assert_eq!(w.scroll(&ui), (0, ch));
    let first = ui.node(w.slots[0].handle).unwrap().region.box_;
    assert_eq!(first.y0, -ch);
    assert_eq!(
        w.item_index_at_point(&ui, 0, 0),
        Some(0),
        "retail's <= comparison assigns an exact scrolled cell boundary to the prior row"
    );
    let i = w
        .item_index_at_point(&ui, 16, 16)
        .expect("visible item under pointer");
    assert_eq!(w.item_at(i), Some(view.items[i]));
    let b = ui.node(w.slots[i].handle).unwrap().region.box_;
    assert!(b.x0 <= 16 && 16 < b.x1 && b.y0 <= 16 && 16 < b.y1);
    assert_ne!(i, 0);
    assert_eq!(w.item_index_at_point(&ui, -1, 16), None);

    // UIItem remains mouse visible: the normal mouse message bubbles to its parent list's
    // Scrollable arm without changing focus rules or making UIItem transparent to input.
    let (ox, oy) = ui.screen_origin(h);
    let hit = ui.hit_test_screen(ox + 16, oy + 16).expect("visible hit");
    assert_eq!(
        hit,
        w.slots[i].handle,
        "hit {:?} at {:?}; list origin {:?}",
        ui.node(hit).unwrap().element_id(),
        ui.node(hit).unwrap().region.box_,
        (ox, oy)
    );
    ui.mouse_down(dereth_ui::focus::action::WHEEL_DOWN, ox + 16, oy + 16);
    assert_eq!(w.scroll(&ui), (0, 2 * ch));

    // A contents refill and a hide/show of this same pack preserve the offset and geometry.
    view.items.swap(0, 1);
    assert!(panel.update(&mut ui, &view));
    let w = panel.item_list.as_ref().unwrap();
    assert_eq!(w.scroll(&ui), (0, 2 * ch));
    assert_eq!(ui.node(w.slots[0].handle).unwrap().region.box_.y0, -2 * ch);
    ui.set_visible(page, false);
    ui.set_visible(page, true);
    panel.update(&mut ui, &view);
    assert_eq!(panel.item_list.as_ref().unwrap().scroll(&ui), (0, 2 * ch));

    // The production container-click consumer changes the parent, which resets the child view.
    let side_slot = panel.container_list.as_ref().unwrap().slots[0].handle;
    assert_eq!(
        panel.on_slot_clicked(&mut ui.requests, side_slot),
        Some(ObjectId(0x6000))
    );
    assert!(panel.update(&mut ui, &view));
    assert_eq!(panel.item_list.as_ref().unwrap().scroll(&ui), (0, 0));
}

fn shop_view() -> View {
    View {
        shop: ShopView {
            open: true,
            vendor: Some(ObjectId(0x7000)),
            stock: (0..40)
                .map(|i| ShopRow {
                    item: ObjectId(0x8000 + i),
                    name: format!("Item {i}"),
                    amount: -1,
                    obj_type: 1,
                    ..ShopRow::default()
                })
                .collect(),
            type_filters: vec![("Weapons", 1)],
            ..ShopView::default()
        },
        ..View::default()
    }
}

#[test]
fn vendor_filter_scroll_to_show_moves_real_slots_and_out_of_range_is_a_noop() {
    let (mut ui, root) = env();
    let mut panel = VendorPanel::default();
    panel.post_init(&mut ui, root);
    let view = shop_view();
    assert!(panel.update(&mut ui, &view));
    let w = panel.stock.as_mut().expect("vendor stock grid");
    show_ancestors(&mut ui, w.handle);
    w.update_layout(&mut ui);
    assert_eq!(w.cell, (32, 32), "retail vendor ItemSlot cell");
    assert!(w.scroll_to_show(&mut ui, 25));
    let offset = w.scroll(&ui);
    assert_ne!(offset, (0, 0));
    assert_eq!(
        ui.node(w.slots[0].handle).unwrap().region.box_.x0,
        -offset.0
    );
    assert!(!w.scroll_to_show(&mut ui, usize::MAX));
    assert_eq!(w.scroll(&ui), offset);
    let h = w.handle;
    // Source guard accepts a nonempty filtered list, then resets to the first actual row.
    assert_eq!(panel.update_items_list(&mut ui, &view.shop, 1, false), 40);
    assert_eq!(panel.stock.as_ref().unwrap().scroll(&ui), (0, 0));
    assert_eq!(
        ui.node(panel.stock.as_ref().unwrap().slots[0].handle)
            .unwrap()
            .region
            .box_
            .x0,
        0
    );
    assert!(set_scroll_offset(&mut ui, h, i32::MAX, i32::MAX));
    let l = list(&ui, h);
    let b = ui.node(h).unwrap().region.box_;
    assert_eq!(
        (l.scroll.x, l.scroll.y),
        (
            (l.scroll.width - b.width()).max(0),
            (l.scroll.height - b.height()).max(0)
        )
    );
}

fn deliver_filter_callbacks(
    ui: &mut UiSystem,
    panel: &mut VendorPanel,
    view: &dyn GameView,
) -> usize {
    let mut delivered = 0;
    for delivery in ui.drain_outbox() {
        if let dereth_ui::Delivery::Element {
            to: dereth_ui::ListenerId::External(743),
            msg,
        } = delivery
        {
            assert!(panel.on_element_message(ui, &msg, view));
            delivered += 1;
        }
    }
    delivered
}

/// Behaviour: vendor.stock.scroll-survives-a-repaint-and-a-refilter-shrinks-the-extent
#[test]
fn vendor_repaint_preserves_scroll_and_open_restores_x_after_its_real_menu_callback() {
    let (mut ui, root) = env();
    let mut panel = VendorPanel::default();
    panel.post_init(&mut ui, root);
    ui.register_for_element_message(
        ElementId(0x1000_00BF),
        MessageId(7),
        dereth_ui::ListenerId::External(743),
    );
    let mut view = shop_view();
    assert!(panel.update(&mut ui, &view));
    // The menu's set-selected-item calls both the list box's own select and its new-selection
    // hook. The list's message-4 relay and the explicit call each emit message 7.
    assert_eq!(deliver_filter_callbacks(&mut ui, &mut panel, &view), 2);
    let w = panel.stock.as_mut().unwrap();
    show_ancestors(&mut ui, w.handle);
    assert!(w.scroll_to_show(&mut ui, 25));
    let offset = w.scroll(&ui);
    assert!(offset.0 > 0);
    // Selection arrives through GameView, the production repaint's existing input. It must
    // update quantity/decoration without resetting the viewport to the first stock row.
    view.selected = Some(view.shop.stock[25].item);
    assert!(panel.update(&mut ui, &view));
    assert_eq!(panel.stock.as_ref().unwrap().scroll(&ui), offset);
    assert_eq!(deliver_filter_callbacks(&mut ui, &mut panel, &view), 0);

    // A changed vendor produces the real OpenVendor edge, not a test-only direct restoration.
    view.shop.vendor = Some(ObjectId(0x7001));
    assert!(panel.update(&mut ui, &view));
    assert_eq!(panel.stock.as_ref().unwrap().scroll(&ui).0, offset.0);
    assert_eq!(deliver_filter_callbacks(&mut ui, &mut panel, &view), 2);
    assert_eq!(
        panel.stock.as_ref().unwrap().scroll(&ui).0,
        offset.0,
        "the queued MENU_CHOSEN must not undo the vendor open's trailing scroll"
    );

    // The next real filter-menu selection is not an OpenVendor callback; it scrolls home.
    let menu = panel.type_menu.unwrap();
    ui.broadcast_element_message(menu, MessageId(7), 0, 0);
    assert_eq!(deliver_filter_callbacks(&mut ui, &mut panel, &view), 1);
    assert_eq!(panel.stock.as_ref().unwrap().scroll(&ui), (0, 0));
}

#[test]
fn vendor_refilter_shrinks_scroll_extent_without_counting_hidden_cached_slots() {
    let (mut ui, root) = env();
    let mut panel = VendorPanel::default();
    panel.post_init(&mut ui, root);
    let mut view = shop_view();
    view.shop.stock[0].obj_type = 2;
    assert!(panel.update(&mut ui, &view));
    let w = panel.stock.as_mut().unwrap();
    show_ancestors(&mut ui, w.handle);
    assert!(w.scroll_to_show(&mut ui, 25));
    let h = w.handle;
    let old_count = w.slots.len();
    assert_eq!(panel.update_items_list(&mut ui, &view.shop, 2, false), 1);
    let w = panel.stock.as_ref().unwrap();
    assert!(
        w.slots.len() < old_count,
        "update_empty_slots removes trailing empty active rows"
    );
    assert_eq!(w.item_at(0), Some(view.shop.stock[0].item));
    assert_eq!(w.scroll(&ui), (0, 0));
    let l = list(&ui, h);
    assert_eq!(l.items.len(), w.slots.len());
    assert!(
        ui.children(h).len() > l.items.len(),
        "old slots remain hidden in the existing cache"
    );
    let b = ui.node(h).unwrap().region.box_;
    assert!(l.scroll.width <= b.width());
    assert!(
        !set_scroll_offset(&mut ui, h, i32::MAX, i32::MAX),
        "cached rows must not leave a phantom scrollbar range"
    );
    // A subsequent global tick must not rediscover cache children as live rows.
    ui.broadcast_global(dereth_ui::msg::global::TICK, 0);
    assert_eq!(panel.stock.as_ref().unwrap().scroll(&ui), (0, 0));
    assert!(list(&ui, h).scroll.width <= b.width());
    let before_created = panel.stock.as_ref().unwrap().created;
    let before_children = ui.children(h).len();
    for _ in 0..3 {
        assert_eq!(panel.update_items_list(&mut ui, &view.shop, 1, false), 39);
        assert_eq!(panel.update_items_list(&mut ui, &view.shop, 2, false), 1);
        assert!(
            list(&ui, h).scroll.width <= b.width(),
            "every refill excludes cached rows"
        );
    }
    assert_eq!(
        panel.stock.as_ref().unwrap().created,
        before_created,
        "creating a row must reuse the FIFO cache on later refills"
    );
    assert_eq!(ui.children(h).len(), before_children);
}
