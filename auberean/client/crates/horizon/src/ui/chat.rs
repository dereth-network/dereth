//! The log window: Dereth's Horizon's own ([`dereth_ui_kit::chat`]), with the game's four chat
//! windows as its tabs, newest line at the foot, each line in its kind of text's color, and the
//! chat entry under them while it is open.
//!
//! **The pane** is one translucent ground with no frame, the tabs along its top, the lines under
//! them and a thin scrollbar at their right; closed, it shows only the log. A small mark in its
//! bottom right corner sizes it. How opaque it is, active (under the pointer, or its line being
//! typed) and not, is set on the chat page of the settings, and kept with its size in Horizon's
//! own settings ([`crate::own`]).
//!
//! **The tabs.** Each is one of the game's chat windows, named Main, General, Fellowship and
//! Allegiance unless the player named it on the chat page of the settings, and shows what that
//! window's filter lets through (and, the first, what the plugins print). The filters are the
//! player's chat options, kept by the server; until they are told, each window shows what a new
//! character's does. A click on a tab shows it, and so do the chat windows' keys (Shift+F2 to F4
//! show the second to the fourth, and again the first).
//!
//! **A tab's menu**: a right click on a tab shows it and opens its menu, titled Filters: a box
//! for each kind of text its window shows, each set sent to the server to keep, and beside the
//! title Pop Out, which takes the tab out into a window of its own (not the first, which stays).
//! A tab popped out is moved by dragging its tab and sized by its corner, and Horizon keeps where
//! it was put and how large it was made; its menu's Dock puts it back among the tabs.
//!
//! **The entry** names the channel it speaks on before the field, in the channel's color, and
//! keeps it for the lines after. A click on the name lists the channels to pick from, those the
//! player may not use now dimmed; Tab steps to the next one the player may use; a channel's
//! command typed at the start of the line (`/cg `, `/f `, `/a `, `/s `...) switches to it and is
//! taken off the line, and the command sent alone switches and sends nothing. A line that is a
//! command of its own (a tell, a reply, an emote) is named for it while it is typed and sent as
//! typed. Enter (or `/`, which starts the line with one) opens it, on what was typed when it last
//! closed; Enter sends to the player's command line; Escape, or a press elsewhere, closes it and
//! keeps what was typed. Up and Down step through the lines sent before, as the game's command
//! list answers them.

use dereth_ui_kit::chat::{ChatEvent, ChatGame, EntryAct, FocusItem, Window};
use dereth_ui_kit::hud;
use dereth_ui_kit::paint::Painter;
use dereth_ui_kit::widgets::Ctx;

use auberean_client_sdk::contract::ChatWindowAction;
use auberean_client_sdk::Action;

use crate::draw::{Argb, Rect};
use crate::own::Own;
use crate::state::{GameState, Line};
use crate::ui::Outcome;

/// The entry's ground, and the rule along its top that sets it apart from the lines.
pub use dereth_ui_kit::hud::{ENTRY_GROUND, ENTRY_RULE};

/// The pane's ground under the tabs, at its first opacity.
#[must_use]
pub fn ground() -> Argb {
    hud::log_ground(hud::LOG_OPACITY)
}

/// The plain scrollbar's thumb, where the art has no scrollbar of its own.
pub const SCROLL_THUMB: Argb = 0xC0C8_B27A;

/// The log window's widest and narrowest at its default size, in layout units.
pub const WIDEST: f32 = 480.0;
pub const NARROWEST: f32 = 300.0;

/// The log window's height at its default size, its tabs and its entry's room included, in layout
/// units.
pub const HEIGHT: f32 = 230.0;

/// How many chat windows the game has, and so tabs the log window.
pub const WINDOWS: usize = 4;

/// The tabs' names until the player names them.
pub const DEFAULT_TAB_NAMES: [&str; WINDOWS] = ["Main", "General", "Fellowship", "Allegiance"];

