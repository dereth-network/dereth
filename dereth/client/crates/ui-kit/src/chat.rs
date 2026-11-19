//! The log window any game's interface can draw: the game's chat windows as its tabs, and the
//! input line under them.
//!
//! Each tab is one of the game's chat windows, showing what the game says that window shows; a
//! tab can be popped out into a window that looks as the docked one does, moved by dragging its
//! tab, sized by its corner and docked again; its filter is set from its menu, titled Filters. The
//! input line speaks to the talk-focus destination, which the game holds: a click on its name
//! lists the destinations to pick from, Tab steps to the next one the player may use, and a
//! channel's command at the start of the line switches to that channel (sent alone, it switches
//! and sends nothing). The lines can be scrolled, selected with the pointer and copied, and the
//! links in them clicked.
//!
//! What differs between the games is the game's to say ([`ChatGame`]): its windows and their
//! filters, its lines and their colours, the destinations and their commands, the filters' groups,
//! the tabs' names and the windows' opacities. What the log window asks of the game comes out as
//! [`ChatEvent`]s, in order, for the game's interface to carry out.

use crate::art::Family;
use crate::draw::{Argb, Rect};
use crate::input::vk;
use crate::paint::{Align, Painter, TextStyle};
use crate::widgets::{self as kit, Ctx};

/// The side of a log window's corner it is sized by, in layout units.
const GRIP: f32 = crate::hud::GRIP;

/// How many chat lines the log keeps.
pub const CHAT_HISTORY: usize = 400;

/// The longest line the input takes.
pub const MAX_LINE: usize = 250;

/// The room a destination's name has in the talk-focus menu and on the input line, in layout
/// units: the menu is as wide as its widest name, between these two, and a name wider than the
/// most is cut short with an ellipsis. Every ordinary name fits whole.
pub const MODE_NAME_LEAST: f32 = 120.0;
pub const MODE_NAME_MOST: f32 = 160.0;

/// The space between the inside of the talk-focus menu's border and its rows' names, across and
/// down, in layout units.
pub const MODE_PAD: (f32, f32) = (8.0, 4.0);

/// The log window's menus' ground: dark and opaque, so the lines under a menu do not show
/// through it.
const MENU_GROUND: Argb = 0xFF12_161C;

/// The log window's menus' frame: a fine bronze rule round the ground and a shadow inside it,
/// each a pixel at the first scale; what the menus' contents stand inside, in layout units.
pub const MENU_RIM: f32 = 2.0;

/// A menu of the log window over `r`: its opaque ground in its thin frame, its contents to stand
/// [`MENU_RIM`] inside its edge.
fn menu_plate(p: &mut Painter<'_>, r: Rect) {
    let line = MENU_RIM / 2.0 * p.scale;
    p.fill(r, MENU_GROUND);
    p.outline(r, line, 0xC0A0_7C48);
    p.outline(r.inset(line), line, 0x6000_0000);
}

/// What clicking a link in a chat line does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// Begins a tell to this name.
    Name(String),
    /// Asks to open this web address in the browser.
    Web(String),
}

/// A link in a chat line: its first and last-plus-one character, and what clicking it does.
pub type Link = (usize, usize, LinkTarget);

/// One of the game's chat windows: its id, and what its filter lets through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub id: u32,
    pub filter: u64,
}

/// A destination the talk-focus menu lists: its id, its words there and their colour, and whether
/// it can be picked now (one that cannot is listed dimmed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusItem {
    pub focus: u32,
    pub label: String,
    pub colour: Argb,
    pub usable: bool,
}

/// What the log window asks of the game, in the order it was asked.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    /// The line as it now stands in the input, or the line before or after it among those sent.
    Entry { text: String, act: EntryAct },
    /// Speak to destination `focus` from now on.
    Focus(u32),
    /// Say `text` to the destination.
    Say(String),
    /// A name in a line was clicked: begin a tell to it, the input holding `text`.
    Tell { name: String, text: String },
    /// A web address in a line was clicked.
    OpenLink(String),
    /// Chat window `window`'s filter set to `mask`.
    Filter { window: u32, mask: u64 },
    /// Keep where popped-out tab `slot` was put, in layout units; `None` for its own place.
    Place { slot: usize, at: Option<(f32, f32)> },
    /// Keep how large log window `slot` was made, in layout units; `None` for its first size.
    Size {
        slot: usize,
        size: Option<(f32, f32)>,
    },
}

