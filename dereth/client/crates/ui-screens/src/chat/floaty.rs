//! `FloatingChat` (`0x10000040`) — the four floating chat windows' own half.
//!
//! `ChatInterface` is the shared base and lives in [`super::interface`]; this module is what
//! `FloatingChat` adds on top of it: the title bar, the close button, and — the one a player would
//! notice across a relog — writing the window's placement back, not only reading it out of the
//! login blob.
//!
//! # The write-backs
//!
//! The placement row for window *n* in the player module's chat-option structure has six
//! properties. Four window events write to it: each first performs the
//! base move, resize, visibility, or title operation, then rejects window id zero, names the
//! changed property, and stores it in that window's chat options:
//!
//! | event | writes |
//! |---|---|
//! | move to (x, y) | `0x10000086` X, `0x10000087` Y |
//! | resize to (w, h) | `0x10000088` Width, `0x10000089` Height |
//! | show / hide | `0x1000008A` Visibility |
//! | set window title | `0x1000008D` Title |
//!
//! **Setting a chat-window option writes the local player options and sends nothing.** The row reaches the
//! server with the rest of the options blob, not once per drag — which is why these are
//! [`UiRequest::SetChatWindowOption`] / [`UiRequest::SetChatWindowTitle`] and not a `Request`.
//!
//! The guard is not decoration: the window id is attribute `0x1000007E` and is **0** for a window
//! whose layout does not carry it, and the chat-option structure's array is indexed by
//! `windowID − 1`. Writing with id 0 would write row −1.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::view::{ChatWindowTitle, UiRequest};

/// The six `Option_Placement` members, by property name.
///
/// [`crate::hud::floaty::WindowPlacement`] is the read side of the same six; these are the four
/// ids `FloatingChat` writes plus the two the move writes, so a reader and a writer of the same
/// row cannot drift apart by naming them separately.
pub mod placement {
    /// `Option_Placement_X`.
    pub const X: u32 = 0x1000_0086;
    /// `Option_Placement_Y`.
    pub const Y: u32 = 0x1000_0087;
    /// `Option_Placement_Width`.
    pub const WIDTH: u32 = 0x1000_0088;
    /// `Option_Placement_Height`.
    pub const HEIGHT: u32 = 0x1000_0089;
    /// `Option_Placement_Visibility`.
    pub const VISIBILITY: u32 = 0x1000_008A;
    /// `Option_Placement_Title` — the complete literal-or-table `StringInfo`.
    pub const TITLE: u32 = 0x1000_008D;
}

/// The title text child the window-title write updates, found recursively as a text element.
pub const TITLE_TEXT: ElementId = ElementId(0x1000_04D9);

/// The close button. Its ordinary click is the element-message handler's only arm and hides the
/// window.
pub const CLOSE_BUTTON: ElementId = ElementId(0x1000_052A);

/// One `FloatingChat`: its window id, its two named children and the last placement it wrote.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FloatyChatUI {
    /// The window's own root element, the one the screen-layout tag names.
    pub root: Option<ElemHandle>,
    /// The window id — attribute `0x1000007E`. **Zero suppresses every write-back.**
    pub window_id: u32,
    /// `0x100004D9`.
    pub title_text: Option<ElemHandle>,
    /// `0x1000052A`.
    pub close_button: Option<ElemHandle>,
    /// The full title last written, so literal overrides and table references remain distinct.
    pub title: Option<ChatWindowTitle>,
}

impl FloatyChatUI {
    /// Bind the two children and read the window id off the live element.
    #[must_use]
    pub fn post_init(ui: &mut UiSystem, root: ElemHandle, window_id: u32) -> Self {
        Self {
            root: Some(root),
            window_id,
            title_text: ui.get_child_recursive(root, TITLE_TEXT),
            close_button: ui.get_child_recursive(root, CLOSE_BUTTON),
            title: None,
        }
    }

    /// The three lines every write-back shares.
    fn write(&self, requests_out: &mut crate::requests::Outbox, property: u32, value: i32) {
        if self.window_id == 0 {
            return;
        }
        requests_out.emit(UiRequest::SetChatWindowOption {
            window: self.window_id,
            property,
            value,
        });
    }

