//! The social window, doing what the game's social panels do, a tab each: the fellowship, the
//! allegiance and the friends.
//!
//! - **Fellowship.** Out of one: a name, whether the experience is shared, how loot is shared
//!   (the second way chosen at first, as the game's page starts), and Create. In one: its name,
//!   its settings and a row per fellow (its level, its share of the experience, its health and
//!   vigor); a row pressed selects its fellow, pressed again at once starts a tell to it. Leave;
//!   for the leader, Recruit (the one selected), Dismiss (the one selected) and Disband.
//! - **Allegiance.** While the tab shows, the game is asked for the profile about the member shown
//!   (the player at first), and no longer once it is hidden. A profile shows the allegiance's
//!   name, size and monarch, the member (its title, name and level, whether online), its patron
//!   and its vassals, and the message of the day. Home shows the player, Up the member's patron, a
//!   vassal's row that vassal. Swear (with no patron and another player selected) offers the
//!   player's oath to the one selected; Break (with the member shown the player's patron or
//!   vassal, or the player an officer) asks first, then breaks with it and shows the player.
//! - **Friends.** The player's own state (available, not to be disturbed, away; each with the
//!   game's emote for it), the friends online (their state beside them) and offline. A friend's
//!   row pressed chooses it: Remove takes it off, Tell starts a tell to it. Add adds the name typed,
//!   or else the player selected. As the game's page does, the friends are asked for again when one
//!   was added, and moving while away sets the player available.

use auberean_client_sdk::contract::view::friend_mode;
use auberean_client_sdk::contract::SocialAction;
use auberean_client_sdk::Action;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::Rect;
use crate::social::{is_player_id, Allegiance, Friend, Social, DEFAULT_LOOTING, LOOTING};
use crate::state::GameState;
use crate::ui::hud::{HEALTH, VIGOR};
use crate::ui::party::DOUBLE_PRESS;

/// The window's body, in layout units.
const BODY: (f32, f32) = (352.0, 470.0);

/// The tabs, in order.
pub const TABS: [&str; 3] = ["Fellowship", "Allegiance", "Friends"];
pub const FELLOWSHIP: usize = 0;
pub const ALLEGIANCE: usize = 1;
pub const FRIENDS: usize = 2;

/// The player's own states on the chat service, in the order the window lists them: the mode, its
/// words, and the emote played for it (by its name in the emotes' string table).
pub const STATES: [(u32, &str, &str); 3] = [
    (friend_mode::AVAILABLE, "Available", "ID_EmoteYawnCmd"),
    (friend_mode::BUSY, "Busy", "ID_EmoteYawnCmd"),
    (friend_mode::AWAY, "Away", "ID_EmoteAFKCmd"),
];

/// The text boxes' focus ids.
const NAME_BOX: u32 = 2;
const FRIEND_BOX: u32 = 3;

/// A row's height in the window's lists, in layout units.
pub const ROW: f32 = 26.0;
/// A fellow's row's height.
pub const FELLOW_ROW: f32 = 40.0;

/// Where the window's parts are, in a window drawn with `body` as its body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parts {
    pub tabs: Rect,
    /// Where the tab's contents go, between the tabs and the foot.
    pub page: Rect,
    /// The four buttons along the foot, left to right.
    pub foot: [Rect; 4],
    /// The text box above the foot (the fellowship's name, a friend's name).
    pub field: Rect,
}

/// The window's parts, at `k` pixels a layout unit.
#[must_use]
pub fn parts(body: Rect, k: f32) -> Parts {
    let w = (body.w - 34.0 * k) / 4.0;
    #[allow(clippy::cast_precision_loss)]
    let button = |i: usize| {
        Rect::new(
            body.x + 8.0 * k + i as f32 * (w + 6.0 * k),
            body.bottom() - 36.0 * k,
            w,
            28.0 * k,
        )
    };
    let field = Rect::new(
        body.x + 8.0 * k,
        body.bottom() - 74.0 * k,
        body.w - 16.0 * k,
        30.0 * k,
    );
    Parts {
        tabs: Rect::new(body.x, body.y, body.w, 26.0 * k),
        page: Rect::new(
            body.x + 8.0 * k,
            body.y + 36.0 * k,
            body.w - 16.0 * k,
            field.y - body.y - 44.0 * k,
        ),
        foot: [button(0), button(1), button(2), button(3)],
        field,
    }
}

