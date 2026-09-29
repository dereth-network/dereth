//! Each dialog kind answers on its own buttons into its own property. The accept and cancel ids are
//! the comparisons in each subclass's own message listener (only `Confirmation` uses `0x17`/`0x19`;
//! a `ConfirmationTextInput`, the character-select delete warning, uses `0x2E`/`0x2F`), each answer
//! property is the one that subclass writes, each caption property is the one its `set_data` reads,
//! and each `d[0x8E]` kind number is both what the dialog-making function passes and what the
//! close-dialog notice switches on. A text-input confirmation records the button and the typed
//! string, and its cancel answers an empty string whatever is in the box.
//!
//! Fixture: the shipped `Dialog` layout (`0x2100003C`) from the retail dats, each kind's root built
//! through the real element factory. Every number is repeated beside the code it constrains.
//! Nothing touches a shard: a confirmed delete is asserted only as the property the dialog carries.

use dereth_primitives::{AssetSource, DataId};

use dereth_ui::dialog::base::child;
use dereth_ui::dialog::types::{dialog_element, set_dialog_data};
use dereth_ui::dialog::{AnswerRole, DialogKind};
use dereth_ui::msg::element::id as msgid;
use dereth_ui::msg::ListenerId;
use dereth_ui::props::attr;
use dereth_ui::{ElemHandle, ElementId, PropertyCollection, PropertyValue, UiSystem};

/// The shipped `Dialog` layout — layout enum 2, `0x2100003C`.
const DIALOG_LAYOUT: DataId = DataId(0x2100_003C);

const ALL_KINDS: [DialogKind; 7] = [
    DialogKind::Confirmation,
    DialogKind::Wait,
    DialogKind::Message,
    DialogKind::TextInput,
    DialogKind::ConfirmationTextInput,
    DialogKind::Menu,
    DialogKind::ConfirmationMenu,
];

// -------------------------------------------------------------------------------------------
// 1. The table, on its own
// -------------------------------------------------------------------------------------------

/// Behaviour: ui.dialog.each-kind-answers-on-its-own-buttons-into-its-own-property
///
/// The seven rows, as an ordered **(accept, cancel)** pair each.
///
/// Each dialog subclass compares the incoming element id against its own button ids.
/// The wait dialog delegates directly to the base handler without comparing any button id,
/// so it has no button answer of its own.
///
/// The last two assertions are the discriminating ones: **no kind but `Confirmation` uses `0x17`
/// or `0x19`**, and every two-button kind's accept and cancel differ.
#[test]
fn each_kind_names_its_own_accept_and_cancel_and_only_confirmation_uses_0x17_and_0x19() {
    let rows: [(DialogKind, Option<u32>, Option<u32>, &str); 7] = [
        (
            DialogKind::Confirmation,
            Some(0x17),
            Some(0x19),
            "the confirmation dialog's listener",
        ),
        (
            DialogKind::ConfirmationMenu,
            Some(0x22),
            Some(0x23),
            "the confirmation-menu listener",
        ),
        (
            DialogKind::ConfirmationTextInput,
            Some(0x2E),
            Some(0x2F),
            "the confirmation-text-input dialog's listener",
        ),
        (
            DialogKind::Menu,
            Some(0x1E),
            None,
            "the menu dialog's listener",
        ),
        (
            DialogKind::Message,
            Some(0x26),
            None,
            "the message dialog's listener",
        ),
        (
            DialogKind::TextInput,
            Some(0x2A),
            None,
            "the text-input dialog's listener",
        ),
        (
            DialogKind::Wait,
            None,
            None,
            "the wait dialog, which compares nothing",
        ),
    ];
    assert_eq!(
        rows.len(),
        ALL_KINDS.len(),
        "all seven kinds, no more and no fewer"
    );

    for (kind, accept, cancel, where_) in rows {
        let got = kind.answer_children();
        assert_eq!(got.0.map(|e| e.0), accept, "{kind:?} accept, per {where_}");
        assert_eq!(got.1.map(|e| e.0), cancel, "{kind:?} cancel, per {where_}");
        if let (Some(a), Some(c)) = got {
            assert_ne!(
                a, c,
                "{kind:?}: accept and cancel are two different buttons"
            );
            assert_eq!(kind.answer_role(a), Some(AnswerRole::Accept), "{kind:?}");
            assert_eq!(kind.answer_role(c), Some(AnswerRole::Cancel), "{kind:?}");
        }
        // A button no subclass sends is not an answer, whichever kind is asked.
        assert_eq!(kind.answer_role(ElementId(0x99)), None, "{kind:?}");
    }

    // Stated directly: only `Confirmation` answers on these ids.
    for kind in ALL_KINDS {
        let uses_confirmation_buttons = kind.answer_role(child::BUTTON1).is_some()
            || kind.answer_role(child::BUTTON2).is_some();
        assert_eq!(
            uses_confirmation_buttons,
            kind == DialogKind::Confirmation,
            "{kind:?}: 0x17/0x19 belong to confirmation dialog and to nothing else"
        );
    }
}

