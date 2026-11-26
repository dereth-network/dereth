//! The settings window's pages of options: the client's own preferences (graphics, sound, the
//! camera and the input, as the game's own options window lists them), the player's options
//! (kept by the server), and the chat windows' options.
//!
//! **The client's preferences** are listed by the client for the page shown, a row each: a box
//! for a yes-or-no preference, a slider for a number, arrows through the choices of one with
//! choices. A preference set is tried, applied and kept at once, as the camera's are on the first
//! page.
//!
//! **The player's options** are the game's own (the same as the game's own options window's
//! character page): each one set is set, told and sent to the server to keep at once.
//!
//! **The chat options:** as Dereth's Horizon's chat page: first the log window's opacities, active
//! and not; then for each of the four chat windows (the log window's tabs) its tab's name, its
//! opacities popped out (but for the first, which stays in the log window), and a box for each
//! kind of text it shows. Each kind set is set and the chat options sent to the server to keep; the
//! names and the opacities are Horizon's own settings, kept in its file. A tab whose name is left
//! blank has its own name. The kinds' font sizes and pop-ups, and the windows' own opacities, which
//! Horizon's log does not use, are left to the game's own options window.

use std::collections::BTreeMap;

use auberean_client_sdk::contract::view::{ChatOptions, GameOptions};
use auberean_client_sdk::contract::{ChatWindowAction, PlayerOption};
use auberean_client_sdk::Action;
use auberean_options::page::{section, Change, Listing, Request as PageRequest};
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx};

use crate::draw::Rect;

/// The pages of the client's preferences, each its name and the sections it lists.
pub const PREFERENCE_PAGES: [(&str, &[u32]); 3] = [
    (
        "Graphics",
        &[section::GRAPHICS, section::EFFECTS, section::AVATARS],
    ),
    ("Sound", &[section::SOUND]),
    ("Camera", &[section::CAMERA, section::INPUT, section::OTHER]),
];

/// The player's yes-or-no options as the character page lists them, with Horizon's names. Those
/// Horizon has no use for are left to the game's own options window: the bar's smooth or instant
/// swap between sets (Horizon's always shows the next set at once), and the damage numbers and
/// player-killer damage (Horizon shows no floating damage text and no player-killer damage
/// meters).
pub const PLAYER_FLAGS: [(PlayerOption, &str); 18] = [
    (PlayerOption::AutoTarget, "Auto-Target"),
    (
        PlayerOption::TakeFellowshipOffers,
        "Accept Fellowship Requests",
    ),
    (
        PlayerOption::TakeAllegianceOffers,
        "Accept Allegiance Requests",
    ),
    (PlayerOption::TakeTradeOffers, "Accept Trade Requests"),
    (PlayerOption::ShortcutNumbers, "See Numbers on Shortcuts"),
    (PlayerOption::ShortcutLocked, "Lock Shortcuts"),
    (
        PlayerOption::AskBeforeMelting,
        "Confirm Before Turning Items To Gold",
    ),
    (PlayerOption::LatencyGraph, "Enable the Bandwidth Meter"),
    (PlayerOption::FellowsOnMap, "Show Fellowship Members on Map"),
    (PlayerOption::PetPanel, "Show the Pet Panel"),
    (PlayerOption::ClassicExamination, "Classic Examination"),
    (PlayerOption::RadarMonsters, "Show Monsters on Minimap"),
    (PlayerOption::RadarPeople, "Show NPCs on Minimap"),
    (PlayerOption::RadarPlayers, "Show Players on Minimap"),
    (PlayerOption::RadarPortals, "Show Portals on Minimap"),
    (PlayerOption::RadarLifestones, "Show Lifestones on Minimap"),
    (PlayerOption::RadarItems, "Show Items on Minimap"),
    (
        PlayerOption::RadarCoordinates,
        "Show Coordinates By the Radar",
    ),
];

/// The kinds of text the chat page lists, each its bit and Horizon's name.
pub const CHAT_KINDS: [(u32, &str); 15] = [
    (0x1, "Errors"),
    (0x2, "Combat"),
    (0x8, "Game"),
    (0x10, "Say"),
    (0x20, "Tells"),
    (0x40, "Emotes"),
    (0x80, "Log"),
    (0x100, "Broadcasts"),
    (0x1_0000, "Fellowship"),
    (0x2_0000, "Allegiance"),
    (0x8_0000, "General"),
    (0x10_0000, "Trade"),
    (0x20_0000, "Region"),
    (0x40_0000, "Kingdom"),
    (0x100_0000, "PvP"),
];

