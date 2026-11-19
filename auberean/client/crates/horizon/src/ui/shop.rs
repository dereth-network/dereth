//! The shop window and the market's catalog.
//!
//! - **The shop**: its name and description, and a row for each sale of the page the game shows
//!   (what is sold, what it costs and how much, how many are left). A press on a row selects its
//!   item; a second press soon after, or Buy, buys it: one, or as many as the player says when it
//!   may buy more than one; the game then asks the player to confirm. A sale carried to the pack
//!   is bought into that slot. A sale restricted to others, or with none left, can't be bought,
//!   and one the player can't afford says so. The arrows show the page before and the next.
//! - **The catalog**: the market's shops, each by its name and how many sales it has. A press
//!   selects one; a second press, or Enter, goes into it. My consignments shows the player's own.

use auberean_client_sdk::contract::StoreAction;
use auberean_client_sdk::Action;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::{Rect, WHITE};
use crate::state::goods::{sale_state, SaleShown};
use crate::state::GameState;
use crate::ui::items::{self, Carried, Carry, Icon, Press, TileLook, Zone, DOUBLE_PRESS, TILE};
use crate::ui::{Outcome, Tip};

/// How many sales a page of the game's shows.
pub const PAGE: i32 = 7;

/// A row's height, in layout units.
pub const ROW: f32 = 50.0;

/// The shop window's body, and the catalog's, in layout units.
pub const SHOP_BODY: (f32, f32) = (480.0, 60.0 + ROW * 7.0 + 48.0);
pub const CATALOG_BODY: (f32, f32) = (420.0, 40.0 + ROW * 7.0 + 48.0);

/// A sale's icon.
#[must_use]
pub fn icon_of(s: &SaleShown) -> Icon {
    Icon {
        layers: s.sale.icon_layers.clone(),
        picture: s.sale.icon,
    }
}

/// What a sale's row says of how many are left (a stock below none: no end to it).
#[must_use]
pub fn stock_line(s: &SaleShown) -> String {
    if s.sale.state == sale_state::RESTRICTED {
        "Restricted".to_owned()
    } else if s.sale.stock == 0 {
        "Sold Out".to_owned()
    } else if s.sale.stock < 0 {
        "Unlimited".to_owned()
    } else {
        format!("{} left", s.sale.stock)
    }
}

/// What a shop's window asks of the windows of things besides the game's actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    /// Buy a sale's item into a slot of the pack (0: wherever).
    Buy { item: u64, slot: u32 },
}

/// Where a shop window's parts are for a body at `body`, `k` pixels a layout unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShopLayout {
    /// The first row (the rest follow by [`ROW`]).
    pub rows: (f32, f32),
    pub previous: Rect,
    pub next: Rect,
    pub buy: Rect,
}

impl ShopLayout {
    #[must_use]
    pub fn of(body: Rect, k: f32) -> Self {
        let foot = body.bottom() - 40.0 * k;
        Self {
            rows: (body.x + 12.0 * k, body.y + 52.0 * k),
            previous: Rect::new(body.x + 12.0 * k, foot, 40.0 * k, 30.0 * k),
            next: Rect::new(body.x + 58.0 * k, foot, 40.0 * k, 30.0 * k),
            buy: Rect::new(body.right() - 142.0 * k, foot, 130.0 * k, 30.0 * k),
        }
    }

    /// The `n`th row.
    #[must_use]
    pub fn row(&self, n: usize, width: f32, k: f32) -> Rect {
        #[allow(clippy::cast_precision_loss)]
        Rect::new(
            self.rows.0,
            self.rows.1 + n as f32 * ROW * k,
            width - 24.0 * k,
            (ROW - 4.0) * k,
        )
    }
}

/// The shop window and the catalog.
#[derive(Debug, Default)]
pub struct Shops {
    pub shop: WindowState,
    pub catalog: WindowState,
    /// The sale selected, by its item; the shop chosen in the catalog, by its id.
    pub selected: Option<u64>,
    pub chosen: Option<u32>,
    last_press: Option<(u32, f64)>,
    shop_drawn: Option<(Rect, Rect)>,
    catalog_drawn: Option<(Rect, Rect)>,
}

impl Shops {
    /// Where the shop window was last drawn, and its body.
    #[must_use]
    pub const fn shop_drawn(&self) -> Option<(Rect, Rect)> {
        self.shop_drawn
    }

