//! What every window of things shares: an item's tile, what the pointer carries from one window to
//! another, the places a carried thing may be put down, the box that asks how many, and the box
//! that asks yes or no.
//!
//! A tile does what the game's item cells do: a press selects its item, a second press on it soon
//! after uses it, a right press examines it, and pulling on it carries it. Let go over a place that takes it, the carried
//! thing goes there (the window that drew the place says what that asks of the game); let go over
//! the world, it goes to what is under the pointer; let go anywhere else, nothing happens.

use auberean_client_sdk::contract::view::Item;
use dereth_ui_kit::art::{Family, Sprite};
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::{Argb, Rect, WHITE};

/// A tile's side, and the step from one tile to the next, in layout units.
pub const TILE: f32 = 44.0;
pub const STEP: f32 = 46.0;

/// How far the pointer moves with the button held before a press carries what it pressed, in
/// pixels.
pub const CARRY_SLOP: f32 = 4.0;

/// How soon a second press on the same thing uses it, in seconds.
pub const DOUBLE_PRESS: f64 = 0.5;

/// The tint of a tile whose thing the player may not have (a sale sold out).
pub const DIM: Argb = 0xFF70_7070;

/// An icon as the game composes it: its layers (each a picture and the color it is tinted by,
/// bottom first), or one picture when it has none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Icon {
    pub layers: Vec<(u32, u32)>,
    pub picture: u32,
}

impl Icon {
    /// An item's icon.
    #[must_use]
    pub fn of(item: &Item) -> Self {
        Self {
            layers: item.icon_layers.clone(),
            picture: item.icon,
        }
    }

    /// The pictures to draw, bottom first, each with its tint.
    #[must_use]
    pub fn sprites(&self, p: &Painter<'_>) -> Vec<(Sprite, Argb)> {
        if self.layers.is_empty() {
            return p
                .art
                .icon(self.picture)
                .map(|s| (s, WHITE))
                .into_iter()
                .collect();
        }
        self.layers
            .iter()
            .filter_map(|(picture, colour)| p.art.icon(*picture).map(|s| (s, *colour)))
            .collect()
    }
}

/// What the pointer carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carried {
    /// An item from a slot of a container: the player's pack or one the player has open.
    Item { item: u64, from: u64, slot: u32 },
    /// An item the player wears, from its place.
    Worn { item: u64, location: u32 },
    /// Coins out of the money bag.
    Coins,
    /// An item the player offers in a trade.
    Offered { item: u64 },
    /// A shop's sale.
    Sale { item: u64 },
    /// One of the player's consignments: its number, and whether it sold.
    Consignment { sale: u32, sold: bool },
    /// An item in a slot of the making window: the item, and its slot.
    Ingredient { item: u64, slot: u32 },
}

impl Carried {
    /// The item it is, when it is one.
    #[must_use]
    pub const fn item(&self) -> Option<u64> {
        match *self {
            Self::Item { item, .. }
            | Self::Worn { item, .. }
            | Self::Offered { item }
            | Self::Sale { item }
            | Self::Ingredient { item, .. } => Some(item),
            Self::Coins | Self::Consignment { .. } => None,
        }
    }
}

/// A place a carried thing may be put down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// A window: what is put down over it and over none of its places stays where it was.
    Window,
    /// A tile of one of the pack's segments: the segment, and the tile's place in it.
    Pack {
        segment: usize,
        cell: u32,
    },
    /// A segment's tab: after its last item.
    PackTab {
        segment: usize,
    },
    /// A tile of an opened container.
    Container {
        container: u64,
        cell: u32,
    },
    /// A place to wear an item, and the player's figure (each item's own place).
    Wear {
        location: u32,
    },
    Figure,
    /// The money bag.
    Money,
    /// The player's side of a trade.
    TradeOffer,
    /// The player's consignments, and one of them.
    Consignments,
    Consignment {
        sale: u32,
    },
    /// The window a new consignment is put up in.
    NewSale,
    /// A slot of the making window, by the ingredient it takes.
    Ingredient {
        slot: u32,
    },
}

