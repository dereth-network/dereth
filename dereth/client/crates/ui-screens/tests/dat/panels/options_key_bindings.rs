//! The shipped key-binding row template and six tab list boxes; a row per bindable action with
//! merged-map keys; capture rebinds and kills the old key; overwrite dialog binds only on OK; full
//! rows recycle; clear binds to do-nothing and survives save/reload; mouse-turning preset rebinds
//! two rows.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use crate::common::layout::Strings;
use std::rc::Rc;

use dereth_input::binding::{Capture, DO_NOTHING};
use dereth_input::spec::{activation, ControlCode, SubControlIndex};
use dereth_input::{ActionId, ControlChord, InputManager, InputMapId};
use dereth_primitives::DataId;
use dereth_ui::framework::Screen;
use dereth_ui::{ElemHandle, ElementId, ElementMessage, UiSystem};

use crate::common::*;
use dereth_ui_screens::options::keybinding::{
    attr, key_button_ids, ActionKeyMapRow, KeyBindingPage, RowDialog, RowEvent, ACTION_CLASSES,
    DIALOG_QUEUE, KEYBOARD_TAB_PAGES, TEMPLATE_ROW,
};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

// ---------------------------------------------------------------------------------------------
// The literals, taken from the shipped keymap and retail rather than from our own symbols
// ---------------------------------------------------------------------------------------------

/// The player movement map.
const MOVEMENT: InputMapId = InputMapId(4);
/// `MovementForward` — the shipped binding: control `0x00110000` (`DIK_W`), meta mode `0`,
/// activation `0x03`, input map `0x00000004`, action `0x00000029`, in the default key map.
const MOVE_FORWARD: ActionId = ActionId(0x29);

const DIK_W: u16 = 0x11;
const DIK_UP: u16 = 0xC8;
/// The one letter-or-function key no shipped binding uses — see `key_rebinding.rs`.
const DIK_F7: u16 = 0x41;
const DIK_A: u16 = 0x1E;

/// The Keyboard panel inside the shipped gameplay tree.
const KEYBOARD_UI: ElementId = ElementId(0x1000_0020);
/// The three key buttons the shipped row template `0x1000002F` declares.
const SHIPPED_KEY_BUTTONS: [u32; 3] = [0x1000_0030, 0x1000_0031, 0x1000_0032];
/// That template's own element id, and the layout it lives in.
const ROW_TEMPLATE: ElementId = ElementId(0x1000_002F);
const KEYBOARD_LAYOUT: DataId = DataId(0x2100_0009);

// ---------------------------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------------------------

fn env() -> UiSystem {
    let (mut ui, _flow, store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui.strings = Some(Rc::new(Strings(Rc::clone(&store))));
    ui
}

/// The shipped gameplay screen, and the Keyboard panel inside it.
fn screen() -> (UiSystem, ElemHandle) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let root = ui.root();
    let h = ui
        .get_child_recursive(root, KEYBOARD_UI)
        .expect("key-binding panel is in the tree");
    (ui, h)
}

/// The same key **as an event**, on its release — which is what
/// hands the row and what step 4 of the capture turns back into a `Click` binding.
fn released(offset: u16) -> ControlChord {
    ControlChord::new(
        ControlCode::new(0, SubControlIndex::None, offset),
        0,
        activation::UP,
    )
}

fn msg(source: ElemHandle, id: dereth_ui::MessageId, p1: u32) -> ElementMessage {
    ElementMessage {
        source_id: ElementId(0),
        source,
        id,
        p1,
        p2: 0,
        point: dereth_ui::msg::MessagePoint::default(),
        serial: 1,
    }
}

fn click(source: ElemHandle) -> ElementMessage {
    msg(source, dereth_ui::msg::element::id::BUTTON_CLICKED, 0)
}

fn right_click(source: ElemHandle) -> ElementMessage {
    msg(
        source,
        dereth_ui::msg::element::id::MOUSE_CLICK,
        dereth_ui::focus::action::SECONDARY_CLICK,
    )
}

/// The built page, with the shipped-keymap manager behind it.
fn page() -> (UiSystem, InputManager, KeyBindingPage) {
    let (mut ui, h) = screen();
    let m = manager();
    let mut p = KeyBindingPage::bind(&ui, h);
    p.init_options(&mut ui, &m);
    (ui, m, p)
}

// ---------------------------------------------------------------------------------------------
// The shipped data
// ---------------------------------------------------------------------------------------------