/// What a [`ChatEvent::Entry`] tells the game of its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryAct {
    /// The line as it stands.
    Draft,
    /// Up: the line sent before the one shown.
    Previous,
    /// Down: the line sent after it.
    Next,
}

/// What the log window reads of the game.
pub trait ChatGame {
    /// A line of the log.
    type Line: Clone;
    /// The game's chat windows, in the order the tabs list them, the main one at least.
    fn windows(&self) -> Vec<Window>;
    /// A line's text, as the game gives it.
    fn text<'l>(&self, line: &'l Self::Line) -> &'l str;
    /// The colour a line is drawn in.
    fn colour(&self, line: &Self::Line) -> Argb;
    /// Whether `line` shows in `window`.
    fn shows(&self, window: &Window, line: &Self::Line) -> bool;
    /// A line's text as it is drawn, and the links in it: none unless the game has them.
    fn links(&self, text: &str) -> (String, Vec<Link>) {
        (text.to_owned(), Vec::new())
    }
    /// The name tab `slot` shows.
    fn tab_name(&self, slot: usize) -> String;
    /// How opaque a log window's ground is: the docked window's (`None`) or tab `slot`'s popped
    /// out, while it is active (under the pointer, or its line being typed) or not.
    fn opacity(&self, slot: Option<usize>, active: bool) -> f32;
    /// The destination's name on the input line, and its colour.
    fn focus_line(&self) -> (String, Argb);
    /// The destinations the talk-focus menu lists, in its order.
    fn focus_menu(&self) -> Vec<FocusItem>;
    /// Whether destination `focus` can be picked now.
    fn focus_usable(&self, focus: u32) -> bool;
    /// The destination Tab steps to; `None` when there is no other.
    fn next_focus(&self) -> Option<u32>;
    /// The destination a line's opening channel command switches to, and the rest of the line;
    /// `None` unless the line begins with a channel's command and a space.
    fn focus_command<'t>(&self, text: &'t str) -> Option<(u32, &'t str)>;
    /// The name and colour the input line shows before what is typed: the command `text` starts
    /// with, else `focus` (the destination's).
    fn channel_label(&self, text: &str, focus: (&str, Argb)) -> (String, Argb);
    /// The filter groups a tab's menu offers for window `id`: caption and mask.
    fn filter_groups(&self, id: u32) -> Vec<(String, u64)>;
    /// Whether the game has told the windows' filters, so the tab's menu can change them.
    fn filters_known(&self) -> bool {
        true
    }
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
    /// The window it is in, as [`ChatGame::windows`] lists them.
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
#[derive(Debug)]
pub struct ChatLog<L, const N: usize> {
    lines: Vec<L>,
    /// The tab shown in the docked window.
    tab: usize,
    /// Each window's scroll, in lines from the newest.
    scroll: [usize; N],
    /// The windows popped out of the dock, each a window of its own.
    floating: [kit::WindowState; N],
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
    /// Where each tab popped out was last put, in layout units, from this interface's settings.
    pub popped_saved: [Option<(f32, f32)>; N],
    /// Where each popped-out tab stands while it is out, in layout units, once it is moved.
    placed: [Option<(f32, f32)>; N],
    /// Where each popped-out tab's window was drawn last.
    popped_rects: [Option<Rect>; N],
    /// Where the open menu was drawn last.
    menu_rect: Option<Rect>,
    /// HUD Layout's Reset put the windows back where they start: the places and sizes kept in
    /// the settings are not shown until the layout is saved or put back.
    forgotten: bool,
    /// The places and sizes as they stood when the HUD Layout window opened, to put back.
    layout_from: Option<Places<N>>,
    /// HUD Layout moved, sized or reset a window since it was last saved.
    places_unsaved: bool,
    /// HUD Layout's Save: the places and sizes are to be kept in the settings.
    write_places: bool,
    /// How large the player made each log window, in layout units, from this interface's
    /// settings: the docked window's (by the main tab), then each tab's popped out.
    pub sizes_saved: [Option<(f32, f32)>; N],
    /// How large each log window is made while this session resizes it, in layout units.
    sizes: [Option<(f32, f32)>; N],
    /// The log window being resized by its corner, and how far the pointer is from the corner.
    resizing: Option<(usize, (f32, f32))>,
    /// The HUD Layout window is open: the popped-out tabs are moved and sized as its elements.
    pub editing: bool,
    /// Where the docked window would stand with no HUD layout: what the popped-out tabs' column
    /// stands over.
    home: Rect,
    /// Where the docked window's lines are, as it drew them before the HUD layout placed it.
    dock_drawn: Rect,
    /// Where the tabs, the open menu's controls and the web links were drawn this frame, by their
    /// captions (a link's is its address): what a scripted run or a test clicks.
    controls: Vec<(String, Rect)>,
    /// Where the talk-focus menu drew each destination this frame, while it is open, and the
    /// name it wrote there.
    focus_rows: Vec<(u32, Rect, String)>,
    /// The destination's name as the input line wrote it this frame, while the line is open.
    entry_label: String,
    /// Where the input line and the destination's name on it were drawn this frame, while the
    /// line is open.
    entry_at: Option<(Rect, Rect)>,
}

impl<L, const N: usize> Default for ChatLog<L, N> {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            tab: 0,
            scroll: [0; N],
            floating: std::array::from_fn(|_| kit::WindowState::default()),
            typing: None,
            told: String::new(),
            slash_typed: false,
            menu: None,
            menu_fresh: false,
            dock: Rect::default(),
            text_area: Rect::default(),
            dropped: 0,
            selection: None,
            popped_saved: [None; N],
            placed: [None; N],
            popped_rects: [None; N],
            menu_rect: None,
            forgotten: false,
            layout_from: None,
            places_unsaved: false,
            write_places: false,
            sizes_saved: [None; N],
            sizes: [None; N],
            resizing: None,
            editing: false,
            home: Rect::default(),
            dock_drawn: Rect::default(),
            controls: Vec::new(),
            focus_rows: Vec::new(),
            entry_label: String::new(),
            entry_at: None,
        }
    }
}

