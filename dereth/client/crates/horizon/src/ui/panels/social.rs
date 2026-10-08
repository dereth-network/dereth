//! The social windows, doing what the game's fellowship, friends and allegiance panels do: the
//! fellowship's members and its management, the friends list with adding, removing and telling,
//! the blacklist (the game's squelch list) with squelching a character or an account and taking a
//! squelch off, and the allegiance with swearing, breaking and releasing (each after the game's
//! own question).
//! While a window is open the game is asked for its live updates, as its panels ask.

use dereth_client_contract::view::{AllegianceAction, PlayerOption};
use dereth_client_contract::UiRequest;
use dereth_primitives::ObjectId;

use super::{WindowId, Windows};
use crate::art::Family;
use crate::draw::Rect;
use crate::ui::game::{GameState, Relation};
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The most a fellowship holds.
const FELLOWSHIP_FULL: usize = 9;

/// How tall a one-line list row is, in layout units: room round its name.
const ROW_H: f32 = 30.0;

/// How tall a text box is, in layout units.
const FIELD_H: f32 = 34.0;

/// How tall a button along a window's foot is, in layout units.
const BUTTON_H: f32 = 30.0;

impl Windows {
    /// While the social or allegiance window is open, the game keeps its fellowship or
    /// allegiance up to date; when it closes, it stops.
    pub(super) fn follow_social(&mut self, out: &mut Outcome) {
        let party = self.is_open(WindowId::Social) && self.social_tab == 0;
        if party != self.fellowship_updates {
            self.fellowship_updates = party;
            out.requests
                .push(UiRequest::FellowshipUpdateRequest { on: party });
        }
        let allegiance = self.is_open(WindowId::Allegiance);
        if allegiance != self.allegiance_updates {
            self.allegiance_updates = allegiance;
            out.requests
                .push(UiRequest::AllegianceUpdateRequest { on: allegiance });
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn social(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        kit::tabs(
            p,
            ctx,
            Rect::new(body.x, body.y, body.w, 26.0 * k),
            &["Fellowship", "Friends", "Squelch"],
            &mut self.social_tab,
        );
        if self.social_tab == 2 {
            self.blacklist(p, ctx, state, body, out);
            return;
        }
        let name = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let top = body.y + 36.0 * k;
        let foot = body.bottom() - 30.0 * k;
        let bw = 96.0 * k;
        let selected_player = state
            .target
            .as_ref()
            .filter(|t| t.relation == Relation::Player)
            .map(|t| (t.id, t.name.clone()));
        if self.social_tab == 0 {
            let Some(f) = state.fellowship_view.as_ref() else {
                // No fellowship: make one.
                p.text(&dim, body.x + 8.0 * k, top, "You are not in a fellowship.");
                p.text(&dim, body.x + 8.0 * k, top + 30.0 * k, "Fellowship name");
                let field = Rect::new(
                    body.x + 8.0 * k,
                    top + 48.0 * k,
                    body.w - 16.0 * k - bw - 8.0 * k,
                    FIELD_H * k,
                );
                let submitted = kit::text_box(
                    p,
                    ctx,
                    field,
                    &mut self.fellowship_name,
                    &mut self.typing_field,
                    1,
                );
                let create = Rect::new(field.right() + 8.0 * k, field.y, bw, field.h);
                let can = !self.fellowship_name.trim().is_empty();
                if (kit::button(p, ctx, create, "Create", can) || submitted) && can {
                    out.requests.push(UiRequest::FellowshipCreate {
                        name: self.fellowship_name.trim().to_owned(),
                        share_xp: state.option(PlayerOption::FellowshipShareXP),
                    });
                    self.fellowship_name.clear();
                }
                self.fellowship_options(
                    p,
                    ctx,
                    state,
                    Rect::new(body.x, top + 96.0 * k, body.w, 120.0 * k),
                    out,
                );
                return;
            };
            let me = state.player_id;
            let leader = me == Some(f.leader);
            p.text(&name, body.x + 8.0 * k, top, &f.name);
            p.text_in(
                &dim,
                Rect::new(body.x, top, body.w - 8.0 * k, 18.0 * k),
                Align::Right,
                &format!(
                    "{} of {FELLOWSHIP_FULL}{}",
                    f.members.len(),
                    if f.open_fellow { "  ·  open" } else { "" }
                ),
            );
            let mut y = top + 26.0 * k;
            for m in &f.members {
                let row = Rect::new(body.x, y, body.w, 44.0 * k);
                let on = self.fellow_selected == Some(m.id);
                if on || ctx.input.hover(&row) {
                    crate::ui::pregame::list_highlight(p, row, on);
                }
                let mark = if m.id == f.leader { "  (leader)" } else { "" };
                p.text_in(
                    &name,
                    Rect::new(row.x + 8.0 * k, y, row.w * 0.5, 24.0 * k),
                    Align::Left,
                    &format!("{}{mark}", m.name),
                );
                p.text_in(
                    &dim,
                    Rect::new(row.x + 8.0 * k, y + 24.0 * k, row.w * 0.5, 20.0 * k),
                    Align::Left,
                    &format!("Level {}  ·  {}% of shared XP", m.level, m.xp_percent),
                );
                let gx = row.x + row.w * 0.52;
                let gw = row.w * 0.44;
                for (i, (cur, max, colour)) in [
                    (m.current_health, m.max_health, 0xFF6E_CB5C),
                    (m.current_stamina, m.max_stamina, 0xFFE8_B84A),
                    (m.current_mana, m.max_mana, 0xFFD8_6CC8),
                ]
                .into_iter()
                .enumerate()
                {
                    #[allow(clippy::cast_precision_loss)]
                    let fill = if max == 0 {
                        0.0
                    } else {
                        cur as f32 / max as f32
                    };
                    #[allow(clippy::cast_precision_loss)]
                    kit::gauge(
                        p,
                        Rect::new(gx, y + 6.0 * k + i as f32 * 12.0 * k, gw, 8.0 * k),
                        fill,
                        colour,
                    );
                }
                if ctx.over(&row) && ctx.input.clicked(&row) {
                    self.fellow_selected = Some(m.id);
                    out.requests.push(UiRequest::Select(m.id));
                }
                y += 46.0 * k;
            }
            let fellow = self
                .fellow_selected
                .filter(|id| f.members.iter().any(|m| m.id == *id) && Some(*id) != me);
            let recruit_target = selected_player
                .as_ref()
                .map(|(id, _)| *id)
                .filter(|id| !f.members.iter().any(|m| m.id == *id));
            let full = f.members.len() >= FELLOWSHIP_FULL;
            let buttons: [(&str, bool, Option<UiRequest>); 6] = [
                (
                    "Recruit",
                    recruit_target.is_some() && !full && (leader || f.open_fellow),
                    recruit_target.map(|target| UiRequest::FellowshipRecruit { target }),
                ),
                (
                    "Dismiss",
                    leader && fellow.is_some(),
                    fellow.map(|target| UiRequest::FellowshipDismiss { target }),
                ),
                (
                    "Make Leader",
                    leader && fellow.is_some(),
                    fellow.map(|target| UiRequest::FellowshipAssignNewLeader { target }),
                ),
                (
                    if f.open_fellow { "Close" } else { "Open" },
                    leader,
                    Some(UiRequest::FellowshipToggleOpenness),
                ),
                (
                    "Quit",
                    true,
                    Some(UiRequest::FellowshipQuit { disband: false }),
                ),
                (
                    "Disband",
                    leader,
                    Some(UiRequest::FellowshipQuit { disband: true }),
                ),
            ];
            let bw = (body.w - 5.0 * 6.0 * k) / 6.0;
            for (i, (label, enabled, request)) in buttons.into_iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let r = Rect::new(body.x + i as f32 * (bw + 6.0 * k), foot, bw, BUTTON_H * k);
                if kit::button(p, ctx, r, label, enabled) {
                    out.requests.extend(request);
                }
            }
            if recruit_target.is_none()
                && ctx.input.hover(&Rect::new(body.x, foot, bw, BUTTON_H * k))
            {
                self.tip = Some(("Recruit".into(), vec!["Select a player to invite.".into()]));
            }
            self.fellowship_options(
                p,
                ctx,
                state,
                Rect::new(body.x, foot - 70.0 * k, body.w, 60.0 * k),
                out,
            );
        } else {
            // Friends: online first, then by name, as the game orders them.
            let mut friends: Vec<_> = state.friend_list.iter().collect();
            friends.sort_by(|a, b| b.online.cmp(&a.online).then(a.name.cmp(&b.name)));
            let field = Rect::new(
                body.x,
                foot - (FIELD_H + 10.0) * k,
                body.w - bw - 8.0 * k,
                FIELD_H * k,
            );
            let list = Rect::new(body.x, top, body.w, field.y - 8.0 * k - top);
            p.list.push_clip(list);
            let mut y = list.y;
            for f in &friends {
                let row = Rect::new(list.x, y, list.w, ROW_H * k);
                let on = self.friend_selected == Some(f.id);
                if on || ctx.input.hover(&row) {
                    crate::ui::pregame::list_highlight(p, row, on);
                }
                let dot = if f.online { 0xFF6E_CB5C } else { 0xFF60_6060 };
                p.fill(
                    Rect::new(
                        row.x + 8.0 * k,
                        (y + (row.h - 8.0 * k) / 2.0).round(),
                        8.0 * k,
                        8.0 * k,
                    ),
                    dot,
                );
                p.text_in(
                    &name,
                    Rect::new(row.x + 24.0 * k, y, row.w - 32.0 * k, row.h),
                    Align::Left,
                    &f.name,
                );
                if ctx.over(&row) && ctx.input.clicked(&row) {
                    self.friend_selected = Some(f.id);
                }
                y += row.h + 2.0 * k;
            }
            if friends.is_empty() {
                p.text(&dim, list.x + 8.0 * k, list.y, "No friends yet.");
            }
            p.list.pop_clip();
            let chosen = self
                .friend_selected
                .and_then(|id| state.friend_list.iter().find(|f| f.id == id));
            let submitted = kit::text_box(
                p,
                ctx,
                field,
                &mut self.friend_name,
                &mut self.typing_field,
                2,
            );
            let full = state.friend_list.len() >= 100;
            let can = !self.friend_name.trim().is_empty() && !full;
            if (kit::button(
                p,
                ctx,
                Rect::new(field.right() + 8.0 * k, field.y, bw, field.h),
                "Add",
                can,
            ) || submitted)
                && can
            {
                out.requests.push(UiRequest::AddFriend {
                    name: self.friend_name.trim().to_owned(),
                });
                self.friend_name.clear();
            }
            let appear_offline = state.option(PlayerOption::AppearOffline);
            let cb = Rect::new(body.x, foot, body.w - 2.0 * bw - 16.0 * k, BUTTON_H * k);
            if let Some(on) = kit::check_row(p, ctx, cb, appear_offline, &dim, "Appear offline") {
                out.requests
                    .push(UiRequest::SetPlayerOption(PlayerOption::AppearOffline, on));
            }
            if kit::button(
                p,
                ctx,
                Rect::new(body.right() - 2.0 * bw - 8.0 * k, foot, bw, BUTTON_H * k),
                "Tell",
                chosen.is_some_and(|f| f.online),
            ) {
                if let Some(f) = chosen {
                    out.requests.push(UiRequest::ChatEntry {
                        window: dereth_client_contract::chat::interface::window::MAIN,
                        text: String::new(),
                        action: dereth_client_contract::chat::entry::EntryAction::StartTell {
                            name: f.name.clone(),
                        },
                    });
                }
            }
            if kit::button(
                p,
                ctx,
                Rect::new(body.right() - bw, foot, bw, BUTTON_H * k),
                "Remove",
                chosen.is_some(),
            ) {
                if let Some(f) = chosen {
                    out.requests.push(UiRequest::RemoveFriend { target: f.id });
                    self.friend_selected = None;
                }
            }
        }
    }

    /// The blacklist: the game's squelch list, one alphabetical list of names, each a character's
    /// or an account's; a typed name squelched as a character or as an account, and the chosen
    /// row's squelch taken off, as the game's squelch panel does.
    fn blacklist(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let name = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let top = body.y + 36.0 * k;
        let foot = body.bottom() - 30.0 * k;
        let mut rows: Vec<&dereth_client_contract::view::SquelchEntry> =
            state.squelches.iter().collect();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        if self.squelch_selected.is_some_and(|i| i >= rows.len()) {
            self.squelch_selected = None;
        }
        let field = Rect::new(body.x, foot - (FIELD_H + 10.0) * k, body.w, FIELD_H * k);
        let row_h = ROW_H * k;
        if rows.is_empty() {
            p.text(&dim, body.x + 8.0 * k, top, "No one is squelched.");
        }
        for (i, e) in rows.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(body.x, top + (row_h + 2.0 * k) * i as f32, body.w, row_h);
            if r.bottom() > field.y - 8.0 * k {
                break;
            }
            let on = self.squelch_selected == Some(i);
            if on || ctx.input.hover(&r) {
                crate::ui::pregame::list_highlight(p, r, on);
            }
            p.text_in(
                &name,
                Rect::new(r.x + 8.0 * k, r.y, r.w * 0.6, r.h),
                Align::Left,
                &e.name,
            );
            p.text_in(
                &dim,
                Rect::new(r.x, r.y, r.w - 8.0 * k, r.h),
                Align::Right,
                if e.account { "Account" } else { "Character" },
            );
            if ctx.over(&r) && ctx.input.clicked(&r) {
                self.squelch_selected = Some(i);
            }
        }
        kit::text_box(
            p,
            ctx,
            field,
            &mut self.squelch_name,
            &mut self.typing_field,
            31,
        );
        let typed = self.squelch_name.trim().to_owned();
        // Each button as wide as its name needs, the two squelches alike.
        let label = TextStyle::new(Family::Body, 14.0, 0xFFFF_FFFF);
        let bw = ["Squelch Character", "Squelch Account"]
            .iter()
            .map(|l| p.measure(&label, l) + 36.0 * k)
            .fold(130.0 * k, f32::max);
        if kit::button(
            p,
            ctx,
            Rect::new(body.x, foot, bw, BUTTON_H * k),
            "Squelch Character",
            !typed.is_empty(),
        ) {
            out.requests.push(UiRequest::ModifyCharacterSquelch {
                object: ObjectId(0),
                add: true,
                account: typed.clone(),
                message_type: dereth_client_model::chat::text_type::ALL_CHANNELS,
            });
            self.squelch_name.clear();
        }
        if kit::button(
            p,
            ctx,
            Rect::new(body.x + bw + 8.0 * k, foot, bw, BUTTON_H * k),
            "Squelch Account",
            !typed.is_empty(),
        ) {
            out.requests.push(UiRequest::ModifyAccountSquelch {
                add: true,
                name: typed,
            });
            self.squelch_name.clear();
        }
        let chosen = self.squelch_selected.and_then(|i| rows.get(i));
        let remove_w = 110.0 * k;
        if kit::button(
            p,
            ctx,
            Rect::new(body.right() - remove_w, foot, remove_w, BUTTON_H * k),
            "Remove",
            chosen.is_some(),
        ) {
            if let Some(e) = chosen {
                out.requests.push(squelch_removal(e));
                self.squelch_selected = None;
            }
        }
    }

