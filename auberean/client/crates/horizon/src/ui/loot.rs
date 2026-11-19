//! The loot window: what a container the player opened holds (a chest, a corpse), its items taken
//! one by one by carrying them to the pack, or all at once; melted all at once; and closed.
//!
//! It shows the container opened last while the game holds it open, and the game closes it when
//! the window's close button is pressed.

use auberean_client_sdk::Action;
use dereth_ui_kit::paint::Painter;
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::Rect;
use crate::state::GameState;
use crate::ui::inventory::select_or_use;
use crate::ui::items::{self, Carried, Carry, Icon, TileLook, Zone, STEP, TILE};
use crate::ui::{Outcome, Tip};

/// How many tiles a row holds.
pub const COLUMNS: u32 = 6;

/// The most rows shown.
pub const ROWS: u32 = 6;

/// The window's body's width, in layout units; its height follows its rows.
const WIDTH: f32 = 24.0 + STEP * 6.0;

/// The loot window.
#[derive(Debug, Default)]
pub struct Loot {
    pub window: WindowState,
    /// The container shown.
    shown: Option<u64>,
    drawn: Option<(Rect, Rect)>,
}

/// Where its parts are for a body at `body`, `k` pixels a layout unit: the first tile (the rest
/// follow by [`STEP`], [`COLUMNS`] a row), and the buttons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub grid: (f32, f32),
    pub take_all: Rect,
    pub melt_all: Rect,
}

impl Layout {
    #[must_use]
    pub fn of(body: Rect, k: f32) -> Self {
        let foot = body.bottom() - 40.0 * k;
        Self {
            grid: (body.x + 12.0 * k, body.y + 8.0 * k),
            take_all: Rect::new(body.x + 12.0 * k, foot, 130.0 * k, 30.0 * k),
            melt_all: Rect::new(body.right() - 142.0 * k, foot, 130.0 * k, 30.0 * k),
        }
    }

    /// The `n`th tile.
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

impl Loot {
    /// The container shown, while it is.
    #[must_use]
    pub const fn shown(&self) -> Option<u64> {
        self.shown
    }

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

    /// Draw it while the game holds a container open; where it is.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        carry: &mut Carry,
        out: &mut Outcome,
        tip: &mut Option<Tip>,
    ) -> Option<Rect> {
        let goods = &state.goods;
        let Some((container, name)) = goods.opened.last() else {
            self.shown = None;
            self.drawn = None;
            return None;
        };
        if self.shown != Some(*container) {
            self.shown = Some(*container);
            self.window = WindowState {
                open: true,
                ..WindowState::default()
            };
        }
        let k = p.scale;
        let size = goods.size(*container).max(1);
        let rows = size.div_ceil(COLUMNS).min(ROWS);
        #[allow(clippy::cast_precision_loss)]
        let body = (WIDTH, 16.0 + STEP * rows as f32 + 44.0);
        let size_px = kit::window_size_for(p, body);
        let default = (
            (p.screen.0 / 2.0 - size_px.0 * k - 120.0 * k).round(),
            ((p.screen.1 - size_px.1 * k) / 2.0).round(),
        );
        let title = if name.is_empty() { "Loot" } else { name };
        let default = crate::ui::place::clear(p, state, size_px, default);
        let w = kit::window(p, ctx, &mut self.window, title, size_px, default);
        if w.closed {
            out.acts.push(Action::SetOpen {
                container: *container,
                open: false,
            });
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        carry.zone(w.rect, Zone::Window);
        let l = Layout::of(w.body, k);
        let held = goods.contents(*container, 0);
        for cell in 0..(rows * COLUMNS).min(size) {
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
            carry.zone(
                r,
                Zone::Container {
                    container: *container,
                    cell,
                },
            );
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
                    from: *container,
                    slot: cell,
                };
                select_or_use(carry, ctx, carried, Icon::of(it), id, out);
            }
        }
        let some = !held.is_empty();
        if kit::button(p, ctx, l.take_all, "Take All", some) {
            out.acts.push(Action::TakeAll {
                container: *container,
            });
        }
        if kit::button(p, ctx, l.melt_all, "Make Coins", some) {
            out.acts.push(Action::MeltAll {
                container: *container,
            });
        }
        Some(w.rect)
    }
}
