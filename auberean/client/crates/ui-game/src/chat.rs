//! The toolbar's chat panel: four chat windows and the line of messages at the screen's top.
//!
//! [`ChatUi`] is the panel. It makes the four chat windows ([`ChatPane`]), each its own child
//! framework from the toolbar's layouts (`ChatWindow1` to `ChatWindow4`); the first shows and is
//! the active window, the others are hidden until their buttons on the first window (or their
//! keys) show them. It hears the interface keys and hands each to the window it is for: the one
//! whose entry has the keyboard, else the one that had it last (shown first when the key is one
//! that starts typing).
//!
//! **What a window shows.** The player's scripts write every line for the chat windows and say
//! what kind of text it is (a bit each: the game's errors, combat, standard messages, sayings,
//! tells, emotes, the channels...). Each window shows the kinds its filter holds, each kind in the
//! font size and color the player's options give it, and pops up when a line of a kind marked to
//! do so comes while it is hidden (its button changes look otherwise). A window may be devoted to
//! a channel: its empty entry is labelled with the channel's name and what is typed there is said
//! on that channel. The error lines also go to the line of messages at the screen's top, each for
//! a few seconds.
//!
//! **The entry.** A press on it, Enter, `/` (a command), `;` (an alias) or the reply key start
//! typing in the window; Enter hands the line to the player's command line and stops typing;
//! Escape stops typing at the next tick (what was typed stays). The arrows go back and forth
//! through the lines typed before, with Ctrl through the tells sent, with Shift through the tells
//! answered; Page Up and Page Down page the log, Ctrl+Home and Ctrl+End go to its start and end.
//!
//! **The window.** Its move bar drags it (and makes it the active window), its lock button keeps
//! the log from scrolling to each new line, pressing its scroll bar does so until let go, its
//! options button shows the opacity slider, which fades the window and is kept with the player's
//! options.

use std::any::Any;
use std::collections::HashMap;

use auberean_ui::framework::FrameId;
use auberean_ui::listener::Mail;
use auberean_ui::manager::{global, LAYOUT_MAPPING};
use auberean_ui::types::key_id;
use auberean_ui::{Handle, Token, Ui, Value};

use crate::gameplay::{Child, Notice, Record, Request, View};
use crate::shortcuts::TOOLBAR_LAYOUT_ENUM;

/// The kinds of text the chat windows show, a bit each.
pub mod text_type {
    pub const ERROR: u32 = 0x1;
    pub const COMBAT: u32 = 0x2;
    pub const ADMIN: u32 = 0x4;
    pub const STANDARD: u32 = 0x8;
    pub const SAY: u32 = 0x10;
    pub const TELL: u32 = 0x20;
    pub const EMOTE: u32 = 0x40;
    pub const LOG: u32 = 0x80;
    pub const BROADCAST: u32 = 0x100;
    pub const FELLOWSHIP: u32 = 0x1_0000;
    pub const ALLEGIANCE: u32 = 0x2_0000;
    /// What the entry itself is drawn as.
    pub const CHAT_ENTRY: u32 = 0x4_0000;
    pub const GENERAL_CHANNEL: u32 = 0x8_0000;
    pub const TRADE_CHANNEL: u32 = 0x10_0000;
    pub const REGION_CHANNEL: u32 = 0x20_0000;
    pub const FACTION_CHANNEL: u32 = 0x40_0000;
    /// The chat options' row of the windows' devoted channels.
    pub const DEVOTED: u32 = 0x80_0000;
    pub const PK_CHANNEL: u32 = 0x100_0000;
}

/// One kind of text as the chat options list it: its bit, its name (in the interface's strings),
/// the command that speaks on it (in the commands' strings; 0 for none) and whether the options
/// panel leaves it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextTypeInfo {
    pub kind: u32,
    pub name: Token,
    pub cmd: u32,
    pub hidden: bool,
}

const fn row(kind: u32, name: &str, cmd: u32, hidden: bool) -> TextTypeInfo {
    TextTypeInfo {
        kind,
        name: Token::of(name),
        cmd,
        hidden,
    }
}

const FELLOWSHIP_CHAT: u32 = Token::of("ID_FellowshipChat").raw();
const ALLEGIANCE_CHAT: u32 = Token::of("ID_AllegianceChat").raw();
const FACTION_CHAT: u32 = Token::of("ID_FactionChat").raw();
const GENERAL_CHAT: u32 = Token::of("ID_GeneralChannelChat").raw();
const TRADE_CHAT: u32 = Token::of("ID_TradeChannelChat").raw();
const REGION_CHAT: u32 = Token::of("ID_RegionChannelChat").raw();

/// The kinds of text, in the chat options' order (the kinds a window looks through for a line's
/// font, first match first).
pub const TEXT_TYPES: [TextTypeInfo; 18] = {
    use text_type as t;
    [
        row(t::ERROR, "ID_Option_ChatErrorMessages", 0, false),
        row(t::COMBAT, "ID_Option_ChatCombatMessages", 0, false),
        row(t::ADMIN, "ID_Option_ChatAdminMessages", 0, true),
        row(t::STANDARD, "ID_Option_ChatStandardMessages", 0, false),
        row(t::SAY, "ID_Option_ChatSayMessages", 0, false),
        row(t::TELL, "ID_Option_ChatTellMessages", 0, false),
        row(t::EMOTE, "ID_Option_ChatEmoteMessages", 0, false),
        row(t::LOG, "ID_Option_ChatLogMessages", 1, false),
        row(
            t::FELLOWSHIP,
            "ID_Option_ChatFellowshipMessages",
            FELLOWSHIP_CHAT,
            false,
        ),
        row(
            t::ALLEGIANCE,
            "ID_Option_ChatAllegianceMessages",
            ALLEGIANCE_CHAT,
            false,
        ),
        row(
            t::FACTION_CHANNEL,
            "ID_Option_ChatFactionChannelMessages",
            FACTION_CHAT,
            false,
        ),
        row(t::PK_CHANNEL, "ID_Option_ChatPKChannelMessages", 0, false),
        row(
            t::GENERAL_CHANNEL,
            "ID_Option_ChatGeneralChannelMessages",
            GENERAL_CHAT,
            false,
        ),
        row(
            t::TRADE_CHANNEL,
            "ID_Option_ChatTradeChannelMessages",
            TRADE_CHAT,
            false,
        ),
        row(
            t::REGION_CHANNEL,
            "ID_Option_ChatRegionChannelMessages",
            REGION_CHAT,
            false,
        ),
        row(t::BROADCAST, "ID_Option_ChatBroadcastMessages", 0, false),
        row(t::DEVOTED, "ID_Option_DevotedChatWindows", 0, false),
        row(t::CHAT_ENTRY, "ID_Option_ChatEntryMessages", 0, false),
    ]
};