/// Where the `i`th way of sharing loot is offered on the fellowship's page `page`.
#[must_use]
pub fn looting_rect(page: Rect, k: f32, i: usize) -> Rect {
    #[allow(clippy::cast_precision_loss)]
    let i = i as f32;
    Rect::new(
        page.x + 12.0 * k,
        page.y + (82.0 + i * 24.0) * k,
        page.w - 12.0 * k,
        22.0 * k,
    )
}

/// The face the fellowship's settings are written in: dim, at the body's size (smaller, the face
/// leaves almost no room after a "t").
#[must_use]
pub fn settings_style(colours: &dereth_ui_kit::colours::Colours) -> TextStyle {
    TextStyle::new(Family::Body, 13.0, colours.dim()).edge(colours.edge())
}

/// Where the party's `i`th fellow's row is on the fellowship's page `page`.
#[must_use]
pub fn fellow_rect(page: Rect, k: f32, i: usize) -> Rect {
    #[allow(clippy::cast_precision_loss)]
    let i = i as f32;
    Rect::new(
        page.x,
        page.y + (48.0 + i * FELLOW_ROW) * k,
        page.w,
        (FELLOW_ROW - 4.0) * k,
    )
}

/// Where a fellow's health and vigor gauges stand in its row `row`: under its name, sharing the
/// row's width.
#[must_use]
pub fn fellow_gauges(row: Rect, k: f32) -> (Rect, Rect) {
    let w = row.w - 20.0 * k;
    let health = Rect::new(row.x + 4.0 * k, row.y + 22.0 * k, w * 0.6, 9.0 * k);
    let vigor = Rect::new(
        health.right() + 12.0 * k,
        health.y,
        row.right() - 4.0 * k - health.right() - 12.0 * k,
        9.0 * k,
    );
    (health, vigor)
}

/// Where the `i`th vassal's row is on the allegiance's page `page`.
#[must_use]
pub fn vassal_rect(page: Rect, k: f32, i: usize) -> Rect {
    #[allow(clippy::cast_precision_loss)]
    let i = i as f32;
    Rect::new(
        page.x + 12.0 * k,
        page.y + (100.0 + i * ROW) * k,
        page.w - 12.0 * k,
        (ROW - 2.0) * k,
    )
}

/// Where the player's own states are offered on the friends' page `page`: their row.
#[must_use]
pub fn states_rect(page: Rect, k: f32) -> Rect {
    Rect::new(page.x, page.y, page.w, 22.0 * k)
}

/// Where each friend's row is on the friends' page `page`, those online first (under their
/// heading) and then those offline: each friend's place in `friends`, and its row.
#[must_use]
pub fn friend_rects(page: Rect, k: f32, friends: &[Friend]) -> Vec<(usize, Rect)> {
    let mut y = page.y + 32.0 * k;
    let mut out = Vec::new();
    for online in [true, false] {
        let listed: Vec<usize> = (0..friends.len())
            .filter(|i| friends[*i].online() == online)
            .collect();
        if listed.is_empty() {
            continue;
        }
        y += ROW * k;
        for i in listed {
            out.push((
                i,
                Rect::new(page.x + 8.0 * k, y, page.w - 8.0 * k, (ROW - 2.0) * k),
            ));
            y += ROW * k;
        }
    }
    out
}

/// The question the window asks before breaking with an allegiance's member: its id and name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Breaking {
    pub id: u64,
    pub name: String,
}

/// The social window.
#[derive(Debug)]
pub struct SocialWindow {
    window: WindowState,
    drawn: Option<(Rect, Rect)>,
    /// The tab shown.
    pub tab: usize,
    focus: u32,
    /// A new fellowship's name, whether it shares the experience, how it shares loot.
    pub fellowship_name: String,
    pub share_experience: bool,
    pub looting: u32,
    /// A friend's name typed, and the friend chosen in the list.
    pub friend_name: String,
    pub chosen: Option<String>,
    /// The player's own state as the window shows it, and the state last told.
    pub own_state: u32,
    told_state: Option<u32>,
    /// The emotes' string table, when the game's records say which it is.
    pub emote_table: Option<u32>,
    /// The allegiance's member shown (0 until the player is known), the one whose profile is
    /// asked for now (0: none), and the player's patron as the player's own profile named it.
    pub shown: u64,
    asking: u64,
    my_patron: u64,
    /// The question asked before a break, while it is up.
    pub breaking: Option<Breaking>,
    /// The last fellow's row pressed, and when.
    last_press: Option<(u64, f64)>,
}