/// The words of a tab's menu's button for a tab in the log window and for one popped out.
pub const POP_OUT: &str = "Pop Out";
pub const DOCK: &str = "Dock";

/// The kinds of text that are channels, by their bits.
pub mod kind {
    pub const SAY: u32 = 0x10;
    pub const TELL: u32 = 0x20;
    pub const EMOTE: u32 = 0x40;
    pub const FELLOWSHIP: u32 = 0x1_0000;
    pub const ALLEGIANCE: u32 = 0x2_0000;
    pub const GENERAL: u32 = 0x8_0000;
    pub const TRADE: u32 = 0x10_0000;
    pub const REGION: u32 = 0x20_0000;
    pub const KINGDOM: u32 = 0x40_0000;
    pub const PVP: u32 = 0x100_0000;
}

/// What each chat window shows as a new character's options have them: the first local talk
/// (errors, combat, the administrators, the game's messages, sayings, tells, emotes, broadcasts,
/// the region and the kingdom), the others general chat, the fellowship and the allegiance.
pub const DEFAULT_FILTERS: [u32; WINDOWS] =
    [LOCAL, kind::GENERAL, kind::FELLOWSHIP, kind::ALLEGIANCE];

/// The first window's kinds: the local ones (errors, combat, the administrators, the game's
/// messages, broadcasts), sayings, tells and emotes, the region and the kingdom.
const LOCAL: u32 = 0x10F | kind::SAY | kind::TELL | kind::EMOTE | kind::REGION | kind::KINGDOM;

/// A channel the entry can speak on: its kind of text, its name on the entry and in its list, and
/// the game's commands that speak on it, the first the one put before a line said there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channel {
    pub kind: u32,
    pub name: &'static str,
    pub commands: &'static [&'static str],
}

/// The channels, in the order Tab steps through them and the entry's list shows them; the last,
/// the player-killers', is never spoken on from the entry.
pub const CHANNELS: [Channel; 8] = [
    Channel {
        kind: kind::SAY,
        name: "Chat",
        commands: &["say", "s", "c"],
    },
    Channel {
        kind: kind::FELLOWSHIP,
        name: "Fellowship",
        commands: &["f", "group", "g"],
    },
    Channel {
        kind: kind::ALLEGIANCE,
        name: "Allegiance",
        commands: &["a", "gu", "guild", "as", "o"],
    },
    Channel {
        kind: kind::KINGDOM,
        name: "Kingdom",
        commands: &["ck"],
    },
    Channel {
        kind: kind::GENERAL,
        name: "General",
        commands: &["cg"],
    },
    Channel {
        kind: kind::TRADE,
        name: "Trade",
        commands: &["ct", "auction"],
    },
    Channel {
        kind: kind::REGION,
        name: "Region",
        commands: &["cr", "broadcast", "br", "y", "yell", "shout"],
    },
    Channel {
        kind: kind::PVP,
        name: "PvP",
        commands: &["cp", "cpk", "cpvp"],
    },
];

/// The channel of kind `kind`.
#[must_use]
pub fn channel(kind: u32) -> Option<&'static Channel> {
    CHANNELS.iter().find(|c| c.kind == kind)
}

/// The name tab `tab` (0 the first) shows: the one the player `typed`, else its own.
#[must_use]
pub fn tab_label(typed: &str, tab: usize) -> &str {
    match typed.trim() {
        "" => DEFAULT_TAB_NAMES.get(tab).copied().unwrap_or(""),
        t => t,
    }
}

/// The channel a line's opening command switches to, and the rest of the line: `/cg hello` is
/// General and `hello`. A command that is not a channel's, or one with nothing typed after its
/// space yet, switches nothing.
#[must_use]
pub fn channel_command(text: &str) -> Option<(u32, &str)> {
    let rest = text.strip_prefix('/')?;
    let (word, after) = rest.split_once(' ')?;
    CHANNELS
        .iter()
        .find(|c| c.commands.iter().any(|n| n.eq_ignore_ascii_case(word)))
        .map(|c| (c.kind, after))
}

