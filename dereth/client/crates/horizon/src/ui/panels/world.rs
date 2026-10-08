//! The world's services, in the Horizon interface's windows: buying and maintaining a dwelling at its
//! deed (the housing window), the player's own house (the house window), chess (the Game
//! Center), the barber, and spell research (the Create Spell tab of the Spellbook). Each does
//! what the game's own panel does, through the game's requests and the shared rules: the payment
//! lists and the board are the runtime's, the house rows and the barber's appearance rules are
//! the retail panels' pure helpers, and the house payment and resign questions are the shared
//! dialog service's.
//!
//! What the world's era does not have is not offered: no house window without housing, no Game
//! Center without chess, no Create Spell without spell research.

use dereth_client_contract::panels::slumlord::{HouseOp, PaymentAction, PaymentListsView};
use dereth_client_contract::research::{Formula, ResearchSuccess, FORMULA_SLOTS};
use dereth_client_contract::view::{
    BarberView, GameView, HouseDataView, HousePurchaseView, MiniGameView, SlumlordView,
};
use dereth_client_contract::UiRequest;
use dereth_primitives::{EraFeatures, ObjectId, WorldRules};
use dereth_ui_screens::panels::barber::BarberPanel;
use dereth_ui_screens::screens::chargen::EParts;

use super::{WindowId, Windows};
use crate::art::Family;
use crate::draw::{Rect, WHITE};
use crate::ui::game::{GameState, Item};
use crate::ui::kit::{self, Ctx, Drop};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The chess board's buttons, by the game's element ids: resign, pass and stalemate.
const RESIGN: u32 = 0x1000_0175;
const STALEMATE: u32 = 0x1000_0177;

/// The world's services as the game has them this frame.
#[derive(Debug, Clone, Default)]
pub struct WorldServices {
    /// The systems the world's era has.
    pub features: EraFeatures,
    /// The spell filter controls this world has.
    pub era_ui: dereth_client_contract::era::EraUiFacts,
    /// The rules the world plays by.
    pub rules: WorldRules,
    /// The housing window's two payment lists, and whether the window is up.
    pub payments: PaymentListsView,
    /// The dwelling the deed just used offers.
    pub slumlord: Option<SlumlordView>,
    /// How many house profiles have arrived: each new one raises the housing window.
    pub slumlord_notices: u64,
    /// The items on the payment lists, as tiles.
    pub payment_items: Vec<Item>,
    /// The player's own house, and the purchase wait.
    pub house: Option<HouseDataView>,
    pub house_purchase: HousePurchaseView,
    pub house_notices: u64,
    /// The chess game.
    pub minigame: Option<MiniGameView>,
    /// The barber's start notice, while one is up.
    pub barber: Option<BarberView>,
    /// The last formula the game confirmed made a spell.
    pub research_success: Option<ResearchSuccess>,
    /// The spellbook's filter, kept by the server.
    pub spell_filters: u32,
}

/// Read the world's services from the game.
#[must_use]
pub fn read(view: &dyn GameView) -> WorldServices {
    let payments = view.payment_lists();
    let payment_items = payments
        .buy
        .iter()
        .chain(payments.rent.iter())
        .map(|p| crate::state::item_of(view, p.id))
        .collect();
    WorldServices {
        features: view.era_features(),
        era_ui: view.era_ui(),
        rules: view
            .era()
            .map(|e| e.world_rules.clone())
            .unwrap_or_default(),
        payments,
        slumlord: view.slumlord(),
        slumlord_notices: view.slumlord_notices(),
        payment_items,
        house: view.house_data(),
        house_purchase: view.house_purchase(),
        house_notices: view.house_data_notices() + view.house_status_notices(),
        minigame: view.minigame(),
        barber: view.barber(),
        research_success: view.research_success(),
        spell_filters: view.spell_filters(),
    }
}

