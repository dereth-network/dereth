//! The screens before the world: character select, which is the first screen, and character
//! creation ([`crate::ui::creation`]), drawn over a view of the world.
//!
//! Character select is the title and the lobby in one: the client's name and the world it
//! reached, the world's news, the characters with the selected one standing in the middle, and
//! the ways on: log in, create, delete or restore, and exit.

use dereth_client_contract::pregame::CharacterAction;
use dereth_primitives::ObjectId;

use crate::art::Family;
use crate::draw::{with_alpha, Rect, WHITE};
use crate::options::HorizonOptions;
use crate::ui::game::GameState;
use crate::ui::input::vk;
use crate::ui::kit::{self, Ctx, WindowState};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::{colours, Outcome, Screen};

/// The screens' state before the world.
#[derive(Debug, Default)]
pub struct Pregame {
    /// The selected character, by its place in the list.
    pub selected: usize,
    /// A log-on or deletion awaiting the player's yes.
    confirm: Option<Confirm>,
    /// Where the automatic log-in has got to, and when it last moved.
    auto: AutoLogin,
    /// When the current screen came up, for its fade-in.
    shown_at: f64,
    last_screen: Option<Screen>,
    /// How far the world's news and the character list are scrolled.
    news_scroll: f32,
    /// Each character held for deletion: its seconds left as the list gave them, and when, so
    /// the countdown runs every frame.
    deleting: std::collections::HashMap<u32, (f64, f64)>,
    list_scroll: f32,
    /// Where the selected character stands this frame, and the mark in the draw list it is
    /// drawn over, while character select shows one.
    pub doll: Option<(Rect, usize)>,
    /// The character being made, while the creation screen is up.
    pub creation: Option<crate::ui::creation::Creation>,
    /// What creation is made from: the world's creation tables and their texts. `None` where the
    /// files hold none, and character select offers no creation.
    pub creation_source: Option<CreationSource>,
}

/// The world's creation tables and the texts they name.
pub type CreationSource = (
    std::rc::Rc<dereth_chargen::CreationTables>,
    std::rc::Rc<dereth_chargen::CreationTexts>,
);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Confirm {
    LogIn(ObjectId, String),
    Delete(ObjectId, String),
    Restore(ObjectId, String),
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct AutoLogin {
    step: u8,
    at: f64,
}

/// The client's name, as character select and its footer show it.
const PRODUCT_NAME: &str = "Dereth";

/// The version at the foot of the pre-game screens: the running client's, as its host names it.
/// With the own art the version alone (the mark names the client), else the name and the version.
fn version_line(own_art: bool, state: &GameState) -> String {
    if own_art {
        state.client_version.clone()
    } else {
        format!("{PRODUCT_NAME} {}", state.client_version)
    }
}

/// A layout's anchor: `(x, y)` in thousandths of the screen, with the widget's `alignment` point
/// (0-8, top-left to bottom-right) placed there. Returns the widget's top-left.
#[must_use]
pub fn anchor(
    screen: (f32, f32),
    per_mille: (f32, f32),
    alignment: u8,
    size: (f32, f32),
) -> (f32, f32) {
    let (ax, ay) = (
        screen.0 * per_mille.0 / 1000.0,
        screen.1 * per_mille.1 / 1000.0,
    );
    let fx = f32::from(alignment % 3) / 2.0;
    let fy = f32::from(alignment / 3) / 2.0;
    (ax - size.0 * fx, ay - size.1 * fy)
}

/// The line, in layout units from the top, the world's name and the character count stand on.
const COUNT_LINE: f32 = 160.0;

/// Where character select puts its parts on a screen of `screen` at `scale`: the news on the
/// left, the characters and their buttons on the right, the selected character between.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LobbyLayout {
    pub news: Rect,
    pub list: Rect,
    pub log_in: Rect,
    pub create: Rect,
    pub delete: Rect,
    pub exit: Rect,
    pub doll: Rect,
}

impl LobbyLayout {
    #[must_use]
    pub fn new(screen: (f32, f32), k: f32) -> Self {
        let (sw, sh) = screen;
        let top = 196.0 * k;
        // The bottom stripe holds the buttons; everything else stops above it.
        let stripe = sh - 110.0 * k;
        let bottom = stripe + 54.0 * k;
        let news_w = (sw * 0.3).clamp(260.0 * k, 480.0 * k);
        // As far above the lower band as it starts below the upper one (which ends at 180).
        let news = Rect::new(40.0 * k, top, news_w, stripe - (top - 180.0 * k) - top);
        let list_w = 360.0 * k;
        let buttons_h = 2.0 * 34.0 * k + 10.0 * k;
        let list = Rect::new(
            sw - list_w - 40.0 * k,
            top,
            list_w,
            bottom - top - buttons_h - 12.0 * k,
        );
        let log_in = Rect::new(list.x, stripe + 12.0 * k, list.w, 34.0 * k);
        let half = (list.w - 10.0 * k) / 2.0;
        let create = Rect::new(list.x, log_in.bottom() + 8.0 * k, half, 30.0 * k);
        let delete = Rect::new(create.right() + 10.0 * k, create.y, half, 30.0 * k);
        let exit = Rect::new(news.x, create.y, 120.0 * k, create.h);
        let doll_x = news.right() + 20.0 * k;
        let doll = Rect::new(
            doll_x,
            150.0 * k,
            (list.x - 20.0 * k - doll_x).max(0.0),
            bottom - 150.0 * k,
        );
        Self {
            news,
            list,
            log_in,
            create,
            delete,
            exit,
            doll,
        }
    }
}