/// Behaviour: ui.dialog.each-kind-answers-on-its-own-buttons-into-its-own-property
///
/// The property each subclass writes its answer into, and the caption properties `set_data` reads.
///
/// `0x92` belongs to confirmation dialogs and **only** them: a caller that reads `0x92` off a
/// `ConfirmationTextInput` reads nothing.
#[test]
fn each_kind_answers_into_its_own_property_and_captions_its_own_buttons() {
    /// kind, answer property, accept caption, cancel caption.
    type Row = (DialogKind, Option<u32>, Option<u32>, Option<u32>);
    let rows: [Row; 7] = [
        // kind, answer property, accept caption, cancel caption
        (DialogKind::Confirmation, Some(0x92), Some(0x90), Some(0x91)),
        (
            DialogKind::ConfirmationMenu,
            Some(0xAB),
            Some(0xA8),
            Some(0xA9),
        ),
        (
            DialogKind::ConfirmationTextInput,
            Some(0x9C),
            Some(0x9A),
            Some(0x9B),
        ),
        (DialogKind::Menu, Some(0xA4), Some(0xA2), None),
        (DialogKind::Message, None, Some(0x95), None),
        (DialogKind::TextInput, Some(0x98), Some(0x97), None),
        (DialogKind::Wait, None, None, None),
    ];
    for (kind, answer, accept_cap, cancel_cap) in rows {
        assert_eq!(kind.answer_property(), answer, "{kind:?} answer property");
        assert_eq!(
            kind.caption_properties(),
            (accept_cap, cancel_cap),
            "{kind:?} captions"
        );
        // A caption property exists exactly where the button it captions does.
        let (a, c) = kind.answer_children();
        assert_eq!(
            a.is_some(),
            accept_cap.is_some(),
            "{kind:?}: accept button vs its caption"
        );
        assert_eq!(
            c.is_some(),
            cancel_cap.is_some(),
            "{kind:?}: cancel button vs its caption"
        );
    }

    // Only the two text kinds have a box to harvest, and each has its own.
    assert_eq!(
        DialogKind::ConfirmationTextInput.text_child(),
        Some(ElementId(0x2C))
    );
    assert_eq!(DialogKind::TextInput.text_child(), Some(ElementId(0x2B)));
    for kind in ALL_KINDS {
        let has_box = matches!(
            kind,
            DialogKind::ConfirmationTextInput | DialogKind::TextInput
        );
        assert_eq!(kind.text_child().is_some(), has_box, "{kind:?}");
    }

    // Every answer property is distinct, or two subclasses would be reading each other's answers.
    let mut props: Vec<u32> = ALL_KINDS
        .iter()
        .filter_map(|k| k.answer_property())
        .collect();
    let before = props.len();
    props.sort_unstable();
    props.dedup();
    assert_eq!(
        props.len(),
        before,
        "five answering subclasses, five distinct properties"
    );
    assert_eq!(before, 5, "Message and Wait answer nothing at all");
}

