//! `GameplayOptionsPanel` — the Options *Game / Support* page.
//!
//! The page is a much smaller thing than a preference page:
//!
//! * **There is no post-construction initializer.** The class does nothing beyond construction,
//!   its element type, registration and its element-message handler. So the empty `children`
//!   table in `panels/catalogue.rs` is **correct**: there is no initializer to bind anything and
//!   the page holds no child handles.
//! * **It is not a preference page.** It writes no `PlayerOption` bit, sends no
//!   `Character_PlayerOptionChangedEvent 0x0005`, and touches no `UserPreferences` entry. It is
//!   seven buttons, and the handler described below is all of it.
//! * **Two of the seven buttons are also handled in [`crate::screens::gameplay`]**:
//!   `0x10000203` and `0x10000617`. The rest of the page goes through
//!   `super::pages::gameplay_option_action`, the transcription of the handler.
//!
//! # The handler
//!
//! The gameplay options panel's element-message handler answers element message 1 only, and
//! switches on the element id:
//!
//! ```text
//! 0x10000203  -> end character session (argument 1)
//! 0x10000206  -> the support URL
//! 0x10000207  -> the support URL
//! 0x10000617  -> global message (id 1, param 0x10000027)
//! 0x100005CC  -> global message (id 0x0C, param 0)
//! ```
//!
//! The URL arm, for both ids, is `ShellExecuteA("open", "http://support.turbine.com/ics/support/…")`;
//! a result of 32 or less shows a `MessageBoxA` titled "Asheron's Call Error", and anything
//! above 32 is success with no message box.
//!
//! So the class answers **five** element ids on element message 1, and
//! `super::pages::gameplay_option_action` already had all five right.
//!
//! # The shipped page has seven buttons, not five
//!
//! Read off the built `classic_gameplay` tree (layout enum `0x10000006`, root `0x10000495`;
//! 1,870 elements, exactly one of type `0x10000029`). The page element is
//! **`0x10000212`** and its seven direct children are all `Button` (type `1`), in
//! layout order:
//!
//! | y | element | type | how it acts |
//! |--:|---|--:|---|
//! | 20 | `0x10000203` | 1 | class arm — end character session (1) |
//! | 60 | `0x10000617` | 1 | class arm — global message (1, `0x10000027`) |
//! | 110 | `0x10000204` | 1 | **no class arm** — attribute `0x12` = `0x1000001F` |
//! | 150 | `0x100005CC` | 1 | class arm — global message (`0x0C`, 0) |
//! | 190 | `0x10000205` | 1 | **no class arm** — attribute `0x12` = `0x7B` |
//! | 240 | `0x10000206` | 1 | class arm — the support URL |
//! | 280 | `0x10000207` | 1 | class arm — the same support URL |
//!
//! **The two with no class arm are not dead and they are not a gap.** They carry
//! [`dereth_ui::props::attr::BUTTON_INPUT_ACTION`] (`0x12`), and
//! the element base class reads that property *before* the message ever reaches a
//! class handler, so the layout — not `GameplayOptionsPanel` — is what gives them their effect:
//! `0x1000001F` is the input action `ToggleKeyboardPanel` and `0x7B` is `ToggleHelp`
//! (see the input crate's action names). That is why nothing in the retail client's page handler
//! switches on either id. They are listed in `LAYOUT_DRIVEN` and
//! asserted by the station so that a later reader does not file them as missing arms.
//!
//! # What this build does with each
//!
//! | button | here |
//! |---|---|
//! | `0x10000203` | [`crate::screens::gameplay::GamePlayScreen::on_end_character_session`] — the asking form |
//! | `0x10000617` | the key-press path with `0x10000027` |
//! | `0x10000206` / `0x10000207` | [`crate::view::UiRequest::OpenUrl`] on the ordinary request queue |
//! | `0x100005CC` | *Restore Defaults* on all three option pages — see `RESTORE_DEFAULTS_NOTE` |
//! | `0x10000204` / `0x10000205` | the generic button path, `dereth_ui::widgets`' `BUTTON_INPUT_ACTION` dispatch |

use dereth_ui::{ElemHandle, ElementId, MessageId, UiSystem};

use super::pages::{gameplay_option_action, GameplayOptionAction};

/// The class name, so that `panels/catalogue.rs`'s row has a module that names it.
pub const CLASS: &str = "GameplayOptionsPanel";