/// The channels a window may be devoted to, by row: a row the options panel leaves out is not
/// offered. (Its rows do not line up with [`TEXT_TYPES`] past the eleventh, and the last is
/// empty.)
pub const DEVOTED_TYPES: [TextTypeInfo; 18] = {
    use text_type as t;
    [
        row(t::ERROR, "ID_Option_ChatErrorMessages", 0, true),
        row(t::COMBAT, "ID_Option_ChatCombatMessages", 0, true),
        row(t::ADMIN, "ID_Option_ChatAdminMessages", 0, true),
        row(t::STANDARD, "ID_Option_ChatStandardMessages", 0, true),
        row(t::SAY, "ID_Option_ChatSayMessages", 0, false),
        row(t::TELL, "ID_Option_ChatTellMessages", 0, true),
        row(t::EMOTE, "ID_Option_ChatEmoteMessages", 0, true),
        row(t::LOG, "ID_Option_ChatLogMessages", 0, true),
        row(
            t::FELLOWSHIP,
            "ID_Option_ChatFellowshipMessages",
            FELLOWSHIP_CHAT,
            false,
        ),
        row(
            t::ALLEGIANCE,
            "ID_Option_ChatAllegianceMessages",
            ALLEGIANCE_CHAT,
            false,
        ),
        row(
            t::FACTION_CHANNEL,
            "ID_Option_ChatFactionChannelMessages",
            FACTION_CHAT,
            false,
        ),
        row(
            t::GENERAL_CHANNEL,
            "ID_Option_ChatGeneralChannelMessages",
            GENERAL_CHAT,
            false,
        ),
        row(
            t::TRADE_CHANNEL,
            "ID_Option_ChatTradeChannelMessages",
            TRADE_CHAT,
            false,
        ),
        row(
            t::REGION_CHANNEL,
            "ID_Option_ChatRegionChannelMessages",
            REGION_CHAT,
            false,
        ),
        row(t::BROADCAST, "ID_Option_ChatBroadcastMessages", 0, true),
        row(t::DEVOTED, "ID_Option_DevotedChatWindows", 0, true),
        row(t::CHAT_ENTRY, "ID_Option_ChatEntryMessages", 0, true),
        TextTypeInfo {
            kind: 0,
            name: Token(0),
            cmd: 0,
            hidden: false,
        },
    ]
};

/// The player's scripts' notices the chat panel hears, by the native that sends each.
pub mod notice {
    /// Put a tell to someone in the entry of the window keys are for.
    pub const DO_ASSISTED_TELL: u32 = 0x0087 << 16 | 0x0002;
    /// The chat's part of the player's options changed.
    pub const RELOAD_CHAT_SETTINGS: u32 = 0x0087 << 16 | 0x0004;
}

/// The interface keys the chat panel hears besides Enter, Escape, the arrows and the page keys.
pub mod key {
    /// Start a command, an alias.
    pub const START_COMMAND: u32 = 0x4100_0010;
    pub const START_ALIAS: u32 = 0x4100_0011;
    /// Show or hide windows 1 to 4.
    pub const CHAT_WINDOW: [u32; 4] = [0x4100_0041, 0x4100_0042, 0x4100_0043, 0x4100_0044];
    /// Reply to the last tell.
    pub const REPLY: u32 = 0x4100_0049;
    /// Start or stop typing in the chat window.
    pub const INCREMENT_ACTIVATION: u32 = auberean_ui::types::key_id::NEXT;
}

/// The string tables the chat writes from, by enumeration value: the interface's strings, the
/// commands' names.
pub const STRINGS: u32 = 0x4100_0018;
pub const COMMANDS: u32 = 0x4100_0007;

/// The layout the line of messages' rows are made from, by enumeration value.
const SPEW_LAYOUT_ENUM: u32 = 0x4100_000F;

/// The windows' top-level elements, by name (the options keep a window's opacity by it).
pub const CHAT_WINDOWS: [Token; 4] = [
    Token::of("ChatWindow1"),
    Token::of("ChatWindow2"),
    Token::of("ChatWindow3"),
    Token::of("ChatWindow4"),
];

mod name {
    use auberean_ui::Token;
    pub const SPEW_BOX: Token = Token::of("SpewBox");
    pub const SPEW_BOX_MESSAGE: Token = Token::of("SpewBoxMessage");
    pub use super::CHAT_WINDOWS;
    /// The buttons on the first window that show and hide the others.
    pub const TOGGLES: [Token; 4] = [
        Token(0),
        Token::of("ChatButtonToggle2"),
        Token::of("ChatButtonToggle3"),
        Token::of("ChatButtonToggle4"),
    ];
    pub const OPACITY_SLIDER_BAR: Token = Token::of("OpacitySliderBar");
    pub const OPTIONS_FIELD: Token = Token::of("OptionsField");
    pub const OPTIONS_BUTTON: Token = Token::of("OptionsButton");
    pub const MOVE_BAR: Token = Token::of("MoveBar");
    pub const MOVE_BAR_FIELD: Token = Token::of("MoveBarField");
    pub const CHAT_LOG: Token = Token::of("ChatLog");
    pub const CHAT_ENTRY: Token = Token::of("ChatEntry");
    pub const CHAT_LOG_SCROLLBAR: Token = Token::of("ChatLogScrollbar");
    pub const CHAT_WINDOW_LOCK: Token = Token::of("ChatWindowLock");
}

/// Attributes, states, messages and string variables the chat reads, sets and hears.
mod tok {
    use auberean_ui::Token;
    pub const ZLEVEL_FRONT: Token = Token::of("zlevel_front");
    pub const TEXT_LOCKED: Token = Token::of("text_locked");
    pub const DECAY_TIME: Token = Token::of("decay_time");
    pub const Y_POSITION: Token = Token::of("y_position");
    pub const LOCKED: Token = Token::of("locked");
    pub const UNLOCKED: Token = Token::of("unlocked");
    pub const ACTIVE: Token = Token::of("active");
    pub const INACTIVE: Token = Token::of("inactive");
    pub const CHANGED: Token = Token::of("changed");
    pub const UNCHANGED: Token = Token::of("unchanged");
    pub const LOCATION_CHANGED: Token = Token::of("location_changed");
    pub const CHARACTER_INPUT: Token = Token::of("character_input");
    pub const ACTIVATED: Token = Token::of("activated");
    pub const DEACTIVATED: Token = Token::of("deactivated");
    pub const MOUSE_PRESS: Token = Token::of("mouse_press");
    pub const MOUSE_RELEASE: Token = Token::of("mouse_release");
    pub const MOUSE_RELEASE_ON: Token = Token::of("mouse_release_on");
    pub const VERTICAL_LINE_DECREMENT: Token = Token::of("vertical_line_decrement");
    pub const VERTICAL_LINE_INCREMENT: Token = Token::of("vertical_line_increment");
    pub const NEW_POSITION: Token = Token::of("new_position");
    pub const WIDGET_RELEASE: Token = Token::of("widget_release");
    pub const PREFIX: Token = Token::of("prefix");
    pub const TARGET: Token = Token::of("target");
    pub const CHANNEL: Token = Token::of("CHANNEL");
}

/// Strings of the interface's table, and the reply command's names in the commands' table.
mod string {
    use auberean_ui::Token;
    pub const CHAT_ENTRY: Token = Token::of("ID_Chat_ChatEntry");
    pub const DEVOTED_LABEL: Token = Token::of("ID_DevotedChatWindowLabel");
    pub const USAGE_MESSAGE: Token = Token::of("ID_Chat_UsageMessage");
    pub const ASSISTED_TELL: Token = Token::of("ID_Chat_AssistedTell");
    pub const COMMAND_PREFIX: Token = Token::of("ID_Chat_CommandPrefix");
    pub const ALIAS_PREFIX: Token = Token::of("ID_Chat_AliasPrefix");
    pub const REPLY: Token = Token::of("ID_Reply");
}

/// What starts a command typed, and what joins a command to its words.
const SLASH: &str = "/";
const SPACE: &str = " ";
/// What separates a command's names in its string.
const NAME_SEPARATORS: &[char] = &[';', ':', ','];

/// What a window shows of one kind of text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MessageInfo {
    /// The font's number and the color's (the text element's `font<n>` and `font_color<n>`).
    pub size: u32,
    pub color: u32,
    /// The kind's name and the command that speaks on it (0: none).
    pub name: Token,
    pub cmd: u32,
    /// A line of this kind shows the window when it is hidden.
    pub popup: bool,
}

