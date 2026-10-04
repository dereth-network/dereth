//! The VENDOR catalogue spec lists all 31 children including the Selling page and matches the
//! measured tree; the sell basket names its bar; the Selling page's four buttons reach the
//! production requests.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use crate::common::*;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_ui::{Delivery, ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::bind::{bind_children, ChildBinding};
use dereth_ui_screens::panels::catalogue;
use dereth_ui_screens::panels::vendor::{self, VendorPanel};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{GameView, ShopRow, ShopView};
use dereth_ui_screens::UiRequest;

// ---------------------------------------------------------------------------------------------
// The live tree — `vendor_stock_scrollbar.rs`'s harness
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

fn vendor_spec() -> &'static [ChildBinding] {
    catalogue::spec("VendorPanel")
        .expect("VendorPanel is catalogued")
        .children
}

/// Make every ancestor of `h` visible so the pointer can reach it. The element's **own**
/// visibility is left alone.
fn show_path_to(ui: &mut UiSystem, h: ElemHandle) {
    let mut chain = Vec::new();
    let mut cur = Some(h);
    while let Some(c) = cur {
        chain.push(c);
        cur = ui.parent(c);
    }
    for c in chain.into_iter().rev() {
        ui.set_visible(c, true);
    }
}

// ---------------------------------------------------------------------------------------------
// 1. The spec is the measured tree
// ---------------------------------------------------------------------------------------------

/// `Button`, `Field`, `Menu`, `Panel`,
/// `Scrollbar`, `TextElement` — `dereth_ui::factory::ty` — and the described class
/// `ItemListWidget`.
const BUTTON: u32 = 0x01;
const FIELD: u32 = 0x03;
const MENU: u32 = 0x06;
const PANEL: u32 = 0x08;
const SCROLLBAR: u32 = 0x0B;
const TEXT: u32 = 0x0C;
const ITEM_LIST: u32 = 0x1000_0031;

/// The whole `VendorPanel` subtree as the shipped `client_local_English.dat` has it: field name,
/// id, parent id, engine type. **Thirty-one rows**, the last ten of them the Selling page.
///
/// `0x100000B8`'s parent is `0x10000062` — `VendorPanel` itself — and so is `btn_close`'s.
const TREE: [(&str, u32, u32, u32); 31] = [
    ("vendor_panel", 0x1000_00B8, 0x1000_0062, PANEL),
    ("tab_items", 0x1000_00B9, 0x1000_00B8, TEXT),
    ("tab_buying", 0x1000_00BA, 0x1000_00B8, TEXT),
    ("tab_selling", 0x1000_00BB, 0x1000_00B8, TEXT),
    ("page_items", 0x1000_00BC, 0x1000_00B8, FIELD),
    ("stock_list", 0x1000_00BD, 0x1000_00BC, ITEM_LIST),
    ("stock_list_scrollbar", 0x1000_00BE, 0x1000_00BC, SCROLLBAR),
    ("item_type_menu", 0x1000_00BF, 0x1000_00BC, MENU),
    ("item_name_text", 0x1000_00C0, 0x1000_00BC, TEXT),
    ("item_cost_text", 0x1000_00C1, 0x1000_00BC, TEXT),
    ("btn_buy", 0x1000_00C2, 0x1000_00BC, BUTTON),
    ("btn_add_to_list", 0x1000_00C3, 0x1000_00BC, BUTTON),
    ("page_buy", 0x1000_00C4, 0x1000_00B8, FIELD),
    ("buy_list", 0x1000_00C5, 0x1000_00C4, ITEM_LIST),
    ("buy_list_scrollbar", 0x1000_00C6, 0x1000_00C4, SCROLLBAR),
    ("buy_list_text", 0x1000_00C7, 0x1000_00C4, TEXT),
    ("buy_purse_text", 0x1000_00C8, 0x1000_00C4, TEXT),
    ("btn_buy_item", 0x1000_00C9, 0x1000_00C4, BUTTON),
    ("btn_buy_all", 0x1000_00CA, 0x1000_00C4, BUTTON),
    ("btn_buy_clear_item", 0x1000_00CB, 0x1000_00C4, BUTTON),
    ("btn_buy_clear_list", 0x1000_00CC, 0x1000_00C4, BUTTON),
    ("page_sell", 0x1000_00CD, 0x1000_00B8, FIELD),
    ("sell_list", 0x1000_00CE, 0x1000_00CD, ITEM_LIST),
    ("sell_list_scrollbar", 0x1000_00CF, 0x1000_00CD, SCROLLBAR),
    ("sell_list_text", 0x1000_00D0, 0x1000_00CD, TEXT),
    ("sell_purse_text", 0x1000_00D1, 0x1000_00CD, TEXT),
    ("btn_sell_item", 0x1000_00D2, 0x1000_00CD, BUTTON),
    ("btn_sell_all", 0x1000_00D3, 0x1000_00CD, BUTTON),
    ("btn_sell_clear_item", 0x1000_00D4, 0x1000_00CD, BUTTON),
    ("btn_sell_clear_list", 0x1000_00D5, 0x1000_00CD, BUTTON),
    ("btn_close", 0x1000_00D6, 0x1000_0062, BUTTON),
];

