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

use crate::art::Family;
use crate::draw::{Argb, Rect};
use crate::options::{ChatOpacity, TabNames};
use crate::ui::colours::chat_colour;
use crate::ui::game::{ChatLine, ChatWindow, GameState};
use crate::ui::hud::{chat_links, Link};
use crate::ui::input::vk;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The side of a log window's corner it is sized by, in layout units.
const GRIP: f32 = 12.0;

/// How many chat lines the log keeps.
const CHAT_HISTORY: usize = 400;

/// The longest line the input takes.
const MAX_LINE: usize = 250;

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

/// The menu open over the log window.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Menu {
    /// The talk-focus destinations.
    Focus,
    /// A tab's menu, opened over `at` (its tab bar, or its popped-out window's header): pop it
    /// out or dock it, and its filter. A docked tab's bar is where the docked window drew it,
    /// before the HUD layout placed the window.
    Tab { slot: usize, at: Rect, docked: bool },
}

/// A stretch of the log selected with the pointer, from where the press was to where the pointer
/// went: each end a line, counted from the first line the log ever kept, and a character in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LogSelection {
    /// The window it is in, as [`windows_of`] lists them.
    slot: usize,
    anchor: (usize, usize),
    end: (usize, usize),
    /// The press is still being dragged.
    dragging: bool,
}

impl LogSelection {
    /// Its two ends, the earlier first.
    fn ends(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.end {
            (self.anchor, self.end)
        } else {
            (self.end, self.anchor)
        }
    }
}

/// One wrapped row of a log window: its colour, its text and links, the line it is from and
/// where in that line it starts.
type Row = (Argb, String, Vec<Link>, usize, usize);

/// The log window's state across frames.
#[derive(Debug, Default)]
pub struct ChatLog {
    lines: Vec<ChatLine>,
    /// The tab shown in the docked window.
    tab: usize,
    /// Each window's scroll, in lines from the newest.
    scroll: [usize; 5],
    /// The windows popped out of the dock, each a window of its own.
    floating: [kit::WindowState; 5],
    /// The input line, while it is open, and its text as last told to the game.
    typing: Option<String>,
    told: String,
    /// The line was opened by the `/` key, whose own typed `/` is still to come and is dropped.
    slash_typed: bool,
    menu: Option<Menu>,
    /// The menu was opened by a press not yet let go.
    menu_fresh: bool,
    /// Where the docked window is on screen, after the HUD layout has placed it.
    pub dock: Rect,
    /// Where the input line's text was drawn.
    text_area: Rect,
    /// How many lines the log has let go of since it began.
    dropped: usize,
    /// The text selected in a window's lines.
    selection: Option<LogSelection>,
    /// The tabs' names, from this interface's settings.
    pub tab_names: TabNames,
    /// How opaque the windows' grounds are, from this interface's settings.
    pub opacity: ChatOpacity,
    /// Where each tab popped out was last put, in layout units, from this interface's settings.
    pub popped_saved: [Option<(f32, f32)>; 5],
    /// Where each popped-out tab stands while it is out, in layout units, once it is moved.
    placed: [Option<(f32, f32)>; 5],
    /// Where each popped-out tab's window was drawn last.
    popped_rects: [Option<Rect>; 5],
    /// Where the open menu was drawn last.
    menu_rect: Option<Rect>,
    /// HUD Layout's Reset put the windows back where they start: the places and sizes kept in
    /// the settings are not shown until the layout is saved or put back.
    forgotten: bool,
    /// The places and sizes as they stood when the HUD Layout window opened, to put back.
    layout_from: Option<Places>,
    /// HUD Layout moved, sized or reset a window since it was last saved.
    places_unsaved: bool,
    /// HUD Layout's Save: the places and sizes are to be kept in the settings.
    write_places: bool,
    /// How large the player made each log window, in layout units, from this interface's
    /// settings: the docked window's (by the main tab), then each tab's popped out.
    pub sizes_saved: [Option<(f32, f32)>; 5],
    /// How large each log window is made while this session resizes it, in layout units.
    sizes: [Option<(f32, f32)>; 5],
    /// The log window being resized by its corner, and how far the pointer is from the corner.
    resizing: Option<(usize, (f32, f32))>,
    /// The HUD Layout window is open: the popped-out tabs are moved and sized as its elements.
    pub editing: bool,
    /// Where the docked window would stand with no HUD layout: what the popped-out tabs' column
    /// stands over.
    home: Rect,
    /// Where the docked window's lines are, as it drew them before the HUD layout placed it.
    dock_drawn: Rect,
    /// Where the tabs and the open menu's controls were drawn this frame, by their captions:
    /// what a scripted run or a test clicks.
    controls: Vec<(String, Rect)>,
}

/// Where each popped-out tab stands and how large each log window is, as this session moved them.
type Places = ([Option<(f32, f32)>; 5], [Option<(f32, f32)>; 5]);

impl ChatLog {
    /// Take this frame's lines, the game's `@clear`, and what the game did to the entry.
    pub fn take(&mut self, state: &GameState, keep: impl Fn(&ChatLine) -> bool) {
        if state.chat_cleared {
            self.dropped += self.lines.len();
            self.lines.clear();
            self.scroll = [0; 5];
            self.selection = None;
        }
        self.lines
            .extend(state.new_chat.iter().filter(|l| keep(l)).cloned());
        if self.lines.len() > CHAT_HISTORY {
            let extra = self.lines.len() - CHAT_HISTORY;
            self.lines.drain(..extra);
            self.dropped += extra;
        }
        for update in &state.chat_entry {
            if update.window != window::MAIN {
                continue;
            }
            if update.focus || self.typing.is_some() {
                self.typing = Some(update.text.clone());
                self.told.clone_from(&update.text);
            }
        }
    }

    /// The log holds `lines` and nothing else.
    pub fn replace(&mut self, lines: Vec<ChatLine>) {
        self.dropped += self.lines.len();
        self.selection = None;
        self.lines = lines;
        if self.lines.len() > CHAT_HISTORY {
            let extra = self.lines.len() - CHAT_HISTORY;
            self.lines.drain(..extra);
        }
        self.scroll = [0; 5];
    }

    /// Open the input line with `text`.
    pub fn open(&mut self, text: String) {
        self.told.clone_from(&text);
        self.typing = Some(text);
    }

    /// Whether the input line is open.
    #[must_use]
    pub fn typing(&self) -> bool {
        self.typing.is_some()
    }

