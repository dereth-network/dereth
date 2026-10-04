//! The Game / Support page maps controls to the shared sheet's actions.
//!
//! Logout, exit and mouse-turning defaults are handled by the screen. Keyboard uses its
//! authored input action; the support buttons receive their forms' input actions during
//! arrangement. They are excluded from manual dispatch so a click opens a form only once.
//! Help remains hidden. The shipped button positions are retained in `BUTTONS`.

use dereth_ui::{ElemHandle, ElementId, MessageId, UiSystem};

use super::pages::gameplay_option_action;
use dereth_client_contract::options::sheet::Act;

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
    /// *Use Mouse Turning Settings* — global message (`0x0C`, 0).
    pub const MOUSE_TURNING_SETTINGS: ElementId = ElementId(0x1000_05CC);
    /// No class arm; the layout fires input action `0x7B` `ToggleHelp`.
    pub const HELP: ElementId = ElementId(0x1000_0205);
    /// The upper support button opens Urgent Assistance.
    pub const SUPPORT_TICKET_UPPER: ElementId = ElementId(0x1000_0206);
    /// The lower support button opens Report Abuse.
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
    (button::MOUSE_TURNING_SETTINGS, 150),
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

/// The input action that opens the in-game Urgent Assistance form, `ToggleUrgentAssistancePanel`.
pub const URGENT_ASSISTANCE_ACTION: u32 = crate::panels::urgent_assistance::TOGGLE_ACTION;
/// The input action that opens the in-game Report Abuse form, `ToggleAbusePanel`.
pub const REPORT_ABUSE_ACTION: u32 = 0x1000_0003;

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

    /// Lay the page out as both interfaces' pages are: In-Game Help is hidden, since no help
    /// ships, and the two support buttons close up over its place and open the in-game forms
    /// (Urgent Assistance, Report Abuse), each through its form's input action, as Configure
    /// Keyboard opens the key page. Returns whether the page was there to lay out.
    pub fn arrange(&self, ui: &mut UiSystem) -> bool {
        let Some(page) = self.page else { return false };
        let Some(help) = ui.get_child_recursive(page, button::HELP) else {
            return false;
        };
        ui.set_visible(help, false);
        ui.set_mouse_visible(help, false);
        let rise = BUTTONS
            .iter()
            .find(|(b, _)| *b == button::SUPPORT_TICKET_UPPER)
            .map_or(0, |(_, y)| *y)
            - BUTTONS
                .iter()
                .find(|(b, _)| *b == button::HELP)
                .map_or(0, |(_, y)| *y);
        for (id, _) in BUTTONS {
            let action = match gameplay_option_action(id) {
                Some(Act::UrgentAssistance) => URGENT_ASSISTANCE_ACTION,
                Some(Act::ReportAbuse) => REPORT_ABUSE_ACTION,
                _ => continue,
            };
            let Some(h) = ui.get_child_recursive(page, id) else {
                continue;
            };
            if let Some(n) = ui.node_mut(h) {
                n.instance_properties.set(
                    dereth_ui::props::attr::BUTTON_INPUT_ACTION,
                    dereth_assets::ui::PropertyValue::Enum(action),
                );
            }
            let at = ui.node(h).map(|n| (n.region.box_.x0, n.region.box_.y0));
            if let Some((x, y)) = at {
                ui.move_to(h, x, y - rise + 10);
            }
        }
        true
    }

    /// Answer a press under this bound page, except actions already delivered by the button.
    #[must_use]
    pub fn on_element_message(
        &self,
        ui: &UiSystem,
        m: &dereth_ui::msg::ElementMessage,
    ) -> Option<Act> {
        if m.id != MessageId(1) {
            return None;
        }
        let page = self.page?;
        if !is_under(ui, m.source, page) {
            return None;
        }
        manual_action(m.source_id)
    }
}

fn manual_action(id: ElementId) -> Option<Act> {
    gameplay_option_action(id).filter(|action| {
        !matches!(
            action,
            Act::ConfigureKeyboard | Act::UrgentAssistance | Act::ReportAbuse
        )
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The three ids the handler answers, against the transcription in `pages.rs`.
    #[test]
    fn the_three_answered_ids_are_the_ones_the_handler_switches_on() {
        assert_eq!(
            manual_action(button::EXIT_TO_CHARACTER_SELECTION),
            Some(Act::ExitToCharacterSelection)
        );
        assert_eq!(manual_action(button::EXIT_GAME), Some(Act::ExitGame));
        assert_eq!(
            manual_action(button::MOUSE_TURNING_SETTINGS),
            Some(Act::MouseTurningSettings)
        );
        for b in [button::SUPPORT_TICKET_UPPER, button::SUPPORT_TICKET_LOWER] {
            assert_eq!(manual_action(b), None, "the forms' actions open them");
        }
        // And the two the class does not answer.
        for (b, _) in LAYOUT_DRIVEN {
            assert_eq!(manual_action(b), None, "{:#010X} has no class arm", b.0);
        }
    }

    /// The switch subtraction establishes `0x100005CC + 0x4B == 0x10000617`.
    #[test]
    fn the_two_broadcast_ids_are_the_pair_the_switch_subtracts() {
        assert_eq!(button::MOUSE_TURNING_SETTINGS.0 + 0x4B, button::EXIT_GAME.0);
        assert_eq!(
            dereth_ui::msg::global::MOUSE_TURNING_DEFAULTS,
            MessageId(0x0C)
        );
    }
}
