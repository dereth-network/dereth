//! The cross hotbars, shown over the hotbar in gamepad mode ([`crate::gamepad`]): two crosses of
//! eight places, the left one LT's (the first eight slots of the shortcut bar's set shown) and
//! the right one RT's (the first eight of the row over it). Each cross is the d-pad's four places
//! beside the face buttons' four, in the pad's own order: up, right, down, left; Y, B, A, X. The
//! cross a trigger holds up is drawn lit, the other dimmed.

use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};

use crate::draw::{Rect, WHITE};
use crate::gamepad::{cross_slot, CROSS_PLACES};
use crate::pad::CrossSet;
use crate::state::GameState;
use crate::ui::hotbar::{slot_rect, SHADE, TILE};

/// A place's side, and how far a place stands from its cluster's middle, in layout units.
const PLACE: f32 = 28.0;
const SPREAD: f32 = 29.0;
/// How far each cluster's middle stands from its cross's, and each cross's from the screen's.
const CLUSTER: f32 = 46.0;
const CROSS: f32 = 100.0;
/// How far over the hotbar's top row the crosses' middles stand.
const OVER_BAR: f32 = 64.0;

/// The colour of the cross not held up.
const DIMMED: u32 = 0x70FF_FFFF;

/// Where place `place` (0 to 7: the d-pad's up, right, down, left, then the face buttons' top,
/// right, bottom, left) of cross `half` is, over a hotbar of `rows` rows on a screen drawn at
/// `k` pixels a layout unit.
#[must_use]
pub fn place_rect(screen: (f32, f32), k: f32, rows: u32, half: CrossSet, place: usize) -> Rect {
    let top = slot_rect(screen, k, rows.saturating_sub(1), 0);
    let side = match half {
        CrossSet::Left => -1.0,
        CrossSet::Right => 1.0,
    };
    let cx = screen.0 / 2.0 + side * CROSS * k;
    let cy = top.y - OVER_BAR * k;
    let cluster = if place < 4 { -1.0 } else { 1.0 };
    let (dx, dy) = match place % 4 {
        0 => (0.0, -1.0),
        1 => (1.0, 0.0),
        2 => (0.0, 1.0),
        _ => (-1.0, 0.0),
    };
    let mx = cx + cluster * CLUSTER * k + dx * SPREAD * k;
    let my = cy + dy * SPREAD * k;
    Rect::new(
        (mx - PLACE * k / 2.0).round(),
        (my - PLACE * k / 2.0).round(),
        (PLACE * k).round(),
        (PLACE * k).round(),
    )
}

/// Where the two crosses are over a hotbar of `rows` rows, their places and their names over
/// them.
#[must_use]
pub fn home(screen: (f32, f32), k: f32, rows: u32) -> Rect {
    let left = place_rect(screen, k, rows, CrossSet::Left, 3);
    let right = place_rect(screen, k, rows, CrossSet::Right, 5);
    let top = place_rect(screen, k, rows, CrossSet::Left, 0).y - 18.0 * k;
    let bottom = place_rect(screen, k, rows, CrossSet::Left, 2).bottom();
    Rect::new(left.x, top, right.right() - left.x, bottom - top)
}

/// The cross hotbars shown, in gamepad mode: the one a trigger holds up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Crosses {
    pub held: Option<CrossSet>,
}

/// The two crosses, `held` the one a trigger holds up; where they are.
pub fn frame(p: &mut Painter<'_>, state: &GameState, held: Option<CrossSet>) -> Vec<Rect> {
    let Some(bar) = &state.bar else {
        return Vec::new();
    };
    let k = p.scale;
    let rows = bar.rows.max(1);
    let label = TextStyle::new(Family::Heading, 13.0, 0xFFE8_E0C8).edge(0xFF00_0000);
    let mut drawn = Vec::new();
    for (half, name) in [(CrossSet::Left, "LT"), (CrossSet::Right, "RT")] {
        let lit = held == Some(half);
        let tint = if lit { WHITE } else { DIMMED };
        let up = place_rect(p.screen, k, rows, half, 0);
        p.text_in(
            &label,
            Rect::new(up.x - 40.0 * k, up.y - 18.0 * k, up.w + 80.0 * k, 16.0 * k),
            Align::Centre,
            name,
        );
        for place in 0..CROSS_PLACES as usize {
            let r = place_rect(p.screen, k, rows, half, place);
            let slot = cross_slot(bar.first(), half, place).and_then(|n| bar.slot(n));
            let icon = slot
                .filter(|s| !s.is_empty())
                .and_then(|s| p.art.icon(s.icon));
            match icon {
                Some(i) => p.sprite(&i, r, tint),
                None => p.fill(r, TILE),
            }
            if !lit {
                p.fill(r, SHADE & 0x60FF_FFFF);
            }
            if lit {
                p.outline(r, 1.0_f32.max(k), 0xC0E8_C890);
            }
            drawn.push(r);
        }
    }
    drawn
}
