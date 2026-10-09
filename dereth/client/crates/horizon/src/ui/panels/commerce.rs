//! Buying and selling at a vendor, and the secure trade between two players, in the interface's
//! shop and trade windows, doing what the game's own vendor and trade panels do.
//!
//! The vendor: the stock under the vendor's own type filters, a quantity for the selected row,
//! Buy (at once) or Add to List, the basket with Buy All and its clears; on the Sell tab, the
//! items dropped there and Sell Item, Sell All and the clears. Closing the window closes the
//! vendor. The trade: what each side offers, an item dropped on your side offered, and Accept,
//! Decline, Clear and Close.

use dereth_client_contract::view::{ShopRow, TradeRow};
use dereth_client_contract::UiRequest;
use dereth_primitives::ObjectId;
use dereth_ui_screens::panels::vendor as retail;

use super::{WindowId, Windows};
use crate::art::Family;
use crate::draw::{Rect, WHITE};
use crate::ui::game::{GameState, Item};
use crate::ui::kit::{self, Ctx, Drop};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// One row of a list: its object, icon, name and the words at its right.
struct Row {
    item: ObjectId,
    icon: Option<u32>,
    name: String,
    right: String,
}

fn shop_row(r: &ShopRow) -> Row {
    Row {
        item: r.item,
        icon: r.icon.map(|d| d.0),
        name: if r.amount > 1 && r.max_stack_size > 1 {
            format!("{} x{}", r.name, r.amount)
        } else {
            r.name.clone()
        },
        right: format!("{}p", retail_commas(r.price)),
    }
}

fn trade_row(r: &TradeRow) -> Row {
    Row {
        item: r.item,
        icon: r.icon.map(|d| d.0),
        name: r.name.clone(),
        right: String::new(),
    }
}

fn retail_commas(v: i32) -> String {
    dereth_ui_screens::panels::examination::insert_commas(v)
}

/// A scrolling list of rows; the index clicked and the index double-clicked, if any.
fn list(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    area: Rect,
    rows: &[Row],
    looks: &[Item],
    selected: Option<ObjectId>,
    scroll: &mut f32,
) -> (Option<usize>, Option<usize>) {
    let k = p.scale;
    let row_h = 34.0 * k;
    p.fill(area, 0x6010_0C08);
    #[allow(clippy::cast_precision_loss)]
    let content = row_h * rows.len() as f32;
    let offset = kit::scroll(p, ctx, area, content, scroll);
    // While the list scrolls, the rows stop short of its bar.
    let bar = if content > area.h {
        kit::scrollbar_width(p).map_or(6.0 * k, |w| (w + 4.0) * k)
    } else {
        6.0 * k
    };
    let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
    let price = TextStyle::new(Family::Body, 12.0, 0xFFE8_D8A0).edge(ctx.colours.edge());
    let (mut clicked, mut doubled) = (None, None);
    p.list.push_clip(area);
    for (i, row) in rows.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let r = Rect::new(
            area.x,
            area.y + i as f32 * row_h - offset,
            area.w - bar,
            row_h,
        );
        if r.bottom() < area.y || r.y > area.bottom() {
            continue;
        }
        let visible = Rect::new(
            r.x,
            r.y.max(area.y),
            r.w,
            r.bottom().min(area.bottom()) - r.y.max(area.y),
        );
        let on = selected == Some(row.item);
        // Each row is a stop for the pad.
        let over = ctx.over(&visible);
        if on || over {
            crate::ui::pregame::list_highlight(p, r, on);
        }
        let icon_r = Rect::new(r.x + 4.0 * k, r.y + 3.0 * k, 28.0 * k, 28.0 * k);
        // The object's tile as the item lists compose it, or its bare icon.
        let icon = looks
            .iter()
            .find(|l| l.id == row.item)
            .and_then(|l| p.art.ac_item(l))
            .or_else(|| row.icon.and_then(|d| p.art.ac_icon(d)));
        if let Some(icon) = icon {
            p.sprite(&icon, icon_r, WHITE);
        }
        p.text_in(
            &name,
            Rect::new(icon_r.right() + 8.0 * k, r.y, r.w * 0.62, r.h),
            Align::Left,
            &row.name,
        );
        if !row.right.is_empty() {
            p.text_in(&price, r.offset(-8.0 * k, 0.0), Align::Right, &row.right);
        }
        if ctx.input.double_clicked(&visible) {
            doubled = Some(i);
        } else if ctx.input.clicked(&visible) {
            clicked = Some(i);
        }
    }
    p.list.pop_clip();
    if ctx.over_quiet(&area) {
        ctx.hot = true;
    }
    (clicked, doubled)
}