impl Pregame {
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        screen: &mut Screen,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        options: &HorizonOptions,
        out: &mut Outcome,
    ) {
        if self.last_screen.as_ref() != Some(screen) {
            self.last_screen = Some(screen.clone());
            self.shown_at = ctx.time;
        }
        if state.entering && *screen != Screen::Entering {
            *screen = Screen::Entering;
        }
        if options.auto_login {
            self.drive_auto_login(screen, ctx, state, options, out);
        }
        self.doll = None;
        match screen {
            Screen::Lobby => self.lobby(screen, p, ctx, state, out),
            Screen::Entering => self.entering(p, ctx, state),
            Screen::Creation => self.creating(screen, p, ctx, state, out),
            Screen::Game => {}
        }
        self.footer(p, ctx, state);
    }

    /// The creation screen: the wizard, and the way back to character select or on into the
    /// world.
    fn creating(
        &mut self,
        screen: &mut Screen,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let Some(creation) = self.creation.as_mut() else {
            *screen = Screen::Lobby;
            return;
        };
        if creation.follow_server(state, out) {
            self.creation = None;
            out.requests
                .push(dereth_client_contract::UiRequest::CharacterCreation(false));
            *screen = Screen::Entering;
            return;
        }
        if creation.frame(p, ctx, state, out) == Some(false) {
            self.creation = None;
            out.requests
                .push(dereth_client_contract::UiRequest::CharacterCreation(false));
            *screen = Screen::Lobby;
        }
    }

    /// Where the creation screen shows the character's model this frame.
    #[must_use]
    pub fn creation_preview(&self) -> Option<(Rect, &crate::ui::creation::Creation)> {
        let c = self.creation.as_ref()?;
        Some((c.preview?, c))
    }

    /// The scripted path a player's clicks would take: the first character (or the one named),
    /// Log In, Yes. Each step waits a moment so a capture can catch it.
    fn drive_auto_login(
        &mut self,
        screen: &mut Screen,
        ctx: &Ctx<'_>,
        state: &GameState,
        options: &HorizonOptions,
        out: &mut Outcome,
    ) {
        let wait = ctx.time - self.auto.at;
        match (self.auto.step, &*screen) {
            (0, Screen::Lobby) if wait > 1.5 && !state.characters.is_empty() => {
                let wanted = options.start_character.as_deref();
                let index = wanted
                    .and_then(|n| {
                        state
                            .characters
                            .iter()
                            .position(|c| c.name.eq_ignore_ascii_case(n))
                    })
                    .unwrap_or(0);
                self.selected = index;
                if let Some(c) = state.characters.get(index) {
                    self.confirm = Some(Confirm::LogIn(c.id, c.name.clone()));
                }
                self.auto = AutoLogin {
                    step: 1,
                    at: ctx.time,
                };
            }
            (1, Screen::Lobby) if wait > 1.0 => {
                if let Some(Confirm::LogIn(id, _)) = self.confirm.take() {
                    out.character_actions.push(CharacterAction::LogOn(id));
                    *screen = Screen::Entering;
                }
                self.auto = AutoLogin {
                    step: 2,
                    at: ctx.time,
                };
            }
            _ => {}
        }
    }

    fn fade(&self, ctx: &Ctx<'_>) -> f32 {
        crate::ui::narrow(((ctx.time - self.shown_at) / 0.35).min(1.0))
    }

    /// The client's name, drawn large in the heading font with the title glow, and the world
    /// it reached under it.
    fn title(&self, p: &mut Painter<'_>, ctx: &Ctx<'_>, state: &GameState) {
        let k = p.scale;
        let big = TextStyle::new(Family::Heading, 68.0, 0xFFFF_FFFF)
            .edge(0xFF15_3A70)
            .tracking(4.0);
        let name = PRODUCT_NAME.to_uppercase();
        let (x, y) = (40.0 * k, 22.0 * k);
        // With the interface's own art: Dereth's mark (its icon and name), then the world.
        if let Some(mark) = p.piece("brand.mark") {
            // The mark centred in the top shade; the world at its far left, on the count's line.
            let (sw, _) = p.screen;
            let band = 180.0 * k;
            let mh = 96.0 * k;
            let mw = mh * mark.w / mark.h.max(1.0);
            p.sprite(
                &mark,
                Rect::new(sw / 2.0 - mw / 2.0, (band - mh) / 2.0, mw, mh),
                WHITE,
            );
            self.world_line(p, ctx, state, 40.0 * k, COUNT_LINE * k, true);
            return;
        }
        p.text(&big, x, y, &name);
        let sub = TextStyle::new(Family::Numerals, 12.0, 0xFFEE_E1C5)
            .edge(0xFF00_0000)
            .tracking(4.0);
        // A plain apostrophe: the heading face has no curly one.
        p.text(&sub, x + 4.0 * k, y + 74.0 * k, "ASHERON'S CALL");
        self.world_line(p, ctx, state, x, y + 104.0 * k, false);
    }

    /// The world's name and whether it answers, at `(x, wy)`; with the interface's own art a
    /// larger marker and no character count (the list carries it).
    fn world_line(
        &self,
        p: &mut Painter<'_>,
        ctx: &Ctx<'_>,
        state: &GameState,
        x: f32,
        wy: f32,
        own: bool,
    ) {
        let k = p.scale;
        // The world: its name, whether it answers, and how many characters the account has.
        let world = state.world.clone().unwrap_or_else(|| {
            if state.host.is_empty() {
                "No world".into()
            } else {
                state.host.clone()
            }
        });
        let head = TextStyle::new(Family::Heading, 34.0, 0xFFFF_FFFF)
            .edge(ctx.colours.ui(colours::row::MENU_GLOW));
        if !own {
            p.text(&head, x, wy, &world);
        }
        let (status, colour) = if state.connected {
            ("Online", 0xFF7B_D67B)
        } else if state.failure.is_some() {
            ("Unreachable", 0xFFFF_4A4A)
        } else if state.host.is_empty() {
            ("Offline", 0xFFA0_A0A0)
        } else {
            ("Connecting", 0xFFE8_C46A)
        };
        let small = TextStyle::new(Family::Body, 12.0, 0xFFD0_D0D0).edge(0xFF00_0000);
        if own {
            // From `x`, all on one line whose letters stand centred on `wy`: the name, the
            // lamp and the word.
            let word = TextStyle::new(Family::Numerals, 14.0, 0xFFE0_DCD0).edge(0xFF00_0000);
            let line = Rect::new(x, wy - 30.0 * k, 0.0, 60.0 * k);
            p.text(&head, x, p.text_top(&head, line.y, line.h), &world);
            let sx = x + p.measure(&head, &world) + 16.0 * k;
            // A lamp: the status colour in a dark ring, larger, then the word.
            let lamp = Rect::new(sx, wy - 7.0 * k, 14.0 * k, 14.0 * k);
            p.fill(lamp, 0xFF14_100C);
            p.fill(lamp.inset(2.0 * k), colour);
            p.fill(
                Rect::new(lamp.x + 4.0 * k, lamp.y + 4.0 * k, 3.0 * k, 3.0 * k),
                0x80FF_FFFF,
            );
            p.text(
                &word,
                sx + 22.0 * k,
                p.text_top(&word, line.y, line.h),
                status,
            );
            return;
        }
        let sx = x + p.measure(&head, &world) + 16.0 * k;
        p.fill(Rect::new(sx, wy + 16.0 * k, 8.0 * k, 8.0 * k), colour);
        let line = if state.character_slots > 0 {
            format!(
                "{status}, {} of {} characters",
                state.characters.len(),
                state.character_slots
            )
        } else {
            status.to_owned()
        };
        p.text(&small, sx + 14.0 * k, wy + 12.0 * k, &line);
    }

    /// The world's news, scrolled with the wheel: the message the world gives the players
    /// choosing a character.
    fn news(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, state: &GameState, r: Rect) {
        let k = p.scale;
        kit::panel(p, r, 0.85);
        let head = TextStyle::new(Family::Heading, 23.0, ctx.colours.heading()).edge(0xFF00_0000);
        let own = p.art.has_piece("window.tl");
        let (il, it, ir, ib) = if own {
            kit::panel_inset(p)
        } else {
            (14.0 * k, 8.0 * k, 10.0 * k, 10.0 * k)
        };
        p.text(
            &head,
            r.x + il,
            r.y + it,
            if own { "Announcements" } else { "News" },
        );
        let area = Rect::new(
            r.x + il,
            r.y + it + 36.0 * k,
            r.w - il - ir,
            r.h - it - ib - 36.0 * k,
        );
        let message = state
            .world_message
            .as_deref()
            .filter(|m| !m.trim().is_empty());
        let Some(message) = message else {
            let dim = TextStyle::new(Family::Body, 13.0, 0xFFA0_A8B0).edge(0xFF00_0000);
            let none = if own {
                "No announcements."
            } else {
                "The world has no news."
            };
            p.text(&dim, area.x, area.y, none);
            return;
        };
        let body = TextStyle::new(Family::Body, 13.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let lh = p.line_height(&body);
        let lines: Vec<String> = message
            .lines()
            .flat_map(|l| {
                let wrapped = p.wrap(&body, l, area.w - 10.0 * k);
                if wrapped.is_empty() {
                    vec![String::new()]
                } else {
                    wrapped
                }
            })
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let content = lh * lines.len() as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.news_scroll);
        p.list.push_clip(area);
        for (i, line) in lines.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let ly = area.y + lh * i as f32 - offset;
            if ly + lh < area.y || ly > area.bottom() {
                continue;
            }
            p.text(&body, area.x, ly, line);
        }
        p.list.pop_clip();
    }

    fn lobby(
        &mut self,
        screen: &mut Screen,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        p.fade = self.fade(ctx);
        let layout = LobbyLayout::new(p.screen, k);
        if self.selected >= state.characters.len() {
            self.selected = 0;
        }
        let ready = self.confirm.is_none();
        if ready && !state.characters.is_empty() {
            let n = state.characters.len();
            if ctx.input.take_key(vk::DOWN) {
                self.selected = (self.selected + 1) % n;
            }
            if ctx.input.take_key(vk::UP) {
                self.selected = (self.selected + n - 1) % n;
            }
            let c = &state.characters[self.selected];
            // Logging in, deleting and restoring wait for the server to finish with the data
            // files.
            let open = state.connect_phase == crate::ui::game::ConnectPhase::Ready;
            if open && ctx.input.take_key(vk::ENTER) && c.delete_seconds == 0 {
                self.confirm = Some(Confirm::LogIn(c.id, c.name.clone()));
            }
            if open && ctx.input.take_key(vk::DELETE) {
                self.confirm = Some(if c.delete_seconds > 0 {
                    Confirm::Restore(c.id, c.name.clone())
                } else {
                    Confirm::Delete(c.id, c.name.clone())
                });
            }
        }
        // The selected character stands over the world and under everything else, the shades
        // along the top and the foot included.
        if let Some(c) = state.characters.get(self.selected) {
            if state.known_looks.contains(&c.id) {
                self.doll = Some((layout.doll, p.list.mark()));
            }
        }
        // A shade along the top and the foot, so the words read over the world.
        p.fill(Rect::new(0.0, 0.0, sw, 180.0 * k), 0x7000_0000);
        p.fill(Rect::new(0.0, sh - 110.0 * k, sw, 110.0 * k), 0x6000_0000);
        self.title(p, ctx, state);
        self.news(p, ctx, state, layout.news);
        // With the own art, how many characters of how many above the list, on the right.
        if p.art.has_piece("window.tl") && state.character_slots > 0 {
            let count = TextStyle::new(Family::Numerals, 22.0, 0xFFF0_E6CC).edge(0xFF00_0000);
            let k = p.scale;
            p.text_in(
                &count,
                Rect::new(
                    layout.list.x,
                    (COUNT_LINE - 14.0) * k,
                    layout.list.w - 8.0 * k,
                    28.0 * k,
                ),
                Align::Right,
                &format!("{} / {}", state.characters.len(), state.character_slots),
            );
        }
        self.character_list(p, ctx, state, layout.list);
        // The ways on, under the list.
        let selected = state.characters.get(self.selected);
        // Not before the server has finished with the data files.
        let phase_ready = state.connect_phase == crate::ui::game::ConnectPhase::Ready;
        if !phase_ready {
            connect_status(p, ctx, state);
        }
        let can_log_in = ready && phase_ready && selected.is_some_and(|c| c.delete_seconds == 0);
        ctx.input.nav.home(layout.log_in);
        if kit::button(p, ctx, layout.log_in, "Log In", can_log_in) {
            if let Some(c) = selected {
                self.confirm = Some(Confirm::LogIn(c.id, c.name.clone()));
            }
        }
        // Creation, while the account has a free slot and the world's files hold the tables.
        let can_create =
            ready && phase_ready && state.free_slot.is_some() && self.creation_source.is_some();
        if kit::button(p, ctx, layout.create, "Create", can_create) {
            if let Some((tables, texts)) = self.creation_source.clone() {
                self.creation = Some(crate::ui::creation::Creation::new(
                    tables,
                    texts,
                    state.account_has_tod,
                    state.chargen_response_notices,
                ));
                out.requests
                    .push(dereth_client_contract::UiRequest::CharacterCreation(true));
                *screen = Screen::Creation;
            }
        }
        // A character being deleted is restored with the same button.
        let restoring = selected.is_some_and(|c| c.delete_seconds > 0);
        let label = if restoring { "Restore" } else { "Delete" };
        if kit::button(
            p,
            ctx,
            layout.delete,
            label,
            ready && phase_ready && selected.is_some(),
        ) {
            if let Some(c) = selected {
                self.confirm = Some(if restoring {
                    Confirm::Restore(c.id, c.name.clone())
                } else {
                    Confirm::Delete(c.id, c.name.clone())
                });
            }
        }
        // The pad's cancel brings the focus to Exit.
        ctx.input.nav.cancel(layout.exit);
        if kit::button(p, ctx, layout.exit, "Exit", ready) {
            out.quit = true;
        }
        p.fade = 1.0;
        if let Some(confirm) = self.confirm.clone() {
            let (title, text, yes) = match &confirm {
                Confirm::LogIn(_, name) => ("Log In", format!("Log in with {name}?"), "Yes"),
                Confirm::Delete(_, name) => (
                    "Delete Character",
                    format!(
                        "Delete {name}? The server keeps a deleted character for a time before \
                         it is gone, and it can be restored until then."
                    ),
                    "Delete",
                ),
                Confirm::Restore(_, name) => {
                    ("Restore Character", format!("Restore {name}?"), "Restore")
                }
            };
            match message_box(p, ctx, title, &text, &[yes, "No"]) {
                Some(0) => {
                    self.confirm = None;
                    match confirm {
                        Confirm::LogIn(id, _) => {
                            out.character_actions.push(CharacterAction::LogOn(id));
                            *screen = Screen::Entering;
                        }
                        Confirm::Delete(id, _) => {
                            out.character_actions.push(CharacterAction::Delete(id));
                        }
                        Confirm::Restore(id, _) => {
                            out.character_actions.push(CharacterAction::Restore(id));
                        }
                    }
                }
                Some(_) => self.confirm = None,
                None => {}
            }
        }
    }

    /// The account's characters, a card each, scrolled with the wheel when they outrun the
    /// room: a click selects one, and a click on the selected one asks to log in with it.
    fn character_list(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
    ) {
        let k = p.scale;
        let card_h = 60.0 * k;
        let gap = 8.0 * k;
        if state.characters.is_empty() {
            // A list that has come and holds no one shows nothing; one still to come says so.
            let msg = if state.character_slots > 0 && state.failure.is_none() {
                return;
            } else if !state.connected && state.host.is_empty() {
                "No server: start with -a <account> -v <password> -h <host>."
            } else if let Some(f) = &state.failure {
                f.as_str()
            } else {
                "Obtaining the character list..."
            };
            let text = TextStyle::new(Family::Body, 14.0, 0xFFFF_FFFF).edge(0xFF00_0000);
            for (i, line) in p.wrap(&text, msg, area.w).iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                p.text(&text, area.x, area.y + 22.0 * k * i as f32, line);
            }
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let content = (card_h + gap) * state.characters.len() as f32 - gap;
        let offset = kit::scroll(p, ctx, area, content, &mut self.list_scroll);
        let name_style = TextStyle::new(Family::Body, 18.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let info_style = TextStyle::new(Family::Body, 12.0, 0xFFC8_D8E8).edge(0xFF00_0000);
        let delete_style = info_style.colour(0xFFFF_9A6A);
        let world = state.world.clone().unwrap_or_default();
        let ready = self.confirm.is_none();
        p.list.push_clip(area);
        for (i, c) in state.characters.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                area.x,
                area.y + (card_h + gap) * i as f32 - offset,
                area.w - 8.0 * k,
                card_h,
            );
            if r.bottom() < area.y || r.y > area.bottom() {
                continue;
            }
            let over = ready && ctx.over(&r) && ctx.input.hover(&area);
            let selected = i == self.selected;
            // The own card: lit when selected, a dark well where a portrait would sit, the
            // text past it.
            let own = p.halves(
                if selected || over {
                    "card.selected"
                } else {
                    "card.normal"
                },
                r,
                if selected || !over {
                    WHITE
                } else {
                    0xC0FF_FFFF
                },
            );
            let text_x = if own {
                let well = p
                    .art
                    .piece_value("card.portrait")
                    .filter(|v| v.len() == 4)
                    .unwrap_or_else(|| vec![27.0, 27.0, 109.0, 110.0]);
                let height = p.art.piece("card.normal.left", 1.0).map_or(136.0, |s| s.h);
                let s = r.h / height.max(1.0);
                let pw = Rect::new(
                    r.x + well[0] * s,
                    r.y + well[1] * s,
                    (well[2] - well[0]) * s,
                    (well[3] - well[1]) * s,
                );
                p.fill(pw, 0xFF19_1E28);
                // The character's heritage crest, where its looks are remembered.
                if let Some(crest) = state
                    .heritages
                    .get(&c.id)
                    .and_then(|h| crest_name(*h))
                    .and_then(|n| p.piece(&format!("crest.{n}")))
                {
                    let side = pw.w.min(pw.h) - 2.0 * k;
                    let cw = side * crest.w / crest.w.max(crest.h);
                    let ch = side * crest.h / crest.w.max(crest.h);
                    p.sprite(
                        &crest,
                        Rect::new(pw.x + (pw.w - cw) / 2.0, pw.y + (pw.h - ch) / 2.0, cw, ch),
                        WHITE,
                    );
                }
                pw.right() + 12.0 * k
            } else {
                r.x + 20.0 * k
            };
            if !own {
                p.fill(r, 0xC010_1830);
            }
            if (selected || over) && !own {
                let rim = if selected { 0xFF4A_B8FF } else { 0x804A_B8FF };
                p.outline(r, 2.0 * k, rim);
            }
            // A character held for deletion: the card dimmed red, its countdown running.
            let left = if c.delete_seconds > 0 {
                let seen = self
                    .deleting
                    .entry(c.id.0)
                    .or_insert((f64::from(c.delete_seconds), ctx.time));
                if (seen.0 - f64::from(c.delete_seconds)).abs() > 2.0 {
                    *seen = (f64::from(c.delete_seconds), ctx.time);
                }
                (seen.0 - (ctx.time - seen.1)).max(0.0)
            } else {
                0.0
            };
            if c.delete_seconds > 0 && own {
                p.fill(r.inset(3.0 * k), 0x40B0_2018);
            }
            if own && c.delete_seconds == 0 {
                // One line on the card: the name, centred on it.
                p.text_in(
                    &name_style,
                    Rect::new(text_x, r.y, r.right() - text_x, r.h),
                    Align::Left,
                    &c.name,
                );
            } else {
                p.text(&name_style, text_x, r.y + 8.0 * k, &c.name);
            }
            if c.delete_seconds > 0 {
                // A server that lists a pending deletion as one second left has not said how
                // long is left: say it is marked, with no clock.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let secs = left.ceil() as u64;
                let info = if c.delete_seconds <= 1 {
                    "Marked for deletion".to_owned()
                } else {
                    format!("Deleting in {}:{:02}", secs / 60, secs % 60)
                };
                p.text(&delete_style, text_x, r.y + 36.0 * k, &info);
            } else if !own {
                p.text(&info_style, text_x, r.y + 36.0 * k, &world);
            }
            // The level the character had when last seen, at the card's right.
            if own {
                if let Some(level) = state.levels.get(&c.id) {
                    let lv = TextStyle::new(Family::Numerals, 15.0, 0xFFE8_D8A8).edge(0xFF00_0000);
                    p.text_in(
                        &lv,
                        Rect::new(r.x, r.y, r.w - 18.0 * k, r.h),
                        Align::Right,
                        &format!("Lv {level}"),
                    );
                }
            }
            if over && ctx.input.clicked(&r) {
                if selected && c.delete_seconds == 0 {
                    self.confirm = Some(Confirm::LogIn(c.id, c.name.clone()));
                } else {
                    self.selected = i;
                }
            }
        }
        p.list.pop_clip();
    }

    fn entering(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, state: &GameState) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        // In portal space the game's own notice says to wait.
        if state.teleporting {
            return;
        }
        p.fill(Rect::new(0.0, 0.0, sw, sh), 0x8000_0000);
        let r = Rect::new(
            sw / 2.0 - 220.0 * k,
            sh / 2.0 - 60.0 * k,
            440.0 * k,
            120.0 * k,
        );
        kit::panel(p, r, 0.95);
        let text = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        p.text_in(
            &text,
            Rect::new(r.x, r.y + 20.0 * k, r.w, 30.0 * k),
            Align::Centre,
            "Logging in...",
        );
        // The progress light running along a thin gauge.
        let track = Rect::new(r.x + 40.0 * k, r.y + 70.0 * k, r.w - 80.0 * k, 10.0 * k);
        let t = crate::ui::narrow((ctx.time * 0.6).fract());
        kit::gauge(p, track, t, 0xFF4A_B8FF);
        let _ = state;
    }

    /// The small print at the foot of every pre-game screen, and the version at the top left.
    fn footer(&self, p: &mut Painter<'_>, ctx: &Ctx<'_>, state: &GameState) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        let small = TextStyle::new(Family::Body, 12.0, 0xFFE0_E0E0)
            .edge(ctx.colours.ui(colours::row::MENU_GLOW));
        let own = p.art.has_piece("window.tl");
        let version = version_line(own, state);
        if own {
            // Small and plain at the very foot, under the buttons.
            let plain = TextStyle::new(Family::Body, 11.0, 0xFFB8_B4AA);
            p.text(&plain, 25.0 * k, sh - 17.0 * k, &version);
        } else {
            p.text(&small, 25.0 * k, sh - 30.0 * k, &version);
        }
        if own {
            return;
        }
        let status = if state.connected {
            format!(
                "Connected to {}",
                state.world.clone().unwrap_or_else(|| state.host.clone())
            )
        } else if let Some(f) = &state.failure {
            f.clone()
        } else if state.host.is_empty() {
            "Offline".to_owned()
        } else {
            format!("Connecting to {}...", state.host)
        };
        let w = p.measure(&small, &status);
        p.text(&small, sw - w - 25.0 * k, sh - 30.0 * k, &status);
    }
}

