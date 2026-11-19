//! The trade window: the player's offers and the partner's side by side, each side accepted or
//! not.
//!
//! It opens when the trade does (the game's question asks whether to trade when the partner
//! asks), and while the player waits for the partner to answer its own ask. An item carried from
//! the pack onto the player's side is offered (a stack asking how many first); one of the player's
//! offers carried out of it and put down anywhere else is taken back. Accept and Decline wait for
//! the game's answer once pressed. Closing it closes the trade; when the partner closed it, the
//! window says so until the player closes it.

use auberean_client_sdk::contract::TradeAction;
use auberean_client_sdk::Action;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::Rect;
use crate::state::goods::{Offer, Trade};
use crate::state::GameState;
use crate::ui::items::{self, Carried, Carry, Icon, TileLook, Zone, STEP, TILE};
use crate::ui::{Outcome, Tip};

/// How many tiles a side's row holds, and its rows.
pub const COLUMNS: u32 = 4;
pub const ROWS: u32 = 4;

/// The window's body, in layout units.
pub const BODY: (f32, f32) = (24.0 + 2.0 * STEP * 4.0 + 24.0, 300.0);

/// The mark of a side that accepted.
pub const ACCEPTED: u32 = 0xFF60_D060;

/// What the trade window asks besides the game's actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settle {
    /// Accept or decline was pressed.
    Answering,
    /// The window closed after the partner closed the trade.
    Dismissed,
}

/// Where its parts are for a body at `body`, `k` pixels a layout unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// Each side's first tile.
    pub mine: (f32, f32),
    pub theirs: (f32, f32),
    /// The player's side, which takes what is offered.
    pub offer: Rect,
    pub accept: Rect,
    pub decline: Rect,
}

impl Layout {
    #[must_use]
    pub fn of(body: Rect, k: f32) -> Self {
        let side = STEP * COLUMNS as f32;
        let mine = (body.x + 12.0 * k, body.y + 30.0 * k);
        let theirs = (body.x + (36.0 + side) * k, body.y + 30.0 * k);
        let foot = body.bottom() - 40.0 * k;
        Self {
            mine,
            theirs,
            offer: Rect::new(mine.0, mine.1, side * k, STEP * ROWS as f32 * k),
            accept: Rect::new(body.x + 12.0 * k, foot, 130.0 * k, 30.0 * k),
            decline: Rect::new(body.right() - 142.0 * k, foot, 130.0 * k, 30.0 * k),
        }
    }

    /// A side's `n`th tile, from its first.
    #[must_use]
    pub fn cell(first: (f32, f32), n: u32, k: f32) -> Rect {
        #[allow(clippy::cast_precision_loss)]
        let (col, row) = ((n % COLUMNS) as f32, (n / COLUMNS) as f32);
        Rect::new(
            first.0 + col * STEP * k,
            first.1 + row * STEP * k,
            TILE * k,
            TILE * k,
        )
    }
}

/// The trade window.
#[derive(Debug, Default)]
pub struct TradeWindow {
    pub window: WindowState,
    drawn: Option<(Rect, Rect)>,
    /// The trade shown was closed by the partner.
    ended: bool,
}