/// A thing pressed, and carried once the pointer moves.
#[derive(Clone, Debug, PartialEq)]
pub struct Drag {
    pub carried: Carried,
    pub icon: Icon,
    /// Where it was pressed.
    pub origin: (f32, f32),
    /// The pointer moved far enough: it is carried.
    pub carrying: bool,
}

/// What a carried thing was put down on.
#[derive(Clone, Debug, PartialEq)]
pub enum Put {
    /// One of the windows' places.
    On(Carried, Zone),
    /// The world, at a point on the screen.
    World(Carried, (f32, f32)),
    /// Somewhere that takes nothing: what it came from may still have something to do with it
    /// (an offer carried out of the trade is taken back).
    Nowhere(Carried),
}

/// How a press on a tile went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// Pressed once.
    Once,
    /// Pressed a second time soon after: use it.
    Twice,
}

/// What the pointer carries, the places drawn this frame that take a carried thing, and the
/// presses that may become a second press.
#[derive(Debug, Default)]
pub struct Carry {
    pub drag: Option<Drag>,
    zones: Vec<(Rect, Zone)>,
    /// The last press, on what, and when.
    last: Option<(Carried, f64)>,
    /// An item pressed with the right button this frame, to examine.
    pub examine: Option<u64>,
}

impl Carry {
    /// Forget last frame's places; a press held far enough from where it was pressed carries its
    /// thing, and one let go without moving carries nothing.
    pub fn begin(&mut self, ctx: &Ctx<'_>) {
        self.zones.clear();
        if let Some(d) = &mut self.drag {
            let (x, y) = ctx.input.mouse;
            if !d.carrying
                && ctx.input.down[0]
                && ((x - d.origin.0).abs() > CARRY_SLOP || (y - d.origin.1).abs() > CARRY_SLOP)
            {
                d.carrying = true;
                // What was carried was not pressed to be used: the next press is a first.
                self.last = None;
            }
        }
    }

    /// A place drawn this frame that takes a carried thing.
    pub fn zone(&mut self, r: Rect, z: Zone) {
        self.zones.push((r, z));
    }

    /// Whether a thing is carried now.
    #[must_use]
    pub fn carrying(&self) -> bool {
        self.drag.as_ref().is_some_and(|d| d.carrying)
    }

    /// A tile of `carried`, its look `icon`, pressed at `time`: once, or a second time soon after
    /// the first on the same thing.
    pub fn press(&mut self, ctx: &Ctx<'_>, carried: Carried, icon: Icon, time: f64) -> Press {
        let twice = self
            .last
            .is_some_and(|(c, at)| c == carried && time - at <= DOUBLE_PRESS);
        self.last = if twice { None } else { Some((carried, time)) };
        self.drag = Some(Drag {
            carried,
            icon,
            origin: ctx.input.mouse,
            carrying: false,
        });
        if twice {
            Press::Twice
        } else {
            Press::Once
        }
    }

    /// The topmost place under `(x, y)` drawn this frame.
    #[must_use]
    pub fn zone_at(&self, (x, y): (f32, f32)) -> Option<Zone> {
        self.zones
            .iter()
            .rev()
            .find(|(r, _)| r.contains(x, y))
            .map(|(_, z)| *z)
    }

    /// The end of the frame: where the carried thing was put down, when the button was let go.
    /// `over_later` says whether something drawn over the windows of things is under the pointer
    /// (a question, the settings); `over_any` whether anything of the interface is.
    pub fn end(&mut self, ctx: &Ctx<'_>, over_later: bool, over_any: bool) -> Option<Put> {
        if ctx.input.down[0] {
            return None;
        }
        let d = self.drag.take()?;
        if !d.carrying {
            return None;
        }
        let at = ctx.input.mouse;
        if over_later {
            return Some(Put::Nowhere(d.carried));
        }
        Some(match self.zone_at(at) {
            Some(Zone::Window) => Put::Nowhere(d.carried),
            Some(z) => Put::On(d.carried, z),
            None if over_any => Put::Nowhere(d.carried),
            None => Put::World(d.carried, at),
        })
    }

