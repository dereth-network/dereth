//! `UrgentAssistancePanel` — the three-page *urgent assistance* request window.
//!
//! # It is outbound, not inbound
//!
//! No inbound message opens this window. It is **outbound**: it is how the *player* asks for urgent
//! help, and its Send button is the client's only producer of a broadcast on the **Help** channel.
//!
//! * It opens from a shipped **input action**, not from the wire. The page element `0x10000189`
//!   carries `dereth_ui::props::attr::INPUT_ACTION` (`0x57`) = `0x1000000B`, which
//!   the action name table names `ToggleUrgentAssistancePanel`, and its close button
//!   `0x100000FC` carries the same action in `BUTTON_INPUT_ACTION` (`0x12`). `0x10000189` is also
//!   `catalogue::PANEL_PAGES[11]`, so it is a toolbar panel page like every other.
//! * **The server answers it**: a Help-channel (`0x400`) chat broadcast is relayed to the
//!   channel like any other.
//! * The Help channel has **no other producer in the client**: the channel-command handler
//!   excludes `0x400` by value, which is what makes `@help` a help command instead of a
//!   channel. `dereth_client_model::chat::
//!   channel_command_broadcasts_on` already carries that exclusion. This window is the way in.
//!
//! # The post-init
//!
//! Two children, and they are the two `panels/catalogue.rs`'s `URGENT` table already had: the
//! entry box `0x100001BA` and the continue (*Send*) button `0x100001BD`.
//!
//! # The element-message handler
//!
//! Three message ids reach it — `1`, `0x12` (character typed) and `0x44` (text changed). `1`
//! goes to a switch on the element id over `0x100001B6`…`0x100001C0`; `0x12` and `0x44` go
//! straight to the entry-box tail. The eleven ids map like this:
//!
//! | element | arm |
//! |---|---|
//! | `0x100001B6`, `0x100001C0` | close and reset |
//! | `0x100001B7` | set state `0x1000000F` (page two) |
//! | `0x100001BD` | send |
//! | `0x100001B8`–`0x100001BC`, `0x100001BE`, `0x100001BF` | nothing but the tail |
//!
//! * **Close / reset** (`0x100001B6` Cancel on either page, `0x100001C0` Done): empty the entry
//!   box, disable Send (state `0x0D`), return to page one (`0x1000000E`).
//! * **`0x100001B7`**: go to page two.
//! * **Send** (`0x100001BD`): an empty entry box does nothing at all; otherwise broadcast the text
//!   on channel `0x400` (Help) and go to page three (`0x10000010`).
//! * **The tail**, for every message id: when the message is from the entry box, disable Send if
//!   the box is empty and enable it otherwise.
//!
//! Note the empty test: the client's string length **includes the terminator**, so a length of
//! 1 is the empty string. Both places that test it are transcribed, not one.
//!
//! # The shipped window, measured
//!
//! Built `0x21000005` (1,870 elements, **one** element of type `0x1000001F`). Page
//! `0x10000189` under panel container `0x10000180`, three page containers whose
//! `dereth_ui::props::attr::HIDE` (`0x3B`) is set per **state**, which is what makes the state write the
//! whole of the page switching:
//!
//! | container | `0x1000000E` | `0x1000000F` | `0x10000010` | holds |
//! |---|---|---|---|---|
//! | `0x100001B4` | shown | hidden | hidden | the warning text `0x100001B5`, *Cancel* `0x100001B6`, *Continue* `0x100001B7` |
//! | `0x100001B8` | hidden | shown | hidden | the prompt `0x100001B9`, the entry box `0x100001BA`, its scrollbar `0x100001BB`, the note `0x100001BC`, *Cancel* `0x100001B6` again, *Send* `0x100001BD` |
//! | `0x100001BE` | hidden | hidden | shown | the confirmation `0x100001BF` and *Done* `0x100001C0` |
//!
//! `0x100001B6` really is in the tree **twice**, once per page, which is why the arms key on
//! `source_id` and not on a bound handle.

use dereth_ui::{ElemHandle, ElementId, ElementType, StateId, UiSystem};

use crate::view::UiRequest;

/// The local behavior label, so `panels/catalogue.rs`'s row has a module that names it.
pub const CLASS: &str = "UrgentAssistancePanel";

/// The urgent-assistance panel's registered element type.
pub const PANEL_TYPE: ElementType = ElementType(0x1000_001F);

/// The shipped page element. It is also `catalogue::PANEL_PAGES[11]`.
pub const PANEL: ElementId = ElementId(0x1000_0189);

/// Attribute `0x57` on [`PANEL`] and attribute `0x12` on [`CLOSE_BUTTON`] —
/// `ToggleUrgentAssistancePanel`. This is the window's shipped trigger; there is no inbound
/// message that opens it.
pub const TOGGLE_ACTION: u32 = 0x1000_000B;

/// The entry box, the post-init's first child lookup.
pub const ENTRY_BOX: ElementId = ElementId(0x1000_01BA);
/// The continue button, the second — the *Send* button on page two.
pub const CONTINUE_BUTTON: ElementId = ElementId(0x1000_01BD);
/// The title bar's close button. Layout-driven: it carries [`TOGGLE_ACTION`] in attribute `0x12`.
pub const CLOSE_BUTTON: ElementId = ElementId(0x1000_00FC);
/// *Cancel*. In the shipped tree **twice**, once on page one and once on page two.
pub const CANCEL_BUTTON: ElementId = ElementId(0x1000_01B6);
/// Page one's *Continue*, which only ever switches to page two.
pub const NEXT_BUTTON: ElementId = ElementId(0x1000_01B7);
/// Page three's *Done*, which shares `0x100001B6`'s arm.
pub const DONE_BUTTON: ElementId = ElementId(0x1000_01C0);