impl TradeWindow {
    /// Where it was last drawn, while it is shown.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.drawn.map(|(r, _)| r)
    }

    /// Its parts as last drawn.
    #[must_use]
    pub fn layout(&self, k: f32) -> Option<Layout> {
        self.drawn.map(|(_, b)| Layout::of(b, k))
    }

    /// Whether it was shown last frame: and if so, whether the partner had closed the trade.
    #[must_use]
    pub const fn shown(&self) -> Option<bool> {
        match self.drawn {
            Some(_) => Some(self.ended),
            None => None,
        }
    }

    /// Whether `trade` shows: open, waited for, or closed by the partner.
    #[must_use]
    pub const fn shows(trade: &Trade) -> bool {
        trade.open || trade.asked_by_player || trade.ended_by_partner
    }

    /// Draw it while there is a trade to show; where it is.
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        carry: &mut Carry,
        out: &mut Outcome,
        settle: &mut Vec<Settle>,
        tip: &mut Option<Tip>,
    ) -> Option<Rect> {
        let Some(trade) = state.goods.trade.as_ref().filter(|t| Self::shows(t)) else {
            self.drawn = None;
            self.window.open = false;
            return None;
        };
        self.window.open = true;
        self.ended = trade.ended_by_partner;
        let k = p.scale;
        let size = kit::window_size_for(p, BODY);
        let default = (
            ((p.screen.0 - size.0 * k) / 2.0).round(),
            ((p.screen.1 - size.1 * k) / 2.0 - 60.0 * k).round(),
        );
        let partner = if trade.partner_name.is_empty() {
            "someone"
        } else {
            &trade.partner_name
        };
        let default = crate::ui::place::clear(p, state, size, default);
        let w = kit::window(
            p,
            ctx,
            &mut self.window,
            &format!("Trade with {partner}"),
            size,
            default,
        );
        if w.closed {
            close(trade, out, settle);
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        carry.zone(w.rect, Zone::Window);
        let l = Layout::of(w.body, k);
        let heading = TextStyle::new(Family::Heading, 14.0, ctx.colours.heading());
        let body = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let side = STEP * COLUMNS as f32 * k;
        for (first, title, accepted) in [
            (l.mine, "Your offer".to_owned(), trade.mine_accepted),
            (l.theirs, "Their offer".to_owned(), trade.theirs_accepted),
        ] {
            let head = Rect::new(first.0, first.1 - 24.0 * k, side, 20.0 * k);
            p.text_in(&heading, head, Align::Left, &title);
            if accepted {
                p.text_in(
                    &TextStyle::new(Family::Body, 13.0, ACCEPTED).edge(ctx.colours.edge()),
                    head,
                    Align::Right,
                    "Accepted",
                );
                p.outline(
                    Rect::new(
                        first.0 - 3.0 * k,
                        first.1 - 3.0 * k,
                        side + 4.0 * k,
                        STEP * ROWS as f32 * k + 4.0 * k,
                    ),
                    2.0 * k,
                    ACCEPTED,
                );
            }
        }
        if !trade.open {
            let line = if trade.ended_by_partner {
                format!("{partner} closed the trade.")
            } else {
                format!("Waiting for {partner} to answer.")
            };
            p.text_in(
                &body,
                Rect::new(
                    w.body.x + 12.0 * k,
                    w.body.y + 120.0 * k,
                    w.body.w - 24.0 * k,
                    20.0 * k,
                ),
                Align::Centre,
                &line,
            );
            let label = if trade.ended_by_partner {
                "Close"
            } else {
                "Cancel"
            };
            if kit::button(p, ctx, l.decline, label, true) {
                close(trade, out, settle);
            }
            return Some(w.rect);
        }
        carry.zone(l.offer, Zone::TradeOffer);
        let goods = &state.goods;
        for (first, offers, mine) in [
            (l.mine, &trade.mine, true),
            (l.theirs, &trade.theirs, false),
        ] {
            for n in 0..COLUMNS * ROWS {
                let r = Layout::cell(first, n, k);
                let offer: Option<&Offer> = offers.get(n as usize);
                let item = offer.and_then(|o| goods.item(o.item).map(|it| (o, it)));
                let icon = item.map(|(_, it)| Icon::of(it));
                let t = items::tile(
                    p,
                    ctx,
                    r,
                    TileLook {
                        icon: icon.as_ref(),
                        quantity: offer.map(|o| o.quantity),
                        selected: offer.is_some_and(|o| o.item == goods.selection),
                        dim: offer.is_some() && item.is_none(),
                        ..TileLook::default()
                    },
                );
                if let (true, Some((_, it))) = (t.over, item) {
                    *tip = Some(Tip {
                        title: it.offered_name.clone().unwrap_or_else(|| it.name.clone()),
                        lines: Vec::new(),
                    });
                }
                if let (true, Some(o)) = (t.examine, offer) {
                    carry.examine = Some(o.item);
                }
                if let (true, Some(o)) = (t.pressed, offer) {
                    out.acts.push(Action::SetSelection { target: o.item });
                    if mine {
                        let icon = item.map(|(_, it)| Icon::of(it)).unwrap_or_default();
                        carry.press(ctx, Carried::Offered { item: o.item }, icon, ctx.time);
                    }
                }
            }
        }
        let free = !trade.answering;
        let accept = if trade.mine_accepted {
            "Accepted"
        } else {
            "Accept"
        };
        if kit::button_lit(
            p,
            ctx,
            l.accept,
            accept,
            free && !trade.mine_accepted,
            trade.mine_accepted,
        ) {
            out.acts.push(Action::Trade(TradeAction::Accept));
            settle.push(Settle::Answering);
        }
        if kit::button(p, ctx, l.decline, "Decline", free && trade.mine_accepted) {
            out.acts.push(Action::Trade(TradeAction::Decline));
            settle.push(Settle::Answering);
        }
        Some(w.rect)
    }
}

/// The window closed: the trade closed, or, when the partner had closed it, only the window.
fn close(trade: &Trade, out: &mut Outcome, settle: &mut Vec<Settle>) {
    if trade.ended_by_partner {
        settle.push(Settle::Dismissed);
    } else {
        out.acts.push(Action::Trade(TradeAction::Close));
    }
}