/// The page element in the shipped `classic_gameplay` tree.
///
/// Measured off the built tree, not taken from a document: it is the **one** element of type
/// `0x10000029` in all 1,870, and its id is `0x10000212`. Both halves are asserted by the station,
/// because an id-only bind can latch onto the wrong element (see `super::character::find_page`).
pub const PAGE_ELEMENT: ElementId = ElementId(0x1000_0212);

/// The seven buttons the shipped page carries, in layout order.
pub mod button {
    use dereth_ui::ElementId;

    /// *Exit to Character Selection* — end character session (1).
    pub const EXIT_TO_CHARACTER_SELECTION: ElementId = ElementId(0x1000_0203);
    /// *Exit Game* — global message (1, `0x10000027`).
    pub const EXIT_GAME: ElementId = ElementId(0x1000_0617);
    /// No class arm; the layout fires input action `0x1000001F` `ToggleKeyboardPanel`.
    pub const KEY_BINDINGS: ElementId = ElementId(0x1000_0204);
    /// *Restore Defaults* — global message (`0x0C`, 0).
    pub const RESTORE_DEFAULTS: ElementId = ElementId(0x1000_05CC);
    /// No class arm; the layout fires input action `0x7B` `ToggleHelp`.
    pub const HELP: ElementId = ElementId(0x1000_0205);
    /// The upper support-ticket button — `ShellExecuteA("open", SUPPORT_URL)`.
    pub const SUPPORT_TICKET_UPPER: ElementId = ElementId(0x1000_0206);
    /// The lower support-ticket button — the same URL, a second arm.
    pub const SUPPORT_TICKET_LOWER: ElementId = ElementId(0x1000_0207);
}

/// Every button on the page, in the layout's own order, with the `y` the shipped tree gives it.
///
/// The order is load bearing only as documentation: it is how the page reads on screen, and it is
/// what the station compares the built tree against.
pub const BUTTONS: [(ElementId, i32); 7] = [
    (button::EXIT_TO_CHARACTER_SELECTION, 20),
    (button::EXIT_GAME, 60),
    (button::KEY_BINDINGS, 110),
    (button::RESTORE_DEFAULTS, 150),
    (button::HELP, 190),
    (button::SUPPORT_TICKET_UPPER, 240),
    (button::SUPPORT_TICKET_LOWER, 280),
];

/// The two buttons the element-message handler does **not** answer, and the input action the shipped
/// layout puts on each in attribute `0x12`.
///
/// `0x1000001F` is `ToggleKeyboardPanel` and `0x7B` is `ToggleHelp`; `0x7B` is the F1 help, which
/// is intentionally inert in this build, so the *Help* button is inert for the same reason.
pub const LAYOUT_DRIVEN: [(ElementId, u32); 2] = [
    (button::KEY_BINDINGS, 0x1000_001F),
    (button::HELP, 0x0000_007B),
];

/// What global message (`0x0C`, 0) means, and how far this build takes it.
///
/// Global message `0x0C` is [`dereth_ui::msg::global::RESTORE_DEFAULTS`]. In the client every
/// options page and every `ActionKeyMapOption` is registered for it, so the button is a
/// *restore every default on every options page* control — not a "refresh the options panels".
///
/// This build takes it to the three `PlayerOptionPage` subclasses — Client Options, Character
/// Options and Chat Options — which is exactly the reach of each page's own *Defaults* button
/// (`super::config::button::DEFAULTS`). The fourth listener, the action-key-map control's
/// global-message handler → its mouse-turning defaults write, needs the host's
/// `InputManager` and cannot run inside an element-message dispatch; it is the one part of this
/// button that is still short of retail.
pub const RESTORE_DEFAULTS_NOTE: &str =
    "global 0x0C restores defaults on every option page; the key-map rows are not reached yet";

/// The Game / Support page, bound to the built tree.
///
/// It holds a handle and nothing else: the page has no state because retail's has none — no
/// post-construction initializer, no child handles, no option array.
#[derive(Debug, Default, Clone, Copy)]
pub struct GameplayOptionsPage {
    /// The `0x10000212` element, once [`Self::bind`] has found it.
    pub page: Option<ElemHandle>,
}