/// Where getting in stands while character select cannot yet be used, centred in the lower band:
/// "Connecting..." or "Updating..." breathing in and out, and while the server sends the data
/// files, a bar of how much has come.
fn connect_status(p: &mut Painter<'_>, ctx: &Ctx<'_>, state: &GameState) {
    use crate::ui::game::ConnectPhase;
    if state.host.is_empty() {
        return;
    }
    let k = p.scale;
    let (sw, sh) = p.screen;
    let band = sh - 110.0 * k;
    let word = match state.connect_phase {
        ConnectPhase::Connecting => "Connecting...",
        ConnectPhase::Updating => "Updating...",
        ConnectPhase::Ready => return,
    };
    let breath = 0.55 + 0.45 * crate::ui::wave(ctx.time, std::f64::consts::TAU / 1.6);
    let style = TextStyle::new(Family::Heading, 20.0, with_alpha(0xFFF0_E6CC, breath))
        .edge(with_alpha(0xFF00_0000, breath));
    p.text_in(
        &style,
        Rect::new(0.0, band + 16.0 * k, sw, 26.0 * k),
        Align::Centre,
        word,
    );
    if let Some((got, all)) = state.patch_progress.filter(|(_, all)| *all > 0) {
        #[allow(clippy::cast_precision_loss)]
        let fill = (got as f32 / all as f32).clamp(0.0, 1.0);
        let w = 360.0 * k;
        let bar = Rect::new(sw / 2.0 - w / 2.0, band + 50.0 * k, w, 10.0 * k);
        if !kit::own_gauge(p, bar, fill, 0xFFE8_B84A) {
            p.fill(bar, 0x60FF_FFFF);
            p.fill(Rect::new(bar.x, bar.y, bar.w * fill, bar.h), 0xFFE8_B84A);
        }
    }
}