    /// The carried thing under the pointer, over everything.
    pub fn draw(&self, p: &mut Painter<'_>, mouse: (f32, f32)) {
        let Some(d) = self.drag.as_ref().filter(|d| d.carrying) else {
            return;
        };
        let k = p.scale;
        let r = Rect::new(
            mouse.0 - TILE * k / 2.0,
            mouse.1 - TILE * k / 2.0,
            TILE * k,
            TILE * k,
        );
        for (s, c) in d.icon.sprites(p) {
            p.sprite(&s, kit::icon_rect(r), c);
        }
        if d.carried == Carried::Coins {
            coin(p, kit::icon_rect(r));
        }
    }
}

/// The game's own picture of the money: a pile of gold coins, as its inventory shows it.
pub const GOLD: u32 = 0x4100_44E3;

/// The money drawn in `r`: the game's picture of gold, as large as `r` holds it with its shape
/// kept; a stack of coins drawn plainly where the game's pictures are not at hand.
pub fn coin(p: &mut Painter<'_>, r: Rect) {
    if let Some(s) = p.art.icon(GOLD) {
        let fit = (r.w / s.w).min(r.h / s.h);
        let (w, h) = (s.w * fit, s.h * fit);
        p.sprite(
            &s,
            Rect::new(r.x + (r.w - w) / 2.0, r.y + (r.h - h) / 2.0, w, h),
            WHITE,
        );
        return;
    }
    let k = r.w / 40.0;
    for i in 0..3 {
        #[allow(clippy::cast_precision_loss)]
        let dy = i as f32 * 5.0 * k;
        p.fill(
            Rect::new(r.x + 8.0 * k, r.y + 22.0 * k - dy, 24.0 * k, 6.0 * k),
            0xFFE8_C050,
        );
        p.outline(
            Rect::new(r.x + 8.0 * k, r.y + 22.0 * k - dy, 24.0 * k, 6.0 * k),
            k.max(1.0),
            0xFF80_5A10,
        );
    }
}

/// How a tile is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TileLook<'a> {
    pub icon: Option<&'a Icon>,
    /// How many, written in its corner (a stack's).
    pub quantity: Option<u32>,
    /// It is the selection.
    pub selected: bool,
    /// Its wear: how worn, of the most.
    pub wear: Option<(u32, u32)>,
    /// Shown dimmed: waiting for the game, or not to be had.
    pub dim: bool,
}

/// What happened to a tile this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileOut {
    pub pressed: bool,
    /// Pressed with the right button: examine it.
    pub examine: bool,
    pub over: bool,
}

/// The wear gauge's fill.
pub const WEAR: Argb = 0xFF7C_D8F8;

/// `c` darkened as [`DIM`] darkens white, its alpha kept.
#[must_use]
pub fn dimmed(c: Argb) -> Argb {
    let channel = |shift: u32| (((c >> shift) & 0xFF) * ((DIM >> shift) & 0xFF) / 0xFF) << shift;
    c & 0xFF00_0000 | channel(16) | channel(8) | channel(0)
}

/// One tile: its frame, its icon's layers, its count, its wear along its foot; whether it was
/// pressed.
pub fn tile(p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect, look: TileLook<'_>) -> TileOut {
    let sprites = look.icon.map(|i| i.sprites(p)).unwrap_or_default();
    let tint = |c: Argb| if look.dim { dimmed(c) } else { c };
    let (first, rest) = sprites
        .split_first()
        .map_or((None, &[][..]), |(f, r)| (Some(f), r));
    let pressed = kit::icon_slot(
        p,
        ctx,
        r,
        first.map(|(s, _)| s),
        first.map_or(WHITE, |(_, c)| tint(*c)),
    );
    let well = kit::icon_rect(r);
    for (s, c) in rest {
        p.sprite(s, well, tint(*c));
    }
    let k = r.w / TILE;
    if let Some(n) = look.quantity.filter(|n| *n > 1) {
        let style = TextStyle::new(Family::Numerals, 11.0, WHITE).edge(0xFF00_0000);
        p.text_in(
            &style,
            Rect::new(well.x, well.bottom() - 13.0 * k, well.w - 2.0 * k, 12.0 * k),
            Align::Right,
            &n.to_string(),
        );
    }
    if let Some((now, most)) = look.wear.filter(|(_, m)| *m > 0) {
        #[allow(clippy::cast_precision_loss)]
        let full = (now as f32 / most as f32).clamp(0.0, 1.0);
        let bar = Rect::new(
            well.x + 2.0 * k,
            well.bottom() - 5.0 * k,
            well.w - 4.0 * k,
            3.0 * k,
        );
        p.fill(bar, 0xFF20_2020);
        p.fill(Rect::new(bar.x, bar.y, bar.w * full, bar.h), WEAR);
    }
    if look.selected {
        kit::selected_tile(p, r);
    }
    TileOut {
        pressed,
        examine: ctx.input.right_clicked(&r),
        over: ctx.input.hover(&r),
    }
}

