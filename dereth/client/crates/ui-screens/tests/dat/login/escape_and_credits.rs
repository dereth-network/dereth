//! Escape on character select raises the confirm-exit dialog once; the credits please-wait is a
//! real dialog element carrying the dat prompt; any action skips the credits.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use dereth_primitives::{AssetSource, DataId};
use dereth_ui::focus::InputEvent;
use dereth_ui::framework::Screen;
use dereth_ui::{Delivery, ElemHandle, ElementId, UiSystem};

use dereth_ui_screens::screens::charmgmt::{CharacterManagementScreen, DialogContext};
use dereth_ui_screens::screens::credits::{self, CreditsScreen};

#[derive(Debug)]
struct Strings {
    store: dereth_dat::RetailDatStore,
    tables: RefCell<BTreeMap<DataId, Option<BTreeMap<u32, Vec<String>>>>>,
}

impl dereth_ui::text::StringResolver for Strings {
    fn resolve_raw(&self, table: DataId, string_id: u32) -> Option<String> {
        if !self.tables.borrow().contains_key(&table) {
            use dereth_assets::Decode;
            let loaded = self.store.read(table).ok().and_then(|b| {
                dereth_assets::ui::StringTable::decode_payload(table, &b)
                    .ok()
                    .map(|t| t.strings.into_iter().map(|(k, v)| (k, v.strings)).collect())
            });
            self.tables.borrow_mut().insert(table, loaded);
        }
        let t = self.tables.borrow();
        t.get(&table)?.as_ref()?.get(&string_id)?.first().cloned()
    }
}

fn env() -> UiSystem {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let strings = dereth_dat::testing::open_store_or_fail();
    ui.strings = Some(Rc::new(Strings {
        store: strings,
        tables: RefCell::new(BTreeMap::new()),
    }));
    ui
}

fn broadcast_action<S: Screen + ?Sized>(ui: &mut UiSystem, screen: &mut S, action: u32) -> usize {
    ui.key_press(&InputEvent {
        action,
        start: true,
        x: 0,
        y: 0,
    });
    let mut n = 0;
    for d in ui.drain_outbox() {
        if let Delivery::Global { id, param, .. } = d {
            screen.on_global_message(&mut dereth_ui::framework::ScreenCx::new(ui), id, param);
            n += 1;
        }
    }
    n
}

/// Deliver only the element messages, the way `UiFlow` does — used to click the EXIT button.
fn pump_elements<S: Screen + ?Sized>(ui: &mut UiSystem, screen: &mut S) {
    for d in ui.drain_outbox() {
        if let Delivery::Element { msg, .. } = d {
            screen.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
        }
    }
}

fn charmgmt() -> (UiSystem, CharacterManagementScreen) {
    let mut ui = env();
    let mut s = CharacterManagementScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the character-management screen builds");
    // `create` raises element traffic of its own; drain it so a later `drain_outbox` sees only
    // what the test caused.
    let _ = ui.drain_outbox();
    (ui, s)
}

// ---------------------------------------------------------------------------------------------
// 1. Escape on character select
// ---------------------------------------------------------------------------------------------

/// Behaviour: dialog-keys.character-select.enter-is-declined-and-escape-asks-once-whether-to-quit
/// Escape raises the confirm exit dialog on character select.
#[test]
fn escape_raises_the_confirm_exit_dialog_on_character_select() {
    let (mut ui, mut s) = charmgmt();
    assert!(
        s.dialog_element(DialogContext::ConfirmExit).is_none(),
        "nothing is up before the key"
    );

    let delivered = broadcast_action(&mut ui, &mut s, 0x27);
    assert_eq!(
        delivered, 0,
        "Unconsumed keyboard messages do not trigger this screen"
    );
    assert!(
        s.dialog_element(DialogContext::ConfirmExit).is_none(),
        "and global message 1 raised nothing"
    );

    // A key that is not cancel must do nothing — the calibration for the assertion below
    // The live-run calibration rule requires proving that the instrument can read the other answer.
    assert!(
        !s.on_action(&mut ui, 0x25),
        "the handler consumes only action 0x27, so it declines accept and the input-handler list gets it"
    );
    assert!(
        s.dialog_element(DialogContext::ConfirmExit).is_none(),
        "action 0x25 (accept) is not 0x27 and raises nothing"
    );

    assert!(
        s.on_action(&mut ui, 0x27),
        "and consumes cancel, so the list is not walked"
    );
    let esc = s
        .dialog_element(DialogContext::ConfirmExit)
        .expect("Escape raised the confirm-exit dialog's element");

    // And it is the same dialog the EXIT button raises: the oracle is the screen's own other
    // producer, not a number written here.
    let mut ui2 = env();
    let mut s2 = CharacterManagementScreen::default();
    s2.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui2))
        .expect("a second screen builds");
    let _ = ui2.drain_outbox();
    let root = ui2
        .get_element(ElementId(0x1000_039A))
        .expect("the char-management root");
    // The quit button. `0x100003A2` is *Enter Game*, not this.
    let exit = ui2
        .get_child_recursive(root, ElementId(0x1000_03A4))
        .expect("the EXIT button 0x100003A4");
    ui2.broadcast_element_message(exit, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
    pump_elements(&mut ui2, &mut s2);
    let by_button = s2
        .dialog_element(DialogContext::ConfirmExit)
        .expect("the EXIT button raises the same one");

    let kind_of = |ui: &UiSystem, h: ElemHandle| ui.node(h).map(dereth_ui::ElementNode::element_id);
    assert_eq!(
        kind_of(&ui, esc),
        kind_of(&ui2, by_button),
        "Escape and EXIT raise the same dialog element"
    );
}