    /// Where the control captioned `label` was drawn this frame: a docked tab by its name, or a
    /// check box or the button of the open tab menu.
    #[must_use]
    pub fn control(&self, label: &str) -> Option<Rect> {
        self.controls
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, r)| *r)
    }

    /// Where popped-out tab `slot`'s window is: its tab bar's top-left, once drawn.
    #[must_use]
    pub fn popped_at(&self, slot: usize) -> Option<(f32, f32)> {
        self.floating
            .get(slot)
            .filter(|w| w.open)
            .and_then(|w| w.pos)
    }

    /// Put every popped-out tab back in its own place in the column over the docked window and
    /// every log window back to its first size, until HUD Layout is saved or put back.
    pub fn reset_places(&mut self) {
        self.placed = [None; 5];
        self.sizes = [None; 5];
        self.forgotten = true;
        self.places_unsaved = true;
    }

    /// The HUD Layout window opened: what it changes can be put back to this.
    pub fn begin_layout(&mut self) {
        self.layout_from = Some((self.placed, self.sizes));
        self.places_unsaved = false;
    }

    /// Whether HUD Layout moved, sized or reset a window since it was last saved.
    #[must_use]
    pub fn places_unsaved(&self) -> bool {
        self.places_unsaved
    }

    /// Keep the places and sizes as they stand: the next frame asks for them to be kept.
    pub fn save_places(&mut self) {
        self.write_places = true;
        self.places_unsaved = false;
        self.layout_from = Some((self.placed, self.sizes));
    }

    /// Put back the places and sizes as they stood when HUD Layout opened or last saved.
    pub fn revert_places(&mut self) {
        if let Some((placed, sizes)) = self.layout_from.take() {
            self.placed = placed;
            self.sizes = sizes;
        }
        self.forgotten = false;
        self.places_unsaved = false;
    }

    /// Where popped-out tab `slot` was last put, as the settings keep it, unless HUD Layout's
    /// Reset is waiting to be saved.
    fn kept_place(&self, slot: usize) -> Option<(f32, f32)> {
        self.popped_saved[slot].filter(|_| !self.forgotten)
    }

    /// How large log window `slot` was last made, as the settings keep it, unless HUD Layout's
    /// Reset is waiting to be saved.
    fn kept_size(&self, slot: usize) -> Option<(f32, f32)> {
        self.sizes_saved[slot].filter(|_| !self.forgotten)
    }

    /// How large log window `slot` is now, in screen pixels at scale `k`: the docked window's
    /// (by the main tab) or a tab's popped out, its tab included.
    #[must_use]
    pub fn window_size(&self, slot: usize, k: f32) -> (f32, f32) {
        self.sizes[slot]
            .or(self.kept_size(slot))
            .map_or_else(|| self.default_size(slot, k), |(w, h)| (w * k, h * k))
    }

    /// How large log window `slot` starts: the docked window as it stands, a popped-out tab as
    /// the docked window's lines and a tab over them.
    fn default_size(&self, slot: usize, k: f32) -> (f32, f32) {
        let h = if slot == 0 { 230.0 } else { 24.0 + 180.0 };
        (self.home.w, h * k)
    }

    /// The smallest a log window may be made, in screen pixels at scale `k`.
    fn smallest(slot: usize, k: f32) -> (f32, f32) {
        (200.0 * k, if slot == 0 { 120.0 } else { 100.0 } * k)
    }

    /// The open menu, as it was last drawn: what is under it does not have the pointer.
    #[must_use]
    pub fn menu_rect(&self) -> Option<Rect> {
        self.menu.and(self.menu_rect)
    }

    /// The popped-out tabs' windows as they were last drawn: what is under them does not
    /// have the pointer.
    #[must_use]
    pub fn popped_rects(&self) -> Vec<Rect> {
        (0..5)
            .filter(|i| self.floating[*i].open)
            .filter_map(|i| self.popped_rects[i])
            .collect()
    }

    /// Where tab `slot` stands popped out until it is put elsewhere: in a column over the docked
    /// window at its left edge, the second tab at the top and the fifth just above the docked
    /// window, each a window's height and a small gap apart; in screen pixels at scale `k`.
    #[must_use]
    pub fn popped_default(&self, slot: usize, k: f32) -> (f32, f32) {
        let d = self.home;
        let whole = self.default_size(1, k).1;
        #[allow(clippy::cast_precision_loss)]
        let above = (5 - slot.clamp(1, 4)) as f32;
        (d.x, d.y - above * (whole + 8.0 * k))
    }

    /// The tab the docked window shows, as the log window's tabs number them.
    #[must_use]
    pub fn shown_tab(&self) -> usize {
        self.tab
    }

    /// Open `menu`, from a press not yet let go.
    fn open_menu(&mut self, menu: Menu) {
        self.menu = Some(menu);
        self.menu_fresh = true;
    }

    /// Whether a tab's menu is open, titled Filters.
    #[must_use]
    pub fn filters_open(&self) -> bool {
        matches!(self.menu, Some(Menu::Tab { .. }))
    }

    /// Whether text is selected in the lines, for Control-C to copy.
    #[must_use]
    pub fn has_selection(&self) -> bool {
        self.selection.is_some_and(|s| s.anchor != s.end)
    }

    /// Whether Escape has something to close here.
    #[must_use]
    pub fn wants_escape(&self) -> bool {
        self.typing.is_some() || self.menu.is_some()
    }

    /// The lines kept, oldest first.
    #[must_use]
    pub fn lines(&self) -> &[ChatLine] {
        &self.lines
    }

    /// Which windows are popped out.
    #[must_use]
    pub fn floating(&self) -> [bool; 5] {
        self.floating.map(|w| w.open)
    }

    /// One of the game's chat actions, its key pressed: replies, a tell to the selection, a
    /// command begun, the entry opened, and the recalled lines.
    pub fn action(&mut self, action: u32, state: &GameState, out: &mut Outcome) {
        use dereth_client_contract::actions::{chat_entry as a, ActionId};
        let id = ActionId(action);
        let text = self.typing.clone().unwrap_or_default();
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
            self.open("/".into());
            // The `/` key also types its own "/", this frame or the next: the line already has it.
            self.slash_typed = true;
            out.requests.push(UiRequest::ChatEntry {
                window: window::MAIN,
                text: "/".into(),
                action: EntryAction::Draft,
            });
        } else if id == a::BEGIN_CHAT_MODE || id == a::TOGGLE_CHAT_ENTRY {
            if self.typing.is_none() {
                self.open(state.chat_draft.clone());
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
        self.controls.clear();
        if std::mem::take(&mut self.write_places) {
            let value =
                |v: Option<(f32, f32)>| v.map_or_else(String::new, |(x, y)| format!("{x},{y}"));
            for slot in 1..5 {
                let at = self.placed[slot].or(self.kept_place(slot));
                out.settings
                    .push((crate::options::popped_key(slot), value(at)));
            }
            for slot in 0..5 {
                let size = self.sizes[slot].or(self.kept_size(slot));
                out.settings
                    .push((crate::options::size_key(slot), value(size)));
            }
            self.forgotten = false;
        }
        let x = 20.0 * k;
        // As wide as it may be without reaching what is centred along the bottom, at its widest
        // (the spell bar's tab keys, or the cross hotbars), and no narrower than it may be made.
        let foot_left = sw / 2.0 - crate::ui::hud::FOOT_REACH * k;
        let home_h = 230.0 * k;
        self.home = Rect::new(
            x,
            sh - home_h - 24.0 * k,
            (480.0 * k)
                .min(foot_left - x - 12.0 * k)
                .max(Self::smallest(0, k).0),
            home_h,
        );
        let y = self.home.y;
        // Made larger or smaller from its corner, it keeps its top-left where it starts.
        let (w, h) = self.window_size(0, k);
        let least = Self::smallest(0, k);
        let (w, h) = (w.min(sw - x).max(least.0), h.min(sh - y).max(least.1));
        let r = Rect::new(x, y + 24.0 * k, w, h - 24.0 * k);
        // The docked log is a panel of its own for the pad's focus, which the Back button
        // brings to it.
        ctx.input.nav.layer(
            Rect::new(x, y, w, h),
            "Chat",
            crate::ui::nav::PanelKind::Chat,
        );
        // The strip the input line sits in shows only while the line is open.
        let strip = if self.typing.is_some() { 0.0 } else { 26.0 * k };
        let shown = Rect::new(x, y, w, h - strip);
        let grip = self.resize_grip(p, ctx, 0, shown, strip, out);
        // Active while the pointer is over it or its line is being typed.
        let active = self.typing.is_some() || ctx.input.hover(&Rect::new(x, y, w, h - strip));
        p.fill(
            Rect::new(r.x, r.y, r.w, r.h - strip),
            ground(self.opacity.get(None, active)),
        );
        let windows = windows_of(state);
        // The docked tabs: the windows not popped out.
        let docked: Vec<usize> = (0..windows.len())
            .filter(|i| !self.floating[*i].open)
            .collect();
        if !docked.contains(&self.tab) {
            self.tab = docked.first().copied().unwrap_or(0);
        }
        let titles: Vec<String> = docked
            .iter()
            .map(|i| self.tab_names.name(*i).to_owned())
            .collect();
        let labels: Vec<&str> = titles.iter().map(String::as_str).collect();
        let tab_bar = Rect::new(x, y, w, 24.0 * k);
        let mut shown = docked.iter().position(|i| *i == self.tab).unwrap_or(0);
        if !labels.is_empty() {
            for (n, tr) in kit::tab_rects_for(p, tab_bar, &labels, shown)
                .into_iter()
                .enumerate()
            {
                self.controls.push((titles[n].clone(), tr));
                // A right click shows the tab as well as opening its menu.
                if ctx.input.right_clicked(&tr) {
                    self.open_menu(Menu::Tab {
                        slot: docked[n],
                        at: tab_bar,
                        docked: true,
                    });
                    if shown != n {
                        shown = n;
                        self.scroll[docked[n]] = 0;
                    }
                }
            }
            if kit::tabs(p, ctx, tab_bar, &labels, &mut shown) {
                self.scroll[docked[shown]] = 0;
            }
            self.tab = docked[shown];
        }
        let body = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        // The lines end above the corner the window is sized by.
        let text_area = Rect::new(
            r.x + 8.0 * k,
            r.y + 4.0 * k,
            r.w - 16.0 * k,
            r.h - (38.0 + GRIP) * k + 4.0 * k,
        );
        if !docked.is_empty() {
            let tab = self.tab;
            self.draw_lines(p, ctx, &windows[tab], tab, text_area, &body, out);
        }
        self.input_line(p, ctx, state, r, &body, out);
        // The pad's focus rests on the input line, where CONFIRM opens it.
        {
            let line = Rect::new(x, y + h - 26.0 * k, w, 26.0 * k);
            let input = &mut *ctx.input;
            input
                .nav
                .note(line, crate::ui::nav::Kind::Text, &input.occluders);
            input.nav.home(line);
        }
        if ctx.over_quiet(&Rect::new(x, y, w, h - strip))
            && (ctx.input.pressed[0] || ctx.input.pressed[1])
        {
            ctx.input.captured = true;
        }
        self.grip_mark(p, ctx, 0, grip);
        ctx.input.nav.end_layer();
        self.dock = r;
        self.dock_drawn = r;
    }

    /// A log window's corner it is made larger or smaller by: a press on it takes hold, a drag
    /// sizes the window, held between its smallest and the screen, and letting go keeps the size.
    /// `shown` is the window as it shows, `hidden` how much of its foot is not shown (the docked
    /// window's input strip while its line is closed). Where the corner is, for
    /// [`Self::grip_mark`].
    fn resize_grip(
        &mut self,
        p: &Painter<'_>,
        ctx: &mut Ctx<'_>,
        slot: usize,
        shown: Rect,
        hidden: f32,
        out: &mut Outcome,
    ) -> Rect {
        let k = p.scale;
        let g = GRIP * k;
        let grip = Rect::new(shown.right() - g, shown.bottom() - g, g, g);
        let (mx, my) = ctx.input.mouse;
        if self.resizing.is_none() && ctx.input.pressed[0] && ctx.input.hover(&grip) {
            ctx.input.pressed[0] = false;
            ctx.input.captured = true;
            self.menu = None;
            self.resizing = Some((slot, (shown.right() - mx, shown.bottom() - my)));
        }
        if let Some((s, (ox, oy))) = self.resizing.filter(|(s, _)| *s == slot) {
            let least = Self::smallest(s, k);
            let (sw, sh) = p.screen;
            let size = (
                (mx + ox - shown.x).clamp(least.0, (sw - shown.x).max(least.0)),
                (my + oy - shown.y + hidden).clamp(least.1, (sh - shown.y).max(least.1)),
            );
            self.sizes[s] = Some((size.0 / k, size.1 / k));
            ctx.input.captured = true;
            if !ctx.input.down[0] {
                self.resizing = None;
                out.settings.push((
                    crate::options::size_key(s),
                    format!("{},{}", size.0 / k, size.1 / k),
                ));
            }
        }
        self.controls.push((format!("Resize {}", slot + 1), grip));
        grip
    }

    /// The mark of a log window's corner it is sized by: a fine bronze angle, lit under the
    /// pointer or while it is held.
    fn grip_mark(&self, p: &mut Painter<'_>, ctx: &Ctx<'_>, slot: usize, grip: Rect) {
        let k = p.scale;
        let lit = ctx.input.hover(&grip) || self.resizing.is_some_and(|(s, _)| s == slot);
        let colour = if lit { 0xFFE8_D29A } else { 0xA0C8_B27A };
        let t = 2.0 * k;
        p.fill(
            Rect::new(grip.x + 3.0 * k, grip.bottom() - t - k, grip.w - 4.0 * k, t),
            colour,
        );
        p.fill(
            Rect::new(grip.right() - t - k, grip.y + 3.0 * k, t, grip.h - 4.0 * k),
            colour,
        );
    }

    /// The popped-out windows and the menus, over the HUD, with [`Self::dock`] placed.
    pub fn overlays(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let windows = windows_of(state);
        let body = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let r = self.dock;
        // The open menu is over the popped-out windows: they do not have the pointer under it.
        let occluders = ctx.input.occluders.len();
        if let Some(r) = self.menu_rect() {
            ctx.input.occluders.push(r);
        }
        for (i, window) in windows.iter().enumerate() {
            if self.floating[i].open {
                self.popped(p, ctx, window, i, &body, out);
            }
        }
        ctx.input.occluders.truncate(occluders);
        self.menus(p, ctx, state, &windows, r, out);
        // Control-C (or Control-Insert) copies what is selected in the lines, when no text being
        // edited took the key for its own selection.
        if let Some(sel) = self.selection.filter(|s| s.anchor != s.end) {
            if ctx.input.ctrl && (ctx.input.take_key(0x43) || ctx.input.take_key(0x2D)) {
                if let Some(window) = windows.get(sel.slot) {
                    ctx.input.copied = Some(self.selected_text(window, sel));
                }
            }
        }
    }

    /// Tab `slot` popped out of the dock: a window as the docked one looks without its input
    /// line, its one tab over its lines, moved by dragging the tab.
    fn popped(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        window: &ChatWindow,
        slot: usize,
        body: &TextStyle,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        let tab_h = 24.0 * k;
        let least = Self::smallest(slot, k);
        let (w, whole_h) = self.window_size(slot, k);
        let (w, whole_h) = (w.min(sw).max(least.0), whole_h.min(sh).max(least.1));
        let h = whole_h - tab_h;
        // Where it was moved to while out, else where it was last put, else its own place in
        // the column over the docked window; kept in layout units, so a new scale keeps it.
        let (mut x, mut y) = self.placed[slot]
            .or(self.kept_place(slot))
            .map_or_else(|| self.popped_default(slot, k), |(x, y)| (x * k, y * k));
        // Held on the screen.
        let hold = |x: f32, y: f32| {
            (
                x.clamp(0.0, (sw - w).max(0.0)),
                y.clamp(0.0, (sh - tab_h - h).max(0.0)),
            )
        };
        (x, y) = hold(x, y);
        let name = self.tab_names.name(slot).to_owned();
        let (mx, my) = ctx.input.mouse;
        let editing = self.editing;
        let float = &mut self.floating[slot];
        let tab = |p: &Painter<'_>, x: f32, y: f32| {
            kit::tab_rects_for(p, Rect::new(x, y, w, tab_h), &[name.as_str()], 0)[0]
        };
        // Moved by its tab; while the HUD Layout window is open, by anywhere on it, as the HUD's
        // elements are.
        let handle = if editing {
            Rect::new(x, y, w, whole_h)
        } else {
            tab(p, x, y)
        };
        if ctx.input.pressed[0] && ctx.input.hover(&handle) {
            ctx.input.pressed[0] = false;
            ctx.input.captured = true;
            float.grab = Some((mx - x, my - y));
            // Taking hold of a window closes the Filters menu.
            self.menu = None;
        }
        let float = &mut self.floating[slot];
        let mut let_go = false;
        if let Some((gx, gy)) = float.grab {
            if ctx.input.down[0] {
                (x, y) = (mx - gx, my - gy);
                ctx.input.captured = true;
            } else {
                float.grab = None;
                let_go = true;
            }
        }
        (x, y) = hold(x, y);
        float.pos = Some((x, y));
        if float.grab.is_some() || let_go {
            self.placed[slot] = Some((x / k, y / k));
        }
        // Where it was let go is kept for the next time it pops out; while the HUD Layout window
        // is open, once the layout is saved.
        if let_go && editing {
            self.places_unsaved = true;
        } else if let_go {
            out.settings.push((
                crate::options::popped_key(slot),
                format!("{},{}", x / k, y / k),
            ));
        }
        let whole = Rect::new(x, y, w, whole_h);
        // While the HUD Layout window is open the wheel over it makes it larger or smaller, a
        // tenth of its first size a notch, kept once the layout is saved.
        if editing && ctx.input.wheel != 0.0 && ctx.input.hover(&whole) {
            let first = self.default_size(slot, k);
            let step = if ctx.input.wheel > 0.0 { 0.1 } else { -0.1 };
            let size = (
                (w + first.0 * step).clamp(least.0, sw.max(least.0)),
                (whole_h + first.1 * step).clamp(least.1, sh.max(least.1)),
            );
            self.sizes[slot] = Some((size.0 / k, size.1 / k));
            self.places_unsaved = true;
            ctx.input.wheel = 0.0;
        }
        let dragging = self.floating[slot].grab.is_some();
        self.popped_rects[slot] = Some(whole);
        let tab_bar = Rect::new(x, y, w, tab_h);
        let tr = tab(p, x, y);
        let lines = Rect::new(x, y + tab_h, w, h);
        let active = dragging || ctx.input.hover(&whole);
        p.fill(lines, ground(self.opacity.get(Some(slot), active)));
        self.controls.push((name.clone(), tr));
        // While the HUD Layout window is open, what is in it has no pointer.
        let pointer = ctx.input.mouse;
        if editing {
            ctx.input.mouse = (-10_000.0, -10_000.0);
        }
        if ctx.input.right_clicked(&tr) {
            self.open_menu(Menu::Tab {
                slot,
                at: tab_bar,
                docked: false,
            });
        }
        kit::tabs(p, ctx, tab_bar, &[name.as_str()], &mut 0);
        // The lines end above the corner the window is sized by.
        let area = Rect::new(
            x + 8.0 * k,
            lines.y + 4.0 * k,
            w - 16.0 * k,
            h - (8.0 + GRIP) * k,
        );
        let grip = if editing {
            None
        } else {
            Some(self.resize_grip(p, ctx, slot, whole, 0.0, out))
        };
        self.draw_lines(p, ctx, window, slot, area, body, out);
        if editing {
            ctx.input.mouse = pointer;
            outline(p, whole, &name);
        }
        if let Some(grip) = grip {
            self.grip_mark(p, ctx, slot, grip);
        }
        if ctx.over_quiet(&whole) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.captured = true;
        }
    }

    /// The text of `sel` among the lines `window` shows, its lines on lines of their own.
    fn selected_text(&self, window: &ChatWindow, sel: LogSelection) -> String {
        let (a, b) = sel.ends();
        let mut out: Vec<String> = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            let at = self.dropped + i;
            if at < a.0 || at > b.0 || !shows_in(window, line) {
                continue;
            }
            let chars: Vec<char> = chat_links(&line.text).0.chars().collect();
            let from = if at == a.0 { a.1.min(chars.len()) } else { 0 };
            let to = if at == b.0 {
                b.1.min(chars.len())
            } else {
                chars.len()
            };
            out.push(chars[from..to.max(from)].iter().collect());
        }
        out.join("\n")
    }

    /// The lines `window` shows, wrapped into `area`, newest at the bottom; the wheel scrolls
    /// them, and a name in a line is a link that begins a tell to it.
    #[allow(clippy::too_many_arguments)]
    fn draw_lines(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        window: &ChatWindow,
        slot: usize,
        area: Rect,
        body: &TextStyle,
        out: &mut Outcome,
    ) {
        let lh = p.line_height(body);
        // The own scrollbar sits inside the window, at its right, and the lines wrap short of it.
        let k = p.scale;
        let area = match kit::scrollbar_width(p) {
            Some(w) => Rect::new(area.x, area.y, area.w - (w + 4.0) * k, area.h),
            None => area,
        };
        let mut lines: Vec<Row> = Vec::new();
        for (i, line) in self
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| shows_in(window, l))
        {
            let at = self.dropped + i;
            let (text, links) = chat_links(&line.text);
            let chars: Vec<char> = text.chars().collect();
            let mut from = 0;
            for wrapped in p.wrap(body, &text, area.w) {
                // Where this piece starts in the whole line, in characters.
                let first = wrapped.chars().next();
                while from < chars.len() && Some(chars[from]) != first {
                    from += 1;
                }
                let len = wrapped.chars().count();
                let here: Vec<Link> = links
                    .iter()
                    .filter(|(s, e, _)| *s < from + len && *e > from)
                    .map(|(s, e, n)| (s.saturating_sub(from), (*e - from).min(len), n.clone()))
                    .collect();
                lines.push((chat_colour(line.chat_type), wrapped, here, at, from));
                from += len;
            }
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let visible = (area.h / lh).floor().max(1.0) as usize;
        let scroll = &mut self.scroll[slot];
        if ctx.over_quiet(&area) && ctx.input.wheel != 0.0 {
            let max = lines.len().saturating_sub(visible);
            if ctx.input.wheel > 0.0 {
                *scroll = (*scroll + 3).min(max);
            } else {
                *scroll = scroll.saturating_sub(3);
            }
            ctx.input.wheel = 0.0;
        }
        // The scrollbar: where the lines shown are among all of them; a press or a drag on it
        // moves there.
        let max = lines.len().saturating_sub(visible);
        // The log is one stop for the pad, scrolled while it is entered.
        {
            let input = &mut *ctx.input;
            input
                .nav
                .list_whole(area, *scroll < max, *scroll > 0, &input.occluders);
        }
        let own = max > 0 && kit::scrollbar_width(p).is_some();
        if own {
            let k = p.scale;
            let w = kit::scrollbar_width(p).unwrap_or(16.0) * k;
            let bar = Rect::new(area.right() + 4.0 * k, area.y, w, area.h);
            #[allow(clippy::cast_precision_loss)]
            let (shown, at) = (
                visible as f32 / lines.len() as f32,
                (max - (*scroll).min(max)) as f32 / max as f32,
            );
            match kit::own_scrollbar(p, ctx, bar, shown, at).flatten() {
                // Up is back through the log: the scroll counts lines back from the newest.
                Some(kit::ScrollAct::Step(n)) => {
                    *scroll = if n < 0 {
                        (*scroll + 3).min(max)
                    } else {
                        scroll.saturating_sub(3)
                    };
                }
                Some(kit::ScrollAct::To(t)) => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let from_top = (t * max as f32).round() as usize;
                    *scroll = max - from_top.min(max);
                }
                None => {}
            }
        } else if max > 0 {
            let k = p.scale;
            let track = Rect::new(area.right() + 2.0 * k, area.y, 4.0 * k, area.h);
            let grab = Rect::new(track.x - 4.0 * k, track.y, track.w + 8.0 * k, track.h);
            if ctx.over_quiet(&grab) && ctx.input.down[0] {
                ctx.input.pressed[0] = false;
                ctx.input.captured = true;
                let t = ((ctx.input.mouse.1 - track.y) / track.h).clamp(0.0, 1.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let from_top = (t * max as f32).round() as usize;
                *scroll = max - from_top.min(max);
            }
            p.fill(track, 0x6000_0000);
            #[allow(clippy::cast_precision_loss)]
            let (thumb_h, at) = (
                (track.h * visible as f32 / lines.len() as f32).max(12.0 * k),
                (max - (*scroll).min(max)) as f32 / max as f32,
            );
            p.fill(
                Rect::new(
                    track.x,
                    track.y + (track.h - thumb_h) * at,
                    track.w,
                    thumb_h,
                ),
                0xC0C8_B27A,
            );
        }
        let end = lines.len().saturating_sub(*scroll);
        let start = end.saturating_sub(visible);
        p.list.push_clip(area);
        #[allow(clippy::cast_precision_loss)]
        let mut ly = area.bottom() - lh * (end - start) as f32;
        let selected = self
            .selection
            .filter(|s| s.slot == slot && s.anchor != s.end)
            .map(|s| s.ends());
        for (colour, text, links, at, from) in &lines[start..end] {
            // The selected part of the row, lit over its text.
            let lit = selected.and_then(|(a, b)| {
                let len = text.chars().count();
                let (s, e) = (a.max((*at, *from)), b.min((*at, from + len)));
                (s < e).then(|| {
                    let chars: Vec<char> = text.chars().collect();
                    let upto = |c: usize| -> String { chars[..c - from].iter().collect() };
                    let x0 = area.x + p.measure(body, &upto(s.1));
                    let x1 = area.x + p.measure(body, &upto(e.1));
                    Rect::new(x0, ly, x1 - x0, lh)
                })
            });
            if links.is_empty() {
                p.text(&body.colour(*colour), area.x, ly, text);
            } else {
                // The line in pieces: plain, a name, plain; a name is lit and can be clicked.
                let chars: Vec<char> = text.chars().collect();
                let mut x = area.x;
                let mut at = 0;
                for (s, e, name) in links {
                    let before: String = chars[at.min(chars.len())..(*s).min(chars.len())]
                        .iter()
                        .collect();
                    x += p.text(&body.colour(*colour), x, ly, &before);
                    let shown: String = chars[(*s).min(chars.len())..(*e).min(chars.len())]
                        .iter()
                        .collect();
                    let w = p.measure(body, &shown);
                    let r = Rect::new(x, ly, w, lh);
                    let over = ctx.over(&r);
                    p.text(
                        &body.colour(if over { 0xFFFF_F0B0 } else { 0xFFB0_D8FF }),
                        x,
                        ly,
                        &shown,
                    );
                    if over && ctx.input.clicked(&r) {
                        out.requests.push(UiRequest::ChatEntry {
                            window: window::MAIN,
                            text: self.typing.clone().unwrap_or_default(),
                            action: EntryAction::StartTell { name: name.clone() },
                        });
                    }
                    x += w;
                    at = *e;
                }
                let after: String = chars[at.min(chars.len())..].iter().collect();
                p.text(&body.colour(*colour), x, ly, &after);
            }
            if let Some(lit) = lit {
                p.fill(lit, crate::ui::edit::SELECTION);
            }
            ly += lh;
        }
        p.list.pop_clip();
        self.select(p, ctx, slot, &lines, start..end, area, body);
    }

    /// The pointer selecting in a window's lines: a press on them (not on a name) starts a
    /// selection, a drag carries it, the lines scrolling when the pointer goes above or below
    /// them, and a press let go where it began selects nothing.
    #[allow(clippy::too_many_arguments)]
    fn select(
        &mut self,
        p: &Painter<'_>,
        ctx: &mut Ctx<'_>,
        slot: usize,
        lines: &[Row],
        shown: std::ops::Range<usize>,
        area: Rect,
        body: &TextStyle,
    ) {
        if shown.is_empty() {
            return;
        }
        let lh = p.line_height(body);
        #[allow(clippy::cast_precision_loss)]
        let top = area.bottom() - lh * shown.len() as f32;
        let (mx, my) = ctx.input.mouse;
        let hit = |mx: f32, my: f32| -> (usize, usize) {
            if my < top {
                return (lines[shown.start].3, lines[shown.start].4);
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let row = (shown.start + ((my - top) / lh) as usize).min(shown.end - 1);
            let (_, text, _, at, from) = &lines[row];
            let chars: Vec<char> = text.chars().collect();
            let col = if my > area.bottom() {
                chars.len()
            } else {
                crate::ui::edit::char_at(p, body, &chars, mx - area.x)
            };
            (*at, from + col)
        };
        if ctx.input.pressed[0] && ctx.over_quiet(&area) {
            ctx.input.pressed[0] = false;
            ctx.input.captured = true;
            let here = hit(mx, my);
            self.selection = Some(LogSelection {
                slot,
                anchor: here,
                end: here,
                dragging: true,
            });
            return;
        }
        let Some(sel) = self
            .selection
            .as_mut()
            .filter(|s| s.slot == slot && s.dragging)
        else {
            return;
        };
        if ctx.input.down[0] {
            sel.end = hit(mx, my);
            let max = lines.len().saturating_sub(shown.len());
            let scroll = &mut self.scroll[slot];
            if my < area.y {
                *scroll = (*scroll + 1).min(max);
            } else if my > area.bottom() {
                *scroll = scroll.saturating_sub(1);
            }
        } else {
            sel.dragging = false;
            if sel.anchor == sel.end {
                self.selection = None;
            }
        }
    }

    /// The input line: Enter opens it with the game's draft, Enter sends to the destination,
    /// Escape closes it and the game keeps the draft, Up and Down walk the lines sent, Tab steps
    /// to the next destination, and a click on the destination's label lists them.
    #[allow(clippy::too_many_arguments)]
    fn input_line(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        r: Rect,
        body: &TextStyle,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let input = Rect::new(
            r.x + 4.0 * k,
            r.bottom() - 28.0 * k,
            r.w - 8.0 * k,
            24.0 * k,
        );
        let focus_label = line_label(state, state.chat_focus.focus);
        let Some(mut text) = self.typing.take() else {
            return;
        };
        ctx.input.text_focus = true;
        if self.slash_typed {
            if let Some(i) = ctx.input.chars.iter().position(|c| *c == '/') {
                ctx.input.chars.remove(i);
                self.slash_typed = false;
            } else if !ctx.input.chars.is_empty() {
                self.slash_typed = false;
            }
        }
        let line_key = crate::ui::edit::line_key(input, 0x0C4A_7000);
        crate::ui::edit::begin(ctx.input, line_key, &text);
        let pressed_in = ctx.input.pressed[0] && ctx.input.hover(&self.text_area);
        if pressed_in {
            ctx.input.pressed[0] = false;
            ctx.input.captured = true;
        }
        let double = pressed_in && ctx.input.double;
        // The pointer on the text as it was drawn last frame, after the label.
        crate::ui::edit::pointer(
            p,
            ctx.input,
            body,
            &text,
            self.text_area,
            pressed_in,
            double,
        );
        crate::ui::edit::keys(ctx.input, &mut text, MAX_LINE);
        let entry = |text: &str, action| UiRequest::ChatEntry {
            window: window::MAIN,
            text: text.to_owned(),
            action,
        };
        // A channel's command at the start of the line switches the talk focus to it and is
        // taken off the line: the focus stays until another is chosen, as the line is retyped.
        if let Some((focus, rest)) = focus_command(&text) {
            if focus_usable(&state.chat_focus, focus) {
                out.requests.push(UiRequest::SetTalkFocus { focus });
                text = rest.to_owned();
            }
        }
        if ctx.input.take_key(vk::TAB) {
            if let Some(focus) = next_focus(&state.chat_focus) {
                out.requests.push(UiRequest::SetTalkFocus { focus });
            }
        }
        if ctx.input.take_key(vk::UP) {
            out.requests.push(entry(&text, EntryAction::Previous));
        }
        if ctx.input.take_key(vk::DOWN) {
            out.requests.push(entry(&text, EntryAction::Next));
        }
        let send = ctx.input.take_key(vk::ENTER);
        // Escape, or a press anywhere off the log window, lets the keyboard go back to the game;
        // the line is kept for the next opening.
        let (mx, my) = ctx.input.mouse;
        let pressed_away = (ctx.input.pressed[0] || ctx.input.pressed[1]) && !r.contains(mx, my);
        let cancel = ctx.input.take_key(vk::ESCAPE) || pressed_away;
        p.fill(input, 0xE020_1C14);
        p.fill(Rect::new(input.x, input.y, input.w, 1.0 * k), 0xFFC8_B27A);
        let (label, colour) =
            channel_label(&text, (&focus_label, focus_colour(state.chat_focus.focus)));
        let channel = TextStyle::new(Family::Body, 12.0, colour).edge(0xFF00_0000);
        let cw = p.text(&channel, input.x + 6.0 * k, input.y + 4.0 * k, &label);
        let label_rect = Rect::new(input.x, input.y, cw + 12.0 * k, input.h);
        if ctx.over(&label_rect) && ctx.input.clicked(&label_rect) {
            self.open_menu(Menu::Focus);
        }
        let tx = input.x + cw + 16.0 * k;
        self.text_area = Rect::new(
            tx,
            input.y,
            (input.right() - 6.0 * k - tx).max(0.0),
            input.h,
        );
        crate::ui::edit::begin(ctx.input, line_key, &text);
        crate::ui::edit::draw(
            p,
            &mut ctx.input.edit,
            body,
            &text,
            self.text_area,
            input.y + 4.0 * k,
            input.y + 4.0 * k,
            (ctx.time * 2.0).fract() < 0.5,
        );
        if send {
            // A channel's command sent alone (`/cg`) chooses that channel and sends nothing.
            let alone = focus_command(&format!("{} ", text.trim()))
                .filter(|(_, rest)| rest.is_empty())
                .map(|(focus, _)| focus);
            if let Some(focus) = alone {
                if focus_usable(&state.chat_focus, focus) {
                    out.requests.push(UiRequest::SetTalkFocus { focus });
                }
                // Nothing was said, so the game keeps no draft of the command.
                out.requests.push(entry("", EntryAction::Draft));
            } else if !text.trim().is_empty() {
                out.requests.push(UiRequest::ChatLine {
                    text,
                    window: window::MAIN,
                });
            }
            self.told.clear();
            self.scroll[0] = 0;
            return;
        }
        if text != self.told {
            out.requests.push(entry(&text, EntryAction::Draft));
            self.told.clone_from(&text);
        }
        if cancel {
            // The game keeps the line, and the next opening begins with it.
            return;
        }
        self.typing = Some(text);
    }

    /// A tab's menu, titled Filters, with a button beside the title that pops the tab out or
    /// docks it (not the main tab's, which stays docked), over its window's filter groups in columns of check boxes: two, or three when
    /// two would not fit above `at`. It opens above `at`, its left edge on `at`'s, or below it
    /// where there is no room above. The menu's rect, and whether the button was pressed.
    fn filter_menu(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        w: &ChatWindow,
        slot: usize,
        at: Rect,
        out: &mut Outcome,
    ) -> (Rect, bool) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        let text = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let heading =
            TextStyle::new(Family::Heading, 18.4, ctx.colours.heading()).edge(ctx.colours.edge());
        let groups = filter_groups(w.id);
        let (left, top, right, bottom) = kit::panel_inset(p);
        let row = 24.0 * k;
        let title_h = 34.0 * k;
        let button_w = 110.0 * k;
        let gap = 8.0 * k;
        // A column as wide as its longest caption beside its box, and a gap.
        let column = groups
            .iter()
            .map(|(caption, _)| p.measure(&text, caption))
            .fold(0.0_f32, f32::max)
            + 28.0 * k
            + 16.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let size = |columns: usize| {
            let rows = groups.len().div_ceil(columns);
            (
                left + right + 2.0 * gap + column * columns as f32,
                top + title_h + row * rows as f32 + gap + bottom,
            )
        };
        let columns = if size(2).1 + 2.0 * k > at.y { 3 } else { 2 };
        let rows = groups.len().div_ceil(columns);
        let size = size(columns);
        let x = at.x.clamp(0.0, (sw - size.0).max(0.0));
        let above = at.y - size.1 - 2.0 * k;
        let y = if above >= 0.0 {
            above
        } else {
            (at.bottom() + 2.0 * k).min(sh - size.1).max(0.0)
        };
        let area = Rect::new(x, y, size.0, size.1);
        kit::panel(p, area, 0.95);
        let inner = Rect::new(
            area.x + left + gap,
            area.y + top,
            area.w - left - right - 2.0 * gap,
            area.h - top - bottom,
        );
        p.text_in(
            &heading,
            Rect::new(inner.x, inner.y, inner.w - button_w, title_h),
            Align::Left,
            "Filters",
        );
        // Down the first column, then the next.
        for (n, (caption, mask)) in groups.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let rr = Rect::new(
                inner.x + column * (n / rows) as f32,
                inner.y + title_h + row * (n % rows) as f32,
                column,
                row,
            );
            let on = w.filter & mask == *mask;
            self.controls
                .push(((*caption).to_owned(), Rect::new(rr.x, rr.y, 20.0 * k, rr.h)));
            if let Some(now) = kit::check_row(p, ctx, rr, on, &text, caption) {
                let filter = if now {
                    w.filter | mask
                } else {
                    w.filter & !mask
                };
                out.requests.push(UiRequest::SetChatWindowFilter {
                    window: w.id,
                    mask: filter,
                });
            }
        }
        // The main tab stays docked.
        if slot == 0 {
            return (area, false);
        }
        let float = &mut self.floating[slot];
        let label = if float.open { "Dock" } else { "Pop Out" };
        let button = Rect::new(
            inner.right() - button_w,
            inner.y + 3.0 * k,
            button_w,
            title_h - 6.0 * k,
        );
        self.controls.push((label.to_owned(), button));
        let pressed = kit::button(p, ctx, button, label, true);
        if pressed {
            float.open = !float.open;
            float.opened_at = ctx.time;
            self.placed[slot] = None;
        }
        (area, pressed)
    }

    /// The menus over the log window: the destinations, or a tab's.
    fn menus(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        windows: &[ChatWindow],
        r: Rect,
        out: &mut Outcome,
    ) {
        let Some(menu) = self.menu else {
            return;
        };
        let k = p.scale;
        let row = 22.0 * k;
        let text = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let dim = text.colour(0xFF80_8080);
        let mut chosen = false;
        let area = match menu {
            Menu::Focus => {
                let focuses = menu_focuses(state.chat_focus.is_olthoi);
                #[allow(clippy::cast_precision_loss)]
                let area = Rect::new(
                    r.x + 4.0 * k,
                    r.bottom() - 30.0 * k - row * focuses.len() as f32,
                    180.0 * k,
                    row * focuses.len() as f32,
                );
                kit::panel(p, area, 0.95);
                for (n, focus) in focuses.iter().enumerate() {
                    #[allow(clippy::cast_precision_loss)]
                    let rr = Rect::new(area.x, area.y + row * n as f32, area.w, row);
                    let usable = focus_usable(&state.chat_focus, *focus);
                    let mut label = focus_label(state, *focus);
                    if *focus == 2 {
                        if let Some(t) = &state.chat_focus.target {
                            label = format!("{label}: {}", t.name);
                        }
                    }
                    if usable && ctx.over(&rr) {
                        p.fill(rr, 0x40FF_FFFF);
                    }
                    let style = if usable {
                        text.colour(focus_colour(*focus))
                    } else {
                        dim
                    };
                    p.text(&style, rr.x + 10.0 * k, rr.y + 4.0 * k, &label);
                    if usable && ctx.input.clicked(&rr) {
                        out.requests.push(UiRequest::SetTalkFocus { focus: *focus });
                        chosen = true;
                    }
                }
                area
            }
            Menu::Tab { slot, at, docked } => {
                let Some(w) = windows.get(slot) else {
                    self.menu = None;
                    return;
                };
                // A docked tab's bar, where the HUD layout placed the docked window.
                let at = if docked {
                    let (d, s) = (self.dock, self.dock_drawn);
                    let f = if s.w > 0.0 { d.w / s.w } else { 1.0 };
                    Rect::new(
                        d.x + (at.x - s.x) * f,
                        d.y + (at.y - s.y) * f,
                        at.w * f,
                        at.h * f,
                    )
                } else {
                    at
                };
                let (area, pressed) = self.filter_menu(p, ctx, w, slot, at, out);
                chosen = pressed;
                area
            }
        };
        self.menu_rect = Some(area);
        if ctx.over(&area) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.captured = true;
        }
        // Any other use of the pointer or the keys closes it: a press or a release off it (a
        // press something under it already took still lets go), the wheel off it, a key, a
        // character typed or one of the game's actions. The release of the press that opened it
        // does not.
        let escape = ctx.input.take_key(vk::ESCAPE);
        let input = &ctx.input;
        let off = !area.contains(input.mouse.0, input.mouse.1);
        let released = input.released.iter().any(|r| *r);
        let fresh = self.menu_fresh;
        self.menu_fresh = fresh && !released;
        let elsewhere = off
            && ((input.pressed.iter().any(|b| *b) && !fresh)
                || (released && !fresh)
                || input.wheel != 0.0);
        let keyed = !input.keys.is_empty() || !input.chars.is_empty() || !input.actions.is_empty();
        if chosen || elsewhere || escape || keyed {
            self.menu = None;
        }
    }
}