/// Where each popped-out tab stands and how large each log window is, as this session moved them.
type Places<const N: usize> = ([Option<(f32, f32)>; N], [Option<(f32, f32)>; N]);

impl<L: Clone, const N: usize> ChatLog<L, N> {
    /// Take this frame's lines, after the log is emptied when the game `cleared` it.
    pub fn take(&mut self, cleared: bool, new: impl IntoIterator<Item = L>) {
        if cleared {
            self.dropped += self.lines.len();
            self.lines.clear();
            self.scroll = [0; N];
            self.selection = None;
        }
        self.lines.extend(new);
        if self.lines.len() > CHAT_HISTORY {
            let extra = self.lines.len() - CHAT_HISTORY;
            self.lines.drain(..extra);
            self.dropped += extra;
        }
    }

    /// The game put `text` in the input: it opens with it when the game gives it the keys
    /// (`focus`), and takes it while it is open.
    pub fn entry_told(&mut self, text: &str, focus: bool) {
        if focus || self.typing.is_some() {
            self.typing = Some(text.to_owned());
            self.told = text.to_owned();
        }
    }

    /// The log holds `lines` and nothing else.
    pub fn replace(&mut self, lines: Vec<L>) {
        self.dropped += self.lines.len();
        self.selection = None;
        self.lines = lines;
        if self.lines.len() > CHAT_HISTORY {
            let extra = self.lines.len() - CHAT_HISTORY;
            self.lines.drain(..extra);
        }
        self.scroll = [0; N];
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

    /// Where the control captioned `label` was drawn this frame: a docked tab by its name, a
    /// check box or the button of the open tab menu, or a web link in the lines by its address.
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
        self.placed = [None; N];
        self.sizes = [None; N];
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
    #[must_use]
    pub fn smallest(slot: usize, k: f32) -> (f32, f32) {
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
        (0..N)
            .filter(|i| self.floating[*i].open)
            .filter_map(|i| self.popped_rects[i])
            .collect()
    }

    /// Where tab `slot` stands popped out until it is put elsewhere: in a column over the docked
    /// window at its left edge, the second tab at the top and the last just above the docked
    /// window, each a window's height and a small gap apart; in screen pixels at scale `k`.
    #[must_use]
    pub fn popped_default(&self, slot: usize, k: f32) -> (f32, f32) {
        let d = self.home;
        let whole = self.default_size(1, k).1;
        #[allow(clippy::cast_precision_loss)]
        let above = (N - slot.clamp(1, N - 1)) as f32;
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
    pub fn lines(&self) -> &[L] {
        &self.lines
    }

    /// Which windows are popped out.
    #[must_use]
    pub fn floating(&self) -> [bool; N] {
        std::array::from_fn(|i| self.floating[i].open)
    }

    /// What is typed in the input line, while it is open.
    #[must_use]
    pub fn typed(&self) -> Option<&str> {
        self.typing.as_deref()
    }

    /// The line as the game was last told it: what was typed when the input last closed, empty
    /// once a line was sent.
    #[must_use]
    pub fn told(&self) -> &str {
        &self.told
    }

    /// Open the input line on a `/` its key typed, which the key's own typed `/`, this frame or
    /// the next, does not double.
    pub fn open_command(&mut self) {
        self.open("/".into());
        self.slash_typed = true;
    }

    /// Close the input line, as Escape does: what is typed stays told.
    pub fn close(&mut self) {
        self.typing = None;
    }

    /// Show tab `slot` in the docked window, its newest lines, unless it is popped out.
    pub fn show_tab(&mut self, slot: usize) {
        if slot < N && !self.floating[slot].open {
            self.tab = slot;
            self.scroll[slot] = 0;
        }
    }

    /// Where the talk-focus menu drew destination `focus` this frame, while it is open.
    #[must_use]
    pub fn focus_row(&self, focus: u32) -> Option<Rect> {
        self.menu?;
        self.focus_rows
            .iter()
            .find(|(f, _, _)| *f == focus)
            .map(|(_, r, _)| *r)
    }

    /// The name the talk-focus menu wrote for destination `focus` this frame, while it is open:
    /// the game's whole name for it, or as much as fits and an ellipsis.
    #[must_use]
    pub fn focus_row_name(&self, focus: u32) -> Option<&str> {
        self.menu?;
        self.focus_rows
            .iter()
            .find(|(f, _, _)| *f == focus)
            .map(|(_, _, n)| n.as_str())
    }

    /// The name the input line wrote before what is typed this frame, while it is open, cut short
    /// as the menu cuts it.
    #[must_use]
    pub fn entry_label(&self) -> Option<&str> {
        self.entry_at.map(|_| self.entry_label.as_str())
    }

    /// Whether the talk-focus menu is open.
    #[must_use]
    pub fn focus_menu_open(&self) -> bool {
        matches!(self.menu, Some(Menu::Focus))
    }

    /// Where the input line, and the destination's name on it, were drawn this frame, while it
    /// is open.
    #[must_use]
    pub fn entry_drawn(&self) -> Option<(Rect, Rect)> {
        self.entry_at
    }

    /// The docked log window, standing at `home` (its top-left, and its width at its first size)
    /// before the HUD layout places it; the window as it shows.
    pub fn frame<G: ChatGame<Line = L>>(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        game: &G,
        home: Rect,
        out: &mut Vec<ChatEvent>,
    ) -> Rect {
        let k = p.scale;
        let (sw, sh) = p.screen;
        self.controls.clear();
        self.focus_rows.clear();
        self.entry_at = None;
        if std::mem::take(&mut self.write_places) {
            for slot in 1..N {
                let at = self.placed[slot].or(self.kept_place(slot));
                out.push(ChatEvent::Place { slot, at });
            }
            for slot in 0..N {
                let size = self.sizes[slot].or(self.kept_size(slot));
                out.push(ChatEvent::Size { slot, size });
            }
            self.forgotten = false;
        }
        let x = home.x;
        self.home = home;
        let y = self.home.y;
        // Made larger or smaller from its corner, it keeps its top-left where it starts.
        let (w, h) = self.window_size(0, k);
        let least = Self::smallest(0, k);
        let (w, h) = (w.min(sw - x).max(least.0), h.min(sh - y).max(least.1));
        let r = Rect::new(x, y + 24.0 * k, w, h - 24.0 * k);
        // The docked log is a panel of its own for the pad's focus, which the Back button
        // brings to it.
        ctx.input
            .nav
            .layer(Rect::new(x, y, w, h), "Chat", crate::nav::PanelKind::Chat);
        // The strip the input line sits in shows only while the line is open.
        let strip = if self.typing.is_some() { 0.0 } else { 26.0 * k };
        let shown = Rect::new(x, y, w, h - strip);
        let grip = self.resize_grip(p, ctx, 0, shown, strip, out);
        // Active while the pointer is over it or its line is being typed.
        let active = self.typing.is_some() || ctx.input.hover(&Rect::new(x, y, w, h - strip));
        p.fill(
            Rect::new(r.x, r.y, r.w, r.h - strip),
            ground(game.opacity(None, active)),
        );
        let windows = game.windows();
        // The docked tabs: the windows not popped out.
        let docked: Vec<usize> = (0..windows.len().min(N))
            .filter(|i| !self.floating[*i].open)
            .collect();
        if !docked.contains(&self.tab) {
            self.tab = docked.first().copied().unwrap_or(0);
        }
        let titles: Vec<String> = docked.iter().map(|i| game.tab_name(*i)).collect();
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
            self.draw_lines(p, ctx, game, &windows[tab], tab, text_area, &body, out);
        }
        self.input_line(p, ctx, game, r, &body, out);
        // The pad's focus rests on the input line, where CONFIRM opens it.
        {
            let line = Rect::new(x, y + h - 26.0 * k, w, 26.0 * k);
            let input = &mut *ctx.input;
            input
                .nav
                .note(line, crate::nav::Kind::Text, &input.occluders);
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
        Rect::new(x, y, w, h - strip)
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
        out: &mut Vec<ChatEvent>,
    ) -> Rect {
        let k = p.scale;
        let grip = crate::hud::grip_rect(shown, k);
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
                out.push(ChatEvent::Size {
                    slot: s,
                    size: Some((size.0 / k, size.1 / k)),
                });
            }
        }
        self.controls.push((format!("Resize {}", slot + 1), grip));
        grip
    }

