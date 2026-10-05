//! The keyboard and the caret: the steps a scenario drives text and bound actions with.
//!
//! This is the crate's general key driver: neither a subject's adapter (like `adapters_chat::Hand`)
//! nor a scenario's private helper. The journal scenarios type into an edit box, and the
//! inscription scenarios are about the focus edges of one.
//!
//! [`crate::Player::Press`], [`crate::Player::Type`] and [`crate::Player::Focus`] are one line
//! each over [`press`], [`type_text`] and [`focus`] below. This module is the implementation and
//! is what a scenario calls for a key *edge* -- [`key`], [`tap`], [`press_bound`], [`bound_scan_code`],
//! [`press_return`] have no variant, because a `when` list is a list of whole gestures and half a
//! tap is not one. Nothing here duplicates a step that exists -- [`crate::Player::Say`] queues a
//! chat line and [`crate::Player::Click`] is a pointer gesture; these are the keyboard.
//!
//! # All of it needs the shell
//!
//! The input manager is part of the UI shell, so every function here is `Assets::Retail` with
//! `ClientSpec::gameplay(..)` or `ClientSpec::screen(..)`, and panics with that sentence otherwise.
//!
//! # The messages are the client's own
//!
//! A key press is built through [`dereth_desktop::pump::Pump`] -- the client's *own* Windows message
//! mapper -- and delivered where the window loop delivers it: to the pump's state and to the real
//! input manager. A character is a `WM_CHAR`, which is what the message loop produces *after*
//! translation, because that is the message an edit box reads.

use dereth_input::keys::Key;
use dereth_input::win32::Win32Message;

use crate::client::HeadlessClient;
use crate::player::{self, Target};

/// The input map a UI action is dispatched on -- the client's own, not a second opinion on it.
pub use dereth_client_shell::ui::UI_INPUT_MAP;

/// One bound action, one-shot, as pressing the key it is bound to does.
///
/// This is `press_the_key`'s body from `scenarios_panels.rs` with the action as a parameter. It
/// injects the action the input manager would have fired rather than a scan code, because **what
/// key an action is bound to is the keymap's claim and not this one's**: a scenario that pressed
/// `A` would be asserting over the shipped binding as well as over what the action does, and the
/// binding has its own rows.
///
/// # Panics
/// Panics on a client with no UI shell.
pub fn press(c: &mut HeadlessClient, action: dereth_input::ActionId) {
    press_on_map(c, action, UI_INPUT_MAP);
}

/// [`press`] on a named input map, for the actions that are not the UI map's -- movement is map 4.
///
/// # Panics
/// Panics on a client with no UI shell.
pub fn press_on_map(
    c: &mut HeadlessClient,
    action: dereth_input::ActionId,
    input_map: dereth_input::InputMapId,
) {
    let e = dereth_input::InputEvent {
        action,
        input_map,
        toggle: dereth_input::ToggleType::OneShot,
        extent: 1.0,
        start: true,
        repeat_delta: 1,
        repeat_total: 0,
        // Injected straight into the queue, with no key-down message in progress.
        from_key_down: false,
    };
    c.app_mut()
        .input_manager_mut()
        .expect(
            "a bound action goes through the client's input manager, which is part of the UI \
             shell; build the scenario with ClientSpec::gameplay(..) or ClientSpec::screen(..)",
        )
        .inject_action(e);
    c.tick(1);
}

/// `USE` -- the action the shipped keymap binds to `R`, and the one a player presses to use what
/// is selected. The use-feedback and interaction scenarios drive it.
pub const USE: dereth_input::ActionId =
    dereth_input::ActionId(dereth_client_runtime::interaction::action::USE);

/// The input map the UI commands live on, which is the one `USE` is bound in.
pub const UI_COMMANDS: dereth_input::InputMapId = dereth_input::InputMapId(0x1000_0009);

/// The scan code the **shipped keymap** binds `action` to on `map`, unmodified (no meta key held).
///
/// # Panics
/// Panics on a client with no UI shell, and when the shipped keymap has no unmodified binding for
/// the action -- which is a claim about the keymap and not a state the client could be in.
#[must_use]
pub fn bound_scan_code(
    c: &mut HeadlessClient,
    action: dereth_input::ActionId,
    map: dereth_input::InputMapId,
) -> u16 {
    c.app_mut()
        .input_manager_mut()
        .expect(
            "the keymap is part of the client's input shell; build the scenario with \
             ClientSpec::gameplay(..) or ClientSpec::screen(..)",
        )
        .keys_for_action(action, map)
        .into_iter()
        .find(|b| b.meta_mode == 0)
        .map(|b| b.control.offset())
        .unwrap_or_else(|| panic!("the shipped keymap binds no unmodified key to {action:?}"))
}

