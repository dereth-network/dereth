//! Dialogs raised through the dialog factory: the factory records the context, the host builds the
//! element out of the shipped `Dialog` layout and binding it raises the open-dialog notice; dialogs
//! from different owners on the same queue do not stack (the second waits behind the first, and the
//! open dialog's banner counts it); a reset destroys the open and the queued; and the two menu
//! kinds answer the menu's selected index into `0xA4` / `0xAB`, with -1 for a cancel or an empty
//! menu. The queue id (`0xC3`, default 2, 1 = not queued) and the replace flag (`0x8D`, a bool) are
//! pinned as literals.
//!
//! Fixture: the shipped `client_local_English.dat` layouts `0x2100003C` (`Dialog`) and `0x21000043`
//! (the menus' popup) from the retail dats. Every number is repeated beside the code it constrains.
//! Nothing touches a shard, a live delete dialog or a character.

use dereth_primitives::{AssetSource, DataId};

use dereth_ui::dialog::base::child;
use dereth_ui::dialog::types::dialog_element;
use dereth_ui::dialog::{AnswerRole, DialogController, DialogKind};
use dereth_ui::msg::element::id as msgid;
use dereth_ui::msg::ListenerId;
use dereth_ui::props::attr;
use dereth_ui::NoticeId;
use dereth_ui::{ElemHandle, ElementId, ElementType, PropertyCollection, PropertyValue, UiSystem};

/// The shipped `Dialog` layout — layout enum 2.
const DIALOG_LAYOUT: DataId = DataId(0x2100_003C);
/// The layout the dialog menus' popup comes out of: attribute **7** on the shipped
/// menu base element `0x1000035B` is `0x21000043`, and attribute **6** names root
/// `0x1000035F` inside it, which is what the menu's popup construction builds from.
const MENU_POPUP_LAYOUT: DataId = DataId(0x2100_0043);
/// `0x1000035F` — the popup container. Its one type-5 child is `0x10000360`, which is what
/// attribute **2** names.
const MENU_POPUP_ROOT: ElementId = ElementId(0x1000_035F);
/// `0x10000360` — the list-box element inside the popup.
const MENU_POPUP_LIST: ElementId = ElementId(0x1000_0360);

const ALL_KINDS: [DialogKind; 7] = [
    DialogKind::Confirmation,
    DialogKind::Wait,
    DialogKind::Message,
    DialogKind::TextInput,
    DialogKind::ConfirmationTextInput,
    DialogKind::Menu,
    DialogKind::ConfirmationMenu,
];

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

fn system(e: &Env) -> UiSystem {
    let mut ui = UiSystem::new((800, 600));
    ui.property_types.clone_from(&e.types);
    ui
}

/// Construct the dialog's property collection, and nothing else.
fn dialog_data(kind: DialogKind, text: &str) -> PropertyCollection {
    let mut d = PropertyCollection::new();
    d.set(attr::DIALOG_KIND, PropertyValue::Integer(kind.property()));
    d.set(attr::DIALOG_MODAL, PropertyValue::Bool(true));
    d.set(
        attr::DIALOG_COUNTDOWN_TEXT,
        PropertyValue::String(text.to_string()),
    );
    d
}

/// The dialog factory's element half, driven from `pending_create()` — the
/// host's side of the split, and the shape `CharacterManagementScreen::service_dialog_queue` uses.
fn service(e: &Env, ui: &mut UiSystem) -> Vec<(u64, ElemHandle)> {
    let mut made = Vec::new();
    for (context, kind) in ui.dialogs.pending_create() {
        let h = ui
            .create_root_by_data_id(&e.store, DIALOG_LAYOUT, kind.root_element_id())
            .unwrap_or_else(|err| panic!("{kind:?} root {:?}: {err}", kind.root_element_id()));
        ui.initialize_tree(h);
        assert!(
            ui.bind_dialog_element(context, h),
            "the context took the element"
        );
        made.push((context, h));
    }
    made
}

/// A button's mouse-up raising element message **1** from a child of the
/// dialog. The message bubbles to the dialog root, which is where `DialogElement` listens.
fn press(ui: &mut UiSystem, dialog: ElemHandle, id: ElementId) {
    let b = ui
        .get_child_recursive(dialog, id)
        .unwrap_or_else(|| panic!("child {id:?}"));
    ui.broadcast_element_message(b, msgid::BUTTON_CLICKED, 0, 0);
}