    /// The mark of a log window's corner it is sized by: a fine bronze angle, lit under the
    /// pointer or while it is held.
    fn grip_mark(&self, p: &mut Painter<'_>, ctx: &Ctx<'_>, slot: usize, grip: Rect) {
        let lit = ctx.input.hover(&grip) || self.resizing.is_some_and(|(s, _)| s == slot);
        crate::hud::grip_mark(p, grip, lit);
    }

    /// The popped-out windows and the menus, over the HUD, with [`Self::dock`] placed.
    pub fn overlays<G: ChatGame<Line = L>>(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        game: &G,
        out: &mut Vec<ChatEvent>,
    ) {
        let windows = game.windows();
        let body = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let r = self.dock;
        // The open menu is over the popped-out windows: they do not have the pointer under it.
        let occluders = ctx.input.occluders.len();
        if let Some(r) = self.menu_rect() {
            ctx.input.occluders.push(r);
        }
        for (i, window) in windows.iter().enumerate().take(N) {
            if self.floating[i].open {
                self.popped(p, ctx, game, window, i, &body, out);
            }
        }
        ctx.input.occluders.truncate(occluders);
        self.menus(p, ctx, game, &windows, r, out);
        // Control-C (or Control-Insert) copies what is selected in the lines, when no text being
        // edited took the key for its own selection.
        if let Some(sel) = self.selection.filter(|s| s.anchor != s.end) {
            if ctx.input.ctrl && (ctx.input.take_key(0x43) || ctx.input.take_key(0x2D)) {
                if let Some(window) = windows.get(sel.slot) {
                    ctx.input.copied = Some(self.selected_text(game, window, sel));
                }
            }
        }
    }