/// What an amount asked for is for, once given.
#[derive(Clone, Debug, PartialEq)]
pub enum For {
    /// Move so many of an item to a slot of a container. `whole`: how many a whole move takes
    /// (the most a stack holds, or all of it); `past_end`: it was put down past the last item of
    /// its own segment, where a whole move lands a slot sooner.
    Put {
        item: u64,
        from: u64,
        to: u64,
        old_slot: u32,
        new_slot: u32,
        whole: u32,
        past_end: bool,
    },
    /// Coins out of the money bag into a slot of a container.
    Coins { slot: u32, container: u64 },
    /// Melt so many of an item into money.
    Melt { item: u64, slot: u32, from: u64 },
    /// Offer so many of an item in the trade.
    Offer { item: u64 },
    /// Buy so many of a sale into a slot of the pack.
    Buy { item: u64, slot: u32 },
    /// Restock a consignment with so many of an item.
    Restock { item: u64, sale: u32 },
    /// Put so many of an item up for sale.
    Sell { item: u64 },
}

/// The box that asks how many.
#[derive(Clone, Debug, PartialEq)]
pub struct Amount {
    pub what: For,
    /// What it asks about (the item's name).
    pub label: String,
    /// The number typed.
    pub typed: String,
    pub most: u32,
    pub window: WindowState,
    focus: u32,
}

/// The text box's own number, for the kit's focus.
const AMOUNT_BOX: u32 = 0x4142_0001;

/// How an amount box was answered.
#[derive(Clone, Debug, PartialEq)]
pub enum Answered<T> {
    Yes(T),
    No,
}

impl Amount {
    /// Ask how many of `label` (at most `most`), starting at `start`.
    #[must_use]
    pub fn new(what: For, label: String, start: u32, most: u32) -> Self {
        Self {
            what,
            label,
            typed: start.min(most).to_string(),
            most,
            window: WindowState {
                open: true,
                ..WindowState::default()
            },
            focus: AMOUNT_BOX,
        }
    }

    /// The number typed, no more than the most.
    #[must_use]
    pub fn value(&self) -> u32 {
        self.typed.trim().parse::<u32>().unwrap_or(0).min(self.most)
    }

    /// Whether its number box has the keys.
    #[must_use]
    pub const fn typing(&self) -> bool {
        self.focus == AMOUNT_BOX
    }

    /// Where it is drawn, in the middle of the screen, `k` pixels a layout unit.
    #[must_use]
    pub fn rect(p: &Painter<'_>) -> Rect {
        let size = kit::window_size_for(p, AMOUNT_BODY);
        let k = p.scale;
        Rect::new(
            ((p.screen.0 - size.0 * k) / 2.0).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
            size.0 * k,
            size.1 * k,
        )
    }