/// The world-service windows' own state.
#[derive(Debug, Default)]
pub struct WorldWindows {
    /// The house profiles seen, so each new one raises the housing window, and whether the
    /// window is up for the game's lists.
    slumlord_seen: u64,
    housing_shown: bool,
    /// A payment question asked and not yet answered, for buying and for maintenance.
    asked: [bool; 2],
    /// The answers to the payment questions the dialog service handed back: rent or buy, and
    /// the answer (`None` when the box went without one).
    pub answers: Vec<(bool, Option<bool>)>,
    /// The house window asked the game for the house once.
    house_queried: bool,
    /// Whether the chess game was up last frame.
    chess_up: bool,
    /// The barber: the retail panel's appearance state and rules, and the part chosen.
    pub barber: BarberPanel,
    barber_part: usize,
    /// The heritage's special option's caption, in the game's words.
    pub barber_caption: Option<String>,
    /// Whether the barber is up for a start notice.
    barber_shown: bool,
    /// The systems the world's era has, once the game has said.
    features: Option<EraFeatures>,
    /// The research formula, the success last followed, and the components' scroll.
    formula: Formula,
    success_seen: u64,
    research_scroll: f32,
}

/// The barber's parts, as its rows name them.
const BARBER_PARTS: [(&str, EParts); 5] = [
    ("Hair", EParts::Hair),
    ("Eyes", EParts::Eyes),
    ("Nose", EParts::Nose),
    ("Mouth", EParts::Mouth),
    ("Skin", EParts::Skin),
];

impl WindowId {
    /// Whether the world's era has what the window shows.
    #[must_use]
    pub fn offered(self, features: &EraFeatures) -> bool {
        match self {
            Self::House | Self::Maintenance => features.housing,
            Self::GameCenter => features.chess,
            Self::Journal => features.journal || features.contracts,
            _ => true,
        }
    }
}

impl Windows {
    /// Whether window `id` is offered on this world.
    #[must_use]
    pub fn offers(&self, id: WindowId) -> bool {
        self.world.features.is_none_or(|f| id.offered(&f))
    }

    /// The world's notices: a new house profile raises the housing window and a closed list
    /// takes it down; the chess game's window follows the game; a barber's start opens the
    /// barber; a confirmed formula is cleared.
    pub(super) fn follow_world(&mut self, ctx: &Ctx<'_>, state: &GameState, out: &mut Outcome) {
        let w = &state.world_services;
        self.world.features = state.in_world.then_some(w.features);
        // The housing window: up while the game has the lists up, raised by each new profile;
        // closed by the player, it closes the lists and stops watching the deed's range.
        let up = w.payments.visible && w.slumlord.is_some();
        if up && self.world.housing_shown && !self.is_open(WindowId::Maintenance) {
            out.requests
                .push(UiRequest::PaymentList(PaymentAction::Close));
            out.requests.push(UiRequest::UnregisterSlumlordRange);
            self.world.housing_shown = false;
        } else if up
            && (w.slumlord_notices != self.world.slumlord_seen || !self.world.housing_shown)
        {
            self.open(WindowId::Maintenance, ctx.time);
            self.world.housing_shown = true;
        } else if !up && self.world.housing_shown {
            self.close(WindowId::Maintenance);
            self.world.housing_shown = false;
        }
        self.world.slumlord_seen = w.slumlord_notices;
        for (rent, confirmed) in std::mem::take(&mut self.world.answers) {
            self.world.asked[usize::from(rent)] = false;
            match confirmed {
                None => {}
                Some(false) => {
                    out.requests
                        .push(UiRequest::PaymentList(PaymentAction::Close));
                    self.close(WindowId::Maintenance);
                }
                Some(true) => {
                    if let Some(r) = payment_submit(&w.payments, rent) {
                        out.requests.push(r);
                    }
                }
            }
        }
        // The chess game.
        let chess = w.minigame.is_some_and(|g| g.visible);
        if chess && !self.world.chess_up {
            self.open(WindowId::GameCenter, ctx.time);
        } else if !chess && self.world.chess_up {
            self.close(WindowId::GameCenter);
        }
        self.world.chess_up = chess;
        // The barber.
        if self.world.barber.adopt(&BarberStart(w.barber)) {
            self.world.barber_part = 0;
            self.world.barber_shown = true;
            self.open(WindowId::Barber, ctx.time);
        } else if self.world.barber_shown && !self.is_open(WindowId::Barber) {
            // Closed without Apply: the barber's Cancel, which sends nothing.
            self.world.barber.cancel();
            self.world.barber_shown = false;
        }
        // Research: a confirmed formula leaves the formula.
        if let Some(success) = &w.research_success {
            if self.world.success_seen != success.serial {
                self.world.success_seen = success.serial;
                if self.world.formula.components() == success.components {
                    self.world.formula.clear();
                }
            }
        }
    }