    /// Where the catalog was last drawn, and its body.
    #[must_use]
    pub const fn catalog_drawn(&self) -> Option<(Rect, Rect)> {
        self.catalog_drawn
    }

    /// Draw the shop while the game shows one; where it is.
    #[allow(clippy::too_many_arguments)]
    pub fn shop_frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        carry: &mut Carry,
        out: &mut Outcome,
        asked: &mut Vec<Asked>,
        tip: &mut Option<Tip>,
    ) -> Option<Rect> {
        let Some(shop) = &state.goods.shop else {
            self.shop_drawn = None;
            self.shop.open = false;
            self.selected = None;
            return None;
        };
        self.shop.open = true;
        let k = p.scale;
        let size = kit::window_size_for(p, SHOP_BODY);
        let default = (
            (40.0 * k).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
        );
        let title = if shop.name.is_empty() {
            "Shop"
        } else {
            &shop.name
        };
        let default = crate::ui::place::clear(p, state, size, default);
        let w = kit::window(p, ctx, &mut self.shop, title, size, default);
        if w.closed {
            out.acts.push(Action::Store(StoreAction::LeaveStore));
            return Some(w.rect);
        }
        self.shop_drawn = Some((w.rect, w.body));
        carry.zone(w.rect, Zone::Window);
        let l = ShopLayout::of(w.body, k);
        let body = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        for (i, line) in p
            .wrap(&body, &shop.description, w.body.w - 24.0 * k)
            .iter()
            .take(2)
            .enumerate()
        {
            #[allow(clippy::cast_precision_loss)]
            p.text(
                &body,
                w.body.x + 12.0 * k,
                w.body.y + (6.0 + 17.0 * i as f32) * k,
                line,
            );
        }
        for (n, sale) in shop
            .sales
            .iter()
            .take(usize::try_from(PAGE).unwrap_or(7))
            .enumerate()
        {
            let row = l.row(n, w.body.w, k);
            let item = sale.sale.item;
            let selected = self.selected == Some(item);
            kit::list_highlight(p, row, selected);
            let tile_at = Rect::new(row.x, row.y + 1.0 * k, TILE * k, TILE * k);
            let icon = icon_of(sale);
            let t = items::tile(
                p,
                ctx,
                tile_at,
                TileLook {
                    icon: Some(&icon),
                    dim: sale.unavailable(),
                    ..TileLook::default()
                },
            );
            let text_x = tile_at.right() + 8.0 * k;
            let desc = p.fit(&body, &sale.description, row.right() - text_x - 120.0 * k);
            p.text(&body, text_x, row.y + 4.0 * k, &desc);
            let small = TextStyle::new(Family::Body, 12.0, 0xFFC8_C0B0).edge(ctx.colours.edge());
            p.text(&small, text_x, row.y + 24.0 * k, &stock_line(sale));
            cost(
                p,
                &body,
                sale,
                Rect::new(row.right() - 116.0 * k, row.y, 116.0 * k, row.h),
            );
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
            let pressed = t.pressed || (over_row && ctx.input.clicked(&row));
            if pressed {
                self.selected = Some(item);
                out.acts.push(Action::SetSelection { target: item });
                let press = carry.press(ctx, Carried::Sale { item }, icon, ctx.time);
                if press == Press::Twice {
                    asked.push(Asked::Buy { item, slot: 0 });
                }
            }
        }
        let (position, size) = (shop.store.position, shop.store.size);
        if kit::button(p, ctx, l.previous, "<", position > 0) {
            out.acts.push(Action::Store(StoreAction::PrevSales));
        }
        if kit::button(p, ctx, l.next, ">", position < size - PAGE) {
            out.acts.push(Action::Store(StoreAction::NextSales));
        }
        if kit::button(p, ctx, l.buy, "Buy", self.selected.is_some()) {
            if let Some(item) = self.selected {
                asked.push(Asked::Buy { item, slot: 0 });
            }
        }
        Some(w.rect)
    }

    /// Draw the catalog while the game shows one; where it is.
    pub fn catalog_frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        carry: &mut Carry,
        out: &mut Outcome,
        tip: &mut Option<Tip>,
    ) -> Option<Rect> {
        let Some(catalog) = &state.goods.catalog else {
            self.catalog_drawn = None;
            self.catalog.open = false;
            self.chosen = None;
            return None;
        };
        self.catalog.open = true;
        let k = p.scale;
        let size = kit::window_size_for(p, CATALOG_BODY);
        let default = (
            ((p.screen.0 - size.0 * k) / 2.0).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
        );
        let default = crate::ui::place::clear(p, state, size, default);
        let w = kit::window(p, ctx, &mut self.catalog, "Store Catalog", size, default);
        if w.closed {
            out.acts.push(Action::Store(StoreAction::LeaveCatalog));
            return Some(w.rect);
        }
        self.catalog_drawn = Some((w.rect, w.body));
        carry.zone(w.rect, Zone::Window);
        let heading = TextStyle::new(Family::Heading, 15.0, ctx.colours.heading());
        let body = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let header = p.fit(&heading, &catalog.header, w.body.w - 24.0 * k);
        p.text(&heading, w.body.x + 12.0 * k, w.body.y + 8.0 * k, &header);
        for (n, shop) in catalog.shops.iter().enumerate().take(7) {
            #[allow(clippy::cast_precision_loss)]
            let row = Rect::new(
                w.body.x + 12.0 * k,
                w.body.y + (36.0 + n as f32 * ROW) * k,
                w.body.w - 24.0 * k,
                (ROW - 4.0) * k,
            );
            let id = shop.store.id;
            kit::list_highlight(p, row, self.chosen == Some(id));
            if let Some(s) = p.art.icon(shop.store.portrait) {
                p.sprite(
                    &s,
                    Rect::new(row.x, row.y + 1.0 * k, TILE * k, TILE * k),
                    WHITE,
                );
            }
            let x = row.x + (TILE + 8.0) * k;
            p.text(
                &body,
                x,
                row.y + 4.0 * k,
                &p.fit(&body, &shop.name, row.right() - x),
            );
            let count = if shop.store.size > 0 {
                usize::try_from(shop.store.size).unwrap_or(0)
            } else {
                shop.sales.len()
            };
            let small = TextStyle::new(Family::Body, 12.0, 0xFFC8_C0B0).edge(ctx.colours.edge());
            p.text(&small, x, row.y + 24.0 * k, &format!("{count} for sale"));
            if ctx.input.hover(&row) && !shop.description.is_empty() {
                *tip = Some(Tip {
                    title: shop.name.clone(),
                    lines: vec![shop.description.clone()],
                });
            }
            if ctx.over(&row) && ctx.input.clicked(&row) {
                let twice = self
                    .last_press
                    .is_some_and(|(s, at)| s == id && ctx.time - at <= DOUBLE_PRESS);
                self.chosen = Some(id);
                self.last_press = if twice { None } else { Some((id, ctx.time)) };
                if twice {
                    out.acts.push(Action::Store(StoreAction::EnterShop(id)));
                }
            }
        }
        let foot = w.body.bottom() - 40.0 * k;
        let enter = Rect::new(w.body.x + 12.0 * k, foot, 130.0 * k, 30.0 * k);
        let mine = Rect::new(w.body.right() - 182.0 * k, foot, 170.0 * k, 30.0 * k);
        if kit::button(p, ctx, enter, "Enter", self.chosen.is_some()) {
            if let Some(id) = self.chosen {
                out.acts.push(Action::Store(StoreAction::EnterShop(id)));
            }
        }
        if kit::button(p, ctx, mine, "My Consignments", true) {
            out.acts.push(Action::Store(StoreAction::EnterConsignment));
        }
        Some(w.rect)
    }
}

/// A price as the windows write it: whole where it is whole.
#[must_use]
pub fn quantity(q: f32) -> String {
    if q.fract() == 0.0 && q.abs() < 1.0e9 {
        format!("{}", dereth_primitives::num::to_i32_f64(f64::from(q)))
    } else {
        format!("{q:.2}")
    }
}

/// What a sale costs, at the right of its row: the icon of what it costs and how much.
fn cost(p: &mut Painter<'_>, style: &TextStyle, sale: &SaleShown, r: Rect) {
    let k = p.scale;
    let icon = Rect::new(r.x, r.y + (r.h - 24.0 * k) / 2.0, 24.0 * k, 24.0 * k);
    if let Some(s) = p.art.icon(sale.sale.cost_icon) {
        p.sprite(&s, icon, WHITE);
    }
    p.text_in(
        style,
        Rect::new(icon.right() + 4.0 * k, r.y, r.w - 28.0 * k, r.h),
        Align::Left,
        &quantity(sale.sale.cost_quantity),
    );
}
