//! The chat panel's side of the client: the chat's part of the player's options read for the
//! windows (when the player is first here, and again when the player's scripts say the options
//! changed), and what the panel asks of the client made before the next frame: a line typed
//! carried out by the player's command line (a command no one knows said as an error line), the
//! player's commands made known, the lines the windows step through, a window's opacity, the chat
//! options set.

use auberean_client_sdk::runtime::chat_options as options;
use auberean_client_sdk::runtime::interface::function;
use auberean_client_sdk::runtime::shell::ShellStatus;
use auberean_client_sdk::runtime::InterfaceArg;
use auberean_client_sdk::Client;
use auberean_ui_game::chat::{self, Ask, ChatSettings, History, TEXT_TYPES};
use auberean_ui_game::gameplay::{self, Notice};

/// What the chat panel reads of the client and asks of it.
#[derive(Debug, Default)]
pub struct ChatClient {
    settings: Option<ChatSettings>,
    revision: u64,
    /// The options changed: read them again before the next frame.
    stale: bool,
    asks: Vec<Ask>,
    /// The names asked for, given back: the window that asked, the list, the name.
    answers: Vec<(usize, History, String)>,
    /// The commands are to be made known; lines typed that named no command since last asked.
    know_commands: bool,
    unknown: usize,
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

    /// Something the panel asked of the player's scripts, for before the next frame.
    pub fn ask(&mut self, a: Ask) {
        self.asks.push(a);
    }

    /// The names asked for since the last frame.
    pub fn take_answers(&mut self) -> Vec<(usize, History, String)> {
        std::mem::take(&mut self.answers)
    }

    /// The player's commands made known before the next frame.
    pub fn know_commands(&mut self) {
        self.know_commands = true;
    }

    /// How many lines typed named no command since last asked.
    pub fn take_unknown(&mut self) -> usize {
        std::mem::take(&mut self.unknown)
    }

    /// Before a frame: the options read where they changed (or were never read); what the panel
    /// asked, asked of the player's scripts.
    pub fn prepare(&mut self, client: &mut Client) {
        if client.model.player_object().is_none() {
            return;
        }
        if std::mem::take(&mut self.know_commands) {
            client.know_commands();
        }
        for a in std::mem::take(&mut self.asks) {
            self.carry_out(client, a);
        }
        if self.stale || self.settings.is_none() {
            if let Some(o) = client.options() {
                self.revision += 1;
                self.settings = Some(read(&o, self.revision));
                self.stale = false;
            }
        }
    }

    fn carry_out(&mut self, client: &mut Client, a: Ask) {
        let word = InterfaceArg::Word;
        match a {
            Ask::History {
                list,
                earlier,
                window,
            } => {
                let f = match (list, earlier) {
                    // D2-00587
                    (History::Retells, true) => function::INQ_PREV_RETELL_HISTORY_ITEM,
                    // D2-00586
                    (History::Retells, false) => function::INQ_NEXT_RETELL_HISTORY_ITEM,
                    // D2-00589
                    (History::Replies, true) => function::INQ_PREV_REPLY_HISTORY_ITEM,
                    // D2-00588
                    (History::Replies, false) => function::INQ_NEXT_REPLY_HISTORY_ITEM,
                    (History::Commands, _) => {
                        let item = if earlier {
                            client.previous_command()
                        } else {
                            client.next_command()
                        };
                        if let Some(t) = item {
                            self.answers.push((window, list, t));
                        }
                        return;
                    }
                };
                if let Some(t) = client.history_item(f) {
                    self.answers.push((window, list, t));
                }
            }
            // D2-00575, D2-01269, D2-00596, D2-01284
            Ask::Opacity { window, opacity } => {
                let a = [word(opacity.to_bits()), word(window.raw())];
                client.interface(function::SET_WINDOW_OPACITY, &a);
                client.interface(function::UPLOAD_UI_SETTINGS, &[]);
            }
            // D2-00571, D2-01248
            Ask::Filter { window, filter } => {
                client.interface(
                    function::SET_CHAT_WINDOW_FILTER,
                    &[word(filter), word(window)],
                );
            }
            // D2-00572, D2-01246
            Ask::FontSize { kind, size } => {
                client.interface(function::SET_CHAT_FONT_SIZE, &[word(size), word(kind)]);
            }
            // D2-00573, D2-01245
            Ask::FontColor { kind, color } => {
                client.interface(function::SET_CHAT_FONT_COLOR, &[word(color), word(kind)]);
            }
            // D2-00576, D2-01247
            Ask::Popup { kind, on } => {
                client.interface(
                    function::SET_CHAT_POPUP_FLAG,
                    &[word(u32::from(on)), word(kind)],
                );
            }
            // D2-00574, D2-01084
            Ask::Devote { window, kind } => {
                client.interface(function::DEVOTE_CHAT_WINDOW, &[word(kind), word(window)]);
            }
            // D2-00595, D2-01282
            Ask::KeepChatOptions => {
                client.interface(function::UPLOAD_CHAT_OPTIONS, &[]);
            }
            Ask::ReloadChatSettings => self.stale = true,
            // D2-03118
            Ask::Text(t) => {
                if client.process_text(&t) == ShellStatus::UnknownCommand {
                    self.unknown += 1;
                }
            }
        }
    }
}

/// The chat's part of a new player's options (the chat page's defaults).
#[must_use]
pub fn defaults() -> ChatSettings {
    read(
        &auberean_client_sdk::script::options::GameplayOptions::default(),
        0,
    )
}

/// The chat's part of a profile, for the windows.
fn read(o: &auberean_client_sdk::script::options::GameplayOptions, revision: u64) -> ChatSettings {
    let windows = [1, 2, 3, 4];
    ChatSettings {
        revision,
        filters: windows.map(|w| options::filter(o, w)),
        devoted: windows.map(|w| options::devoted_channel(o, w)),
        fonts: TEXT_TYPES
            .iter()
            .map(|t| {
                (
                    t.kind,
                    options::font_size(o, t.kind),
                    options::font_color(o, t.kind),
                    options::popup(o, t.kind),
                )
            })
            .collect(),
        opacities: chat::CHAT_WINDOWS
            .iter()
            .filter_map(|w| options::window_opacity(o, w.raw()).map(|v| (*w, v)))
            .collect(),
    }
}
