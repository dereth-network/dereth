//! The in-game HUD, laid out at 1080 lines and scaled to the screen:
//!
//! * the parameter bar (health, mana and stamina) above the hotbars, with the stance gauge beside it;
//! * two hotbars of twelve: the shortcut bar, then the first spell tab;
//! * the EXP bar along the foot, with unassigned experience as the rested segment;
//! * the target bar at the top centre;
//! * the status row (enchantments with their time left) at the top right;
//! * the minimap (the radar) in the top-right corner, with the server-info line above it;
//! * the party list (the player and the fellowship) at the left;
//! * the log window (chat, with tabs and an input line) at the bottom left;
//! * the main menu at the bottom right.

use dereth_client_contract::UiRequest;

use crate::art::{Family, Sprite};
use crate::draw::{with_alpha, Argb, Rect, WHITE};
use crate::ui::game::{BlipKind, ChatLine, GameState, Relation};
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::panels::{WindowId, Windows};
use crate::ui::Outcome;

mod lamps;

/// The chat type of the game's own feedback to the player: refusals and warnings.
const FEEDBACK_CHAT_TYPE: u8 = 0x1A;

/// How long a feedback line floats at the top of the screen.
const NOTICE_SECONDS: f64 = 4.0;

/// A pending tooltip, drawn last.
#[derive(Debug, Clone)]
struct Tip {
    title: String,
    lines: Vec<String>,
}

/// The HUD's state across frames.
#[derive(Debug, Default)]
pub struct Hud {
    /// The log window.
    pub log: crate::ui::chat::ChatLog,
    /// The main menu entry whose sub-menu is open.
    menu_open: Option<usize>,
    tip: Option<Tip>,
    /// The selection the last health query was sent for.
    queried: Option<dereth_primitives::ObjectId>,
    /// Smoothed vitals, for the delayed "damage chunk".
    shown: [f32; 3],
    /// Each shortcut slot's cooldown as last told: its seconds left and when it was told, so
    /// the sweep runs down every frame between the game's updates.
    cooldowns: std::collections::HashMap<usize, (f64, f64)>,
    /// The game's feedback lines floating at the top of the screen, with when each arrived.
    notices: Vec<(String, f64)>,
    /// When the player's level last changed, for the banner.
    level_seen: u32,
    level_banner_at: Option<f64>,
    /// Where the player has put each element.
    pub layout: crate::ui::layout::HudLayout,
    /// The shortcut slots a drag can land on, on screen, with their slot numbers.
    pub drop_slots: Vec<(Rect, u32)>,
    /// The character the layout was loaded for.
    layout_for: Option<String>,
    /// The HUD Layout window was open last frame.
    layout_open: bool,
    /// The folder the layout files live in.
    pub settings_dir: Option<std::path::PathBuf>,
    /// The spell tab the second bar shows, and the spell the cast key casts.
    spell_tab: usize,
    spell_index: usize,
    /// The log-out question is up.
    log_out_asked: bool,
    /// Its words, from the game's string table.
    pub log_out_text: String,
    /// A window's text box has the keyboard.
    pub other_text_focus: bool,
    /// The attack power notch is being dragged.
    power_drag: bool,
    /// The effects list open under its icon: `Some(true)` the harmful ones, `Some(false)` the
    /// beneficial ones; and how far it is scrolled.
    effects_open: Option<bool>,
    /// When the effects list was opened, and whether a window has opened since (which then
    /// takes Escape before the list does).
    effects_opened_at: f64,
    window_over_effects: bool,
    effects_scroll: f32,
    /// How far the radar is zoomed in: 1 its full reach, down to a quarter; 0 is unset (full).
    radar_zoom: f32,
    /// The heading the radar shows, easing after the character's, and when it was last moved.
    radar_heading: Option<(f32, f64)>,
    /// The minimap turns with the character; otherwise it stands north up.
    pub minimap_rotates: bool,
    /// What the HUD's clicks asked the information windows to show this frame.
    asks: Vec<crate::ui::panels::info::Ask>,
}

/// The action that logs the character out (Shift+Escape), and the one that also quits.
pub const ACTION_LOG_OUT: u32 = 0x1000_0026;
pub const ACTION_QUIT: u32 = 0x1000_0027;
/// The game's use-the-selection action, which the interface answers itself: pick up, use, or
/// find the nearest thing.
pub const ACTION_USE: u32 = 0x1000_0025;
/// The game's own actions the use key hands on: pick up the selection, and select the nearest
/// thing on the compass.
pub const ACTION_PICK_UP: u32 = 0x1000_002C;
pub const ACTION_CLOSEST_THING: u32 = 0x1000_002F;
/// The game's next- and previous-monster actions, which the interface answers itself by
/// distance ([`tab_target`]).
pub const ACTION_PREVIOUS_MONSTER: u32 = 0x1000_0036;
pub const ACTION_NEXT_MONSTER: u32 = 0x1000_0037;

/// The monster to select for Tab: the nearest first, then each further one in turn (`nearer_first`),
/// or the furthest first, then each nearer one (Shift+Tab), round again at the end. Monsters only:
/// the creatures the radar shows to fight.
#[must_use]
pub fn tab_target(state: &GameState, nearer_first: bool) -> Option<dereth_primitives::ObjectId> {
    let selected = state.target.as_ref();
    let mut monsters: Vec<(f32, dereth_primitives::ObjectId)> = state
        .blips
        .iter()
        .filter(|b| match b.kind {
            BlipKind::Creature => true,
            BlipKind::Selected => selected.is_some_and(|t| {
                t.id == b.id && matches!(t.relation, Relation::Hostile | Relation::Engaged)
            }),
            _ => false,
        })
        .map(|b| (b.dx * b.dx + b.dy * b.dy, b.id))
        // No further than the radar reaches.
        .filter(|(d2, _)| *d2 <= state.radar_range * state.radar_range)
        .collect();
    monsters.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1 .0.cmp(&b.1 .0)));
    let n = monsters.len();
    if n == 0 {
        return None;
    }
    let at = selected.and_then(|t| monsters.iter().position(|(_, id)| *id == t.id));
    let i = match (at, nearer_first) {
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
        (None, true) => 0,
        (None, false) => n - 1,
    };
    Some(monsters[i].1)
}

/// What the game says when a log-off is refused in the air.
const AIRBORNE_REFUSAL: &str = "Cannot log off while in mid-air.";

/// The window a window key opens and closes: the game's panel keys, on the windows that carry
/// those panels here.
#[must_use]
pub fn window_for_action(action: u32) -> Option<WindowId> {
    Some(match action {
        // Allegiance, fellowship.
        0x1000_000E => WindowId::Allegiance,
        0x1000_000F => WindowId::Social,
        // Spellbook, spell components.
        0x1000_0011 | 0x1000_0012 => WindowId::Actions,
        // Attributes, skills.
        0x1000_0014 | 0x1000_0015 => WindowId::Character,
        // World and map.
        0x1000_0016 | 0x1000_0017 => WindowId::Map,
        // Inventory.
        0x1000_0019 => WindowId::Inventory,
        // Options, and the gameplay options Escape opens with nothing selected.
        0x1000_001A | 0x1000_001B => WindowId::Options,
        // The social panel; spell management; skill management; titles.
        0x1000_000D => WindowId::Social,
        0x1000_0010 => WindowId::Actions,
        0x1000_0013 | 0x1000_011A => WindowId::Character,
        // Vitae; the house; the option and key pages; contracts.
        0x1000_000C => WindowId::Vitae,
        0x1000_0018 => WindowId::House,
        0x1000_001C | 0x1000_001D | 0x1000_001F => WindowId::Options,
        0x1000_012E => WindowId::Journal,
        // Character information; friends; the quest journal.
        0x1000_0005 => WindowId::Character,
        0x1000_0118 => WindowId::Social,
        0x1000_0128 => WindowId::Journal,
        _ => return None,
    })
}

/// The HUD Layout window's size, in layout units.
const LAYOUT_SIZE: (f32, f32) = (380.0, 200.0);

impl Hud {
    /// The game's actions that are the interface's: the shortcut keys, the window keys and
    /// logging out; and the spell keys, on the second bar.
    pub fn game_keys(
        &mut self,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        windows: &mut Windows,
        out: &mut Outcome,
    ) {
        use dereth_ui_screens::toolbar::shortcuts::{shortcut_action, ShortcutAction};
        for action in std::mem::take(&mut ctx.input.actions) {
            if let Some(a) = shortcut_action(action) {
                match a {
                    ShortcutAction::Use { slot, primary } => {
                        let Some(s) = state.shortcuts.iter().find(|s| s.index == slot) else {
                            continue;
                        };
                        let request = match (s.spell, s.object) {
                            (_, Some(item)) if state.targeting => {
                                Some(UiRequest::ExecuteTargetItem(item))
                            }
                            (Some(spell_id), _) if primary => {
                                Some(UiRequest::CastSpell { spell_id })
                            }
                            (_, Some(item)) if primary => Some(UiRequest::Use(item)),
                            (_, Some(item)) => Some(UiRequest::Select(item)),
                            _ => None,
                        };
                        out.requests.extend(request);
                    }
                    ShortcutAction::CreateToSelected => {
                        if let Some(t) = &state.target {
                            out.requests.push(UiRequest::CreateShortcut(t.id));
                        }
                    }
                }
            } else if let Some(id) = window_for_action(action) {
                windows.toggle(id, ctx.time);
            } else if crate::ui::chat::is_chat_action(action) {
                // A window's text box with the keyboard keeps the chat keys for itself.
                if !self.other_text_focus {
                    let was_typing = self.log.typing();
                    self.log.action(action, state, out);
                    // The Enter that opened the line does not also send it.
                    if !was_typing && self.log.typing() {
                        ctx.input.take_key(crate::ui::input::vk::ENTER);
                    }
                }
            } else if action == ACTION_NEXT_MONSTER || action == ACTION_PREVIOUS_MONSTER {
                if let Some(id) = tab_target(state, action == ACTION_NEXT_MONSTER) {
                    out.requests.push(UiRequest::Select(id));
                }
            } else if action == ACTION_USE {
                Self::use_key(state, out);
            } else if action == ACTION_LOG_OUT {
                self.ask_log_out(ctx);
            } else if action == ACTION_QUIT {
                out.quit = true;
            }
        }
        // The keys the interface keeps for itself, while nothing is being typed: F1 to F9 select
        // the fellowship's members in turn, T puts the keyboard in the stack splitter's number,
        // and Escape with nothing open lets go of the selection, and with nothing selected either
        // asks to log out. None of them while the HUD is being laid out.
        let typing = self.log.typing() || self.other_text_focus;
        if !typing && !windows.is_open(WindowId::Layout) {
            for n in 0..9u8 {
                if ctx.input.take_key(0x70 + usize::from(n)) {
                    if let Some(id) = fellow_in_reach(state, usize::from(n)) {
                        out.requests.push(UiRequest::Select(id));
                    }
                }
            }
            if ctx.input.take_key(0x54) {
                windows.focus_splitter();
            }
            // Escape closes what is open first: a window, or the list of effects.
            let open = windows.any_open() || self.effects_open.is_some();
            if escape_deselects(state, open) && ctx.input.take_key(crate::ui::input::vk::ESCAPE) {
                out.requests
                    .push(UiRequest::Select(dereth_primitives::ObjectId(0)));
            } else if escape_logs_out(state, open)
                && ctx.input.take_key(crate::ui::input::vk::ESCAPE)
            {
                self.ask_log_out(ctx);
            }
        }
        let tabs = state.spell_tabs.len().max(1);
        for notice in std::mem::take(&mut ctx.input.magic) {
            use dereth_client_contract::view::MagicNotice as M;
            let tab_len = state.spell_tabs.get(self.spell_tab).map_or(0, Vec::len);
            match notice {
                M::CastCurrentSpell => {
                    if let Some(id) = state
                        .spell_tabs
                        .get(self.spell_tab)
                        .and_then(|t| t.get(self.spell_index))
                    {
                        out.requests.push(UiRequest::CastSpell { spell_id: *id });
                    }
                }
                M::CastQuickslotSpell { slot } => {
                    // The slot on the page the bar shows.
                    let slot = (self.spell_index / SPELL_PAGE) * SPELL_PAGE + slot;
                    if let Some(id) = state
                        .spell_tabs
                        .get(self.spell_tab)
                        .and_then(|t| t.get(slot))
                    {
                        self.spell_index = slot;
                        out.requests.push(UiRequest::CastSpell { spell_id: *id });
                    }
                }
                M::NextSpellSelection if tab_len > 0 => {
                    self.spell_index = (self.spell_index + 1) % tab_len;
                }
                M::PrevSpellSelection if tab_len > 0 => {
                    self.spell_index = (self.spell_index + tab_len - 1) % tab_len;
                }
                M::FirstSpellSelection => self.spell_index = 0,
                M::LastSpellSelection => self.spell_index = tab_len.saturating_sub(1),
                M::NextSpellTab => {
                    self.spell_tab = (self.spell_tab + 1) % tabs;
                    self.spell_index = 0;
                }
                M::PrevSpellTab => {
                    self.spell_tab = (self.spell_tab + tabs - 1) % tabs;
                    self.spell_index = 0;
                }
                M::FirstSpellTab => {
                    self.spell_tab = 0;
                    self.spell_index = 0;
                }
                M::LastSpellTab => {
                    self.spell_tab = tabs - 1;
                    self.spell_index = 0;
                }
                M::NextSpellSelection | M::PrevSpellSelection => {}
            }
        }
    }