/// What the entry names before the field while `text` is typed on `mode`: the command the line
/// starts with (a tell, a reply, an emote, a channel's, another command), else the channel, each in
/// its color.
#[must_use]
pub fn entry_label(text: &str, mode: u32) -> (&'static str, Argb) {
    let Some(rest) = text.strip_prefix('/') else {
        let c = channel(mode).unwrap_or(&CHANNELS[0]);
        return (c.name, colour(c.kind));
    };
    let word = rest
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match word.as_str() {
        "" => ("Command", 0xFFCC_CCCC),
        "tell" | "t" | "send" | "reply" | "r" | "retell" | "rt" => ("Tell", colour(kind::TELL)),
        "emote" | "em" | "me" | "e" => ("Emote", colour(kind::EMOTE)),
        w => CHANNELS
            .iter()
            .find(|c| c.commands.contains(&w))
            .map_or(("Command", 0xFFCC_CCCC), |c| (c.name, colour(c.kind))),
    }
}

/// The line the command line is given for `text` typed on `mode`: as typed for a command (or an
/// alias, `;`) or on the sayings, else with the channel's command before it; none for a blank line.
#[must_use]
pub fn outgoing(text: &str, mode: u32) -> Option<String> {
    let line = text.trim();
    if line.is_empty() {
        return None;
    }
    if line.starts_with('/') || line.starts_with(';') || mode == kind::SAY {
        return Some(text.to_owned());
    }
    Some(match channel(mode) {
        Some(c) => format!("/{} {line}", c.commands[0]),
        None => text.to_owned(),
    })
}

/// Whether a line of kind `line` shows in the chat window `window` (0 the first), whose filter is
/// `filter`: what its filter holds, and in the first what the plugins print.
#[must_use]
pub fn shows_in(window: usize, filter: u32, line: u32) -> bool {
    line & filter != 0 || (window == 0 && line == crate::state::PLUGIN_TEXT)
}

/// Each chat window's filter, as the player's options have them, else a new character's.
#[must_use]
pub fn filters(state: &GameState) -> [u32; WINDOWS] {
    state
        .chat_options
        .as_ref()
        .map_or(DEFAULT_FILTERS, |o| o.filters)
}

/// Whether the player may speak on channel `kind` now: the fellowship only in one, the kingdom
/// only in one, the allegiance unless the game has said the player is in none, never the
/// player-killers'.
#[must_use]
pub fn usable(state: &GameState, kind: u32) -> bool {
    let social = &state.social;
    match kind {
        kind::FELLOWSHIP => social.party.is_some(),
        kind::KINGDOM => social.kingdom.is_some(),
        kind::ALLEGIANCE => social
            .allegiance
            .as_ref()
            .is_none_or(|a| a.member.is_some()),
        kind::PVP => false,
        k => channel(k).is_some(),
    }
}

/// The channel after `mode` the player may use now, in [`CHANNELS`]' order, wrapping; none when
/// there is no other.
#[must_use]
pub fn next_mode(state: &GameState, mode: u32) -> Option<u32> {
    let at = CHANNELS.iter().position(|c| c.kind == mode).unwrap_or(0);
    (1..=CHANNELS.len())
        .map(|step| CHANNELS[(at + step) % CHANNELS.len()].kind)
        .find(|k| *k != mode && usable(state, *k))
}