// ---------------------------------------------------------------------------------------------
// 1. The queue seam
// ---------------------------------------------------------------------------------------------

/// **The wire.** A dialog raised through `DialogController` reaches the screen: the factory records
/// the context, `pending_create()` reports what the host owes it, the host builds the element out
/// of the shipped `Dialog` layout, and `bind_dialog_element` completes host binding by
/// bringing it to front and raising the dialog-opened notice.
///
/// Falsified by: taking `dialogs` off `UiSystem` (nothing compiles); dropping the
/// `pending_create` filter so a bound context is offered again; removing the `send_notice` from
/// `bind_dialog_element` (the outbox stays empty).
#[test]
fn a_dialog_raised_through_the_factory_gets_its_element_and_raises_the_open_notice() {
    let e = env();
    let mut ui = system(&e);
    // A listener for the open-dialog notice, which is how a screen learns a dialog appeared.
    let who = ListenerId::External(0x1000_0190);
    ui.notices.register(NoticeId::DialogOpened, who);

    assert!(!ui.dialogs.is_dialog_open(0), "nothing open to begin with");
    let context = ui
        .dialogs
        .make_dialog(dialog_data(DialogKind::Confirmation, "Really?"), 0.0)
        .expect("d[0x8E] = 1 is a kind");
    assert_eq!(
        context, 1,
        "the global dialog context counter starts at one"
    );
    assert!(
        ui.dialogs.is_dialog_open(0),
        "make_dialog created it at once on an empty queue"
    );
    assert_eq!(
        ui.dialogs.pending_create(),
        vec![(context, DialogKind::Confirmation)],
        "the host still owes this context an element"
    );

    let made = service(&e, &mut ui);
    assert_eq!(made.len(), 1);
    let (_, h) = made[0];
    assert_eq!(
        ui.node(h).expect("live").element_id(),
        DialogKind::Confirmation.root_element_id(),
        "d[0x8E] = 1 asks for root 0x15"
    );
    assert!(
        dialog_element(&ui, h).is_some(),
        "and the root really is a DialogElement"
    );
    assert!(
        ui.dialogs.pending_create().is_empty(),
        "nothing is owed one now"
    );

    let out = ui.drain_outbox();
    assert_eq!(
        out.len(),
        1,
        "the open-dialog notice reached the one registered listener"
    );
    match &out[0] {
        dereth_ui::Delivery::Notice { id, payload, .. } => {
            assert_eq!(*id, NoticeId::DialogOpened);
            assert_eq!(
                payload.a, 1,
                "the notice carries the context make_dialog returned"
            );
        }
        other => panic!("expected a notice, got {other:?}"),
    }
}

/// Behaviour: ui.dialog.a-second-dialog-on-the-same-queue-waits-behind-the-first
///
/// **What the queue is for**, stated as the thing that cannot happen without it: dialogs raised by
/// different owners on the same queue do not stack — the second waits, the open dialog's
/// child-`0x33` banner counts it, and it becomes current only when the first closes. This is not
/// how a second *server* question behaves: the gameplay confirmation keeps one slot, and a second
/// question from the shard takes that slot over rather than queueing a dialog behind the first.
///
/// This is the dialog-making `push_back` arm, and the close's tail opens the next dialog in the
/// queue. It is also why the queue is not dead weight for a screen that
/// keeps its own five handles: the character-management screen's five dialog makers all share queue
/// **2** — none of them sets `0xC3` — so an error message raised while the delete warning is up
/// queues behind it.
///
/// Falsified by: hard-coding `pending_create` to report every open dialog (the second one gets an
/// element while the first is still up); dropping `open_next_dialog` from `close_dialog` (the
/// second never arrives); dropping `update_pending_display` (the banner stays at 0).
#[test]
fn a_second_dialog_on_the_same_queue_waits_and_the_banner_counts_it() {
    let e = env();
    let mut ui = system(&e);
    let first = ui
        .dialogs
        .make_dialog(dialog_data(DialogKind::Confirmation, "one"), 0.0)
        .unwrap();
    let second = ui
        .dialogs
        .make_dialog(dialog_data(DialogKind::Message, "two"), 0.0)
        .unwrap();
    assert_ne!(
        first, second,
        "the global dialog context counter is monotonic"
    );

    // Only the first is current: the second has a context and no `Dialog` at all.
    assert_eq!(
        ui.dialogs.pending_create(),
        vec![(first, DialogKind::Confirmation)]
    );
    assert_eq!(ui.dialogs.waiting_on(2), 1);
    assert_eq!(
        ui.dialogs
            .open_on(2)
            .and_then(|i| i.dialog.as_ref())
            .map(|d| d.pending_behind),
        Some(1),
        "pending_behind is the child-0x33 banner's number"
    );
    assert!(
        ui.dialogs.info(second).is_none(),
        "a queued context is not open"
    );

    let made = service(&e, &mut ui);
    assert_eq!(made.len(), 1, "exactly one element exists");
    let (_, first_h) = made[0];

    // Closing the first is what promotes the second, and only then does the host owe an element.
    let gone = ui.dialogs.close_dialog(first, 1.0);
    assert_eq!(
        gone,
        Some(first_h),
        "close_dialog hands back the element to delete"
    );
    ui.remove_and_delete_root(first_h);
    assert_eq!(
        ui.dialogs.pending_create(),
        vec![(second, DialogKind::Message)]
    );
    assert_eq!(ui.dialogs.waiting_on(2), 0);

    let made = service(&e, &mut ui);
    assert_eq!(made.len(), 1);
    assert_eq!(
        ui.node(made[0].1).expect("live").element_id(),
        DialogKind::Message.root_element_id(),
        "d[0x8E] = 3 asks for root 0x24 — the second dialog, not a second copy of the first"
    );
    assert_eq!(
        ui.dialogs.completed.len(),
        1,
        "the first was handed back with its data"
    );
}

