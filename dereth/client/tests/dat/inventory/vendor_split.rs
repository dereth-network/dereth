//! A rendered quantity reaches both the live and frozen vendor action views.

use super::icon_bench::{as_gameplay, shipped_gameplay};
use dereth_client_contract::{GameSnapshot, GameView, UiRequest};
use dereth_client_model::{vendor::ItemProfile, weenie::Weenie};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_ui_screens::panels::vendor::{VendorPanel, BTN_ADD_TO_LIST};

/// Behaviour: vendor.money.a-basket-is-counted-in-things-and-not-in-rows
#[test]
fn the_rendered_split_quantity_reaches_the_live_and_frozen_buy_basket() {
    let (mut ui, mut screen) = shipped_gameplay();
    let hud = dereth_client::hud::Hud::new();
    let mut objects = dereth_client::objects::ObjectStream::new();
    let mut interaction = dereth_client::interaction::Interaction::new();
    let item = ObjectId(0x8000_0010);
    let vendor = ObjectId(0x8000_0001);
    let mut object = Weenie::new(item);
    object.pwd.name = "Apple".into();
    object.pwd.obj_type = 0x20;
    object.pwd.stack_size = Some(100);
    object.pwd.max_stack_size = Some(100);
    object.pwd.container_id = Some(vendor);
    objects.world.shop.vendor_id = Some(vendor);
    objects.world.shop.stock.push(ItemProfile {
        iid: item,
        amount: -1,
        pwd: object.pwd.clone(),
    });
    objects.world.tables.weenies.insert(item, object);
    objects.world.selected = Some(item);
    objects.world.refresh_stack_split();
    assert_eq!(objects.world.split.split_size, 1);
    let gameplay = as_gameplay(&mut screen);
    gameplay.update_toolbar_selection(&mut ui, &hud.view(&objects));
    let entry = gameplay
        .toolbar_children
        .get("stack_size_entry_box")
        .unwrap();
    ui.text_element_mut(entry).unwrap().set_text("40");
    gameplay.on_stack_box_focus(&mut ui, false);
    interaction.queue(vec![], ui.requests.take());
    interaction.run_ui_requests(&mut objects.world, false, ServerTime(0.0));
    assert_eq!(
        (
            hud.view(&objects).split_size(),
            hud.view(&objects).max_split_size()
        ),
        (40, 100)
    );
    let frozen = GameSnapshot::from_view(&hud.view(&objects));
    assert_eq!((frozen.split_size(), frozen.max_split_size()), (40, 100));
    let mut panel = VendorPanel::default();
    panel.handle_button_click(
        &mut ui.requests,
        BTN_ADD_TO_LIST,
        frozen.selected_object(),
        frozen.split_size(),
    );
    let requests = ui.requests.take();
    assert_eq!(
        requests,
        vec![UiRequest::VendorAddToBuyList { item, split: 40 }]
    );
    interaction.queue(vec![], requests);
    interaction.run_ui_requests(&mut objects.world, false, ServerTime(0.0));
    assert_eq!(objects.world.shop.buy_list, vec![(item, 40)]);
    assert_eq!(hud.view(&objects).shop().buy_items, 40);
    objects.world.refresh_stack_split();
    gameplay.update_toolbar_selection(&mut ui, &hud.view(&objects));
    assert_eq!(
        gameplay.splitter.split_size, 40,
        "idle projection keeps the typed amount"
    );
}

/// Behaviour: inventory.split.shared-selection-projects-before-ui
#[test]
fn changed_stack_receipts_reseed_before_the_app_projects_the_toolbar() {
    use crate::common::app::{app_in_gameplay_unanswered, gameplay_screen};
    let player = ObjectId(1);
    let item = ObjectId(2);
    let mut app = app_in_gameplay_unanswered(2, Some(player));
    let world = &mut app.objects_mut().world;
    let mut row = Weenie::new(item);
    row.pwd.name = "Apples".into();
    row.pwd.obj_type = 0x20;
    row.pwd.container_id = Some(player);
    row.pwd.stack_size = Some(100);
    row.pwd.max_stack_size = Some(100);
    world.tables.weenies.insert(item, row);
    world.set_selected_object(
        Some(item),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    app.frame();
    assert_eq!(gameplay_screen(&mut app).1.splitter.split_size, 100);
    app.objects_mut().world.split.split_size = 40;
    app.objects_mut()
        .world
        .weenie_mut(item)
        .unwrap()
        .pwd
        .stack_size = Some(80);
    app.frame();
    let shown = gameplay_screen(&mut app).1.splitter;
    assert_eq!((shown.split_size, shown.max_split_size), (80, 80));
    assert_eq!(app.objects_mut().world.split.split_size, 80);
}