/// The shipped row template is three key buttons named by attribute 0x1000002b.
#[test]
fn the_shipped_row_template_is_three_key_buttons_named_by_attribute_0x1000002b() {
    let mut ui = env();
    let root = ui.root();
    let h = dereth_ui_screens::env::create_child_element_by_data_id(
        &mut ui,
        root,
        KEYBOARD_LAYOUT,
        ROW_TEMPLATE,
    )
    .expect("the classic_keyboard row template builds");
    assert_eq!(
        ui.node(h).map(|n| n.ty().0),
        Some(0x1000_0034),
        "template 1 of every key-binding list box is a key-action option row"
    );
    let ids: Vec<u32> = key_button_ids(&ui, h).into_iter().map(|e| e.0).collect();
    assert_eq!(
        ids, SHIPPED_KEY_BUTTONS,
        "attribute 0x1000002B is the key buttons"
    );
    assert_eq!(attr::KEY_BUTTONS, 0x1000_002B);
    assert_eq!(attr::KEY_BUTTON_MEMBER, 0x1000_002C);
    // …and the row has no clear button, which is why the refresh's boolean-attribute `0x0D` tail is
    // unobservable in this build. Stated rather than left silent.
    let row = ActionKeyMapRow::post_init(&ui, h).expect("post-init");
    assert_eq!(row.key_buttons.len(), 3);
    assert!(
        row.clear_button.is_none(),
        "the shipped template declares no 0x1000002A"
    );
    assert_eq!(attr::CLEAR_BUTTON, 0x1000_002A);
    // Every one of the three ids really is a child of the row, which is what
    // the descendant lookup in post-init establishes.
    for id in SHIPPED_KEY_BUTTONS {
        assert!(
            ui.get_child_recursive(h, ElementId(id)).is_some(),
            "{id:#010X}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------------------------

/// Behaviour: options.key-bindings.the-page-builds-one-row-for-every-bindable-action-once
/// **The row's first acceptance clause**: *"draws a row per bindable action with the keys
/// `InputShell::keys_for_action` reports"*.
///
/// The denominator is asserted under the stated testability rule, *"assert the denominator"*: the number of rows
/// is the number of `is_user_bindable` entries in the shipped `ActionMap`, counted here from the
/// map rather than written down, and the keys on each row are compared against
/// `find_keys_for_action` on the merged master map.
///
/// Falsified by: dropping the `is_user_bindable` filter; grouping by input map instead of action
/// class; initialising `current` from `defaults` written in the test.
#[test]
fn the_page_draws_a_row_per_bindable_action_with_the_keys_the_merged_map_reports() {
    let (ui, m, p) = page();
    assert_eq!(p.list_boxes.len(), 6, "six tab pages, six action classes");
    assert_eq!(
        p.list_boxes.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        ACTION_CLASSES.iter().map(|(c, _)| *c).collect::<Vec<_>>()
    );
    let bindable = m
        .action_map
        .entries()
        .filter(|(map, a, _)| m.action_map.is_user_bindable(*map, *a))
        .count();
    assert!(
        bindable > 0,
        "the shipped ActionMap has user-bindable entries"
    );
    // **The filter is unfalsifiable against this corpus, and that is a
    // measurement rather than a hole.** An entry is user-bindable when its action class and
    // action name are both non-zero, and in the shipped map every one of the 83 refused entries
    // fails **both** halves — so the "class 0 has no tab page" skip below already removes exactly
    // the same 83, and deleting the filter changes nothing here. Asserted so that a future
    // `ActionMap` in which the two halves disagree reddens this test instead of quietly changing
    // the page.
    let (mut class0, mut name0) = (0, 0);
    for (_, _, v) in m.action_map.entries() {
        if v.action_class == 0 && v.action_name != 0 {
            class0 += 1;
        }
        if v.action_class != 0 && v.action_name == 0 {
            name0 += 1;
        }
    }
    assert_eq!(
        (class0, name0),
        (0, 0),
        "the two halves of is_user_bindable agree on all {} entries in this map",
        m.action_map.entries().count()
    );
    // One row per row of the set both interfaces' pages list that this interface acts on: the
    // bindable entries but the quickslots 10 to 18, the quest detail panel and the five rows only
    // the classic interface answers, with Disable Most Weather Effects.
    assert_eq!(
        p.rows.len(),
        dereth_input::presentation::ROWS.len() - 5,
        "one row per listed (map, action)"
    );
    assert_eq!(
        bindable,
        p.rows.len() + 10 - 1 + 5,
        "of the {bindable} bindable entries"
    );
    assert_eq!(
        p.failures, 0,
        "every template insertion produced an element"
    );
    assert!(p.headers > 0, "one section header per (class, input map)");
    // Every row shows exactly what the merged map says, and at least one row shows something.
    let mut with_keys = 0;
    for r in &p.rows {
        let want = m.find_keys_for_action(r.action, r.input_map);
        assert_eq!(r.current, want, "row for {:?}/{:?}", r.input_map, r.action);
        assert_eq!(r.saved, want, "saved starts equal");
        assert_eq!(r.defaults, want, "defaults starts equal");
        assert_eq!(
            r.key_buttons.len(),
            3,
            "the shipped row has three key buttons"
        );
        if !want.is_empty() {
            with_keys += 1;
        }
    }
    assert!(
        with_keys > 0,
        "{with_keys} of {} rows carry a binding",
        p.rows.len()
    );
    // *Move Forward* is the worked example: two controls in map 4.
    let i = p
        .row_of(MOVEMENT, MOVE_FORWARD)
        .expect("Move Forward has a row");
    let shown: Vec<u16> = p.rows[i]
        .current
        .iter()
        .map(|q| q.control.offset())
        .collect();
    assert_eq!(
        shown,
        vec![DIK_W, DIK_UP],
        "the default key map binds W and UP to MovementForward"
    );
    // …and the row's caption is the action's own name out of the page's string table, resolved
    // rather than written here. A blank one would mean the string table did not answer.
    assert!(!p.rows[i].label.is_empty(), "the row is captioned");
    assert_eq!(p.rows[i].button_labels[0], "W");
    assert_eq!(p.rows[i].button_labels[1], "Up");
    assert_eq!(p.rows[i].button_labels[2], "", "the third button is empty");
    let _ = ui;
}

/// The six tab-page ids are the ones post-init looks a list box up inside, and each really has
/// one. Literals, because a transposed page id would silently give an action class the wrong list.
#[test]
fn the_six_tab_pages_each_carry_one_key_binding_list_box() {
    let (ui, h) = screen();
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
    for id in KEYBOARD_TAB_PAGES {
        let tab = ui
            .get_child_recursive(h, id)
            .unwrap_or_else(|| panic!("{:#010X}", id.0));
        let lb = ui
            .get_child_recursive(tab, ElementId(0x1000_0025))
            .unwrap_or_else(|| panic!("list box under {:#010X}", id.0));
        let w = dereth_ui_screens::panels::listbox::ListBoxWidget::bind(&ui, lb);
        assert_eq!(w.templates.len(), 2, "header and row templates");
        assert_eq!(w.templates[TEMPLATE_ROW], (KEYBOARD_LAYOUT, ROW_TEMPLATE));
    }
}

// ---------------------------------------------------------------------------------------------
// The capture flow
// ---------------------------------------------------------------------------------------------

/// Behaviour: options.key-bindings.a-key-pressed-over-a-row-rebinds-it-and-frees-the-old-key
/// **The second and third acceptance clauses, end to end**: clicking a key button enters capture,
/// the next key press arrives through `capture_key_hit` and lands via `set_binding`, and *the old
/// key goes dead*.
///
/// The last part is asserted on the **map walk**, not on the row's own list: a build that bound
/// everything to everything would pass a "the new key works" test.
///
/// Falsified by: not calling `initiate_binding` on message 1; passing the *binding*-shaped control
/// to `capture_key_hit` (it would answer `Ignored`); dropping the `set_binding` call from the
/// `Ready` arm.
#[test]
fn clicking_a_key_button_captures_the_next_key_and_the_old_one_goes_dead() {
    let (mut ui, mut m, mut p) = page();
    let i = p
        .row_of(MOVEMENT, MOVE_FORWARD)
        .expect("Move Forward has a row");
    // Before: W fires MovementForward and F7 fires nothing.
    assert!(m
        .find_keys_for_action(MOVE_FORWARD, MOVEMENT)
        .contains(&keyboard(DIK_W)));
    assert!(!m
        .find_keys_for_action(MOVE_FORWARD, MOVEMENT)
        .contains(&keyboard(DIK_F7)));
    // Click the first key button — the one showing W.
    let button = p.rows[i].key_buttons[0];
    let ev = p.on_element_message(&mut ui, &mut m, &click(button));
    assert_eq!(ev, Some((i, RowEvent::CaptureStarted { slot: 0 })));
    assert!(
        p.rows[i].capturing(),
        "initiate_binding raised the map-warn dialog"
    );
    assert!(
        ui.dialogs.is_dialog_open(DIALOG_QUEUE),
        "on queue 0x10000001"
    );
    // The key press. `Down` is not an answer; the release is.
    let d = "+";
    let down = ControlChord::new(
        ControlCode::new(0, SubControlIndex::None, DIK_F7),
        0,
        activation::DOWN,
    );
    assert_eq!(
        p.rows[i].key_hit(&mut ui, &mut m, down, d),
        Capture::Ignored
    );
    assert!(
        p.rows[i].capturing(),
        "the handler stays registered on a Down"
    );
    let v = p.rows[i].key_hit(&mut ui, &mut m, released(DIK_F7), d);
    assert!(
        matches!(v, Capture::Ready { .. }),
        "F7 is unbound, so nothing to confirm: {v:?}"
    );
    assert!(!p.rows[i].capturing(), "the map-warn dialog closed");
    // After: F7 fires MovementForward, W does not, and the row's *other* key is untouched.
    let keys = m.find_keys_for_action(MOVE_FORWARD, MOVEMENT);
    assert!(keys.contains(&keyboard(DIK_F7)), "{keys:?}");
    assert!(
        !keys.contains(&keyboard(DIK_W)),
        "the old key stopped firing: {keys:?}"
    );
    assert!(
        keys.contains(&keyboard(DIK_UP)),
        "the second binding survived: {keys:?}"
    );
    // W is bound to `DoNothing`, not deleted — the half a save-and-reload needs.
    assert_eq!(
        m.keymap
            .section(MOVEMENT)
            .expect("the movement map")
            .bindings()
            .iter()
            .find(|(qc, _)| qc.is_exactly_equal(&keyboard(DIK_W)))
            .map(|(_, a)| *a),
        Some(DO_NOTHING),
        "SetBinding binds the freed control to DoNothing"
    );
    // And the row redrew.
    assert_eq!(p.rows[i].button_labels[0], "F7");
    assert_eq!(p.rows[i].current.len(), 2);
    assert!(
        p.rows[i].changed(),
        "changed() sees the difference from saved"
    );
}

/// Behaviour: options.key-bindings.a-key-already-in-use-asks-first-and-one-that-cannot-be-taken-refuses
/// **The fourth acceptance clause**: a key already bound elsewhere raises the overwrite dialog and
/// only rebinds on OK.
///
/// Both answers are driven, in that order, because a build that always rebound would pass a
/// yes-only test and a build that never did would pass a no-only one.
///
/// Falsified by: dropping the `NeedsConfirmation` arm; calling `set_binding` before the answer;
/// treating a *No* as a yes in `close_dialog`.
#[test]
fn a_key_bound_elsewhere_raises_the_overwrite_dialog_and_binds_only_on_ok() {
    let d = "+";
    // `DIK_A` is bound in the movement map by the default key map; binding it to *Move Forward*
    // must therefore displace whatever holds it. The test reads that from the map rather than
    // assuming.
    let taken = {
        let m = manager();
        m.keymap
            .section(MOVEMENT)
            .expect("movement")
            .bindings()
            .iter()
            .find(|(qc, _)| qc.control.offset() == DIK_A)
            .map(|(_, a)| *a)
    };
    assert!(
        taken.is_some(),
        "DIK_A is bound in map 4 by the shipped keymap"
    );

    // --- No -------------------------------------------------------------------------------
    let (mut ui, mut m, mut p) = page();
    let i = p.row_of(MOVEMENT, MOVE_FORWARD).unwrap();
    let button = p.rows[i].key_buttons[0];
    p.on_element_message(&mut ui, &mut m, &click(button));
    let v = p.rows[i].key_hit(&mut ui, &mut m, released(DIK_A), d);
    assert!(
        matches!(v, Capture::NeedsConfirmation { .. }),
        "a conflicting, user-bindable action asks: {v:?}"
    );
    let ctx = p.rows[i]
        .dialog_context(RowDialog::Overwrite)
        .expect("the overwrite dialog is up");
    assert!(ui.dialogs.is_dialog_open(DIALOG_QUEUE));
    assert!(!m
        .find_keys_for_action(MOVE_FORWARD, MOVEMENT)
        .contains(&keyboard(DIK_A)));
    let bound = p.rows[i].close_dialog(&mut ui, &mut m, ctx, false, d);
    assert!(!bound, "No binds nothing");
    assert!(!m
        .find_keys_for_action(MOVE_FORWARD, MOVEMENT)
        .contains(&keyboard(DIK_A)));
    assert_eq!(
        m.find_keys_for_action(MOVE_FORWARD, MOVEMENT),
        vec![keyboard(DIK_W), keyboard(DIK_UP)]
    );
    assert!(
        p.rows[i].binding_being_changed.is_none(),
        "the pending control is cleared either way"
    );

    // --- Yes ------------------------------------------------------------------------------
    let (mut ui, mut m, mut p) = page();
    let i = p.row_of(MOVEMENT, MOVE_FORWARD).unwrap();
    let button = p.rows[i].key_buttons[0];
    p.on_element_message(&mut ui, &mut m, &click(button));
    p.rows[i].key_hit(&mut ui, &mut m, released(DIK_A), d);
    let ctx = p.rows[i].dialog_context(RowDialog::Overwrite).unwrap();
    let bound = p.rows[i].close_dialog(&mut ui, &mut m, ctx, true, d);
    assert!(bound, "OK binds");
    let keys = m.find_keys_for_action(MOVE_FORWARD, MOVEMENT);
    assert!(keys.contains(&keyboard(DIK_A)), "{keys:?}");
    assert!(
        !keys.contains(&keyboard(DIK_W)),
        "slot 0 was replaced: {keys:?}"
    );
    // The action that used to own A no longer does.
    let old = taken.expect("checked above");
    assert!(
        !m.find_keys_for_action(old, MOVEMENT)
            .iter()
            .any(|q| q.control.offset() == DIK_A),
        "the displaced action lost the key"
    );
}

/// **The fifth acceptance clause**: a full row recycles its oldest binding.
///
/// The shipped row has three key buttons, so the fourth key added to a row that already has three
/// must drop the head. This is the branch [`dereth_input::InputManager::set_binding`] deliberately
/// does **not** carry — it is the page's, because how many buttons a row has is layout.
///
/// See the module docs on `options::keybinding` for what is verified here and what is `[inferred]`:
/// retail visibly unbinds the head and rebinds it to `DoNothing`, but its removal from the
/// current list is not directly observed.
///
/// Falsified by: removing the `self.current.len() >= self.key_buttons.len()` branch; recycling the
/// tail instead of the head; leaving the head in `current`.
#[test]
fn a_full_row_recycles_its_oldest_binding() {
    let d = "+";
    let (mut ui, mut m, mut p) = page();
    let i = p.row_of(MOVEMENT, MOVE_FORWARD).unwrap();
    assert_eq!(p.rows[i].key_buttons.len(), 3);
    assert_eq!(p.rows[i].current.len(), 2, "W and UP");
    // Fill the third slot: no slot selected, so this is an *addition*.
    p.rows[i].slot_being_changed = None;
    assert!(p.rows[i].set_binding(&mut m, keyboard(DIK_F7), None));
    p.rows[i].refresh(&mut ui, &m, d);
    assert_eq!(p.rows[i].current.len(), 3, "the row is now full");
    assert_eq!(
        p.rows[i].button_labels,
        vec!["W".to_string(), "Up".to_string(), "F7".to_string()]
    );
    // A fourth. The head — W — is recycled.
    let f8 = ControlCode::new(0, SubControlIndex::None, 0x42);
    let fourth = ControlChord::new(f8, 0, activation::CLICK);
    assert!(p.rows[i].set_binding(&mut m, fourth, None));
    p.rows[i].refresh(&mut ui, &m, d);
    assert_eq!(p.rows[i].current.len(), 3, "still three, not four");
    assert_eq!(
        p.rows[i].current[0],
        keyboard(DIK_UP),
        "the head was dropped"
    );
    assert_eq!(p.rows[i].current[2], fourth);
    let keys = m.find_keys_for_action(MOVE_FORWARD, MOVEMENT);
    assert!(
        !keys.contains(&keyboard(DIK_W)),
        "the recycled key stopped firing: {keys:?}"
    );
    assert_eq!(
        m.keymap
            .section(MOVEMENT)
            .unwrap()
            .bindings()
            .iter()
            .find(|(qc, _)| qc.is_exactly_equal(&keyboard(DIK_W)))
            .map(|(_, a)| *a),
        Some(DO_NOTHING),
        "and it was recycled to DoNothing, not deleted"
    );
}

/// Clearing a binding binds it to do nothing rather than deleting it.
#[test]
fn clearing_a_binding_binds_it_to_do_nothing_rather_than_deleting_it() {
    let d = "+";
    let (mut ui, mut m, mut p) = page();
    let i = p.row_of(MOVEMENT, MOVE_FORWARD).unwrap();
    let button = p.rows[i].key_buttons[0];

    // A *primary* click is a capture, not an erase — the `dwParam1 == 8` guard.
    let ev = p.on_element_message(&mut ui, &mut m, &click(button));
    assert_eq!(ev, Some((i, RowEvent::CaptureStarted { slot: 0 })));
    p.rows[i].close_map_warn_dialog(&mut ui);

    let ev = p.on_element_message(&mut ui, &mut m, &right_click(button));
    assert_eq!(ev, Some((i, RowEvent::Erased { slot: 0 })));
    assert_eq!(p.rows[i].current, vec![keyboard(DIK_UP)], "W left current");
    let w = m
        .keymap
        .section(MOVEMENT)
        .unwrap()
        .bindings()
        .iter()
        .find(|(qc, _)| qc.is_exactly_equal(&keyboard(DIK_W)))
        .map(|(_, a)| *a);
    assert_eq!(
        w,
        Some(DO_NOTHING),
        "erase_binding writes DoNothing, it does not delete"
    );
    assert_eq!(DO_NOTHING.0, 1);
    assert!(!m
        .find_keys_for_action(MOVE_FORWARD, MOVEMENT)
        .contains(&keyboard(DIK_W)));
    assert_eq!(
        p.rows[i].button_labels[0], "Up",
        "Refresh shifted the row up"
    );

    // Right-clicking past the end of `current` does nothing: the in-range guard.
    let third = p.rows[i].key_buttons[2];
    assert_eq!(
        p.on_element_message(&mut ui, &mut m, &right_click(third)),
        None
    );

    let before = p.rows[i].current.clone();
    let primary = msg(
        p.rows[i].key_buttons[0],
        dereth_ui::msg::element::id::MOUSE_CLICK,
        dereth_ui::focus::action::PRIMARY_CLICK,
    );
    assert_eq!(p.on_element_message(&mut ui, &mut m, &primary), None);
    assert_eq!(
        p.rows[i].current, before,
        "a left click on a key button is not an erase"
    );

    // `clear_all_bindings` frees the rest the same way.
    let n = p.rows[i].clear_all_bindings(&mut ui, &mut m, d);
    assert_eq!(n, 1);
    assert!(p.rows[i].current.is_empty());
    assert!(m.find_keys_for_action(MOVE_FORWARD, MOVEMENT).is_empty());
    for k in [DIK_W, DIK_UP] {
        assert_eq!(
            m.keymap
                .section(MOVEMENT)
                .unwrap()
                .bindings()
                .iter()
                .find(|(qc, _)| qc.is_exactly_equal(&keyboard(k)))
                .map(|(_, a)| *a),
            Some(DO_NOTHING),
            "{k:#04X}"
        );
    }
    // …and puts them back.
    p.rows[i].restore_default_value(&mut ui, &mut m, d);
    assert_eq!(
        m.find_keys_for_action(MOVE_FORWARD, MOVEMENT),
        vec![keyboard(DIK_W), keyboard(DIK_UP)]
    );
    assert!(!p.rows[i].changed());
}

/// The key-binding page's element-message handler gates **both** of its arms on
/// whether dialog queue `0x10000001` is open.
///
/// Falsified by: dropping either guard; using `DEFAULT_QUEUE` (2) instead of `0x10000001`, which
/// would let an unrelated confirmation elsewhere in the UI block a rebind and vice versa.
#[test]
fn a_dialog_on_the_key_binding_queue_refuses_a_new_capture() {
    let (mut ui, mut m, mut p) = page();
    let i = p.row_of(MOVEMENT, MOVE_FORWARD).unwrap();
    let button = p.rows[i].key_buttons[0];
    // Raise something on the key-binding queue, from another row.
    let j = p
        .rows
        .iter()
        .position(|r| r.action != MOVE_FORWARD)
        .expect("another row");
    p.rows[j].initiate_binding(&mut ui, 0);
    assert!(ui.dialogs.is_dialog_open(DIALOG_QUEUE));
    assert_eq!(
        p.rows[i].on_element_message(&mut ui, &mut m, &click(button), "+"),
        Some(RowEvent::Refused)
    );
    assert_eq!(
        p.rows[i].on_element_message(&mut ui, &mut m, &right_click(button), "+"),
        Some(RowEvent::Refused)
    );
    assert_eq!(p.rows[i].current.len(), 2, "nothing was erased");
    // A dialog on the *default* queue does not block, which is what the distinct queue buys.
    p.rows[j].close_map_warn_dialog(&mut ui);
    assert!(!ui.dialogs.is_dialog_open(DIALOG_QUEUE));
    let mut other = dereth_ui::PropertyCollection::new();
    other.set(
        dereth_ui::props::attr::DIALOG_KIND,
        dereth_assets::ui::PropertyValue::Integer(
            dereth_ui::dialog::DialogKind::Confirmation.property(),
        ),
    );
    ui.dialogs
        .make_dialog(other, ui.now.0)
        .expect("a dialog on queue 2");
    assert!(ui
        .dialogs
        .is_dialog_open(dereth_ui::dialog::factory::DEFAULT_QUEUE));
    assert!(!ui.dialogs.is_dialog_open(DIALOG_QUEUE));
    assert_eq!(
        p.rows[i].on_element_message(&mut ui, &mut m, &click(button), "+"),
        Some(RowEvent::CaptureStarted { slot: 0 })
    );
}

/// The refresh-action-key-mapping notice is what makes one row's rebind visible on another.
///
/// Setting a binding displaces conflicting controls in every map that can be registered at
/// the same time; without the notice the other row would go on showing a key that no longer fires.
///
/// Falsified by: changing the notice id (it is a literal here); making `refresh_mappings` re-read
/// its own `current` instead of the map.
#[test]
fn the_refresh_notice_re_reads_every_row_from_the_map() {
    let (mut ui, mut m, mut p) = page();
    let i = p.row_of(MOVEMENT, MOVE_FORWARD).unwrap();
    // Take W away behind the row's back, the way another row's `SetBinding` would.
    m.unbind_by_key(&keyboard(DIK_W), MOVEMENT);
    m.bind_action(keyboard(DIK_W), DO_NOTHING, MOVEMENT);
    assert_eq!(
        p.rows[i].current.len(),
        2,
        "the row still shows the stale pair"
    );
    p.on_refresh_action_key_mapping(&mut ui, &m);
    assert_eq!(
        p.rows[i].current,
        vec![keyboard(DIK_UP)],
        "the row re-read the map"
    );
    assert_eq!(p.rows[i].button_labels[0], "Up");
    assert_eq!(p.rows[i].button_labels[1], "");
}

/// The mouse-turning defaults preset — the producer
/// `options::config::MOUSE_TURNING_KEY_MESSAGES` did not have.
///
/// Only two rows in the whole page do anything: actions `0x33` and `0x34` in input map 5. Every
/// other row is a no-op, which is asserted so that a version that rebound *every* row on global
/// message `0x0C` would fail.
#[test]
fn the_mouse_turning_preset_rebinds_exactly_two_rows() {
    let (mut ui, mut m, mut p) = page();
    let camera = InputMapId(5);
    let mut touched = 0;
    let mut lines = Vec::new();
    for k in 0..p.rows.len() {
        let out = p.rows[k].set_mouse_turning_defaults(&mut ui, &mut m, "+");
        if !out.is_empty() {
            touched += 1;
            lines.extend(out);
        }
    }
    assert_eq!(
        touched, 2,
        "actions 0x33 and 0x34 of input map 5, and nothing else"
    );
    assert_eq!(
        lines,
        dereth_ui_screens::options::config::MOUSE_TURNING_KEY_MESSAGES.to_vec(),
        "the two chat lines, in order"
    );
    // The wheel really is bound now. `0x00080101` / `0x00080201` are the two literals
    // `set_mouse_turning_defaults` pushes.
    for (action, cs) in [
        (ActionId(0x33), 0x0008_0101u32),
        (ActionId(0x34), 0x0008_0201),
    ] {
        let keys = m.find_keys_for_action(action, camera);
        assert!(
            keys.iter().any(|q| q.control.0 == cs),
            "action {:#X} should carry {cs:#010X}: {keys:?}",
            action.0
        );
    }
}

/// **`KeyBindingPage` had no production caller.** Every test above builds it with
/// `KeyBindingPage::bind` and drives it by hand, which is the stated testability rule's *"the worst case: a
/// test fixture standing in for the missing producer"* — the page was complete, correct and
/// unreachable from a running client.
///
/// This is the producer: `GamePlayScreen::post_init` binds it, as the keyboard page's post-init
/// does, and `key_bindings_init_options` builds the rows. The
/// split exists because every row reads the merged master keymap, which lives in the host's
/// `InputManager` and not in the UI tree.
///
/// Falsified by: removing either call from `post_init`; binding by the wrong element id.
#[test]
fn the_gameplay_screen_builds_the_key_binding_page() {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    // Bound by `post_init`, before anything else runs.
    assert_eq!(
        s.key_bindings
            .page
            .and_then(|h| ui.node(h))
            .map(|n| n.element_id().0),
        Some(0x1000_0020),
        "key-binding panel, by literal element id"
    );
    assert_eq!(
        s.key_bindings.list_boxes.len(),
        6,
        "six tab pages, six action classes"
    );
    assert_eq!(
        s.key_bindings.rows.len(),
        0,
        "option initialization has not run yet — it needs the map"
    );

    let m = manager();
    let n = s.key_bindings_init_options(&mut ui, &m);
    let bindable = m
        .action_map
        .entries()
        .filter(|(map, a, _)| m.action_map.is_user_bindable(*map, *a))
        .count();
    assert_eq!(
        n,
        dereth_input::presentation::ROWS.len() - 5,
        "one row per listed (map, action), of the {bindable} bindable"
    );
    assert_eq!(s.key_bindings.failures, 0);
    assert!(
        s.key_bindings.headers > 0,
        "{} section headers",
        s.key_bindings.headers
    );
    let with_keys = s
        .key_bindings
        .rows
        .iter()
        .filter(|r| !r.current.is_empty())
        .count();
    assert!(
        with_keys > 0,
        "{with_keys} of {n} rows carry a binding from the merged map"
    );
    // The captions come out of, resolved through the same
    // `StringResolver` the client installs.
    let captioned = s
        .key_bindings
        .rows
        .iter()
        .filter(|r| !r.label.is_empty())
        .count();
    assert_eq!(
        captioned, n,
        "rows captioned from the action map's descriptions, of {n}"
    );
    let i = s
        .key_bindings
        .row_of(MOVEMENT, MOVE_FORWARD)
        .expect("Move Forward has a row");
    assert_eq!(s.key_bindings.rows[i].label, "Move Forward");
}

/// **The capture flow, driven through the screen** rather than through the page object: a click on
/// a key button is delivered to `GamePlayScreen::on_element_message` the way a real click is, the
/// captured key arrives through `key_bindings_key_hit`, and `drive_key_bindings` applies both.
///
/// The screen queues rather than dispatches, for `dereth_ui::Delivery`'s own reason — a `Screen`
/// handler is given `&mut UiSystem` and nothing else, so a listener needing a system this crate
/// does not own cannot run inside the broadcast. It also means every write a row makes happens
/// **after** the broadcast has unwound, so nothing is read back out of the arena from inside its
/// own handler, as required by the stated testability rule.
///
/// Falsified by: not queuing the message in `on_element_message`; dropping the key hit when no row
/// is capturing (the guard is asserted in both directions); applying the key hit before the click.
#[test]
fn a_click_on_a_key_button_reaches_the_page_through_the_screen_and_rebinds() {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let mut m = manager();
    s.key_bindings_init_options(&mut ui, &m);
    let i = s
        .key_bindings
        .row_of(MOVEMENT, MOVE_FORWARD)
        .expect("Move Forward has a row");
    let button = s.key_bindings.rows[i].key_buttons[0];

    // Nothing is capturing, so a stray key press is refused rather than queued — the client's
    // *"is a key-hit handler registered"*.
    assert!(
        !s.key_bindings_key_hit(released(DIK_F7)),
        "no row is capturing yet"
    );

    // The click, through the screen's own listener.
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &click(button),
    );
    assert!(
        !s.key_bindings.rows[i].capturing(),
        "queued, not dispatched, inside the broadcast"
    );
    let (events, verdicts) = s.drive_key_bindings(&mut ui, &mut m);
    assert_eq!(events, vec![(i, RowEvent::CaptureStarted { slot: 0 })]);
    assert!(verdicts.is_empty());
    assert!(
        s.key_bindings.rows[i].capturing(),
        "the map-warn dialog is up"
    );
    assert!(ui.dialogs.is_dialog_open(DIALOG_QUEUE));

    // The key press, through the screen. `Down` is not an answer; the release is.
    assert!(
        s.key_bindings_key_hit(released(DIK_F7)),
        "a row is capturing, so it is taken"
    );
    let (events, verdicts) = s.drive_key_bindings(&mut ui, &mut m);
    assert!(events.is_empty());
    assert_eq!(verdicts.len(), 1);
    assert!(
        matches!(verdicts[0], Capture::Ready { .. }),
        "{:?}",
        verdicts[0]
    );
    assert!(!s.key_bindings.rows[i].capturing());

    // The world changed: F7 fires the action, W does not, and W is `DoNothing` rather than gone.
    let keys = m.find_keys_for_action(MOVE_FORWARD, MOVEMENT);
    assert!(keys.contains(&keyboard(DIK_F7)), "{keys:?}");
    assert!(!keys.contains(&keyboard(DIK_W)), "{keys:?}");
    assert!(
        keys.contains(&keyboard(DIK_UP)),
        "the row's other binding survived: {keys:?}"
    );
    assert_eq!(
        m.keymap
            .section(MOVEMENT)
            .expect("the movement map")
            .bindings()
            .iter()
            .find(|(qc, _)| qc.is_exactly_equal(&keyboard(DIK_W)))
            .map(|(_, a)| *a),
        Some(DO_NOTHING),
        "The freed key is bound to DoNothing"
    );
    assert_eq!(
        s.key_bindings.rows[i].button_labels[0], "F7",
        "and the row redrew"
    );
}

/// *"Clear" must clear to `DoNothing`.* Right-clicking a key button erases the binding;
/// it goes through `unbind_by_key` + `bind_action(control, DoNothing)` rather than
/// deleting the mapping, because the key-map merge *adds what is absent* and
/// takes the user file **before** the default key map — a deleted binding comes
/// back on the next run and both keys then fire.
///
/// Driven through the screen, and asserted **after a save-and-reload**, which is the only place
/// the difference between "cleared" and "deleted" is observable at all.
///
/// Falsified by: removing the `bind_action(qc, DoNothing)` from `erase_binding`; asserting only on
/// the row's `current`, which is empty either way.
#[test]
fn clearing_a_key_through_the_screen_survives_a_save_and_reload_as_do_nothing() {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let mut m = manager();
    s.key_bindings_init_options(&mut ui, &m);
    let i = s
        .key_bindings
        .row_of(MOVEMENT, MOVE_FORWARD)
        .expect("Move Forward has a row");
    let button = s.key_bindings.rows[i].key_buttons[0];
    assert!(m
        .find_keys_for_action(MOVE_FORWARD, MOVEMENT)
        .contains(&keyboard(DIK_W)));

    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &right_click(button),
    );
    let (events, _) = s.drive_key_bindings(&mut ui, &mut m);
    assert_eq!(events, vec![(i, RowEvent::Erased { slot: 0 })]);
    assert!(!m
        .find_keys_for_action(MOVE_FORWARD, MOVEMENT)
        .contains(&keyboard(DIK_W)));

    // The round trip. `save_keymap` writes the **full merged map**; a reload merges the user file
    // first and the default key map after it, so anything the user file omits comes back.
    let text = m.keymap.to_keymap_text();
    let reloaded = dereth_input::keymap::MasterInputMap::from_keymap_text(&text)
        .expect("the written keymap parses");
    assert_eq!(
        reloaded
            .section(MOVEMENT)
            .expect("the movement map survived the round trip")
            .bindings()
            .iter()
            .find(|(qc, _)| qc.is_exactly_equal(&keyboard(DIK_W)))
            .map(|(_, a)| *a),
        Some(DO_NOTHING),
        "W is written as DoNothing, so the shipped default cannot merge back over it"
    );
    assert!(
        !reloaded
            .section(MOVEMENT)
            .expect("movement")
            .bindings()
            .iter()
            .any(|(qc, a)| qc.is_exactly_equal(&keyboard(DIK_W)) && *a == MOVE_FORWARD),
        "and W does not still fire Move Forward"
    );
}

/// Each key binding tab shows the bindings its label names.
#[test]
fn each_key_binding_tab_shows_the_bindings_its_label_names() {
    let (mut ui, m, p) = page();
    let h = ui
        .get_child_recursive(ui.root(), KEYBOARD_UI)
        .expect("the panel");
    let class_of_label = |label: &str| match label {
        "Movement" => 1,
        "Camera" => 2,
        "UI" => 3,
        "Combat" => 4,
        "Emotes" => 5,
        "CharacterSettings" => 7,
        other => panic!("a tab label this test does not know: {other:?}"),
    };
    let mut labels = Vec::new();
    for page_id in KEYBOARD_TAB_PAGES {
        let page = ui.get_child_recursive(h, page_id).expect("the tab page");
        let parent = ui.parent(page).expect("the tab control");
        let siblings = ui.children(parent);
        let at = siblings
            .iter()
            .position(|c| *c == page)
            .expect("the page is its parent's child");
        let tab = siblings[at.checked_sub(1).expect("a tab precedes its page")];
        let label = ui
            .text_element_mut(tab)
            .map(|t| t.glyphs.inq_text(false))
            .unwrap_or_default();
        let want = class_of_label(&label);
        let mut rows = 0;
        for r in &p.rows {
            let Some(e) = r.element else { continue };
            let mut up = ui.parent(e);
            while let Some(a) = up {
                if a == page {
                    break;
                }
                up = ui.parent(a);
            }
            if up.is_none() {
                continue;
            }
            rows += 1;
            assert_eq!(
                dereth_input::presentation::retail_class(r.input_map, r.action)
                    .unwrap_or_else(|| m.action_map.action_class(r.input_map, r.action)),
                want,
                "the {label:?} tab shows {:?}/{:?}, which is another tab's binding",
                r.input_map,
                r.action
            );
        }
        assert!(rows > 0, "the {label:?} tab shows some bindings");
        labels.push(label);
    }
    assert_eq!(
        labels,
        [
            "Movement",
            "Camera",
            "Combat",
            "UI",
            "CharacterSettings",
            "Emotes"
        ]
    );
}

mod defaults {
    //! The key-binding page binds its four buttons; restore defaults is unconditional and cancel is
    //! not; a real press on restore defaults puts the shipped key back; cancel reverts and OK snapshots
    //! and queues SaveKeyMap.
    //! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

    use crate::common::layout::RegistrationOrder;
    use crate::common::layout::Strings;
    use std::rc::Rc;

    use dereth_input::{ActionId, InputManager, InputMapId};
    use dereth_ui::framework::Screen;
    use dereth_ui::msg::Delivery;
    use dereth_ui::{ElemHandle, ElementId, UiSystem};

    use crate::common::*;
    use dereth_ui_screens::options::keybinding::{page_attr, PageEvent};
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::UiRequest;

    // ---------------------------------------------------------------------------------------------
    // Literals, not symbols, as required by the stated testability rule.
    // ---------------------------------------------------------------------------------------------

    /// The keyboard panel in the shipped `classic_gameplay` tree.
    const KEYBOARD_UI: ElementId = ElementId(0x1000_0020);

    /// The four button attributes the page's post-init stores, and the element ids they resolve to
    /// in the shipped tree (measured off the built tree).
    const OK_ATTR: u32 = 0x1000_0019;
    const CANCEL_ATTR: u32 = 0x1000_001A;
    const RESET_ATTR: u32 = 0x1000_001B;
    const REVERT_ATTR: u32 = 0x1000_001C;
    const OK_ELEMENT: u32 = 0x1000_002C;
    const CANCEL_ELEMENT: u32 = 0x1000_002D;
    const RESET_ELEMENT: u32 = 0x1000_002A;
    const REVERT_ELEMENT: u32 = 0x1000_002B;

    /// The player system's movement map and `MovementForward`, as the shipped default key map binds
    /// `DIK_W` — the same pair `options_key_bindings.rs` drives.
    const MOVEMENT: InputMapId = InputMapId(4);
    const MOVE_FORWARD: ActionId = ActionId(0x29);
    const DIK_W: u16 = 0x11;
    /// The one letter-or-function key no shipped binding uses (`o196_rebinding.rs`).
    const DIK_F7: u16 = 0x41;

    // ---------------------------------------------------------------------------------------------
    // Environment — the same construction `options_key_bindings.rs` uses
    // ---------------------------------------------------------------------------------------------

    fn env() -> UiSystem {
        let (mut ui, _flow, store) =
            crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
        ui.strings = Some(Rc::new(Strings(Rc::clone(&store))));
        ui
    }

    /// The shipped gameplay screen with the key-binding rows built, and its keyboard-panel handle.
    fn page() -> (UiSystem, InputManager, GamePlayScreen, ElemHandle) {
        let mut ui = env();
        let mut s = GamePlayScreen::default();
        s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .expect("the gameplay screen builds");
        let root = ui.root();
        let h = ui
            .get_child_recursive(root, KEYBOARD_UI)
            .expect("the keyboard-settings panel is in the tree");
        let m = manager();
        let rows = s.key_bindings_init_options(&mut ui, &m);
        assert!(
            rows > 0,
            "init_options built no rows; every assertion below would be vacuous"
        );
        let _ = ui.drain_outbox();
        ui.requests.clear();
        (ui, m, s, h)
    }

    /// Make the element visible up the whole ancestry, so `hit_test_screen` can reach the
    /// button. The Key Bindings page is a tab page of the options window and is not shown by default.
    fn reveal(ui: &mut UiSystem, mut h: ElemHandle) {
        loop {
            ui.set_visible(h, true);
            match ui.parent(h) {
                Some(p) => h = p,
                None => break,
            }
        }
        ui.drain_outbox();
    }

    /// **A real pointer press**, not a synthesised element message: hit-test the button's own centre,
    /// then `mouse_down`/`mouse_up` with `PRIMARY_CLICK` (first parameter `== 7`, which is exactly what
    /// the page's button-click arm requires). Every message the press raises is then delivered to the
    /// screen, and the frame drain is run.
    ///
    /// Returns what the page's own handler made of it.
    fn press(
        ui: &mut UiSystem,
        s: &mut GamePlayScreen,
        m: &mut InputManager,
        h: ElemHandle,
    ) -> Vec<PageEvent> {
        reveal(ui, h);
        let b = ui.screen_box(h);
        let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        assert_eq!(
            ui.hit_test_screen(cx, cy),
            Some(h),
            "the press must land on the button itself, not on something drawn over it"
        );
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
        for _ in 0..8 {
            let batch = ui.drain_outbox();
            if batch.is_empty() {
                break;
            }
            for d in batch {
                if let Delivery::Element { msg, .. } = d {
                    s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
                }
            }
        }
        let _ = s.drive_key_bindings(ui, m);
        s.take_key_binding_page_events()
    }

    /// The row for `MovementForward` in the movement map, and its index.
    fn forward(s: &GamePlayScreen) -> usize {
        s.key_bindings
            .row_of(MOVEMENT, MOVE_FORWARD)
            .expect("MovementForward has a row")
    }

    // ---------------------------------------------------------------------------------------------
    // 1. The bindings the post-init makes
    // ---------------------------------------------------------------------------------------------

    /// The page binds the four buttons postinit stores.
    #[test]
    fn the_page_binds_the_four_buttons_postinit_stores() {
        let (ui, _m, s, h) = page();

        // The attribute numbers are the ones the page's post-init pushes, written as literals.
        assert_eq!(page_attr::OK_BUTTON, OK_ATTR);
        assert_eq!(page_attr::CANCEL_BUTTON, CANCEL_ATTR);
        assert_eq!(page_attr::RESET_TO_DEFAULTS_BUTTON, RESET_ATTR);
        assert_eq!(page_attr::REVERT_TO_SAVED_BUTTON, REVERT_ATTR);
        assert_eq!(page_attr::KEYMAP_FILENAME_LABEL, 0x1000_001D);

        // …and the shipped layout answers each of them with a distinct button element.
        for (attr, want) in [
            (OK_ATTR, OK_ELEMENT),
            (CANCEL_ATTR, CANCEL_ELEMENT),
            (RESET_ATTR, RESET_ELEMENT),
            (REVERT_ATTR, REVERT_ELEMENT),
        ] {
            let e = ui
                .node(h)
                .and_then(|n| n.merged_properties().get_enum(attr));
            assert_eq!(
                e,
                Some(want),
                "attribute {attr:#010X} on the shipped keyboard-settings panel"
            );
        }

        let got = [
            s.key_bindings.ok_button,
            s.key_bindings.cancel_button,
            s.key_bindings.reset_defaults_button,
            s.key_bindings.revert_to_saved_button,
        ];
        for (i, g) in got.iter().enumerate() {
            let g = g.unwrap_or_else(|| panic!("button {i} is unbound"));
            assert_eq!(
                ui.node(g).expect("live").ty().0,
                1,
                "every one of the four is a button element"
            );
        }
        let mut ids: Vec<u32> = got
            .iter()
            .map(|g| ui.node(g.expect("bound")).expect("live").element_id().0)
            .collect();
        assert_eq!(
            ids,
            vec![OK_ELEMENT, CANCEL_ELEMENT, RESET_ELEMENT, REVERT_ELEMENT]
        );
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids.len(),
            4,
            "four distinct elements, not one found four times"
        );

        // The two keymap-file buttons were already bound and must stay so.
        assert!(s.key_bindings.load_button.is_some());
        assert!(s.key_bindings.save_button.is_some());
    }

    // ---------------------------------------------------------------------------------------------
    // 2. The two walks are not the same walk
    // ---------------------------------------------------------------------------------------------

    /// The page's *Restore Defaults* walk restores the default on **every** row; the *Revert to
    /// Saved* walk asks each row whether it changed first and skips the rows that answer false. Reading them as
    /// the same walk is the easy mistake and it is measurable here: on an untouched page, Defaults
    /// writes every row and Cancel writes none.
    #[test]
    fn restore_defaults_is_unconditional_and_cancel_is_not() {
        let (mut ui, mut m, mut s, _h) = page();
        let n = s.key_bindings.rows.len();

        assert!(
            !s.key_bindings.changed(),
            "an untouched page reports no changes"
        );
        assert_eq!(
            s.key_bindings.restore_saved_values(&mut ui, &mut m),
            0,
            "nothing to revert"
        );
        assert_eq!(
            s.key_bindings.restore_default_values(&mut ui, &mut m),
            n,
            "…but every row is defaulted"
        );
        assert_eq!(
            s.key_bindings.save_current_values(),
            n,
            "…and every row is snapshotted"
        );

        // Move exactly one row, the way a capture does.
        let i = forward(&s);
        s.key_bindings.rows[i].current = vec![keyboard(DIK_F7)];
        assert!(s.key_bindings.rows[i].changed());
        assert!(
            s.key_bindings.changed(),
            "the page reports a change when any row changed"
        );
        assert_eq!(
            s.key_bindings.restore_saved_values(&mut ui, &mut m),
            1,
            "one row moved, so one row is written back"
        );
        assert!(!s.key_bindings.changed());
    }

    // ---------------------------------------------------------------------------------------------
    // 3. A real press, through hit-testing
    // ---------------------------------------------------------------------------------------------

    /// Behaviour: options.key-bindings.restoring-the-defaults-gives-back-the-shipped-keys-and-not-the-saved-ones
    /// A real press on restore defaults puts the shipped key back.
    #[test]
    fn a_real_press_on_restore_defaults_puts_the_shipped_key_back() {
        let (mut ui, mut m, mut s, _h) = page();
        let i = forward(&s);
        let defaults = s.key_bindings.rows[i].defaults.clone();
        assert!(
            defaults.iter().any(|q| q.control.offset() == DIK_W),
            "the shipped default for MovementForward is W: {defaults:?}"
        );

        // Rebind, through the same call the capture makes.
        m.unbind_all_by_action(MOVE_FORWARD, MOVEMENT);
        m.bind_action(keyboard(DIK_F7), MOVE_FORWARD, MOVEMENT);
        s.key_bindings.rows[i].current = vec![keyboard(DIK_F7)];
        let now = m.find_keys_for_action(MOVE_FORWARD, MOVEMENT);
        assert!(
            now.iter().all(|q| q.control.offset() != DIK_W),
            "W is gone: {now:?}"
        );

        let h = s.key_bindings.reset_defaults_button.expect("bound");
        let events = press(&mut ui, &mut s, &mut m, h);
        let n = s.key_bindings.rows.len();
        assert_eq!(
            events,
            vec![PageEvent::RestoredDefaults(n)],
            "the press must reach the defaults walk"
        );

        let back = m.find_keys_for_action(MOVE_FORWARD, MOVEMENT);
        assert!(
            back.iter().any(|q| q.control.offset() == DIK_W),
            "W must be bound to MovementForward again in the merged map: {back:?}"
        );
        assert_eq!(s.key_bindings.rows[i].current, defaults);
    }

    /// A real press on cancel reverts and on ok snapshots and writes the keymap.
    #[test]
    fn a_real_press_on_cancel_reverts_and_on_ok_snapshots_and_writes_the_keymap() {
        let (mut ui, mut m, mut s, _h) = page();
        let i = forward(&s);
        let saved = s.key_bindings.rows[i].saved.clone();

        // --- Cancel, with one row moved -------------------------------------------------------
        m.unbind_all_by_action(MOVE_FORWARD, MOVEMENT);
        m.bind_action(keyboard(DIK_F7), MOVE_FORWARD, MOVEMENT);
        s.key_bindings.rows[i].current = vec![keyboard(DIK_F7)];
        ui.requests.clear();

        let cancel = s.key_bindings.cancel_button.expect("bound");
        let events = press(&mut ui, &mut s, &mut m, cancel);
        assert_eq!(
            events,
            vec![PageEvent::RestoredSaved(1)],
            "Cancel is the saved-values walk, one row"
        );
        assert_eq!(s.key_bindings.rows[i].current, saved);
        assert!(
            m.find_keys_for_action(MOVE_FORWARD, MOVEMENT)
                .iter()
                .any(|q| q.control.offset() == DIK_W),
            "and the merged map went back with it"
        );
        assert!(
            ui.requests
                .take()
                .iter()
                .all(|r| *r != UiRequest::SaveKeyMap),
            "Cancel never writes the keymap file"
        );

        // --- OK on an unchanged page: snapshot, no file write ----------------------------------
        assert!(!s.key_bindings.changed());
        ui.requests.clear();
        let ok = s.key_bindings.ok_button.expect("bound");
        let n = s.key_bindings.rows.len();
        let events = press(&mut ui, &mut s, &mut m, ok);
        assert_eq!(
            events,
            vec![PageEvent::Applied {
                rows: n,
                saved: false
            }]
        );
        assert!(
            ui.requests
                .take()
                .iter()
                .all(|r| *r != UiRequest::SaveKeyMap),
            "cancel saves the keymap only when a binding changed"
        );

        // --- OK on a changed page: snapshot **and** the file write ------------------------------
        s.key_bindings.rows[i].current = vec![keyboard(DIK_F7)];
        assert!(s.key_bindings.changed());
        ui.requests.clear();
        let events = press(&mut ui, &mut s, &mut m, ok);
        assert_eq!(
            events,
            vec![PageEvent::Applied {
                rows: n,
                saved: true
            }]
        );
        assert_eq!(
            ui.requests
                .take()
                .into_iter()
                .filter(|r| *r == UiRequest::SaveKeyMap)
                .count(),
            1,
            "save the configured keymap once, without replacing defaults"
        );
        // The snapshot happened, so a second Cancel is a no-op — that is what the saved bindings
        // moving means and it is the difference between Apply and doing nothing.
        assert!(!s.key_bindings.changed());
        assert_eq!(s.key_bindings.rows[i].saved, vec![keyboard(DIK_F7)]);
    }
}