    /// The housing window: buying the dwelling, or paying its maintenance, with the items the
    /// price asks for dropped on its list.
    pub(super) fn maintenance(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let w = &state.world_services;
        let Some(h) = w.slumlord.as_ref() else {
            return;
        };
        let rent = w.payments.op.is_rent();
        let mut tab = usize::from(rent);
        if kit::tabs(
            p,
            ctx,
            Rect::new(body.x, body.y, body.w, 26.0 * k),
            &["Buy", "Maintenance"],
            &mut tab,
        ) {
            let op = if tab == 1 {
                HouseOp::Rent
            } else {
                HouseOp::Buy
            };
            out.requests
                .push(UiRequest::PaymentList(PaymentAction::Select(op)));
        }
        let text = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let payment = if rent {
            &w.payments.rent_payment
        } else {
            &w.payments.buy_payment
        };
        let owner = if h.owner_name.is_empty() {
            dereth_ui_screens::panels::slumlord::OWNER_NONE
        } else {
            h.owner_name.as_str()
        };
        p.text(
            &text,
            body.x + 8.0 * k,
            body.y + 34.0 * k,
            &format!(
                "{}{owner}",
                dereth_ui_screens::panels::slumlord::OWNER_PREFIX
            ),
        );
        let mut y = body.y + 56.0 * k;
        for line in p.wrap(&text, &payment.requirements, body.w - 16.0 * k) {
            p.text(&text, body.x + 8.0 * k, y, &line);
            y += 18.0 * k;
        }
        // The offered items: dropped here, double-clicked off.
        let offered = if rent {
            &w.payments.rent
        } else {
            &w.payments.buy
        };
        let strip = Rect::new(
            body.x + 8.0 * k,
            body.bottom() - 96.0 * k,
            body.w - 16.0 * k,
            48.0 * k,
        );
        p.fill(strip, 0x6010_0C08);
        ctx.drops.push((strip, Some(Drop::Window)));
        let slot = 46.0 * k;
        for (i, row) in offered.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                strip.x + i as f32 * slot,
                strip.y + 1.0 * k,
                slot - 2.0 * k,
                slot - 2.0 * k,
            );
            if r.right() > strip.right() {
                break;
            }
            let item = w.payment_items.iter().find(|it| it.id == row.id);
            if let Some(icon) = item.and_then(|it| {
                p.art
                    .ac_item(it)
                    .or_else(|| it.ac_icon.and_then(|d| p.art.ac_icon(d)))
            }) {
                p.sprite(&icon, kit::icon_rect(r), WHITE);
            }
            if ctx.input.double_clicked(&r) {
                out.requests
                    .push(UiRequest::PaymentList(PaymentAction::Remove(row.id)));
            } else if ctx.over(&r) && ctx.input.clicked(&r) {
                out.requests.push(UiRequest::Select(row.id));
            }
        }
        // A drop on the list: the selected stack splits first, as the game's window does.
        if let Some(item) = dropped_on(ctx, &strip) {
            match self.split {
                Some((id, amount, max)) if id == item && amount < max && amount > 0 => {
                    out.requests
                        .push(UiRequest::StackSliderChanged { split: amount, max });
                    out.requests
                        .push(UiRequest::PaymentList(PaymentAction::Add(item)));
                }
                _ => out
                    .requests
                    .push(UiRequest::PaymentList(PaymentAction::Add(item))),
            }
        }
        let enabled = pay_allowed(&w.payments, h);
        let b = Rect::new(
            body.right() - 140.0 * k,
            body.bottom() - 40.0 * k,
            130.0 * k,
            32.0 * k,
        );
        if kit::button(p, ctx, b, if rent { "Maintenance" } else { "Buy" }, enabled) {
            let question = if rent {
                !h.am_i_the_owner
            } else {
                dereth_client_contract::panels::slumlord::purchase_asks_first(
                    h.house_type,
                    &w.rules,
                )
            };
            if question {
                if !std::mem::replace(&mut self.world.asked[usize::from(rent)], true) {
                    out.requests
                        .push(UiRequest::HousePaymentConfirmation { rent });
                }
            } else if let Some(r) = payment_submit(&w.payments, rent) {
                out.requests.push(r);
            }
        }
    }

    /// The house window: the player's own house, its price, maintenance and where it is, and
    /// when another may be bought, in the rows the game's house pane shows.
    pub(super) fn house(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let w = &state.world_services;
        if w.house.is_none() && !self.world.house_queried {
            self.world.house_queried = true;
            out.requests.push(UiRequest::QueryHouse);
        }
        let mut y = body.y + 8.0 * k;
        for (line, colour) in
            dereth_ui_screens::panels::house::house_lines(w.house.as_ref(), w.house_purchase)
        {
            use dereth_ui_screens::panels::house::HousePanelTextColor as C;
            let argb = match colour {
                C::Normal => ctx.colours.text(),
                C::RentPaid => 0xFF00_FF00,
                C::RentNotPaid => 0xFFFF_FF00,
            };
            let style = TextStyle::new(Family::Body, 13.0, argb).edge(ctx.colours.edge());
            for part in line.split('\n') {
                for wrapped in p.wrap(&style, part, body.w - 16.0 * k) {
                    p.text(&style, body.x + 8.0 * k, y, &wrapped);
                    y += 18.0 * k;
                }
            }
            y += 6.0 * k;
        }
    }

    /// The Game Center: the board as the game draws it for the player's side, a press on a
    /// square to choose and move, and Resign (asked first) and Stalemate.
    pub(super) fn chess(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let dim = TextStyle::new(Family::Body, 13.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let Some(g) = state.world_services.minigame.filter(|g| g.game.0 != 0) else {
            p.text_in(&dim, body, Align::Centre, "No game is in progress.");
            return;
        };
        let side = (body.w.min(body.h - 50.0 * k) - 16.0 * k).max(64.0 * k);
        let cell = side / 8.0;
        let board = Rect::new(body.x + (body.w - side) / 2.0, body.y + 8.0 * k, side, side);
        for i in 0..dereth_ui_screens::panels::minigame::CELLS {
            let (col, row) = (i % 8, i / 8);
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                board.x + col as f32 * cell,
                board.y + row as f32 * cell,
                cell,
                cell,
            );
            p.fill(
                r,
                if (col + row) % 2 == 0 {
                    0xFFD8_C8A0
                } else {
                    0xFF6A_4C2E
                },
            );
            if g.selected_cell == Some(i) {
                p.outline(r, 3.0 * k, 0xFF40_C0FF);
            }
            if let Some(slot) = g.piece_slots[i] {
                let (letter, white) = piece(slot);
                let style = TextStyle::new(
                    Family::Heading,
                    23.0,
                    if white { 0xFFFF_FFFF } else { 0xFF10_1010 },
                )
                .edge(if white { 0xFF00_0000 } else { 0xFFC0_C0C0 });
                p.text_in(&style, r, Align::Centre, letter);
            }
            // A move is made on the press, as the game's board takes it.
            if ctx.over(&r) && ctx.input.clicked(&r) {
                out.requests.push(UiRequest::MiniGameBoardPress(i));
            }
        }
        let y = board.bottom() + 10.0 * k;
        if kit::button(
            p,
            ctx,
            Rect::new(body.x + 8.0 * k, y, 120.0 * k, 30.0 * k),
            "Resign",
            true,
        ) {
            out.requests.push(UiRequest::MiniGameButton(RESIGN));
        }
        let label = if g.stalemate {
            "Stalemate (offered)"
        } else {
            "Stalemate"
        };
        if kit::button(
            p,
            ctx,
            Rect::new(body.right() - 168.0 * k, y, 160.0 * k, 30.0 * k),
            label,
            true,
        ) {
            out.requests.push(UiRequest::MiniGameButton(STALEMATE));
        }
    }

    /// The barber: each part's style stepped, its colour and shade chosen, the heritage's
    /// special option, and Apply or Cancel.
    pub(super) fn barber(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        _state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let text = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let world = &mut self.world;
        let b = &mut world.barber;
        let row_h = 34.0 * k;
        for (i, (name, part)) in BARBER_PARTS.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let row = Rect::new(
                body.x,
                body.y + 6.0 * k + i as f32 * row_h,
                body.w,
                row_h - 4.0 * k,
            );
            let on = world.barber_part == i;
            if on || ctx.input.hover(&row) {
                crate::ui::pregame::list_highlight(p, row, on);
            }
            p.text_in(&text, row.offset(10.0 * k, 0.0), Align::Left, name);
            if let Some((current, count)) = b.choice(*part) {
                let prev = Rect::new(row.right() - 150.0 * k, row.y, 34.0 * k, row.h);
                let next = Rect::new(row.right() - 40.0 * k, row.y, 34.0 * k, row.h);
                if kit::button(p, ctx, prev, "<", count > 1) {
                    b.step(*part, -1);
                    world.barber_part = i;
                }
                p.text_in(
                    &dim,
                    Rect::new(prev.right(), row.y, next.x - prev.right(), row.h),
                    Align::Centre,
                    &format!("{} / {count}", current + 1),
                );
                if kit::button(p, ctx, next, ">", count > 1) {
                    b.step(*part, 1);
                    world.barber_part = i;
                }
            }
            if ctx.over(&row) && ctx.input.clicked(&row) {
                world.barber_part = i;
            }
        }
        // The chosen part's colours and shade.
        let part = BARBER_PARTS[world.barber_part.min(BARBER_PARTS.len() - 1)].1;
        let (swatches, chosen) = b.colors(part);
        let y = body.y + 6.0 * k + 5.0 * row_h + 8.0 * k;
        let spot = 28.0 * k;
        for (i, colour) in swatches.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                body.x + 10.0 * k + i as f32 * (spot + 6.0 * k),
                y,
                spot,
                spot,
            );
            p.fill(r, colour.map_or(0xFF30_3030, |c| 0xFF00_0000 | c));
            if chosen == i32::try_from(i).ok() && swatches.len() > 1 {
                p.outline(r, 2.0 * k, 0xFFFF_FFFF);
            }
            if ctx.over(&r) && ctx.input.clicked(&r) {
                b.choose_color(part, i32::try_from(i).unwrap_or(0));
            }
        }
        if let Some(shade) = b.shade(part) {
            let r = Rect::new(
                body.x + 10.0 * k,
                y + spot + 10.0 * k,
                body.w - 20.0 * k,
                24.0 * k,
            );
            #[allow(clippy::cast_possible_truncation)]
            if let Some(v) = kit::slider(p, ctx, r, shade as f32, 0.0, 1.0) {
                b.choose_shade(part, f64::from(v));
            }
        }
        // The heritage's own option.
        let mut by = body.bottom() - 40.0 * k;
        if let Some((token, on)) = b.special_option() {
            let r = Rect::new(
                body.x + 10.0 * k,
                by - 34.0 * k,
                body.w - 20.0 * k,
                28.0 * k,
            );
            let caption = world
                .barber_caption
                .clone()
                .unwrap_or_else(|| option_caption(token));
            if let Some(v) = kit::check_row(p, ctx, r, on, &text, &caption) {
                b.set_special_option(v);
            }
            by = body.bottom() - 40.0 * k;
        }
        let apply = Rect::new(body.right() - 250.0 * k, by, 116.0 * k, 32.0 * k);
        let cancel = Rect::new(body.right() - 126.0 * k, by, 116.0 * k, 32.0 * k);
        let mut done = false;
        if kit::button(p, ctx, apply, "Apply", true) {
            out.requests.extend(b.apply());
            done = true;
        } else if kit::button(p, ctx, cancel, "Cancel", true) {
            b.cancel();
            done = true;
        }
        if done {
            world.barber_shown = false;
            self.close(WindowId::Barber);
        }
    }

    /// The Create Spell tab: up to eight carried components laid into a formula, which Test
    /// tries on the selection in magic mode. A component is laid by a double-click or a drag
    /// onto the formula, and taken out by a double-click there.
    pub(super) fn research(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let head =
            TextStyle::new(Family::Heading, 18.0, ctx.colours.heading()).edge(ctx.colours.edge());
        let carried: Vec<&dereth_client_contract::view::ComponentRow> = state
            .components
            .iter()
            .flat_map(|c| c.rows.iter())
            .filter(|r| r.owned > 0)
            .collect();
        let slot = 44.0 * k;
        p.text(&head, body.x + 8.0 * k, body.y + 34.0 * k, "Formula");
        #[allow(clippy::cast_precision_loss)]
        let formula = Rect::new(
            body.x + 8.0 * k,
            body.y + 58.0 * k,
            slot * FORMULA_SLOTS as f32,
            slot,
        );
        p.fill(formula, 0x6010_0C08);
        ctx.drops.push((formula, Some(Drop::Window)));
        let mut remove = None;
        for i in 0..FORMULA_SLOTS {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                formula.x + i as f32 * slot,
                formula.y,
                slot - 2.0 * k,
                slot - 2.0 * k,
            );
            let wcid = self.world.formula.components().get(i).copied();
            // A component no longer carried keeps its place, drawn empty.
            let icon = wcid
                .and_then(|w| carried.iter().find(|r| r.wcid == w))
                .and_then(|r| r.icon)
                .and_then(|d| p.art.ac_component(d.0));
            kit::icon_slot(p, ctx, r, icon.as_ref(), WHITE);
            if wcid.is_some() && ctx.input.double_clicked(&r) {
                remove = Some(i);
            }
        }
        if let Some(i) = remove {
            self.world.formula.remove(i);
        }
        if let Some(item) = dropped_on(ctx, &formula) {
            if let Some(row) = carried.iter().find(|r| r.object == Some(item)) {
                self.world.formula.add(row.wcid);
            }
        }
        p.text(
            &head,
            body.x + 8.0 * k,
            formula.bottom() + 12.0 * k,
            "Components",
        );
        // Whole slots across and down, and room for the scrollbar: no part slot of empty ground.
        let bar = kit::scrollbar_width(p).map_or(10.0, |w| w + 4.0) * k;
        let room = Rect::new(
            body.x + 8.0 * k,
            formula.bottom() + 36.0 * k,
            body.w - 16.0 * k,
            body.bottom() - formula.bottom() - 88.0 * k,
        );
        let columns = ((room.w - bar) / slot).floor().max(1.0);
        let rows_shown = (room.h / slot).floor().max(1.0);
        let area = Rect::new(room.x, room.y, columns * slot + bar, rows_shown * slot);
        p.fill(area, 0x6010_0C08);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let per_row = columns as usize;
        #[allow(clippy::cast_precision_loss)]
        let content = slot * carried.len().div_ceil(per_row) as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.world.research_scroll);
        p.list.push_clip(area);
        // Empty slots fill what the carried components leave of the visible rows.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let visible_rows = (area.h / slot).ceil() as usize;
        let rows = carried.len().div_ceil(per_row).max(visible_rows);
        for i in carried.len()..rows * per_row {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                area.x + (i % per_row) as f32 * slot,
                area.y + (i / per_row) as f32 * slot - offset,
                slot - 2.0 * k,
                slot - 2.0 * k,
            );
            if r.bottom() >= area.y && r.y <= area.bottom() {
                kit::icon_slot(p, ctx, r, None, WHITE);
            }
        }
        for (i, row) in carried.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                area.x + (i % per_row) as f32 * slot,
                area.y + (i / per_row) as f32 * slot - offset,
                slot - 2.0 * k,
                slot - 2.0 * k,
            );
            if r.bottom() < area.y || r.y > area.bottom() {
                continue;
            }
            let icon = row.icon.and_then(|d| p.art.ac_component(d.0));
            if kit::icon_slot(p, ctx, r, icon.as_ref(), WHITE) {
                if ctx.input.double {
                    self.world.formula.add(row.wcid);
                } else if let Some(object) = row.object {
                    out.requests.push(UiRequest::Select(object));
                    *ctx.drag = Some(crate::ui::Drag {
                        item: object,
                        look: None,
                        spell_look: None,
                        from_shortcut: None,
                        icon: row.icon.map(|d| d.0),
                        origin: ctx.input.mouse,
                        active: false,
                        on_click: None,
                        spell: None,
                        from_spell_slot: None,
                        component: true,
                    });
                }
            }
        }
        p.list.pop_clip();
        let any = !self.world.formula.is_empty();
        let y = body.bottom() - 40.0 * k;
        if kit::button(
            p,
            ctx,
            Rect::new(body.x + 8.0 * k, y, 120.0 * k, 32.0 * k),
            "Test",
            any,
        ) {
            out.requests.push(UiRequest::TestSpellFormula {
                components: self.world.formula.components().to_vec(),
            });
        }
        if kit::button(
            p,
            ctx,
            Rect::new(body.x + 136.0 * k, y, 120.0 * k, 32.0 * k),
            "Clear",
            any,
        ) {
            self.world.formula.clear();
        }
    }
}

