//! The log window: the game's chat windows as its tabs, and the input line.
//!
//! Each tab is one of the game's chat windows (the main one and the four floating ones), showing
//! what that window's filter lets through, by the shared routing rule; a tab can be popped out
//! into a window that looks as the docked one does, moved by dragging its tab, and docked again;
//! its filter is set from its menu. A tab's filter is the game's filter for its window, the same
//! one the Chat page of Settings sets; until the character has set one for a window, the tab has
//! this interface's own (see [`default_filter`]). The tabs' names and the windows' opacities are
//! this interface's settings. The input line
//! speaks to the game's talk-focus destination, chosen from the game's own list of destinations
//! the player may use now; what is typed is the game's chat entry, so its draft, recalled lines,
//! replies and tells are the game's, the same in every interface.

use dereth_client_contract::chat::entry::{EntryAction, ReplyTarget};
use dereth_client_contract::chat::interface::{route, window, Routed};
use dereth_client_contract::UiRequest;
use dereth_ui_kit::chat::{ChatEvent, ChatGame, EntryAct, FocusItem, Link, Window};

use crate::draw::{Argb, Rect};
use crate::options::{ChatOpacity, TabNames};
use crate::ui::colours::chat_colour;
use crate::ui::game::{ChatLine, ChatWindow, GameState};
use crate::ui::hud::chat_links;
use crate::ui::kit::Ctx;
use crate::ui::paint::Painter;
use crate::ui::Outcome;

/// The game's chat windows, in the order the log window's tabs list them.
pub const WINDOWS: [u32; 5] = [
    window::MAIN,
    window::FLOATY_1,
    window::FLOATY_2,
    window::FLOATY_3,
    window::FLOATY_4,
];

/// The colour a talk-focus destination's label is drawn in: its kind of chat's.
#[must_use]
pub fn focus_colour(focus: u32) -> Argb {
    match focus {
        2 => 0xFFFF_B8DE,
        3 => 0xFF66_E5FF,
        4..=7 => 0xFFAB_DBE5,
        8..=12 => 0xFFD4_FF7D,
        13 => 0xFFFF_A666,
        _ => 0xFFF7_F7F7,
    }
}

/// The destinations the talk-focus menu lists, in the game's menu order, with Olthoi's own
/// channel for an Olthoi player.
#[must_use]
pub fn menu_focuses(is_olthoi: bool) -> Vec<u32> {
    let mut out: Vec<u32> = dereth_client_model::chat::TALK_FOCUS_MENU_ORDER
        .iter()
        .map(|f| *f as u32)
        .collect();
    if is_olthoi {
        out.push(dereth_client_model::chat::TalkFocus::Olthoi as u32);
    }
    out
}

/// Whether a destination is one of the game's own chat rooms.
fn is_room_focus(focus: u32) -> bool {
    use dereth_client_model::chat::TalkFocus as F;
    [
        F::General,
        F::Trade,
        F::Lfg,
        F::Roleplay,
        F::Society,
        F::Olthoi,
    ]
    .iter()
    .any(|f| *f as u32 == focus)
}

/// Whether destination `focus` can be picked now.
#[must_use]
pub fn focus_usable(
    view: &dereth_client_contract::chat::mainchat::ChatFocusView,
    focus: u32,
) -> bool {
    let i = focus as usize;
    view.enabled.get(i).copied().unwrap_or(false)
        && view.selectable.get(i).copied().unwrap_or(false)
}

/// The destination after `current` that can be picked now, in menu order, wrapping; `None` when
/// there is no other.
#[must_use]
pub fn next_focus(view: &dereth_client_contract::chat::mainchat::ChatFocusView) -> Option<u32> {
    let order = menu_focuses(view.is_olthoi);
    let at = order.iter().position(|f| *f == view.focus).unwrap_or(0);
    (1..=order.len())
        .map(|step| order[(at + step) % order.len()])
        .find(|f| *f != view.focus && focus_usable(view, *f))
}

