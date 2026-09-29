//! On the shipped gameplay tree, moving the open child container reopens the top container while
//! unrelated moves do not; notice ordering, unknown and sealed-parent controls; the environment
//! host covers/closes/reopens.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use dereth_primitives::{DataId, ObjectId};
use dereth_ui::framework::LayoutEnum;
use dereth_ui::{ElemHandle, ElementId, ElementMessage, UiSystem};
use dereth_ui_screens::panels::external_container::{
    ExternalContainerNotice as Notice, ExternalContainerPanel, PANEL,
};
use dereth_ui_screens::panels::panel_stack::PanelStack;
use dereth_ui_screens::view::{GameView, SlotDecoration, UiRequest};

const CORPSE: ObjectId = ObjectId(101);
const PACK: ObjectId = ObjectId(102);
const LOOSE: ObjectId = ObjectId(103);
const INSIDE: ObjectId = ObjectId(104);

fn env() -> (UiSystem, ExternalContainerPanel, PanelStack) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let root = dereth_ui_screens::env::create_and_add_root_element(
        &mut ui,
        LayoutEnum(0x1000_0006),
        ElementId(0x1000_0495),
    )
    .expect("shipped gameplay root");
    let mut panel = ExternalContainerPanel::default();
    panel.post_init(&mut ui, root);
    assert!(
        panel.root.is_some()
            && panel.top_container.is_some()
            && panel.container_list.is_some()
            && panel.item_list.is_some()
    );
    let host = ui
        .get_child_recursive(
            root,
            dereth_ui_screens::screens::gameplay::window::ENV_PANEL,
        )
        .unwrap();
    let mut stack = PanelStack::default();
    stack.setup_children_with(
        &mut ui,
        host,
        &dereth_ui_screens::panels::catalogue::ENV_PANEL_PAGES,
    );
    stack.root = Some(host);
    let _ = ui.requests.take();
    (ui, panel, stack)
}

#[derive(Debug, Default)]
struct View {
    pack_moved: bool,
    sealed: bool,
}
impl GameView for View {
    fn container_contents(&self, id: ObjectId) -> &[ObjectId] {
        if id == CORPSE {
            &[LOOSE]
        } else if id == PACK {
            &[INSIDE]
        } else {
            &[]
        }
    }
    fn contained_containers(&self, id: ObjectId) -> &[ObjectId] {
        if id == CORPSE && !self.pack_moved {
            &[PACK]
        } else {
            &[]
        }
    }
    fn slot_decoration(&self, id: ObjectId) -> Option<SlotDecoration> {
        [CORPSE, PACK, LOOSE, INSIDE]
            .contains(&id)
            .then_some(SlotDecoration {
                is_container: [CORPSE, PACK].contains(&id),
                openable: !self.sealed,
                items_capacity: 10,
                containers_capacity: 2,
                icon_id: 0x0600_13a5,
                ..Default::default()
            })
    }
    fn icon(&self, _: ObjectId) -> Option<DataId> {
        Some(DataId(0x0600_13a5))
    }
    fn items_capacity(&self, _: ObjectId) -> Option<i32> {
        Some(10)
    }
    fn containers_capacity(&self, _: ObjectId) -> Option<i32> {
        Some(2)
    }
}

fn press(ui: &UiSystem, source: ElemHandle) -> ElementMessage {
    ElementMessage {
        source,
        source_id: ui.node(source).unwrap().desc.element_id,
        id: dereth_ui::msg::element::id::MOUSE_PRESS,
        p1: 7,
        p2: 0,
        point: Default::default(),
        serial: 1,
    }
}

/// Behaviour: inventory.external-container.moving-the-open-child-reopens-the-top-and-unrelated-moves-do-not
#[test]
fn moving_the_open_child_reopens_the_top_but_unrelated_moves_do_not() {
    let (mut ui, mut panel, _) = env();
    let mut view = View::default();
    panel.recv_notice(&mut ui, Notice::SetGroundObject(CORPSE), &view);
    assert!(panel.item_list.as_ref().unwrap().is_in_list(LOOSE));
    let slot = panel.container_list.as_ref().unwrap().slots[0].handle;
    let message = press(&ui, slot);
    assert!(panel.on_element_message(&mut ui, &message, &view));
    panel.update(&mut ui, &view);
    assert_eq!(panel.open_container, Some(PACK));
    assert!(panel.item_list.as_ref().unwrap().is_in_list(INSIDE));
    assert_eq!(panel.top_container.as_ref().unwrap().open_item_id, None);
    assert_eq!(
        panel.container_list.as_ref().unwrap().open_item_id,
        Some(PACK)
    );
    panel.recv_notice(
        &mut ui,
        Notice::ItemMoved {
            object: LOOSE,
            container: ObjectId(500),
        },
        &view,
    );
    assert_eq!(panel.open_container, Some(PACK));
    panel.recv_notice(
        &mut ui,
        Notice::ItemMoved {
            object: PACK,
            container: CORPSE,
        },
        &view,
    );
    assert_eq!(
        panel.open_container,
        Some(PACK),
        "reorder within the same parent does not close child"
    );
    view.pack_moved = true;
    panel.recv_notice(
        &mut ui,
        Notice::ItemMoved {
            object: PACK,
            container: ObjectId(500),
        },
        &view,
    );
    assert_eq!(panel.open_container, Some(CORPSE));
    assert!(panel.item_list.as_ref().unwrap().is_in_list(LOOSE));
    assert!(!panel.container_list.as_ref().unwrap().is_in_list(PACK));
    assert_eq!(
        panel.top_container.as_ref().unwrap().open_item_id,
        Some(CORPSE)
    );
    assert!(ui
        .requests
        .take()
        .contains(&UiRequest::NewParentContainer(CORPSE)));
}

