//! The plugin bar: a strip with a handle to move it by at its left end, a button for each plugin,
//! and a grip at its right end that sets how wide it is. Where the buttons do not all fit, a
//! step left and a step right stand before the grip, and the wheel over the bar scrolls the
//! buttons along it.

use dereth_ui_kit::art::Family;
use dereth_ui_kit::draw::{Argb, Rect};
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx};

use crate::BarItem;

/// How the bar is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarLook {
    /// Flat fills in the skin's palette, each button colored by what its plugin is doing: the
    /// layer over the modern and the classic interface.
    Plain,
    /// The kit's plain ground and its lit buttons, a lamp on each saying what its plugin is
    /// doing: an interface with art of its own.
    Kit,
}

impl BarLook {
    /// The bar's height, its handle's width, the room either side of a button's name, the gap
    /// between buttons and how far a button keeps from the bar's top and bottom, in layout units.
    const fn measures(self) -> (f32, f32, f32, f32, f32) {
        match self {
            Self::Plain => (20.0, 12.0, 8.0, 2.0, 2.0),
            Self::Kit => (28.0, 16.0, 12.0, 4.0, 3.0),
        }
    }
}

/// The grip's width at the bar's right end, in layout units.
const GRIP_W: f32 = 10.0;

/// The narrowest a button's share of the bar may be squeezed to, in layout units.
const LEAST_VIEW: f32 = 48.0;

/// How far a step of the bar's scroll moves its buttons, in layout units.
const STEP: f32 = 60.0;

/// Where the player left the plugin bar: its top left in screen pixels, and the width they gave
/// it in layout units, if they gave one.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BarPlace {
    pub at: (f32, f32),
    pub width: Option<f32>,
}

/// The bar's state from one frame to the next.
#[derive(Debug, Default, Clone, Copy)]
pub struct Bar {
    /// Its top left, in screen pixels, once drawn.
    pos: Option<(f32, f32)>,
    /// The width the player gave it, in layout units.
    width: Option<f32>,
    /// Where in the handle a move took hold.
    grab: Option<(f32, f32)>,
    /// Where the pointer took hold of the grip, and the width then (screen pixels).
    sizing: Option<(f32, f32)>,
    /// How far the buttons are scrolled along, in screen pixels.
    scroll: f32,
    /// Where it was drawn last.
    rect: Rect,
}

/// The buttons' colors by what the plugin is doing.
#[must_use]
pub fn state_colour(state: &str) -> Argb {
    match state {
        "running" => 0xFF2E_5A28,
        "paused" => 0xFF6A_5418,
        "error" => 0xFF70_2018,
        _ => 0xFF3A_3A3A,
    }
}

impl Bar {
    /// Where the bar stands and the width the player gave it, once it has been drawn.
    #[must_use]
    pub fn place(&self) -> Option<BarPlace> {
        self.pos.map(|at| BarPlace {
            at,
            width: self.width,
        })
    }