/// The talk focus a line's opening command switches to, and the rest of the line: `/cg hello`
/// is General and `hello`. The commands are the game's own channel commands
/// ([`dereth_client_model::cmd::table`]); a command that is not a channel's, or one with nothing
/// typed after its space yet, switches nothing.
#[must_use]
pub fn focus_command(text: &str) -> Option<(u32, &str)> {
    use dereth_client_model::chat::TalkFocus as F;
    use dereth_client_model::cmd::table::{
        CommandHandler as H, INITIALIZE_COMMANDS, TURBINE_CHAT_COMMANDS,
    };
    let rest = text.strip_prefix('/').or_else(|| text.strip_prefix('@'))?;
    let (word, after) = rest.split_once(' ')?;
    let entry = INITIALIZE_COMMANDS
        .iter()
        .chain(TURBINE_CHAT_COMMANDS)
        .find(|e| e.name.eq_ignore_ascii_case(word))?;
    let focus = match entry.handler? {
        H::Say => F::All,
        H::General => F::General,
        H::Trade => F::Trade,
        H::Lfg => F::Lfg,
        H::Roleplay => F::Roleplay,
        H::Society => F::Society,
        H::Olthoi => F::Olthoi,
        H::Allegiance => F::Allegiance,
        H::ChannelShortcut => match word.to_ascii_lowercase().as_str() {
            "f" => F::Fellowship,
            "a" => F::Allegiance,
            "m" => F::Monarch,
            "p" => F::Patron,
            "v" => F::Vassals,
            _ => return None,
        },
        _ => return None,
    };
    Some((focus as u32, after))
}

/// The channel a line being typed goes to, for its label: the one a command it starts with
/// names, else `focus` (the destination's label and colour).
#[must_use]
pub fn channel_label(text: &str, focus: (&str, Argb)) -> (String, Argb) {
    let lower = text.to_ascii_lowercase();
    let command = lower
        .strip_prefix('/')
        .or_else(|| lower.strip_prefix('@'))
        .map(|rest| rest.split_whitespace().next().unwrap_or(""));
    let (label, colour) = match command {
        Some("t" | "tell" | "w" | "whisper" | "send" | "r" | "reply" | "rt" | "retell") => {
            ("Tell", 0xFFFF_B8DE)
        }
        Some("f" | "fellowship" | "fellow" | "fellows") => ("Fellowship", 0xFF66_E5FF),
        Some(
            "a" | "allegiance" | "p" | "patron" | "m" | "monarch" | "v" | "vassals" | "c"
            | "covassals",
        ) => ("Allegiance", 0xFFAB_DBE5),
        Some("cg" | "general") => ("General", 0xFFD4_FF7D),
        Some("ct" | "trade") => ("Trade", 0xFFD4_FF7D),
        Some("clfg" | "lfg") => ("LFG", 0xFFD4_FF7D),
        Some("e" | "em" | "emote" | "me") => ("Emote", 0xFFBA_FFF0),
        Some(_) => ("Command", 0xFFCC_CCCC),
        None => return (focus.0.to_owned(), focus.1),
    };
    (label.to_owned(), colour)
}

/// Whether `line` shows in `window`.
#[must_use]
pub fn shows_in(window: &ChatWindow, line: &ChatLine) -> bool {
    route(
        window.id,
        window.filter,
        u32::from(line.chat_type),
        line.window,
    ) == Routed::Accepted
}

/// The mask of the filter group the pages caption `caption`.
fn group(caption: &str) -> u64 {
    dereth_client_contract::options::sheet::FILTER_GROUPS
        .iter()
        .find(|(c, _)| *c == caption)
        .map_or(0, |(_, mask)| *mask)
}