#[test]
fn notice_order_close_reopen_unknown_and_sealed_parent_controls() {
    let (mut ui, mut panel, _) = env();
    let view = View::default();
    panel.recv_notice(&mut ui, Notice::SetGroundObject(CORPSE), &view);
    panel.recv_notice(&mut ui, Notice::SetGroundObject(ObjectId(0)), &view);
    assert_eq!(panel.ground_object, None);
    assert!(panel
        .item_list
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .all(|s| s.item.is_none()));
    panel.recv_notice(&mut ui, Notice::SetGroundObject(PACK), &view);
    assert_eq!(panel.ground_object, Some(PACK));
    assert!(panel.item_list.as_ref().unwrap().is_in_list(INSIDE));
    assert!(
        !ui.requests
            .take()
            .iter()
            .any(|r| matches!(r, UiRequest::Use(_) | UiRequest::CloseExternalContainer(_))),
        "zero notice never echoes a close"
    );
    panel.recv_notice(&mut ui, Notice::SetGroundObject(ObjectId(999)), &view);
    assert_eq!(panel.ground_object, Some(ObjectId(999)));
    assert_eq!(
        panel.item_list.as_ref().unwrap().parent_container,
        None,
        "SetParentContainer rejects missing weenie"
    );
    assert!(panel
        .item_list
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .all(|s| s.item.is_none()));
    panel.recv_notice(
        &mut ui,
        Notice::SetGroundObject(CORPSE),
        &View {
            sealed: true,
            ..Default::default()
        },
    );
    assert_eq!(
        panel.item_list.as_ref().unwrap().parent_container,
        Some(CORPSE)
    );
    assert!(
        panel
            .item_list
            .as_ref()
            .unwrap()
            .slots
            .iter()
            .all(|s| s.item.is_none()),
        "non-openable non-player container keeps no visible contents"
    );
}

/// Behaviour: inventory.environment-host.covers-closes-and-reopens-without-toolbar-restore
#[test]
fn environment_host_covers_closes_and_reopens_without_toolbar_restore_rules() {
    let (mut ui, mut panel, mut stack) = env();
    let view = View::default();
    let page = stack
        .pages
        .iter()
        .find(|p| p.element == PANEL)
        .copied()
        .unwrap();
    let other = stack
        .pages
        .iter()
        .find(|p| p.element != PANEL)
        .copied()
        .unwrap();
    let root = stack.root.unwrap();
    panel.recv_notice(&mut ui, Notice::SetGroundObject(CORPSE), &view);
    let requested_height = ui.node(page.handle).unwrap().region.box_.height();
    let requested_width = ui.node(root).unwrap().region.box_.width();
    let expected_size = PanelStack::size_clamps(&ui, root).apply(requested_width, requested_height);
    stack.recv_env_panel_visibility(&mut ui, page.panel_id, true);
    assert!(ui.node(root).unwrap().region.flags.visible);
    assert_eq!(
        ui.node(root).unwrap().region.box_.height(),
        expected_size.1,
        "GetHeight is sampled BEFORE resizing; layout anchors then reflow the child"
    );
    let events = stack.recv_env_panel_visibility(&mut ui, other.panel_id, true);
    assert!(events.contains(&UiRequest::SetPanelVisibility {
        panel: page.panel_id,
        visible: false
    }));
    panel.update(&mut ui, &view);
    assert_eq!(panel.ground_object, None);
    assert_eq!(
        ui.requests
            .take()
            .iter()
            .filter(|r| matches!(r, UiRequest::CloseExternalContainer(id) if *id == CORPSE))
            .count(),
        1
    );
    stack.recv_env_panel_visibility(&mut ui, other.panel_id, false);
    assert_eq!(stack.current, None);
    assert!(
        !ui.node(root).unwrap().region.flags.visible,
        "ENV has no default page registered"
    );
    panel.recv_notice(&mut ui, Notice::SetGroundObject(CORPSE), &view);
    stack.recv_env_panel_visibility(&mut ui, page.panel_id, true);
    assert!(ui.node(root).unwrap().region.flags.visible);
    assert!(ui.node(page.handle).unwrap().region.flags.visible);
    assert!(
        !panel.update(&mut ui, &view),
        "unchanged frame does not flush the lists"
    );
}