/// A reset destroys every open **and** queued dialog with a mode switch, and the elements come
/// back for deletion.
#[test]
fn reset_destroys_the_open_and_the_queued_dialogs() {
    let e = env();
    let mut ui = system(&e);
    ui.dialogs
        .make_dialog(dialog_data(DialogKind::Confirmation, "one"), 0.0);
    ui.dialogs
        .make_dialog(dialog_data(DialogKind::Message, "two"), 0.0);
    let made = service(&e, &mut ui);
    assert_eq!(made.len(), 1);

    let elements = ui.dialogs.reset();
    assert_eq!(elements, vec![made[0].1], "the one element that existed");
    for h in elements {
        ui.remove_and_delete_root(h);
        assert!(!ui.is_alive(h));
    }
    assert!(!ui.dialogs.is_dialog_open(0));
    assert_eq!(ui.dialogs.waiting_on(2), 0);
    assert_eq!(ui.dialogs.completed.len(), 2, "both, open and queued alike");
}

/// The factory is a plain field on `UiSystem`, so anything holding one can raise a dialog. A
/// `Screen` gets a `&mut UiSystem` and nothing else.
#[test]
fn the_factory_is_reachable_through_the_reference_a_screen_is_given() {
    fn a_screen_would_see(ui: &mut UiSystem) -> Option<u64> {
        ui.dialogs
            .make_dialog(dialog_data(DialogKind::Wait, "Please wait"), 0.0)
    }
    let mut ui = UiSystem::new((800, 600));
    assert_eq!(a_screen_would_see(&mut ui), Some(1));
    assert!(ui.dialogs.is_dialog_open(2));
    // A fresh factory starts with no open dialog queues.
    assert!(!DialogController::new().is_dialog_open(0));
}

// ---------------------------------------------------------------------------------------------
// 2. The menus
// ---------------------------------------------------------------------------------------------