/// The filter this interface gives a chat window the character keeps no filter for: the main
/// window shows talk on every channel (and the game's own messages) but not combat or magic,
/// the second shows combat and magic, the third allegiance, the fourth fellowship, and the fifth
/// the global channels and society.
#[must_use]
pub fn default_filter(id: u32) -> u64 {
    use dereth_client_contract::options::sheet::MAIN_WINDOW_DEFAULT_FILTER;
    let society = group("Society");
    match id {
        window::MAIN => {
            (MAIN_WINDOW_DEFAULT_FILTER & !(group("Combat") | group("Magic"))) | society
        }
        window::FLOATY_1 => group("Combat") | group("Magic"),
        window::FLOATY_2 => group("Allegiance"),
        window::FLOATY_3 => group("Fellowship"),
        window::FLOATY_4 => {
            group("General") | group("Trade") | group("LFG") | group("Roleplay") | society
        }
        _ => 0,
    }
}

/// Chat window `id`'s filter: the one the game keeps for the character, else this interface's
/// [`default_filter`]. A kept filter that is the game's own starting filter for that window is
/// one the game wrote back for itself, not one the player chose, and counts as none. The log
/// window's tabs and the Chat page of Settings both show and edit this one filter.
#[must_use]
pub fn window_filter(state: &GameState, id: u32) -> u64 {
    state
        .chat_filters
        .iter()
        .find(|(w, f)| {
            *w == id && *f != dereth_client_contract::chat::interface::default_filter(id)
        })
        .map_or_else(|| default_filter(id), |(_, f)| *f)
}

/// The log window's state across frames: the shared log window ([`dereth_ui_kit::chat`]) with
/// this game's five chat windows, and this interface's settings for it.
#[derive(Debug, Default)]
pub struct ChatLog {
    log: dereth_ui_kit::chat::ChatLog<ChatLine, 5>,
    /// The tabs' names, from this interface's settings.
    pub tab_names: TabNames,
    /// How opaque the windows' grounds are, from this interface's settings.
    pub opacity: ChatOpacity,
}

impl std::ops::Deref for ChatLog {
    type Target = dereth_ui_kit::chat::ChatLog<ChatLine, 5>;
    fn deref(&self) -> &Self::Target {
        &self.log
    }
}

impl std::ops::DerefMut for ChatLog {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.log
    }
}

/// The game as the log window reads it: this frame's state, and the interface's settings.
struct Game<'a> {
    state: &'a GameState,
    names: &'a TabNames,
    opacity: &'a ChatOpacity,
}

impl ChatGame for Game<'_> {
    type Line = ChatLine;

    fn windows(&self) -> Vec<Window> {
        windows_of(self.state)
            .iter()
            .map(|w| Window {
                id: w.id,
                filter: w.filter,
            })
            .collect()
    }

    fn text<'l>(&self, line: &'l ChatLine) -> &'l str {
        &line.text
    }

    fn colour(&self, line: &ChatLine) -> Argb {
        chat_colour(line.chat_type)
    }

    fn shows(&self, window: &Window, line: &ChatLine) -> bool {
        route(
            window.id,
            window.filter,
            u32::from(line.chat_type),
            line.window,
        ) == Routed::Accepted
    }

    fn links(&self, text: &str) -> (String, Vec<Link>) {
        chat_links(text)
    }

    fn tab_name(&self, slot: usize) -> String {
        self.names.name(slot).to_owned()
    }

    fn opacity(&self, slot: Option<usize>, active: bool) -> f32 {
        self.opacity.get(slot, active)
    }

    fn focus_line(&self) -> (String, Argb) {
        let focus = self.state.chat_focus.focus;
        (mode_name(self.state, focus), focus_colour(focus))
    }

    fn focus_menu(&self) -> Vec<FocusItem> {
        let state = self.state;
        let rooms = state.era.is_none_or(|e| e.global_chat_rooms);
        let mut focuses = menu_focuses(state.chat_focus.is_olthoi);
        // A world without the game's own rooms lists only one the server named.
        focuses.retain(|f| {
            rooms
                || !is_room_focus(*f)
                || state.chat_focus.enabled.get(*f as usize).copied() == Some(true)
        });
        focuses
            .into_iter()
            .map(|focus| FocusItem {
                focus,
                label: mode_name(state, focus),
                colour: focus_colour(focus),
                usable: focus_usable(&state.chat_focus, focus),
            })
            .collect()
    }

    fn focus_usable(&self, focus: u32) -> bool {
        focus_usable(&self.state.chat_focus, focus)
    }

    fn next_focus(&self) -> Option<u32> {
        next_focus(&self.state.chat_focus)
    }

    fn focus_command<'t>(&self, text: &'t str) -> Option<(u32, &'t str)> {
        focus_command(text)
    }

    fn channel_label(&self, text: &str, focus: (&str, Argb)) -> (String, Argb) {
        channel_label(text, focus)
    }

    fn filter_groups(&self, id: u32) -> Vec<(String, u64)> {
        filter_groups(id, self.state.era.as_ref())
            .into_iter()
            .map(|(caption, mask)| (caption.to_owned(), mask))
            .collect()
    }
}