impl Default for SocialWindow {
    fn default() -> Self {
        Self {
            window: WindowState::default(),
            drawn: None,
            tab: FELLOWSHIP,
            focus: 0,
            fellowship_name: String::new(),
            share_experience: false,
            looting: DEFAULT_LOOTING,
            friend_name: String::new(),
            chosen: None,
            own_state: friend_mode::AVAILABLE,
            told_state: None,
            emote_table: None,
            shown: 0,
            asking: 0,
            my_patron: 0,
            breaking: None,
            last_press: None,
        }
    }
}

/// A name as the game's requests carry it.
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

impl SocialWindow {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.window.open
    }

    /// Whether one of its text boxes has the keys.
    #[must_use]
    pub const fn typing(&self) -> bool {
        self.window.open && self.focus != 0
    }

    pub fn open(&mut self) {
        self.window.open = true;
    }

    pub fn toggle(&mut self, actions: &mut Vec<Action>) {
        if self.window.open {
            self.close(actions);
        } else {
            self.open();
        }
    }

    /// Open it on `tab`; already open on it, close it.
    pub fn show_tab(&mut self, tab: usize, actions: &mut Vec<Action>) {
        if self.window.open && self.tab == tab {
            self.close(actions);
        } else {
            self.tab = tab;
            self.open();
        }
    }

    /// Close it: the profiles no longer asked for, its question down.
    pub fn close(&mut self, actions: &mut Vec<Action>) {
        self.window.open = false;
        self.drawn = None;
        self.focus = 0;
        self.breaking = None;
        self.ask(0, actions);
    }

    /// Where it was last drawn, while it is open.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.drawn.map(|(r, _)| r)
    }

    /// Where its body was last drawn, while it is open.
    #[must_use]
    pub fn body(&self) -> Option<Rect> {
        self.drawn.map(|(_, b)| b)
    }

    /// Ask for the profile about `id` (0: no longer), when it is not what is asked for now.
    fn ask(&mut self, id: u64, actions: &mut Vec<Action>) {
        if id != self.asking {
            self.asking = id;
            actions.push(Action::Social(SocialAction::QueryAllegiance(id)));
        }
    }

    /// The player's own state set: told to the chat service, nowhere, and its emote played.
    fn set_state(&mut self, mode: u32, actions: &mut Vec<Action>) {
        self.own_state = mode;
        actions.push(Action::Social(SocialAction::SetState {
            mode,
            location: Vec::new(),
        }));
        let emote = STATES.iter().find(|(m, _, _)| *m == mode).map(|s| s.2);
        if let (Some(emote), Some(table)) = (emote, self.emote_table) {
            actions.push(Action::Emote {
                string: auberean_ui::token::Token::of(emote).raw(),
                table,
            });
        }
    }

    /// What the window keeps up each frame, shown or not: the player's own state as told; moving
    /// while away sets the player available; a friend added asks for the friends again; the
    /// profiles asked for only while the allegiance's tab shows.
    pub fn follow(&mut self, state: &GameState, actions: &mut Vec<Action>) {
        let s = &state.social;
        if s.own_state != self.told_state {
            self.told_state = s.own_state;
            if let Some(m) = s
                .own_state
                .filter(|m| STATES.iter().any(|(x, _, _)| x == m))
            {
                self.own_state = m;
            }
        }
        if !state.in_world {
            self.shown = 0;
            self.asking = 0;
            self.my_patron = 0;
            return;
        }
        if self.own_state == friend_mode::AWAY && s.moving {
            self.set_state(friend_mode::AVAILABLE, actions);
        }
        if s.friend_added {
            actions.push(Action::Social(SocialAction::GetFriends));
        }
        if self.shown == 0 {
            self.shown = state.player.unwrap_or(0);
        }
        if let Some(Allegiance {
            member: Some(m),
            patron,
            ..
        }) = &s.allegiance
        {
            if Some(m.id) == state.player {
                self.my_patron = patron.as_ref().map_or(0, |p| p.id);
            }
        }
        let want = if self.window.open && self.tab == ALLEGIANCE {
            self.shown
        } else {
            0
        };
        self.ask(want, actions);
    }

    /// Draw it while it is open; where it is.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        actions: &mut Vec<Action>,
    ) -> Option<Rect> {
        if !self.window.open {
            return None;
        }
        let k = p.scale;
        let size = kit::window_size_for(p, BODY);
        let at = (
            (p.screen.0 * 0.3 - size.0 * k / 2.0).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
        );
        let at = crate::ui::place::clear(p, state, size, at);
        let w = kit::window(p, ctx, &mut self.window, "Social", size, at);
        if w.closed {
            self.window.open = true;
            self.close(actions);
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        let parts = parts(w.body, k);
        let before = self.tab;
        kit::tabs(p, ctx, parts.tabs, &TABS, &mut self.tab);
        if self.tab != before {
            self.focus = 0;
            self.breaking = None;
            // The profile asked for at once as the tab shows, as the game's page asks.
            let want = if self.tab == ALLEGIANCE {
                self.shown
            } else {
                0
            };
            self.ask(want, actions);
        }
        match self.tab {
            FELLOWSHIP => self.fellowship(p, ctx, state, parts, actions),
            ALLEGIANCE => self.allegiance(p, ctx, state, parts, actions),
            _ => self.friends(p, ctx, state, parts, actions),
        }
        Some(w.rect)
    }

    fn styles(ctx: &Ctx<'_>) -> (TextStyle, TextStyle, TextStyle) {
        (
            TextStyle::new(Family::Heading, 15.0, ctx.colours.text()).edge(ctx.colours.edge()),
            TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge()),
            TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge()),
        )
    }

    fn fellowship(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        parts: Parts,
        actions: &mut Vec<Action>,
    ) {
        let k = p.scale;
        let (head, body, dim) = Self::styles(ctx);
        let page = parts.page;
        let Some(party) = &state.social.party else {
            p.text(&dim, page.x, page.y, "You are not in a fellowship.");
            let share = Rect::new(page.x, page.y + 28.0 * k, page.w, 22.0 * k);
            if let Some(on) = kit::check_row(
                p,
                ctx,
                share,
                self.share_experience,
                &body,
                "Share experience",
            ) {
                self.share_experience = on;
            }
            p.text(&body, page.x, page.y + 60.0 * k, "Looting Method");
            for (i, (words, _)) in LOOTING.iter().enumerate() {
                let r = looting_rect(page, k, i);
                let at = u32::try_from(i).unwrap_or(u32::MAX);
                let clicked = kit::radio(p, ctx, r, self.looting == at);
                p.text_in(
                    &body,
                    Rect::new(r.x + 26.0 * k, r.y, r.w - 26.0 * k, r.h),
                    Align::Left,
                    words,
                );
                if clicked || ctx.over_quiet(&r) && ctx.input.clicked(&r) {
                    self.looting = at;
                }
            }
            p.text(
                &dim,
                parts.field.x,
                parts.field.y - 18.0 * k,
                "Fellowship name",
            );
            let entered = kit::text_box(
                p,
                ctx,
                parts.field,
                &mut self.fellowship_name,
                &mut self.focus,
                NAME_BOX,
            );
            let can = state.player.is_some() && !self.fellowship_name.trim().is_empty();
            if (kit::button(p, ctx, parts.foot[3], "Create", can) || entered) && can {
                actions.push(Action::Social(SocialAction::CreateFellowship {
                    name: units(self.fellowship_name.trim()),
                    social: !self.share_experience,
                    looting: self.looting,
                }));
                self.fellowship_name.clear();
                self.focus = 0;
            }
            return;
        };
        let leads = state.player == Some(party.leader);
        let title = p.fit(&head, &party.name, page.w);
        p.text(&head, page.x, page.y, &title);
        let looting = usize::try_from(party.looting)
            .ok()
            .and_then(|i| LOOTING.get(i))
            .map_or("", |l| l.0);
        let settings = format!(
            "{}  ·  Loot: {looting}",
            if party.shares_experience {
                "Experience shared"
            } else {
                "Experience not shared"
            },
        );
        let style = settings_style(ctx.colours);
        let settings = p.fit(&style, &settings, page.w);
        p.text(&style, page.x, page.y + 22.0 * k, &settings);
        let selected = state.target.as_ref().map(|t| t.id);
        let number = TextStyle::new(Family::Numerals, 12.0, ctx.colours.dim());
        for (i, f) in crate::ui::party::rows(state).iter().enumerate() {
            let row = fellow_rect(page, k, i);
            let over = ctx.over(&row);
            kit::list_highlight(p, row, over || selected == Some(f.id));
            let lead = if f.id == party.leader {
                "  (Leader)"
            } else {
                ""
            };
            // A fellow's level is not known until the game tells it (0 till then).
            let mut misc = if f.level == 0 {
                String::new()
            } else {
                format!("Level {}", f.level)
            };
            if let Some(share) = f.share {
                if !misc.is_empty() {
                    misc.push_str("  ·  ");
                }
                misc.push_str(&format!(
                    "{}%",
                    dereth_primitives::num::to_i32((share * 100.0).round())
                ));
            }
            let mw = p.measure(&number, &misc);
            p.text(&number, row.right() - mw - 4.0 * k, row.y + 2.0 * k, &misc);
            let name = p.fit(&body, &format!("{}{lead}", f.name), row.w - mw - 16.0 * k);
            p.text(&body, row.x + 4.0 * k, row.y + 2.0 * k, &name);
            let (health, vigor) = fellow_gauges(row, k);
            kit::gauge(p, health, f.health.ratio(), HEALTH);
            kit::gauge(p, vigor, f.vigor.ratio(), VIGOR);
            if over && ctx.input.clicked(&row) {
                actions.push(Action::SetSelection { target: f.id });
                let again = self
                    .last_press
                    .is_some_and(|(id, at)| id == f.id && ctx.time - at <= DOUBLE_PRESS);
                if again {
                    actions.push(Action::AssistedTell {
                        name: f.name.clone(),
                    });
                    self.last_press = None;
                } else {
                    self.last_press = Some((f.id, ctx.time));
                }
            }
        }
        let picked = selected.filter(|id| *id != 0);
        if kit::button(p, ctx, parts.foot[0], "Recruit", leads && picked.is_some()) && leads {
            if let Some(id) = picked {
                actions.push(Action::Social(SocialAction::Recruit(id)));
            }
        }
        if kit::button(p, ctx, parts.foot[1], "Dismiss", leads && picked.is_some()) && leads {
            if let Some(id) = picked {
                actions.push(Action::Social(SocialAction::Dismiss(id)));
            }
        }
        if kit::button(p, ctx, parts.foot[2], "Quit", true) {
            actions.push(Action::Social(SocialAction::LeaveFellowship));
        }
        if kit::button(p, ctx, parts.foot[3], "Disband", leads) && leads {
            actions.push(Action::Social(SocialAction::DisbandFellowship));
        }
    }

    #[allow(clippy::too_many_lines)]
    fn allegiance(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        parts: Parts,
        actions: &mut Vec<Action>,
    ) {
        let k = p.scale;
        let (head, body, dim) = Self::styles(ctx);
        let page = parts.page;
        let player = state.player.unwrap_or(0);
        let picked = state
            .target
            .as_ref()
            .filter(|t| t.id != player && is_player_id(t.id))
            .map(|t| t.id);
        let a = state.social.allegiance.as_ref();
        let member = a.and_then(|a| a.member.as_ref());
        let in_one = a.is_some_and(|a| a.member.is_some());
        let showing = member.is_some_and(|m| m.id == self.shown);
        // The question before a break, in place of the page.
        if let Some(b) = self.breaking.clone() {
            let line = p.line_height(&body);
            let question = format!("Break your allegiance with {}?", b.name);
            for (i, l) in p.wrap(&body, &question, page.w).iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                p.text(&body, page.x, page.y + i as f32 * line, l);
            }
            if kit::button(p, ctx, parts.foot[2], "Yes", true) {
                self.breaking = None;
                actions.push(Action::Social(SocialAction::Break(b.id)));
                let next = match a {
                    Some(a) if a.vassal(player) => player,
                    Some(a) => a.patron.as_ref().map_or(player, |r| r.id),
                    None => player,
                };
                self.shown = next;
                self.ask(next, actions);
            }
            if kit::button(p, ctx, parts.foot[3], "No", true) {
                self.breaking = None;
            }
            return;
        }
        match a {
            None => {
                p.text(&dim, page.x, page.y, "Waiting for the allegiance...");
            }
            Some(a) if a.member.is_none() => {
                p.text(&dim, page.x, page.y, "You are not in an allegiance.");
            }
            Some(_) if !showing => {
                p.text(&dim, page.x, page.y, "Waiting for the allegiance...");
            }
            Some(a) => {
                let m = a.member.as_ref().unwrap_or_else(|| unreachable!());
                let name = p.fit(&head, &a.name, page.w);
                p.text(&head, page.x, page.y, &name);
                let monarch = a.monarch.as_ref().map_or("", |r| r.name.as_str());
                let size = p.fit(
                    &dim,
                    &format!("Size: {}  ·  Monarch: {monarch}", a.total),
                    page.w,
                );
                p.text(&dim, page.x, page.y + 22.0 * k, &size);
                let mut y = page.y + 50.0 * k;
                let titled = |r: &crate::social::Rank| {
                    if r.title.is_empty() {
                        r.name.clone()
                    } else {
                        format!("{} {}", r.title, r.name)
                    }
                };
                let online = |r: &crate::social::Rank| {
                    if r.id == player {
                        ""
                    } else if r.online {
                        "  (online)"
                    } else {
                        "  (offline)"
                    }
                };
                let level = if m.level == 0 {
                    String::new()
                } else {
                    format!(", Level {}", m.level)
                };
                let member = p.fit(&body, &format!("{}{level}{}", titled(m), online(m)), page.w);
                p.text(&body, page.x, y, &member);
                y += 22.0 * k;
                let patron = a.patron.as_ref().map_or_else(
                    || "Patron: None".to_owned(),
                    |r| format!("Patron: {}", r.name),
                );
                let patron = p.fit(&dim, &patron, page.w - 12.0 * k);
                p.text(&dim, page.x + 12.0 * k, y, &patron);
                y += 28.0 * k;
                if a.vassals.is_empty() {
                    p.text(&dim, page.x + 12.0 * k, y, "No vassals");
                    y += ROW * k;
                }
                for (i, v) in a.vassals.iter().enumerate() {
                    let row = vassal_rect(page, k, i);
                    let over = ctx.over(&row);
                    kit::list_highlight(p, row, over);
                    let passed = v.experience.to_string();
                    let pw = p.measure(&dim, &passed);
                    let name = p.fit(
                        &body,
                        &format!("{}{}", titled(v), online(v)),
                        row.w - pw - 16.0 * k,
                    );
                    p.text_in(
                        &body,
                        Rect::new(row.x + 4.0 * k, row.y, row.w - 8.0 * k, row.h),
                        Align::Left,
                        &name,
                    );
                    p.text(&dim, row.right() - pw - 4.0 * k, row.y + 4.0 * k, &passed);
                    if over && ctx.input.clicked(&row) {
                        self.shown = v.id;
                    }
                    y += ROW * k;
                }
                y += 8.0 * k;
                let message = if a.message.is_empty() {
                    "There is no message of the day.".to_owned()
                } else {
                    format!("Message of the day: {}", a.message)
                };
                let line = p.line_height(&dim);
                for l in p.wrap(&dim, &message, page.w) {
                    if y + line > page.bottom() {
                        break;
                    }
                    p.text(&dim, page.x, y, &l);
                    y += line;
                }
            }
        }
        let a = state.social.allegiance.as_ref().filter(|_| showing);
        let cur_patron = a.and_then(|a| a.patron.as_ref()).map_or(0, |r| r.id);
        if kit::button(p, ctx, parts.foot[0], "Home", self.shown != player) {
            self.shown = player;
        }
        if kit::button(p, ctx, parts.foot[1], "Up", cur_patron != 0) && cur_patron != 0 {
            self.shown = cur_patron;
        }
        let can_swear = self.my_patron == 0 && picked.is_some() && player != 0;
        if kit::button(p, ctx, parts.foot[2], "Swear", can_swear) && can_swear {
            actions.push(Action::Social(SocialAction::Swear));
        }
        let officer = a.is_some_and(|a| a.officers.contains(&player));
        let can_break = in_one
            && showing
            && self.shown != player
            && self.shown != 0
            && (self.shown == self.my_patron || cur_patron == player || officer);
        if kit::button(p, ctx, parts.foot[3], "Break", can_break) && can_break {
            let name = member.map(|m| m.name.clone()).unwrap_or_default();
            self.breaking = Some(Breaking {
                id: self.shown,
                name,
            });
        }
        // The member shown changed: its profile asked for at once.
        if self.window.open && self.tab == ALLEGIANCE {
            self.ask(self.shown, actions);
        }
    }

    fn friends(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        parts: Parts,
        actions: &mut Vec<Action>,
    ) {
        let k = p.scale;
        let (head, body, dim) = Self::styles(ctx);
        let page = parts.page;
        let s: &Social = &state.social;
        let names: Vec<&str> = STATES.iter().map(|(_, w, _)| *w).collect();
        let at = STATES.iter().position(|(m, _, _)| *m == self.own_state);
        let row = states_rect(page, k);
        if let Some(i) = kit::radios(p, ctx, row, &names, at) {
            if Some(i) != at && state.player.is_some() {
                self.set_state(STATES[i].0, actions);
            }
        }
        if self
            .chosen
            .as_ref()
            .is_some_and(|c| !s.friends.iter().any(|f| f.name == *c))
        {
            self.chosen = None;
        }
        let rows = friend_rects(page, k, &s.friends);
        let mut last_online = None;
        for (i, r) in rows {
            let f = &s.friends[i];
            let online = f.online();
            if last_online != Some(online) {
                last_online = Some(online);
                let heading = if online { "Online" } else { "Offline" };
                p.text(&head, page.x, r.y - ROW * k, heading);
            }
            {
                let over = ctx.over(&r);
                let chosen = self.chosen.as_deref() == Some(f.name.as_str());
                kit::list_highlight(p, r, over || chosen);
                let words = match f.state_word() {
                    Some(w) => format!("{} ({w})", f.name),
                    None => f.name.clone(),
                };
                let style = if online { &body } else { &dim };
                let words = p.fit(style, &words, r.w - 8.0 * k);
                p.text_in(
                    style,
                    Rect::new(r.x + 4.0 * k, r.y, r.w - 8.0 * k, r.h),
                    Align::Left,
                    &words,
                );
                if over && ctx.input.clicked(&r) {
                    self.chosen = (!chosen).then(|| f.name.clone());
                }
            }
        }
        if s.friends.is_empty() {
            p.text(&dim, page.x, page.y + 32.0 * k, "No friends yet.");
        }
        p.text(&dim, parts.field.x, parts.field.y - 18.0 * k, "Name");
        let entered = kit::text_box(
            p,
            ctx,
            parts.field,
            &mut self.friend_name,
            &mut self.focus,
            FRIEND_BOX,
        );
        let player = state.player.unwrap_or(0);
        let typed = self.friend_name.trim().to_owned();
        let selected = state
            .target
            .as_ref()
            .filter(|t| t.id != player && is_player_id(t.id) && !t.name.is_empty())
            .map(|t| t.name.clone())
            .filter(|n| !s.friends.iter().any(|f| f.name == *n));
        let adding = if typed.is_empty() {
            selected
        } else {
            Some(typed)
        };
        let can_add = player != 0 && adding.is_some();
        if (kit::button(p, ctx, parts.foot[0], "Add", can_add) || entered) && can_add {
            if let Some(name) = adding {
                actions.push(Action::Social(SocialAction::AddFriend(units(&name))));
            }
            self.friend_name.clear();
            self.focus = 0;
        }
        let chosen = self.chosen.clone();
        if kit::button(p, ctx, parts.foot[1], "Remove", chosen.is_some()) {
            if let Some(name) = chosen.clone() {
                actions.push(Action::Social(SocialAction::RemoveFriend(units(&name))));
            }
        }
        let online = chosen
            .as_ref()
            .filter(|c| s.friends.iter().any(|f| f.name == **c && f.online()));
        if kit::button(p, ctx, parts.foot[2], "Tell", online.is_some()) {
            if let Some(name) = online {
                actions.push(Action::AssistedTell { name: name.clone() });
            }
        }
    }
}