/// The chat's part of the player's options, as the chat windows read it when told the options
/// changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatSettings {
    /// A new number each time the options are read again.
    pub revision: u64,
    /// Each window's filter (the kinds of text it shows) and the channel it is devoted to,
    /// windows 1 to 4.
    pub filters: [u32; 4],
    pub devoted: [u32; 4],
    /// Each kind of text's font size, font color and popup flag.
    pub fonts: Vec<(u32, u32, u32, bool)>,
    /// The windows' opacities the options keep, by the window's name.
    pub opacities: Vec<(Token, f32)>,
}

impl ChatSettings {
    /// A kind of text's font size, font color and popup flag (0, 0 and off for a kind not
    /// listed).
    #[must_use]
    pub fn font(&self, kind: u32) -> (u32, u32, bool) {
        self.fonts
            .iter()
            .find(|f| f.0 == kind)
            .map_or((0, 0, false), |f| (f.1, f.2, f.3))
    }

    fn opacity(&self, window: Token) -> Option<f32> {
        self.opacities.iter().find(|o| o.0 == window).map(|o| o.1)
    }

    /// The window's filter (windows 1 to 4), 0 for none.
    #[must_use]
    pub fn filter(&self, window: u32) -> u32 {
        self.filters
            .get(window.wrapping_sub(1) as usize)
            .copied()
            .unwrap_or(0)
    }

    /// The channel the window is devoted to, the sayings' when none.
    #[must_use]
    pub fn devoted_channel(&self, window: u32) -> u32 {
        self.devoted
            .get(window.wrapping_sub(1) as usize)
            .copied()
            .unwrap_or(text_type::SAY)
    }
}

/// Lists of lines a window goes back and forth through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum History {
    /// The lines typed.
    Commands,
    /// The tells sent.
    Retells,
    /// The tells answered.
    Replies,
}

/// What the chat panel asks of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// A line typed in a window, for the player's command line.
    Text(String),
    /// The line before (`earlier`) or after the one shown from one of the lists, for a window
    /// (its place in the panel): given back through [`ChatUi::history_item`].
    History {
        list: History,
        earlier: bool,
        window: usize,
    },
    /// A window's opacity (by its name) kept with the player's options, and the windows'
    /// settings sent to the server to keep.
    Opacity {
        window: Token,
        opacity: f32,
    },
    /// The chat options set (windows 1 to 4; a kind of text): a window's filter, a kind's font
    /// size, font color and popup flag, the channel a window is devoted to.
    Filter {
        window: u32,
        filter: u32,
    },
    FontSize {
        kind: u32,
        size: u32,
    },
    FontColor {
        kind: u32,
        color: u32,
    },
    Popup {
        kind: u32,
        on: bool,
    },
    Devote {
        window: u32,
        kind: u32,
    },
    /// The chat options sent to the server to keep.
    KeepChatOptions,
    /// The chat options read again (they changed).
    ReloadChatSettings,
}

/// A line for the chat windows: its kind, its text as written, and the string reference it was
/// written from when it came as one (the log adds the reference, written once its tables are at
/// hand).
// D2-03085
#[derive(Clone, Debug)]
struct Line {
    kind: u32,
    text: String,
    info: Option<auberean_text::StringInfo>,
}

impl Line {
    const fn written(kind: u32, text: String) -> Self {
        Self {
            kind,
            text,
            info: None,
        }
    }
}

/// The string table an enumeration value names.
fn table(ui: &Ui, table_enum: u32) -> u32 {
    ui.assets()
        .data_id_by_enum(table_enum, crate::flow::TABLE_MAPPING)
}

/// A string reference, written.
fn write(ui: &Ui, info: &auberean_text::StringInfo) -> String {
    ui.assets()
        .write(info)
        .or_else(|| ui.assets().text(info.table, info.string_id))
        .unwrap_or_default()
}

/// A string of one of the chat's tables, by its token.
fn entry(ui: &Ui, table_enum: u32, token: Token) -> auberean_text::StringInfo {
    auberean_text::StringInfo::entry(table(ui, table_enum), token.raw())
}

/// The words of a string, split at the separators, each trimmed, the empty ones dropped.
fn words(s: &str) -> Vec<&str> {
    s.split(NAME_SEPARATORS)
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect()
}

/// One chat window: its framework and elements, what it shows and how.
#[derive(Debug)]
pub struct ChatPane {
    frame: FrameId,
    /// Its number (1 to 4), its top-level element's name, and the name of the button that shows
    /// and hides it (none for the first).
    number: u32,
    window: Token,
    button_name: Token,
    root: Option<Handle>,
    entry: Option<Handle>,
    log: Option<Handle>,
    button: Option<Handle>,
    opacity_bar: Option<Handle>,
    options_field: Option<Handle>,
    options_button: Option<Handle>,
    move_bar: Option<Handle>,
    move_bar_field: Option<Handle>,
    scrollbar: Option<Handle>,
    lock: Option<Handle>,
    /// The entry holds a label (the prompt, a channel's name) that goes when typing starts.
    prepopulated: bool,
    /// Escape was pressed: the entry gives up the keyboard at the next tick.
    deactivate_pending: bool,
    /// The kinds of text it shows; the lines that came, waiting for the next tick.
    filter: u32,
    pending: Vec<Line>,
    /// Its depth while its entry has the keyboard.
    zlevel_front: i32,
    /// The channel it is devoted to.
    devoted: u32,
    /// What it shows of each kind.
    infos: HashMap<u32, MessageInfo>,
    /// The entry was labelled once.
    labelled: bool,
    /// The log does not follow new lines: while its scroll bar is held, and while locked.
    temp_lock: bool,
    perm_lock: bool,
}

impl ChatPane {
    /// The window `number` (1 to 4) under the chat panel: its top-level element from the
    /// toolbar's layouts and its parts found by name, the opacity slider's field hidden, its
    /// button (on the first window) heard, its parts' messages and the clock heard; hidden.
    // D2-03117
    fn new(ui: &mut Ui, parent: FrameId, number: u32, window: Token, button_name: Token) -> Self {
        let frame = ui.new_child_frame(parent);
        let root = ui.frame_create_root(frame, TOOLBAR_LAYOUT_ENUM, window);
        let part = |ui: &Ui, n: Token| root.and_then(|r| ui.child_recursive(r, n));
        let mut me = Self {
            frame,
            number,
            window,
            button_name,
            root,
            entry: part(ui, name::CHAT_ENTRY),
            log: part(ui, name::CHAT_LOG),
            button: None,
            opacity_bar: part(ui, name::OPACITY_SLIDER_BAR),
            options_field: part(ui, name::OPTIONS_FIELD),
            options_button: part(ui, name::OPTIONS_BUTTON),
            move_bar: part(ui, name::MOVE_BAR),
            move_bar_field: part(ui, name::MOVE_BAR_FIELD),
            scrollbar: part(ui, name::CHAT_LOG_SCROLLBAR),
            lock: part(ui, name::CHAT_WINDOW_LOCK),
            prepopulated: true,
            deactivate_pending: false,
            filter: 0xF000_0000,
            pending: Vec::new(),
            zlevel_front: 0,
            devoted: text_type::SAY,
            infos: HashMap::new(),
            labelled: false,
            temp_lock: false,
            perm_lock: false,
        };
        if let Some(f) = me.options_field {
            ui.set_visible(f, false);
        }
        if let Some(r) = root {
            me.zlevel_front = ui
                .node(r)
                .and_then(|n| n.int_attr(tok::ZLEVEL_FRONT))
                .unwrap_or(0);
        }
        if let Some(l) = ui.frame_listener(frame) {
            if button_name != Token(0) {
                me.button = ui.element(button_name);
                ui.listen_for_message(l, button_name, tok::MOUSE_RELEASE);
            }
            for (el, what) in [
                (window, tok::LOCATION_CHANGED),
                (name::CHAT_ENTRY, tok::CHARACTER_INPUT),
                (name::CHAT_ENTRY, tok::ACTIVATED),
                (name::CHAT_ENTRY, tok::DEACTIVATED),
                (name::CHAT_LOG_SCROLLBAR, tok::MOUSE_PRESS),
                (name::CHAT_LOG_SCROLLBAR, tok::MOUSE_RELEASE),
                (name::CHAT_LOG_SCROLLBAR, tok::VERTICAL_LINE_DECREMENT),
                (name::CHAT_LOG_SCROLLBAR, tok::VERTICAL_LINE_INCREMENT),
                (name::OPACITY_SLIDER_BAR, tok::NEW_POSITION),
                (name::OPACITY_SLIDER_BAR, tok::WIDGET_RELEASE),
                (name::OPACITY_SLIDER_BAR, tok::MOUSE_RELEASE),
                (name::OPTIONS_BUTTON, tok::MOUSE_RELEASE_ON),
                (name::MOVE_BAR, tok::MOUSE_PRESS),
                (name::MOVE_BAR, tok::MOUSE_RELEASE),
                (name::CHAT_WINDOW_LOCK, tok::MOUSE_PRESS),
            ] {
                ui.listen_for_message(l, el, what);
            }
            ui.listen_for_global_message(l, global::TIME);
        }
        me.show(ui, false);
        me
    }