impl ChatLog {
    /// Take this frame's lines, the game's `@clear`, and what the game did to the entry.
    pub fn take(&mut self, state: &GameState, keep: impl Fn(&ChatLine) -> bool) {
        self.log.take(
            state.chat_cleared,
            state.new_chat.iter().filter(|l| keep(l)).cloned(),
        );
        for update in &state.chat_entry {
            if update.window != window::MAIN {
                continue;
            }
            self.log.entry_told(&update.text, update.focus);
        }
    }

    /// One of the game's chat actions, its key pressed: replies, a tell to the selection, a
    /// command begun, the entry opened, and the recalled lines.
    pub fn action(&mut self, action: u32, state: &GameState, out: &mut Outcome) {
        use dereth_client_contract::actions::{chat_entry as a, ActionId};
        let id = ActionId(action);
        let text = self.log.typed().unwrap_or_default().to_owned();
        let entry = |action| UiRequest::ChatEntry {
            window: window::MAIN,
            text: text.clone(),
            action,
        };
        if let Some(target) = ReplyTarget::from_action(id) {
            out.requests.push(entry(EntryAction::Reply {
                target,
                prefix: "@tell ".into(),
            }));
        } else if id == a::TELL_TO_SELECTED {
            if let Some(t) = state
                .target
                .as_ref()
                .filter(|t| Some(t.id) != state.player_id && !t.name.is_empty())
            {
                out.requests.push(entry(EntryAction::StartTell {
                    name: t.name.clone(),
                }));
            }
        } else if id == a::START_COMMAND {
            // The `/` key also types its own "/", this frame or the next: the line already has it.
            self.log.open_command();
            out.requests.push(UiRequest::ChatEntry {
                window: window::MAIN,
                text: "/".into(),
                action: EntryAction::Draft,
            });
        } else if id == a::BEGIN_CHAT_MODE || id == a::TOGGLE_CHAT_ENTRY {
            if !self.log.typing() {
                self.log.open(state.chat_draft.clone());
            }
        } else if Some(id) == named("RecallLastMessage") {
            out.requests.push(entry(EntryAction::RecallLast));
        } else if Some(id) == named("PreviousMessage") {
            out.requests.push(entry(EntryAction::Previous));
        } else if Some(id) == named("NextMessage") {
            out.requests.push(entry(EntryAction::Next));
        }
    }

    /// The docked log window, in the HUD layout's default place.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        let x = 20.0 * k;
        // As wide as it may be without reaching what is centred along the bottom, at its widest
        // (the spell bar's tab keys, or the cross hotbars), and no narrower than it may be made.
        let foot_left = sw / 2.0 - crate::ui::hud::FOOT_REACH * k;
        let home_h = 230.0 * k;
        let least = dereth_ui_kit::chat::ChatLog::<ChatLine, 5>::smallest(0, k).0;
        let home = Rect::new(
            x,
            sh - home_h - 24.0 * k,
            (480.0 * k).min(foot_left - x - 12.0 * k).max(least),
            home_h,
        );
        let game = Game {
            state,
            names: &self.tab_names,
            opacity: &self.opacity,
        };
        let mut asked = Vec::new();
        self.log.frame(p, ctx, &game, home, &mut asked);
        carry_out(asked, out);
    }

    /// The popped-out windows and the menus, over the HUD, with [`dereth_ui_kit::chat::ChatLog::dock`]
    /// placed.
    pub fn overlays(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let game = Game {
            state,
            names: &self.tab_names,
            opacity: &self.opacity,
        };
        let mut asked = Vec::new();
        self.log.overlays(p, ctx, &game, &mut asked);
        carry_out(asked, out);
    }
}

