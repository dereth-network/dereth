//! The inventory window: what the player wears around its figure, the pack in its four segments
//! (the pack itself and three bags, one shown at a time by its tab), and the money bag.
//!
//! An item is selected by a press, used by a second press soon after, and carried by pulling on
//! it: to another tile or a segment's tab (moved there, a stack asking how many first), to a place
//! to wear it or onto the figure (worn there, or at its own place), onto the money bag (melted
//! into money, after a question unless the player turned it off or holds Shift), into another
//! window, or onto the world. A worn item carried to the pack is taken off into that slot. The
//! coins are carried out of the money bag the same way, asking how many.

use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::Rect;
use crate::state::goods::SEGMENTS;
use crate::state::GameState;
use crate::ui::items::{self, Carried, Carry, Icon, Press, TileLook, Zone, STEP, TILE};
use crate::ui::{Outcome, Tip};

use auberean_client_sdk::Action;

/// The window's body, in layout units.
pub const BODY: (f32, f32) = (604.0, 400.0);

/// The money bag's tile's side, in layout units: smaller than an item's, its amount in the
/// window's body text beside it.
pub const MONEY_TILE: f32 = 28.0;

/// How many tiles a row of the pack holds.
pub const COLUMNS: u32 = 7;

/// The segments' tabs.
pub const TABS: [&str; SEGMENTS] = ["Main Pack", "Bag 1", "Bag 2", "Bag 3"];

/// The places an item is worn at, as the game numbers them, and where each is drawn: the left
/// column from the top, the right column from the top, then the two hands under the figure.
pub mod place {
    pub const HEAD: u32 = 0x1;
    pub const TORSO: u32 = 0x2;
    pub const BACK: u32 = 0x4;
    pub const LEGS: u32 = 0x8;
    pub const FEET: u32 = 0x10;
    pub const HANDS: u32 = 0x20;
    pub const PRIMARY_HAND: u32 = 0x40;
    pub const SECONDARY_HAND: u32 = 0x80;
    pub const PRIMARY_RING: u32 = 0x100;
    pub const SECONDARY_RING: u32 = 0x200;
    pub const NECKLACE: u32 = 0x400;

    /// The left column, the right column, and the row under the figure.
    pub const LEFT: [u32; 5] = [HEAD, TORSO, HANDS, LEGS, FEET];
    pub const RIGHT: [u32; 4] = [NECKLACE, BACK, PRIMARY_RING, SECONDARY_RING];
    pub const UNDER: [u32; 2] = [PRIMARY_HAND, SECONDARY_HAND];

    /// Each place's name, for its tooltip when nothing is worn there.
    #[must_use]
    pub const fn name(location: u32) -> &'static str {
        match location {
            HEAD => "Head",
            TORSO => "Torso",
            BACK => "Back",
            LEGS => "Legs",
            FEET => "Feet",
            HANDS => "Hands",
            PRIMARY_HAND => "Primary Hand",
            SECONDARY_HAND => "Secondary Hand",
            PRIMARY_RING | SECONDARY_RING => "Ring",
            NECKLACE => "Necklace",
            _ => "",
        }
    }
}

/// Where the window's parts are, for a body at `body` drawn `k` pixels a layout unit.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// Each place to wear an item, and its tile.
    pub worn: Vec<(u32, Rect)>,
    /// The player's figure.
    pub figure: Rect,
    /// The segments' tabs, and the first tile of the pack's grid (the rest follow by
    /// [`STEP`], [`COLUMNS`] a row).
    pub tabs: Rect,
    pub grid: (f32, f32),
    /// The money bag's tile, and its amount.
    pub money: Rect,
    pub amount: Rect,
}