/// A popped-out log window's outline while the HUD Layout window is open, as the HUD's elements
/// are outlined there, with its tab's name.
fn outline(p: &mut Painter<'_>, r: Rect, name: &str) {
    let k = p.scale;
    p.fill(r, 0x20FF_FFFF);
    for side in [
        Rect::new(r.x, r.y, r.w, 1.5),
        Rect::new(r.x, r.bottom() - 1.5, r.w, 1.5),
        Rect::new(r.x, r.y, 1.5, r.h),
        Rect::new(r.right() - 1.5, r.y, 1.5, r.h),
    ] {
        p.fill(side, 0xC0FF_FFFF);
    }
    let label = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
    p.text(
        &label,
        r.x + 4.0 * k,
        r.y + 2.0 * k,
        &format!("Log Window: {name}"),
    );
}

/// A log window's ground at `opacity`.
fn ground(opacity: f32) -> Argb {
    crate::draw::with_alpha(0xFF10_1010, opacity)
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

/// A destination's label: the game's own word for it, else a plain one.
/// The destination's name on the input line, in full (General, where the game's menu says Gen),
/// so the channel the player is talking in reads plainly while they type.
fn line_label(state: &GameState, focus: u32) -> String {
    FOCUS_WORDS
        .get(focus as usize)
        .filter(|w| !w.is_empty())
        .map_or_else(|| focus_label(state, focus), |w| (*w).to_owned())
}

fn focus_label(state: &GameState, focus: u32) -> String {
    state
        .chat_focus_labels
        .get(focus as usize)
        .filter(|l| !l.is_empty())
        .cloned()
        .unwrap_or_else(|| {
            FOCUS_WORDS
                .get(focus as usize)
                .copied()
                .unwrap_or("Chat")
                .to_owned()
        })
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
pub fn filter_groups(id: u32) -> Vec<(&'static str, u64)> {
    use dereth_client_contract::options::interface::Interface;
    use dereth_client_contract::options::sheet::{rows_for, PageId, Value};
    rows_for(PageId::Chat, Interface::Horizon)
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
    fn the_input_line_names_the_saved_channel_in_full_where_the_game_s_menu_shortens_it() {
        let mut labels = vec![String::new(); 14];
        labels[8] = "Gen".into();
        let state = GameState {
            chat_focus_labels: labels,
            ..GameState::default()
        };
        assert_eq!(line_label(&state, 8), "General");
        assert_eq!(
            focus_label(&state, 8),
            "Gen",
            "the menu keeps the game's word"
        );
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
        assert_eq!(filter_groups(window::MAIN).len(), 12);
        assert_eq!(filter_groups(window::FLOATY_2).len(), 13);
        assert_eq!(filter_groups(window::FLOATY_2)[0].0, "Gameplay");
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