/// What the log window asked, as the game's requests and this interface's settings.
fn carry_out(asked: Vec<ChatEvent>, out: &mut Outcome) {
    let value = |v: Option<(f32, f32)>| v.map_or_else(String::new, |(x, y)| format!("{x},{y}"));
    let entry = |text, action| UiRequest::ChatEntry {
        window: window::MAIN,
        text,
        action,
    };
    for event in asked {
        match event {
            ChatEvent::Entry { text, act } => out.requests.push(entry(
                text,
                match act {
                    EntryAct::Draft => EntryAction::Draft,
                    EntryAct::Previous => EntryAction::Previous,
                    EntryAct::Next => EntryAction::Next,
                },
            )),
            ChatEvent::Focus(focus) => out.requests.push(UiRequest::SetTalkFocus { focus }),
            ChatEvent::Say(text) => out.requests.push(UiRequest::ChatLine {
                text,
                window: window::MAIN,
            }),
            ChatEvent::Tell { name, text } => {
                out.requests
                    .push(entry(text, EntryAction::StartTell { name }));
            }
            ChatEvent::OpenLink(url) => out.requests.push(UiRequest::ChatLinkClicked(url)),
            ChatEvent::Filter { window, mask } => out
                .requests
                .push(UiRequest::SetChatWindowFilter { window, mask }),
            ChatEvent::Place { slot, at } => out
                .settings
                .push((crate::options::popped_key(slot), value(at))),
            ChatEvent::Size { slot, size } => out
                .settings
                .push((crate::options::size_key(slot), value(size))),
        }
    }
}

/// The game's chat windows as this frame's state has them, the main one at least.
/// Each with its filter as [`window_filter`] has it.
fn windows_of(state: &GameState) -> Vec<ChatWindow> {
    let mut windows = if state.chat_windows.is_empty() {
        vec![ChatWindow {
            id: window::MAIN,
            filter: 0,
            title: None,
        }]
    } else {
        state.chat_windows.clone()
    };
    for w in &mut windows {
        w.filter = window_filter(state, w.id);
    }
    windows
}

/// A destination's name in the talk-focus menu and on the input line, in full (General, where
/// the game's own menu says Gen), so the channel the player is talking in reads plainly: a tell to
/// the selection names who it goes to (Tell to Borin). A destination with no plain name has the
/// game's own word for it, else Chat.
fn mode_name(state: &GameState, focus: u32) -> String {
    if focus == 2 {
        if let Some(t) = &state.chat_focus.target {
            return format!("Tell to {}", t.name);
        }
    }
    FOCUS_WORDS
        .get(focus as usize)
        .filter(|w| !w.is_empty())
        .map(|w| (*w).to_owned())
        .or_else(|| {
            state
                .chat_focus_labels
                .get(focus as usize)
                .filter(|l| !l.is_empty())
                .cloned()
        })
        .unwrap_or_else(|| "Chat".to_owned())
}

/// The destinations' plain names, by focus.
pub const FOCUS_WORDS: [&str; 14] = [
    "",
    "Chat",
    "Tell",
    "Fellowship",
    "Patron",
    "Monarch",
    "Vassals",
    "Allegiance",
    "General",
    "Trade",
    "LFG",
    "Roleplay",
    "Society",
    "Olthoi",
];

