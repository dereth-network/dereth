//! The local player's top container slot uses the backpack icon while other objects keep theirs;
//! the shipped container strip hides its disabled scrollbar while the grid keeps one.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

#![allow(clippy::pedantic)]

use crate::common::layout::RegistrationOrder;
use dereth_primitives::{DataId, ObjectId};
use dereth_ui::region::IconRecipe;
use dereth_ui::widgets::{listbox::ListBox, scrollbar::attr};
use dereth_ui::{ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::panels::inventory::InventoryPanels;
use dereth_ui_screens::view::{GameView, SlotDecoration};

#[derive(Debug)]
struct View {
    items: Vec<ObjectId>,
    packs: Vec<ObjectId>,
}
impl GameView for View {
    fn player(&self) -> Option<ObjectId> {
        Some(ObjectId(0x1000))
    }
    fn container_contents(&self, _: ObjectId) -> &[ObjectId] {
        &self.items
    }
    fn contained_containers(&self, _: ObjectId) -> &[ObjectId] {
        &self.packs
    }
    fn items_capacity(&self, _: ObjectId) -> Option<i32> {
        Some(102)
    }
    fn containers_capacity(&self, _: ObjectId) -> Option<i32> {
        Some(7)
    }
    fn icon(&self, _: ObjectId) -> Option<DataId> {
        Some(DataId(0x0600_13A5))
    }
    fn slot_decoration(&self, id: ObjectId) -> Option<SlotDecoration> {
        Some(SlotDecoration {
            is_player: Some(id) == self.player(),
            is_container: self.packs.contains(&id),
            obj_type: if self.packs.contains(&id) {
                0x200
            } else {
                0x10
            },
            icon_id: self.icon(id).unwrap().0,
            items_capacity: 102,
            ..SlotDecoration::default()
        })
    }
}

fn env() -> (UiSystem, InventoryPanels, View, ElemHandle) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let mut screen = dereth_ui_screens::screens::gameplay::GamePlayScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("shipped gameplay and production visibility setup");
    let page = ui
        .get_child_recursive(screen.root().unwrap(), ElementId(0x1000_018B))
        .unwrap();
    ui.set_visible(page, true);
    let mut panel = InventoryPanels::default();
    panel.post_init(&mut ui, page);
    let view = View {
        items: (0..102).map(|i| ObjectId(0x5000 + i)).collect(),
        packs: (0..3).map(|i| ObjectId(0x6000 + i)).collect(),
    };
    assert!(panel.update(&mut ui, &view));
    (ui, panel, view, page)
}

fn bar(ui: &UiSystem, h: ElemHandle) -> ElemHandle {
    ui.node(h)
        .unwrap()
        .behaviour
        .as_ref()
        .unwrap()
        .as_any()
        .unwrap()
        .downcast_ref::<ListBox>()
        .unwrap()
        .scroll
        .scrollbar(ui, h, false)
        .unwrap()
}

/// Behaviour: ui.item-cell.the-players-own-cell-draws-a-backpack-and-not-a-second-backdrop
#[test]
fn local_player_top_container_uses_backpack_enum_but_other_objects_keep_their_icons() {
    let (mut ui, mut panel, view, page) = env();
    let top = &panel.top_container.as_ref().unwrap().slots[0];
    assert_eq!(top.item, view.player(), "identity remains the local player");
    let backpack = dereth_ui_screens::env::did_by_enum(&ui, 7, 0x1000_0004).unwrap();
    assert_eq!(
        backpack,
        DataId(0x0600_127E),
        "the shipped backpack, not a second type tile"
    );
    let container_tile = dereth_ui_screens::env::did_by_enum(&ui, 0x1000_0004, 10).unwrap();
    assert_ne!(
        backpack, container_tile,
        "the icon is not the background it is drawn over"
    );
    assert_ne!(Some(backpack), view.icon(top.item.unwrap()));
    let expected = IconRecipe::Object {
        background: Some(container_tile),
        effects: dereth_ui_screens::env::did_by_enum(&ui, 0x1000_0005, 0x21),
        icon: Some(backpack),
        overlay: None,
        underlay: None,
    };
    assert_eq!(top.icon_recipe(&ui), Some(expected));
    for w in panel.container_list.iter().chain(panel.item_list.iter()) {
        let s = &w.slots[0];
        let Some(IconRecipe::Object { icon, .. }) = s.icon_recipe(&ui) else {
            panic!("recipe")
        };
        assert_eq!(
            icon,
            view.icon(s.item.unwrap()),
            "not an override for other containers/creatures"
        );
    }
    let handle = top.handle;
    ui.set_visible(page, false);
    ui.set_visible(page, true);
    panel.update(&mut ui, &view);
    let top = &panel.top_container.as_ref().unwrap().slots[0];
    assert_eq!(top.handle, handle);
    assert_eq!(top.icon_recipe(&ui), Some(expected));
}

/// Behaviour: inventory.container-strip.hides-its-disabled-bar-and-the-grid-keeps-its-scrollbar
#[test]
fn shipped_container_strip_hides_its_disabled_bar_and_grid_keeps_its_scrollbar() {
    let (mut ui, mut panel, _, page) = env();
    let side = panel.container_list.as_ref().unwrap().handle;
    let grid = panel.item_list.as_ref().unwrap().handle;
    let side_bar = bar(&ui, side);
    let grid_bar = bar(&ui, grid);
    for h in [side_bar, grid_bar] {
        let n = ui.node(h).unwrap();
        println!(
            "bar {:?}: {:?}, disabled={:?}, hide={:?}, visible={}",
            n.element_id(),
            n.region.box_,
            n.merged_properties().get_bool(attr::DISABLED),
            n.merged_properties().get_bool(attr::HIDE_WHEN_DISABLED),
            n.region.flags.visible
        );
        assert_eq!(
            n.merged_properties().get_bool(attr::HIDE_WHEN_DISABLED),
            Some(true)
        );
    }
    assert_eq!(
        ui.node(side_bar)
            .unwrap()
            .merged_properties()
            .get_bool(attr::DISABLED),
        Some(true)
    );
    assert_eq!(
        ui.node(grid_bar)
            .unwrap()
            .merged_properties()
            .get_bool(attr::DISABLED),
        Some(false)
    );
    assert!(
        !ui.node(side_bar).unwrap().region.flags.visible,
        "only the unnecessary side-pack bar hides"
    );
    assert!(ui.node(grid_bar).unwrap().region.flags.visible);
    let original = ui.node(side).unwrap().region.box_;
    ui.resize_to(side, original.width(), 64);
    panel
        .container_list
        .as_mut()
        .unwrap()
        .update_layout(&mut ui);
    assert!(
        ui.node(side_bar).unwrap().region.flags.visible,
        "overflow re-enables the same bar"
    );
    ui.resize_to(side, original.width(), original.height());
    panel
        .container_list
        .as_mut()
        .unwrap()
        .update_layout(&mut ui);
    assert!(!ui.node(side_bar).unwrap().region.flags.visible);
    ui.set_visible(page, false);
    ui.set_visible(page, true);
    assert!(!ui.node(side_bar).unwrap().region.flags.visible);
    assert!(ui.node(grid_bar).unwrap().region.flags.visible);
    ui.set_attribute_bool(side_bar, attr::HIDE_WHEN_DISABLED, false);
    assert!(
        ui.node(side_bar).unwrap().region.flags.visible,
        "disabled bars without HideDisabled remain visible"
    );
}
