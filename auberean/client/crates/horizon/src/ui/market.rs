//! The player's consignments and the window a new one is put up in.
//!
//! - **Consignments**: a row for each thing the player put up for sale (its item, its price, on
//!   sale, expired or sold). A press selects it; Collect, or carrying it to the pack, collects
//!   its coins once it sold, or its item when it did not. An item carried from the pack onto a
//!   consignment of the same kind of thing that has not sold restocks it, after a question saying
//!   what that costs; carried anywhere else on the list, it opens a new sale with it. New opens an
//!   empty new sale.
//! - **A new sale**: the item carried onto it (how many asked when it is a stack) and its price
//!   typed; Put up asks, saying what putting it up costs (its price times how many times the
//!   market's charge, at least a coin), and a yes puts it up and closes the window.
//!
//! A sale reminder lights over the menu button while a sale of the player's waits to be
//! collected; pressed, it shows them.

use auberean_client_sdk::contract::StoreAction;
use auberean_client_sdk::Action;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::Rect;
use crate::state::goods::{sale_state, Goods};
use crate::state::GameState;
use crate::ui::items::{self, Asking, Carried, Carry, Icon, Question, TileLook, Zone, TILE};
use crate::ui::shop::{icon_of, quantity, ROW};
use crate::ui::{Outcome, Tip};

/// The consignments' body and the new sale's, in layout units.
pub const CONSIGNMENTS_BODY: (f32, f32) = (480.0, ROW * 8.0 + 52.0);
pub const NEW_SALE_BODY: (f32, f32) = (340.0, 200.0);

/// How many consignment rows show at once.
pub const ROWS: usize = 8;

/// The text box's own number, for the kit's focus.
const PRICE_BOX: u32 = 0x4142_0002;

/// What a consignment's row says of it.
#[must_use]
pub const fn state_line(state: u32) -> &'static str {
    match state {
        sale_state::SOLD => "Sold",
        sale_state::EXPIRED => "Expired",
        _ => "For Sale",
    }
}

/// What it costs to put up `quantity` at `price` each, with the market's `charge`: at least a
/// coin.
#[must_use]
pub fn cost_to_put_up(price: u32, quantity: u32, charge: f32) -> u32 {
    #[allow(clippy::cast_precision_loss)]
    let total = f64::from(price as f32) * f64::from(quantity as f32) * f64::from(charge);
    u32::try_from(dereth_primitives::num::to_i32_f64(total.ceil()))
        .unwrap_or(0)
        .max(1)
}

/// The new sale being made: its item and how many, and its price typed.
#[derive(Debug, Default)]
pub struct NewSale {
    pub window: WindowState,
    pub item: Option<(u64, u32)>,
    pub price: String,
    focus: u32,
    drawn: Option<(Rect, Rect)>,
}

impl NewSale {
    /// Open it, empty or with an item.
    pub fn open(&mut self, item: Option<(u64, u32)>) {
        self.window.open = true;
        if item.is_some() {
            self.item = item;
        }
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }

    /// The price typed.
    #[must_use]
    pub fn price(&self) -> u32 {
        self.price.trim().parse().unwrap_or(0)
    }

    /// Whether its price box has the keys.
    #[must_use]
    pub const fn typing(&self) -> bool {
        self.window.open && self.focus == PRICE_BOX
    }

    /// Where it was last drawn.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.drawn.map(|(r, _)| r)
    }

    /// Where its body was last drawn.
    #[must_use]
    pub fn body(&self) -> Option<Rect> {
        self.drawn.map(|(_, b)| b)
    }
}

/// The consignments' window, the new sale, the reminder.
#[derive(Debug, Default)]
pub struct Market {
    pub consignments: WindowState,
    pub new_sale: NewSale,
    /// The consignment selected, by its number.
    pub selected: Option<u32>,
    drawn: Option<(Rect, Rect)>,
    scroll: f32,
}

impl Market {
    /// Where the consignments were last drawn, and their body.
    #[must_use]
    pub const fn drawn(&self) -> Option<(Rect, Rect)> {
        self.drawn
    }

    /// The consignments' `n`th row shown, for a body at `body`.
    #[must_use]
    pub fn row(body: Rect, n: usize, k: f32) -> Rect {
        #[allow(clippy::cast_precision_loss)]
        Rect::new(
            body.x + 12.0 * k,
            body.y + (8.0 + n as f32 * ROW) * k,
            body.w - 24.0 * k,
            (ROW - 4.0) * k,
        )
    }