/// The **cross-check from the other end**, and it is what makes the dialog-factory table more than
/// a transcription: the five character-management dialog factories use exactly four
/// distinct `d[0x8E]` values, and those four are exactly the cases
/// the close-dialog notice demultiplexes on.
///
/// | which dialog is made | `d[0x8E]` | kind | root |
/// |---|---:|---|---:|
/// | the error message | 3 | `Message` | 0x24 |
/// | the delete-character confirmation | 5 | `ConfirmationTextInput` | 0x2C |
/// | the please-wait dialog | 2 | `Wait` | 0x31 |
/// | the entering-world dialog | 2 | `Wait` | 0x31 |
/// | the confirm-exit dialog | 1 | `Confirmation` | 0x15 |
///
/// And the switch: **case 1** reads property `0x92`, **case 5** reads `0x9C` and then calls
/// the delete-character close (`close_delete_character_dialog`), **cases 2 and 3** read nothing,
/// and **case 4** (`TextInput`) is an empty `break` — the kind whose root does not exist in any
/// shipped layout.
#[test]
fn the_char_screens_five_dialogs_and_the_close_notice_switch_agree_on_four_kinds() {
    let makes: [(&str, u32, DialogKind, u32); 5] = [
        ("the error-message dialog", 3, DialogKind::Message, 0x24),
        (
            "the delete-character confirmation",
            5,
            DialogKind::ConfirmationTextInput,
            0x2C,
        ),
        ("the please-wait dialog", 2, DialogKind::Wait, 0x31),
        ("the entering-world dialog", 2, DialogKind::Wait, 0x31),
        ("the confirm-exit dialog", 1, DialogKind::Confirmation, 0x15),
    ];
    for (name, prop, kind, root) in makes {
        assert_eq!(
            DialogKind::from_property(prop),
            Some(kind),
            "{name}: d[0x8E] = {prop}"
        );
        assert_eq!(kind.root_element_id().0, root, "{name}: root");
    }

    // The switch's four cases, as (case, the property that arm reads).
    let switch: [(u32, Option<u32>); 4] = [
        (1, Some(attr::DIALOG_ANSWER)),                 // 0x92
        (5, Some(attr::DIALOG_TEXT_INPUT_ANSWER_TEXT)), // 0x9C, then the delete-character close
        (2, None),                                      // the please-wait, nothing to read
        (3, None),                                      // the error message, nothing to read
    ];
    let mut seen: Vec<u32> = makes.iter().map(|m| m.1).collect();
    seen.sort_unstable();
    seen.dedup();
    let mut cases: Vec<u32> = switch.iter().map(|s| s.0).collect();
    cases.sort_unstable();
    assert_eq!(
        seen, cases,
        "the factory kinds and the switch cases are the same four"
    );

    for (case, reads) in switch {
        let kind = DialogKind::from_property(case).expect("a real kind");
        assert_eq!(
            kind.answer_property(),
            reads,
            "case {case} ({kind:?}) reads the property its own subclass writes"
        );
    }
    // Case 4 is an empty `break`, and `TextInput` has no root in any of the 101 shipped layouts.
    assert_eq!(DialogKind::from_property(4), Some(DialogKind::TextInput));
    assert!(
        !makes.iter().any(|m| m.1 == 4),
        "no dialog factory asks for a TextInput"
    );
}

// -------------------------------------------------------------------------------------------
// 2. A live `ConfirmationTextInput`, built out of the shipped layout
// -------------------------------------------------------------------------------------------

struct Env {
    store: dereth_dat::RetailDatStore,
    types: dereth_assets::ui::PropertyTypes,
}

/// Require the local DAT fixture: missing input must fail this tier rather than skip its checks.
fn env() -> Env {
    let dir = dereth_dat::testing::dat_dir();
    let store = dereth_dat::RetailDatStore::open_dir(&dir)
        .unwrap_or_else(|e| panic!("retail dats under {}: {e}", dir.display()));
    let master_id = DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("MasterProperty 0x39000001");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("the property type table");
    let types = master.property_types();
    Env { store, types }
}