    /// Move the window, then write the new X and Y back to the placement row.
    pub fn move_to(&self, ui: &mut UiSystem, x: i32, y: i32) {
        if let Some(root) = self.root {
            ui.move_to(root, x, y);
        }
        self.write(&mut ui.requests, placement::X, x);
        self.write(&mut ui.requests, placement::Y, y);
    }

    /// Resize the window — the chat interface's own resize first
    /// (which re-lays the log and the entry), then the two writes.
    pub fn resize_to(&self, ui: &mut UiSystem, w: i32, h: i32) {
        if let Some(root) = self.root {
            ui.resize_to(root, w, h);
        }
        self.write(&mut ui.requests, placement::WIDTH, w);
        self.write(&mut ui.requests, placement::HEIGHT, h);
    }

    /// Show or hide the window — the override every one of the five chat
    /// windows inherits, and the reason `Option_Placement_Visibility` is **server state** rather
    /// than layout data ([`crate::hud::floaty::READS_PLACEMENT_VISIBILITY`]).
    pub fn set_visible(&self, ui: &mut UiSystem, visible: bool) {
        if let Some(root) = self.root {
            ui.set_visible(root, visible);
        }
        self.write(&mut ui.requests, placement::VISIBILITY, i32::from(visible));
    }

    /// Set the window title: find the title text `0x100004D9` and, if present, give it the new
    /// caption; then, if the window id is non-zero and a player exists, write property
    /// `0x1000008D` to that window's chat options; return whether the title text was found.
    ///
    /// Note the **order and the independence**: the property is written even when the element is
    /// missing, and the return value reports only whether the caption was drawn.
    ///
    /// The caller supplies the same typed `StringInfo` shape the native method receives. Literal
    /// `@title` text and authored table references must not collapse into the same integer hash.
    pub fn set_window_title_info(&mut self, ui: &mut UiSystem, title: ChatWindowTitle) -> bool {
        let drawn = self.draw_window_title(ui, &title);
        if self.window_id != 0 {
            ui.requests.emit(UiRequest::SetChatWindowTitle {
                window: self.window_id,
                title,
            });
        }
        drawn
    }

    /// Draw the title half of retail's set-window-title; the caller owns its PlayerModule writeback.
    fn draw_window_title(&mut self, ui: &mut UiSystem, title: &ChatWindowTitle) -> bool {
        let text = match title {
            ChatWindowTitle::Literal(text) => Some(text.clone()),
            ChatWindowTitle::Table {
                string_id,
                table_id,
            } => ui.resolve_string(dereth_primitives::DataId(*table_id), *string_id),
        };
        let drawn = match (self.title_text, text) {
            (Some(h), Some(text)) => ui.text_element_mut(h).is_some_and(|t| {
                t.set_text(&text);
                true
            }),
            _ => false,
        };
        self.title = Some(title.clone());
        drawn
    }

    /// Existing authored-token producer. Unlike the literal title path, this is a table
    /// reference rather than a literal override, so both StringInfo ids are retained in the PlayerModule.
    pub fn set_window_title(&mut self, ui: &mut UiSystem, token: &str) -> bool {
        let text = super::mainchat::label(ui, token);
        let drawn = match self.title_text.and_then(|h| ui.text_element_mut(h)) {
            Some(t) => {
                t.set_text(&text);
                true
            }
            None => false,
        };
        let title = ChatWindowTitle::Table {
            string_id: dereth_primitives::num::hash::str_hash(token.as_bytes()),
            table_id: super::mainchat::CAPTION_STRING_TABLE.0,
        };
        self.title = Some(title.clone());
        if self.window_id != 0 {
            ui.requests.emit(UiRequest::SetChatWindowTitle {
                window: self.window_id,
                title,
            });
        }
        drawn
    }

    /// The set-chat-window-title notice `(window_id, title)` — if `window_id` is this window's
    /// id, set the window title; nothing else.
    ///
    /// The notice is the set-chat-window-title notice, notice id the client
    /// ([`dereth_ui::msg::notice`]), and it is broadcast to all five windows: the id test is what
    /// makes it reach one.
    pub fn on_set_chat_window_title(
        &mut self,
        ui: &mut UiSystem,
        window_id: u32,
        title: ChatWindowTitle,
    ) -> bool {
        if window_id != self.window_id {
            return false;
        }
        self.set_window_title_info(ui, title);
        true
    }