/// The game's start notice, as the barber's rules read it.
#[derive(Debug)]
struct BarberStart(Option<BarberView>);

impl GameView for BarberStart {
    fn barber(&self) -> Option<BarberView> {
        self.0
    }
}

/// The special option's caption: the game's words where the barber's string table is read, else
/// the token's own.
fn option_caption(token: &str) -> String {
    match token {
        "ID_Barber_Empyrean_Earthbound" => "Earthbound".to_owned(),
        "ID_Barber_Undead_NoFlame" => "No Flame".to_owned(),
        "ID_Barber_Shadow_NoCrown" => "No Crown".to_owned(),
        other => other.to_owned(),
    }
}

/// A piece's letter, and whether it is the first side's: the board's slots are pawn, bishop,
/// knight, rook, queen and king, each side's six in turn.
fn piece(slot: u8) -> (&'static str, bool) {
    let letter = match slot % 6 {
        0 => "P",
        1 => "B",
        2 => "N",
        3 => "R",
        4 => "Q",
        _ => "K",
    };
    (letter, slot < 6)
}

/// Whether the payment for the chosen tab may be made: buying an unowned dwelling with its
/// price paid in full, or maintaining an owned one, with something on the list.
fn pay_allowed(payments: &PaymentListsView, h: &SlumlordView) -> bool {
    let rent = payments.op.is_rent();
    let offered = if rent { &payments.rent } else { &payments.buy };
    (rent || payments.buy_payment.paid_in_full) && !offered.is_empty() && (rent == (h.owner.0 != 0))
}