    /// Draw it; how it was answered, when it was (Enter or OK, with the amount; Escape, Cancel
    /// or its close button).
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
    ) -> (Rect, Option<Answered<u32>>) {
        let k = p.scale;
        let at = Self::rect(p);
        let size = kit::window_size_for(p, AMOUNT_BODY);
        let w = kit::window(p, ctx, &mut self.window, "How many?", size, (at.x, at.y));
        if w.closed {
            return (w.rect, Some(Answered::No));
        }
        let body = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let label = p.fit(&body, &self.label, w.body.w - 24.0 * k);
        p.text(&body, w.body.x + 12.0 * k, w.body.y + 8.0 * k, &label);
        let row = w.body.y + 34.0 * k;
        let minus = Rect::new(w.body.x + 12.0 * k, row, 30.0 * k, 28.0 * k);
        let field = Rect::new(minus.right() + 6.0 * k, row, 110.0 * k, 28.0 * k);
        let plus = Rect::new(field.right() + 6.0 * k, row, 30.0 * k, 28.0 * k);
        let all = Rect::new(plus.right() + 6.0 * k, row, 60.0 * k, 28.0 * k);
        let n = self.value();
        if kit::button(p, ctx, minus, "-", n > 0) {
            self.typed = n.saturating_sub(1).to_string();
        }
        if kit::button(p, ctx, plus, "+", n < self.most) {
            self.typed = (n + 1).min(self.most).to_string();
        }
        if kit::button(p, ctx, all, "All", true) {
            self.typed = self.most.to_string();
        }
        let mut entered = kit::text_box_aligned(
            p,
            ctx,
            field,
            &mut self.typed,
            &mut self.focus,
            AMOUNT_BOX,
            Align::Right,
        );
        self.typed.retain(|c| c.is_ascii_digit());
        let foot = w.body.y + 76.0 * k;
        let ok = Rect::new(w.body.x + 12.0 * k, foot, 110.0 * k, 30.0 * k);
        let cancel = Rect::new(w.body.right() - 122.0 * k, foot, 110.0 * k, 30.0 * k);
        entered |= kit::button(p, ctx, ok, "OK", true);
        let cancelled = kit::button(p, ctx, cancel, "Cancel", true)
            || ctx.input.take_key(crate::ui::vk::ESCAPE);
        let answer = if entered {
            Some(Answered::Yes(self.value()))
        } else if cancelled {
            Some(Answered::No)
        } else {
            None
        };
        (w.rect, answer)
    }
}

/// The amount box's body, in layout units.
const AMOUNT_BODY: (f32, f32) = (330.0, 116.0);

/// What a yes is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asking {
    /// Melt so many of an item into money.
    Melt {
        item: u64,
        quantity: u32,
        slot: u32,
        from: u64,
    },
    /// Restock a consignment with so many of an item.
    Restock { item: u64, quantity: u32, sale: u32 },
    /// Put so many of an item up for sale at a price.
    Sell {
        item: u64,
        quantity: u32,
        price: u32,
    },
    /// The purchase the scripts asked to confirm.
    Purchase,
}

/// A yes-or-no question of the windows' own.
#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub asking: Asking,
    pub text: String,
    pub window: WindowState,
}

/// The question box's body, in layout units.
const QUESTION_BODY: (f32, f32) = (360.0, 120.0);

impl Question {
    #[must_use]
    pub fn new(asking: Asking, text: String) -> Self {
        Self {
            asking,
            text,
            window: WindowState {
                open: true,
                ..WindowState::default()
            },
        }
    }

    /// Draw it; whether it was answered yes or no, when it was.
    pub fn frame(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>) -> (Rect, Option<bool>) {
        let k = p.scale;
        let size = kit::window_size_for(p, QUESTION_BODY);
        let at = (
            ((p.screen.0 - size.0 * k) / 2.0).round(),
            ((p.screen.1 - size.1 * k) / 2.0 - 80.0 * k).round(),
        );
        let w = kit::window(p, ctx, &mut self.window, "Confirm", size, at);
        if w.closed {
            return (w.rect, Some(false));
        }
        let body = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        for (i, line) in p
            .wrap(&body, &self.text, w.body.w - 24.0 * k)
            .iter()
            .take(3)
            .enumerate()
        {
            #[allow(clippy::cast_precision_loss)]
            let y = w.body.y + (8.0 + 18.0 * i as f32) * k;
            p.text(&body, w.body.x + 12.0 * k, y, line);
        }
        let foot = w.body.y + 80.0 * k;
        let yes = Rect::new(w.body.x + 12.0 * k, foot, 110.0 * k, 30.0 * k);
        let no = Rect::new(w.body.right() - 122.0 * k, foot, 110.0 * k, 30.0 * k);
        let answer = if kit::button(p, ctx, yes, "Yes", true)
            || ctx.input.take_key(crate::ui::vk::ENTER)
        {
            Some(true)
        } else if kit::button(p, ctx, no, "No", true) || ctx.input.take_key(crate::ui::vk::ESCAPE) {
            Some(false)
        } else {
            None
        };
        (w.rect, answer)
    }
}