    /// The element-message handler — message **1** on `0x1000052A` is
    /// hiding the window, which then writes `Option_Placement_Visibility` through the
    /// override above. That is the whole of the close button: the window is hidden, not destroyed,
    /// and the shard remembers it was.
    ///
    /// Returns whether this window's close button was the source.
    pub fn listen_to_element_message(
        &self,
        ui: &mut UiSystem,
        source: ElemHandle,
        id: dereth_ui::MessageId,
    ) -> bool {
        if id != dereth_ui::msg::element::id::BUTTON_CLICKED || self.close_button != Some(source) {
            return false;
        }
        self.set_visible(ui, false);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the four overrides, each of
    /// which names its property by a literal property id.
    #[test]
    fn the_six_placement_property_ids_are_the_documented_ones() {
        assert_eq!(placement::X, 0x1000_0086);
        assert_eq!(placement::Y, 0x1000_0087);
        assert_eq!(placement::WIDTH, 0x1000_0088);
        assert_eq!(placement::HEIGHT, 0x1000_0089);
        assert_eq!(placement::VISIBILITY, 0x1000_008A);
        assert_eq!(placement::TITLE, 0x1000_008D);
        // The read side names the same six.
        assert_eq!(placement::X, crate::screens::gameplay::placement::X);
        assert_eq!(placement::Y, crate::screens::gameplay::placement::Y);
    }

    /// Oracle: the non-zero window id check that opens all four write-backs. The chat-option
    /// structure is indexed by `windowID − 1`, so id 0 has no row to write.
    #[test]
    fn a_window_with_no_id_writes_nothing_back() {
        let mut ui = UiSystem::new((800, 600));
        let w = FloatyChatUI {
            window_id: 0,
            ..FloatyChatUI::default()
        };
        w.set_visible(&mut ui, false);
        w.move_to(&mut ui, 10, 20);
        w.resize_to(&mut ui, 30, 40);
        assert_eq!(
            ui.requests.take(),
            Vec::new(),
            "id 0 is row -1; nothing is written"
        );

        let w = FloatyChatUI {
            window_id: 3,
            ..FloatyChatUI::default()
        };
        w.move_to(&mut ui, 10, 20);
        assert_eq!(
            ui.requests.take(),
            vec![
                UiRequest::SetChatWindowOption {
                    window: 3,
                    property: placement::X,
                    value: 10
                },
                UiRequest::SetChatWindowOption {
                    window: 3,
                    property: placement::Y,
                    value: 20
                },
            ]
        );
    }

    /// Oracle: setting the window title — the property write is **outside** the found-element
    /// check that draws the caption, and the return value reports only the drawing.
    #[test]
    fn the_title_write_back_happens_even_when_the_caption_element_is_missing() {
        let mut ui = UiSystem::new((800, 600));
        let mut w = FloatyChatUI {
            window_id: 2,
            ..FloatyChatUI::default()
        };
        assert!(
            !w.set_window_title(&mut ui, "ID_Chat_Chat1_DefaultTitle"),
            "no element to draw in"
        );
        let id = dereth_primitives::num::hash::str_hash(b"ID_Chat_Chat1_DefaultTitle");
        let title = ChatWindowTitle::Table {
            string_id: id,
            table_id: super::super::mainchat::CAPTION_STRING_TABLE.0,
        };
        assert_eq!(w.title, Some(title.clone()));
        assert_eq!(
            ui.requests.take(),
            vec![UiRequest::SetChatWindowTitle { window: 2, title }]
        );
    }

    /// Oracle: the set-chat-window-title notice sets the title only when the notice's window id is
    /// this window's.
    #[test]
    fn the_title_notice_reaches_exactly_the_window_it_names() {
        let mut ui = UiSystem::new((800, 600));
        let mut w = FloatyChatUI {
            window_id: 4,
            ..FloatyChatUI::default()
        };
        assert!(!w.on_set_chat_window_title(&mut ui, 2, ChatWindowTitle::Literal("first".into()),));
        assert_eq!(w.title, None);
        assert!(ui.requests.take().is_empty());
        assert!(w.on_set_chat_window_title(&mut ui, 4, ChatWindowTitle::Literal("third".into()),));
        assert_eq!(w.title, Some(ChatWindowTitle::Literal("third".into())));
        assert_eq!(ui.requests.len(), 1);
        ui.requests.clear();
    }
}