    /// Where the bar was drawn last, in screen pixels.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.pos.map(|_| self.rect)
    }

    /// How far its buttons are scrolled along, in screen pixels.
    #[must_use]
    pub fn scrolled(&self) -> f32 {
        self.scroll
    }

    /// Draw the bar of `items` in `look`, the first time at `remembered` (where the player left
    /// it), and answer where it is and the plugin whose button was clicked. The bar keeps on the
    /// screen, and is never wider than the screen.
    #[allow(clippy::too_many_lines)]
    pub fn draw(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        items: &[BarItem],
        remembered: BarPlace,
        look: BarLook,
    ) -> (Rect, Option<String>) {
        let k = p.scale;
        let (bar_h, handle_w, pad, gap, edge) = look.measures();
        let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let widths: Vec<f32> = items
            .iter()
            .map(|i| (p.measure(&style, &i.name) + 2.0 * pad * k).round())
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let content = widths.iter().sum::<f32>() + gap * k * (items.len() as f32 + 1.0);
        let h = bar_h * k;
        let (fixed_w, grip_w) = (handle_w * k, GRIP_W * k);
        let arrows_w = 2.0 * h;
        let natural = fixed_w + content + grip_w;
        // Narrowed, it keeps its handle, its steps, its grip and a little of its buttons.
        let least = (fixed_w + arrows_w + grip_w + LEAST_VIEW * k).min(natural);
        let (mut x, mut y) = self.pos.unwrap_or(remembered.at);
        if self.width.is_none() {
            self.width = remembered.width;
        }
        // Moved by its handle.
        if let Some((gx, gy)) = self.grab {
            if ctx.input.down[0] {
                x = ctx.input.mouse.0 - gx;
                y = ctx.input.mouse.1 - gy;
                ctx.input.captured = true;
            } else {
                self.grab = None;
            }
        }
        // Sized by its grip.
        if let Some((from, then)) = self.sizing {
            if ctx.input.down[0] {
                let w = (then + ctx.input.mouse.0 - from).max(least);
                self.width = Some(w / k.max(0.01));
                ctx.input.captured = true;
            } else {
                self.sizing = None;
            }
        }
        let wanted = self.width.map_or(natural, |w| (w * k).max(least));
        // Never wider than the screen, and on it.
        let w = wanted.min(p.screen.0).max(least.min(p.screen.0));
        x = x.clamp(0.0, (p.screen.0 - w).max(0.0));
        y = y.clamp(0.0, (p.screen.1 - h).max(0.0));
        self.pos = Some((x, y));
        let r = Rect::new(x, y, w, h);
        self.rect = r;
        let line = 1.0_f32.max(k.floor());
        match look {
            BarLook::Plain => {
                let pal = *p.art.palette();
                p.fill(r, pal.panel | 0xFF00_0000);
                p.outline(r, line, pal.panel_border | 0xFF00_0000);
            }
            BarLook::Kit => kit::plain_ground(p, r),
        }
        // The handle.
        let handle = Rect::new(x, y, fixed_w, h);
        let strokes = (h * 0.2).round();
        for i in 0..3_u8 {
            p.fill(
                Rect::new(
                    x + (3.0 + 3.0 * f32::from(i)) * k,
                    y + strokes,
                    line,
                    h - 2.0 * strokes,
                ),
                ctx.colours.dim(),
            );
        }
        if ctx.over_quiet(&handle) && ctx.input.pressed[0] {
            ctx.input.pressed[0] = false;
            ctx.input.captured = true;
            self.grab = Some((ctx.input.mouse.0 - x, ctx.input.mouse.1 - y));
        }
        // The grip.
        let grip = Rect::new(r.right() - grip_w, y, grip_w, h);
        for i in 0..2_u8 {
            p.fill(
                Rect::new(
                    grip.x + (3.0 + 3.0 * f32::from(i)) * k,
                    y + strokes,
                    line,
                    h - 2.0 * strokes,
                ),
                ctx.colours.dim(),
            );
        }
        if ctx.over_quiet(&grip) && ctx.input.pressed[0] {
            ctx.input.pressed[0] = false;
            ctx.input.captured = true;
            self.sizing = Some((ctx.input.mouse.0, w));
        }
        // The buttons, scrolled along where they do not all fit.
        let room = w - fixed_w - grip_w;
        let scrolls = content > room + 0.5;
        let view = Rect::new(
            handle.right(),
            y,
            if scrolls { room - arrows_w } else { room },
            h,
        );
        let most = (content - view.w).max(0.0);
        if scrolls {
            if ctx.input.hover(&r) && ctx.input.wheel != 0.0 {
                self.scroll -= ctx.input.wheel * STEP * k;
                ctx.input.wheel = 0.0;
            }
            let back = Rect::new(view.right(), y + line, h, h - 2.0 * line);
            let on = Rect::new(back.right(), y + line, h, h - 2.0 * line);
            if kit::button(p, ctx, back, "<", self.scroll > 0.0) {
                self.scroll -= STEP * k;
            }
            if kit::button(p, ctx, on, ">", self.scroll < most) {
                self.scroll += STEP * k;
            }
        }
        self.scroll = self.scroll.clamp(0.0, most);
        let mut clicked = None;
        p.list.push_clip(view);
        let mut bx = view.x + gap * k - self.scroll;
        for (item, bw) in items.iter().zip(&widths) {
            let b = Rect::new(bx, y + edge * k, *bw, h - 2.0 * edge * k);
            bx += bw + gap * k;
            // A button scrolled out of sight takes no pointer.
            let Some(shown) = b.intersect(&view) else {
                continue;
            };
            let hit = match look {
                BarLook::Plain => {
                    let over = ctx.over(&shown);
                    p.fill(b, state_colour(&item.state));
                    if over {
                        p.fill(b, p.art.palette().button_glow);
                    }
                    if item.open {
                        p.outline(b, line, 0xFFD8_B860);
                    }
                    p.text_in(&style, b, Align::Centre, &item.name);
                    over && ctx.input.clicked(&shown)
                }
                BarLook::Kit => {
                    let hit = kit::button_lit(p, ctx, b, &item.name, true, item.open)
                        && shown.contains(ctx.input.mouse.0, ctx.input.mouse.1);
                    // What the plugin is doing: a lamp at the button's left end.
                    let lamp =
                        Rect::new(b.x + 4.0 * k, b.y + b.h / 2.0 - 3.0 * k, 6.0 * k, 6.0 * k);
                    p.fill(lamp, state_colour(&item.state));
                    hit
                }
            };
            if hit {
                clicked = Some(item.plugin.clone());
            }
        }
        p.list.pop_clip();
        if ctx.input.hover(&r) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.captured = true;
        }
        (r, clicked)
    }
}
