//! The character screen's message window: what a world says to the players choosing a character
//! (`0xF65A Login_CharacterScreenMessage`), shown over the right of the character screen in the
//! interface's own floating chat window, with its scrollbar.
//!
//! The end-of-retail client ignored the message; the clients before it showed it in the character
//! screen's message box. The window is the gameplay layout's first floating chat window, built
//! as a child of the character screen: its frame, title bar, close button and scrolling text are
//! the layout's own.
//!
//! - It stands right of the Create Character button, from the button's top edge to the bottom of
//!   the characters frame, and is titled [`TITLE_TEXT`].
//! - Its input row (the input line, menu and send button) is hidden and the text and its scrollbar
//!   grow down over the space, because nothing is typed here.
//! - The mouse wheel scrolls the text: the character screen registers the scrollable controls.
//! - A world's text may end its lines with carriage returns; they are line breaks.
//! - The close button hides the window.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

/// The gameplay screen's layout enum, which the floating chat windows are elements of.
const GAMEPLAY_LAYOUT: dereth_ui::framework::LayoutEnum =
    dereth_ui::framework::LayoutEnum(0x1000_0006);

/// The first floating chat window's root.
pub const WINDOW: ElementId = ElementId(0x1000_0505);

/// The window's scrolling text: the chat log, which names its own scrollbar.
pub const TEXT: ElementId = ElementId(0x1000_0011);

/// The window's close button.
pub const CLOSE_BUTTON: ElementId = crate::chat::floaty::CLOSE_BUTTON;

/// The window's title text.
pub const TITLE: ElementId = crate::chat::floaty::TITLE_TEXT;

/// The window's title.
pub const TITLE_TEXT: &str = "Announcements";

/// The children of a chat window that take typed input, hidden here: the input line, the chat
/// menu and the send button.
pub const INPUT_CHILDREN: [ElementId; 3] = [
    ElementId(0x1000_0016),
    ElementId(0x1000_0014),
    ElementId(0x1000_0019),
];

/// The row along the window's bottom that holds the input children. Hidden, and the text's frame
/// and the scrollbar grow down over it.
pub const INPUT_ROW: ElementId = ElementId(0x1000_0509);

/// The text's frame, the parent of [`TEXT`].
pub const TEXT_FRAME: ElementId = ElementId(0x1000_0010);

/// The text's scrollbar.
pub const SCROLLBAR: ElementId = ElementId(0x1000_0012);

/// Where the window stands on the 800x600 character screen: right of the Create Character
/// button, from the button's top edge to the bottom of the characters frame.
pub const PLACE: (i32, i32, i32, i32) = (455, 211, 330, 328);

/// The text as the window shows it: a carriage return, alone or before a line feed, is one line
/// break.
#[must_use]
pub fn shown_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// The window and the text it shows.
#[derive(Debug, Default)]
pub struct ScreenMessageWindow {
    /// The window's root, once built.
    pub window: Option<ElemHandle>,
    /// The text last put in it.
    pub shown: Option<String>,
    /// Whether the player closed it: it stays closed for this screen.
    pub closed: bool,
}

impl ScreenMessageWindow {
    /// Show `text` in the window under `root`, building the window the first time. Returns whether
    /// anything changed.
    pub fn show(&mut self, ui: &mut UiSystem, root: ElemHandle, text: &str) -> bool {
        let text = shown_text(text);
        if self.closed || self.shown.as_deref() == Some(text.as_str()) {
            return false;
        }
        if self.window.is_none() {
            let built = ui.require_env().and_then(|e| {
                e.create_nested_child_element_by_enum(ui, root, GAMEPLAY_LAYOUT, WINDOW)
            });
            let w = match built {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!("the character screen's message window was not built: {e}");
                    self.closed = true;
                    return false;
                }
            };
            for id in INPUT_CHILDREN {
                if let Some(h) = ui.get_child_recursive(w, id) {
                    ui.set_visible(h, false);
                    ui.set_mouse_visible(h, false);
                }
            }
            let (x, y, width, height) = PLACE;
            ui.move_to(w, x, y);
            ui.resize_to(w, width, height);
            Self::close_up_input_row(ui, w);
            if let Some(t) = ui
                .get_child_recursive(w, TITLE)
                .and_then(|h| ui.text_element_mut(h))
            {
                t.set_text(TITLE_TEXT);
            }
            ui.set_visible(w, true);
            self.window = Some(w);
        }
        let Some(w) = self.window else { return false };
        if let Some(h) = ui.get_child_recursive(w, TEXT) {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&text);
            }
        }
        self.shown = Some(text);
        true
    }

    /// Hide the input row and grow the text's frame and the scrollbar down over its height.
    fn close_up_input_row(ui: &mut UiSystem, w: ElemHandle) {
        let Some(row) = ui.get_child_recursive(w, INPUT_ROW) else {
            return;
        };
        let Some(gap) = ui.node(row).map(|n| n.region.box_.height()) else {
            return;
        };
        ui.set_visible(row, false);
        ui.set_mouse_visible(row, false);
        for id in [TEXT_FRAME, SCROLLBAR] {
            let Some(h) = ui.get_child_recursive(w, id) else {
                continue;
            };
            let Some(b) = ui.node(h).map(|n| n.region.box_) else {
                continue;
            };
            ui.resize_to(h, b.width(), b.height() + gap);
        }
    }

    /// The close button's click: hide the window for the rest of this screen. Returns whether the
    /// message was this window's.
    pub fn on_close(&mut self, ui: &mut UiSystem, source: ElementId) -> bool {
        if source != CLOSE_BUTTON {
            return false;
        }
        let Some(w) = self.window else { return false };
        ui.set_visible(w, false);
        self.closed = true;
        true
    }
}