/// The dialog factory's element half: the `Dialog` layout, the kind's own
/// root, initialised.
fn raise(e: &Env, kind: DialogKind) -> (UiSystem, ElemHandle) {
    let mut ui = UiSystem::new((800, 600));
    ui.property_types.clone_from(&e.types);
    let h = ui
        .create_root_by_data_id(&e.store, DIALOG_LAYOUT, kind.root_element_id())
        .unwrap_or_else(|err| panic!("{kind:?} root {:?}: {err}", kind.root_element_id()));
    ui.initialize_tree(h);
    (ui, h)
}

/// A player typing into the box the dialog harvests: focus it and feed characters through
/// the text element's own character handler, rather than writing the field by hand.
fn type_into(ui: &mut UiSystem, dialog: ElemHandle, box_id: ElementId, s: &str) -> ElemHandle {
    let b = ui
        .get_child_recursive(dialog, box_id)
        .expect("the edit box is in the layout");
    ui.set_focus_element(Some(b));
    for ch in s.encode_utf16() {
        ui.character(ch);
    }
    b
}

/// A button's mouse-up raising element message **1** from a child of the
/// dialog. The message bubbles to the dialog root, which is where `DialogElement` listens.
fn press(ui: &mut UiSystem, dialog: ElemHandle, id: ElementId) {
    let b = ui
        .get_child_recursive(dialog, id)
        .unwrap_or_else(|| panic!("child {id:?}"));
    ui.broadcast_element_message(b, msgid::BUTTON_CLICKED, 0, 0);
}

