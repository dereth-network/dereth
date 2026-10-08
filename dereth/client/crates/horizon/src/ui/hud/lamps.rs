//! The status row's lamps after the effects: vitae while death's penalty lasts, and burden while
//! more is carried than can be, each in the game's own lamp picture, each opening its window on
//! a click.

use super::Hud;
use crate::art::Family;
use crate::draw::{Rect, WHITE};
use crate::ui::game::GameState;
use crate::ui::kit::Ctx;
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::panels::info::{self, Ask};

impl Hud {
    /// The lamps, leftwards from `x` (the left edge of the next icon), `size` tall.
    pub(super) fn lamps(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        x: f32,
        size: f32,
        pitch: f32,
    ) {
        let k = p.scale;
        let mut x = x;
        let label = TextStyle::new(Family::Numerals, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        if let Some(v) = state.vitae.filter(|v| *v < 1.0) {
            let pct = dereth_primitives::num::to_i32(((1.0 - v) * 100.0).round());
            let r = lamp(p, x, size, "status.vitae", info::VITAE_ICON, 0xC060_1818);
            p.text_in(
                &label,
                Rect::new(r.x - 8.0 * k, r.bottom(), r.w + 16.0 * k, 16.0 * k),
                Align::Centre,
                &format!("{pct}%"),
            );
            if ctx.over(&r) {
                self.tip(
                    "Vitae",
                    vec![format!("{pct}% vitae penalty. Click for details.")],
                );
                if ctx.input.clicked(&r) {
                    self.asks.push(Ask::Vitae);
                }
            }
            x -= pitch;
        }
        if let Some(icon) = info::burden_icon(state.burden) {
            let load = state.burden.unwrap_or(0.0);
            let own = if load >= 2.0 {
                "status.overburden"
            } else {
                "status.burden"
            };
            let r = lamp(p, x, size, own, icon, 0xC060_4018);
            p.text_in(
                &label,
                Rect::new(r.x - 8.0 * k, r.bottom(), r.w + 16.0 * k, 16.0 * k),
                Align::Centre,
                &crate::ui::panels::burden_percent(load),
            );
            if ctx.over(&r) {
                let what = if load >= 2.0 {
                    "Overburdened"
                } else {
                    "Burdened"
                };
                self.tip(
                    what,
                    vec![format!(
                        "Carrying {} of what you can. Click for details.",
                        crate::ui::panels::burden_percent(load)
                    )],
                );
                if ctx.input.clicked(&r) {
                    self.asks.push(Ask::Burden);
                }
            }
        }
    }
}

/// A lamp whose right edge is where the next effect icon's would be (`x + size`): the
/// interface's own icon `own`, square; else the game's picture `icon` at its own proportions and
/// `size` tall, or a plain square of `ground` without it. Where it was drawn.
fn lamp(p: &mut Painter<'_>, x: f32, size: f32, own: &str, icon: u32, ground: u32) -> Rect {
    let right = x + size;
    let at = |w: f32| Rect::new(right - w, 44.0 * p.scale, w, size);
    if let Some(s) = p.piece(own) {
        let r = at(size);
        p.sprite(&s, r, WHITE);
        return r;
    }
    match p.art.ac_icon(icon) {
        Some(s) => {
            let w = if s.h > 0.0 { size * s.w / s.h } else { size };
            let r = at(w);
            p.sprite(&s, r, WHITE);
            r
        }
        None => {
            let r = at(size);
            p.fill(r, ground);
            r
        }
    }
}
