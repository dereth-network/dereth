//! The chat panel's side of the client: the chat's part of the player's options taken from the
//! game's view for the windows (when the player is first here, and again when the player's
//! scripts say the options changed), and what the panel asks of the client as the client's actions: a line typed carried
//! out by the player's command line, the lines the windows step through (each answered in a later
//! frame, in order, to the window that asked), a window's opacity, the chat options set.

use std::collections::VecDeque;

use auberean_client_sdk::contract::view::{GameOptions, View};
use auberean_client_sdk::contract::{ChatHistory, ChatWindowAction};
use auberean_client_sdk::Action;
use auberean_ui_game::chat::{self, Ask, ChatSettings, History, TEXT_TYPES};
use auberean_ui_game::gameplay::{self, Notice};

/// What the chat panel reads of the client and asks of it.
#[derive(Debug, Default)]
pub struct ChatClient {
    settings: Option<ChatSettings>,
    revision: u64,
    /// The options changed: read them again before the next frame.
    stale: bool,
    /// The lists asked of and not yet answered, oldest first: the window that asked, the list.
    asked: VecDeque<(usize, History)>,
}

/// A list the windows step through, as the client's actions name it.
const fn list(h: History) -> ChatHistory {
    match h {
        History::Commands => ChatHistory::Commands,
        History::Retells => ChatHistory::Retells,
        History::Replies => ChatHistory::Replies,
    }
}

impl ChatClient {
    /// The chat's options as last read.
    #[must_use]
    pub const fn settings(&self) -> Option<&ChatSettings> {
        self.settings.as_ref()
    }

    /// The notices of a frame: the chat's options or the player's options changed.
    pub fn heard(&mut self, notices: &[Notice]) {
        if notices.iter().any(|n| {
            n.native == chat::notice::RELOAD_CHAT_SETTINGS
                || n.native == gameplay::notice::RELOAD_OPTIONS
        }) {
            self.stale = true;
        }
    }

    /// Something the panel asked, as the client's action (none for what the panel's side keeps
    /// to itself: the options read again).
    pub fn action(&mut self, a: Ask) -> Option<Action> {
        use ChatWindowAction as W;
        Some(match a {
            Ask::Text(text) => Action::Say { text },
            Ask::History {
                list: l,
                earlier,
                window,
            } => {
                self.asked.push_back((window, l));
                Action::AskHistory {
                    list: list(l),
                    earlier,
                }
            }
            Ask::Opacity { window, opacity } => Action::ChatWindows(W::Opacity {
                window: window.raw(),
                opacity,
            }),
            Ask::Filter { window, filter } => Action::ChatWindows(W::Filter { window, filter }),
            Ask::FontSize { kind, size } => Action::ChatWindows(W::FontSize { kind, size }),
            Ask::FontColor { kind, color } => Action::ChatWindows(W::FontColor { kind, color }),
            Ask::Popup { kind, on } => Action::ChatWindows(W::Popup { kind, on }),
            Ask::Devote { window, kind } => Action::ChatWindows(W::Devote { window, kind }),
            Ask::KeepChatOptions => Action::ChatWindows(W::Keep),
            Ask::ReloadChatSettings => {
                self.stale = true;
                return None;
            }
        })
    }

    /// The client answered a list asked of: the window that asked it, and the list, when one
    /// waits for it.
    pub fn answered(&mut self, l: ChatHistory) -> Option<(usize, History)> {
        let at = self.asked.iter().position(|(_, h)| list(*h) == l)?;
        self.asked.remove(at)
    }

    /// Before a frame: the options taken from the view where they changed (or were never
    /// taken).
    pub fn take(&mut self, view: &View) {
        if view.player.is_none() {
            return;
        }
        if self.stale || self.settings.is_none() {
            if let Some(o) = &view.options {
                self.revision += 1;
                self.settings = Some(read(o, self.revision));
                self.stale = false;
            }
        }
    }
}

/// The chat's part of a new player's options (the chat page's defaults).
#[must_use]
pub fn defaults() -> ChatSettings {
    read(&auberean_client_sdk::runtime::view::default_options(), 0)
}

/// The chat's part of a profile, for the windows.
fn read(o: &GameOptions, revision: u64) -> ChatSettings {
    let c = &o.chat;
    ChatSettings {
        revision,
        filters: c.filters,
        devoted: c.devoted,
        fonts: TEXT_TYPES
            .iter()
            .map(|t| {
                let (size, color, popup) = c.fonts.get(&t.kind).copied().unwrap_or_default();
                (t.kind, size, color, popup)
            })
            .collect(),
        opacities: chat::CHAT_WINDOWS
            .iter()
            .filter_map(|w| c.opacities.get(&w.raw()).map(|v| (*w, *v)))
            .collect(),
    }
}