    /// The use key: an item on the ground or in the chest or corpse open is picked up; anything
    /// else selected but a creature or a player is used; with nothing (or a creature or a player)
    /// selected, the nearest thing is selected instead.
    fn use_key(state: &GameState, out: &mut Outcome) {
        let in_loot = |id| {
            state
                .loot
                .as_ref()
                .is_some_and(|(_, _, items)| items.iter().any(|i| i.id == id))
        };
        match &state.target {
            // Use first: a thing used where it stands, a person; then a thing to pick up.
            Some(t) if state.target_usable_here || t.relation == Relation::Npc => {
                out.requests.push(UiRequest::Use(t.id));
            }
            Some(t) if state.target_pickable || in_loot(t.id) => {
                out.actions.push(ACTION_PICK_UP);
            }
            Some(t) if t.relation == Relation::Object => {
                out.requests.push(UiRequest::Use(t.id));
            }
            // A creature or a player selected stays selected: there is nothing to use.
            Some(_) => {}
            None => out.actions.push(ACTION_CLOSEST_THING),
        }
    }

    /// Whether the log-out question is up.
    #[must_use]
    pub fn log_out_asked(&self) -> bool {
        self.log_out_asked
    }

    /// Log out, as the game asks it: refused in the air, and otherwise a question first.
    fn ask_log_out(&mut self, ctx: &Ctx<'_>) {
        if ctx.input.airborne {
            push_notice(&mut self.notices, AIRBORNE_REFUSAL, ctx.time);
        } else {
            self.log_out_asked = true;
        }
    }

    /// The log-out question, while it is up.
    pub fn log_out_question(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, out: &mut Outcome) {
        if !self.log_out_asked {
            return;
        }
        let text = if self.log_out_text.is_empty() {
            "This will exit your character from the game world. Are you sure?"
        } else {
            self.log_out_text.as_str()
        };
        if let Some(b) =
            crate::ui::pregame::dialog_box(p, ctx, "Log Out", text, &["Yes", "No"], true, 0)
        {
            self.log_out_asked = false;
            if b == 0 {
                out.log_off = true;
            }
        }
    }

    /// Open the chat line with `text` already typed.
    pub fn open_chat(&mut self, text: String) {
        self.log.open(text);
    }

    /// The radar's zoom: its full reach until the player zooms in.
    fn zoom(&self) -> f32 {
        if self.radar_zoom > 0.0 {
            self.radar_zoom.clamp(0.25, 1.0)
        } else {
            1.0
        }
    }

    /// The spell tab the second bar shows.
    #[must_use]
    pub const fn spell_tab(&self) -> usize {
        self.spell_tab
    }

    /// The log window holds `lines` and nothing else: the history another interface hands over.
    /// The game's floating feedback is left out, as it never reaches a chat window.
    pub fn replace_log(&mut self, lines: Vec<ChatLine>) {
        self.log.replace(
            lines
                .into_iter()
                .filter(|l| l.chat_type != FEEDBACK_CHAT_TYPE)
                .collect(),
        );
    }

    /// Whether the log-out question is up.
    #[must_use]
    pub const fn log_out_open(&self) -> bool {
        self.log_out_asked
    }

    /// Whether the HUD has something open that Escape closes.
    #[must_use]
    pub fn wants_escape(&self) -> bool {
        self.log.wants_escape()
            || self.menu_open.is_some()
            || self.log_out_asked
            || self.effects_open.is_some()
    }
}

/// Thousands separators.
#[must_use]
pub fn grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A time left in the status row's style: seconds, then minutes, then hours.
#[must_use]
pub fn time_left(seconds: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let s = seconds.max(0.0).ceil() as u64;
    if s < 60 {
        format!("{s}")
    } else if s <= 3600 {
        format!("{}m", s.div_ceil(60))
    } else {
        format!("{}h", s.div_ceil(3600))
    }
}