/// Behaviour: options.key-bindings.this-clients-own-actions-are-listed-by-name
#[test]
fn this_clients_own_actions_have_rows_under_their_own_names_in_a_section_of_their_own() {
    let (mut ui, m, p) = page();
    let own = dereth_input::dereth::INPUT_MAP;
    // Each this interface acts on: the classic interface's own settings' keys are not listed.
    for a in dereth_input::dereth::ACTIONS.iter().filter(|a| {
        dereth_input::presentation::find(own, ActionId(a.action)).is_some_and(|r| {
            r.not_used(dereth_input::presentation::Interface::Retail)
                .is_none()
        })
    }) {
        let i = p
            .row_of(own, ActionId(a.action))
            .unwrap_or_else(|| panic!("{} has a row", a.name));
        assert_eq!(p.rows[i].label, a.name);
        assert!(!a.name.is_empty());
    }
    // The performance panel's row shows the keys the key map gives it.
    let perf = p
        .row_of(
            own,
            ActionId(dereth_client_contract::actions::dereth::TOGGLE_PERFORMANCE_PANEL),
        )
        .expect("a row");
    assert_eq!(
        p.rows[perf].current,
        m.find_keys_for_action(
            ActionId(dereth_client_contract::actions::dereth::TOGGLE_PERFORMANCE_PANEL),
            own
        )
    );
    // Each tab that lists them heads them "Dereth".
    let headed = p
        .header_elements
        .iter()
        .filter(|&&h| {
            ui.text_element_mut(h)
                .is_some_and(|t| t.glyphs.inq_text(false) == dereth_input::dereth::SECTION_NAME)
        })
        .count();
    assert_eq!(headed, 3, "movement, interface and character settings");
}