/// The two kinds that own a menu element, and the five that do not.
///
/// The menu dialog's `set_data` looks up child `0x1D` and requires it to be type 6; the
/// confirmation-menu dialog's `set_data` looks up child `0x21` and requires the same.
/// Type **6** is a menu element.
///
/// The second half of the test is the one that matters: the shipped `Dialog` layout really does
/// put a type-6 element at each of those two ids, so "`DialogElement` holds no menu" was a gap in
/// this crate and not a gap in the data.
///
/// Falsified by: transposing `0x1D` and `0x21`; pointing either at its kind's *button*
/// (`0x1E` / `0x22`), which is a text element and not a menu.
#[test]
fn both_menu_kinds_name_the_menu_their_own_setdata_casts_and_the_layout_has_one_there() {
    let rows: [(DialogKind, Option<u32>); 7] = [
        (DialogKind::Menu, Some(0x1D)),
        (DialogKind::ConfirmationMenu, Some(0x21)),
        (DialogKind::Confirmation, None),
        (DialogKind::ConfirmationTextInput, None),
        (DialogKind::Message, None),
        (DialogKind::TextInput, None),
        (DialogKind::Wait, None),
    ];
    assert_eq!(
        rows.len(),
        ALL_KINDS.len(),
        "all seven kinds, no more and no fewer"
    );
    for (kind, want) in rows {
        assert_eq!(kind.menu_child().map(|c| c.0), want, "{kind:?}");
    }
    assert_eq!(DialogKind::Menu.menu_child(), Some(child::MENU_MENU));
    assert_eq!(
        DialogKind::ConfirmationMenu.menu_child(),
        Some(child::CONFIRM_MENU_MENU)
    );
    // The menu is never the button.
    for kind in [DialogKind::Menu, DialogKind::ConfirmationMenu] {
        assert_ne!(kind.menu_child(), kind.answer_children().0, "{kind:?}");
    }

    let e = env();
    for kind in [DialogKind::Menu, DialogKind::ConfirmationMenu] {
        let mut ui = system(&e);
        let h = ui
            .create_root_by_data_id(&e.store, DIALOG_LAYOUT, kind.root_element_id())
            .expect("the kind's root is in the shipped Dialog layout");
        ui.initialize_tree(h);
        let menu = ui
            .get_child_recursive(h, kind.menu_child().expect("a menu kind"))
            .unwrap_or_else(|| panic!("{kind:?}: no menu child in the shipped layout"));
        assert_eq!(
            ui.node(menu).expect("live").ty(),
            ElementType(6),
            "{kind:?}: set_data's type-6 check has a menu to find"
        );
    }
}

/// **A confirmation-menu dialog's cancel answers -1 without ever looking at the menu**, and that is
/// a behaviour rather than a fallback: its message listener primes the answer with
/// `-1` and asks the menu for its selected index **only** on the accept id `0x22`, so the `0x23`
/// arm writes the -1 it started with. Its
/// cancel writes the same literal.
///
/// Falsified by: transposing `ConfirmationMenu`'s accept/cancel row; making the cancel arm read
/// the menu; returning `None` again for the menu kinds.
#[test]
fn a_confirmation_menu_cancel_answers_minus_one_into_0xab() {
    let e = env();
    let mut ui = system(&e);
    let kind = DialogKind::ConfirmationMenu;
    let h = ui
        .create_root_by_data_id(&e.store, DIALOG_LAYOUT, kind.root_element_id())
        .unwrap();
    ui.initialize_tree(h);
    assert_eq!(
        dialog_element(&ui, h).expect("a dialog").answer_property(),
        None,
        "nothing yet"
    );

    press(&mut ui, h, child::CONFIRM_MENU_CANCEL);

    let d = dialog_element(&ui, h).expect("still live");
    assert_eq!(d.answer, Some(ElementId(0x23)));
    assert_eq!(d.answer_role, Some(AnswerRole::Cancel));
    assert_eq!(d.answer_index, Some(-1));
    assert_eq!(
        d.answer_property(),
        Some((attr::DIALOG_CONFIRM_MENU_ANSWER, PropertyValue::Integer(-1))),
        "0xAB, the key the close-dialog notice's case 3 would read"
    );
}

