//! The windows of the player's things together: the inventory ([`super::inventory`]), the loot
//! window ([`super::loot`]), the trade ([`super::trade`]), the shop and the market's catalog
//! ([`super::shop`]), the consignments and a new sale ([`super::market`]), and what passes
//! between them: a thing carried from one to another and put down, the box asking how many, and
//! the questions they ask.
//!
//! What a thing put down asks of the game follows the game's own panels:
//! - an item moved within the pack, or between the pack and an open container, goes to the slot
//!   it is put down on (no further than just past the last item of that segment); a whole stack
//!   moved further along its own segment, past its last item, lands a slot sooner, as the place it
//!   left closes up. A stack asks how many first (at most what a stack holds). Put down on the tab
//!   of a segment not shown, it goes after that segment's last item, unless the segment is full.
//! - a worn item put down on the pack is taken off into that slot; an item put down on a place to
//!   wear it is worn there (on the figure, at its own place), and one from an open container is
//!   brought into the pack instead.
//! - an item put down on the money bag is melted: the kinds that melt at once at once, any other
//!   after a question unless the player turned it off or holds Shift (a stack asks how many
//!   first). The coins put down on a container come out of the bag, as many as the player says.
//! - an item put down on the world goes to what is under the pointer (the player itself is
//!   nothing: the ground), the coins likewise.
//! - an item put down on the trade is offered; one of the player's offers put down anywhere else
//!   is taken back.
//! - a sale put down on the pack is bought into that slot; a consignment, collected.
//! - an item put down on the consignments restocks one of the same kind that has not sold, after
//!   a question; anywhere else on them, or on the new sale, it becomes the new sale's item.

use auberean_client_sdk::contract::{StoreAction, TradeAction};
use auberean_client_sdk::Action;
use dereth_ui_kit::paint::Painter;
use dereth_ui_kit::widgets::Ctx;

use crate::draw::Rect;
use crate::state::goods::{sale_state, Goods as Things};
use crate::state::GameState;
use crate::ui::inventory::{self, Inventory};
use crate::ui::items::{Amount, Answered, Asking, Carried, Carry, For, Put, Question, Zone};
use crate::ui::loot::Loot;
use crate::ui::market::{self, cost_to_put_up, Market};
use crate::ui::shop::{self, Shops};
use crate::ui::trade::{Settle, TradeWindow};
use crate::ui::{Outcome, Tip};

/// What the windows of things ask of the interface besides the game's actions, taken once a
/// frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Asks {
    /// Find what is under the pointer in the world, where a carried thing was put down.
    pub pick_world: Option<(f32, f32)>,
    /// Lines to tell the player.
    pub told: Vec<String>,
    /// What the trade window settled.
    pub trade: Vec<Settle>,
    /// The game's purchase question was answered.
    pub purchase_answered: bool,
}

/// The windows of things, in the order they are drawn until one is opened or pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    Inventory,
    Loot,
    Trade,
    Catalog,
    Shop,
    Consignments,
    NewSale,
}

impl Window {
    pub const ALL: [Self; 7] = [
        Self::Inventory,
        Self::Loot,
        Self::Trade,
        Self::Catalog,
        Self::Shop,
        Self::Consignments,
        Self::NewSale,
    ];
}

/// The windows of things' order, the bottom first.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Order(Vec<Window>);

impl Default for Order {
    fn default() -> Self {
        Self(Window::ALL.to_vec())
    }
}

impl Order {
    fn raise(&mut self, w: Window) {
        self.0.retain(|o| *o != w);
        self.0.push(w);
    }
}

/// The windows of the player's things.
#[derive(Debug, Default)]
pub struct Goods {
    pub carry: Carry,
    pub inventory: Inventory,
    pub loot: Loot,
    pub trade: TradeWindow,
    pub shops: Shops,
    pub market: Market,
    /// The box asking how many, and the question up.
    pub amount: Option<Amount>,
    pub question: Option<Question>,
    /// A thing put down on the world, waiting for what is under the pointer.
    pub world_drop: Option<Carried>,
    pub asks: Asks,
    /// Where the windows were last drawn, each with its place among them ([`Window::ALL`]), in
    /// drawing order, and the box over them.
    rects: Vec<(Window, Rect)>,
    over: Option<Rect>,
    /// The windows' order, the bottom first: one opened, or pressed on, comes to the front.
    order: Order,
    /// The windows drawn under these this frame: each of these, and the box, is backed where it
    /// lies over one of them or over one of these drawn before it.
    pub below: Vec<Rect>,
}