/// The vendor spec lists all thirty one children including the selling page.
#[test]
fn the_vendor_spec_lists_all_thirty_one_children_including_the_selling_page() {
    let spec: Vec<(&str, u32, Option<u32>)> = vendor_spec()
        .iter()
        .map(|c| (c.field, c.id.0, c.parent.map(|p| p.0)))
        .collect();
    let want: Vec<(&str, u32, Option<u32>)> = TREE
        .iter()
        .map(|(f, id, parent, _)| {
            (
                *f,
                *id,
                if *id == 0x1000_00B8 {
                    None
                } else {
                    Some(*parent)
                },
            )
        })
        .collect();
    assert_eq!(
        spec.len(),
        31,
        "the vendor window has 31 children, not {}: missing {:?}",
        spec.len(),
        want.iter()
            .filter(|(_, id, _)| !spec.iter().any(|(_, s, _)| s == id))
            .map(|(f, id, _)| format!("{f} {id:#010X}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(spec, want, "every row's field, id and recorded parent");
}

/// The classes and the parents, against the shipped dat. This is what makes the `child_under`
/// parents a claim: a wrong one resolves the wrong element, or none.
#[test]
fn the_vendor_spec_is_the_measured_tree() {
    let (ui, s) = gameplay();
    let root = s.root().expect("the gameplay root");

    // Step 1: the spec resolves completely, `bind_children`'s two-step lookup included.
    let b = bind_children(&ui, root, vendor_spec());
    assert!(
        b.is_complete(),
        "unresolved vendor children: {:?}",
        b.missing
            .iter()
            .map(|m| (m.field, m.id))
            .collect::<Vec<_>>()
    );
    assert_eq!(b.found.len(), 31, "and it resolves all thirty-one");

    // Step 2: each handle is the element the table says, in the place the table says.
    for (field, id, parent, ty) in TREE {
        let h = find(&ui, &s, ElementId(id));
        assert_eq!(id_of(&ui, h), id, "{field}");
        assert_eq!(ty_of(&ui, h), ty, "{field} {id:#010X}: engine type");
        assert_eq!(
            ui.parent(h).map(|p| id_of(&ui, p)),
            Some(parent),
            "{field} {id:#010X}: parent"
        );
        // and `bind_children` landed on that same handle, not on a same-id element elsewhere.
        assert_eq!(
            b.get(field),
            Some(h),
            "{field}: bind_children resolved this element"
        );
    }

    // Step 3: the row that is not where a reader would guess.
    let tabs = find(&ui, &s, vendor::TABS);
    let close = find(&ui, &s, vendor::BTN_CLOSE);
    assert!(
        !ui.is_ancestor_of(tabs, close),
        "btn_close 0x100000D6 is a sibling of the tab element 0x100000B8, not a descendant"
    );
    assert_eq!(
        ui.parent(close),
        ui.parent(tabs),
        "both hang off VendorPanel 0x10000062, which is why post_init cannot bind it from TABS"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. `0x100000CF` — the `ID_ACCOUNTED_FOR` `LAYOUT` line's station
// ---------------------------------------------------------------------------------------------

/// The sell basket names its bar in the layout.
#[test]
fn the_sell_basket_names_its_bar_in_the_layout() {
    let (ui, s) = gameplay();
    let list = find(&ui, &s, vendor::SELL_LIST);
    let bar = find(&ui, &s, ElementId(0x1000_00CF));

    assert_eq!(
        ui.node(list)
            .expect("alive")
            .merged_properties()
            .get_enum(dereth_ui::scrollable::attr::H_SCROLLBAR),
        Some(0x1000_00CF),
        "0x100000CE's H_SCROLLBAR (attribute 0x71) is 0x100000CF"
    );
    assert_eq!(
        ui.node(list)
            .expect("alive")
            .merged_properties()
            .get_enum(dereth_ui::scrollable::attr::V_SCROLLBAR),
        None,
        "and no vertical bar: the sell basket is one row of slots, scrolled sideways"
    );
    assert_eq!(ty_of(&ui, bar), SCROLLBAR, "0x100000CF is a Scrollbar");
}

// ---------------------------------------------------------------------------------------------
// 3. The page is live in this build
// ---------------------------------------------------------------------------------------------

const MISC: u32 = 0x0000_0200;

#[derive(Debug, Default)]
struct Shop(ShopView);
impl GameView for Shop {
    fn shop(&self) -> ShopView {
        self.0.clone()
    }
    fn selected_object(&self) -> Option<ObjectId> {
        Some(ObjectId(0x8000_0100))
    }
}

fn shop() -> Shop {
    Shop(ShopView {
        open: true,
        vendor: Some(ObjectId(0x8000_0001)),
        stock: (0..4)
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

/// Behaviour: vendor.sell.the-selling-pages-four-buttons-reach-their-requests
/// Clicking the selling pages four buttons reaches the production requests.
#[test]
fn clicking_the_selling_pages_four_buttons_reaches_the_production_requests() {
    let item = ObjectId(0x8000_0100);
    let expected: [(ElementId, UiRequest, &str); 5] = [
        (
            vendor::BTN_SELL_ITEM,
            UiRequest::VendorSellSingle { item },
            "Sell Item",
        ),
        (vendor::BTN_SELL_ALL, UiRequest::VendorSellAll, "Sell All"),
        (
            vendor::BTN_SELL_CLEAR_ITEM,
            UiRequest::VendorClearList {
                sell: true,
                item: Some(item),
            },
            "Clear Item",
        ),
        (
            vendor::BTN_SELL_CLEAR_LIST,
            UiRequest::VendorClearList {
                sell: true,
                item: None,
            },
            "Clear List",
        ),
        (vendor::BTN_CLOSE, UiRequest::VendorClose, "Close"),
    ];

    for (id, want, label) in expected {
        let (mut ui, mut s) = gameplay();
        let root = s.root().expect("the gameplay root");
        let mut p = VendorPanel::default();
        p.post_init(&mut ui, root);
        assert!(p.bound(), "the vendor panel binds against the shipped tree");
        let mut view = shop();
        view.0.sell_list.push(view.0.stock[0].clone());
        p.open_vendor_type_filters(&mut ui, &view.0);
        p.update(&mut ui, &view);

        let btn = find(&ui, &s, id);
        show_path_to(&mut ui, btn);
        ui.set_visible(btn, true);
        let b = ui.screen_box(btn);
        assert!(b.is_valid(), "{label} {:#010X} has a box to point at", id.0);
        let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);

        ui.requests.clear();
        let t = 100.0_f64;
        ui.use_time(LocalTime(t), &mut dereth_ui::NullInputPump);
        ui.mouse_move(LocalTime(t), x, y);
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
        pump(&mut ui, &mut s, &mut p, &view);

        let got = ui.requests.take();
        assert!(
            got.contains(&want),
            "{label} {:#010X}: the click produced {got:?}, not {want:?}",
            id.0
        );
        ui.requests.clear();
    }
}

/// Behaviour: vendor.controls.selection-follows-basket-membership
#[test]
fn basket_selection_controls_the_real_buttons_and_survives_stock_refresh() {
    #[derive(Debug)]
    struct View {
        shop: ShopView,
        selected: Option<ObjectId>,
    }
    impl GameView for View {
        fn shop(&self) -> ShopView {
            self.shop.clone()
        }
        fn selected_object(&self) -> Option<ObjectId> {
            self.selected
        }
    }
    let (mut ui, s) = gameplay();
    let mut panel = VendorPanel::default();
    panel.post_init(&mut ui, s.root().unwrap());
    let mut view = View {
        shop: shop().0,
        selected: Some(ObjectId(0x8000_0100)),
    };
    panel.update(&mut ui, &view);
    ui.drain_outbox();
    ui.requests.clear();
    view.shop.buy_list.push(view.shop.stock[0].clone());
    view.shop.sell_list.push(view.shop.stock[0].clone());
    for (selected, enabled) in [
        (view.selected, true),
        (Some(ObjectId(99)), false),
        (None, false),
    ] {
        view.selected = selected;
        panel.update(&mut ui, &view);
        for id in [
            vendor::BTN_BUY_ITEM,
            vendor::BTN_BUY_CLEAR_ITEM,
            vendor::BTN_SELL_ITEM,
            vendor::BTN_SELL_CLEAR_ITEM,
        ] {
            assert_eq!(
                ui.node(find(&ui, &s, id)).unwrap().state.0,
                if enabled { 1 } else { 0x0d }
            );
            ui.requests.clear();
            panel.handle_button_click(&mut ui.requests, id, selected, 1);
            assert_eq!(!ui.requests.take().is_empty(), enabled);
        }
        for id in [
            vendor::BTN_BUY_ALL,
            vendor::BTN_BUY_CLEAR_LIST,
            vendor::BTN_SELL_ALL,
            vendor::BTN_SELL_CLEAR_LIST,
        ] {
            assert_eq!(ui.node(find(&ui, &s, id)).unwrap().state.0, 1);
        }
        for list in [panel.buy.as_ref().unwrap(), panel.sell.as_ref().unwrap()] {
            assert_eq!(list.slots[0].selected, enabled);
            let ring = list.slots[0].selected_ring.unwrap();
            assert_eq!(ui.node(ring).unwrap().region.flags.visible, enabled);
        }
    }
    view.selected = Some(view.shop.buy_list[0].item);
    view.shop.stock.clear();
    view.shop.type_filters.clear();
    ui.drain_outbox();
    ui.requests.clear();
    panel.update(&mut ui, &view);
    for d in ui.drain_outbox() {
        if let Delivery::Element { msg, .. } = d {
            panel.on_element_message(&mut ui, &msg, &view);
        }
    }
    assert!(
        !ui.requests
            .take()
            .iter()
            .any(|r| matches!(r, UiRequest::Select(_))),
        "same vendor refresh must not replace basket selection"
    );
    assert!(panel.buy.as_ref().unwrap().slots[0].selected);
    view.shop.buy_list.clear();
    view.shop.sell_list.clear();
    panel.update(&mut ui, &view);
    for id in [
        vendor::BTN_BUY_ITEM,
        vendor::BTN_BUY_CLEAR_ITEM,
        vendor::BTN_SELL_ITEM,
        vendor::BTN_SELL_CLEAR_ITEM,
        vendor::BTN_BUY_ALL,
        vendor::BTN_BUY_CLEAR_LIST,
        vendor::BTN_SELL_ALL,
        vendor::BTN_SELL_CLEAR_LIST,
    ] {
        assert_eq!(ui.node(find(&ui, &s, id)).unwrap().state.0, 0x0d);
        ui.requests.clear();
        panel.handle_button_click(&mut ui.requests, id, view.selected, 1);
        assert!(ui.requests.take().is_empty());
    }
}