/// The kinds of text, by their bits, and the color each is drawn in, the first that a line's kind
/// has deciding.
const COLOURS: &[(u32, Argb)] = &[
    (crate::state::PLUGIN_TEXT, 0xFF9C_E6C4), // what the plugins print
    (0x1, 0xFFFF_6A5A),                       // errors
    (0x2, 0xFFFF_B060),                       // combat
    (0x4, 0xFFFF_E060),                       // the game's administrators
    (0x10, 0xFFFF_FFFF),                      // sayings
    (0x20, 0xFFFF_B8DE),                      // tells
    (0x40, 0xFFC8_C8FF),                      // emotes
    (0x80, 0xFFB4_B0A8),                      // the log
    (0x100, 0xFFFF_D070),                     // broadcasts
    (0x1_0000, 0xFF66_E5FF),                  // the fellowship
    (0x2_0000, 0xFF90_F090),                  // the allegiance
    (0x8_0000, 0xFFD4_FF7D),                  // the general channel
    (0x10_0000, 0xFFD4_FF7D),                 // the trade channel
    (0x20_0000, 0xFFD4_FF7D),                 // the region channel
    (0x40_0000, 0xFFAB_DBE5),                 // the kingdom channel
    (0x100_0000, 0xFFFF_8080),                // the player-killer channel
];

/// The color a line of `kind` is drawn in.
#[must_use]
pub fn colour(kind: u32) -> Argb {
    COLOURS
        .iter()
        .find(|(bit, _)| kind & bit != 0)
        .map_or(0xFFE8_E0D0, |(_, c)| *c)
}

/// The game as the log window reads it: this frame's state, Horizon's own settings and the
/// entry's channel.
struct Game<'a> {
    state: &'a GameState,
    own: &'a Own,
    mode: u32,
}

impl ChatGame for Game<'_> {
    type Line = Line;

    fn windows(&self) -> Vec<Window> {
        filters(self.state)
            .iter()
            .zip(1..)
            .map(|(filter, id)| Window {
                id,
                filter: u64::from(*filter),
            })
            .collect()
    }

    fn text<'l>(&self, line: &'l Line) -> &'l str {
        &line.text
    }

    fn colour(&self, line: &Line) -> Argb {
        colour(line.kind)
    }

    fn shows(&self, window: &Window, line: &Line) -> bool {
        let at = usize::try_from(window.id.saturating_sub(1)).unwrap_or(0);
        let filter = u32::try_from(window.filter).unwrap_or(u32::MAX);
        shows_in(at, filter, line.kind)
    }

    fn tab_name(&self, slot: usize) -> String {
        let typed = self.own.tab_names.get(slot).map_or("", String::as_str);
        tab_label(typed, slot).to_owned()
    }

    fn opacity(&self, slot: Option<usize>, active: bool) -> f32 {
        let (on, off) = self.own.chat_opacity[slot.unwrap_or(0).min(WINDOWS - 1)];
        if active {
            on
        } else {
            off
        }
    }

    fn focus_line(&self) -> (String, Argb) {
        let c = channel(self.mode).unwrap_or(&CHANNELS[0]);
        (c.name.to_owned(), colour(c.kind))
    }

    fn focus_menu(&self) -> Vec<FocusItem> {
        // The player-killers' channel is never spoken on from the entry, so it is not listed.
        CHANNELS
            .iter()
            .filter(|c| c.kind != kind::PVP)
            .map(|c| FocusItem {
                focus: c.kind,
                label: c.name.to_owned(),
                colour: colour(c.kind),
                usable: usable(self.state, c.kind),
            })
            .collect()
    }

    fn focus_usable(&self, focus: u32) -> bool {
        usable(self.state, focus)
    }

    fn next_focus(&self) -> Option<u32> {
        next_mode(self.state, self.mode)
    }

    fn focus_command<'t>(&self, text: &'t str) -> Option<(u32, &'t str)> {
        channel_command(text)
    }

    fn channel_label(&self, text: &str, focus: (&str, Argb)) -> (String, Argb) {
        if text.starts_with('/') {
            let (label, c) = entry_label(text, self.mode);
            (label.to_owned(), c)
        } else {
            (focus.0.to_owned(), focus.1)
        }
    }

    fn filter_groups(&self, _id: u32) -> Vec<(String, u64)> {
        crate::ui::options::CHAT_KINDS
            .iter()
            .map(|(bit, name)| ((*name).to_owned(), u64::from(*bit)))
            .collect()
    }

    fn filters_known(&self) -> bool {
        self.state.chat_options.is_some()
    }
}