impl GameplayOptionsPage {
    /// Find the page under `root`, by **id and type**, the way the other option pages are bound.
    #[must_use]
    pub fn bind(ui: &UiSystem, root: ElemHandle) -> Self {
        let page = ui.element_list().iter().copied().find(|h| {
            ui.node(*h).is_some_and(|n| {
                n.element_id() == PAGE_ELEMENT
                    && n.ty() == crate::element_types::ty::GAMEPLAY_OPTIONS
            }) && is_under(ui, *h, root)
        });
        Self { page }
    }

    /// The whole of it.
    ///
    /// Returns the action the client performs, or `None` when the message is not this page's.
    /// The effects themselves belong to the host and to the screen (a notice delivery, a key
    /// press, a request), which is why this returns rather than acts — the same division
    /// [`super::pages::GameplayOptionAction`] was written for.
    ///
    /// The source is required to sit **under the bound page**. The page's five answered ids are
    /// unique in the shipped tree today, but `0x100001FC`–`0x100001FE` taught this crate twice
    /// that an options id is not a page identity (`super::config::CONFIG_PAGE_ELEMENT`), so the
    /// containment test is here from the start.
    #[must_use]
    pub fn on_element_message(
        &self,
        ui: &UiSystem,
        m: &dereth_ui::msg::ElementMessage,
    ) -> Option<GameplayOptionAction> {
        if m.id != MessageId(1) {
            return None;
        }
        let page = self.page?;
        if !is_under(ui, m.source, page) {
            return None;
        }
        gameplay_option_action(m.source_id)
    }

    /// The request a click raises, for the two arms whose whole effect is a request.
    ///
    /// `0x10000206` and `0x10000207` both put [`crate::view::UiRequest::OpenUrl`] on the queue, which
    /// is where `ShellExecuteA("open", url)` lives in this build: this crate may not call the
    /// shell, and the request queue is the seam every other host effect on this screen uses.
    pub fn emit(requests_out: &mut crate::requests::Outbox, action: &GameplayOptionAction) -> bool {
        match action {
            GameplayOptionAction::Request(r) => {
                requests_out.emit(r.clone());
                true
            }
            GameplayOptionAction::BroadcastGlobal { .. } => false,
        }
    }
}

fn is_under(ui: &UiSystem, mut h: ElemHandle, root: ElemHandle) -> bool {
    loop {
        if h == root {
            return true;
        }
        match ui.parent(h) {
            Some(p) => h = p,
            None => return false,
        }
    }
}

/// The support URL, re-exported so a consumer needs one import.
pub use super::pages::SUPPORT_URL;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::UiRequest;

    /// The five ids the handler answers, against the transcription in `pages.rs`.
    #[test]
    fn the_five_answered_ids_are_the_ones_the_handler_switches_on() {
        assert_eq!(
            gameplay_option_action(button::EXIT_TO_CHARACTER_SELECTION),
            Some(GameplayOptionAction::Request(
                UiRequest::EndCharacterSession { ask: true }
            ))
        );
        assert_eq!(
            gameplay_option_action(button::EXIT_GAME),
            Some(GameplayOptionAction::BroadcastGlobal {
                id: 1,
                param: 0x1000_0027
            })
        );
        assert_eq!(
            gameplay_option_action(button::RESTORE_DEFAULTS),
            Some(GameplayOptionAction::BroadcastGlobal { id: 0x0C, param: 0 })
        );
        for b in [button::SUPPORT_TICKET_UPPER, button::SUPPORT_TICKET_LOWER] {
            assert_eq!(
                gameplay_option_action(b),
                Some(GameplayOptionAction::Request(UiRequest::OpenUrl(
                    SUPPORT_URL
                )))
            );
        }
        // And the two the class does not answer.
        for (b, _) in LAYOUT_DRIVEN {
            assert_eq!(
                gameplay_option_action(b),
                None,
                "{:#010X} has no class arm",
                b.0
            );
        }
    }

    /// The switch subtraction establishes `0x100005CC + 0x4B == 0x10000617`.
    #[test]
    fn the_two_broadcast_ids_are_the_pair_the_switch_subtracts() {
        assert_eq!(button::RESTORE_DEFAULTS.0 + 0x4B, button::EXIT_GAME.0);
        assert_eq!(dereth_ui::msg::global::RESTORE_DEFAULTS, MessageId(0x0C));
    }
}