    /// Its framework.
    #[must_use]
    pub const fn frame(&self) -> FrameId {
        self.frame
    }

    /// Its top-level element, its entry and its log.
    #[must_use]
    pub const fn root(&self) -> Option<Handle> {
        self.root
    }

    #[must_use]
    pub const fn entry(&self) -> Option<Handle> {
        self.entry
    }

    #[must_use]
    pub const fn log(&self) -> Option<Handle> {
        self.log
    }

    /// The kinds of text it shows, and the channel it is devoted to.
    #[must_use]
    pub const fn filter(&self) -> u32 {
        self.filter
    }

    #[must_use]
    pub const fn devoted(&self) -> u32 {
        self.devoted
    }

    /// Whether it shows (its own flag, while the panel shows).
    #[must_use]
    pub fn shown(&self, ui: &Ui) -> bool {
        ui.frame_shown(self.frame)
    }

    /// The log's lines, oldest first.
    #[must_use]
    pub fn lines(&self, ui: &Ui) -> Vec<String> {
        self.log.map(|l| ui.lines(l)).unwrap_or_default()
    }

    /// Whether the top-level element of what has the keyboard is its own.
    // D2-03076
    fn is_active_root(&self, ui: &Ui) -> bool {
        let Some(e) = self.entry else { return false };
        ui.active_root() == Some(ui.root_of(e))
    }

    /// Whether its entry has the keyboard.
    // D2-03077
    #[must_use]
    pub fn is_active(&self, ui: &Ui) -> bool {
        self.entry.is_some() && ui.active() == self.entry
    }

    /// Its entry takes the keyboard, while it shows.
    // D2-03078
    fn activate_entry(&self, ui: &mut Ui) {
        if self.shown(ui) {
            if let Some(e) = self.entry {
                ui.activate_element(e);
            }
        }
    }

    /// Its entry gives up the keyboard.
    // D2-03079
    fn deactivate_entry(&mut self, ui: &mut Ui) {
        self.deactivate_pending = false;
        if let Some(e) = self.entry {
            ui.deactivate_element(e);
        }
    }

    /// Show or hide it. Shown, its entry gives up the keyboard; hidden, its button looks
    /// unchanged again. Its top-level element shows while it does.
    // D2-03081
    fn show(&mut self, ui: &mut Ui, on: bool) {
        let was = self.shown(ui);
        ui.frame_show(self.frame, on);
        let now = self.shown(ui);
        if was != now {
            if on {
                self.deactivate_entry(ui);
            } else {
                if self.shown(ui) {
                    self.activate_entry(ui);
                }
                if let Some(b) = self.button {
                    ui.set_state(b, tok::UNCHANGED);
                }
            }
        }
        if let Some(r) = self.root {
            ui.set_visible(r, now);
        }
    }

    /// Its kept place was put back: it shows as its top-level element was kept.
    // D2-03082
    fn location_applied(&mut self, ui: &mut Ui) {
        if let Some(r) = self.root {
            let visible = ui.node(r).is_some_and(|n| n.visible);
            self.show(ui, visible);
        }
    }

    /// Its depth among the screen's top-level elements.
    // D2-03083
    fn set_z_level(&self, ui: &mut Ui, z: i32) {
        if let Some(r) = self.root {
            ui.set_z_level(r, z);
        }
    }

    /// An element's faded pictures drawn at `opacity`, and its elements', all the way down.
    // D2-03086
    fn set_opacity(ui: &mut Ui, h: Handle, opacity: f32) {
        ui.set_alpha(h, opacity);
        let children = ui.node(h).map(|n| n.children.clone()).unwrap_or_default();
        for c in children {
            Self::set_opacity(ui, c, opacity);
        }
    }

    /// Its move bar looks active or not.
    // D2-03087
    fn set_active_window(&self, ui: &mut Ui, on: bool) {
        if let Some(f) = self.move_bar_field {
            ui.set_state(f, if on { tok::ACTIVE } else { tok::INACTIVE });
        }
    }

    /// A line for the chat windows: kept for the next tick when its kind is one it shows.
    // D2-03095
    fn display_string_info(&mut self, line: &Line) {
        if line.kind & self.filter != 0 {
            self.pending.push(line.clone());
        }
    }

    /// The entry gave up the keyboard: an empty entry of a devoted window is labelled with its
    /// channel's name in the channel's font.
    // D2-03098
    fn label_devoted(&mut self, ui: &mut Ui) {
        let Some(e) = self.entry else { return };
        let empty = ui.text_state(e).is_none_or(|t| t.glyphs.count() < 1);
        if !empty || self.devoted == text_type::SAY {
            return;
        }
        let Some(info) = self.infos.get(&self.devoted).copied() else {
            return;
        };
        let mut label = entry(ui, STRINGS, string::DEVOTED_LABEL);
        label.add_variable_string_id(tok::CHANNEL.raw(), info.name.raw());
        let text = write(ui, &label);
        ui.add_string_with_font(e, &text, info.size, info.color);
        self.prepopulated = true;
    }

    /// What it shows of the first kind in `mask` that it shows (its filter's, or its channel), in
    /// the kinds' order.
    // D2-03100
    fn message_info(&self, mask: u32) -> Option<MessageInfo> {
        TEXT_TYPES.iter().find_map(|t| {
            (mask & t.kind != 0 && (self.filter & t.kind != 0 || self.devoted == t.kind))
                .then(|| self.infos.get(&t.kind).copied())
                .flatten()
        })
    }

    /// A tell to `target` put in the entry ("tell Name, " after the command's mark) in the
    /// entry's font, and typing started there.
    // D2-03101
    pub fn do_assisted_tell(&mut self, ui: &mut Ui, target: &str) {
        self.prepopulated = false;
        let mut tell = entry(ui, STRINGS, string::ASSISTED_TELL);
        tell.add_variable_string_id(tok::PREFIX.raw(), string::COMMAND_PREFIX.raw());
        let units: Vec<u16> = target.encode_utf16().collect();
        tell.add_variable_string(tok::TARGET.raw(), &units);
        let Some(e) = self.entry else { return };
        let info = self.message_info(text_type::CHAT_ENTRY).unwrap_or_default();
        ui.set_string_info_with_font(e, &tell, info.size, info.color);
        ui.select_all(e);
        ui.activate_element(e);
    }