/// A second escape does not raise a second dialog.
#[test]
fn a_second_escape_does_not_raise_a_second_dialog() {
    let (mut ui, mut s) = charmgmt();
    s.on_action(&mut ui, 0x27);
    let first = s
        .dialog_element(DialogContext::ConfirmExit)
        .expect("one dialog");
    let roots_after_one = s.roots().len();

    s.on_action(&mut ui, 0x27);
    assert_eq!(
        s.dialog_element(DialogContext::ConfirmExit),
        Some(first),
        "the second Escape did not replace the element"
    );
    assert_eq!(
        s.roots().len(),
        roots_after_one,
        "and did not add a second root"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The credits' please-wait
// ---------------------------------------------------------------------------------------------

/// **The row's part (b).** The credits' please-wait dialog was a `bool` and nothing else; it is
/// now a real element carrying the prompt retail names.
///
/// The token and the table are literals: `ID_Wait_PleaseWait`, string table enum
/// `0x10000001`, resolved here against `StringTable 0x23000001`.
#[test]
fn the_credits_wait_dialog_is_a_real_element_carrying_the_prompt_from_the_dats() {
    let mut ui = env();
    let before = ui.element_list().len();
    let mut s = CreditsScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the credits screen builds");
    let _ = ui.drain_outbox();
    assert!(
        s.wait_element.is_none(),
        "nothing is up while the roll runs"
    );

    let prompt = ui
        .resolve_string(
            credits::STRING_TABLE,
            dereth_primitives::num::hash::str_hash(credits::PLEASE_WAIT_STRING.as_bytes()),
        )
        .expect("ID_Wait_PleaseWait is a row of StringTable 0x23000001");
    assert_ne!(
        prompt,
        credits::PLEASE_WAIT_STRING,
        "the dat answered, not the fallback"
    );
    assert!(!prompt.is_empty());
    eprintln!("ID_Wait_PleaseWait -> {prompt:?}");

    let delivered = broadcast_action(&mut ui, &mut s, 0x27);
    assert_eq!(
        delivered, 0,
        "Unconsumed keyboard messages do not trigger this screen"
    );
    assert!(!s.finished, "and global message 1 did not end the roll");
    assert!(s.wait_element.is_none(), "nor raise the wait dialog");

    s.on_action(&mut ui);
    assert!(s.finished, "OnAction short-circuits the roll");
    assert!(s.please_wait, "and takes the please-wait dialog arm");
    let h = s.wait_element.expect("the wait dialog has an element");
    assert!(
        s.roots().contains(&h),
        "and it is one of the screen's roots, so the mode switch frees it"
    );

    // The prompt really is on the dialog's text child `0x3E`, which is the one the dialog's text update writes.
    let text = ui
        .get_child_recursive(h, dereth_ui::dialog::base::child::TEXT)
        .and_then(|c| ui.text_element_mut(c))
        .map(|t| t.glyphs.inq_text(false))
        .expect("the dialog's 0x3E text child");
    assert_eq!(text, prompt, "the shipped prompt, not the token");

    // `Wait` is `d[0x8E] = 2` and has no buttons: nothing to click, and the mode switch is what
    // takes it down. Credits-screen teardown closes the factory's context too.
    let ctx = s.wait_context.expect("the wait context");
    assert_eq!(
        ui.dialogs.info(ctx).map(|i| i.kind),
        Some(dereth_ui::dialog::DialogKind::Wait)
    );
    let roots = s.roots().to_vec();
    for h in roots {
        ui.remove_and_delete_root(h);
    }
    s.destroy(&mut dereth_ui::framework::ScreenCx::new(&mut ui));
    ui.clean_delete_queue();
    assert!(
        s.wait_context.is_none(),
        "credits-screen destruction closed the context"
    );
    assert!(ui.dialogs.info(ctx).is_none(), "and the factory forgot it");
    // `screen_conformance`'s step 5, applied here because a dialog element added *after*
    // construction is exactly the shape that leaks past a teardown that only frees `roots()` as it
    // was at `create` time.
    assert_eq!(
        ui.element_list().len(),
        before,
        "the wait dialog's element leaked past the mode switch"
    );
}

/// Behaviour: dialog-keys.credits.either-key-ends-the-roll-and-ends-it-once
/// Any action at all skips the credits.
#[test]
fn any_action_at_all_skips_the_credits() {
    for action in [0x25u32, 0x27, 7] {
        let mut ui = env();
        let mut s = CreditsScreen::default();
        s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .expect("the credits screen builds");
        let _ = ui.drain_outbox();
        assert_eq!(
            broadcast_action(&mut ui, &mut s, action),
            0,
            "action {action:#04X} must not reach this screen through global message 1"
        );
        assert!(
            !s.finished,
            "and global message 1 alone did not skip the roll"
        );
        s.on_action(&mut ui);
        assert!(s.finished, "action {action:#04X} skipped the roll");
        assert!(
            s.wait_element.is_some(),
            "action {action:#04X} raised the wait dialog"
        );
    }
}