impl Hud {
    #[allow(clippy::too_many_lines)]
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        windows: &mut Windows,
        out: &mut Outcome,
    ) {
        self.tip = None;
        // The game's own feedback ("you cannot do that", the portal-space line) floats at the
        // top of the screen, where error messages go. The game shows that type in its
        // floating line only, never in a chat window.
        self.notices
            .retain(|(_, at)| ctx.time - at < NOTICE_SECONDS);
        for line in &state.new_chat {
            if line.chat_type == FEEDBACK_CHAT_TYPE {
                push_notice(&mut self.notices, &line.text, ctx.time);
            }
        }
        self.log
            .take(state, |line| line.chat_type != FEEDBACK_CHAT_TYPE);
        if self.notices.len() > 3 {
            let extra = self.notices.len() - 3;
            self.notices.drain(..extra);
        }
        // Ask the server for the selection's health once per new selection, as the game's own
        // target panel does.
        let target_id = state.target.as_ref().map(|t| t.id);
        if target_id != self.queried {
            if let Some(id) = target_id {
                out.requests.push(UiRequest::QueryHealth(id));
            }
            self.queried = target_id;
        }
        if state.level != self.level_seen {
            if self.level_seen != 0 && state.level > self.level_seen {
                self.level_banner_at = Some(ctx.time);
            }
            self.level_seen = state.level;
        }
        // The layout of the character in the world, loaded once per character.
        if self.layout_for.as_deref() != Some(state.name.as_str()) && !state.name.is_empty() {
            if let Some(dir) = &self.settings_dir {
                let safe: String = state
                    .name
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect();
                self.layout =
                    crate::ui::layout::HudLayout::load(dir.join(format!("horizon-hud-{safe}.txt")));
            }
            self.layout_for = Some(state.name.clone());
        }
        let editing = windows.is_open(WindowId::Layout);
        self.layout.outlines.clear();
        // The names in the world first, under everything of the HUD.
        if !state.teleporting {
            self.nameplates(p, ctx, state);
        }
        macro_rules! element {
            ($name:literal, $call:expr) => {{
                let open = self.layout.begin($name, p, ctx, editing);
                $call;
                self.layout.end(open, p, ctx);
            }};
        }
        element!("server", self.server_info(p, ctx, state));
        element!("minimap", self.minimap(p, ctx, state, out));
        self.window_over_effects = windows
            .latest_opened_at()
            .is_some_and(|t| t > self.effects_opened_at);
        element!("status", self.status_row(p, ctx, state));
        element!("target", self.target_bar(p, ctx, state));
        element!("party", self.party_list(p, ctx, state, out));
        element!("chat", self.log.frame(p, ctx, state, out));
        self.log.dock = self.layout.map_rect("chat", self.log.dock, p.scale);
        element!("parameter", self.parameter_bar(p, ctx, state));
        element!("stance", self.stance_gauge(p, ctx, state, out));
        self.drop_slots.clear();
        element!("hotbars", self.hotbars(p, ctx, state, out));
        element!("jump", self.jump_bar(p, state));
        let placed: Vec<(Rect, u32)> = self
            .drop_slots
            .iter()
            .map(|(r, s)| (self.layout.map_rect("hotbars", *r, p.scale), *s))
            .collect();
        self.drop_slots = placed;
        element!("exp", self.exp_bar(p, ctx, state));
        element!("split", windows.ground_split(p, ctx, state, out));
        element!("menu", self.main_menu(p, ctx, windows, out));
        for ask in self.asks.drain(..) {
            windows.ask(ask, ctx.time);
        }
        self.level_banner(p, ctx, state);
        self.feedback_notices(p, ctx);
    }

    /// While the HUD Layout window is open, the elements' outlines to move them by and the
    /// window itself, drawn over the log window's popped-out tabs and under the other windows.
    pub fn layout_overlay(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        windows: &mut Windows,
    ) {
        let open = windows.is_open(WindowId::Layout);
        if open && !self.layout_open {
            self.log.begin_layout();
        }
        // Closed with changes unsaved: the layout last kept comes back.
        if !open && self.layout_open {
            self.layout.revert();
            self.log.revert_places();
        }
        self.layout_open = open;
        if open {
            self.tip = None;
            self.layout.edit(p, ctx);
            self.layout_panel(p, ctx, windows);
        }
    }

    /// Whether the HUD Layout window has changes not yet saved.
    #[must_use]
    pub fn layout_unsaved(&self) -> bool {
        self.layout.unsaved() || self.log.places_unsaved()
    }

    /// What [`Self::layout_overlay`] takes the pointer over: the elements' outlines and the HUD
    /// Layout window, while it is open.
    #[must_use]
    pub fn layout_rects(&self, windows: &Windows, scale: f32, screen: (f32, f32)) -> Vec<Rect> {
        if !windows.is_open(WindowId::Layout) {
            return Vec::new();
        }
        let (w, h) = (LAYOUT_SIZE.0 * scale, LAYOUT_SIZE.1 * scale);
        let (x, y) = windows
            .state(WindowId::Layout)
            .pos
            .unwrap_or((screen.0 / 2.0 - w / 2.0, 260.0 * scale));
        let mut out: Vec<Rect> = self.layout.outlines.iter().map(|(_, r)| *r).collect();
        out.push(Rect::new(x, y, w, h));
        out
    }

    /// The HUD Layout window: the element in hand, its size, and Save, Reset and Close. What it
    /// changes shows at once and is kept by Save; Close, with changes unsaved, puts back the layout
    /// last kept.
    fn layout_panel(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, windows: &mut Windows) {
        let k = p.scale;
        let (sw, _) = p.screen;
        let mut state = windows.state(WindowId::Layout);
        let win = kit::window(
            p,
            ctx,
            &mut state,
            "HUD Layout",
            LAYOUT_SIZE,
            (sw / 2.0 - LAYOUT_SIZE.0 / 2.0 * k, 260.0 * k),
        );
        let body = win.body;
        let text = TextStyle::new(Family::Body, 14.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let dim = TextStyle::new(Family::Body, 12.0, 0xFFB0_B0B0).edge(0xFF00_0000);
        match self.layout.selected_placement() {
            Some((name, pl)) => {
                let title = crate::ui::layout::element_title(name);
                p.text(&text, body.x + 8.0 * k, body.y + 4.0 * k, &title);
                let pct = dereth_primitives::num::to_i32((pl.scale * 100.0).round());
                let line = format!("Size {pct}%   X {:+.0}  Y {:+.0}", pl.dx, pl.dy);
                p.text(&dim, body.x + 8.0 * k, body.y + 26.0 * k, &line);
            }
            None => {
                p.text(
                    &dim,
                    body.x + 8.0 * k,
                    body.y + 4.0 * k,
                    "Click an element to select it.",
                );
            }
        }
        let hint = "Drag to move; the wheel changes the size.";
        p.text(&dim, body.x + 8.0 * k, body.y + 50.0 * k, hint);
        // Three buttons sharing the body's width.
        let bw = (body.w - 16.0 * k - 2.0 * 8.0 * k) / 3.0;
        let b = Rect::new(body.x + 8.0 * k, body.bottom() - 34.0 * k, bw, 30.0 * k);
        // Save is lit while there is something to save.
        let unsaved = self.layout_unsaved();
        if kit::button_lit(p, ctx, b, "Save", unsaved, unsaved) {
            if let Err(e) = self.layout.save() {
                tracing::warn!("the HUD layout was not saved: {e}");
            }
            self.log.save_places();
        }
        if kit::button(p, ctx, b.offset(bw + 8.0 * k, 0.0), "Reset", true) {
            self.layout.reset();
            self.log.reset_places();
        }
        if kit::button(p, ctx, b.offset(2.0 * (bw + 8.0 * k), 0.0), "Close", true) || win.closed {
            state.open = false;
        }
        windows.set_state(WindowId::Layout, state);
    }

    /// What draws over the windows: the tooltip.
    pub fn frame_overlays(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        _state: &GameState,
        _out: &mut Outcome,
    ) {
        if let Some(tip) = self.tip.take() {
            kit::tooltip(p, ctx, &tip.title, &tip.lines);
        }
    }

    fn tip(&mut self, title: impl Into<String>, lines: Vec<String>) {
        self.tip = Some(Tip {
            title: title.into(),
            lines,
        });
    }

    // -----------------------------------------------------------------------------------------
    // The parameter bar
    // -----------------------------------------------------------------------------------------

    /// One gauge of the parameter bar: its label, its number in the bar-number font, and the bar.
    #[allow(clippy::too_many_arguments)]
    fn gauge(
        &self,
        p: &mut Painter<'_>,
        _ctx: &Ctx<'_>,
        x: f32,
        y: f32,
        label: &str,
        value: u32,
        fill: f32,
        own: (f32, Argb),
    ) {
        let k = p.scale;
        let bar = Rect::new(x, y + 18.0 * k, 160.0 * k, 20.0 * k);
        // The own gauge: the drained chunk, pale, behind the value in the gauge's colour.
        kit::own_gauge_layers(p, bar, &[(own.0, 0xB0E8_DCC8), (fill, own.1)]);
        let label_style = TextStyle::new(Family::Numerals, 12.0, 0xFFFF_FFFF).edge(0xFF33_2211);
        p.text(&label_style, x + 6.0 * k, y + 8.0 * k, label);
        let number = TextStyle::new(Family::BarNumbers, 20.0, 0xFFFF_FFFF).edge(0xFF33_2211);
        let n = value.to_string();
        let w = p.measure(&number, &n);
        p.text(&number, x + 152.0 * k - w, y - 2.0 * k, &n);
    }

    fn parameter_bar(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, state: &GameState) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        let targets = [
            state.health.ratio(),
            state.mana.ratio(),
            state.stamina.ratio(),
        ];
        #[allow(clippy::cast_possible_truncation)]
        let step = (ctx.dt as f32 * 1.5).min(1.0);
        for (shown, target) in self.shown.iter_mut().zip(targets) {
            // A drop shows at once; the lighter chunk behind it catches up.
            *shown = if target < *shown {
                (*shown - step * 0.6).max(target)
            } else {
                target
            };
        }
        let total = 3.0 * 164.0 * k;
        let x0 = sw / 2.0 - total / 2.0;
        // Centred along the foot of the screen, under the hotbars.
        let y = sh - 30.0 * k;
        // Health green, mana pink, stamina gold: the own gauge's neutral fill in each colour.
        let vitals = [
            ("Health", state.health, 0xFF78_D85A),
            ("Mana", state.mana, 0xFFF0_80D8),
            ("Stamina", state.stamina, 0xFFF0_C050),
        ];
        for (i, (label, v, colour)) in vitals.into_iter().enumerate() {
            let x = x0 + i as f32 * 164.0 * k;
            self.gauge(
                p,
                ctx,
                x,
                y,
                label,
                v.current,
                v.ratio(),
                (self.shown[i], colour),
            );
            let hit = Rect::new(x, y, 160.0 * k, 40.0 * k);
            if ctx.over(&hit) {
                let name = ["Health", "Mana", "Stamina"][i];
                self.tip(name, vec![format!("{} / {}", v.current, v.max)]);
            }
        }
    }

    /// The stance gauge: the combat mode as four crests, lit for the one in force, and the power
    /// bar's charge as a row of gold pips.
    fn stance_gauge(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        let plain = p.art.has_piece("window.tl");
        if plain {
            self.stance_bar(p, ctx, state, out);
            return;
        }
        // The small window beside the parameter bar, headed STANCE.
        let (x, y) = if plain {
            (sw / 2.0 + 200.0 * k, sh - 178.0 * k)
        } else {
            (sw / 2.0 + 260.0 * k, sh - 158.0 * k)
        };
        let fighting = state.combat_mode == 2 || state.combat_mode == 4;
        let r = Rect::new(
            x,
            y - if fighting { 34.0 * k } else { 0.0 },
            200.0 * k,
            if fighting { 86.0 } else { 52.0 } * k,
        );
        let y = r.y;
        // With the own art the stance is a plain button floating on its own, the power bar
        // under it when there is one; otherwise a small window headed STANCE.
        if !plain {
            kit::panel(p, r, 0.75);
            let heading =
                TextStyle::new(Family::Heading, 18.4, ctx.colours.heading()).edge(0xFF00_0000);
            p.text(&heading, x + 10.0 * k, y + 4.0 * k, "STANCE");
        }
        // One toggle between peace and the combat mode the wielded weapon calls for: the game
        // chooses which combat mode, from what is in hand.
        let peace = state.combat_mode == 1;
        let combat = match state.combat_mode {
            2 => "Melee",
            4 => "Missile",
            8 => "Magic",
            _ => "Combat",
        };
        let toggle = if plain {
            Rect::new(x + 30.0 * k, y + 6.0 * k, 140.0 * k, 30.0 * k)
        } else {
            Rect::new(x + 70.0 * k, y + 6.0 * k, 120.0 * k, 28.0 * k)
        };
        let label = if peace { "Peace" } else { combat };
        if kit::button(p, ctx, toggle, label, true) {
            out.actions.push(crate::ui::ACTION_TOGGLE_COMBAT);
        }
        if ctx.over(&toggle) {
            let line = if peace {
                "Click to fight with what is in hand."
            } else {
                "Click to return to peace."
            };
            self.tip("Stance", vec![line.into()]);
        }
        // The power charge, filling while an attack or a jump builds; in a fighting stance,
        // the power the attack aims at is a notch on it, set by a click or a drag.
        // As the experience bar is drawn, charging red; a jump's charge has a bar of its own.
        let bar = Rect::new(x + 10.0 * k, y + 40.0 * k, 180.0 * k, 8.0 * k);
        let attack_power = state.power.filter(|_| !state.power_jump);
        if fighting || attack_power.is_some() {
            let charge = attack_power.unwrap_or(0.0).clamp(0.0, 1.0);
            if !kit::own_gauge(p, bar, charge, 0xFFE0_4030) {
                p.fill(bar, 0x60FF_FFFF);
                p.fill(Rect::new(bar.x, bar.y, bar.w * charge, bar.h), 0xFFE0_4030);
            }
        }
        if fighting {
            let desired = state.combat_bar.desired_power.clamp(0.0, 1.0);
            let nx = bar.x + bar.w * desired;
            // The aim: the slider's round knob on the bar, else a white notch.
            if let Some(knob) = p.piece("control.knob") {
                let d = 14.0 * k;
                p.sprite(
                    &knob,
                    Rect::new(nx - d / 2.0, bar.y + bar.h / 2.0 - d / 2.0, d, d),
                    WHITE,
                );
            } else {
                p.fill(
                    Rect::new(nx - 1.5 * k, bar.y - 4.0 * k, 3.0 * k, bar.h + 8.0 * k),
                    0xFFFF_FFFF,
                );
            }
            let hit = Rect::new(bar.x, bar.y - 6.0 * k, bar.w, bar.h + 12.0 * k);
            if ctx.over(&hit) {
                self.tip(
                    "Attack power",
                    vec![format!(
                        "Aim at {}% power. Click or drag to set.",
                        (desired * 100.0).round()
                    )],
                );
                if ctx.input.down[0] && (ctx.input.pressed[0] || self.power_drag) {
                    self.power_drag = true;
                    ctx.input.pressed[0] = false;
                    ctx.input.captured = true;
                    let at = ((ctx.input.mouse.0 - bar.x) / bar.w).clamp(0.0, 1.0);
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let position = (at * 1000.0).round() as u32;
                    out.requests
                        .push(UiRequest::CombatSetDesiredPower { position });
                }
            }
            if !ctx.input.down[0] {
                self.power_drag = false;
            }
            // The attack height: high, medium, low.
            let heights = [(3, "Low"), (2, "Medium"), (1, "High")];
            for (i, (h, name)) in heights.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let b = Rect::new(
                    x + 10.0 * k + i as f32 * 62.0 * k,
                    y + 54.0 * k,
                    58.0 * k,
                    24.0 * k,
                );
                // The chosen height lit, as a button held down shows.
                let on = state.combat_bar.requested_attack_height == *h;
                if kit::button_lit(p, ctx, b, name, true, on) {
                    out.requests
                        .push(UiRequest::CombatSetAttackHeight { height: *h });
                }
            }
        }
    }

    /// The stance with the own art, over the hotbars along the spell bar's width: the button that
    /// toggles peace and combat centred above where the spell bar stands; in a melee or missile
    /// stance, the power bar along the whole width where the spell bar would be, the attack
    /// heights (low, medium, high) under its right end.
    fn stance_bar(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        // The hotbars' twelve slots, 46 units apart, centred 12 units right of the middle.
        let width = 12.0 * 46.0 * k - 2.0 * k;
        let left = sw / 2.0 + 12.0 * k - width / 2.0;
        let fighting = state.combat_mode == 2 || state.combat_mode == 4;
        let peace = state.combat_mode == 1;
        let combat = match state.combat_mode {
            2 => "Melee",
            4 => "Missile",
            8 => "Magic",
            _ => "Combat",
        };
        // Above the spell bar's place (and the jump bar over it), whatever stance.
        let toggle = Rect::new(
            sw / 2.0 + 12.0 * k - 75.0 * k,
            sh - 198.0 * k,
            150.0 * k,
            30.0 * k,
        );
        let label = if peace { "Peace" } else { combat };
        if kit::button(p, ctx, toggle, label, true) {
            out.actions.push(crate::ui::ACTION_TOGGLE_COMBAT);
        }
        if ctx.over(&toggle) {
            let line = if peace {
                "Click to fight with what is in hand."
            } else {
                "Click to return to peace."
            };
            self.tip("Stance", vec![line.into()]);
        }
        if !fighting {
            return;
        }
        // The power charge along the spell bar's width, drawn as the experience bar is and
        // charging red; the aimed power is the slider's knob on it, set by a click or a drag.
        // As thick as the jump bar.
        let bar = Rect::new(left, sh - 138.0 * k, width, 16.0 * k);
        let charge = state
            .power
            .filter(|_| !state.power_jump)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        if !kit::own_gauge(p, bar, charge, 0xFFE0_4030) {
            p.fill(bar, 0x60FF_FFFF);
            p.fill(Rect::new(bar.x, bar.y, bar.w * charge, bar.h), 0xFFE0_4030);
        }
        let desired = state.combat_bar.desired_power.clamp(0.0, 1.0);
        let nx = bar.x + bar.w * desired;
        if let Some(knob) = p.piece("control.knob") {
            let d = 22.0 * k;
            p.sprite(
                &knob,
                Rect::new(nx - d / 2.0, bar.y + bar.h / 2.0 - d / 2.0, d, d),
                WHITE,
            );
        }
        let hit = Rect::new(bar.x, bar.y - 6.0 * k, bar.w, bar.h + 12.0 * k);
        if ctx.over(&hit) {
            self.tip(
                "Attack power",
                vec![format!(
                    "Aim at {}% power. Click or drag to set.",
                    (desired * 100.0).round()
                )],
            );
            if ctx.input.down[0] && (ctx.input.pressed[0] || self.power_drag) {
                self.power_drag = true;
                ctx.input.pressed[0] = false;
                ctx.input.captured = true;
                let at = ((ctx.input.mouse.0 - bar.x) / bar.w).clamp(0.0, 1.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let position = (at * 1000.0).round() as u32;
                out.requests
                    .push(UiRequest::CombatSetDesiredPower { position });
            }
        }
        if !ctx.input.down[0] {
            self.power_drag = false;
        }
        // The attack heights, low to high, under the bar's right end.
        let (bw, bh, gap) = (78.0 * k, 26.0 * k, 6.0 * k);
        let heights = [(3, "Low"), (2, "Medium"), (1, "High")];
        for (i, (h, name)) in heights.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let b = Rect::new(
                bar.right() - (3.0 - i as f32) * (bw + gap) + gap,
                bar.bottom() + 6.0 * k,
                bw,
                bh,
            );
            let on = state.combat_bar.requested_attack_height == *h;
            if kit::button_lit(p, ctx, b, name, true, on) {
                out.requests
                    .push(UiRequest::CombatSetAttackHeight { height: *h });
            }
        }
    }

    /// The spell bar's page arrows at its right end, the page between them, once the tab fills a
    /// page: there is always one page more than the spells fill, to drop a new spell onto. Each
    /// arrow moves the selection, and so the bar, a page along.
    fn spell_pages(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        x: f32,
        y: f32,
        slot: f32,
    ) {
        let len = state.spell_tabs.get(self.spell_tab).map_or(0, Vec::len);
        if len < SPELL_PAGE {
            return;
        }
        let k = p.scale;
        // The pages the spells fill, and an empty one after them.
        let pages = len / SPELL_PAGE + 1;
        let page = (self.spell_index / SPELL_PAGE).min(pages - 1);
        let side = 14.0 * k;
        let up = Rect::new(x, y, side, side);
        let down = Rect::new(x, y + slot - side, side, side);
        if kit::step_button(p, ctx, up, true, "", page > 0) {
            self.spell_index = (page - 1) * SPELL_PAGE;
        }
        if kit::step_button(p, ctx, down, false, "", page + 1 < pages) {
            self.spell_index = (page + 1) * SPELL_PAGE;
        }
        let label = TextStyle::new(Family::Body, 10.0, 0xFFC8_C0B0).edge(0xFF00_0000);
        p.text_in(
            &label,
            Rect::new(
                x - 6.0 * k,
                up.bottom(),
                side + 12.0 * k,
                down.y - up.bottom(),
            ),
            Align::Centre,
            &format!("{}/{pages}", page + 1),
        );
    }

    /// A jump's charge: its own bar above the hotbars (above the spell bar while it is up), a
    /// little wider than they are, twice as tall as the experience bar and coloured as it is.
    fn jump_bar(&mut self, p: &mut Painter<'_>, state: &GameState) {
        let Some(power) = state.power.filter(|_| state.power_jump) else {
            return;
        };
        let k = p.scale;
        let (sw, sh) = p.screen;
        // Over the spell bar, or the power bar where it stands, or else the shortcut bar.
        let over_row = matches!(state.combat_mode, 2 | 4 | 8);
        let top = if over_row {
            sh - 134.0 * k
        } else {
            sh - 86.0 * k
        };
        // The hotbars' slots span twelve of 46 units, centred 12 units right of the middle.
        let w = 12.0 * 46.0 * k * 1.2;
        let bar = Rect::new(sw / 2.0 + 12.0 * k - w / 2.0, top - 26.0 * k, w, 16.0 * k);
        let fill = power.clamp(0.0, 1.0);
        if !kit::own_gauge(p, bar, fill, 0xFFE8_B84A) {
            p.fill(bar, 0x60FF_FFFF);
            p.fill(Rect::new(bar.x, bar.y, bar.w * fill, bar.h), 0xFFE8_B84A);
        }
    }

    // -----------------------------------------------------------------------------------------
    // The hotbars
    // -----------------------------------------------------------------------------------------

    #[allow(clippy::too_many_lines)]
    fn hotbars(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        let slot = 44.0 * k;
        let gap = 2.0 * k;
        let width = 12.0 * (slot + gap) + 24.0 * k;
        let x0 = sw / 2.0 - width / 2.0 + 24.0 * k;
        let keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "="];
        // The spell bar is up only while the character fights with magic, as the game's spell
        // bar is.
        let magic = state.combat_mode == dereth_client_contract::combat_mode::MAGIC;
        let bars: &[(f32, u32)] = if magic {
            &[(sh - 86.0 * k, 1), (sh - 134.0 * k, 2)]
        } else {
            &[(sh - 86.0 * k, 1)]
        };
        let small = TextStyle::new(Family::Numerals, 10.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let number = TextStyle::new(Family::Numerals, 14.0, 0xFFEE_E1C5).edge(0xFF00_0000);
        for &(y, bar) in bars {
            // The bar number, at the bar's leading edge; the spell bar shows its tab, with
            // arrows to change it.
            if bar == 2 {
                // The arrows that turn the bar to the previous and the next tab, each with the
                // key that does it beside it.
                let tabs = state.spell_tabs.len().max(1);
                let side = 14.0 * k;
                let up = Rect::new(x0 - 22.0 * k, y, side, side);
                let down = Rect::new(x0 - 22.0 * k, y + slot - side, side, side);
                if kit::step_button(p, ctx, up, true, "", true) {
                    self.spell_tab = (self.spell_tab + tabs - 1) % tabs;
                    self.spell_index = 0;
                }
                if kit::step_button(p, ctx, down, false, "", true) {
                    self.spell_tab = (self.spell_tab + 1) % tabs;
                    self.spell_index = 0;
                }
                let key = TextStyle::new(Family::Body, 10.0, 0xFFC8_C0B0).edge(0xFF00_0000);
                let (prev, next) = &state.spell_tab_keys;
                for (label, at) in [(prev, up), (next, down)] {
                    if !label.is_empty() {
                        let w = p.measure(&key, label);
                        p.text_in(
                            &key,
                            Rect::new(at.x - w - 4.0 * k, at.y, w, at.h),
                            Align::Right,
                            label,
                        );
                    }
                }
                p.text_in(
                    &number,
                    Rect::new(x0 - 24.0 * k, y, 20.0 * k, slot),
                    Align::Centre,
                    &(self.spell_tab + 1).to_string(),
                );
            } else {
                p.text_in(
                    &number,
                    Rect::new(x0 - 24.0 * k, y, 20.0 * k, slot),
                    Align::Centre,
                    &bar.to_string(),
                );
            }
            // The spell bar shows the page of twelve holding the selected spell; a tab holds as
            // many spells as the player puts in it.
            let page_base = (self.spell_index / SPELL_PAGE) * SPELL_PAGE;
            if bar == 2 {
                self.spell_pages(p, ctx, state, x0 + 12.0 * (slot + gap) + 4.0 * k, y, slot);
            }
            for (i, key) in keys.iter().enumerate() {
                let r = Rect::new(x0 + i as f32 * (slot + gap), y, slot, slot + 2.0 * k);
                // The spell's place in its tab, on this page.
                let j = if bar == 2 { page_base + i } else { i };
                // Bar 1 is the shortcut bar; bar 2 the first spell tab.
                let (icon, name, action): (Option<Sprite>, Option<String>, Option<UiRequest>) =
                    if bar == 1 {
                        match state.shortcuts.iter().find(|s| s.index as usize == i) {
                            Some(s) => (
                                s.look
                                    .as_ref()
                                    .and_then(|i| p.art.ac_item(i))
                                    .or_else(|| s.ac_icon.and_then(|d| p.art.ac_icon(d))),
                                Some(s.name.clone()),
                                if state.targeting {
                                    s.object.map(UiRequest::ExecuteTargetItem)
                                } else {
                                    s.spell
                                        .map(|spell_id| UiRequest::CastSpell { spell_id })
                                        .or_else(|| s.object.map(UiRequest::Use))
                                },
                            ),
                            None => (None, None, None),
                        }
                    } else {
                        match state.spell_tabs.get(self.spell_tab).and_then(|t| t.get(j)) {
                            Some(id) => {
                                let spell = state.spells.iter().find(|s| s.id == *id);
                                (
                                    spell.and_then(|s| {
                                        p.art
                                            .ac_spell(s.ac_icon, s.icon_power, s.bitfield)
                                            .or_else(|| p.art.ac_icon(s.ac_icon))
                                    }),
                                    spell.map(|s| s.name.clone()),
                                    Some(UiRequest::CastSpell { spell_id: *id }),
                                )
                            }
                            None => (None, None, None),
                        }
                    };
                let cooldown = if bar == 1 {
                    state
                        .shortcuts
                        .iter()
                        .find(|s| s.index as usize == i)
                        .and_then(|s| s.cooldown)
                } else {
                    None
                };
                let pressed = kit::icon_slot(p, ctx, r, icon.as_ref(), WHITE);
                // A right-click on a spell identifies it, in a window of its own.
                if bar == 2 && ctx.input.right_clicked(&r) {
                    if let Some(id) = state.spell_tabs.get(self.spell_tab).and_then(|t| t.get(j)) {
                        self.asks.push(crate::ui::panels::info::Ask::Spell(*id));
                    }
                }
                // The spell the cast key casts, ringed.
                if bar == 2 && j == self.spell_index && icon.is_some() {
                    p.outline(r, 2.0 * k, 0xFFF0_C860);
                }
                // The time left counted down every frame from what the game last said; a new
                // figure from the game that disagrees by more than a moment replaces it.
                let cooldown = match cooldown {
                    Some((left, length)) if bar == 1 => {
                        let seen = self.cooldowns.entry(i).or_insert((left, ctx.time));
                        let now = run_down(seen, left, ctx.time);
                        (now > 0.0).then_some((now, length))
                    }
                    other => {
                        if bar == 1 {
                            self.cooldowns.remove(&i);
                        }
                        other
                    }
                };
                if let Some((left, length)) = cooldown {
                    recast_sweep(p, r, left, length);
                }
                if bar == 1 {
                    self.drop_slots.push((r, u32::try_from(i).unwrap_or(0)));
                } else {
                    ctx.drops.push((
                        r,
                        Some(crate::ui::kit::Drop::SpellSlot {
                            tab: self.spell_tab,
                            index: j,
                        }),
                    ));
                }
                if bar == 1 {
                    p.text(&small, r.x + 3.0 * k, r.y + 1.0 * k, key);
                }
                if ctx.input.hover(&r) {
                    if let Some(n) = &name {
                        self.tip(
                            n.clone(),
                            vec![if bar == 1 {
                                "Shortcut".into()
                            } else {
                                "Spell".into()
                            }],
                        );
                    }
                }
                if pressed {
                    // A shortcut is picked up by the press, and selected by a release that did
                    // not drag it; a double-click uses it. A spell is cast at once.
                    let shortcut = state
                        .shortcuts
                        .iter()
                        .find(|s| bar == 1 && s.index as usize == i && s.spell.is_none());
                    let spell = (bar == 2)
                        .then(|| {
                            state
                                .spell_tabs
                                .get(self.spell_tab)
                                .and_then(|t| t.get(j))
                                .copied()
                        })
                        .flatten();
                    match (
                        shortcut.and_then(|s| s.object.map(|o| (o, s.ac_icon))),
                        spell,
                    ) {
                        (Some(_), _) if ctx.input.double => {
                            ctx.input.double = false;
                            *ctx.drag = None;
                            if let Some(use_it) = action {
                                out.requests.push(use_it);
                            }
                        }
                        (Some((item, icon)), _) => {
                            *ctx.drag = Some(crate::ui::Drag {
                                item,
                                look: shortcut.and_then(|s| s.look.clone()),
                                spell_look: None,
                                from_shortcut: Some(u32::try_from(i).unwrap_or(0)),
                                icon,
                                origin: ctx.input.mouse,
                                active: false,
                                on_click: Some(UiRequest::Select(item)),
                                spell: None,
                                from_spell_slot: None,
                                component: false,
                            });
                        }
                        // A spell is picked up by the press, and cast by a release that did not
                        // drag it; dragged, it leaves the bar or moves along it.
                        (None, Some(spell_id)) => {
                            self.spell_index = j;
                            *ctx.drag = Some(crate::ui::Drag {
                                item: dereth_primitives::ObjectId(0),
                                look: None,
                                spell_look: state
                                    .spells
                                    .iter()
                                    .find(|s| s.id == spell_id)
                                    .map(|s| (s.icon_power, s.bitfield)),
                                from_shortcut: None,
                                icon: state
                                    .spells
                                    .iter()
                                    .find(|s| s.id == spell_id)
                                    .map(|s| s.ac_icon),
                                origin: ctx.input.mouse,
                                active: false,
                                on_click: action,
                                spell: Some(spell_id),
                                from_spell_slot: Some((self.spell_tab, j)),
                                component: false,
                            });
                        }
                        (None, None) => out.requests.extend(action),
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // The EXP bar
    // -----------------------------------------------------------------------------------------

    fn exp_bar(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, state: &GameState) {
        let k = p.scale;
        let (sw, _) = p.screen;
        // Centred along the top of the screen, its figures above it, clear of the server line at
        // the top right.
        let w = (620.0 * k).min(sw - 2.0 * 290.0 * k).max(200.0 * k);
        let x = sw / 2.0 - w / 2.0;
        let y = 20.0 * k;
        let bar = Rect::new(x, y, w, 8.0 * k);
        let fill = match (state.xp_this, state.xp_next) {
            (Some(into), Some(span)) if span > 0 => {
                #[allow(clippy::cast_precision_loss)]
                let f = into as f32 / span as f32;
                f
            }
            _ => 0.0,
        };
        // The gauge: the track, the fill inside its rounded ends, and the frame.
        if !kit::own_gauge(p, bar, fill, 0xFFE8_B84A) {
            p.fill(
                Rect::new(x, y, w * fill.clamp(0.0, 1.0), bar.h),
                0xFFE8_B84A,
            );
        }
        let label = TextStyle::new(Family::Numerals, 12.0, 0xFFEE_E1C5).edge(0xFF00_0000);
        let lv = format!("Lv{}", state.level);
        p.text(&label, x, y - 18.0 * k, &lv);
        let body = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let xp = match state.xp_next {
            Some(span) if span > 0 => format!(
                "EXP {}/{}",
                grouped(state.xp_this.unwrap_or(0)),
                grouped(span)
            ),
            _ => format!("EXP {}", grouped(state.total_xp)),
        };
        let lw = p.measure(&label, &lv);
        p.text(&body, x + lw + 16.0 * k, y - 18.0 * k, &xp);
        if state.unassigned_xp > 0 {
            let un = format!("Unassigned {}", grouped(state.unassigned_xp));
            let uw = p.measure(&body, &un);
            p.text(&body.colour(0xFF9A_D0FF), x + w - uw, y - 18.0 * k, &un);
        }
        if ctx.over(&Rect::new(x, y - 20.0 * k, w, 30.0 * k)) {
            self.tip(
                "Experience",
                vec![
                    format!("Total {}", grouped(state.total_xp)),
                    format!("Unassigned {}", grouped(state.unassigned_xp)),
                ],
            );
        }
    }

    // -----------------------------------------------------------------------------------------
    // The target bar
    // -----------------------------------------------------------------------------------------

    fn target_bar(&mut self, p: &mut Painter<'_>, _ctx: &mut Ctx<'_>, state: &GameState) {
        let Some(t) = &state.target else {
            return;
        };
        let k = p.scale;
        let (sw, _) = p.screen;
        let w = 440.0 * k;
        let x = sw / 2.0 - w / 2.0 + 60.0 * k;
        // Under the experience bar.
        let y = 68.0 * k;
        let name_colour: Argb = match t.relation {
            Relation::Hostile => 0xFFF3_D36C,
            Relation::Engaged => 0xFFFF_7B52,
            Relation::Npc => 0xFFA8_E0A0,
            Relation::Player => 0xFFD8_E8FF,
            Relation::Object => 0xFFFF_FFFF,
        };
        let name = TextStyle::new(Family::Body, 18.0, name_colour).edge(0xFF00_0000);
        let mut label = t.name.clone();
        if let Some(l) = t.level {
            label = format!("Lv{l} {label}");
        }
        p.text(&name, x, y - 26.0 * k, &label);
        let bar = Rect::new(x, y, w, 14.0 * k);
        // A creature's or a person's health; an object has none to show.
        let living = t.relation != Relation::Object || t.health.is_some();
        if living {
            let fill: Argb = match t.relation {
                Relation::Hostile | Relation::Engaged => 0xFFD6_4B3A,
                _ => 0xFF6E_CB5C,
            };
            kit::gauge(p, bar, t.health.unwrap_or(1.0), fill);
        }
        if let Some(h) = t.health {
            let pct = TextStyle::new(Family::BarNumbers, 16.0, 0xFFFF_FFFF).edge(0xFF33_2211);
            #[allow(clippy::cast_possible_truncation)]
            let s = format!("{}%", dereth_primitives::num::to_i32((h * 100.0).round()));
            let pw = p.measure(&pct, &s);
            p.text(&pct, bar.right() - pw, y - 22.0 * k, &s);
        }
        // An object's picture before the bar, as the game's item lists compose it, on its own
        // tile and with no slot frame; a creature or a person shows none.
        let icon = t
            .look
            .as_ref()
            .and_then(|i| p.art.ac_item(i))
            .or_else(|| t.icon.and_then(|d| p.art.ac_icon(d)))
            .filter(|_| t.relation == Relation::Object);
        if let Some(icon) = icon {
            let ir = Rect::new(x - 52.0 * k, y - 28.0 * k, 40.0 * k, 40.0 * k);
            p.sprite(&icon, ir, WHITE);
        }
    }

    // -----------------------------------------------------------------------------------------
    // The status row
    // -----------------------------------------------------------------------------------------

    fn status_row(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, state: &GameState) {
        let k = p.scale;
        let (sw, _) = p.screen;
        let size = 32.0 * k;
        let right = sw - 260.0 * k;
        let count_style = TextStyle::new(Family::Numerals, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        // Every icon of the row an even step apart: an icon, or the widest figure under one
        // ("300%"), whichever is wider, and a gap.
        let pitch = size.max(p.measure(&count_style, "300%")) + 8.0 * k;
        // One icon for the beneficial effects and one for the harmful ones, each showing the
        // effect that ends first and how many there are; a click lists them all.
        let mut x = right - size;
        for harmful in [false, true] {
            let mut effects: Vec<&crate::ui::game::Effect> = state
                .effects
                .iter()
                .filter(|e| e.harmful == harmful)
                .collect();
            if effects.is_empty() {
                if self.effects_open == Some(harmful) {
                    self.effects_open = None;
                }
                continue;
            }
            effects.sort_by(|a, b| {
                a.remaining
                    .unwrap_or(f64::MAX)
                    .total_cmp(&b.remaining.unwrap_or(f64::MAX))
            });
            let r = Rect::new(x, 44.0 * k, size, size);
            x -= pitch;
            // The interface's own icon for the group; else the first effect's, marked green
            // or red along its foot.
            let own = p.piece(if harmful {
                "status.harmful"
            } else {
                "status.beneficial"
            });
            if let Some(icon) = own {
                p.sprite(&icon, r, WHITE);
            } else {
                if let Some(icon) = p.art.ac_component(effects[0].ac_icon) {
                    p.sprite(&icon, r, WHITE);
                } else {
                    p.fill(r, 0x80404040);
                }
                let rim = if harmful { 0xFFE0_4040 } else { 0xFF60_C060 };
                p.fill(Rect::new(r.x, r.bottom() - 3.0 * k, r.w, 3.0 * k), rim);
            }
            p.text_in(
                &count_style,
                Rect::new(r.x, r.bottom(), r.w, 16.0 * k),
                Align::Centre,
                &effects.len().to_string(),
            );
            if ctx.over(&r) {
                let what = if harmful { "Harmful" } else { "Beneficial" };
                self.tip(
                    format!("{what} effects"),
                    vec![format!("{} in force. Click to list them.", effects.len())],
                );
                if ctx.input.clicked(&r) {
                    self.effects_open = if self.effects_open == Some(harmful) {
                        None
                    } else {
                        self.effects_scroll = 0.0;
                        self.effects_opened_at = ctx.time;
                        Some(harmful)
                    };
                }
            }
            if self.effects_open == Some(harmful) {
                self.effects_list(
                    p,
                    ctx,
                    &effects,
                    Rect::new(r.right() - 300.0 * k, r.bottom() + 20.0 * k, 300.0 * k, 0.0),
                );
            }
        }
        self.lamps(p, ctx, state, x, size, pitch);
    }

    /// The list of one kind of effect: each one's icon, name and time left, under its icon.
    fn effects_list(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        effects: &[&crate::ui::game::Effect],
        at: Rect,
    ) {
        let k = p.scale;
        let row_h = 30.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * effects.len() as f32;
        // Inside the frame, clear of its corners; the window exactly as tall as the rows it
        // shows and its insets.
        let (il, it, ir, ib) = kit::panel_inset(p);
        let (top, bottom) = ((it * 0.6).max(8.0 * k), (ib * 0.6).max(8.0 * k));
        let r = Rect::new(at.x, at.y, at.w, content.min(row_h * 12.0) + top + bottom);
        kit::panel(p, r, 0.95);
        if ctx.over(&r) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.captured = true;
        }
        // Escape closes the list, but a window opened since takes it first.
        if !self.window_over_effects && ctx.input.take_key(crate::ui::input::vk::ESCAPE) {
            self.effects_open = None;
        }
        let area = Rect::new(
            r.x + il.max(8.0 * k),
            r.y + top,
            r.w - il.max(8.0 * k) - ir.max(8.0 * k),
            r.h - top - bottom,
        );
        let bar = kit::scrollbar_width(p).map_or(0.0, |w| (w + 6.0) * k);
        let offset = kit::scroll(p, ctx, area, content, &mut self.effects_scroll);
        let name = TextStyle::new(Family::Body, 13.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let time = TextStyle::new(Family::Body, 12.0, 0xFFC8_D8E8).edge(0xFF00_0000);
        p.list.push_clip(area);
        for (i, e) in effects.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let y = area.y + row_h * i as f32 - offset;
            if y + row_h < area.y || y > area.bottom() {
                continue;
            }
            let icon = Rect::new(area.x, y + 2.0 * k, 24.0 * k, 24.0 * k);
            if let Some(s) = p.art.ac_component(e.ac_icon) {
                p.sprite(&s, icon, WHITE);
            }
            let text_w = area.w - 100.0 * k;
            let shown = p.fit(&name, &e.name, text_w);
            p.text(&name, icon.right() + 8.0 * k, y + 6.0 * k, &shown);
            // A right-click examines the effect, in a window of its own.
            let row = Rect::new(area.x, y, area.w - bar, row_h);
            if y >= area.y - row_h / 2.0 && ctx.input.right_clicked(&row) {
                self.asks
                    .push(crate::ui::panels::info::Ask::Effect(e.spell));
            }
            let left = e.remaining.map_or_else(|| "Lasting".to_owned(), time_left);
            let w = p.measure(&time, &left);
            p.text(&time, area.right() - bar - w - 6.0 * k, y + 7.0 * k, &left);
        }
        p.list.pop_clip();
    }

    // -----------------------------------------------------------------------------------------
    // The minimap and the server-info line
    // -----------------------------------------------------------------------------------------

    fn minimap(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let (sw, _) = p.screen;
        let d = 196.0 * k;
        let x = sw - d - 26.0 * k;
        let y = 44.0 * k;
        let r = Rect::new(x, y, d, d);
        let centre = (x + d / 2.0, y + d / 2.0);
        // The radar stands north up, or turns with the character; turning, it follows over a
        // moment, not in a snap, so a quick turn to and fro shows as a sway: the shown heading
        // closes on the true one by the shorter way.
        let aim = if self.minimap_rotates {
            state.heading
        } else {
            0.0
        };
        let heading = {
            let (shown, at) = self.radar_heading.unwrap_or((aim, ctx.time));
            #[allow(clippy::cast_possible_truncation)]
            let dt = (ctx.time - at).clamp(0.0, 0.25) as f32;
            let gap = (aim - shown + 540.0).rem_euclid(360.0) - 180.0;
            let ease = 1.0 - dereth_primitives::num::math::expf(-dt / RADAR_EASE_SECONDS);
            let next = (shown + gap * ease).rem_euclid(360.0);
            self.radar_heading = Some((next, ctx.time));
            next
        };
        // The blips come turned to the true heading; turned on by what the radar lags it.
        let lag = (state.heading - heading).to_radians();
        let (lag_sin, lag_cos) = (
            dereth_primitives::num::math::sinf(lag),
            dereth_primitives::num::math::cosf(lag),
        );
        // The dark disc of the map, under the blips.
        let own = p.art.has_piece("minimap.ring");
        if let Some(mask) = own.then(|| p.piece("minimap.mask")).flatten() {
            p.sprite(&mask, r, 0x9020_2A34);
        } else {
            p.fill(r.inset(20.0 * k), 0x7028_3848);
        }
        // The radar's blips, its reach to the rim, in the game's colours and shapes. The one
        // under the pointer is named, and a click selects it.
        let range = if state.radar_range > 0.0 {
            state.radar_range
        } else {
            75.0
        } * self.zoom();
        let radius = d / 2.0 - 16.0 * k;
        let mut hovered: Option<(f32, &crate::ui::game::Blip)> = None;
        for b in &state.blips {
            let (bx, by) = (b.dx / range * radius, -b.dy / range * radius);
            let (bx, by) = (bx * lag_cos - by * lag_sin, by * lag_cos + bx * lag_sin);
            if bx * bx + by * by > radius * radius {
                continue;
            }
            let colour = if b.colour != 0 {
                b.colour
            } else {
                match b.kind {
                    BlipKind::Creature => 0xFFFF_7050,
                    BlipKind::Npc => 0xFF90_F070,
                    BlipKind::Player => 0xFF70_C0FF,
                    BlipKind::Fellow => 0xFF66_E5FF,
                    BlipKind::Portal => 0xFFD0_90FF,
                    BlipKind::Item => 0xFFE0_E0E0,
                    BlipKind::Selected => 0xFFFF_E050,
                }
            };
            let (px, py) = (centre.0 + bx, centre.1 + by);
            let cell = 2.0 * k;
            let shape = blip_pixels(b.shape);
            for (dx, dy) in &shape {
                #[allow(clippy::cast_precision_loss)]
                p.fill(
                    Rect::new(
                        px + *dx as f32 * cell - cell / 2.0,
                        py + *dy as f32 * cell - cell / 2.0,
                        cell,
                        cell,
                    ),
                    colour,
                );
            }
            if b.kind == BlipKind::Selected {
                let s = 9.0 * k;
                p.outline(
                    Rect::new(px - s / 2.0, py - s / 2.0, s, s),
                    1.0 * k,
                    0xFFFF_E050,
                );
            }
            let (mx, my) = ctx.input.mouse;
            let dist = (mx - px).powi(2) + (my - py).powi(2);
            if dist <= (12.0 * k).powi(2) && hovered.is_none_or(|(d, _)| dist < d) {
                hovered = Some((dist, b));
            }
        }
        // Where the camera looks, a soft cone from the centre, under the player's arrow.
        if let Some(cone) = own.then(|| p.piece("minimap.cone")).flatten() {
            let c = radius * 1.1;
            p.sprite_turned(
                &cone,
                Rect::new(centre.0 - c, centre.1 - c, 2.0 * c, 2.0 * c),
                0x90F0_E0B0,
                (state.camera_heading - heading).to_radians(),
            );
        }
        // The player's arrow at the centre, pointing the way they face.
        if let Some(arrow) = own.then(|| p.piece("minimap.arrow")).flatten() {
            let h = 20.0 * k;
            let w = h * arrow.w / arrow.h.max(1.0);
            p.sprite_turned(
                &arrow,
                Rect::new(centre.0 - w / 2.0, centre.1 - h / 2.0, w, h),
                WHITE,
                (state.heading - heading).to_radians(),
            );
        }
        // The ornate bezel, then the compass letters around it.
        if let Some(ring) = own.then(|| p.piece("minimap.ring")).flatten() {
            // The ring turns with the letters: its marks stand at the compass points.
            p.sprite_turned(&ring, r, WHITE, (45.0 - heading).to_radians());
            // The own ring carries no letters: they are set in the heading face, on the
            // ring's band.
            let band = p
                .art
                .piece_value("minimap.letters_radius")
                .and_then(|v| v.first().copied())
                .unwrap_or(81.0);
            // Centred on the ring's outer edge, half out of it, larger than the band.
            let rr = (band / ring.w.max(1.0) * d).max(d / 2.0 + 1.0 * k);
            let style = TextStyle::new(Family::Heading, 24.0, 0xFFF5_EBD2).edge(0xFF14_120E);
            for (letter, deg) in [("N", 0.0), ("E", 90.0), ("S", 180.0), ("W", 270.0)] {
                let a = (deg - heading).to_radians();
                let (lx, ly) = (
                    centre.0 + dereth_primitives::num::math::sinf(a) * rr,
                    centre.1 - dereth_primitives::num::math::cosf(a) * rr,
                );
                let lr = Rect::new(lx - 14.0 * k, ly - 14.0 * k, 28.0 * k, 28.0 * k);
                // Heavier: drawn twice, a little apart.
                p.text_in(&style, lr, Align::Centre, letter);
                p.text_in(&style, lr.offset(0.8 * k, 0.0), Align::Centre, letter);
            }
        }
        // Zoom in (+) and back out (-), from the radar's full reach to a quarter of it.
        let mut zoom_button = None;
        for i in [0, 1] {
            let br = Rect::new(
                x + d - 30.0 * k,
                y + d - 58.0 * k + i as f32 * 26.0 * k,
                24.0 * k,
                24.0 * k,
            );
            let name = if i == 0 {
                "minimap.zoom-plus"
            } else {
                "minimap.zoom-minus"
            };
            if let Some(b) = own.then(|| p.piece(name)).flatten() {
                p.sprite(&b, br.inset(2.0 * k), WHITE);
            }
            if ctx.over(&br) && ctx.input.clicked(&br) {
                zoom_button = Some(i);
            }
        }
        match zoom_button {
            Some(0) => self.radar_zoom = (self.zoom() * 0.5).max(0.25),
            Some(_) => self.radar_zoom = (self.zoom() * 2.0).min(1.0),
            None => {}
        }
        // Coordinates and the region under the map.
        let small = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        if let Some((n, e)) = state.coords {
            let s = format!(
                "{:.1}{} {:.1}{}",
                n.abs(),
                if n >= 0.0 { "N" } else { "S" },
                e.abs(),
                if e >= 0.0 { "E" } else { "W" }
            );
            p.text_in(
                &small,
                Rect::new(x, y + d + 10.0 * k, d, 16.0 * k),
                Align::Centre,
                &s,
            );
        }
        if ctx.over(&r) {
            if let Some((_, b)) = hovered {
                self.tip(b.name.clone(), vec![]);
                if ctx.input.clicked(&r) {
                    out.requests.push(UiRequest::Select(b.id));
                }
            } else if ctx.input.clicked(&r) {
                // A click on no blip is the minimap's own: it does not reach the world.
            }
        }
    }

    fn server_info(&mut self, p: &mut Painter<'_>, _ctx: &mut Ctx<'_>, state: &GameState) {
        let k = p.scale;
        let (sw, _) = p.screen;
        let style = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let mut parts = Vec::new();
        if let Some(w) = &state.world {
            parts.push(w.clone());
        }
        // The player's own clock.
        if let Some(t) = &state.local_time {
            parts.push(t.clone());
        }
        // The link: the last round trip and the loss, and a lamp for how lately the server was
        // heard (green, then amber after two seconds, red after five or when it is not).
        if let Some(ms) = state.ping_ms {
            parts.push(format!("{ms:.0} ms"));
        }
        let text = parts.join("  |  ");
        let w = p.measure(&style, &text);
        let ty = 5.0 * k;
        p.text(&style, sw - w - 30.0 * k, ty, &text);
        if state.in_world {
            let lamp = match state.link_quiet {
                Some(s) if s < 2.0 => 0xFF6E_CB5C,
                Some(s) if s < 5.0 => 0xFFE8_B84A,
                _ => 0xFFE0_4040,
            };
            // The lamp centred on the line's letters, in a dark ring.
            let d = 9.0 * k;
            let mid = p
                .letter_band(&style)
                .map_or(p.line_height(&style) / 2.0, |(top, base)| {
                    (top + base) / 2.0
                });
            let l = Rect::new(sw - 24.0 * k, ty + mid - d / 2.0, d, d);
            p.fill(l, 0xFF14_100C);
            p.fill(l.inset(1.5 * k), lamp);
            let hover = Rect::new(sw - w - 30.0 * k, 8.0 * k, w + 30.0 * k, 20.0 * k);
            if _ctx.input.hover(&hover) {
                let mut lines = Vec::new();
                if let Some(ms) = state.ping_ms {
                    lines.push(format!("Round trip: {ms:.0} ms"));
                }
                lines.push(format!("Packet loss: {:.1}%", state.packet_loss * 100.0));
                if let Some(s) = state.link_quiet {
                    lines.push(format!("Last heard: {s:.1} s ago"));
                }
                self.tip("Connection", lines);
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // The party list
    // -----------------------------------------------------------------------------------------

    fn party_list(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        // The list is the fellowship's: with none, there is no party to show.
        if state.fellowship.is_empty() {
            return;
        }
        let k = p.scale;
        let x = 22.0 * k;
        let mut y = 86.0 * k;
        type Vitals = (
            crate::ui::game::Vital,
            crate::ui::game::Vital,
            crate::ui::game::Vital,
        );
        // The player first, then the others in the fellowship's order; each with health, stamina
        // and mana, and whether they lead.
        let self_leads = state
            .fellowship
            .iter()
            .any(|(name, _, _, _, leader)| name == &state.name && *leader);
        let mut rows: Vec<(String, Vitals, bool)> = vec![(
            state.name.clone(),
            (state.health, state.stamina, state.mana),
            self_leads,
        )];
        for (name, hp, sp, mp, leader) in &state.fellowship {
            if name != &state.name {
                rows.push((name.clone(), (*hp, *sp, *mp), *leader));
            }
        }
        let name_style = TextStyle::new(Family::Body, 14.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let num = TextStyle::new(Family::Numerals, 12.0, 0xFFEE_E1C5).edge(0xFF00_0000);
        let hp_style = TextStyle::new(Family::BarNumbers, 16.0, 0xFFFF_FFFF).edge(0xFF33_2211);
        let badge = TextStyle::new(Family::Numerals, 10.0, 0xFF20_1000);
        for (i, (name, (hp, sp, mp), leader)) in rows.iter().enumerate() {
            let row = Rect::new(x, y, 320.0 * k, 46.0 * k);
            if ctx.over(&row) {
                party_row(p, row);
            }
            p.text(&num, x, y + 6.0 * k, &(i + 1).to_string());
            let label = if i == 0 {
                format!("Lv{} {name}", state.level)
            } else {
                name.clone()
            };
            let lw = p.text(&name_style, x + 22.0 * k, y + 2.0 * k, &label);
            // The leader's mark beside the name.
            if *leader {
                let b = Rect::new(x + 28.0 * k + lw, y + 4.0 * k, 16.0 * k, 16.0 * k);
                p.fill(b, 0xFFE8_C060);
                p.outline(b, 1.0 * k, 0xFF40_2808);
                p.text_in(&badge, b, Align::Centre, "L");
                if ctx.over(&b) {
                    self.tip("Fellowship leader", vec![]);
                }
            }
            let hpb = Rect::new(x + 22.0 * k, y + 24.0 * k, 120.0 * k, 10.0 * k);
            kit::gauge(p, hpb, hp.ratio(), 0xFF6E_CB5C);
            let spb = Rect::new(x + 150.0 * k, y + 24.0 * k, 80.0 * k, 10.0 * k);
            kit::gauge(p, spb, sp.ratio(), 0xFFE8_B84A);
            let mpb = Rect::new(x + 238.0 * k, y + 24.0 * k, 80.0 * k, 10.0 * k);
            kit::gauge(p, mpb, mp.ratio(), 0xFFDF_6FB5);
            let n = hp.current.to_string();
            let nw = p.measure(&hp_style, &n);
            p.text(&hp_style, hpb.right() - nw, y + 30.0 * k, &n);
            // A click on a member's row selects them, as the fellowship window's rows do.
            if ctx.over(&row) && ctx.input.clicked(&row) {
                if let Some(id) = party_member(state, i, name) {
                    out.requests.push(UiRequest::Select(id));
                }
            }
            if ctx.over(&row) {
                self.tip(
                    name.clone(),
                    vec![
                        format!("Health {} / {}", hp.current, hp.max),
                        format!("Stamina {} / {}", sp.current, sp.max),
                        format!("Mana {} / {}", mp.current, mp.max),
                    ],
                );
            }
            y += 54.0 * k;
        }
    }

    // -----------------------------------------------------------------------------------------
    // The log window
    // -----------------------------------------------------------------------------------------

    // -----------------------------------------------------------------------------------------
    // The main menu
    // -----------------------------------------------------------------------------------------

    fn main_menu(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        windows: &mut Windows,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        // Character, Journal, Spellbook, Travel, Social, Allegiance, System: the menu's seven
        // medallions.
        let menus: [(&str, &[(&str, MenuAction)]); 7] = [
            (
                "Character",
                &[
                    ("Character", MenuAction::Open(WindowId::Character)),
                    ("Inventory", MenuAction::Open(WindowId::Inventory)),
                ],
            ),
            (
                "Journal",
                &[("Journal", MenuAction::Open(WindowId::Journal))],
            ),
            (
                "Spellbook",
                &[("Spellbook", MenuAction::Open(WindowId::Actions))],
            ),
            (
                "Travel",
                &[
                    ("Map", MenuAction::Open(WindowId::Map)),
                    ("House", MenuAction::Open(WindowId::House)),
                ],
            ),
            ("Social", &[("Social", MenuAction::Open(WindowId::Social))]),
            (
                "Allegiance",
                &[("Allegiance", MenuAction::Open(WindowId::Allegiance))],
            ),
            (
                "System",
                &[
                    ("Settings", MenuAction::Open(WindowId::Options)),
                    ("HUD Layout", MenuAction::Open(WindowId::Layout)),
                    ("Log Out", MenuAction::LogOut),
                    ("Exit Game", MenuAction::Quit),
                ],
            ),
        ];
        // The seven buttons close together on one shared bar, with room above and below them,
        // the bar as far from the screen's right edge as from its foot. Each on a whole pixel.
        let size = (34.0 * k).round();
        let gap = (2.0 * k).round().max(1.0);
        let (pad_x, pad_y) = ((6.0 * k).round(), (7.0 * k).round());
        let margin = (14.0 * k).round();
        // A button with nothing the world's era offers is left off the bar.
        let shown: Vec<usize> = menus
            .iter()
            .enumerate()
            .filter(|(_, (_, items))| {
                items.iter().any(|(_, a)| match a {
                    MenuAction::Open(id) => windows.offers(*id),
                    _ => true,
                })
            })
            .map(|(i, _)| i)
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let count = shown.len() as f32;
        let bar_w = count * size + (count - 1.0).max(0.0) * gap + 2.0 * pad_x;
        let bar = Rect::new(
            sw - margin - bar_w,
            sh - margin - size - 2.0 * pad_y,
            bar_w,
            size + 2.0 * pad_y,
        );
        let own_art = p.art.has_piece("window.tl");
        if own_art {
            // So the gold reads against any ground.
            kit::plain_ground(p, bar);
        }
        let x0 = bar.x + pad_x;
        let y = bar.y + pad_y;
        for (place, &i) in shown.iter().enumerate() {
            let (name, items) = &menus[i];
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(x0 + place as f32 * (size + gap), y, size, size);
            let over = ctx.over(&r);
            if own_art && (over || self.menu_open == Some(i)) {
                p.fill(r, 0x30C8_9646);
            }
            // The own icons, one for each of the menu's groups.
            let own_icon = [
                "menu.character",
                "menu.journal",
                "menu.spellbook",
                "menu.map",
                "menu.social",
                "menu.allegiance",
                "menu.settings",
            ]
            .get(i)
            .and_then(|n| p.piece(n));
            if let Some(icon) = own_icon {
                p.sprite(
                    &icon,
                    r,
                    if over || self.menu_open == Some(i) {
                        WHITE
                    } else {
                        0xFFD0_D0D0
                    },
                );
            }
            // What the world's era does not have is not on the menu.
            let items: Vec<_> = items
                .iter()
                .filter(|(_, a)| match a {
                    MenuAction::Open(id) => windows.offers(*id),
                    _ => true,
                })
                .collect();
            if over {
                if self.menu_open.is_none() {
                    self.tip(*name, vec![]);
                }
                if ctx.input.clicked(&r) {
                    // A button of one choice opens it at once; another opens its menu.
                    if let [(_, MenuAction::Open(id))] = items.as_slice() {
                        windows.toggle(*id, ctx.time);
                        self.menu_open = None;
                    } else {
                        self.menu_open = if self.menu_open == Some(i) {
                            None
                        } else {
                            Some(i)
                        };
                    }
                }
            }
            if self.menu_open == Some(i) {
                // The sub-menu, rising above the medallion.
                let style = TextStyle::new(Family::Body, 14.0, 0xFFFF_FFFF).edge(0xFF00_0000);
                let row_h = 26.0 * k;
                let w = items
                    .iter()
                    .map(|(l, _)| p.measure(&style, l))
                    .fold(0.0, f32::max)
                    + 40.0 * k;
                let pad = 12.0 * k;
                let h = row_h * items.len() as f32 + 2.0 * pad;
                let mx = (r.x - w / 2.0 + size / 2.0).min(sw - w - margin);
                let menu = Rect::new(mx, bar.y - h - 6.0 * k, w, h);
                kit::plain_ground(p, menu);
                for (j, (label, action)) in items.iter().enumerate() {
                    let row = Rect::new(
                        menu.x + 6.0 * k,
                        menu.y + pad + j as f32 * row_h,
                        w - 12.0 * k,
                        row_h,
                    );
                    if ctx.over(&row) {
                        crate::ui::pregame::list_highlight(p, row, true);
                    }
                    let enabled = true;
                    let s = if enabled {
                        style
                    } else {
                        style.colour(0xFF80_8080)
                    };
                    p.text_in(&s, row.offset(12.0 * k, 0.0), Align::Left, label);
                    if ctx.input.clicked(&row) {
                        match action {
                            MenuAction::Open(id) => windows.open(*id, ctx.time),
                            MenuAction::LogOut => self.ask_log_out(ctx),
                            MenuAction::Quit => out.quit = true,
                        }
                        self.menu_open = None;
                    }
                }
                if ctx.over(&menu) && ctx.input.pressed[0] {
                    ctx.input.captured = true;
                }
            }
        }
        if ctx.input.pressed[0] && self.menu_open.is_some() && !ctx.input.captured {
            self.menu_open = None;
        }
    }

    /// The names over the people and creatures near the player, coloured by what they are, and
    /// the target marker over the selection, tinted to match.
    fn nameplates(&mut self, p: &mut Painter<'_>, ctx: &Ctx<'_>, state: &GameState) {
        let k = p.scale;
        let target_colour = state.target.as_ref().map(|t| match t.relation {
            Relation::Hostile => 0xFFF3_D36C,
            Relation::Engaged => 0xFFFF_7B52,
            Relation::Npc => 0xFFA8_E0A0,
            Relation::Player => 0xFFB0_D8FF,
            Relation::Object => 0xFFFF_FFFF,
        });
        // Names are full size out to `near`, then shrink and fade to nothing at `far`: the names'
        // range, or the radar's where that is shorter (indoors), since only what the radar holds
        // is named.
        let far = crate::state::NAMEPLATE_RANGE.min(state.radar_range.max(1.0));
        let near = far * NAME_FULL_SIZE_SHARE;
        for plate in &state.nameplates {
            // The selection's name stays whole.
            let fade = match plate.distance {
                Some(d) if !plate.selected => ((far - d) / (far - near)).clamp(0.0, 1.0),
                _ => 1.0,
            };
            if fade <= 0.05 {
                continue;
            }
            let colour = match plate.kind {
                BlipKind::Creature => 0xFFF3_D36C,
                BlipKind::Npc => 0xFFA8_E0A0,
                BlipKind::Player | BlipKind::Fellow => 0xFFB0_D8FF,
                BlipKind::Selected => target_colour.unwrap_or(0xFFFF_FFFF),
                BlipKind::Portal | BlipKind::Item => 0xFFE0_E0E0,
            };
            let style = TextStyle::new(Family::Body, 14.0 * fade, with_alpha(colour, fade))
                .edge(with_alpha(0xFF00_0000, fade));
            let w = p.measure(&style, &plate.name);
            // Clear of the subject's head: a little higher the taller it stands on screen.
            let y = plate.y - 32.0 * k - plate.bounds.h * 0.04;
            p.text(&style, (plate.x - w / 2.0).round(), y.round(), &plate.name);
            if plate.selected {
                if let Some(marker) = p.piece("target.marker") {
                    let mh = 26.0 * k;
                    let mw = mh * marker.w / marker.h.max(1.0);
                    let bob = 2.0 * k * crate::ui::wave(ctx.time, 4.0);
                    let r = Rect::new(plate.x - mw / 2.0, y - mh - 6.0 * k - bob, mw, mh);
                    p.sprite(&marker, r, WHITE);
                }
            }
        }
    }

    /// The feedback lines at the top of the screen, newest lowest, each fading out at the end.
    fn feedback_notices(&self, p: &mut Painter<'_>, ctx: &Ctx<'_>) {
        let k = p.scale;
        let (sw, _) = p.screen;
        let mut y = 140.0 * k;
        for (text, at) in &self.notices {
            let age = ctx.time - at;
            let a = crate::ui::narrow(((NOTICE_SECONDS - age) / 0.5).clamp(0.0, 1.0));
            let style = TextStyle::new(Family::Body, 18.0, with_alpha(0xFFFF_6A5A, a))
                .edge(with_alpha(0xFF20_0000, a));
            p.text_in(&style, Rect::new(0.0, y, sw, 26.0 * k), Align::Centre, text);
            y += 28.0 * k;
        }
    }

    /// The level-up banner: a big gold word that fades.
    fn level_banner(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, state: &GameState) {
        let Some(at) = self.level_banner_at else {
            return;
        };
        let age = ctx.time - at;
        if age > 4.0 {
            self.level_banner_at = None;
            return;
        }
        let k = p.scale;
        let (sw, sh) = p.screen;
        #[allow(clippy::cast_possible_truncation)]
        let a = if age < 3.0 { 1.0 } else { (4.0 - age) as f32 };
        let style = TextStyle::new(Family::Banner, 46.0, with_alpha(0xFFFF_E080, a))
            .edge(with_alpha(0xFF60_3000, a));
        p.text_in(
            &style,
            Rect::new(0.0, sh * 0.3, sw, 60.0 * k),
            Align::Centre,
            "LEVEL UP",
        );
        let sub = TextStyle::new(Family::Body, 18.0, with_alpha(0xFFFF_FFFF, a))
            .edge(with_alpha(0xFF00_0000, a));
        p.text_in(
            &sub,
            Rect::new(0.0, sh * 0.3 + 64.0 * k, sw, 30.0 * k),
            Align::Centre,
            &format!("Level {}", state.level),
        );
    }
}

/// How many spells the spell bar shows at once: a page of its tab.
const SPELL_PAGE: usize = 12;

/// How long the radar takes to come most of the way round to a new heading, in seconds.
const RADAR_EASE_SECONDS: f32 = 0.08;

/// How much of the names' range they show at full size, before shrinking away over the rest.
const NAME_FULL_SIZE_SHARE: f32 = 0.6;

/// A countdown the game reports now and then, run down every frame between its reports: `seen` is
/// the last figure taken and when it was taken; a report that disagrees with the run-down figure
/// by more than a moment replaces it.
pub(crate) fn run_down(seen: &mut (f64, f64), left: f64, now: f64) -> f64 {
    if (seen.0 - (now - seen.1) - left).abs() > 1.05 {
        *seen = (left, now);
    }
    seen.0 - (now - seen.1)
}

/// The recast sweep over a slot: a dark wedge over the part of the cooldown still to run, and
/// the whole seconds left in the bar-number font.
pub(crate) fn recast_sweep(p: &mut Painter<'_>, r: Rect, left: f64, length: f64) {
    let k = r.w / 44.0;
    if p.art.has_piece("slot.empty") {
        let done = if length > 0.0 {
            (1.0 - left / length).clamp(0.0, 1.0)
        } else {
            1.0
        };
        #[allow(clippy::cast_possible_truncation)]
        sweep(p, crate::ui::kit::icon_rect(r), done as f32);
    } else {
        p.fill(r, 0x9000_0000);
    }
    let style = TextStyle::new(Family::Numerals, 18.0, 0xFFFF_FFFF).edge(0xFF00_0000);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let secs = left.ceil() as u64;
    let text = if secs >= 60 {
        format!("{}m", secs.div_ceil(60))
    } else {
        secs.to_string()
    };
    p.text_in(
        &style,
        Rect::new(r.x, r.y + 6.0 * k, r.w, r.h - 6.0 * k),
        Align::Centre,
        &text,
    );
}

/// What a main-menu entry does.
#[derive(Debug, Clone, Copy)]
enum MenuAction {
    Open(WindowId),
    LogOut,
    Quit,
}

/// A name in a chat line: its first and last-plus-one character, and the name a tell goes to.
pub type Link = (usize, usize, String);

/// A chat line's text with the game's text tags taken out, and the names they marked: each
/// name's first and last-plus-one character in that text, and the name a tell goes to.
#[must_use]
pub fn chat_links(raw: &str) -> (String, Vec<Link>) {
    if !raw.contains('<') {
        return (raw.to_owned(), Vec::new());
    }
    let tagged = dereth_ui::text::tag::parse(raw);
    let chars: Vec<char> = tagged.text.chars().collect();
    let links = tagged
        .spans
        .iter()
        .filter(|s| s.end > s.start && s.end <= chars.len())
        .map(|s| {
            let shown: String = chars[s.start..s.end].iter().collect();
            let name = s.tag.payload().map_or_else(|| shown.clone(), str::to_owned);
            (s.start, s.end, name)
        })
        .collect();
    (tagged.text, links)
}

/// The pixels of a radar blip of the game's shape `shape`, around its centre; a plain dot for a
/// shape the game does not number.
fn blip_pixels(shape: u8) -> Vec<(i32, i32)> {
    use dereth_ui_screens::mapradar::radar::BlipShape as S;
    let shape = match shape {
        1 => S::Circle,
        2 => S::AllegianceMember,
        3 => S::Threat,
        4 => S::Default,
        5 => S::FellowshipLeader,
        6 => S::Fellowship,
        7 => S::ThreatAllegiance,
        _ => return vec![(0, 0), (1, 0), (0, 1), (1, 1)],
    };
    shape.pixels()
}

/// Queue a floating notice, unless the same words are showing already: a use repeated quickly
/// says its line once.
fn push_notice(notices: &mut Vec<(String, f64)>, text: &str, at: f64) {
    if !notices.iter().any(|(t, _)| t == text) {
        notices.push((text.to_owned(), at));
    }
}

/// A fellowship row's background of the interface's own art over `row`: the cap and band at the
/// row's height, fading out over the right third of the row.
fn party_row(p: &mut Painter<'_>, row: Rect) {
    let (Some(left), Some(_)) = (
        p.art.piece("party.left", 1.0),
        p.art.piece("party.mid", 1.0),
    ) else {
        return;
    };
    let k = row.h / left.h.max(1.0);
    let (Some(left), Some(mid)) = (p.art.piece("party.left", k), p.art.piece("party.mid", k))
    else {
        return;
    };
    let lw = (left.w * k).min(row.w);
    p.sprite(
        &left.sub(0.0, 0.0, lw / k, left.h),
        Rect::new(row.x, row.y, lw, row.h),
        WHITE,
    );
    // The band runs on; its last third fades out in steps.
    let rest = Rect::new(row.x + lw, row.y, row.w - lw, row.h);
    let fade_from = row.x + row.w * 0.65;
    const STEPS: u8 = 12;
    let solid = Rect::new(rest.x, rest.y, (fade_from - rest.x).max(0.0), rest.h);
    p.tiled(&mid, solid, k, WHITE);
    let span = row.right() - fade_from.max(rest.x);
    let step = span / f32::from(STEPS);
    for i in 0..STEPS {
        let t = (f32::from(i) + 0.5) / f32::from(STEPS);
        let a = 1.0 - t * t * (3.0 - 2.0 * t);
        let x = fade_from.max(rest.x) + step * f32::from(i);
        // Each step shows the band where it would be, so the texture runs on unbroken.
        let offset = (x - rest.x) % (mid.w * k);
        let part = mid.sub(offset / k, 0.0, (step / k).min(mid.w - offset / k), mid.h);
        p.sprite(
            &part,
            Rect::new(x, row.y, part.w * k, row.h),
            crate::draw::with_alpha(WHITE, a),
        );
    }
}

/// A cooldown drawn in code over `r`: the part still to recharge darkened, a wipe going round
/// clockwise from twelve o'clock as `done` goes from 0 to 1, laid down a row at a time.
fn sweep(p: &mut Painter<'_>, r: Rect, done: f32) {
    if done >= 1.0 {
        return;
    }
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let swept = done * std::f32::consts::TAU;
    let rows = r.h.ceil().max(1.0);
    let step = r.h / rows;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (n_rows, n_cols) = (rows as u32, r.w.ceil().max(1.0) as u32);
    let col_w = r.w / n_cols as f32;
    for j in 0..n_rows {
        let y = r.y + step * j as f32;
        let dy = y + step / 2.0 - cy;
        let mut run: Option<f32> = None;
        for i in 0..=n_cols {
            let x = r.x + col_w * i as f32;
            let dark = i < n_cols && {
                let dx = x + col_w / 2.0 - cx;
                // Clockwise from straight up, 0 to a full turn.
                let a =
                    dereth_primitives::num::math::atan2f(dx, -dy).rem_euclid(std::f32::consts::TAU);
                a >= swept
            };
            match (dark, run) {
                (true, None) => run = Some(x),
                (false, Some(start)) => {
                    p.fill(Rect::new(start, y, x - start, step), 0xA000_0000);
                    run = None;
                }
                _ => {}
            }
        }
    }
    // The wipe's leading edge, a thin bright line from the centre to the rim.
    let (sx, sy) = (
        dereth_primitives::num::math::sinf(swept),
        -dereth_primitives::num::math::cosf(swept),
    );
    let reach = r.w.max(r.h) / 2.0;
    let dots = (reach / 1.5).ceil().max(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for t in 0..dots as u32 {
        let d = reach * t as f32 / dots;
        let (px, py) = (cx + sx * d, cy + sy * d);
        if r.contains(px, py) {
            p.fill(Rect::new(px - 0.75, py - 0.75, 1.5, 1.5), 0xC0FF_E8B0);
        }
    }
}

/// The object of the party list's row `row` named `name`: the player's own first, then the
/// fellow of that name.
#[must_use]
pub fn party_member(
    state: &GameState,
    row: usize,
    name: &str,
) -> Option<dereth_primitives::ObjectId> {
    if row == 0 {
        return state.player_id;
    }
    state
        .fellowship_view
        .as_ref()?
        .members
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.id)
}

/// The fellowship's member `n` (0 the first, as the party list shows them), when they are within
/// reach to select: the player, or someone the radar shows.
#[must_use]
pub fn fellow_in_reach(state: &GameState, n: usize) -> Option<dereth_primitives::ObjectId> {
    let id = state.fellowship_view.as_ref()?.members.get(n)?.id;
    (Some(id) == state.player_id || state.blips.iter().any(|b| b.id == id)).then_some(id)
}

/// Whether Escape lets go of the selection: in the world, with something selected, no window
/// open, no use waiting for its target and no power bar building.
#[must_use]
pub fn escape_deselects(state: &GameState, window_open: bool) -> bool {
    state.in_world
        && state.target.is_some()
        && !window_open
        && !state.targeting
        && state.power.is_none()
}

/// Whether Escape asks to log out: in the world, with nothing selected, no window open, no use
/// waiting for its target and no power bar building (Escape lets those go first).
#[must_use]
pub fn escape_logs_out(state: &GameState, window_open: bool) -> bool {
    state.in_world
        && state.target.is_none()
        && !window_open
        && !state.targeting
        && state.power.is_none()
}

#[cfg(test)]
mod tests {
    /// Behaviour: none (experimental Horizon interface)
    #[test]
    fn the_game_s_panel_keys_open_the_window_that_holds_that_panel() {
        use super::{window_for_action, WindowId};
        assert_eq!(window_for_action(0x1000_0018), Some(WindowId::House));
        assert_eq!(window_for_action(0x1000_000C), Some(WindowId::Vitae));
        assert_eq!(window_for_action(0x1000_001F), Some(WindowId::Options));
        assert_eq!(window_for_action(0x1000_012E), Some(WindowId::Journal));
        assert_eq!(
            window_for_action(0x1000_0003),
            None,
            "a panel it does not have"
        );
    }

    /// Behaviour: none (this client's chat log)
    #[test]
    fn a_tell_tag_comes_out_of_the_text_and_leaves_the_name_as_a_link() {
        let raw =
            r#"<Tell:IIDString:1342177281:Borin Stonefist>Borin Stonefist<\Tell> says, "Hail!""#;
        let (text, links) = super::chat_links(raw);
        assert_eq!(text, "Borin Stonefist says, \"Hail!\"");
        assert_eq!(links, vec![(0, 15, "Borin Stonefist".to_owned())]);
    }

    use super::*;

    /// Behaviour: none (the HUD's number formatting)
    #[test]
    fn numbers_are_grouped_in_threes() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }

    /// Behaviour: none (the HUD's number formatting)
    #[test]
    fn a_time_left_is_seconds_then_minutes_then_hours() {
        assert_eq!(time_left(27.2), "28");
        assert_eq!(time_left(59.0), "59");
        assert_eq!(time_left(61.0), "2m");
        assert_eq!(time_left(3600.0), "60m");
        assert_eq!(time_left(3601.0), "2h");
    }

    /// Behaviour: none (experimental Horizon interface)
    #[test]
    fn a_notice_already_showing_is_not_queued_again_but_a_different_one_is() {
        let mut notices = Vec::new();
        super::push_notice(&mut notices, "Using Corpse of a Drudge", 1.0);
        super::push_notice(&mut notices, "Using Corpse of a Drudge", 1.2);
        super::push_notice(&mut notices, "You cannot do that", 1.3);
        let shown: Vec<&str> = notices.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(shown, ["Using Corpse of a Drudge", "You cannot do that"]);
    }
}