/// Behaviour: ui.dialog.a-menu-dialog-answers-into-its-property
///
/// A menu dialog's single button `0x1E` writes the selected index into `0xA4`
/// — the index read straight off the menu, and no other property.
///
/// **The answer is -1 for a menu with no popup.** The menu element exists, is type 6, and carries
/// the shipped list-box and popup element ids in attributes **2** and **6**; the popup those ids
/// point into is not built until something runs `make_popup`. The selected-index read answers -1
/// for a menu with no list box, so -1 is the client's own answer for this state.
///
/// Falsified by: returning `None` for `Menu` in `answer_property`;
/// dropping the attribute-2/6 arms from `Menu::on_set_attribute` (the ids read as `None`).
#[test]
fn a_menu_dialog_answers_into_0xa4_and_an_empty_menu_answers_minus_one_for_a_named_reason() {
    let e = env();
    let mut ui = system(&e);
    let kind = DialogKind::Menu;
    let h = ui
        .create_root_by_data_id(&e.store, DIALOG_LAYOUT, kind.root_element_id())
        .unwrap();
    ui.initialize_tree(h);
    let menu = ui
        .get_child_recursive(h, child::MENU_MENU)
        .expect("child 0x1D");

    // The denominator. The shipped base element `0x1000035B` carries 2 = 0x10000360 (the list box)
    // and 6 = 0x1000035F (the popup root), and both reached the widget through `on_set_attribute`.
    let props = ui.node(menu).expect("live").merged_properties();
    assert_eq!(
        props.get_enum(2).map(ElementId),
        Some(MENU_POPUP_LIST),
        "attribute 2 names the list-box element inside the popup"
    );
    assert_eq!(
        props.get_enum(6).map(ElementId),
        Some(MENU_POPUP_ROOT),
        "attribute 6 names the popup root"
    );
    assert_eq!(
        props.get_data_id(7),
        Some(MENU_POPUP_LAYOUT),
        "attribute 7 names the layout make_popup loads it from"
    );
    // ...and the popup itself does not exist, because nothing runs `make_popup`.
    assert!(
        ui.get_child_recursive(menu, MENU_POPUP_LIST).is_none(),
        "no popup subtree until make_popup runs"
    );
    assert_eq!(
        ui.menu_selected_index(menu),
        -1,
        "the selected index of a menu with no list box"
    );

    press(&mut ui, h, child::MENU_BUTTON);

    let d = dialog_element(&ui, h).expect("still live");
    assert_eq!(d.answer, Some(ElementId(0x1E)));
    assert_eq!(d.answer_role, Some(AnswerRole::Accept));
    assert_eq!(
        d.answer_property(),
        Some((attr::DIALOG_MENU_ANSWER, PropertyValue::Integer(-1))),
        "0xA4, the key the dialog-close handler's case 2 would read"
    );
}

/// The menu wire, end to end, through the shipped data: `make_popup` builds the popup as a
/// **root**, `initialize_popup` binds the list box below it, and `add_text_item` fills the item
/// list, so **a menu given its rows answers the selected row, not -1.**
///
/// The rows a dialog uses come from the caller's collection:
/// the menu dialog's `set_data` walks property **`0xA0`** as an array — a count and a pointer per
/// entry — into text-item insertion, and the confirmation-menu dialog does the same with
/// `0xA6`.
#[test]
fn the_popup_the_shipped_data_describes_binds_the_list_box_and_the_rows_are_real() {
    let e = env();
    let mut ui = system(&e);
    let kind = DialogKind::ConfirmationMenu;
    let h = ui
        .create_root_by_data_id(&e.store, DIALOG_LAYOUT, kind.root_element_id())
        .unwrap();
    ui.initialize_tree(h);
    let menu = ui
        .get_child_recursive(h, child::CONFIRM_MENU_MENU)
        .expect("child 0x21");
    assert!(
        dereth_ui::widgets::menu::list_box_handle(&ui, menu).is_none(),
        "no popup yet"
    );
    assert_eq!(
        ui.menu_selected_index(menu),
        -1,
        "and no list box answers -1"
    );

    // The popup comes out of the menu's own attributes 7 and 6, not out of this file.
    let popup = dereth_ui::widgets::menu::make_popup(&mut ui, &e.store, menu).expect("make_popup");
    assert_eq!(
        ui.node(popup).expect("live").element_id(),
        MENU_POPUP_ROOT,
        "attribute 6 names 0x1000035F in layout 0x21000043, which attribute 7 names"
    );
    assert_eq!(ui.node(popup).expect("live").layout_did, MENU_POPUP_LAYOUT);
    assert!(
        !ui.node(popup).expect("live").region.flags.visible,
        "a newly created menu popup starts hidden"
    );
    assert_ne!(
        ui.parent(popup),
        Some(menu),
        "created as a root element, not as a child"
    );

    let list = dereth_ui::widgets::menu::initialize_popup(&mut ui, menu)
        .expect("initialisation binds the list box below the popup");
    assert_eq!(
        ui.node(list).expect("live").element_id(),
        MENU_POPUP_LIST,
        "and it is the id attribute 2 names"
    );
    assert_eq!(
        ui.node(list).expect("live").ty(),
        ElementType(5),
        "list-box element — what the selected-index lookup walks"
    );
    assert_eq!(
        ui.menu_selected_index(menu),
        -1,
        "an empty menu still answers -1"
    );

    // Add a text item, three times.
    for caption in ["Yes", "No", "Maybe"] {
        dereth_ui::widgets::menu::add_text_item(&mut ui, &e.store, menu, caption)
            .unwrap_or_else(|| panic!("add_text_item({caption})"));
    }
    assert_eq!(
        dereth_ui::widgets::menu::num_items(&ui, menu),
        3,
        "the menu holds three items"
    );
    assert_eq!(
        ui.menu_selected_index(menu),
        -1,
        "rows, but nothing chosen yet"
    );

    // Setting the menu dialog's data selects the requested row.
    dereth_ui::widgets::menu::set_selected_index(&mut ui, menu, 1);
    assert_eq!(
        ui.menu_selected_index(menu),
        1,
        "the answer is no longer -1"
    );

    // And the answer travels.
    press(&mut ui, h, child::CONFIRM_MENU_ACCEPT);
    let d = dialog_element(&ui, h).expect("still live");
    assert_eq!(d.answer_role, Some(AnswerRole::Accept));
    assert_eq!(
        d.answer_property(),
        Some((attr::DIALOG_CONFIRM_MENU_ANSWER, PropertyValue::Integer(1)))
    );
}