/// The crest drawn for heritage group `heritage`; the two Olthoi share one.
#[must_use]
pub fn crest_name(heritage: u32) -> Option<&'static str> {
    Some(match heritage {
        1 => "aluvian",
        2 => "gharundim",
        3 => "sho",
        4 => "viamontian",
        5 => "umbraen",
        6 => "gearknight",
        7 => "tumerok",
        8 => "lugian",
        9 => "empyrean",
        10 => "penumbraen",
        11 => "undead",
        12 | 13 => "olthoi",
        _ => return None,
    })
}

/// The highlighted list row: the teal light of a selected list item.
pub fn list_highlight(p: &mut Painter<'_>, r: Rect, on: bool) {
    if !on {
        return;
    }
    // With the interface's own art: a warm band with fine bronze lines along it, fading at both
    // ends, drawn here rather than from a picture.
    if p.art.has_piece("window.tl") {
        const STEPS: u8 = 8;
        let end = (r.w / 6.0).min(48.0 * p.scale);
        let step = end / f32::from(STEPS);
        let line = p.scale.max(1.0);
        let band = |p: &mut Painter<'_>, x: f32, w: f32, a: f32| {
            p.fill(
                Rect::new(x, r.y, w, r.h),
                crate::draw::with_alpha(0x46C8_9646, a),
            );
            p.fill(
                Rect::new(x, r.y, w, line),
                crate::draw::with_alpha(0x96C8_9646, a),
            );
            p.fill(
                Rect::new(x, r.bottom() - line, w, line),
                crate::draw::with_alpha(0x96C8_9646, a),
            );
        };
        for i in 0..STEPS {
            let a = (f32::from(i) + 0.5) / f32::from(STEPS);
            band(p, r.x + step * f32::from(i), step, a);
            band(p, r.right() - step * f32::from(i + 1), step, a);
        }
        band(p, r.x + end, r.w - 2.0 * end, 1.0);
        return;
    }
    p.fill(r, 0x6040_A0C0);
}