impl Goods {
    /// Where the windows were last drawn, and the box over them: what lies under them does not
    /// answer the pointer.
    #[must_use]
    pub fn rects(&self) -> Vec<Rect> {
        self.rects
            .iter()
            .map(|(_, r)| *r)
            .chain(self.over)
            .collect()
    }

    /// The windows drawn last frame, in the order drawn (the front one last), each where it was.
    #[must_use]
    pub fn drawn(&self) -> &[(Window, Rect)] {
        &self.rects
    }

    /// The windows' order, the bottom first.
    #[must_use]
    pub fn order(&self) -> &[Window] {
        &self.order.0
    }

    /// Whether one of its windows or boxes is open.
    #[must_use]
    pub fn any_open(&self) -> bool {
        !self.rects.is_empty() || self.amount.is_some() || self.question.is_some()
    }

    /// Whether one of its boxes has the keys: the amount's number, the new sale's price, a
    /// question (Enter says yes, Escape no).
    #[must_use]
    pub fn typing(&self) -> bool {
        self.amount.as_ref().is_some_and(Amount::typing)
            || self.question.is_some()
            || self.market.new_sale.typing()
    }

    /// Close what is on top, as last drawn: a box, else the window in front; whether something
    /// was open.
    pub fn close_top(&mut self, actions: &mut Vec<Action>) -> bool {
        if self.amount.take().is_some() {
            return true;
        }
        if let Some(q) = self.question.take() {
            if q.asking == Asking::Purchase {
                actions.push(Action::Store(StoreAction::Confirm(false)));
                self.asks.purchase_answered = true;
            }
            return true;
        }
        let order = self.order.0.clone();
        order
            .into_iter()
            .rev()
            .any(|w| self.close_window(w, actions))
    }

    /// Close one of the windows when it is open; whether it was.
    fn close_window(&mut self, w: Window, actions: &mut Vec<Action>) -> bool {
        match w {
            Window::NewSale if self.market.new_sale.window.open => self.market.new_sale.close(),
            Window::Trade => match self.trade.shown() {
                Some(true) => self.asks.trade.push(Settle::Dismissed),
                Some(false) => actions.push(Action::Trade(TradeAction::Close)),
                None => return false,
            },
            Window::Shop if self.shops.shop_drawn().is_some() => {
                actions.push(Action::Store(StoreAction::LeaveStore));
            }
            Window::Catalog if self.shops.catalog_drawn().is_some() => {
                actions.push(Action::Store(StoreAction::LeaveCatalog));
            }
            Window::Consignments if self.market.drawn().is_some() => {
                actions.push(Action::Store(StoreAction::LeaveConsignment));
            }
            Window::Loot => match self.loot.shown() {
                Some(c) => actions.push(Action::SetOpen {
                    container: c,
                    open: false,
                }),
                None => return false,
            },
            Window::Inventory if self.inventory.is_open() => self.inventory.close(),
            _ => return false,
        }
        true
    }

    /// Everything of its own closed and what is carried let go, as the interface is left.
    pub fn close_all(&mut self) {
        self.inventory.close();
        self.market.new_sale.close();
        self.amount = None;
        self.question = None;
        self.carry.drag = None;
        self.world_drop = None;
    }