fn text_of(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// **A text-input confirmation records the button and the string.** A `ConfirmationTextInput` —
/// the char-select DELETE warning's kind — built
/// through the real element factory out of the real layout, typed into and answered, records both
/// the button and the string.
///
/// The first assertion is that `0x17`/`0x19` are inert on this kind, the same statement from the
/// other side: a dialog watching them would never answer the *Done* press below.
///
/// Falsified by: transposing `ConfirmationTextInput`'s row (Cancel would then harvest the phrase);
/// pointing `text_child` at `0x2B` or at the prompt `0x3E`; dropping the harvest so the answer is
/// the button alone.
#[test]
fn a_confirmation_text_input_records_the_button_and_the_typed_string() {
    let e = env();
    let (mut ui, h) = raise(&e, DialogKind::ConfirmationTextInput);
    assert_eq!(
        ui.node(h).expect("live").element_id(),
        ElementId(0x2C),
        "d[0x8E] = 5 asks for root 0x2C"
    );
    assert!(
        dialog_element(&ui, h).is_some(),
        "the root really is a DialogElement"
    );
    assert_eq!(
        dialog_element(&ui, h).unwrap().kind,
        DialogKind::ConfirmationTextInput
    );

    let box_ = type_into(&mut ui, h, ElementId(0x2C), "DELETE");
    assert_eq!(
        text_of(&mut ui, box_),
        "DELETE",
        "the characters reached the character handler"
    );
    assert_ne!(
        box_, h,
        "get_child_recursive(0x2C) finds the descendant, not the receiver"
    );

    // The confirmation kind's ids. A confirmation text-input element never sees either, and
    // neither exists in its layout at all.
    for stale in [child::BUTTON1, child::BUTTON2] {
        assert!(
            ui.get_child_recursive(h, stale).is_none(),
            "{stale:?} is not in a confirmation text-input dialog"
        );
    }
    assert_eq!(
        dialog_element(&ui, h).unwrap().answer,
        None,
        "nothing pressed yet"
    );

    // **The production shape, not a stripped one.** `CharacterManagementScreen::make_dialog`
    // registers the screen on the button *by id*, and `broadcast_element_message` consults that
    // table **before** the element's own
    // listener. An `External` listener answers `Default`, so the message goes on to bubble to
    // the dialog root; if it did not, everything below would be measuring a message the running
    // client never delivers.
    ui.register_for_element_message(
        child::CONFIRM_TEXT_INPUT_ACCEPT,
        msgid::BUTTON_CLICKED,
        ListenerId::External(0x1000_0175),
    );

    press(&mut ui, h, child::CONFIRM_TEXT_INPUT_ACCEPT);

    assert_eq!(
        ui.drain_outbox().len(),
        1,
        "the by-id listener was reached first, and answered"
    );
    let d = dialog_element(&ui, h).expect("still live");
    assert_eq!(d.answer, Some(ElementId(0x2E)), "Done");
    assert_eq!(d.answer_role, Some(AnswerRole::Accept));
    assert_eq!(
        d.answer_text.as_deref(),
        Some("DELETE"),
        "child 0x2C's text, into property 0x9C"
    );
    assert_eq!(
        d.answer_property(),
        Some((0x9C, PropertyValue::String("DELETE".into()))),
        "the dialog-close handler's case 5 reads exactly this key, then compares it ignoring case"
    );
}

/// The confirmation-text-input dialog's cancel writes an **empty** `0x9C` — so a cancel
/// is `Some("")` and never `None`, and never the phrase that is sitting in the box.
///
/// This is the assertion that stands between a *Cancel* and a deleted character: the phrase is
/// typed **correctly** first, so a transposed row would answer `"DELETE"` here and `_stricmp`
/// would match. It is asserted on the property the dialog carries, not by sending anything.
#[test]
fn cancel_answers_an_empty_string_even_with_the_phrase_in_the_box() {
    let e = env();
    let (mut ui, h) = raise(&e, DialogKind::ConfirmationTextInput);
    let box_ = type_into(&mut ui, h, ElementId(0x2C), "DELETE");
    assert_eq!(
        text_of(&mut ui, box_),
        "DELETE",
        "the phrase that *would* have deleted"
    );

    press(&mut ui, h, child::CONFIRM_TEXT_INPUT_CANCEL);

    let d = dialog_element(&ui, h).expect("still live");
    assert_eq!(d.answer, Some(ElementId(0x2F)), "Cancel");
    assert_eq!(d.answer_role, Some(AnswerRole::Cancel));
    assert_eq!(
        d.answer_text.as_deref(),
        Some(""),
        "cancel_dialog writes an empty 0x9C"
    );
    assert_eq!(
        d.answer_property(),
        Some((0x9C, PropertyValue::String(String::new())))
    );
    // And the box was not cleared: the client reads the box's text only on the accept arm.
    assert_eq!(text_of(&mut ui, box_), "DELETE");
}

/// The message dialog's listener — one button, `0x26`, and it writes no
/// property at all. `0x17` is not in its layout.
#[test]
fn a_message_dialog_answers_on_0x26_and_writes_no_property() {
    let e = env();
    let (mut ui, h) = raise(&e, DialogKind::Message);
    assert_eq!(
        ui.node(h).expect("live").element_id(),
        ElementId(0x24),
        "d[0x8E] = 3"
    );
    assert!(
        ui.get_child_recursive(h, child::BUTTON1).is_none(),
        "no 0x17 in a message dialog"
    );

    press(&mut ui, h, child::MESSAGE_BUTTON);
    let d = dialog_element(&ui, h).expect("live");
    assert_eq!(d.answer, Some(ElementId(0x26)));
    assert_eq!(d.answer_role, Some(AnswerRole::Accept));
    assert_eq!(d.answer_text, None, "a message dialog has no box");
    assert_eq!(d.answer_property(), None, "it only closes the dialog");
}

/// The confirmation dialog answers on `0x17` and `0x19` into `0x92`, and the answer is a bool:
/// "was the pressed id 0x17".
#[test]
fn a_confirmation_still_answers_on_0x17_and_0x19_into_property_0x92() {
    let e = env();
    for (id, role) in [
        (child::BUTTON1, AnswerRole::Accept),
        (child::BUTTON2, AnswerRole::Cancel),
    ] {
        let (mut ui, h) = raise(&e, DialogKind::Confirmation);
        assert_eq!(
            ui.node(h).expect("live").element_id(),
            ElementId(0x15),
            "d[0x8E] = 1"
        );
        press(&mut ui, h, id);
        let d = dialog_element(&ui, h).expect("live");
        assert_eq!(d.answer, Some(id));
        assert_eq!(d.answer_role, Some(role));
        assert_eq!(d.answer_text, None, "no box on a plain confirmation");
        assert_eq!(
            d.answer_property(),
            Some((0x92, PropertyValue::Bool(id == ElementId(0x17)))),
            "the answer is the bool `id == 0x17`, not an integer button id"
        );
    }
}

/// The wait dialog's listener just defers to the base:
/// it compares nothing, so **no** button press is an answer. A `Wait` is taken down by an event.
#[test]
fn a_wait_dialog_answers_nothing_at_all() {
    let e = env();
    let (mut ui, h) = raise(&e, DialogKind::Wait);
    assert_eq!(
        ui.node(h).expect("live").element_id(),
        ElementId(0x31),
        "d[0x8E] = 2"
    );
    for id in [
        child::BUTTON1,
        child::BUTTON2,
        child::MESSAGE_BUTTON,
        ElementId(0x2E),
    ] {
        // Nothing to press, and pressing the dialog itself is not an answer either.
        assert!(
            ui.get_child_recursive(h, id).is_none(),
            "a wait dialog has no {id:?}"
        );
    }
    ui.broadcast_element_message(h, msgid::BUTTON_CLICKED, 0, 0);
    let d = dialog_element(&ui, h).expect("live");
    assert_eq!(d.answer, None);
    assert_eq!(d.answer_role, None);
    assert_eq!(d.answer_property(), None);
}

/// Each subclass's `set_data` puts the caller's **own** caption properties on the subclass's
/// **own** buttons: `0x9A` onto `0x2E` and `0x9B` onto `0x2F` for a `ConfirmationTextInput`
/// and `0x90`/`0x91` onto `0x17`/`0x19` for a `Confirmation`.
///
#[test]
fn set_data_captions_each_subclasss_own_buttons() {
    let e = env();

    let (mut ui, h) = raise(&e, DialogKind::ConfirmationTextInput);
    let mut data = PropertyCollection::new();
    data.set(
        attr::DIALOG_TEXT_INPUT_ACCEPT_CAPTION,
        PropertyValue::String("Done".into()),
    );
    data.set(
        attr::DIALOG_TEXT_INPUT_CANCEL_CAPTION,
        PropertyValue::String("Cancel".into()),
    );
    // The properties the *wrong* table would have used. They must change nothing.
    data.set(attr::DIALOG_BUTTON1, PropertyValue::String("WRONG".into()));
    data.set(attr::DIALOG_BUTTON2, PropertyValue::String("WRONG".into()));
    assert!(set_dialog_data(&mut ui, h, &data), "the root is a dialog");

    let done = ui.get_child_recursive(h, ElementId(0x2E)).expect("0x2E");
    let cancel = ui.get_child_recursive(h, ElementId(0x2F)).expect("0x2F");
    assert_eq!(text_of(&mut ui, done), "Done");
    assert_eq!(text_of(&mut ui, cancel), "Cancel");

    let (mut ui, h) = raise(&e, DialogKind::Confirmation);
    let mut data = PropertyCollection::new();
    data.set(attr::DIALOG_BUTTON1, PropertyValue::String("Yes".into()));
    data.set(attr::DIALOG_BUTTON2, PropertyValue::String("No".into()));
    data.set(
        attr::DIALOG_TEXT_INPUT_ACCEPT_CAPTION,
        PropertyValue::String("WRONG".into()),
    );
    assert!(set_dialog_data(&mut ui, h, &data));
    let yes = ui.get_child_recursive(h, ElementId(0x17)).expect("0x17");
    let no = ui.get_child_recursive(h, ElementId(0x19)).expect("0x19");
    assert_eq!(text_of(&mut ui, yes), "Yes");
    assert_eq!(text_of(&mut ui, no), "No");

    // A `Wait` has no buttons and no caption properties: `set_data` is a no-op on it, not a panic.
    let (mut ui, h) = raise(&e, DialogKind::Wait);
    assert!(set_dialog_data(&mut ui, h, &data));
    assert_eq!(DialogKind::Wait.caption_properties(), (None, None));
}