/// A centred message with buttons that takes every press until it is answered; the index of the
/// one clicked.
pub fn message_box(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    title: &str,
    text: &str,
    buttons: &[&str],
) -> Option<usize> {
    dialog_box(p, ctx, title, text, buttons, true, 0)
}

/// A message with buttons, `offset` boxes down and right of the centre; the index of the one
/// clicked. A modal box dims the screen, takes every press, and answers Enter with its first
/// button and Escape with its last; any other leaves the game playable around it.
/// The room the own frame keeps under a box's contents, clear of its bottom plates.
fn dialog_plates(p: &Painter<'_>) -> f32 {
    if p.art.has_piece("window.tl") {
        22.0 * p.scale
    } else {
        0.0
    }
}

/// Where the game's box saying `text` stands, the `offset`th of those up at once: a box over
/// the middle of the screen, as tall as its words, each further one a little down and right.
#[must_use]
pub fn dialog_rect(p: &Painter<'_>, text: &str, offset: usize) -> Rect {
    let k = p.scale;
    let (sw, sh) = p.screen;
    let body = TextStyle::new(Family::Body, 14.0, 0xFFFF_FFFF);
    let w = 520.0 * k;
    let lines = p.wrap(&body, text.trim(), w - 90.0 * k);
    let lh = p.line_height(&body);
    #[allow(clippy::cast_precision_loss)]
    let h = (kit::HEADER + 30.0) * k + lh * lines.len() as f32 + 60.0 * k + dialog_plates(p);
    #[allow(clippy::cast_precision_loss)]
    let shift = 24.0 * k * offset as f32;
    Rect::new(
        sw / 2.0 - w / 2.0 + shift,
        sh / 2.0 - h / 2.0 - sh * 0.12 + shift,
        w,
        h,
    )
}