    /// Draw the windows in their order, then the box over them; where each was drawn. A press on
    /// one (where nothing over these windows is) brings it to the front first; one opened this
    /// frame comes to the front from the next. The places that take a carried thing were
    /// forgotten for the frame before any window that has them was drawn ([`Carry::begin`]).
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
        tip: &mut Option<Tip>,
    ) -> Vec<Rect> {
        let base = ctx.input.occluders.len();
        let last = std::mem::take(&mut self.rects);
        let mut drawn = Vec::new();
        let mut shop_asked = Vec::new();
        let mut settle = Vec::new();
        let mut question = None;
        let backing = p.art.palette().panel;
        let mut below = std::mem::take(&mut self.below);
        if ctx.input.pressed[0] || ctx.input.pressed[1] {
            let at = ctx.input.mouse;
            let hidden = ctx.input.occluders[..base]
                .iter()
                .chain(self.over.as_ref())
                .any(|r| r.contains(at.0, at.1));
            let pressed = last.iter().rev().find(|(_, r)| r.contains(at.0, at.1));
            if let (false, Some((w, _))) = (hidden, pressed) {
                self.order.raise(*w);
            }
        }
        let order = self.order.0.clone();
        // Each window under those drawn after it last frame, and under the box.
        for (i, w) in order.iter().enumerate() {
            let from = p.list.quads.len();
            ctx.input.occluders.truncate(base);
            ctx.input.occluders.extend(
                last.iter()
                    .filter(|(o, _)| order[i + 1..].contains(o))
                    .map(|(_, r)| *r),
            );
            ctx.input.occluders.extend(self.over);
            let c = &mut self.carry;
            let r = match w {
                Window::Inventory => self.inventory.frame(p, ctx, state, c, out, tip),
                Window::Loot => self.loot.frame(p, ctx, state, c, out, tip),
                Window::Trade => self.trade.frame(p, ctx, state, c, out, &mut settle, tip),
                Window::Catalog => self.shops.catalog_frame(p, ctx, state, c, out, tip),
                Window::Shop => self
                    .shops
                    .shop_frame(p, ctx, state, c, out, &mut shop_asked, tip),
                Window::Consignments => self.market.consignments_frame(p, ctx, state, c, out, tip),
                Window::NewSale => {
                    let (r, q) =
                        self.market
                            .new_sale_frame(p, ctx, &state.goods, c, &mut self.asks.told);
                    question = q;
                    r
                }
            };
            if let Some(r) = r {
                crate::ui::stack::back(p.list, from, r, &below, backing);
                crate::ui::place::add_open(r);
                below.push(r);
                drawn.push((*w, r));
            }
        }
        ctx.input.occluders.truncate(base);
        // A window not open last frame comes to the front.
        for (w, _) in &drawn {
            if !last.iter().any(|(o, _)| o == w) {
                self.order.raise(*w);
            }
        }
        self.asks.trade.extend(settle);
        if let Some(q) = question {
            self.question = Some(q);
        }
        for a in shop_asked {
            match a {
                shop::Asked::Buy { item, slot } => self.buy(&state.goods, item, slot, out),
            }
        }
        let mut covers: Vec<Rect> = drawn.iter().map(|(_, r)| *r).collect();
        if let Some(r) = market::reminder(p, ctx, state, out) {
            covers.push(r);
        }
        self.follow_purchase(&state.goods);
        self.over = None;
        if let Some(a) = &mut self.amount {
            let from = p.list.quads.len();
            let (r, answer) = a.frame(p, ctx);
            crate::ui::stack::back(p.list, from, r, &below, backing);
            self.over = Some(r);
            covers.push(r);
            if let Some(answer) = answer {
                let a = self.amount.take().unwrap_or_else(|| unreachable!());
                if let Answered::Yes(n) = answer {
                    self.amounted(&a, n, state, ctx.input.shift, out);
                }
            }
        }
        if let Some(q) = &mut self.question {
            let from = p.list.quads.len();
            let (r, answer) = q.frame(p, ctx);
            crate::ui::stack::back(p.list, from, r, &below, backing);
            self.over = Some(r);
            covers.push(r);
            if let Some(yes) = answer {
                let q = self.question.take().unwrap_or_else(|| unreachable!());
                self.answered(&q.asking, yes, out);
            }
        }
        self.rects = drawn;
        covers
    }

    /// The end of the frame: the carried thing put down, when the button was let go, and drawn
    /// under the pointer while it is carried. `over_later`: something drawn after the windows is
    /// under the pointer; `over_any`: anything of the interface is. A put down nothing takes is
    /// handed back.
    pub fn finish(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &Ctx<'_>,
        state: &GameState,
        over_later: bool,
        over_any: bool,
        out: &mut Outcome,
    ) -> Option<Put> {
        self.carry.draw(p, ctx.input.mouse);
        let put = self.carry.end(ctx, over_later, over_any)?;
        self.put(put, state, ctx.input.shift, out)
    }

    /// What was found under the pointer where a thing was put down on the world.
    pub fn world_found(&mut self, found: u64, state: &GameState, out: &mut Outcome) {
        let Some(carried) = self.world_drop.take() else {
            return;
        };
        let g = &state.goods;
        let Some(player) = g.player() else { return };
        let target = if found == player { 0 } else { found };
        match carried {
            Carried::Item { item, slot, .. } => {
                let Some(it) = g.item(item) else { return };
                if it.stackable && it.quantity > 1 {
                    let whole = it.stack_size.min(it.quantity);
                    self.amount = Some(Amount::new(
                        For::Put {
                            item,
                            from: player,
                            to: target,
                            old_slot: slot,
                            new_slot: 0,
                            whole,
                            past_end: false,
                        },
                        it.name.clone(),
                        whole,
                        whole,
                    ));
                } else {
                    out.acts.push(Action::PutItem {
                        item,
                        from: player,
                        to: target,
                        old_slot: slot,
                        new_slot: 0,
                        quantity: it.quantity,
                    });
                }
            }
            Carried::Coins => self.ask_coins(g, 0, target),
            _ => {}
        }
    }

    /// The player's purchase question follows the game's: up while the game asks, down when the
    /// shop goes.
    fn follow_purchase(&mut self, g: &Things) {
        let asked = self
            .question
            .as_ref()
            .is_some_and(|q| q.asking == Asking::Purchase);
        match (&g.purchase, asked) {
            (Some(p), false) if self.question.is_none() && !self.asks.purchase_answered => {
                let what = if p.quantity == 1 {
                    p.product.clone()
                } else {
                    format!("{} {}", p.quantity, p.product)
                };
                self.question = Some(Question::new(
                    Asking::Purchase,
                    format!("Buy {what} for {} {}?", p.cost_quantity, p.cost),
                ));
            }
            (None, true) => self.question = None,
            _ => {}
        }
    }

    /// Ask how many coins to take out of the bag into a slot of a container.
    fn ask_coins(&mut self, g: &Things, slot: u32, container: u64) {
        let coin_stack = g.inventory.as_ref().map_or(0, |i| i.coin_stack);
        let most = g.money().min(coin_stack);
        if most == 0 {
            return;
        }
        self.amount = Some(Amount::new(
            For::Coins { slot, container },
            "Coins".to_owned(),
            most,
            most,
        ));
    }

    /// A sale bought into a slot of the pack: not to be had, the player told so; more than one
    /// to be had, how many asked; else one.
    fn buy(&mut self, g: &Things, item: u64, slot: u32, out: &mut Outcome) {
        let Some(sale) = g.sale(item).filter(|s| !s.unavailable()) else {
            self.asks.told.push("That can't be bought.".to_owned());
            return;
        };
        let most = g.max_purchase.get(&item).copied().unwrap_or(0);
        if most == 0 {
            self.asks.told.push("You can't afford that.".to_owned());
        } else if most == 1 {
            out.acts.push(Action::Store(StoreAction::Purchase {
                item,
                quantity: 1,
                slot,
            }));
        } else {
            self.amount = Some(Amount::new(
                For::Buy { item, slot },
                sale.description.clone(),
                1,
                most,
            ));
        }
    }

    /// Whether to ask before melting: unless the player turned the question off or holds Shift.
    fn ask_before_melting(g: &Things, shift: bool) -> bool {
        !shift && !g.inventory.as_ref().is_some_and(|i| i.no_melt_question)
    }

    /// The melt question, for `quantity` of an item.
    fn melt_question(g: &Things, item: u64, quantity: u32, slot: u32, from: u64) -> Question {
        let name = g.item(item).map_or_else(String::new, |it| it.name.clone());
        Question::new(
            Asking::Melt {
                item,
                quantity,
                slot,
                from,
            },
            format!("Turn {name} into gold?"),
        )
    }

    /// A thing put down: what it asks of the game.
    #[allow(clippy::too_many_lines)]
    fn put(&mut self, put: Put, state: &GameState, shift: bool, out: &mut Outcome) -> Option<Put> {
        let g = &state.goods;
        let (carried, zone) = match put {
            Put::On(c, z) => (c, z),
            Put::World(c, at) => {
                match c {
                    Carried::Item { from, .. } if Some(from) == g.player() => {
                        self.world_drop = Some(c);
                        self.asks.pick_world = Some(at);
                    }
                    Carried::Coins => {
                        self.world_drop = Some(c);
                        self.asks.pick_world = Some(at);
                    }
                    Carried::Offered { item } => {
                        out.acts.push(Action::Trade(TradeAction::Revoke(item)))
                    }
                    _ => return Some(Put::World(c, at)),
                }
                return None;
            }
            Put::Nowhere(c) => {
                if let Carried::Offered { item } = c {
                    out.acts.push(Action::Trade(TradeAction::Revoke(item)));
                    return None;
                }
                return Some(Put::Nowhere(c));
            }
        };
        let player = g.player()?;
        if let Carried::Offered { item } = carried {
            if zone != Zone::TradeOffer {
                out.acts.push(Action::Trade(TradeAction::Revoke(item)));
            }
            return None;
        }
        // Where in a container the thing goes: the container, its slot, whether that is past the
        // last item of its segment.
        let into = match zone {
            Zone::Pack { segment, cell } => {
                let count = g.segment_count(segment);
                Some((
                    player,
                    g.first_slot(segment) + cell.min(count),
                    cell >= count,
                ))
            }
            Zone::PackTab { segment } => {
                let count = g.segment_count(segment);
                if count >= g.rooms()[segment] {
                    return None;
                }
                Some((player, g.first_slot(segment) + count, false))
            }
            Zone::Container { container, cell } => {
                let count = u32::try_from(g.contents(container, 0).len()).unwrap_or(0);
                Some((container, cell.min(count), cell >= count))
            }
            _ => None,
        };
        match (carried, zone, into) {
            (Carried::Item { .. }, Zone::PackTab { segment }, _)
                if segment == self.inventory.segment =>
            {
                return None;
            }
            (Carried::Item { item, from, slot }, _, Some((to, new_slot, past_end))) => {
                if from == to && slot == new_slot {
                    return None;
                }
                let it = g.item(item)?;
                let whole = it.stack_size.min(it.quantity);
                let what = For::Put {
                    item,
                    from,
                    to,
                    old_slot: slot,
                    new_slot,
                    whole,
                    past_end,
                };
                if it.stackable && it.quantity > 1 {
                    self.amount = Some(Amount::new(what, it.name.clone(), whole, whole));
                } else {
                    self.amounted_put(&what, it.quantity, out);
                }
            }
            (Carried::Worn { item, .. }, _, Some((to, new_slot, _))) if to == player => {
                out.acts.push(Action::TakeOff {
                    item,
                    slot: new_slot,
                });
            }
            (Carried::Coins, _, Some((container, slot, _))) => self.ask_coins(g, slot, container),
            (Carried::Sale { item }, _, Some((to, slot, _))) if to == player => {
                self.buy(g, item, slot, out);
            }
            (Carried::Consignment { sale, sold }, _, Some((to, _, _))) if to == player => {
                out.acts
                    .push(Action::Store(StoreAction::Collect { sale, sold }));
            }
            (Carried::Item { item, from, .. }, Zone::Wear { .. } | Zone::Figure, _) => {
                let it = g.item(item)?;
                if from != player {
                    out.acts.push(Action::PutItem {
                        item,
                        from,
                        to: player,
                        old_slot: 0,
                        new_slot: 0,
                        quantity: 1,
                    });
                    return None;
                }
                let location = match zone {
                    Zone::Wear { location } => location,
                    _ => it.preferred_location,
                };
                if location != 0 {
                    out.acts.push(Action::Wear { item, location });
                }
            }
            (Carried::Worn { item, location: at }, Zone::Wear { location }, _)
                if at != location =>
            {
                out.acts.push(Action::Wear { item, location });
            }
            (Carried::Item { item, from, slot }, Zone::Money, _) if from == player => {
                let it = g.item(item)?;
                if it.worn || !it.had {
                    return None;
                }
                if it.melts_at_once {
                    out.acts.push(Action::IntoMoney {
                        item,
                        quantity: it.quantity,
                        slot,
                        from,
                    });
                } else if it.stackable && it.quantity > 1 {
                    let n = it.stack_size.min(it.quantity);
                    self.amount = Some(Amount::new(
                        For::Melt { item, slot, from },
                        it.name.clone(),
                        n,
                        n,
                    ));
                } else if Self::ask_before_melting(g, shift) {
                    self.question = Some(Self::melt_question(g, item, it.quantity, slot, from));
                } else {
                    out.acts.push(Action::IntoMoney {
                        item,
                        quantity: it.quantity,
                        slot,
                        from,
                    });
                }
            }
            (Carried::Item { item, from, .. }, Zone::TradeOffer, _) if from == player => {
                if !g.trade.as_ref().is_some_and(|t| t.open) {
                    return None;
                }
                let it = g.item(item)?;
                if it.worn {
                    return None;
                }
                if it.stackable && it.quantity > 1 {
                    let n = it.stack_size.min(it.quantity);
                    self.amount = Some(Amount::new(For::Offer { item }, it.name.clone(), n, n));
                } else {
                    out.acts.push(Action::Trade(TradeAction::Offer {
                        item,
                        quantity: it.quantity,
                    }));
                }
            }
            (
                Carried::Item { item, from, .. },
                Zone::Consignment { .. } | Zone::Consignments | Zone::NewSale,
                _,
            ) if from == player => {
                let it = g.item(item)?;
                let restocks = match zone {
                    Zone::Consignment { sale } => g.consignments.as_ref().and_then(|c| {
                        c.sales.iter().find(|s| {
                            s.sale.ordinal == sale
                                && s.sale.state != sale_state::SOLD
                                && s.sale.item_data == it.data_id
                        })
                    }),
                    _ => None,
                };
                match restocks {
                    Some(s) => {
                        let sale = s.sale.ordinal;
                        if it.quantity > 1 {
                            self.amount = Some(Amount::new(
                                For::Restock { item, sale },
                                it.name.clone(),
                                it.quantity,
                                it.quantity,
                            ));
                        } else {
                            self.restock_question(g, item, 1, sale);
                        }
                    }
                    None if it.quantity > 1 => {
                        self.amount = Some(Amount::new(
                            For::Sell { item },
                            it.name.clone(),
                            it.quantity,
                            it.quantity,
                        ));
                    }
                    None => self.market.new_sale.open(Some((item, 1))),
                }
            }
            _ => return Some(Put::On(carried, zone)),
        }
        None
    }

    /// The restock question: what restocking `quantity` costs.
    fn restock_question(&mut self, g: &Things, item: u64, quantity: u32, sale: u32) {
        let Some(s) = g
            .consignments
            .as_ref()
            .and_then(|c| c.sales.iter().find(|s| s.sale.ordinal == sale))
        else {
            return;
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let price = s.sale.cost_quantity.max(0.0) as u32;
        let fee = cost_to_put_up(price, quantity, g.consignment_charge);
        let name = g.item(item).map_or_else(String::new, |it| it.name.clone());
        self.question = Some(Question::new(
            Asking::Restock {
                item,
                quantity,
                sale,
            },
            format!("Restock with {name}? It costs {}.", inventory::coins(fee)),
        ));
    }

    /// A move of so many: a whole stack moved further along its own segment, past its last
    /// item, lands a slot sooner.
    fn amounted_put(&self, what: &For, n: u32, out: &mut Outcome) {
        let For::Put {
            item,
            from,
            to,
            old_slot,
            new_slot,
            whole,
            past_end,
        } = *what
        else {
            return;
        };
        let new_slot = if from == to && past_end && n == whole {
            new_slot.saturating_sub(1)
        } else {
            new_slot
        };
        if from == to && new_slot == old_slot {
            return;
        }
        out.acts.push(Action::PutItem {
            item,
            from,
            to,
            old_slot,
            new_slot,
            quantity: n,
        });
    }

    /// The box asking how many said `n`.
    fn amounted(&mut self, a: &Amount, n: u32, state: &GameState, shift: bool, out: &mut Outcome) {
        if n == 0 {
            return;
        }
        let g = &state.goods;
        match a.what {
            For::Put { .. } => self.amounted_put(&a.what, n, out),
            For::Coins { slot, container } => out.acts.push(Action::OutOfMoney {
                amount: n,
                slot,
                container,
            }),
            For::Melt { item, slot, from } => {
                if Self::ask_before_melting(g, shift) {
                    self.question = Some(Self::melt_question(g, item, n, slot, from));
                } else {
                    out.acts.push(Action::IntoMoney {
                        item,
                        quantity: n,
                        slot,
                        from,
                    });
                }
            }
            For::Offer { item } => out
                .acts
                .push(Action::Trade(TradeAction::Offer { item, quantity: n })),
            For::Buy { item, slot } => out.acts.push(Action::Store(StoreAction::Purchase {
                item,
                quantity: n,
                slot,
            })),
            For::Restock { item, sale } => self.restock_question(g, item, n, sale),
            For::Sell { item } => self.market.new_sale.open(Some((item, n))),
        }
    }

    /// A question of the windows' own answered.
    fn answered(&mut self, asking: &Asking, yes: bool, out: &mut Outcome) {
        match *asking {
            Asking::Purchase => {
                out.acts.push(Action::Store(StoreAction::Confirm(yes)));
                self.asks.purchase_answered = true;
            }
            _ if !yes => {}
            Asking::Melt {
                item,
                quantity,
                slot,
                from,
            } => out.acts.push(Action::IntoMoney {
                item,
                quantity,
                slot,
                from,
            }),
            Asking::Restock {
                item,
                quantity,
                sale,
            } => out.acts.push(Action::Store(StoreAction::Restock {
                item,
                quantity,
                sale,
            })),
            Asking::Sell {
                item,
                quantity,
                price,
            } => {
                out.acts.push(Action::Store(StoreAction::Sell {
                    item,
                    quantity,
                    price,
                }));
                self.market.new_sale.close();
            }
        }
    }
}