/// The filter groups the options offer for window `id`, as the shared options sheet lists them
/// for this interface: caption and mask.
#[must_use]
pub fn filter_groups(
    id: u32,
    features: Option<&dereth_primitives::EraFeatures>,
) -> Vec<(&'static str, u64)> {
    use dereth_client_contract::options::interface::Interface;
    use dereth_client_contract::options::sheet::{rows_for, PageId, Value};
    rows_for(PageId::Chat, Interface::Horizon)
        .filter(|r| r.needs.met(features))
        .filter_map(|r| match r.value {
            Value::Filter { window, mask } if window == id => Some((r.caption, mask)),
            _ => None,
        })
        .collect()
}

/// The action an action name stands for.
fn named(name: &str) -> Option<dereth_client_contract::actions::ActionId> {
    dereth_client_contract::actions::names::action_for_enum_name(name)
}

/// Whether `action` is one of the chat entry's, which the log window answers.
#[must_use]
pub fn is_chat_action(action: u32) -> bool {
    use dereth_client_contract::actions::{chat_entry as a, ActionId};
    let id = ActionId(action);
    [
        a::REPLY,
        a::MONARCH_REPLY,
        a::PATRON_REPLY,
        a::TELL_TO_SELECTED,
        a::START_COMMAND,
        a::BEGIN_CHAT_MODE,
        a::TOGGLE_CHAT_ENTRY,
    ]
    .contains(&id)
        || ["RecallLastMessage", "PreviousMessage", "NextMessage"]
            .iter()
            .any(|n| named(n) == Some(id))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;
    use dereth_client_contract::chat::mainchat::ChatFocusView;

    fn view(focus: u32, usable: &[u32]) -> ChatFocusView {
        let mut v = ChatFocusView {
            focus,
            ..ChatFocusView::default()
        };
        for f in usable {
            v.enabled[*f as usize] = true;
            v.selectable[*f as usize] = true;
        }
        v
    }

    #[test]
    fn tab_steps_to_the_next_destination_the_game_lets_the_player_use_in_its_menu_order() {
        // The menu's order begins Monarch, Selected, Patron, All, Vassals, Fellowship.
        let v = view(1, &[1, 3, 8]);
        assert_eq!(next_focus(&v), Some(3));
        let v = view(3, &[1, 3, 8]);
        assert_eq!(next_focus(&v), Some(8));
        let v = view(8, &[1, 3, 8]);
        assert_eq!(next_focus(&v), Some(1), "it wraps round");
        assert_eq!(next_focus(&view(1, &[1])), None);
    }

    #[test]
    fn a_destination_switched_off_or_not_selectable_cannot_be_picked() {
        let mut v = view(1, &[1, 4]);
        assert!(focus_usable(&v, 4));
        v.selectable[4] = false;
        assert!(!focus_usable(&v, 4));
        assert!(!focus_usable(&v, 13));
        assert!(!menu_focuses(false).contains(&13));
        assert!(menu_focuses(true).contains(&13));
    }

    #[test]
    fn the_input_line_and_the_menu_name_the_channel_in_full_where_the_game_s_menu_shortens_it() {
        let mut labels = vec![String::new(); 14];
        labels[8] = "Gen".into();
        let mut state = GameState {
            chat_focus_labels: labels,
            ..GameState::default()
        };
        assert_eq!(mode_name(&state, 8), "General");
        assert_eq!(mode_name(&state, 2), "Tell", "no one selected");
        state.chat_focus.target = Some(dereth_client_contract::chat::mainchat::SpeakableTarget {
            name: "Borin".into(),
            ..Default::default()
        });
        assert_eq!(mode_name(&state, 2), "Tell to Borin");
    }

    #[test]
    fn a_typed_command_names_its_channel_and_plain_text_takes_the_destination() {
        let focus = ("Monarch", 0xFFAB_DBE5);
        assert_eq!(channel_label("/t Borin, hi", focus).0, "Tell");
        assert_eq!(channel_label("@f ready", focus).0, "Fellowship");
        assert_eq!(channel_label("/cg anyone?", focus).0, "General");
        assert_eq!(
            channel_label("hello", focus),
            ("Monarch".to_owned(), 0xFFAB_DBE5)
        );
    }

    #[test]
    fn a_window_shows_lines_addressed_to_it_and_the_broadcasts_its_filter_lets_through() {
        let w = ChatWindow {
            id: window::FLOATY_1,
            filter: 1 << 2,
            title: None,
        };
        let line = |chat_type, window| ChatLine {
            chat_type,
            text: String::new(),
            window,
        };
        assert!(shows_in(&w, &line(2, 0)));
        assert!(!shows_in(&w, &line(3, 0)));
        assert!(shows_in(&w, &line(3, window::FLOATY_1)));
        assert!(!shows_in(&w, &line(2, window::MAIN)));
    }

    #[test]
    fn the_main_window_offers_twelve_filter_groups_and_a_floating_one_thirteen() {
        assert_eq!(filter_groups(window::MAIN, None).len(), 12);
        assert_eq!(filter_groups(window::FLOATY_2, None).len(), 13);
        assert_eq!(filter_groups(window::FLOATY_2, None)[0].0, "Gameplay");
    }

    #[test]
    fn each_window_s_own_default_shows_its_channels_until_the_character_keeps_a_filter() {
        let shows = |id: u32, caption: &str| default_filter(id) & group(caption) == group(caption);
        for c in [
            "Area Speech",
            "Tells",
            "Allegiance",
            "Fellowship",
            "General",
            "Trade",
            "LFG",
            "Roleplay",
            "Society",
        ] {
            assert!(shows(window::MAIN, c), "main shows {c}");
        }
        assert!(!shows(window::MAIN, "Combat") && !shows(window::MAIN, "Magic"));
        assert!(!shows(window::MAIN, "Errors"));
        assert!(
            shows(window::MAIN, "Gameplay"),
            "the main window keeps the game's own messages"
        );
        assert_eq!(
            default_filter(window::FLOATY_1),
            group("Combat") | group("Magic")
        );
        assert_eq!(default_filter(window::FLOATY_2), group("Allegiance"));
        assert_eq!(default_filter(window::FLOATY_3), group("Fellowship"));
        assert!(shows(window::FLOATY_4, "Society") && shows(window::FLOATY_4, "LFG"));
        let mut state = GameState::default();
        assert_eq!(
            window_filter(&state, window::FLOATY_1),
            default_filter(window::FLOATY_1)
        );
        state.chat_filters = vec![(
            window::FLOATY_1,
            dereth_client_contract::chat::interface::default_filter(window::FLOATY_1),
        )];
        assert_eq!(
            window_filter(&state, window::FLOATY_1),
            default_filter(window::FLOATY_1),
            "the game's own starting filter kept for the window is not the player's choice"
        );
        state.chat_filters = vec![(window::FLOATY_1, 0)];
        assert_eq!(
            window_filter(&state, window::FLOATY_1),
            0,
            "a filter the character keeps is the player's, even one showing nothing"
        );
    }

    #[test]
    fn a_channel_command_at_the_start_of_the_line_names_its_talk_focus_and_leaves_the_rest() {
        use dereth_client_model::chat::TalkFocus as F;
        assert_eq!(
            focus_command("/cg hello"),
            Some((F::General as u32, "hello"))
        );
        assert_eq!(focus_command("@CT "), Some((F::Trade as u32, "")));
        assert_eq!(
            focus_command("/f we go"),
            Some((F::Fellowship as u32, "we go"))
        );
        assert_eq!(focus_command("/clfg x"), Some((F::Lfg as u32, "x")));
        assert_eq!(focus_command("/cg"), None, "no space typed yet");
        assert_eq!(
            focus_command("/tell Bob, hi"),
            None,
            "a tell is not a channel"
        );
        assert_eq!(focus_command("hello"), None);
    }
}