/// The headings of the chat page's windows, in the log window's tabs' order.
pub const CHAT_HEADINGS: [&str; 4] = ["Main Tab", "Tab #2", "Tab #3", "Tab #4"];

/// A row of the chat page: the log window's heading, one of its opacities (the log window's, or
/// a tab's popped out; active or not), a window's heading, its tab's name, or one of the kinds of
/// text it shows (windows from 0, kinds by their bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatRow {
    LogWindow,
    Opacity(Option<usize>, bool),
    Heading(usize),
    Name(usize),
    Kind(usize, u32),
}

/// The heading over the log window's own rows on the chat page.
pub const LOG_WINDOW: &str = "Chat Window";

/// The caption of an opacity's row: the log window's, or a tab's popped out.
#[must_use]
pub const fn opacity_caption(tab: Option<usize>, active: bool) -> &'static str {
    match (tab, active) {
        (None, true) => "Active Opacity",
        (None, false) => "Inactive Opacity",
        (Some(_), true) => "Undocked Opacity (Active)",
        (Some(_), false) => "Undocked Opacity (Inactive)",
    }
}

/// The chat page's rows, top to bottom: the log window's heading and its two opacities, then
/// each window's heading, its tab's name, its two opacities popped out (but for the first), and
/// its kinds.
#[must_use]
pub fn chat_rows() -> Vec<ChatRow> {
    let mut rows = vec![
        ChatRow::LogWindow,
        ChatRow::Opacity(None, true),
        ChatRow::Opacity(None, false),
    ];
    for w in 0..CHAT_HEADINGS.len() {
        rows.extend([ChatRow::Heading(w), ChatRow::Name(w)]);
        if w > 0 {
            rows.extend([
                ChatRow::Opacity(Some(w), true),
                ChatRow::Opacity(Some(w), false),
            ]);
        }
        rows.extend(CHAT_KINDS.iter().map(|(kind, _)| ChatRow::Kind(w, *kind)));
    }
    rows
}

/// Where a kind's box is in its chat page row `r`, at `k` pixels a layout unit.
#[must_use]
pub fn chat_check(r: Rect, k: f32) -> Rect {
    Rect::new(r.x + 16.0 * k, r.y, 20.0 * k, r.h)
}

/// A row's height on a page, in layout units.
pub const ROW: f32 = 26.0;

/// What the pages read of the game and the client, as the interface last gave it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageData {
    /// The client's preferences as listed for the page shown.
    pub listing: Option<Listing>,
    /// The words of the preferences' labels and choices: by string table (an enumeration value)
    /// and string.
    pub words: BTreeMap<(u32, u32), String>,
    /// The player's options, in the world.
    pub options: Option<GameOptions>,
}

impl PageData {
    /// A preference the page set, shown from the next frame: its row in the listing takes the
    /// value set. The client lists a page's preferences as the page attaches them, not as each is
    /// set, so the page keeps what it set itself, as the game's own options window does.
    pub fn fold(&mut self, change: &Change) {
        let Some(l) = &mut self.listing else {
            return;
        };
        let key = key_of(change);
        for row in l.sections.iter_mut().flat_map(|(_, rows)| rows.iter_mut()) {
            if key_of(row) == key {
                row.clone_from(change);
            }
        }
    }
}

/// A page's rows, laid out from `top` in `area`, scrolled by `offset` pixels: the `i`th row.
#[must_use]
pub fn row(area: Rect, offset: f32, i: usize, k: f32) -> Rect {
    #[allow(clippy::cast_precision_loss)]
    let y = area.y + ROW * k * i as f32 - offset;
    Rect::new(area.x, y, area.w - 12.0 * k, (ROW - 2.0) * k)
}

/// The room a page's rows leave at their right for its scrollbar, in pixels: the scrollbar and a
/// gap.
#[must_use]
pub fn gutter(p: &Painter<'_>) -> f32 {
    kit::scrollbar_width(p).map_or(12.0, |w| w + 8.0) * p.scale
}