/// The payment, sent once the question (if one was asked) has been answered yes: refused when
/// nothing is offered, or a purchase is not paid in full.
fn payment_submit(payments: &PaymentListsView, rent: bool) -> Option<UiRequest> {
    let allowed = if rent {
        !payments.rent.is_empty()
    } else {
        !payments.buy.is_empty() && payments.buy_payment.paid_in_full
    };
    allowed.then_some(UiRequest::PaymentList(PaymentAction::Submit))
}

/// The item in hand dropped on `r` this frame, if one was.
fn dropped_on(ctx: &Ctx<'_>, r: &Rect) -> Option<ObjectId> {
    let drag = ctx.drag.as_ref()?;
    (drag.active && drag.spell.is_none() && !ctx.input.down[0] && ctx.input.hover(r))
        .then_some(drag.item)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;
    use dereth_client_contract::panels::slumlord::PaymentItem;
    use dereth_client_contract::view::SlumlordPayment;

    fn row(id: u32) -> PaymentItem {
        PaymentItem {
            id: ObjectId(id),
            wcid: 273,
            amount: 1,
            trade_note_value: None,
        }
    }

    #[test]
    fn a_purchase_is_paid_only_in_full_and_maintenance_only_on_an_owned_dwelling() {
        let unowned = SlumlordView::default();
        let owned = SlumlordView {
            owner: ObjectId(7),
            ..SlumlordView::default()
        };
        let mut buy = PaymentListsView {
            op: HouseOp::Buy,
            buy: vec![row(1)],
            ..PaymentListsView::default()
        };
        assert!(!pay_allowed(&buy, &unowned), "not paid in full");
        assert_eq!(payment_submit(&buy, false), None);
        buy.buy_payment = SlumlordPayment {
            paid_in_full: true,
            ..SlumlordPayment::default()
        };
        assert!(pay_allowed(&buy, &unowned));
        assert!(
            !pay_allowed(&buy, &owned),
            "an owned dwelling is not for sale"
        );
        assert_eq!(
            payment_submit(&buy, false),
            Some(UiRequest::PaymentList(PaymentAction::Submit))
        );
        let rent = PaymentListsView {
            op: HouseOp::Rent,
            rent: vec![row(2)],
            ..PaymentListsView::default()
        };
        assert!(pay_allowed(&rent, &owned));
        assert!(!pay_allowed(&rent, &unowned));
        assert!(!pay_allowed(
            &PaymentListsView {
                op: HouseOp::Rent,
                ..PaymentListsView::default()
            },
            &owned
        ));
    }

    #[test]
    fn each_side_s_six_slots_are_pawn_bishop_knight_rook_queen_and_king() {
        assert_eq!(piece(0), ("P", true));
        assert_eq!(piece(5), ("K", true));
        assert_eq!(piece(6), ("P", false));
        assert_eq!(piece(10), ("Q", false));
    }

    #[test]
    fn the_era_decides_whether_the_house_and_the_game_center_are_offered() {
        let mut f = EraFeatures::ALL;
        assert!(WindowId::House.offered(&f));
        f.housing = false;
        f.chess = false;
        assert!(!WindowId::House.offered(&f));
        assert!(!WindowId::Maintenance.offered(&f));
        assert!(!WindowId::GameCenter.offered(&f));
        assert!(WindowId::Character.offered(&f));
    }
}