impl Layout {
    #[must_use]
    pub fn of(body: Rect, k: f32) -> Self {
        let at = |x: f32, y: f32| Rect::new(body.x + x * k, body.y + y * k, TILE * k, TILE * k);
        let mut worn = Vec::new();
        #[allow(clippy::cast_precision_loss)]
        for (i, l) in place::LEFT.iter().enumerate() {
            worn.push((*l, at(12.0, 10.0 + 56.0 * i as f32)));
        }
        #[allow(clippy::cast_precision_loss)]
        for (i, l) in place::RIGHT.iter().enumerate() {
            worn.push((*l, at(208.0, 10.0 + 56.0 * i as f32)));
        }
        worn.push((place::UNDER[0], at(76.0, 300.0)));
        worn.push((place::UNDER[1], at(144.0, 300.0)));
        Self {
            worn,
            figure: Rect::new(body.x + 64.0 * k, body.y + 10.0 * k, 136.0 * k, 280.0 * k),
            tabs: Rect::new(
                body.x + 270.0 * k,
                body.y + 8.0 * k,
                STEP * COLUMNS as f32 * k,
                28.0 * k,
            ),
            grid: (body.x + 270.0 * k, body.y + 44.0 * k),
            money: Rect::new(
                body.x + 270.0 * k,
                body.y + 352.0 * k,
                MONEY_TILE * k,
                MONEY_TILE * k,
            ),
            amount: Rect::new(
                body.x + (276.0 + MONEY_TILE) * k,
                body.y + 352.0 * k,
                260.0 * k,
                MONEY_TILE * k,
            ),
        }
    }

    /// The pack's `n`th tile.
    #[must_use]
    pub fn cell(&self, n: u32, k: f32) -> Rect {
        #[allow(clippy::cast_precision_loss)]
        let (col, row) = ((n % COLUMNS) as f32, (n / COLUMNS) as f32);
        Rect::new(
            self.grid.0 + col * STEP * k,
            self.grid.1 + row * STEP * k,
            TILE * k,
            TILE * k,
        )
    }
}

/// The inventory window.
#[derive(Debug, Default)]
pub struct Inventory {
    pub window: WindowState,
    /// The segment shown.
    pub segment: usize,
    /// Where it was last drawn, and its body.
    drawn: Option<(Rect, Rect)>,
    /// Where the player's figure was last drawn, and how many of the frame's quads were drawn
    /// before it (they stand under it).
    figure: Option<(Rect, usize)>,
    /// Where the segments' tabs were last drawn.
    tabs: Vec<Rect>,
}