    /// The four fellowship switches, the player's own options.
    fn fellowship_options(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let options = [
            (PlayerOption::FellowshipShareXP, "Share experience"),
            (PlayerOption::FellowshipShareLoot, "Share loot"),
            (
                PlayerOption::FellowshipAutoAcceptRequests,
                "Accept All Invitations",
            ),
            (PlayerOption::IgnoreFellowshipRequests, "Ignore invitations"),
        ];
        for (i, (o, label)) in options.into_iter().enumerate() {
            let (col, row) = (i % 2, i / 2);
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                area.x + col as f32 * area.w / 2.0,
                area.y + row as f32 * 28.0 * k,
                area.w / 2.0 - 8.0 * k,
                24.0 * k,
            );
            if let Some(on) = kit::check_row(p, ctx, r, state.option(o), &dim, label) {
                out.requests.push(UiRequest::SetPlayerOption(o, on));
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn allegiance(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let a = &state.roster;
        let head =
            TextStyle::new(Family::Heading, 24.0, ctx.colours.heading()).edge(ctx.colours.edge());
        let name = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let mut y = body.y + 4.0 * k;
        let title = if a.allegiance_name.is_empty() {
            "Allegiance"
        } else {
            a.allegiance_name.as_str()
        };
        p.text(&head, body.x + 8.0 * k, y, title);
        y += 32.0 * k;
        if let Some(me) = &a.subject {
            p.text(
                &dim,
                body.x + 8.0 * k,
                y,
                &format!(
                    "Rank {}  ·  {} followers  ·  {} XP passed up",
                    me.rank,
                    a.total_vassals,
                    crate::ui::hud::grouped(u64::from(a.own_cp_tithed))
                ),
            );
            y += 22.0 * k;
        }
        let online = |on: bool| if on { 0xFF6E_CB5C } else { 0xFF60_6060 };
        let person = |p: &mut Painter<'_>,
                      ctx: &mut Ctx<'_>,
                      y: f32,
                      label: &str,
                      e: &dereth_client_contract::view::AllegianceEntry,
                      sel: &mut Option<ObjectId>| {
            let row = Rect::new(body.x, y, body.w, 26.0 * k);
            let on = *sel == Some(e.id);
            if on || ctx.input.hover(&row) {
                crate::ui::pregame::list_highlight(p, row, on);
            }
            p.fill(
                Rect::new(row.x + 8.0 * k, y + 9.0 * k, 8.0 * k, 8.0 * k),
                online(e.logged_in),
            );
            p.text_in(
                &dim,
                Rect::new(row.x + 24.0 * k, y, 80.0 * k, row.h),
                Align::Left,
                label,
            );
            p.text_in(
                &name,
                Rect::new(row.x + 110.0 * k, y, row.w - 120.0 * k, row.h),
                Align::Left,
                &e.full_name,
            );
            p.text_in(
                &dim,
                row.offset(-8.0 * k, 0.0),
                Align::Right,
                &format!("rank {}", e.rank),
            );
            if ctx.over(&row) && ctx.input.clicked(&row) {
                *sel = Some(e.id);
            }
        };
        let mut selected = self.allegiance_selected;
        if let Some(m) = &a.monarch {
            person(p, ctx, y, "Monarch", m, &mut selected);
            y += 28.0 * k;
        }
        if let Some(pa) = a
            .patron
            .as_ref()
            .filter(|pa| a.monarch.as_ref().is_none_or(|m| m.id != pa.id))
        {
            person(p, ctx, y, "Patron", pa, &mut selected);
            y += 28.0 * k;
        }
        y += 8.0 * k;
        p.text(
            &dim,
            body.x + 8.0 * k,
            y,
            &format!("Vassals ({})", a.vassals.len()),
        );
        y += 20.0 * k;
        let foot = body.bottom() - 30.0 * k;
        let list = Rect::new(body.x, y, body.w, foot - y - 8.0 * k);
        p.list.push_clip(list);
        for v in &a.vassals {
            let row = Rect::new(list.x, y, list.w, 26.0 * k);
            let on = selected == Some(v.id);
            if on || ctx.input.hover(&row) {
                crate::ui::pregame::list_highlight(p, row, on);
            }
            p.fill(
                Rect::new(row.x + 8.0 * k, y + 9.0 * k, 8.0 * k, 8.0 * k),
                online(v.logged_in),
            );
            p.text_in(
                &name,
                Rect::new(row.x + 24.0 * k, y, row.w - 32.0 * k, row.h),
                Align::Left,
                &v.full_name,
            );
            p.text_in(
                &dim,
                row.offset(-8.0 * k, 0.0),
                Align::Right,
                &format!(
                    "{} XP passed up",
                    crate::ui::hud::grouped(u64::from(v.cp_cached))
                ),
            );
            if ctx.over(&row) && ctx.input.clicked(&row) {
                selected = Some(v.id);
            }
            y += 28.0 * k;
        }
        if a.vassals.is_empty() {
            p.text(&dim, list.x + 8.0 * k, list.y, "None.");
        }
        p.list.pop_clip();
        self.allegiance_selected = selected;
        // Swear to the selected player, break from the patron, release the selected vassal:
        // each after the game's own question.
        let swear_to = state
            .target
            .as_ref()
            .filter(|t| t.relation == Relation::Player)
            .filter(|t| a.patron.as_ref().is_none_or(|pa| pa.id != t.id))
            .map(|t| (t.id, t.name.clone()));
        let vassal = selected.and_then(|id| a.vassals.iter().find(|v| v.id == id));
        let bw = (body.w - 12.0 * k) / 3.0;
        let swear = kit::button(
            p,
            ctx,
            Rect::new(body.x, foot, bw, 28.0 * k),
            "Swear",
            swear_to.is_some() && a.patron.is_none(),
        );
        let brk = kit::button(
            p,
            ctx,
            Rect::new(body.x + bw + 6.0 * k, foot, bw, 28.0 * k),
            "Break",
            a.patron.is_some(),
        );
        let release = kit::button(
            p,
            ctx,
            Rect::new(body.x + 2.0 * (bw + 6.0 * k), foot, bw, 28.0 * k),
            "Release",
            vassal.is_some(),
        );
        use dereth_ui_screens::panels::allegiance as retail;
        if swear {
            if let Some((id, n)) = &swear_to {
                self.ask_allegiance(
                    out,
                    AllegianceAction::Swear,
                    *id,
                    retail::ID_SWEAR_CONFIRMATION,
                    n,
                );
            }
        }
        if brk {
            if let Some(pa) = &a.patron {
                self.ask_allegiance(
                    out,
                    AllegianceAction::Break,
                    pa.id,
                    retail::ID_BREAK_CONFIRMATION,
                    &pa.full_name,
                );
            }
        }
        if release {
            if let Some(v) = vassal {
                self.ask_allegiance(
                    out,
                    AllegianceAction::Kick,
                    v.id,
                    retail::ID_KICK_CONFIRMATION,
                    &v.full_name,
                );
            }
        }
        if ctx.input.hover(&Rect::new(body.x, foot, bw, 28.0 * k)) && swear_to.is_none() {
            self.tip = Some((
                "Swear".into(),
                vec!["Select the player to swear allegiance to.".into()],
            ));
        }
    }

    /// Ask the game's question before an allegiance action: its words filled with the other
    /// player's name, found by the shell in the game's string table.
    fn ask_allegiance(
        &mut self,
        out: &mut Outcome,
        action: AllegianceAction,
        target: ObjectId,
        token: &'static str,
        who: &str,
    ) {
        out.allegiance_question = Some((action, target, token, who.to_owned()));
    }
}

/// What taking a squelch off sends: an account's by its account name, a character's by its
/// name, for every kind of message, as the game's squelch panel sends it.
#[must_use]
pub fn squelch_removal(entry: &dereth_client_contract::view::SquelchEntry) -> UiRequest {
    if entry.account {
        UiRequest::ModifyAccountSquelch {
            add: false,
            name: entry.name.clone(),
        }
    } else {
        UiRequest::ModifyCharacterSquelch {
            object: ObjectId(0),
            add: false,
            account: entry.name.clone(),
            message_type: dereth_client_model::chat::text_type::ALL_CHANNELS,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    #[test]
    fn an_account_squelch_comes_off_by_account_and_a_character_s_by_name_for_every_message() {
        let account = dereth_client_contract::view::SquelchEntry {
            name: "someone".into(),
            account: true,
        };
        assert_eq!(
            squelch_removal(&account),
            UiRequest::ModifyAccountSquelch {
                add: false,
                name: "someone".into()
            }
        );
        let character = dereth_client_contract::view::SquelchEntry {
            name: "Borin".into(),
            account: false,
        };
        assert_eq!(
            squelch_removal(&character),
            UiRequest::ModifyCharacterSquelch {
                object: ObjectId(0),
                add: false,
                account: "Borin".into(),
                message_type: 1,
            }
        );
    }
}