    /// Tab `slot` popped out of the dock: a window as the docked one looks without its input
    /// line, its one tab over its lines, moved by dragging the tab.
    #[allow(clippy::too_many_arguments)]
    fn popped<G: ChatGame<Line = L>>(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        game: &G,
        window: &Window,
        slot: usize,
        body: &TextStyle,
        out: &mut Vec<ChatEvent>,
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
        let name = game.tab_name(slot);
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
            out.push(ChatEvent::Place {
                slot,
                at: Some((x / k, y / k)),
            });
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
        p.fill(lines, ground(game.opacity(Some(slot), active)));
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
        self.draw_lines(p, ctx, game, window, slot, area, body, out);
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
    fn selected_text<G: ChatGame<Line = L>>(
        &self,
        game: &G,
        window: &Window,
        sel: LogSelection,
    ) -> String {
        let (a, b) = sel.ends();
        let mut out: Vec<String> = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            let at = self.dropped + i;
            if at < a.0 || at > b.0 || !game.shows(window, line) {
                continue;
            }
            let chars: Vec<char> = game.links(game.text(line)).0.chars().collect();
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
    /// them; a name in a line is a link that begins a tell to it, and a web address one that asks
    /// to be opened in the browser.
    #[allow(clippy::too_many_arguments)]
    fn draw_lines<G: ChatGame<Line = L>>(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        game: &G,
        window: &Window,
        slot: usize,
        area: Rect,
        body: &TextStyle,
        out: &mut Vec<ChatEvent>,
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
            .filter(|(_, l)| game.shows(window, l))
        {
            let at = self.dropped + i;
            let (text, links) = game.links(game.text(line));
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
                lines.push((game.colour(line), wrapped, here, at, from));
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
                // The line in pieces: plain, a link, plain; a link is lit and can be clicked. A
                // name begins a tell; a web address, underlined, asks to be opened.
                let chars: Vec<char> = text.chars().collect();
                let mut x = area.x;
                let mut at = 0;
                for (s, e, target) in links {
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
                    let lit = if over { 0xFFFF_F0B0 } else { 0xFFB0_D8FF };
                    p.text(&body.colour(lit), x, ly, &shown);
                    if let LinkTarget::Web(url) = target {
                        p.fill(Rect::new(x, ly + lh - 2.0 * k, w, k.max(1.0)), lit);
                        self.controls.push((url.clone(), r));
                    }
                    if over && ctx.input.clicked(&r) {
                        out.push(match target {
                            LinkTarget::Name(name) => ChatEvent::Tell {
                                name: name.clone(),
                                text: self.typing.clone().unwrap_or_default(),
                            },
                            LinkTarget::Web(url) => ChatEvent::OpenLink(url.clone()),
                        });
                    }
                    x += w;
                    at = *e;
                }
                let after: String = chars[at.min(chars.len())..].iter().collect();
                p.text(&body.colour(*colour), x, ly, &after);
            }
            if let Some(lit) = lit {
                p.fill(lit, crate::edit::SELECTION);
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
                crate::edit::char_at(p, body, &chars, mx - area.x)
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
    fn input_line<G: ChatGame<Line = L>>(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        game: &G,
        r: Rect,
        body: &TextStyle,
        out: &mut Vec<ChatEvent>,
    ) {
        let k = p.scale;
        let input = Rect::new(
            r.x + 4.0 * k,
            r.bottom() - 28.0 * k,
            r.w - 8.0 * k,
            24.0 * k,
        );
        let (focus_label, focus_colour) = game.focus_line();
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
        let line_key = crate::edit::line_key(input, 0x0C4A_7000);
        crate::edit::begin(ctx.input, line_key, &text);
        let pressed_in = ctx.input.pressed[0] && ctx.input.hover(&self.text_area);
        if pressed_in {
            ctx.input.pressed[0] = false;
            ctx.input.captured = true;
        }
        let double = pressed_in && ctx.input.double;
        // The pointer on the text as it was drawn last frame, after the label.
        crate::edit::pointer(
            p,
            ctx.input,
            body,
            &text,
            self.text_area,
            pressed_in,
            double,
        );
        crate::edit::keys(ctx.input, &mut text, MAX_LINE);
        let entry = |text: &str, act| ChatEvent::Entry {
            text: text.to_owned(),
            act,
        };
        // A channel's command at the start of the line switches the talk focus to it and is
        // taken off the line: the focus stays until another is chosen, as the line is retyped.
        if let Some((focus, rest)) = game.focus_command(&text) {
            if game.focus_usable(focus) {
                out.push(ChatEvent::Focus(focus));
                text = rest.to_owned();
            }
        }
        if ctx.input.take_key(vk::TAB) {
            if let Some(focus) = game.next_focus() {
                out.push(ChatEvent::Focus(focus));
            }
        }
        if ctx.input.take_key(vk::UP) {
            out.push(entry(&text, EntryAct::Previous));
        }
        if ctx.input.take_key(vk::DOWN) {
            out.push(entry(&text, EntryAct::Next));
        }
        let send = ctx.input.take_key(vk::ENTER);
        // Escape, or a press anywhere off the log window, lets the keyboard go back to the game;
        // the line is kept for the next opening.
        let (mx, my) = ctx.input.mouse;
        let pressed_away = (ctx.input.pressed[0] || ctx.input.pressed[1]) && !r.contains(mx, my);
        let cancel = ctx.input.take_key(vk::ESCAPE) || pressed_away;
        crate::hud::entry_line(p, input);
        let (label, colour) = game.channel_label(&text, (&focus_label, focus_colour));
        let channel = TextStyle::new(Family::Body, 12.0, colour).edge(0xFF00_0000);
        // The destination's whole name, cut short as the menu cuts it.
        let label = p.fit(&channel, &label, MODE_NAME_MOST * k);
        let cw = p.text(&channel, input.x + 6.0 * k, input.y + 4.0 * k, &label);
        self.entry_label = label;
        let label_rect = Rect::new(input.x, input.y, cw + 12.0 * k, input.h);
        self.entry_at = Some((input, label_rect));
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
        crate::edit::begin(ctx.input, line_key, &text);
        let blink = (ctx.time * 2.0).fract() < 0.5;
        crate::edit::draw(
            p,
            &mut ctx.input.edit,
            body,
            &text,
            self.text_area,
            input.y + 4.0 * k,
            input.y + 4.0 * k,
            blink,
        );
        if send {
            // A channel's command sent alone (`/cg`) chooses that channel and sends nothing.
            let alone = game
                .focus_command(&format!("{} ", text.trim()))
                .filter(|(_, rest)| rest.is_empty())
                .map(|(focus, _)| focus);
            if let Some(focus) = alone {
                if game.focus_usable(focus) {
                    out.push(ChatEvent::Focus(focus));
                }
                // Nothing was said, so the game keeps no draft of the command.
                out.push(entry("", EntryAct::Draft));
            } else if !text.trim().is_empty() {
                out.push(ChatEvent::Say(text));
            }
            self.told.clear();
            self.scroll[0] = 0;
            return;
        }
        if text != self.told {
            out.push(entry(&text, EntryAct::Draft));
            self.told.clone_from(&text);
        }
        if cancel {
            // The game keeps the line, and the next opening begins with it.
            return;
        }
        self.typing = Some(text);
    }

    /// A tab's menu, titled Filters, with a button beside the title that pops the tab out or
    /// docks it (not the main tab's, which stays docked), over its window's filter groups in
    /// columns of check boxes: two, or three when two would not fit above `at`. It opens above
    /// `at`, its left edge on `at`'s, or below it where there is no room above. Until the game has
    /// told the filters, its boxes are greyed and change nothing. The menu's rect, and whether the
    /// button was pressed.
    #[allow(clippy::too_many_arguments)]
    fn filter_menu<G: ChatGame<Line = L>>(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        game: &G,
        w: &Window,
        slot: usize,
        at: Rect,
        out: &mut Vec<ChatEvent>,
    ) -> (Rect, bool) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        let text = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let heading =
            TextStyle::new(Family::Heading, 18.4, ctx.colours.heading()).edge(ctx.colours.edge());
        let groups = game.filter_groups(w.id);
        let known = game.filters_known();
        // Inside its thin frame, with a little room over its title and button.
        let rim = MENU_RIM * k;
        let (left, top, right, bottom) = (rim, rim + 4.0 * k, rim, rim);
        let row = 24.0 * k;
        let title_h = kit::TITLE_BAR * k;
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
        menu_plate(p, area);
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
                .push((caption.clone(), Rect::new(rr.x, rr.y, 20.0 * k, rr.h)));
            if !known {
                kit::checkbox_greyed(p, Rect::new(rr.x, rr.y, 20.0 * k, rr.h), on);
                let label = Rect::new(rr.x + 28.0 * k, rr.y, rr.w - 28.0 * k, rr.h);
                p.text_in(&text, label, Align::Left, caption);
            } else if let Some(now) = kit::check_row(p, ctx, rr, on, &text, caption) {
                let filter = if now {
                    w.filter | mask
                } else {
                    w.filter & !mask
                };
                out.push(ChatEvent::Filter {
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
            inner.y + 2.0 * k,
            button_w,
            title_h - 4.0 * k,
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
    fn menus<G: ChatGame<Line = L>>(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        game: &G,
        windows: &[Window],
        r: Rect,
        out: &mut Vec<ChatEvent>,
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
                let focuses = game.focus_menu();
                // As wide as its widest name, within the names' room, and padded inside its
                // border; an opaque plate, its rows one under another, none reaching its border.
                let rim = MENU_RIM * k;
                let (left, top, right, bottom) = (rim, rim, rim, rim);
                let (pad_x, pad_y) = (MODE_PAD.0 * k, MODE_PAD.1 * k);
                let names_w = focuses
                    .iter()
                    .map(|item| p.measure(&text, &item.label))
                    .fold(0.0_f32, f32::max)
                    .clamp(MODE_NAME_LEAST * k, MODE_NAME_MOST * k);
                #[allow(clippy::cast_precision_loss)]
                let rows_h = row * focuses.len() as f32;
                let (w, h) = (
                    left + right + names_w + 2.0 * pad_x,
                    top + bottom + rows_h + 2.0 * pad_y,
                );
                let area = Rect::new(r.x + 4.0 * k, (r.bottom() - 30.0 * k - h).max(0.0), w, h);
                menu_plate(p, area);
                // The rows stand inside the border by half the padding; their names by all of it.
                let inner = Rect::new(
                    area.x + left + pad_x / 2.0,
                    area.y + top + pad_y,
                    area.w - left - right - pad_x,
                    rows_h,
                );
                for (n, item) in focuses.iter().enumerate() {
                    #[allow(clippy::cast_precision_loss)]
                    let rr = Rect::new(inner.x, inner.y + row * n as f32, inner.w, row);
                    let usable = item.usable;
                    if usable && ctx.over(&rr) {
                        p.fill(rr, 0x40FF_FFFF);
                    }
                    let style = if usable {
                        text.colour(item.colour)
                    } else {
                        dim
                    };
                    let name = p.fit(&style, &item.label, names_w);
                    p.text_in(
                        &style,
                        Rect::new(area.x + left + pad_x, rr.y, names_w, rr.h),
                        Align::Left,
                        &name,
                    );
                    self.focus_rows.push((item.focus, rr, name));
                    if usable && ctx.input.clicked(&rr) {
                        out.push(ChatEvent::Focus(item.focus));
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
                let (area, pressed) = self.filter_menu(p, ctx, game, w, slot, at, out);
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
    crate::hud::log_ground(opacity)
}