    /// The entry's text set in the entry's font.
    fn set_entry_text(&self, ui: &mut Ui, text: &str) {
        let Some(e) = self.entry else { return };
        let info = self.message_info(text_type::CHAT_ENTRY).unwrap_or_default();
        ui.set_string_with_font(e, text, info.size, info.color);
    }

    /// The lines that came since the last tick, into the log, each in its kind's font and color;
    /// the log follows them unless locked; hidden, the window shows for a kind that pops up, and
    /// its button changes look.
    // D2-03107
    fn all_text_to_log(&mut self, ui: &mut Ui) {
        let Some(log) = self.log else { return };
        if self.pending.is_empty() {
            return;
        }
        ui.move_cursor_to_end(log);
        let mut popup = false;
        for line in std::mem::take(&mut self.pending) {
            let info = self.message_info(line.kind).unwrap_or_default();
            if !popup {
                popup = info.popup;
            }
            match &line.info {
                Some(r) => ui.add_string_info_with_font(log, r, info.size, info.color),
                None => ui.add_string_with_font(log, &line.text, info.size, info.color),
            };
        }
        ui.recalculate_glyph_list(log);
        if let Some(b) = self.button {
            if !self.shown(ui) {
                ui.set_state(b, tok::CHANGED);
            }
        }
        if !self.temp_lock && !self.perm_lock {
            ui.scroll_to_end(log);
        }
        if popup {
            self.show(ui, true);
        }
    }

    /// The window's size changed under its log: the log shows its newest lines, unless locked.
    fn show_newest_lines(&self, ui: &mut Ui) {
        let Some(log) = self.log else { return };
        if !self.temp_lock && !self.perm_lock {
            ui.recalculate_glyph_list(log);
            ui.scroll_to_end(log);
        }
    }

    /// A line from one of the lists, given back while the entry has the keyboard: a typed line
    /// into the entry, selected; a tell's name as a tell to them.
    // D2-03108, D2-03109, D2-03110
    fn history_item(&mut self, ui: &mut Ui, list: History, text: &str) {
        let Some(e) = self.entry else { return };
        if ui.active() != Some(e) {
            return;
        }
        match list {
            History::Commands => {
                let info = self.message_info(text_type::CHAT_ENTRY).unwrap_or_default();
                ui.set_string_with_font(e, text, info.size, info.color);
                ui.select_all(e);
            }
            History::Retells | History::Replies => self.do_assisted_tell(ui, text),
        }
    }

    /// The clock's tick: the lines that came into the log; the entry gives up the keyboard if
    /// Escape asked.
    // D2-03114
    fn tick(&mut self, ui: &mut Ui) {
        self.all_text_to_log(ui);
        if self.deactivate_pending {
            self.deactivate_entry(ui);
        }
    }

    /// An interface key for this window: with Ctrl alone the arrows go through the tells sent
    /// and Home and End go to the log's start and end; with Shift alone the arrows go through the
    /// tells answered; with neither the arrows go through the lines typed, the page keys page the
    /// log and Escape stops typing at the next tick. With Alt held it uses none of them. Returns
    /// whether it used the key.
    // D2-03115
    fn key_press(&mut self, ui: &mut Ui, k: u32, window: usize, out: &mut Vec<Request>) -> bool {
        if ui.alt_held() {
            return false;
        }
        let (shift, ctrl) = ui.modifiers();
        let list = if ctrl && !shift {
            match k {
                key_id::HOME | key_id::END => {
                    if let Some(l) = self.log {
                        if k == key_id::HOME {
                            ui.scroll_to_home(l);
                        } else {
                            ui.scroll_to_end(l);
                        }
                    }
                    return true;
                }
                _ => History::Retells,
            }
        } else if shift && !ctrl {
            History::Replies
        } else if !shift && !ctrl {
            match k {
                key_id::PAGE_UP | key_id::PAGE_DOWN => {
                    if let Some(l) = self.log {
                        if k == key_id::PAGE_UP {
                            ui.vertical_page_decrement(l);
                        } else {
                            ui.vertical_page_increment(l);
                        }
                    }
                    return true;
                }
                key_id::ESCAPE => {
                    self.deactivate_pending = true;
                    return true;
                }
                _ => History::Commands,
            }
        } else {
            return false;
        };
        if k != key_id::UP && k != key_id::DOWN {
            return false;
        }
        // The lists are asked only while the entry has the keyboard.
        if self.entry.is_some() && ui.active() == self.entry {
            out.push(Request::Chat(Ask::History {
                list,
                earlier: k == key_id::UP,
                window,
            }));
        }
        true
    }

    /// A line typed in a devoted window that is not a command is said on the window's channel:
    /// the channel's command (its first name) put before it. Returns whether it was.
    // D2-03116
    fn prepend_devoted_command(&self, ui: &Ui, text: &mut String) -> bool {
        if self.devoted == text_type::SAY {
            return false;
        }
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.starts_with(SLASH) {
            return false;
        }
        let Some(info) = self.infos.get(&self.devoted) else {
            return false;
        };
        if info.cmd == 0 {
            return false;
        }
        let names = write(
            ui,
            &auberean_text::StringInfo::entry(table(ui, COMMANDS), info.cmd),
        );
        let Some(first) = words(&names).first().map(|w| (*w).to_owned()) else {
            return false;
        };
        *text = format!("{SLASH}{first}{SPACE}{trimmed}");
        true
    }

    /// The line typed (a devoted window's on its channel) handed to the player's command line,
    /// the entry emptied.
    // D2-03118
    fn process_command(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let Some(e) = self.entry else { return };
        let mut text = ui.text(e).unwrap_or_default();
        if text.is_empty() {
            return;
        }
        self.prepend_devoted_command(ui, &mut text);
        out.push(Request::Chat(Ask::Text(text)));
        ui.clear_all_text(e);
    }

    /// Enter: while typing, the line goes and the entry gives up the keyboard; else the entry of
    /// a shown window takes the keyboard.
    // D2-03119
    fn handle_enter_key(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        if let Some(e) = self.entry.filter(|e| ui.active() == Some(*e)) {
            self.process_command(ui, out);
            self.deactivate_pending = false;
            ui.deactivate_element(e);
            return;
        }
        self.activate_entry(ui);
    }

    /// The player's options changed: the chat's options read again, then the window faded as the
    /// options keep it, the slider set to match.
    // D2-03120
    fn reload_options(&mut self, ui: &mut Ui, settings: &ChatSettings) {
        self.reload_chat_settings(ui, settings);
        let Some(opacity) = settings.opacity(self.window) else {
            return;
        };
        if let Some(r) = self.root {
            Self::set_opacity(ui, r, opacity);
        }
        if let Some(bar) = self.opacity_bar {
            ui.set_attr(bar, tok::Y_POSITION, Value::Float((opacity - 0.0) / 1.0));
        }
    }