/// The log window and its entry.
#[derive(Debug)]
pub struct Chat {
    log: dereth_ui_kit::chat::ChatLog<Line, WINDOWS>,
    /// The channel the entry speaks on, kept from line to line.
    mode: u32,
    /// How many lines had come to the log when it last took them.
    heard: u64,
    /// Horizon's own settings were changed since this was last asked: where a window was put or
    /// how large it was made.
    kept: bool,
    /// The window as last drawn, its entry's room included, before the HUD layout placed it.
    drawn: Option<Rect>,
}

impl Default for Chat {
    fn default() -> Self {
        Self {
            log: dereth_ui_kit::chat::ChatLog::default(),
            mode: kind::SAY,
            heard: 0,
            kept: false,
            drawn: None,
        }
    }
}

impl std::ops::Deref for Chat {
    type Target = dereth_ui_kit::chat::ChatLog<Line, WINDOWS>;
    fn deref(&self) -> &Self::Target {
        &self.log
    }
}

impl std::ops::DerefMut for Chat {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.log
    }
}

impl Chat {
    /// Open the entry with `text` in it.
    pub fn open(&mut self, text: &str) {
        self.log.open(text.to_owned());
    }

    /// The chat entry's key: open the entry on what was typed when it last closed (`slash`, on a
    /// slash, starting a command). The entry already open stays as it is.
    pub fn begin(&mut self, slash: bool) {
        if slash {
            self.log.open_command();
        } else if !self.log.typing() {
            let kept = self.log.told().to_owned();
            self.log.open(kept);
        }
    }

    /// What is typed in the entry; empty while it is closed.
    #[must_use]
    pub fn text(&self) -> &str {
        self.log.typed().unwrap_or("")
    }

    /// The channel the entry speaks on.
    #[must_use]
    pub const fn mode(&self) -> u32 {
        self.mode
    }

    /// A chat window's key: show its tab, or the first again when it is the one shown.
    pub fn window_key(&mut self, tab: usize) {
        let to = if tab == self.log.shown_tab() { 0 } else { tab };
        self.log.show_tab(to.min(WINDOWS - 1));
    }

    /// What the log window draws over the rest of the HUD, as drawn last frame: the tabs popped
    /// out and the open menu. What is under them does not have the pointer.
    #[must_use]
    pub fn over_hud(&self) -> Vec<Rect> {
        let mut out = self.log.popped_rects();
        out.extend(self.log.menu_rect());
        out
    }

    /// Whether Horizon's own settings were changed since this was last asked.
    pub fn take_kept(&mut self) -> bool {
        std::mem::take(&mut self.kept)
    }

    /// Where the window is on a screen drawn at `k` pixels a layout unit, at its default size: at
    /// the bottom left, as wide as it can be up to [`WIDEST`] and still keep clear of the
    /// parameter bar and the hotbar, though never narrower than [`NARROWEST`]. The room at its
    /// foot is the entry's, shown only while it is open.
    #[must_use]
    pub fn rect(screen: (f32, f32), k: f32) -> Rect {
        let x = 16.0 * k;
        let right = crate::ui::hud::gauge_rects(screen, k)[0]
            .x
            .min(crate::ui::hotbar::slot_rect(screen, k, 0, 0).x)
            - 12.0 * k;
        let w = (right - x).clamp(NARROWEST * k, WIDEST * k);
        Rect::new(x, screen.1 - (HEIGHT + 16.0) * k, w.floor(), HEIGHT * k)
    }

    /// The log's new lines, the game's answers to Up and Down, and Horizon's own settings for the
    /// windows' places and sizes.
    fn follow(&mut self, state: &GameState, own: &Own) {
        let new = usize::try_from(state.said.saturating_sub(self.heard)).unwrap_or(usize::MAX);
        self.heard = state.said;
        let from = state.log.len().saturating_sub(new);
        self.log.take(false, state.log.iter().skip(from).cloned());
        for recalled in state.recalled.iter().flatten() {
            self.log.entry_told(recalled, false);
        }
        self.log.sizes_saved[0] = own.chat_size;
        for tab in 1..WINDOWS {
            self.log.popped_saved[tab] = own.chat_popped[tab];
            self.log.sizes_saved[tab] = own.chat_popped_size[tab];
        }
    }

