//! The shipped stack-split box's focus policy is read from its resolved properties; text focus
//! policies honour true/false/missing and release without a generic blur.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use dereth_ui::framework::LayoutEnum;
use dereth_ui::{ElementId, UiSystem};

fn gameplay() -> (UiSystem, dereth_ui::ElemHandle) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let root = dereth_ui_screens::env::create_and_add_root_element(
        &mut ui,
        LayoutEnum(0x1000_0006),
        ElementId(0x1000_0495),
    )
    .expect("gameplay DAT root");
    (ui, root)
}

/// Behaviour: ui.focus.a-text-boxs-focus-policy-comes-from-its-shipped-properties
#[test]
fn shipped_stack_box_focus_policy_is_consumed_from_its_resolved_properties() {
    let (mut ui, root) = gameplay();
    let entry = ui
        .get_child_recursive(root, dereth_ui_screens::toolbar::splitter::ENTRY_BOX)
        .expect("stack box");
    let props = ui.node(entry).unwrap().merged_properties();
    let escape = props.get_bool(0xcb);
    let accept = props.get_bool(0xcc);
    assert_eq!(escape, None, "the retail box does not author0xCB");
    assert_eq!(
        accept,
        Some(true),
        "the retail box explicitly authors0xCC=true"
    );
    let bits = ui.text_element_mut(entry).unwrap().bits;
    assert_eq!(
        bits.lose_focus_on_escape(),
        escape.unwrap_or(false),
        "AttributeChanged0xCB -> SetLoseFocusOnEscape4674F0"
    );
    assert_eq!(
        bits.lose_focus_on_accept(),
        accept.unwrap_or(false),
        "AttributeChanged0xCC -> SetLoseFocusOnAcceptInput467520"
    );
}

#[test]
fn text_focus_policies_honor_true_false_missing_and_release_without_generic_blur() {
    let (mut ui, root) = gameplay();
    let entry = ui
        .get_child_recursive(root, dereth_ui_screens::toolbar::splitter::ENTRY_BOX)
        .expect("stack box");
    for (property, action) in [
        (0xcb, dereth_ui::focus::action::ESCAPE),
        (0xcc, dereth_ui::focus::action::ACCEPT),
    ] {
        // Private runtime controls only; the shipped DAT remains read-only. A property change
        // sets the policy, but does not itself relinquish focus or synthesize an input action.
        ui.set_focus_element(Some(entry));
        ui.set_attribute_bool(entry, property, false);
        assert_eq!(ui.focus_element(), Some(entry));
        let event = dereth_ui::focus::InputEvent {
            action,
            start: true,
            x: 0,
            y: 0,
        };
        assert!(ui.dispatch_action(entry, &event));
        assert_eq!(
            ui.focus_element(),
            Some(entry),
            "false consumes without focus loss"
        );
        ui.set_attribute_bool(entry, property, true);
        assert_eq!(
            ui.focus_element(),
            Some(entry),
            "setting policy is not a focus gesture"
        );
        let release = dereth_ui::focus::InputEvent {
            start: false,
            ..event
        };
        assert!(!ui.dispatch_action(entry, &release));
        assert_eq!(
            ui.focus_element(),
            Some(entry),
            "OnAction only handles the starting edge"
        );
        assert!(ui.dispatch_action(entry, &event));
        assert_ne!(
            ui.focus_element(),
            Some(entry),
            "true makes the existing OnAction relinquish"
        );
        ui.set_focus_element(Some(entry));
        // `on_set_attribute` with no value treats the flag as false. This tests
        // that callback directly, not an invented property-table-removal/fallback operation.
        ui.on_set_attribute(entry, property, None);
        assert!(ui.dispatch_action(entry, &event));
        assert_eq!(
            ui.focus_element(),
            Some(entry),
            "NULL value clears, not retain previous true"
        );
    }
}
