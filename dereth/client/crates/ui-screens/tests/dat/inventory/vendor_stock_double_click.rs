//! A double-click on a shopkeeper's stock row buys it, by the pointer, through the shipped tree.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_primitives::{LocalTime, ObjectId};
use dereth_ui::{Delivery, ElemHandle, Screen, UiSystem};
use dereth_ui_screens::panels::vendor::{self, VendorPanel};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{GameView, ShopRow, ShopView};
use dereth_ui_screens::UiRequest;

const MISC: u32 = 0x0000_0200;
/// A slider value other than the default 1, so the test sees which split the buy carries.
const SPLIT: i32 = 3;

#[derive(Debug, Default)]
struct Shop(ShopView);
impl GameView for Shop {
    fn shop(&self) -> ShopView {
        self.0.clone()
    }
    fn split_size(&self) -> i32 {
        SPLIT
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

/// The gameplay screen with a bound vendor panel showing `shop()`'s stock, every ancestor of the
/// stock list made visible so the pointer reaches its rows.
fn open_shop() -> (UiSystem, GamePlayScreen, VendorPanel, Shop) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    let root = s.root().expect("the gameplay root");
    let mut p = VendorPanel::default();
    p.post_init(&mut ui, root);
    assert!(p.bound(), "the vendor panel binds against the shipped tree");
    let view = shop();
    p.open_vendor_type_filters(&mut ui, &view.0);
    p.update(&mut ui, &view);
    assert_eq!(p.rows(vendor::Tab::Items).len(), 4, "every row is listed");
    let list = p.stock.as_ref().expect("the stock list").handle;
    let mut chain = Vec::new();
    let mut cur = Some(list);
    while let Some(h) = cur {
        chain.push(h);
        cur = ui.parent(h);
    }
    for h in chain.into_iter().rev() {
        ui.set_visible(h, true);
    }
    (ui, s, p, view)
}

/// The centre of the stock list's `slot`th row, and the item on it.
fn row(ui: &UiSystem, p: &VendorPanel, slot: usize) -> ((i32, i32), ObjectId) {
    let list = p.stock.as_ref().expect("the stock list");
    let h: ElemHandle = list.slots[slot].handle;
    let b = ui.screen_box(h);
    assert!(b.is_valid(), "stock row {slot} has a box to point at");
    let item = list.item_at(slot).expect("the row holds an item");
    (((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2), item)
}

fn click(ui: &mut UiSystem, action: u32, (x, y): (i32, i32)) {
    ui.mouse_down(action, x, y);
    ui.mouse_up(action, x, y, false);
}

/// Behaviour: vendor.buy.a-double-click-on-a-stock-row-buys-it
#[test]
fn a_double_click_on_a_stock_row_buys_that_row_at_the_slider_amount() {
    let (mut ui, mut s, mut p, view) = open_shop();
    let (at, item) = row(&ui, &p, 1);
    ui.use_time(LocalTime(100.0), &mut dereth_ui::NullInputPump);
    ui.mouse_move(LocalTime(100.0), at.0, at.1);
    ui.requests.clear();

    // The first press of the pair is a plain click: it selects, and buys nothing.
    click(&mut ui, dereth_ui::focus::action::PRIMARY_CLICK, at);
    pump(&mut ui, &mut s, &mut p, &view);
    let first = ui.requests.take();
    assert!(
        first.contains(&UiRequest::Select(item)),
        "the first click selects the row: {first:?}"
    );
    assert!(
        !first
            .iter()
            .any(|r| matches!(r, UiRequest::VendorBuySingle { .. })),
        "a single click never buys: {first:?}"
    );

    // The second press resolves to the double-click action.
    click(&mut ui, dereth_ui::focus::DOUBLE_CLICK_ACTIONS[0], at);
    pump(&mut ui, &mut s, &mut p, &view);
    let second = ui.requests.take();
    let buys: Vec<_> = second
        .iter()
        .filter(|r| matches!(r, UiRequest::VendorBuySingle { .. }))
        .collect();
    assert_eq!(
        buys,
        [&UiRequest::VendorBuySingle { item, split: SPLIT }],
        "the double-click buys the row under the pointer, once, at the slider amount: {second:?}"
    );
    assert!(
        !second.contains(&UiRequest::Use(item)),
        "the stock row is bought, never used: {second:?}"
    );
    assert!(
        !second
            .iter()
            .any(|r| matches!(r, UiRequest::VendorAddToBuyList { .. })),
        "the buy goes out on its own and not through the basket: {second:?}"
    );
}