    /// Draw the window; where it shows. The tabs popped out and the open menu are drawn over the
    /// rest of the HUD ([`Self::overlays`]).
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        own: &mut Own,
        out: &mut Outcome,
    ) -> Rect {
        self.follow(state, own);
        let home = Self::rect(p.screen, p.scale);
        let game = Game {
            state,
            own,
            mode: self.mode,
        };
        let mut asked = Vec::new();
        let shown = self.log.frame(p, ctx, &game, home, &mut asked);
        // The lines reach the window's foot, the entry's room included.
        let lines = self.log.dock;
        self.drawn = Some(Rect::new(
            shown.x,
            shown.y,
            shown.w,
            lines.bottom() - shown.y,
        ));
        self.carry_out(asked, own, out);
        shown
    }

    /// Where the window was last drawn, as large as the player made it and its entry's room
    /// included, before the HUD layout placed it; never drawn, where it stands at its default size
    /// ([`Self::rect`]).
    #[must_use]
    pub fn window(&self, screen: (f32, f32), k: f32) -> Rect {
        self.drawn.unwrap_or_else(|| Self::rect(screen, k))
    }

    /// Where the HUD layout put the window just drawn: `place` maps a rect as it was drawn to
    /// where it shows.
    pub fn place(&mut self, place: impl Fn(Rect) -> Rect) {
        self.log.dock = place(self.log.dock);
    }

    /// The tabs popped out and the open menu, over the rest of the HUD, on a screen where the log
    /// window is placed as [`Self::place`] said.
    pub fn overlays(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        own: &mut Own,
        out: &mut Outcome,
    ) {
        let game = Game {
            state,
            own,
            mode: self.mode,
        };
        let mut asked = Vec::new();
        self.log.overlays(p, ctx, &game, &mut asked);
        self.carry_out(asked, own, out);
    }

    /// What the log window asked: the entry's channel and what it keeps typed, the line said, the
    /// lines asked for, a window's filter sent to the server to keep, and the windows' places and
    /// sizes kept in Horizon's own settings.
    fn carry_out(&mut self, asked: Vec<ChatEvent>, own: &mut Own, out: &mut Outcome) {
        for event in asked {
            match event {
                // What is typed is kept by the log window itself, for the next opening.
                ChatEvent::Entry { act, .. } => match act {
                    EntryAct::Draft => {}
                    EntryAct::Previous => out.recall = Some(true),
                    EntryAct::Next => out.recall = Some(false),
                },
                ChatEvent::Focus(mode) => self.mode = mode,
                ChatEvent::Say(text) => out.say = outgoing(&text, self.mode),
                // The game's lines carry no names or addresses to click.
                ChatEvent::Tell { .. } | ChatEvent::OpenLink(_) => {}
                ChatEvent::Filter { window, mask } => {
                    let filter = u32::try_from(mask).unwrap_or(u32::MAX);
                    out.acts.push(Action::ChatWindows(ChatWindowAction::Filter {
                        window,
                        filter,
                    }));
                    out.acts.push(Action::ChatWindows(ChatWindowAction::Keep));
                }
                ChatEvent::Place { slot, at } => {
                    if let Some(kept) = own.chat_popped.get_mut(slot) {
                        *kept = at;
                        self.kept = true;
                    }
                }
                ChatEvent::Size { slot: 0, size } => {
                    own.chat_size = size;
                    self.kept = true;
                }
                ChatEvent::Size { slot, size } => {
                    if let Some(kept) = own.chat_popped_size.get_mut(slot) {
                        *kept = size;
                        self.kept = true;
                    }
                }
            }
        }
    }
}