pub fn dialog_box(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    title: &str,
    text: &str,
    buttons: &[&str],
    modal: bool,
    offset: usize,
) -> Option<usize> {
    let k = p.scale;
    let (sw, sh) = p.screen;
    let body = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
    let r = dialog_rect(p, text, offset);
    let w = r.w;
    let lines = p.wrap(&body, text.trim(), w - 90.0 * k);
    let lh = p.line_height(&body);
    let plates = dialog_plates(p);
    if modal {
        p.fill(Rect::new(0.0, 0.0, sw, sh), 0x5000_0000);
        // Nothing under a modal box is in the pad's reach.
        ctx.input.nav.modal();
    }
    let mut state = WindowState {
        open: true,
        pos: Some((r.x, r.y)),
        opened_at: ctx.time - 1.0,
        no_close: true,
        ..WindowState::default()
    };
    let win = kit::window(p, ctx, &mut state, title, (w / k, r.h / k), (r.x, r.y));
    let mut y = win.body.y + 8.0 * k;
    for l in &lines {
        let lw = p.measure(&body, l);
        p.text(&body, r.x + w / 2.0 - lw / 2.0, y, l);
        y += lh;
    }
    let bw = 120.0 * k;
    let total = bw * buttons.len() as f32 + 10.0 * k * (buttons.len().saturating_sub(1)) as f32;
    let mut bx = r.x + w / 2.0 - total / 2.0;
    let by = r.bottom() - 46.0 * k - plates;
    ctx.input.nav.mark_box();
    let mut clicked = None;
    for (i, label) in buttons.iter().enumerate() {
        let b = Rect::new(bx, by, bw, 30.0 * k);
        if kit::button(p, ctx, b, label, true) {
            clicked = Some(i);
        }
        // The pad's cancel answers with the last button: No to a question, OK to a notice.
        if i + 1 == buttons.len() {
            ctx.input.nav.cancel(b);
        }
        bx += bw + 10.0 * k;
    }
    ctx.input.nav.end_layer();
    if modal && ctx.input.take_key(vk::ENTER) {
        clicked = Some(0);
    }
    if modal && ctx.input.take_key(vk::ESCAPE) {
        clicked = Some(buttons.len() - 1);
    }
    if win.closed {
        clicked = Some(buttons.len() - 1);
    }
    if modal {
        // A modal box takes every press.
        ctx.input.pressed = [false; 3];
        ctx.input.captured = true;
        ctx.hot = true;
    } else if r.contains(ctx.input.mouse.0, ctx.input.mouse.1) {
        ctx.hot = true;
    }
    clicked
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    /// The version the pre-game screens show is the running client's, as its host names it, and
    /// not the version of the interface's own library.
    #[test]
    fn the_pre_game_version_line_shows_the_running_client_s_version() {
        let state = GameState {
            client_version: "9.8.7".into(),
            ..GameState::default()
        };
        assert_eq!(version_line(true, &state), "9.8.7");
        assert_eq!(version_line(false, &state), "Dereth 9.8.7");
    }
}