/// Press the key the **shipped keymap** binds `action` to, as a player's finger does.
///
/// It is not the same step as [`press`]: that one injects the action the input manager would have
/// fired, so the binding is out of the claim; this one goes in at the **key**, so the scenario is
/// asserting the shipped binding as well as what the action does. Both are worth having and the
/// difference is the point.
///
/// `key` is the host's own resolved key -- the virtual key and the scan code -- which a scenario
/// gets from `dereth_desktop::platform::window::key_from_key_code(KeyCode::KeyR)`. Naming a key is
/// the window layer's job and this crate must not take a second opinion on it; `winit` is a
/// dev-dependency here for exactly that reason. The scan code is **asserted against the shipped
/// binding**, low seven bits, so a scenario that names the wrong key fails saying so rather than
/// pressing something else.
///
/// # Panics
/// As [`bound_scan_code`], and when `key` is not the key the keymap binds.
pub fn press_bound(
    c: &mut HeadlessClient,
    action: dereth_input::ActionId,
    map: dereth_input::InputMapId,
    key: Key,
) {
    let bound = bound_scan_code(c, action, map);
    assert_eq!(
        key.scan_code & 0x7F,
        bound & 0x7F,
        "the shipped keymap binds {action:?} to scan code {bound:#06X}, and this press names \
         {:#06X}",
        key.scan_code
    );
    self::key(c, key, true);
    self::key(c, key, false);
    // The frames the dialog or the request the press raises needs to reach the screen, which is
    // what every station that drove this by hand ran.
    c.tick(SETTLE_FRAMES);
}

/// [`press_bound`] for [`USE`] on [`UI_COMMANDS`] -- the shipped use key.
///
/// # Panics
/// As [`press_bound`].
pub fn press_use(c: &mut HeadlessClient, key: Key) {
    press_bound(c, USE, UI_COMMANDS, key);
}

/// The frames allowed for a key press to update the displayed screen.
pub const SETTLE_FRAMES: u64 = 6;

/// One key transition, as a key going down or coming up.
///
/// It takes the host's own resolved key -- the virtual key and the scan code -- rather than a
/// name, because naming a key is the window layer's job and this crate must not take a second
/// opinion on it. A scenario gets one from `dereth_desktop::platform::window::key_from_key_code`.
///
/// No frame is run: the two edges of a tap are often wanted in one frame, and [`tap`] is the pair.
///
/// # Panics
/// Panics on a client with no UI shell.
pub fn key(c: &mut HeadlessClient, k: Key, pressed: bool) {
    let t = c.next_pointer_time();
    let mut pump = player::pointer_pump();
    let m = pump.key_message_for_key(k, pressed, t);
    player::deliver(c, &mut pump, m);
}

/// A key pressed and released, with a frame after each edge -- which is what a player's finger is.
///
/// # Panics
/// Panics on a client with no UI shell.
pub fn tap(c: &mut HeadlessClient, k: Key) {
    key(c, k, true);
    c.tick(1);
    key(c, k, false);
    c.tick(1);
}

/// Type `text` into whatever holds the keyboard, one character at a time, and run the frame that
/// consumes it.
///
/// # Panics
/// Panics on a client with no UI shell, and on a character outside plain ASCII. The wide-character
/// path is a claim of its own, and a scenario that typed one here would be asserting over the code
/// page of whatever machine ran it -- the same reason the chat-room scenarios are deliberately
/// ASCII.
pub fn type_text(c: &mut HeadlessClient, text: &str) {
    for byte in text.bytes() {
        assert!(
            byte.is_ascii(),
            "this gesture is deliberately ASCII: {text:?}"
        );
        let t = c.next_pointer_time();
        let mut pump = player::pointer_pump();
        let m = Win32Message::new(dereth_input::win32::msg::WM_CHAR, usize::from(byte), 0, t);
        player::deliver(c, &mut pump, m);
    }
    c.tick(1);
}

/// Press and release the return key, which is what commits a line.
///
/// # Panics
/// Panics on a client with no UI shell.
pub fn press_return(c: &mut HeadlessClient) {
    for (msg, lparam) in [
        (dereth_input::win32::msg::WM_KEYDOWN, 0x001c_0001_u32),
        (dereth_input::win32::msg::WM_KEYUP, 0xc01c_0001_u32),
    ] {
        let t = c.next_pointer_time();
        let mut pump = player::pointer_pump();
        #[allow(clippy::cast_possible_wrap)]
        let m = Win32Message::new(msg, 13, lparam as isize, t);
        player::deliver(c, &mut pump, m);
    }
    c.tick(1);
}

/// Put the caret in `target`: the pointer goes there and the left button goes down and up, which
/// is the only thing that moves the keyboard focus in this client.
///
/// It is [`crate::Player::Click`] under a name that says what it is for, because the **focus
/// edges** are what the three inscription modules are about -- a click that drops a caret in, a
/// click elsewhere that commits what was typed -- and a scenario that says `focus` reads as the
/// claim it is making.
///
/// # Panics
/// Panics on a client with no UI shell, and on an element the shipped layout does not carry or
/// that is clipped away entirely.
pub fn focus(c: &mut HeadlessClient, target: impl Into<Target>) {
    use crate::client::Step as _;

    crate::Player::Click(target.into()).apply(c);
}
