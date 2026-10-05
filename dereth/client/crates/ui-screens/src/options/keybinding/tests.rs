use super::*;

/// Oracle: the shipped template `0x1000002F` of layout `0x21000009`, and
/// The action key map option control's post-init / the refresh.
///
/// **Literals, not symbols.** the stated testability rule: a test that reads a constant through the same
/// symbol it writes it through cannot detect a wrong constant — and `0x1000002B` was labelled
/// *the action id* in this crate and in the knowledge base until this unit read the layout.
#[test]
fn the_action_key_map_attributes_are_the_ones_the_client_reads() {
    assert_eq!(
        attr::CLEAR_BUTTON,
        0x1000_002A,
        "enum attribute -> the clear button"
    );
    assert_eq!(attr::KEY_BUTTONS, 0x1000_002B, "the key-button array");
    assert_eq!(
        attr::KEY_BUTTON_MEMBER,
        0x1000_002C,
        "each array member's property id"
    );
    assert_eq!(
        attr::BINDING_TEXT,
        0x49,
        "the string-info attribute Refresh writes"
    );
    assert_eq!(
        attr::CLEAR_DISABLED,
        0x0D,
        "the bool attribute on the clear button"
    );
    assert_eq!(
        DIALOG_QUEUE, 0x1000_0001,
        "the open-dialog check's queue, not the default 2"
    );
    assert_ne!(DIALOG_QUEUE, dereth_ui::dialog::factory::DEFAULT_QUEUE);
    assert_eq!(STRING_TABLE_ENUM, 0x1000_0004, "not 0x10000003");
    assert_eq!(DISABLED_STATE.0, 0x0D);
    assert_eq!(TEMPLATE_HEADER, 0);
    assert_eq!(TEMPLATE_ROW, 1);
    // The two message ids the row acts on, and the first parameter that makes the second one an
    // erase rather than a click.
    assert_eq!(dereth_ui::msg::element::id::MOUSE_CLICK.0, 0x19);
    assert_eq!(dereth_ui::focus::action::SECONDARY_CLICK, 8);
    assert_eq!(dereth_ui::msg::element::id::BUTTON_CLICKED.0, 1);
    // `DO_NOTHING` is the constant this whole unit turns on.
    assert_eq!(DO_NOTHING.0, 1, "action 1 is DoNothing");
}

/// Oracle: the client's six hard-coded page lookups, and
/// the action classes.
#[test]
fn the_page_has_six_tab_pages_and_six_action_classes() {
    assert_eq!(KEYBOARD_TAB_PAGES.len(), ACTION_CLASSES.len());
    assert_eq!(
        KEYBOARD_TAB_PAGES.map(|e| e.0),
        [
            0x1000_049D,
            0x1000_049F,
            0x1000_04A1,
            0x1000_04A3,
            0x1000_0211,
            0x1000_04A5
        ]
    );
    // Page order: the client keys the six pages 1, 2, 4, 3, 7, 5.
    assert_eq!(ACTION_CLASSES.map(|(c, _)| c), [1, 2, 4, 3, 7, 5]);
    assert!(
        !ACTION_CLASSES.iter().any(|(c, _)| *c == 0),
        "class 0 is not user-bindable"
    );
    assert_eq!(page_attr::LIST_BOX_CHILD, 0x1000_0018);
    assert_eq!(page_attr::LOAD_KEYMAP_BUTTON, 0x1000_001E);
    assert_eq!(page_attr::SAVE_KEYMAP_BUTTON, 0x1000_001F);
    assert_eq!(page_attr::ALL.len(), 8);
    assert_eq!(page_attr::ALL[0], 0x1000_0018);
    assert_eq!(page_attr::ALL[7], 0x1000_001F);
}

/// The three dialogs' kinds, and the fact that the overwrite one is the only kind whose answer
/// property is `0x92` — which is what reads.
#[test]
fn the_overwrite_dialog_is_the_kind_whose_answer_is_property_0x92() {
    use dereth_ui::dialog::DialogKind;
    assert_eq!(dialog_kind(RowDialog::Overwrite), DialogKind::Confirmation);
    assert_eq!(dialog_kind(RowDialog::CantOverwrite), DialogKind::Message);
    assert_eq!(dialog_kind(RowDialog::MapWarn), DialogKind::Wait);
    assert_eq!(DialogKind::Confirmation.answer_property(), Some(0x92));
    assert_eq!(DialogKind::Message.answer_property(), None);
    assert_eq!(DialogKind::Wait.answer_property(), None);
    assert_eq!(super::super::pages::DIALOG_ANSWER_PROPERTY, 0x92);
}