impl Windows {
    /// The vendor and trade windows follow the game: each opens with the game's shop or trade,
    /// closes with it, and closing the window closes it in the game.
    pub(super) fn follow_commerce(
        &mut self,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let shop = state.shop.as_ref().is_some_and(|s| s.open);
        if shop && !self.vendor_shown {
            self.open(WindowId::Vendor, ctx.time);
            self.vendor_shown = true;
            self.vendor_tab = usize::from(state.shop.as_ref().is_some_and(|s| s.sell_mode));
        } else if !shop && self.vendor_shown {
            self.close(WindowId::Vendor);
            self.vendor_shown = false;
        } else if shop && !self.is_open(WindowId::Vendor) {
            out.requests.push(UiRequest::VendorClose);
            self.vendor_shown = false;
        }
        if state.shop_buying && shop {
            self.open(WindowId::Vendor, ctx.time);
            self.vendor_tab = 0;
        }
        let trade = state.trade.as_ref().is_some_and(|t| t.open);
        if trade && !self.trade_shown {
            self.open(WindowId::Trade, ctx.time);
            self.trade_shown = true;
        } else if !trade && self.trade_shown {
            self.close(WindowId::Trade);
            self.trade_shown = false;
        } else if trade && !self.is_open(WindowId::Trade) {
            out.requests.push(UiRequest::TradeClose);
            self.trade_shown = false;
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn vendor(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let Some(shop) = state.shop.as_ref() else {
            return;
        };
        let k = p.scale;
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let text = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        // Something of the player's dragged over the window turns it to selling.
        let selling_drag = ctx
            .drag
            .as_ref()
            .is_some_and(|d| d.active && !shop.stock.iter().any(|r| r.item == d.item));
        if selling_drag && self.vendor_tab != 1 && ctx.over(&body) {
            self.vendor_tab = 1;
            self.vendor_filter_menu = None;
        }
        // The open category list takes the presses over it before the stock under it, and a
        // press anywhere else closes it.
        let occluders = ctx.input.occluders.len();
        let categories = shop.type_filters.len() + 1;
        if let Some((anchor, _)) = self.vendor_filter_menu {
            if self.vendor_tab != 0 || kit::dropdown_dismissed(p, ctx, anchor, categories) {
                self.vendor_filter_menu = None;
            } else {
                ctx.input
                    .occluders
                    .push(kit::dropdown_list_rect(p, anchor, categories));
            }
        }
        kit::tabs(
            p,
            ctx,
            Rect::new(body.x, body.y, body.w, 26.0 * k),
            &["Buy", "Sell"],
            &mut self.vendor_tab,
        );
        p.text_in(
            &dim,
            Rect::new(body.x, body.y, body.w - 6.0 * k, 26.0 * k),
            Align::Right,
            &retail::purse_line(shop.total_value),
        );
        let top = body.y + 34.0 * k;
        let foot = body.bottom() - 34.0 * k;
        let half = (body.w - 12.0 * k) / 2.0;
        let left = Rect::new(body.x, top, half, foot - top - 70.0 * k);
        let right = Rect::new(
            body.x + half + 12.0 * k,
            top + 20.0 * k,
            half,
            foot - top - 90.0 * k,
        );
        if self.vendor_tab == 0 {
            // The vendor's own type filters, as a row of tabs over the stock.
            let names: Vec<&str> = std::iter::once("All")
                .chain(shop.type_filters.iter().map(|(n, _)| *n))
                .collect();
            if self.vendor_filter >= names.len() {
                self.vendor_filter = 0;
            }
            let filter_row = Rect::new(left.x, left.y, left.w, 24.0 * k);
            let filter = self.vendor_filter.min(names.len().saturating_sub(1));
            if names.len() > 2 {
                // The vendor's categories in a dropdown box: some vendors have a great many.
                let open = self.vendor_filter_menu.is_some();
                if kit::dropdown_box(p, ctx, filter_row, names[filter], open, true) {
                    self.vendor_filter_menu = if open { None } else { Some((filter_row, 0)) };
                }
            }
            let mask = (self.vendor_filter > 0)
                .then(|| {
                    shop.type_filters
                        .get(self.vendor_filter - 1)
                        .map(|(_, m)| *m)
                })
                .flatten();
            // The stock, less what the basket already holds of a finite row, and without
            // containers that hold anything.
            let mut sizes = Vec::new();
            let (selected_item, quantity) = (self.vendor_selected, self.vendor_quantity);
            let stock: Vec<&ShopRow> = shop
                .stock
                .iter()
                .filter(|r| mask.is_none_or(|m| r.obj_type & m != 0))
                .filter(|r| {
                    let in_basket: i32 = shop
                        .buy_list
                        .iter()
                        .filter(|b| b.item == r.item)
                        .map(|b| b.amount)
                        .sum();
                    let remaining = if r.amount < 0 {
                        -1
                    } else {
                        r.amount - in_basket
                    };
                    if r.amount >= 0 && in_basket > 0 && remaining <= 0 {
                        return false;
                    }
                    if r.max_stack_size > 1 {
                        let top = i32::try_from(r.max_stack_size).unwrap_or(i32::MAX);
                        let top = if remaining < 0 {
                            top
                        } else {
                            remaining.min(top)
                        };
                        // The chosen row is sized to the quantity chosen, so what the basket
                        // adds, counts and costs is that many.
                        let size = if Some(r.item) == selected_item {
                            i32::try_from(quantity.max(1)).unwrap_or(1).min(top.max(1))
                        } else {
                            top
                        };
                        sizes.push((r.item, size));
                    }
                    r.contained_items == 0 && r.contained_containers == 0
                })
                .collect();
            // The rows' stack sizes, as the game's list sets them, sent when they change.
            if sizes != self.vendor_sizes {
                for (item, size) in &sizes {
                    out.requests.push(UiRequest::VendorSetObjectStackSize {
                        item: *item,
                        size: *size,
                    });
                }
                self.vendor_sizes = sizes;
            }
            let rows: Vec<Row> = stock.iter().map(|r| shop_row(r)).collect();
            let list_area = Rect::new(left.x, left.y + 30.0 * k, left.w, left.h - 30.0 * k);
            let (clicked, doubled) = list(
                p,
                ctx,
                list_area,
                &rows,
                &state.looks,
                self.vendor_selected,
                &mut self.vendor_scroll,
            );
            if let Some(i) = clicked {
                self.vendor_selected = Some(rows[i].item);
                self.vendor_quantity = 1;
                // With the pad, confirming on a row of the stock puts one on the shopping list.
                if ctx.input.pad.mode.is_some() {
                    out.requests.push(UiRequest::VendorAddToBuyList {
                        item: rows[i].item,
                        split: 1,
                    });
                }
            }
            if let Some(i) = doubled {
                out.requests.push(UiRequest::VendorBuySingle {
                    item: rows[i].item,
                    split: 1,
                });
            }
            // The selected row, its quantity and its cost.
            let selected = stock
                .iter()
                .find(|r| Some(r.item) == self.vendor_selected)
                .copied();
            let info_y = left.bottom() + 8.0 * k;
            if let Some(r) = selected {
                let max = if r.max_stack_size > 1 {
                    let top = r.max_stack_size;
                    if r.amount < 0 {
                        top
                    } else {
                        top.min(u32::try_from(r.amount).unwrap_or(1)).max(1)
                    }
                } else {
                    1
                };
                self.vendor_quantity = self.vendor_quantity.clamp(1, max);
                let q = self.vendor_quantity;
                p.text(
                    &text,
                    left.x,
                    info_y,
                    &retail::item_name_line(&r.name, None, q),
                );
                p.text(
                    &dim,
                    left.x,
                    info_y + 18.0 * k,
                    &retail::item_cost_line(
                        r.price.saturating_mul(i32::try_from(q).unwrap_or(1)),
                        shop.total_value,
                        q,
                    ),
                );
                if max > 1 {
                    let minus = Rect::new(left.right() - 120.0 * k, info_y, 26.0 * k, 22.0 * k);
                    let shown = Rect::new(minus.right() + 2.0 * k, info_y, 60.0 * k, 22.0 * k);
                    let plus = Rect::new(shown.right() + 2.0 * k, info_y, 26.0 * k, 22.0 * k);
                    let step = if ctx.input.shift { 10 } else { 1 };
                    if kit::button(p, ctx, minus, "-", q > 1) {
                        self.vendor_quantity = q.saturating_sub(step).max(1);
                    }
                    p.fill(shown, 0x8010_0C08);
                    p.text_in(&text, shown, Align::Centre, &q.to_string());
                    if kit::button(p, ctx, plus, "+", q < max) {
                        self.vendor_quantity = (q + step).min(max);
                    }
                }
            }
            let item = selected.map(|r| r.item);
            let split = i32::try_from(self.vendor_quantity).unwrap_or(1);
            let by = foot;
            let bw = 104.0 * k;
            if kit::button(
                p,
                ctx,
                Rect::new(left.x, by, bw, 28.0 * k),
                "Buy",
                item.is_some(),
            ) {
                if let Some(item) = item {
                    out.requests
                        .push(UiRequest::VendorBuySingle { item, split });
                }
            }
            if kit::button(
                p,
                ctx,
                Rect::new(left.x + bw + 8.0 * k, by, bw + 20.0 * k, 28.0 * k),
                "Add to List",
                item.is_some(),
            ) {
                if let Some(item) = item {
                    out.requests
                        .push(UiRequest::VendorAddToBuyList { item, split });
                }
            }
            // The basket.
            p.text(&dim, right.x, right.y - 20.0 * k, "Shopping list");
            let basket: Vec<Row> = shop.buy_list.iter().map(shop_row).collect();
            let (clicked, _) = list(
                p,
                ctx,
                right,
                &basket,
                &state.looks,
                self.basket_selected,
                &mut self.basket_scroll,
            );
            if let Some(i) = clicked {
                self.basket_selected = Some(basket[i].item);
            }
            p.text(
                &text,
                right.x,
                right.bottom() + 8.0 * k,
                &retail::transaction_line("Buying", shop.buy_items, shop.buy_transaction),
            );
            let rx = right.x;
            if kit::button(
                p,
                ctx,
                Rect::new(rx, by, bw, 28.0 * k),
                "Buy All",
                !basket.is_empty(),
            ) {
                out.requests.push(UiRequest::VendorBuyAll);
            }
            let chosen = self
                .basket_selected
                .filter(|id| shop.buy_list.iter().any(|r| r.item == *id));
            if kit::button(
                p,
                ctx,
                Rect::new(rx + bw + 8.0 * k, by, bw, 28.0 * k),
                "Remove",
                chosen.is_some(),
            ) {
                out.requests.push(UiRequest::VendorClearList {
                    sell: false,
                    item: chosen,
                });
            }
            if kit::button(
                p,
                ctx,
                Rect::new(rx + 2.0 * (bw + 8.0 * k), by, bw - 20.0 * k, 28.0 * k),
                "Clear",
                !basket.is_empty(),
            ) {
                out.requests.push(UiRequest::VendorClearList {
                    sell: false,
                    item: None,
                });
            }
        } else {
            // Selling: drop items from the inventory here.
            let area = Rect::new(body.x, top + 20.0 * k, body.w, foot - top - 60.0 * k);
            p.text(
                &dim,
                area.x,
                top,
                "Drag items here from your inventory to sell them.",
            );
            ctx.drops.push((area, Some(Drop::Sell)));
            let rows: Vec<Row> = shop.sell_list.iter().map(shop_row).collect();
            let (clicked, _) = list(
                p,
                ctx,
                area,
                &rows,
                &state.looks,
                self.basket_selected,
                &mut self.basket_scroll,
            );
            if let Some(i) = clicked {
                self.basket_selected = Some(rows[i].item);
            }
            p.text(
                &text,
                area.x,
                area.bottom() + 8.0 * k,
                &retail::transaction_line("Selling", shop.sell_items, shop.sell_transaction),
            );
            let chosen = self
                .basket_selected
                .filter(|id| shop.sell_list.iter().any(|r| r.item == *id));
            let bw = 110.0 * k;
            let by = foot;
            if kit::button(
                p,
                ctx,
                Rect::new(body.x, by, bw, 28.0 * k),
                "Sell Item",
                chosen.is_some(),
            ) {
                if let Some(item) = chosen {
                    out.requests.push(UiRequest::VendorSellSingle { item });
                }
            }
            if kit::button(
                p,
                ctx,
                Rect::new(body.x + bw + 8.0 * k, by, bw, 28.0 * k),
                "Sell All",
                !rows.is_empty(),
            ) {
                out.requests.push(UiRequest::VendorSellAll);
            }
            if kit::button(
                p,
                ctx,
                Rect::new(body.x + 2.0 * (bw + 8.0 * k), by, bw, 28.0 * k),
                "Remove",
                chosen.is_some(),
            ) {
                out.requests.push(UiRequest::VendorClearList {
                    sell: true,
                    item: chosen,
                });
            }
            if kit::button(
                p,
                ctx,
                Rect::new(body.x + 3.0 * (bw + 8.0 * k), by, bw, 28.0 * k),
                "Clear",
                !rows.is_empty(),
            ) {
                out.requests.push(UiRequest::VendorClearList {
                    sell: true,
                    item: None,
                });
            }
        }
        ctx.input.occluders.truncate(occluders);
        // The open category list, over the stock.
        if let Some((anchor, top)) = self.vendor_filter_menu.as_mut() {
            let names: Vec<&str> = std::iter::once("All")
                .chain(shop.type_filters.iter().map(|(n, _)| *n))
                .collect();
            let at = self.vendor_filter.min(names.len().saturating_sub(1));
            if let Some(i) = kit::dropdown_list(p, ctx, *anchor, &names, at, top) {
                self.vendor_filter = i;
                self.vendor_filter_menu = None;
                self.vendor_scroll = 0.0;
            }
        }
    }

    pub(super) fn trade(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let Some(t) = state.trade.as_ref() else {
            return;
        };
        let k = p.scale;
        let head = TextStyle::new(Family::Body, 14.0, 0xFFFF_FFFF).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let half = (body.w - 12.0 * k) / 2.0;
        let top = body.y + 26.0 * k;
        let foot = body.bottom() - 34.0 * k;
        let mine = Rect::new(body.x, top, half, foot - top - 34.0 * k);
        let theirs = Rect::new(body.x + half + 12.0 * k, top, half, foot - top - 34.0 * k);
        p.text(&head, mine.x, body.y + 2.0 * k, "Your offer");
        p.text(
            &head,
            theirs.x,
            body.y + 2.0 * k,
            if t.partner_name.is_empty() {
                "Their offer"
            } else {
                &t.partner_name
            },
        );
        ctx.drops.push((
            mine,
            Some(Drop::Trade(u32::try_from(t.self_rows.len()).unwrap_or(0))),
        ));
        let my_rows: Vec<Row> = t.self_rows.iter().map(trade_row).collect();
        let their_rows: Vec<Row> = t.partner_rows.iter().map(trade_row).collect();
        let _ = list(
            p,
            ctx,
            mine,
            &my_rows,
            &state.looks,
            None,
            &mut self.trade_scroll.0,
        );
        let _ = list(
            p,
            ctx,
            theirs,
            &their_rows,
            &state.looks,
            None,
            &mut self.trade_scroll.1,
        );
        let ok = 0xFF7C_E07C;
        for (r, accepted) in [(mine, t.accepted), (theirs, t.partner_accepted)] {
            let s = if accepted { dim.colour(ok) } else { dim };
            p.text(
                &s,
                r.x,
                r.bottom() + 8.0 * k,
                if accepted {
                    "Accepted"
                } else {
                    "Not yet accepted"
                },
            );
        }
        let bw = 110.0 * k;
        if kit::button(
            p,
            ctx,
            Rect::new(body.x, foot, bw, 28.0 * k),
            "Accept",
            !t.accepted,
        ) {
            out.requests.push(UiRequest::TradeAccept {
                displayed_self: t.self_rows.len(),
                displayed_partner: t.partner_rows.len(),
            });
        }
        if kit::button(
            p,
            ctx,
            Rect::new(body.x + bw + 8.0 * k, foot, bw, 28.0 * k),
            "Decline",
            t.accepted,
        ) {
            out.requests.push(UiRequest::TradeDecline);
        }
        if kit::button(
            p,
            ctx,
            Rect::new(body.x + 2.0 * (bw + 8.0 * k), foot, bw, 28.0 * k),
            "Clear",
            !t.self_rows.is_empty(),
        ) {
            out.requests.push(UiRequest::TradeReset);
        }
        if kit::button(
            p,
            ctx,
            Rect::new(body.right() - bw, foot, bw, 28.0 * k),
            "Close",
            true,
        ) {
            out.requests.push(UiRequest::TradeClose);
        }
    }
}