impl Inventory {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.window.open
    }

    /// Open it, or close it.
    pub fn toggle(&mut self) {
        self.window.open = !self.window.open;
        if !self.window.open {
            self.close();
        }
    }

    pub fn close(&mut self) {
        self.window.open = false;
        self.drawn = None;
        self.figure = None;
    }

    /// Where it was last drawn, while it is open.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.drawn.map(|(r, _)| r)
    }

    /// Its parts as it was last drawn, at `k` pixels a layout unit.
    #[must_use]
    pub fn layout(&self, k: f32) -> Option<Layout> {
        self.drawn.map(|(_, b)| Layout::of(b, k))
    }

    /// Where the segments' tabs were last drawn.
    #[must_use]
    pub fn tabs(&self) -> &[Rect] {
        &self.tabs
    }

    /// Where the player's figure stands, as last drawn, and how many of the frame's quads stand
    /// under it.
    #[must_use]
    pub const fn figure(&self) -> Option<(Rect, usize)> {
        self.figure
    }

    /// Draw it while it is open; where it is.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        carry: &mut Carry,
        out: &mut Outcome,
        tip: &mut Option<Tip>,
    ) -> Option<Rect> {
        self.figure = None;
        if !self.window.open {
            return None;
        }
        let k = p.scale;
        let size = kit::window_size_for(p, BODY);
        let default = (
            (p.screen.0 - size.0 * k - 40.0 * k).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
        );
        let default = crate::ui::place::clear(p, state, size, default);
        let w = kit::window(p, ctx, &mut self.window, "Inventory", size, default);
        if w.closed {
            self.close();
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        carry.zone(w.rect, Zone::Window);
        let l = Layout::of(w.body, k);
        let goods = &state.goods;
        // The figure, its ground under it; what follows stands over it.
        kit::figure_ground(p, l.figure);
        self.figure = Some((l.figure, p.list.mark()));
        carry.zone(l.figure, Zone::Figure);
        for (location, r) in &l.worn {
            let worn = goods.worn_at(*location);
            let item = worn.and_then(|id| goods.item(id).map(|it| (id, it)));
            let icon = item.map(|(_, it)| Icon::of(it));
            let t = items::tile(
                p,
                ctx,
                *r,
                TileLook {
                    icon: icon.as_ref(),
                    selected: worn.is_some_and(|id| id == goods.selection),
                    wear: item.and_then(|(_, it)| it.wear),
                    ..TileLook::default()
                },
            );
            carry.zone(
                *r,
                Zone::Wear {
                    location: *location,
                },
            );
            if t.over {
                *tip = Some(Tip {
                    title: item.map_or_else(
                        || place::name(*location).to_owned(),
                        |(_, it)| it.name.clone(),
                    ),
                    lines: item
                        .and_then(|(_, it)| it.wear)
                        .map(|(n, m)| vec![format!("Durability {n} / {m}")])
                        .unwrap_or_default(),
                });
            }
            if let (true, Some((id, _))) = (t.examine, item) {
                carry.examine = Some(id);
            }
            if let (true, Some((id, it))) = (t.pressed, item) {
                let carried = Carried::Worn {
                    item: id,
                    location: *location,
                };
                select_or_use(carry, ctx, carried, Icon::of(it), id, out);
            }
        }
        // The segments' tabs, each taking what is put down on it.
        let rects = kit::tab_rects_for(p, l.tabs, &TABS, self.segment);
        kit::tabs(p, ctx, l.tabs, &TABS, &mut self.segment);
        for (segment, r) in rects.iter().enumerate() {
            carry.zone(*r, Zone::PackTab { segment });
        }
        self.tabs = rects;
        let player = goods.player().unwrap_or(0);
        let rooms = goods.rooms();
        let segment = self.segment.min(SEGMENTS - 1);
        let first = goods.first_slot(segment);
        let held = goods.contents(player, segment);
        for cell in 0..rooms[segment] {
            let r = l.cell(cell, k);
            let id = held.get(cell as usize).copied();
            let item = id.and_then(|id| goods.item(id).map(|it| (id, it)));
            let icon = item.map(|(_, it)| Icon::of(it));
            let t = items::tile(
                p,
                ctx,
                r,
                TileLook {
                    icon: icon.as_ref(),
                    quantity: item.map(|(_, it)| it.quantity),
                    selected: id.is_some_and(|id| id == goods.selection),
                    dim: id.is_some() && item.is_none(),
                    ..TileLook::default()
                },
            );
            carry.zone(r, Zone::Pack { segment, cell });
            if let (true, Some((_, it))) = (t.over, item) {
                *tip = Some(Tip {
                    title: it.name.clone(),
                    lines: Vec::new(),
                });
            }
            if let (true, Some((id, _))) = (t.examine, item) {
                carry.examine = Some(id);
            }
            if let (true, Some((id, it))) = (t.pressed, item) {
                let carried = Carried::Item {
                    item: id,
                    from: player,
                    slot: first + cell,
                };
                select_or_use(carry, ctx, carried, Icon::of(it), id, out);
            }
        }
        // The money bag: its coins carried out, an item put down on it melted.
        let money = goods.money();
        let over = ctx.input.hover(&l.money);
        let pressed = kit::icon_slot(p, ctx, l.money, None, crate::draw::WHITE);
        items::coin(p, kit::icon_rect(l.money));
        carry.zone(l.money, Zone::Money);
        if pressed && money > 0 {
            carry.press(ctx, Carried::Coins, Icon::default(), ctx.time);
        }
        let style = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        p.text_in(&style, l.amount, Align::Left, &coins(money));
        if over {
            *tip = Some(Tip {
                title: "Money".to_owned(),
                lines: vec![
                    coins(money),
                    "Drag coins out, or drop an item here to turn it into gold.".to_owned(),
                ],
            });
        }
        Some(w.rect)
    }
}

/// An amount of money, as the window writes it.
#[must_use]
pub fn coins(n: u32) -> String {
    let out = crate::ui::hud::grouped(n);
    format!("{out} {}", if n == 1 { "coin" } else { "coins" })
}

/// A press on a tile of an item: the item selected, and used when pressed again soon after; it
/// may be carried from here.
pub fn select_or_use(
    carry: &mut Carry,
    ctx: &Ctx<'_>,
    carried: Carried,
    icon: Icon,
    item: u64,
    out: &mut Outcome,
) {
    let press = carry.press(ctx, carried, icon, ctx.time);
    out.acts.push(Action::SetSelection { target: item });
    if press == Press::Twice {
        out.acts.push(Action::UseSelected);
    }
}
