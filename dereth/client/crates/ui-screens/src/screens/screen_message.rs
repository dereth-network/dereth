//! The character screen's message window: what a world says to the players choosing a character
//! (`0xF65A Login_CharacterScreenMessage`), shown over the right of the character screen in the
//! interface's own floating chat window, with its scrollbar.
//!
//! The end-of-retail client ignored the message; the clients before it showed it in the character
//! screen's message box. The window is the gameplay layout's first floating chat window, built
//! as a child of the character screen: its frame, title bar, close button and scrolling text are
//! the layout's own. Its input line, menu and send button are hidden, because nothing is typed
//! here; the close button hides the window.

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

/// The children of a chat window that take typed input, hidden here: the input line, the chat
/// menu and the send button.
pub const INPUT_CHILDREN: [ElementId; 3] = [
    ElementId(0x1000_0016),
    ElementId(0x1000_0014),
    ElementId(0x1000_0019),
];

/// Where the window stands on the 800x600 character screen: over the right of it, on the
/// picture, clear of the buttons along the bottom.
pub const PLACE: (i32, i32, i32, i32) = (455, 150, 330, 380);

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
    /// Show `text` in the window under `root`, titled `title`, building the window the first time.
    /// Returns whether anything changed.
    pub fn show(
        &mut self,
        ui: &mut UiSystem,
        root: ElemHandle,
        text: &str,
        title: Option<&str>,
    ) -> bool {
        if self.closed || self.shown.as_deref() == Some(text) {
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
            ui.set_visible(w, true);
            self.window = Some(w);
        }
        let Some(w) = self.window else { return false };
        if let (Some(title), Some(h)) = (title, ui.get_child_recursive(w, TITLE)) {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(title);
            }
        }
        if let Some(h) = ui.get_child_recursive(w, TEXT) {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(text);
            }
        }
        self.shown = Some(text.to_owned());
        true
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