/// As [`row`], the row ending `gutter` pixels short of the area's right, clear of its scrollbar.
#[must_use]
pub fn page_row(area: Rect, offset: f32, i: usize, k: f32, gutter: f32) -> Rect {
    let r = row(area, offset, i, k);
    Rect::new(r.x, r.y, (area.w - gutter).max(0.0), r.h)
}

/// The control of a row: right of its label.
#[must_use]
pub fn control(r: Rect) -> Rect {
    Rect::new(r.x + r.w * 0.55, r.y, r.w * 0.45, r.h)
}

/// The chat page's tab name boxes' focus ids, the first window's first.
const NAME_FIELD: u32 = 0x4E00;

/// The pages' own state: how far each is scrolled, and what they asked of the client.
#[derive(Debug, Default)]
pub struct Pages {
    scroll: f32,
    /// The chat page's box that has the keys (a tab's name), and what is typed in it.
    typing: u32,
    draft: String,
    /// The requests of the client's preferences asked, carried out by the interface.
    pub asked: Vec<PageRequest>,
    /// Where the page shown drew its controls this frame, and its scrollbar's lane (while it
    /// scrolls).
    pub drawn: Vec<Rect>,
    pub bar: Option<Rect>,
}

/// The words a label names, or what stands in for them.
fn words(data: &PageData, table: u32, string: u32, or: &str) -> String {
    data.words
        .get(&(table, string))
        .filter(|w| !w.is_empty())
        .cloned()
        .unwrap_or_else(|| or.to_owned())
}

/// The key of a preference.
const fn key_of(c: &Change) -> u32 {
    match c {
        Change::Flag(f) => f.description.key,
        Change::Number(n) => n.description.key,
        Change::Choice(c) => c.description.key,
    }
}

impl Pages {
    /// The page scrolled back to its top, as another is shown.
    pub fn top(&mut self) {
        self.scroll = 0.0;
        self.let_go();
    }

    /// A box the page was typing in let go.
    pub fn let_go(&mut self) {
        self.typing = 0;
    }

    /// Whether a box on the page has the keys.
    #[must_use]
    pub const fn typing(&self) -> bool {
        self.typing != 0
    }

    /// Note the scrollbar's lane at the right of `area` while `content` is taller than it.
    fn note_bar(&mut self, p: &Painter<'_>, area: Rect, content: f32) {
        self.bar = (content > area.h).then(|| {
            let w = kit::scrollbar_width(p).unwrap_or(4.0) * p.scale;
            Rect::new(area.right() - w, area.y, w, area.h)
        });
    }