    /// The chat options changed: what the window shows of each kind; the channel it is devoted
    /// to; the entry's label (the prompt the first time; later a channel's name, when the channel
    /// changed and the entry holds a label or is empty and not typed in); its filter (a window
    /// that comes to show nothing says how to choose what it shows); the entry's font.
    // D2-03121
    fn reload_chat_settings(&mut self, ui: &mut Ui, settings: &ChatSettings) {
        let before = self.devoted;
        self.devoted = settings.devoted_channel(self.number);
        self.infos.clear();
        // The last kind's font stays for the label of a window devoted to the sayings.
        let mut font = MessageInfo::default();
        for t in &TEXT_TYPES {
            let (size, color, popup) = settings.font(t.kind);
            font = MessageInfo {
                size,
                color,
                name: t.name,
                cmd: t.cmd,
                popup,
            };
            self.infos.insert(t.kind, font);
        }
        let idle = self.entry.is_some_and(|e| {
            ui.text_state(e).is_none_or(|t| t.glyphs.count() < 1) && ui.active() != Some(e)
        });
        if !self.labelled || ((self.prepopulated || idle) && self.devoted != before) {
            let channel = |ui: &Ui, i: &MessageInfo| {
                let mut l = entry(ui, STRINGS, string::DEVOTED_LABEL);
                l.add_variable_string_id(tok::CHANNEL.raw(), i.name.raw());
                l
            };
            let label = if self.devoted != text_type::SAY {
                self.infos.get(&self.devoted).map(|i| channel(ui, i))
            } else if self.labelled {
                self.infos
                    .get(&text_type::CHAT_ENTRY)
                    .map(|i| channel(ui, i))
            } else {
                Some(entry(ui, STRINGS, string::CHAT_ENTRY))
            };
            if self.devoted != text_type::SAY {
                if let Some(i) = self.infos.get(&self.devoted) {
                    font = *i;
                }
            }
            if let Some(e) = self.entry {
                ui.clear_all_text(e);
                let text = label.map(|l| write(ui, &l)).unwrap_or_default();
                ui.add_string_with_font(e, &text, font.size, font.color);
            }
            self.labelled = true;
        }
        let old = self.filter;
        self.filter = settings.filter(self.number) | text_type::CHAT_ENTRY;
        if self.filter == text_type::CHAT_ENTRY
            && old | text_type::CHAT_ENTRY != text_type::CHAT_ENTRY
        {
            let usage = write(ui, &entry(ui, STRINGS, string::USAGE_MESSAGE));
            self.pending.push(Line::written(text_type::STANDARD, usage));
            self.filter |= text_type::STANDARD;
            self.all_text_to_log(ui);
            self.filter = text_type::CHAT_ENTRY;
        }
        if let Some(e) = self.entry {
            let kind = if self.devoted == text_type::SAY {
                text_type::CHAT_ENTRY
            } else {
                self.devoted
            };
            let info = self.message_info(kind).unwrap_or_default();
            ui.set_font_num_and_color(e, info.size, info.color);
        }
    }

    /// The window goes: its top-level element and its listening.
    // D2-03105
    fn close(self, ui: &mut Ui) {
        if let Some(r) = self.root {
            ui.frame_remove_root(self.frame, r);
            ui.remove(r);
        }
        ui.drop_frame(self.frame);
    }
}

/// The toolbar's chat panel.
#[derive(Debug)]
pub struct ChatUi {
    frame: FrameId,
    windows: Vec<ChatPane>,
    /// The window that had the keyboard last.
    last_focus: Option<usize>,
    /// The line of messages at the screen's top: the error lines waiting for the next tick, and
    /// the rows shown with the time each goes.
    spew_box: Option<Handle>,
    spew_pending: Vec<String>,
    spew_items: Vec<(Handle, f64)>,
    /// The options' revision the windows read last; the notices waiting on the next one.
    seen: u64,
    /// What the windows asked while heard between steps, for the next frame's requests.
    asked: Vec<Request>,
    reload_chat: Option<u64>,
    reload_options: Option<u64>,
    /// The lines that came after the scripts said the options changed, before the windows read
    /// them again: each window takes them by what it shows once it has.
    held: Vec<Line>,
    /// A reload still waited as the last frame ended.
    waiting: bool,
}

impl ChatUi {
    /// The windows were laid out anew from a layout file: each log shows its newest lines, unless
    /// locked.
    pub fn show_newest_lines(&self, ui: &mut Ui) {
        for w in &self.windows {
            w.show_newest_lines(ui);
        }
    }