/// The page lists the rows of the shared set this interface acts on and no others: none of the
/// five only the classic interface answers -- right-click mouse look, the stretched layout,
/// automatic shortcuts, and the classic cancel and repeat-message keys -- and neither the hidden
/// quickslots nor the quest detail panel; Disable Most Weather Effects is listed, among the
/// character options.
///
/// Behaviour: keys.retail.the-key-page-lists-the-shared-rows-this-interface-acts-on
#[test]
fn the_key_page_lists_the_shared_rows_this_interface_acts_on_and_no_others() {
    use dereth_input::presentation::{find, Interface};
    let (mut ui, m, p) = page();
    assert!(
        p.rows.iter().all(|r| find(r.input_map, r.action)
            .is_some_and(|row| row.not_used(Interface::Retail).is_none())),
        "every row acts here"
    );
    for h in p.header_elements.clone() {
        let text = ui
            .text_element_mut(h)
            .map(|t| t.glyphs.inq_text(false))
            .unwrap_or_default();
        assert!(
            !text.contains("NOT USED"),
            "no heading over unused rows: {text}"
        );
    }
    let names: Vec<String> = p
        .rows
        .iter()
        .map(|r| dereth_input::names::enum_name_for_action(r.action))
        .collect();
    for gone in [
        "UseQuickSlot_10",
        "UseQuickSlot_18",
        "ToggleQuestManagementPanel",
        "ToggleRightClickMouseLook",
        "ToggleStretchUI",
        "PlayerOption_AutoCreateShortcuts",
        "Cancel",
        "RepeatLastMessage",
    ] {
        assert!(!names.iter().any(|n| n == gone), "{gone} is not listed");
    }
    let weather = p
        .rows
        .iter()
        .find(|r| {
            dereth_input::names::enum_name_for_action(r.action)
                == "PlayerOption_DisableMostWeatherEffects"
        })
        .expect("the weather row");
    assert_eq!(weather.label, "Disable Most Weather Effects");
    let _ = m;
}