    /// The rows of the preferences in `sections`, as listed.
    #[must_use]
    pub fn listed<'a>(data: &'a PageData, sections: &[u32]) -> Vec<(u32, &'a Change)> {
        let Some(l) = &data.listing else {
            return Vec::new();
        };
        sections
            .iter()
            .filter_map(|s| l.sections.iter().find(|(at, _)| at == s))
            .flat_map(|(s, rows)| rows.iter().map(move |c| (*s, c)))
            .collect()
    }

    /// A page of the client's preferences in `sections`, in `area`. A preference set shows as
    /// set from the next frame ([`PageData::fold`]).
    pub fn preferences(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        area: Rect,
        data: &mut PageData,
        sections: &[u32],
    ) {
        let k = p.scale;
        self.drawn.clear();
        self.bar = None;
        let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let rows = Self::listed(data, sections);
        if rows.is_empty() {
            p.text(&style, area.x, area.y, "Nothing to set here.");
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let content = ROW * k * rows.len() as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.scroll);
        self.note_bar(p, area, content);
        let gutter = gutter(p);
        let mut set = Vec::new();
        p.list.push_clip(area);
        for (i, (s, c)) in rows.into_iter().enumerate() {
            let r = page_row(area, offset, i, k, gutter);
            if r.bottom() < area.y || r.y > area.bottom() {
                continue;
            }
            let d = match c {
                Change::Flag(f) => f.description,
                Change::Number(n) => n.description,
                Change::Choice(c) => c.description,
            };
            let fallback = format!("Preference {:#x}", d.key);
            let label = words(data, d.table, d.label, &fallback);
            self.drawn.push(match c {
                Change::Flag(_) => r,
                _ => control(r),
            });
            let changed = match c {
                Change::Flag(f) => {
                    let mut f = *f;
                    kit::check_row(p, ctx, r, f.get(), &style, &label)
                        .filter(|v| f.set_value(*v))
                        .map(|_| Change::Flag(f))
                }
                Change::Number(n) => {
                    p.text_in(&style, r, Align::Left, &label);
                    let (lo, hi) = n.range();
                    let mut n = *n;
                    kit::slider(p, ctx, control(r), n.get(), lo, hi)
                        .filter(|v| n.set_value(*v))
                        .map(|_| Change::Number(n))
                }
                Change::Choice(c) => {
                    p.text_in(&style, r, Align::Left, &label);
                    let list = c.list_choices();
                    let names: Vec<String> = list
                        .iter()
                        .map(|&(v, (table, string))| {
                            c.literal_choice(v).map_or_else(
                                || words(data, table, string, &v.to_string()),
                                str::to_owned,
                            )
                        })
                        .collect();
                    let names: Vec<&str> = names.iter().map(String::as_str).collect();
                    let at = list.iter().position(|(v, _)| *v == c.get()).unwrap_or(0);
                    let mut c = c.clone();
                    kit::stepper(p, ctx, control(r), &names, at)
                        .filter(|i| *i != at)
                        .and_then(|i| list.get(i))
                        .filter(|(v, _)| c.set_value(*v))
                        .map(|_| Change::Choice(c))
                }
            };
            if let Some(change) = changed {
                debug_assert_eq!(key_of(&change), d.key);
                set.push(change.clone());
                self.asked.push(PageRequest::Change(change));
                self.asked.push(PageRequest::Apply(s));
                self.asked.push(PageRequest::Save);
            }
        }
        p.list.pop_clip();
        for change in &set {
            data.fold(change);
        }
    }

    /// The player's options, in `area`; what they ask is put in `acts`.
    pub fn character(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        area: Rect,
        options: Option<&GameOptions>,
        acts: &mut Vec<Action>,
    ) {
        let k = p.scale;
        let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        self.drawn.clear();
        self.bar = None;
        let Some(o) = options else {
            p.text(&style, area.x, area.y, "Log in to change these.");
            return;
        };
        #[allow(clippy::cast_precision_loss)]
        let content = ROW * k * (PLAYER_FLAGS.len() + 1) as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.scroll);
        self.note_bar(p, area, content);
        let gutter = gutter(p);
        p.list.push_clip(area);
        for (i, (option, name)) in PLAYER_FLAGS.iter().enumerate() {
            let r = page_row(area, offset, i, k, gutter);
            if r.bottom() < area.y || r.y > area.bottom() {
                continue;
            }
            let on = o.flag(*option).unwrap_or(false);
            self.drawn.push(r);
            if let Some(v) = kit::check_row(p, ctx, r, on, &style, name) {
                acts.push(Action::SetOptions {
                    changes: vec![(*option, u32::from(v))],
                    tell: true,
                });
            }
        }
        // The hotbar's rows: one to four.
        let r = page_row(area, offset, PLAYER_FLAGS.len(), k, gutter);
        self.drawn.push(control(r));
        p.text_in(&style, r, Align::Left, "Shortcut Bar Rows");
        let names = ["1", "2", "3", "4"];
        let at = usize::try_from(o.shortcut_height.clamp(1, 4) - 1).unwrap_or(0);
        if let Some(i) = kit::stepper(p, ctx, control(r), &names, at).filter(|i| *i != at) {
            acts.push(Action::SetOptions {
                changes: vec![(
                    PlayerOption::ShortcutHeight,
                    u32::try_from(i + 1).unwrap_or(1),
                )],
                tell: true,
            });
        }
        p.list.pop_clip();
    }

    /// The chat windows' options, in `area`, with Horizon's own settings `own` (the tabs' names
    /// and the opacities); what they ask of the server is put in `acts`. Whether `own` was
    /// changed.
    #[allow(clippy::too_many_lines)]
    pub fn chat(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        area: Rect,
        chat: Option<&ChatOptions>,
        own: &mut crate::own::Own,
        acts: &mut Vec<Action>,
    ) -> bool {
        let k = p.scale;
        self.drawn.clear();
        self.bar = None;
        let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let Some(chat) = chat else {
            p.text(&style, area.x, area.y, "Log in to change these.");
            self.typing = 0;
            return false;
        };
        let heading =
            TextStyle::new(Family::Heading, 16.0, ctx.colours.heading()).edge(ctx.colours.edge());
        let rows = chat_rows();
        #[allow(clippy::cast_precision_loss)]
        let content = ROW * k * rows.len() as f32;
        let window = |w: usize| u32::try_from(w + 1).unwrap_or(1);
        let offset = kit::scroll(p, ctx, area, content, &mut self.scroll);
        self.note_bar(p, area, content);
        let gutter = gutter(p);
        p.list.push_clip(area);
        let mut keep = false;
        let mut changed = false;
        // A name box scrolled out of sight lets the keys go.
        let mut typed_in = false;
        for (i, line) in rows.iter().enumerate() {
            let r = page_row(area, offset, i, k, gutter);
            if r.bottom() < area.y || r.y > area.bottom() {
                continue;
            }
            match *line {
                ChatRow::LogWindow => {
                    p.text_in(&heading, r, Align::Left, LOG_WINDOW);
                }
                ChatRow::Heading(w) => {
                    p.text_in(&heading, r, Align::Left, CHAT_HEADINGS[w]);
                }
                ChatRow::Opacity(tab, active) => {
                    let label = Rect::new(r.x + 16.0 * k, r.y, r.w - 16.0 * k, r.h);
                    p.text_in(&style, label, Align::Left, opacity_caption(tab, active));
                    let field = control(r);
                    self.drawn.push(field);
                    let kept = &mut own.chat_opacity[tab.unwrap_or(0)];
                    let now = if active { kept.0 } else { kept.1 };
                    // Shown in percent.
                    if let Some(v) = kit::slider(p, ctx, field, now * 100.0, 0.0, 100.0) {
                        let v = (v / 100.0).clamp(0.0, 1.0);
                        if active {
                            kept.0 = v;
                        } else {
                            kept.1 = v;
                        }
                        changed = true;
                    }
                }
                ChatRow::Name(w) => {
                    let label = Rect::new(r.x + 16.0 * k, r.y, r.w - 16.0 * k, r.h);
                    p.text_in(&style, label, Align::Left, "Name");
                    let field = control(r);
                    self.drawn.push(field);
                    let id = NAME_FIELD + window(w);
                    // The box shows the tab's name; while it has the keys, what is typed, which
                    // names the tab as it is typed (blank, its own name again).
                    let shown = crate::ui::chat::tab_label(&own.tab_names[w], w);
                    let mut text = if self.typing == id {
                        self.draft.clone()
                    } else {
                        shown.to_owned()
                    };
                    let before = text.clone();
                    let had = self.typing == id;
                    // Enter lets the box go, as a press elsewhere does.
                    if kit::text_box_holding(
                        p,
                        ctx,
                        field,
                        &mut text,
                        &mut self.typing,
                        id,
                        Align::Left,
                        crate::own::TAB_NAME_MAX,
                    ) {
                        self.typing = 0;
                    }
                    typed_in |= self.typing == id;
                    if (had || self.typing == id) && text != before {
                        let typed = text.trim();
                        let name = if typed == crate::ui::chat::tab_label("", w) {
                            ""
                        } else {
                            typed
                        };
                        if own.tab_names[w] != name {
                            own.set_tab_name(w, name);
                            changed = true;
                        }
                    }
                    if self.typing == id {
                        self.draft = text;
                    }
                }
                ChatRow::Kind(w, kind) => {
                    let name = CHAT_KINDS
                        .iter()
                        .find(|(b, _)| *b == kind)
                        .map_or("", |(_, n)| *n);
                    let filter = chat.filters[w];
                    let line = Rect::new(r.x + 16.0 * k, r.y, r.w - 16.0 * k, r.h);
                    self.drawn.push(line);
                    if let Some(on) = kit::check_row(p, ctx, line, filter & kind != 0, &style, name)
                    {
                        let filter = if on { filter | kind } else { filter & !kind };
                        acts.push(Action::ChatWindows(ChatWindowAction::Filter {
                            window: window(w),
                            filter,
                        }));
                        keep = true;
                    }
                }
            }
        }
        p.list.pop_clip();
        if !typed_in {
            self.typing = 0;
        }
        if keep {
            acts.push(Action::ChatWindows(ChatWindowAction::Keep));
        }
        changed
    }
}