    /// The chat panel under the toolbar's framework: the line of messages found, the clock and
    /// the interface keys heard, its windows made.
    // D2-02485
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let spew_box = ui.element(name::SPEW_BOX);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_global_message(l, global::TIME);
            ui.listen_for_global_message(l, global::KEY_DOWN);
        }
        let mut me = Self {
            frame,
            windows: Vec::new(),
            last_focus: None,
            spew_box,
            spew_pending: Vec::new(),
            spew_items: Vec::new(),
            seen: 0,
            asked: Vec::new(),
            reload_chat: None,
            reload_options: None,
            held: Vec::new(),
            waiting: false,
        };
        me.init(ui);
        me
    }

    /// The four windows, the first shown and the active one. (The lines about every object reach
    /// the windows: an object coming into the world asks nothing more of them.)
    // D2-02483, D2-03084
    fn init(&mut self, ui: &mut Ui) {
        for (i, (window, button)) in name::CHAT_WINDOWS.iter().zip(name::TOGGLES).enumerate() {
            let number = u32::try_from(i + 1).unwrap_or(1);
            let mut w = ChatPane::new(ui, self.frame, number, *window, button);
            if i == 0 {
                w.show(ui, true);
                w.set_active_window(ui, true);
                self.last_focus = Some(0);
            }
            self.windows.push(w);
        }
    }

    /// The windows.
    #[must_use]
    pub fn windows(&self) -> &[ChatPane] {
        &self.windows
    }

    /// The window that had the keyboard last.
    #[must_use]
    pub const fn last_focus(&self) -> Option<usize> {
        self.last_focus
    }

    /// Whether a line is being typed in one of the windows.
    #[must_use]
    pub fn typing(&self, ui: &Ui) -> bool {
        self.windows.iter().any(|w| w.is_active(ui))
    }

    /// The window whose entry's window had the keyboard.
    // D2-02470
    fn set_last_focus(&mut self, i: usize) {
        self.last_focus = Some(i);
    }

    /// The window a key is for: the shown one whose entry's window has the keyboard; else the one
    /// that had it last, if it shows; else the first that shows; else, for a key that starts
    /// typing (`show`), the one that had it last, shown.
    // D2-02472
    fn determine_target_window(&mut self, ui: &mut Ui, show: bool) -> Option<usize> {
        if let Some(i) = self
            .windows
            .iter()
            .position(|w| w.is_active_root(ui) && w.shown(ui))
        {
            return Some(i);
        }
        let last = self.last_focus?;
        if !self.windows[last].shown(ui) {
            if let Some(i) = self.windows.iter().position(|w| w.shown(ui)) {
                return Some(i);
            }
            if !show {
                return None;
            }
            self.windows[last].show(ui, true);
        }
        Some(last)
    }

    /// The windows' depths: the one typed in at `front`, the others behind it in their order.
    // D2-02473
    fn update_z_levels(&self, ui: &mut Ui, front: i32) {
        for (i, w) in self.windows.iter().enumerate() {
            let z = if w.is_active(ui) {
                front
            } else {
                i32::try_from(i).unwrap_or(0) + 1 + front
            };
            w.set_z_level(ui, z);
        }
    }

    /// One window active (its move bar says so) and the one that had the keyboard last, the
    /// others not active.
    // D2-02474
    fn set_active_window(&mut self, ui: &mut Ui, active: usize) {
        for (i, w) in self.windows.iter().enumerate() {
            w.set_active_window(ui, i == active);
        }
        if active < self.windows.len() {
            self.last_focus = Some(active);
        }
    }

    /// A tell to someone put in the window keys are for.
    // D2-02475
    pub fn do_assisted_tell(&mut self, ui: &mut Ui, target: &str) {
        if let Some(i) = self.determine_target_window(ui, false) {
            self.windows[i].do_assisted_tell(ui, target);
        }
    }

    /// A window's entry took the keyboard: the windows' depths with it in front; its label goes,
    /// or what it held is selected; it is the active window.
    // D2-03080
    fn handle_activation(&mut self, ui: &mut Ui, i: usize) {
        let front = self.windows[i].zlevel_front;
        self.update_z_levels(ui, front);
        let w = &mut self.windows[i];
        let Some(e) = w.entry else { return };
        if w.prepopulated {
            ui.clear_all_text(e);
            w.prepopulated = false;
        } else {
            ui.select_all(e);
        }
        self.set_active_window(ui, i);
    }

    /// An interface key, to the window it is for: `/` and `;` start a command or an alias, Enter
    /// starts or ends typing, the reply key starts a reply; the others as the window takes them.
    /// The keys that start typing show the window that had the keyboard last if none shows.
    // D2-02480
    fn dispatch_keypress_to_window(&mut self, ui: &mut Ui, k: u32, out: &mut Vec<Request>) {
        let show = matches!(
            k,
            key_id::USE | key::START_COMMAND | key::START_ALIAS | key::REPLY
        );
        let Some(i) = self.determine_target_window(ui, show) else {
            return;
        };
        match k {
            key::START_COMMAND | key::START_ALIAS => self.handle_command_or_alias_key(ui, i, k),
            key_id::USE => self.windows[i].handle_enter_key(ui, out),
            key::REPLY => self.handle_reply_key(ui, i),
            _ => {
                self.windows[i].key_press(ui, k, i, out);
            }
        }
    }

    /// What a window's listener heard so far, heard now: an element's messages reach their
    /// listeners as they are sent (the entry taking the keyboard is acted on before the next
    /// step).
    fn hear(&mut self, ui: &mut Ui, i: usize) {
        let mut out = Vec::new();
        for m in crate::camera::take(ui, self.windows[i].frame) {
            self.window_heard(ui, i, &m, &mut out);
        }
        self.asked.append(&mut out);
    }

    /// `/` or `;` pressed: the window's entry holds the command's or the alias's mark and takes
    /// the keyboard, the caret after the mark.
    // D2-03106
    fn handle_command_or_alias_key(&mut self, ui: &mut Ui, i: usize, k: u32) {
        let w = &mut self.windows[i];
        w.prepopulated = false;
        let mark = if k == key::START_COMMAND {
            string::COMMAND_PREFIX
        } else {
            string::ALIAS_PREFIX
        };
        let Some(e) = w.entry else { return };
        let text = write(ui, &entry(ui, STRINGS, mark));
        w.set_entry_text(ui, &text);
        ui.activate_element(e);
        self.hear(ui, i);
        ui.deselect(e);
    }

    /// The reply key: the window's entry takes the keyboard holding the reply command's mark and
    /// its first name (`/reply `).
    // D2-03113
    fn handle_reply_key(&mut self, ui: &mut Ui, i: usize) {
        let Some(e) = self.windows[i].entry else {
            return;
        };
        if self.windows[i].shown(ui) {
            ui.activate_element(e);
            self.hear(ui, i);
        }
        let names = write(ui, &entry(ui, COMMANDS, string::REPLY));
        let Some(first) = words(&names).first().map(|w| (*w).to_owned()) else {
            return;
        };
        let text = format!("{SLASH}{first}{SPACE}");
        self.windows[i].set_entry_text(ui, &text);
        ui.activate_element(e);
        self.hear(ui, i);
    }

    /// The key that starts and stops typing: the window being typed in stops; else the one that
    /// had the keyboard last shows, takes it, and is the active window.
    // D2-02481
    fn increment_activation(&mut self, ui: &mut Ui) {
        if let Some(w) = self
            .windows
            .iter_mut()
            .find(|w| w.is_active_root(ui) && w.shown(ui))
        {
            w.deactivate_entry(ui);
            return;
        }
        let Some(last) = self.last_focus else { return };
        if !self.windows[last].shown(ui) {
            self.windows[last].show(ui, true);
        }
        self.windows[last].activate_entry(ui);
        self.set_active_window(ui, last);
    }

    /// A line for the chat windows: an error line also goes to the line of messages.
    // D2-02484
    fn display_string_info(&mut self, kind: u32, text: &str) {
        if kind & text_type::ERROR != 0 {
            self.spew_pending.push(text.to_owned());
        }
    }

    /// The line of messages at the tick: the rows whose time is up go; each line waiting is a new
    /// row as wide as the line of messages and as tall as its text, to go after its `decay_time`
    /// (5 seconds unless the row says); the line shows its last row.
    // D2-02486
    fn update(&mut self, ui: &mut Ui) {
        let Some(list) = self.spew_box else { return };
        let now = ui.now();
        let mut changed = false;
        let mut i = 0;
        while i < self.spew_items.len() {
            let (el, until) = self.spew_items[i];
            if until <= now {
                if let Some(at) = ui.list_index_of(list, el) {
                    ui.list_multi_remove(list, at);
                }
                ui.remove(el);
                self.spew_items.swap_remove(i);
                changed = true;
            } else {
                i += 1;
            }
        }
        let pending = std::mem::take(&mut self.spew_pending);
        if !pending.is_empty() {
            let layout = ui
                .assets()
                .data_id_by_enum(SPEW_LAYOUT_ENUM, LAYOUT_MAPPING);
            let layout = ui.load_layout(layout);
            let width = ui.node(list).map_or(0, |n| n.rect.width());
            for text in pending {
                let Some(row) = layout.and_then(|l| ui.create(l, name::SPEW_BOX_MESSAGE)) else {
                    continue;
                };
                ui.set_text(row, text.trim_end());
                let h = ui.node(row).map_or(0, |n| n.rect.height());
                ui.resize_to(row, width, h);
                ui.recalculate_glyph_list(row);
                let paper = ui.text_state(row).map_or(h, |t| t.paper_h);
                ui.resize_to(row, width, paper);
                ui.list_multi_add(list, row);
                let decay = ui
                    .node(row)
                    .and_then(|n| n.float_attr(tok::DECAY_TIME))
                    .unwrap_or(5.0);
                self.spew_items.push((row, now + f64::from(decay)));
                changed = true;
            }
        }
        if changed {
            ui.list_changes_done(list);
            ui.list_end(list);
        }
    }

    /// The rows the line of messages shows, oldest first.
    #[must_use]
    pub fn messages(&self, ui: &Ui) -> Vec<String> {
        self.spew_box
            .map(|l| {
                ui.list_items(l)
                    .into_iter()
                    .filter_map(|r| ui.text(r))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A line asked of one of the lists, given back for the window that asked.
    pub fn history_item(&mut self, ui: &mut Ui, window: usize, list: History, text: &str) {
        if let Some(w) = self.windows.get_mut(window) {
            w.history_item(ui, list, text);
            self.hear(ui, window);
        }
    }

    /// What a window's listener heard.
    fn window_heard(&mut self, ui: &mut Ui, i: usize, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Show { shown } => self.windows[i].show(ui, shown),
            Mail::PlacesPutBack => self.windows[i].location_applied(ui),
            Mail::Global {
                what: global::TIME, ..
            } => self.windows[i].tick(ui),
            Mail::Element {
                from,
                element,
                what,
                ..
            } => self.window_message(ui, i, from, element, what, out),
            Mail::Global { .. } => {}
        }
    }

    /// A message to a window: while its entry's window has the keyboard it is the last focus.
    /// Its button shows and hides it; its move bar pressed makes it the active window, let go
    /// keeps the places; its lock locks and unlocks the log; its options button shows and hides
    /// the opacity slider; its entry taking the keyboard (the window active) and giving it up (a
    /// devoted window's label); its scroll bar pressed holds the log still until let go or
    /// stepped; the opacity slider moved fades it, let go keeps the opacity.
    // D2-03099
    fn window_message(
        &mut self,
        ui: &mut Ui,
        i: usize,
        from: Handle,
        element: Token,
        what: Token,
        out: &mut Vec<Request>,
    ) {
        if self.windows[i].root.is_none() {
            return;
        }
        if self.windows[i].is_active_root(ui) {
            self.set_last_focus(i);
        }
        let w = &self.windows[i];
        if Some(from) == w.root && what == tok::LOCATION_CHANGED {
            return;
        }
        if element == w.button_name {
            if w.button_name != Token(0) {
                let shown = w.shown(ui);
                self.windows[i].show(ui, !shown);
            }
            return;
        }
        if element == name::MOVE_BAR {
            if what == tok::MOUSE_PRESS && Some(from) == w.move_bar {
                self.set_active_window(ui, i);
            }
            if what == tok::MOUSE_RELEASE && Some(from) == self.windows[i].move_bar {
                out.push(Request::SaveLocations);
            }
        }
        let w = &mut self.windows[i];
        if Some(from) == w.lock && what == tok::MOUSE_PRESS {
            w.perm_lock = !w.perm_lock;
            let state = if w.perm_lock {
                Some((true, tok::LOCKED))
            } else if w.temp_lock {
                None
            } else {
                Some((false, tok::UNLOCKED))
            };
            if let Some((locked, state)) = state {
                if let Some(l) = w.log {
                    ui.set_attr(l, tok::TEXT_LOCKED, Value::Bool(locked));
                }
                if let Some(b) = w.lock {
                    ui.set_state(b, state);
                }
            }
        }
        if Some(from) == w.options_button {
            if what == tok::MOUSE_RELEASE_ON {
                if let Some(f) = w.options_field {
                    let visible = ui.node(f).is_some_and(|n| n.visible);
                    ui.set_visible(f, !visible);
                }
            }
            return;
        }
        if Some(from) == w.entry {
            if what == tok::ACTIVATED {
                self.handle_activation(ui, i);
            }
            if what == tok::DEACTIVATED {
                self.windows[i].label_devoted(ui);
            }
            return;
        }
        if Some(from) == w.scrollbar {
            if what == tok::MOUSE_PRESS {
                w.temp_lock = true;
                if let Some(l) = w.log {
                    ui.set_attr(l, tok::TEXT_LOCKED, Value::Bool(true));
                }
            }
            if what == tok::MOUSE_RELEASE
                || what == tok::VERTICAL_LINE_DECREMENT
                || what == tok::VERTICAL_LINE_INCREMENT
            {
                w.temp_lock = false;
                if !w.perm_lock {
                    if let Some(l) = w.log {
                        ui.set_attr(l, tok::TEXT_LOCKED, Value::Bool(false));
                    }
                }
            }
            return;
        }
        if Some(from) == w.opacity_bar {
            let y = ui
                .node(from)
                .and_then(|n| n.float_attr(tok::Y_POSITION))
                .unwrap_or(0.0);
            let opacity = y * (1.0 - 0.0) + 0.0;
            if let Some(r) = w.root {
                ChatPane::set_opacity(ui, r, opacity);
            }
            if what == tok::WIDGET_RELEASE || what == tok::MOUSE_RELEASE {
                out.push(Request::Chat(Ask::Opacity {
                    window: w.window,
                    opacity,
                }));
            }
        }
    }

    /// The windows read the options again for the notices that wait, once the client has read
    /// them since the notice came.
    fn reload(&mut self, ui: &mut Ui, view: &View<'_>) {
        let Some(s) = view.chat else { return };
        if self.reload_options.is_some_and(|r| s.revision > r) {
            self.reload_options = None;
            for w in &mut self.windows {
                w.reload_options(ui, s);
            }
        }
        if self.reload_chat.is_some_and(|r| s.revision > r) {
            self.reload_chat = None;
            for w in &mut self.windows {
                w.reload_chat_settings(ui, s);
            }
        }
        self.seen = s.revision;
    }
}

impl ChatUi {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x1_040B, 0x419, 0x42A, 0x1_040C, 0x468, 0x417];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for ChatUi {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden: the windows by their own flags; shown again, the line of messages
    /// brought up to date.
    // D2-02487
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        let was = ui.frame_shown(self.frame);
        ui.frame_show(self.frame, on);
        for i in 0..self.windows.len() {
            for m in crate::camera::take(ui, self.windows[i].frame) {
                self.window_heard(ui, i, &m, out);
            }
        }
        let now = ui.frame_shown(self.frame);
        if was != now && now {
            self.update(ui);
        }
    }

    /// While the panel shows: the windows' keys show and hide them; the key that starts and
    /// stops typing; the other keys to the window they are for; the clock, for the line of
    /// messages.
    // D2-02488
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        if !ui.frame_shown(self.frame) {
            return;
        }
        match *mail {
            Mail::Global {
                what: global::KEY_DOWN,
                param,
            } => {
                if let Some(n) = key::CHAT_WINDOW.iter().position(|k| *k == param) {
                    if n < self.windows.len() {
                        let shown = self.windows[n].shown(ui);
                        self.windows[n].show(ui, !shown);
                    }
                } else if param == key::INCREMENT_ACTIVATION {
                    self.increment_activation(ui);
                } else {
                    self.dispatch_keypress_to_window(ui, param, out);
                }
            }
            Mail::Global {
                what: global::TIME, ..
            } => self.update(ui),
            _ => {}
        }
    }

    /// The chat options or the player's options changed (the player's options with the chat's
    /// read again first): the windows read them again once the client has. A tell to someone
    /// goes in the entry of the window keys are for.
    fn notice(&mut self, ui: &mut Ui, n: &Notice, _out: &mut Vec<Request>) {
        match n.native {
            notice::DO_ASSISTED_TELL => {
                let target = n.records.iter().flatten().find_map(|r| match r {
                    Record::Text(t) => ui.assets().write(t),
                    _ => None,
                });
                if let Some(target) = target {
                    self.do_assisted_tell(ui, &target);
                }
            }
            notice::RELOAD_CHAT_SETTINGS => self.reload_chat = Some(self.seen),
            crate::gameplay::notice::RELOAD_OPTIONS => self.reload_options = Some(self.seen),
            _ => {}
        }
    }

    /// The lines the game shows, to the panel and each window; the options read again where
    /// they wait; then what the windows heard.
    ///
    /// The scripts change the player's options and write lines in one go (as a character enters:
    /// its options, then the lines about the services it could not reach), and the windows read
    /// the options as the scripts left them: a line that comes while the options wait to be read
    /// again is held until they have been, and each window then takes it by what it shows.
    ///
    /// A line written before those options were set is taken at once, by what the windows show
    /// now: as a character enters, that is nothing yet.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        let before = if self.waiting {
            0
        } else {
            view.lines_before_options
        };
        for (i, (kind, text)) in view.lines.iter().enumerate() {
            self.display_string_info(*kind, text);
            let line = Line {
                kind: *kind,
                text: text.clone(),
                info: view.line_infos.get(i).cloned().flatten(),
            };
            if i < before {
                for w in &mut self.windows {
                    w.display_string_info(&line);
                }
            } else {
                self.held.push(line);
            }
        }
        self.reload(ui, view);
        self.waiting = self.reload_chat.is_some() || self.reload_options.is_some();
        if !self.waiting {
            for line in std::mem::take(&mut self.held) {
                for w in &mut self.windows {
                    w.display_string_info(&line);
                }
            }
        }
        for i in 0..self.windows.len() {
            for m in crate::camera::take(ui, self.windows[i].frame) {
                self.window_heard(ui, i, &m, out);
            }
        }
        out.append(&mut self.asked);
    }

    /// The panel goes: its windows and the line of messages' rows.
    // D2-02471
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        for w in self.windows.drain(..) {
            w.close(ui);
        }
        if let Some(list) = self.spew_box {
            for (el, _) in self.spew_items.drain(..) {
                if let Some(at) = ui.list_index_of(list, el) {
                    ui.list_remove(list, at);
                }
                ui.remove(el);
            }
        }
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