/// **Five of the seven kinds write an answer, and each produces one.** `Message` and `Wait`
/// produce none because neither writes anything at all — the message and wait dialogs only close.
#[test]
fn five_of_the_seven_kinds_write_an_answer_and_each_produces_one() {
    let e = env();
    let mut answered = Vec::new();
    for kind in ALL_KINDS {
        let Some(accept) = kind.answer_children().0 else {
            assert_eq!(
                kind,
                DialogKind::Wait,
                "only the wait-dialog variant has no button at all"
            );
            assert_eq!(kind.answer_property(), None);
            continue;
        };
        // `TextInput`'s root `0x28` is in no shipped layout, so it cannot be built; its table
        // rows are still asserted.
        if kind == DialogKind::TextInput {
            assert_eq!(kind.answer_property(), Some(attr::DIALOG_TEXT_INPUT_ANSWER));
            continue;
        }
        let mut ui = system(&e);
        let h = ui
            .create_root_by_data_id(&e.store, DIALOG_LAYOUT, kind.root_element_id())
            .unwrap();
        ui.initialize_tree(h);
        press(&mut ui, h, accept);
        let got = dialog_element(&ui, h).expect("live").answer_property();
        match kind.answer_property() {
            Some(key) => {
                let (k, _) = got
                    .unwrap_or_else(|| panic!("{kind:?} pressed its accept and answered nothing"));
                assert_eq!(k, key, "{kind:?} answers into its own property");
                answered.push(kind);
            }
            None => {
                assert!(got.is_none(), "{kind:?} writes nothing at all");
                assert!(matches!(kind, DialogKind::Message), "{kind:?}");
            }
        }
    }
    // `TextInput` is counted by its table row above rather than by a press, so four are driven.
    assert_eq!(
        answered,
        vec![
            DialogKind::Confirmation,
            DialogKind::ConfirmationTextInput,
            DialogKind::Menu,
            DialogKind::ConfirmationMenu,
        ],
        "every buildable answering kind produced its own key"
    );
}

/// **The queue and replace property numbers, as literals.**
///
/// Every other test writes the property through the same symbol it is read through, so a wrong
/// number stays consistent and invisible; this test pins the literals independently.
///
/// The oracle is the retail client's handling of each, stated here rather than only in the doc
/// comment:
///
/// * `0xC3` — the queue slot is seeded with `2`, the property is fetched **into that same slot**
///   through the **ulong** getter, and the code branches on that slot being `1`, whose equal arm
///   creates the dialog at once. A dword, used as the queue key — not a bool.
/// * `0x8D` — fetched through the **bool** getter into a **one-byte** destination, the same
///   getter the bool `0xAC` uses, and tested as a byte. It is a replacement flag,
///   not a dialog pointer.
#[test]
fn the_dialog_queue_and_replace_properties_are_0xc3_and_0x8d() {
    assert_eq!(
        attr::DIALOG_QUEUE_ID,
        0xC3,
        "the queue id the dialog-making probe reads"
    );
    assert_eq!(
        attr::DIALOG_REPLACE,
        0x8D,
        "the replace flag the same probe reads as a byte"
    );
    // ...and they are different keys: `0xC3` is not the flag.
    assert_ne!(attr::DIALOG_QUEUE_ID, attr::DIALOG_REPLACE);
    // The default the queue slot is seeded with, and the one value that is not a queue.
    assert_eq!(
        dereth_ui::dialog::factory::DEFAULT_QUEUE,
        2,
        "ordinary dialogs use queue 2"
    );
    assert_eq!(
        dereth_ui::dialog::factory::NON_QUEUED,
        1,
        "non-queued dialogs use mode 1"
    );
}
