//! On the shipped inventory list, item assignment, spell assignment, clearing and root deletion
//! change a slot's runtime identity with no leak into reused slots or arena handles.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use dereth_primitives::ObjectId;
use dereth_ui::framework::LayoutEnum;
use dereth_ui::ElementId;
use dereth_ui_screens::items::runtime::identity;
use dereth_ui_screens::panels::inventory::InventoryPanels;

/// Behaviour: ui.item-cell.a-reused-slot-never-keeps-an-old-item-or-spell-identity
#[test]
fn real_item_assignment_spell_clear_and_reused_arena_never_leak_an_old_item_identity() {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let root = dereth_ui_screens::env::create_and_add_root_element(
        &mut ui,
        LayoutEnum(0x1000_0006),
        ElementId(0x1000_0495),
    )
    .expect("shipped gameplay root");
    let page = ui
        .get_child_recursive(root, ElementId(0x1000_018b))
        .expect("inventory page");
    let mut inventory = InventoryPanels::default();
    inventory.post_init(&mut ui, page);
    let list = inventory.item_list.as_mut().expect("real inventory list");
    let item = ObjectId(0x1234);
    assert_eq!(
        list.set_contents(&mut ui, None, Some(2), &[item], &|_| None),
        1
    );
    let handle = list.slots[0].handle;
    assert_eq!(identity(&ui, handle), Some((item, 0)));
    assert_eq!(identity(&ui, list.slots[1].handle), Some((ObjectId(0), 0)));
    let icon = list.slots[0].icon.expect("shipped icon child");
    assert!(
        identity(&ui, icon).is_none(),
        "exact dynamic cast, not an ancestor lookup"
    );
    assert!(
        identity(&ui, root).is_none(),
        "non-item root is not an empty UIItem"
    );
    let spell = dereth_ui_screens::view::SpellEntry {
        id: 17,
        name: "source-derived spell station".into(),
        icon: None,
        school: 1,
        level: 1,
        icon_power: 1,
        display_order: 0,
        bitfield: 0,
    };
    assert_eq!(list.set_spells(&mut ui, &[spell]), 1);
    assert_eq!(list.slots[0].handle, handle, "same live slot is reused");
    assert_eq!(identity(&ui, handle), Some((ObjectId(0), 17)));
    assert_eq!(
        list.set_contents(&mut ui, None, Some(2), &[item], &|_| None),
        1
    );
    assert_eq!(
        identity(&ui, handle),
        Some((item, 0)),
        "initialising the item slot clears spell identity"
    );
    list.set_contents(&mut ui, None, Some(2), &[], &|_| None);
    assert_eq!(identity(&ui, handle), Some((ObjectId(0), 0)));
    ui.remove_and_delete_root(root);
    assert!(identity(&ui, handle).is_none());
    let replacement = ui.create_hollow(None);
    assert_ne!(replacement, handle);
    assert!(identity(&ui, replacement).is_none());
}

mod drag_icon {
    //! A shipped inventory slot's drag icon is parented to the slot, is draggable, and the slot has no
    //! no-drag-proxy attribute.
    //! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

    use crate::common::layout::RegistrationOrder;

    use dereth_primitives::ObjectId;
    use dereth_ui::framework::LayoutEnum;
    use dereth_ui::ElementId;
    use dereth_ui_screens::panels::inventory::InventoryPanels;

    /// Behaviour: ui.item-cell.the-drag-icon-is-the-slots-own-child
    #[test]
    fn the_drag_icon_is_the_slots_child_and_the_slot_resolves_it_back() {
        let (mut ui, _flow, _store) =
            crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
        let root = dereth_ui_screens::env::create_and_add_root_element(
            &mut ui,
            LayoutEnum(0x1000_0006),
            ElementId(0x1000_0495),
        )
        .expect("shipped gameplay root");
        let page = ui
            .get_child_recursive(root, ElementId(0x1000_018b))
            .expect("inventory page");
        let mut inventory = InventoryPanels::default();
        inventory.post_init(&mut ui, page);
        let list = inventory.item_list.as_mut().expect("real inventory list");
        assert_eq!(
            list.set_contents(&mut ui, None, Some(2), &[ObjectId(0x1234)], &|_| None),
            1
        );

        let slot = list.slots[0].handle;
        // The slot's own back-reference to the icon it made.
        let icon = list.slots[0]
            .drag_icon
            .expect("the drag icon was created for the shipped slot");

        // The new element's parent is the parent the slot passed: the slot.
        assert_eq!(
            ui.parent(icon),
            Some(slot),
            "the drag icon's parent is its item slot, the element the slot passed as parent"
        );
        assert_ne!(
            icon, slot,
            "and it is a distinct element, not the slot itself"
        );

        // The two properties that together make that latent loop unreachable: the icon is
        // accepted at the first level of the walk, and its parent can build the proxy.
        assert!(
            ui.node(icon).expect("the icon is alive").flags.dragable(),
            "0x3A on the drag icon: the drag start takes its own arm at once and never climbs"
        );
        assert_eq!(
            ui.node(slot)
                .expect("the slot is alive")
                .merged_properties()
                .get_bool(dereth_ui::props::attr::NO_DRAG_PROXY),
            Some(false),
            "no 0x39 on the slot: the drag builds the proxy instead of refusing"
        );

        ui.remove_and_delete_root(root);
    }
}
