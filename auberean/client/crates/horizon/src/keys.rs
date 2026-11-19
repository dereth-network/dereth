//! The keys' actions Horizon carries out.
//!
//! The keys are the shell's: while Horizon is shown its key layers are in front of the player's
//! maps (`auberean_shell::interface_keys`), and the world's controls carry out what the keys ask
//! of the game (moving, the camera, selecting and using). The interface's actions the keys begin
//! come to Horizon in [`auberean_shell::interface::Game::keyed`], and Horizon carries out those
//! it has something for, below; its one action of its own opens the crafting window.
//!
//! Besides them, two keys are the interface's own while it holds something open: every key while
//! the chat entry has them, and Escape while the settings window is open, which closes it.

/// The game's interface actions Horizon carries out, by their numbers in the game's action map.
pub mod action {
    /// Enter or leave the chat entry.
    pub const CHAT_ENTRY: u32 = 0x4000_0017;
    /// The options panel: Horizon's settings.
    pub const OPTIONS: u32 = 0x4100_0004;
    /// Log out.
    pub const LOG_OUT: u32 = 0x4100_000E;
    /// Start a slash command in the chat entry.
    pub const SLASH_COMMAND: u32 = 0x4100_0010;
    /// Start a reply to the last tell in the chat entry.
    pub const REPLY: u32 = 0x4100_0049;
    /// Save a screenshot.
    pub const SCREENSHOT: u32 = 0x4100_0017;
    /// The examination window, the skill window.
    pub const EXAMINATION: u32 = 0x4100_0001;
    pub const SKILLS: u32 = 0x4100_0006;
    /// The inventory window.
    pub const INVENTORY: u32 = 0x4100_0002;
    /// Start or stop attacking.
    pub const AUTOATTACK: u32 = 0x4100_0013;
    /// Use slots 1 to 10 of the set shown; of the row under it.
    pub const SHORTCUT_1: u32 = 0x4100_002B;
    pub const SHORTCUT_0: u32 = 0x4100_0034;
    pub const SECOND_ROW_1: u32 = 0x4100_005F;
    pub const SECOND_ROW_0: u32 = 0x4100_0068;
    /// Show sets 1 to 10; the next set, the one before.
    pub const SET_1: u32 = 0x4100_0035;
    pub const SET_0: u32 = 0x4100_003E;
    pub const SET_NEXT: u32 = 0x4100_003F;
    pub const SET_PREVIOUS: u32 = 0x4100_0040;
    /// The examination's next tab, the one before.
    pub const NEXT_TAB: u32 = 0x4100_006A;
    pub const PREVIOUS_TAB: u32 = 0x4100_006B;
    /// The map window, the quest window: Horizon's map and journal.
    pub const MAP: u32 = 0x4100_0003;
    pub const QUESTS: u32 = 0x4100_0005;
    /// The social window.
    pub const SOCIAL: u32 = 0x4100_0007;
    /// Select the party list's first to ninth fellow.
    pub const FELLOW_1: u32 = 0x4100_0053;
    pub const FELLOW_9: u32 = 0x4100_005B;
    /// Show the first to the fourth chat window: their tabs of the log window.
    pub const CHAT_WINDOW_1: u32 = 0x4100_0041;
    pub const CHAT_WINDOW_4: u32 = 0x4100_0044;
    /// The crafting window: this client's own action.
    pub const CRAFTING: u32 = auberean_shell::interface_keys::own::CRAFTING;
}

/// What Horizon does for an interface action a key began.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyed {
    /// Open the chat entry, empty or with a slash in it.
    Chat {
        slash: bool,
    },
    /// Open the chat entry with the reply command in it.
    Reply,
    /// Open or close the settings.
    Settings,
    LogOut,
    Screenshot,
    /// Use a slot of the shortcut bar: its row (0 the set shown) and its place in the row.
    Shortcut {
        row: u32,
        column: u32,
    },
    /// Show a set of the shortcut bar (from 1); the next or the one before.
    Set(u32),
    StepSet {
        next: bool,
    },
    /// Start attacking, or stop.
    Attack,
    /// Open or close the skill window, the examine window.
    Skills,
    Examine,
    /// Open or close the inventory window.
    Inventory,
    /// The examine window's next tab, or the one before.
    StepTab {
        next: bool,
    },
    /// Open or close the map, the journal.
    Map,
    Journal,
    /// Open or close the social window.
    Social,
    /// Select a fellow of the party list, by its place (from 0).
    Fellow(usize),
    /// Open or close the crafting window.
    Crafting,
    /// Show a chat window's tab of the log window (from 0), or the first again when it is shown.
    ChatTab(usize),
}

/// What Horizon does for `action`, begun by a key; none for an action it has nothing for (the
/// world's controls carry out the game's own).
#[must_use]
pub const fn keyed(action: u32) -> Option<Keyed> {
    Some(match action {
        action::CHAT_ENTRY => Keyed::Chat { slash: false },
        action::SLASH_COMMAND => Keyed::Chat { slash: true },
        action::REPLY => Keyed::Reply,
        action::OPTIONS => Keyed::Settings,
        action::LOG_OUT => Keyed::LogOut,
        action::SCREENSHOT => Keyed::Screenshot,
        action::EXAMINATION => Keyed::Examine,
        action::SKILLS => Keyed::Skills,
        action::INVENTORY => Keyed::Inventory,
        action::AUTOATTACK => Keyed::Attack,
        action::SHORTCUT_1..=action::SHORTCUT_0 => Keyed::Shortcut {
            row: 0,
            column: action - action::SHORTCUT_1,
        },
        action::SECOND_ROW_1..=action::SECOND_ROW_0 => Keyed::Shortcut {
            row: 1,
            column: action - action::SECOND_ROW_1,
        },
        action::SET_1..=action::SET_0 => Keyed::Set(action - action::SET_1 + 1),
        action::SET_NEXT => Keyed::StepSet { next: true },
        action::SET_PREVIOUS => Keyed::StepSet { next: false },
        action::NEXT_TAB => Keyed::StepTab { next: true },
        action::PREVIOUS_TAB => Keyed::StepTab { next: false },
        action::MAP => Keyed::Map,
        action::QUESTS => Keyed::Journal,
        action::SOCIAL => Keyed::Social,
        action::CRAFTING => Keyed::Crafting,
        action::FELLOW_1..=action::FELLOW_9 => Keyed::Fellow((action - action::FELLOW_1) as usize),
        action::CHAT_WINDOW_1..=action::CHAT_WINDOW_4 => {
            Keyed::ChatTab((action - action::CHAT_WINDOW_1) as usize)
        }
        _ => return None,
    })
}