/// Page one — the warning. The state `Reset` returns to.
pub const PAGE_WARNING: StateId = StateId(0x1000_000E);
/// Page two — the entry form.
pub const PAGE_FORM: StateId = StateId(0x1000_000F);
/// Page three — the sent confirmation.
pub const PAGE_SENT: StateId = StateId(0x1000_0010);

/// The three page containers, in state order. Their `HIDE` flag is what the state switches.
pub const PAGE_CONTAINERS: [(StateId, ElementId); 3] = [
    (PAGE_WARNING, ElementId(0x1000_01B4)),
    (PAGE_FORM, ElementId(0x1000_01B8)),
    (PAGE_SENT, ElementId(0x1000_01BE)),
];

/// `0x400` — `Channel::Help`. `dereth_client_model::chat::get_channel_id("help")` is the
/// same number from the other direction, and `channel_command_broadcasts_on` excludes it because
/// **this** window owns it.
pub const HELP_CHANNEL: u32 = 0x0000_0400;

/// The bound window.
#[derive(Debug, Default)]
pub struct UrgentAssistancePanel {
    pub panel: Option<ElemHandle>,
    entry_box: Option<ElemHandle>,
    continue_button: Option<ElemHandle>,
    /// How many `0x0147` broadcasts the Send button has raised. A counter a station can assert on.
    pub requests_sent: u32,
}

impl UrgentAssistancePanel {
    /// The two child lookups.
    ///
    /// The panel itself is found by **type**, as `panels/abuse.rs` finds its sibling: the window
    /// is the one element of type `0x1000001F` in the tree, and finding it by id alone would tie
    /// the bind to a layout id the client never names.
    pub fn post_init(&mut self, ui: &UiSystem, root: ElemHandle) {
        self.requests_sent = 0;
        self.panel = find_panel(ui, root);
        self.entry_box = self
            .panel
            .and_then(|p| ui.get_child_recursive(p, ENTRY_BOX));
        self.continue_button = self
            .panel
            .and_then(|p| ui.get_child_recursive(p, CONTINUE_BUTTON));
    }

    #[must_use]
    pub fn bound(&self) -> bool {
        self.panel.is_some() && self.entry_box.is_some() && self.continue_button.is_some()
    }

    /// The urgent assistance panel's element-message handler, arm for arm.
    ///
    /// Returns true when this window consumed the message.
    pub fn on_element_message(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        use dereth_ui::msg::element::id as msg;
        if !self.bound() {
            return false;
        }
        // The tail: any of the three message ids, keyed on the **entry box**.
        if m.id == msg::CHARACTER || m.id == msg::TEXT_CHANGED {
            if Some(m.source) != self.entry_box && m.source_id != ENTRY_BOX {
                return false;
            }
            self.update_continue(ui);
            return true;
        }
        if m.id != msg::BUTTON_CLICKED {
            return false;
        }
        match m.source_id {
            // The client — both Cancels and the Done button.
            CANCEL_BUTTON | DONE_BUTTON => {
                self.reset(ui);
                true
            }
            // The client — page one's Continue.
            NEXT_BUTTON => {
                if let Some(p) = self.panel {
                    ui.set_state(p, PAGE_FORM);
                }
                true
            }
            // The client — Send.
            CONTINUE_BUTTON => {
                self.send(ui);
                true
            }
            // The entry box also raises message 1; the tail owns it.
            ENTRY_BOX => {
                self.update_continue(ui);
                true
            }
            _ => false,
        }
    }

    /// The client arm: hide the window, empty the box, disable Send, and return to page one.
    ///
    /// The order is the client's, and it matters for the **next** open: the window is reset
    /// *before* it is next shown, not when it is shown, so a cancelled report is not still on
    /// screen the second time the player presses the toolbar button.
    pub fn reset(&mut self, ui: &mut UiSystem) {
        let Some(p) = self.panel else { return };
        ui.set_visible(p, false);
        if let Some(t) = self.entry_box.and_then(|h| ui.text_element_mut(h)) {
            t.set_text("");
        }
        set_enabled(ui, self.continue_button, false);
        ui.set_state(p, PAGE_WARNING);
    }

    /// The tail: Send follows the box's emptiness, one way and the other.
    fn update_continue(&self, ui: &mut UiSystem) {
        let on = self
            .entry_box
            .is_some_and(|h| !entry_text(ui, h).is_empty());
        set_enabled(ui, self.continue_button, on);
    }

    /// The client arm: an empty box does nothing at all — no broadcast and **no page change**
    /// (the client's empty test skips both).
    fn send(&mut self, ui: &mut UiSystem) {
        let Some(box_h) = self.entry_box else { return };
        let text = entry_text(ui, box_h);
        if text.is_empty() {
            return;
        }
        ui.requests.emit(UiRequest::ChannelBroadcast {
            channel: HELP_CHANNEL,
            text,
        });
        self.requests_sent += 1;
        if let Some(p) = self.panel {
            ui.set_state(p, PAGE_SENT);
        }
    }
}

fn entry_text(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

fn set_enabled(ui: &mut UiSystem, h: Option<ElemHandle>, on: bool) {
    if let Some(h) = h {
        ui.set_state(
            h,
            if on {
                dereth_ui::widgets::button::state::NORMAL
            } else {
                dereth_ui::widgets::button::state::DISABLED
            },
        );
    }
}

fn find_panel(ui: &UiSystem, h: ElemHandle) -> Option<ElemHandle> {
    if ui.node(h).is_some_and(|n| n.ty() == PANEL_TYPE) {
        return Some(h);
    }
    ui.children(h).into_iter().find_map(|c| find_panel(ui, c))
}