    /// Draw the consignments while the game shows them; where they are.
    #[allow(clippy::too_many_lines)]
    pub fn consignments_frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        carry: &mut Carry,
        out: &mut Outcome,
        tip: &mut Option<Tip>,
    ) -> Option<Rect> {
        let Some(mine) = &state.goods.consignments else {
            self.drawn = None;
            self.consignments.open = false;
            self.selected = None;
            return None;
        };
        self.consignments.open = true;
        let k = p.scale;
        let size = kit::window_size_for(p, CONSIGNMENTS_BODY);
        let default = (
            (60.0 * k).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
        );
        let title = if mine.name.is_empty() {
            "My Consignments"
        } else {
            &mine.name
        };
        let default = crate::ui::place::clear(p, state, size, default);
        let w = kit::window(p, ctx, &mut self.consignments, title, size, default);
        if w.closed {
            out.acts.push(Action::Store(StoreAction::LeaveConsignment));
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        carry.zone(w.rect, Zone::Window);
        let list = Rect::new(w.body.x, w.body.y, w.body.w, ROW * ROWS as f32 * k);
        carry.zone(list, Zone::Consignments);
        let body = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let small = TextStyle::new(Family::Body, 12.0, 0xFFC8_C0B0).edge(ctx.colours.edge());
        let rows = mine.sales.len();
        #[allow(clippy::cast_precision_loss)]
        let content = rows as f32 * ROW * k;
        let offset = kit::scroll(p, ctx, list, content, &mut self.scroll);
        let first = dereth_primitives::num::to_i32((offset / (ROW * k)).floor())
            .max(0)
            .unsigned_abs() as usize;
        for (n, sale) in mine.sales.iter().enumerate().skip(first).take(ROWS) {
            let row = Self::row(w.body, n - first, k);
            let ordinal = sale.sale.ordinal;
            let sold = sale.sale.state == sale_state::SOLD;
            kit::list_highlight(p, row, self.selected == Some(ordinal));
            let tile_at = Rect::new(row.x, row.y + 1.0 * k, TILE * k, TILE * k);
            let icon = icon_of(sale);
            let t = items::tile(
                p,
                ctx,
                tile_at,
                TileLook {
                    icon: (!sold).then_some(&icon),
                    ..TileLook::default()
                },
            );
            if sold {
                items::coin(p, kit::icon_rect(tile_at));
            }
            let x = tile_at.right() + 8.0 * k;
            p.text(
                &body,
                x,
                row.y + 4.0 * k,
                &p.fit(&body, &sale.description, row.right() - x - 110.0 * k),
            );
            p.text(&small, x, row.y + 24.0 * k, state_line(sale.sale.state));
            p.text_in(
                &body,
                Rect::new(row.right() - 104.0 * k, row.y, 100.0 * k, row.h),
                Align::Right,
                &quantity(sale.sale.cost_quantity),
            );
            if !sold {
                carry.zone(row, Zone::Consignment { sale: ordinal });
            }
            let over_row = ctx.input.hover(&row);
            if t.over || over_row {
                *tip = Some(Tip {
                    title: sale.description.clone(),
                    lines: vec![format!(
                        "{} {}",
                        quantity(sale.sale.cost_quantity),
                        sale.cost
                    )],
                });
            }
            if t.pressed || (over_row && ctx.input.clicked(&row)) {
                self.selected = Some(ordinal);
                if sale.sale.item != 0 {
                    out.acts.push(Action::SetSelection {
                        target: sale.sale.item,
                    });
                }
                carry.press(
                    ctx,
                    Carried::Consignment {
                        sale: ordinal,
                        sold,
                    },
                    icon,
                    ctx.time,
                );
            }
        }
        let foot = w.body.bottom() - 40.0 * k;
        let collect = Rect::new(w.body.x + 12.0 * k, foot, 130.0 * k, 30.0 * k);
        let new = Rect::new(w.body.right() - 142.0 * k, foot, 130.0 * k, 30.0 * k);
        let chosen = self
            .selected
            .and_then(|s| mine.sales.iter().find(|x| x.sale.ordinal == s));
        if kit::button(p, ctx, collect, "Collect", chosen.is_some()) {
            if let Some(s) = chosen {
                out.acts.push(Action::Store(StoreAction::Collect {
                    sale: s.sale.ordinal,
                    sold: s.sale.state == sale_state::SOLD,
                }));
            }
        }
        if kit::button(p, ctx, new, "New", true) {
            self.new_sale.open(None);
        }
        Some(w.rect)
    }

    /// Draw the new sale while it is open; where it is, and a question it asks.
    pub fn new_sale_frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        goods: &Goods,
        carry: &mut Carry,
        told: &mut Vec<String>,
    ) -> (Option<Rect>, Option<Question>) {
        let n = &mut self.new_sale;
        if !n.window.open {
            n.drawn = None;
            return (None, None);
        }
        let k = p.scale;
        let size = kit::window_size_for(p, NEW_SALE_BODY);
        let default = (
            ((p.screen.0 - size.0 * k) / 2.0 + 260.0 * k).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
        );
        let w = kit::window(p, ctx, &mut n.window, "New Sale", size, default);
        if w.closed {
            n.close();
            return (Some(w.rect), None);
        }
        n.drawn = Some((w.rect, w.body));
        carry.zone(w.rect, Zone::NewSale);
        let body = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let tile_at = Rect::new(w.body.x + 12.0 * k, w.body.y + 8.0 * k, TILE * k, TILE * k);
        let item = n
            .item
            .and_then(|(id, q)| goods.item(id).map(|it| (id, q, it)));
        let icon = item.map(|(_, _, it)| Icon::of(it));
        items::tile(
            p,
            ctx,
            tile_at,
            TileLook {
                icon: icon.as_ref(),
                quantity: item.map(|(_, q, _)| q),
                ..TileLook::default()
            },
        );
        let x = tile_at.right() + 10.0 * k;
        let label = item.map_or_else(
            || "Drag an item here from your inventory to sell it.".to_owned(),
            |(_, q, it)| {
                if q == it.quantity {
                    it.name.clone()
                } else {
                    format!("{q} of {}", it.name)
                }
            },
        );
        p.text(
            &body,
            x,
            w.body.y + 20.0 * k,
            &p.fit(&body, &label, w.body.right() - x - 12.0 * k),
        );
        p.text(
            &body,
            w.body.x + 12.0 * k,
            w.body.y + 74.0 * k,
            "Price Per Unit",
        );
        let field = Rect::new(
            w.body.x + 120.0 * k,
            w.body.y + 68.0 * k,
            140.0 * k,
            28.0 * k,
        );
        let entered = kit::text_box_aligned(
            p,
            ctx,
            field,
            &mut n.price,
            &mut n.focus,
            PRICE_BOX,
            Align::Right,
        );
        n.price.retain(|c| c.is_ascii_digit());
        let quantity = item.map_or(0, |(_, q, _)| q);
        let price = n.price();
        let fee = cost_to_put_up(price, quantity, goods.consignment_charge);
        if item.is_some() && price > 0 {
            p.text(
                &body,
                w.body.x + 12.0 * k,
                w.body.y + 110.0 * k,
                &format!("Listing it costs {}.", crate::ui::inventory::coins(fee)),
            );
        }
        let put = Rect::new(
            w.body.x + 12.0 * k,
            w.body.bottom() - 40.0 * k,
            130.0 * k,
            30.0 * k,
        );
        let mut question = None;
        if kit::button(p, ctx, put, "Add Sale", true) || entered {
            match item {
                Some((id, q, it)) if price > 0 && q > 0 => {
                    question = Some(Question::new(
                        Asking::Sell {
                            item: id,
                            quantity: q,
                            price,
                        },
                        format!(
                            "Sell {} for {} each? It costs {}.",
                            it.name,
                            crate::ui::inventory::coins(price),
                            crate::ui::inventory::coins(fee)
                        ),
                    ));
                }
                _ => told.push("Choose an item and a price first.".to_owned()),
            }
        }
        (Some(w.rect), question)
    }
}

/// The sale reminder over the menu button, while a sale of the player's waits; where it is.
pub fn reminder(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    state: &GameState,
    out: &mut Outcome,
) -> Option<Rect> {
    if !state.goods.sale_waiting {
        return None;
    }
    let k = p.scale;
    let r = reminder_rect(p.screen, k);
    if kit::button_lit(p, ctx, r, "Sold!", true, true) {
        out.acts.push(Action::SaleReminders);
    }
    Some(r)
}

/// Where the sale reminder is on a screen drawn at `k` pixels a layout unit: over the menu
/// button.
#[must_use]
pub fn reminder_rect(screen: (f32, f32), k: f32) -> Rect {
    Rect::new(
        screen.0 - 104.0 * k,
        screen.1 - 96.0 * k,
        88.0 * k,
        28.0 * k,
    )
}
