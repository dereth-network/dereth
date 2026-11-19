//! The selection's marks in the world: a ring on the ground under a selected player or creature,
//! turning slowly (laid by the world as it is drawn, [`ground`]), and its name centered over the
//! box the selection fills on the screen with a single arrow over the name pointing down at it,
//! bobbing a little; both in the art's own colors, whatever the selection is and whatever the
//! player does. While none of the box is on the screen, one arrow at the screen's edge points the
//! way to it instead.
//!
//! The box is the one the selection's drawn shape fills, averaged over its last few frames so the
//! marks do not shake as it moves. The marks are drawn with the rest of the interface, before the
//! world moves on for the frame, and moved once it has to where the selection is as the world is
//! drawn ([`follow`]), so they never trail a turning camera.

use std::collections::VecDeque;

use auberean_primitives::Vec3;
use auberean_scene::seen::Seen;
use auberean_shell::interface::GroundMarker;
use dereth_primitives::num::math as fm;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::draw::{Anchor, Spot};
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::selection::{self, ARROW_PIECE, UNTINTED};

use crate::draw::{Argb, DrawList, Rect};
use crate::state::GameState;

/// Where the selection is on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Where {
    /// The box it fills, in pixels; some of it on the screen.
    Box(Rect),
    /// None of it is on the screen: the way to it from the screen's middle, one long, x to the
    /// right and y down.
    Off { toward: (f32, f32) },
}

/// The color of the plain square drawn for an arrow where the art has none. The art's own arrow
/// is never tinted.
pub const MARK: Argb = 0xFF7C_C8FF;

/// How far the name stands over the top of the selection's box, in layout units.
pub const NAME_GAP: f32 = 4.0;

/// The edge arrow's side, and how far the screen's edges keep it in, in layout units: clear of
/// the hotbar at the foot.
pub const EDGE: f32 = 32.0;
pub const EDGE_INSET: f32 = 24.0;
pub const EDGE_FOOT: f32 = 150.0;

/// How many frames the box is averaged over.
pub const AVERAGE_FRAMES: usize = 4;

/// The quads of the marks are kept as one group, by this id, to move after the world moves.
const GROUP: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0);

/// The selection's recent boxes, in its own frame, for the average.
#[derive(Clone, Debug, Default)]
pub struct Recent {
    id: u64,
    boxes: VecDeque<(Vec3, Vec3)>,
}

impl Recent {
    /// The box of `id` as the last world frame drew it, averaged with its last few; none when
    /// nothing of it was drawn. A new selection starts the average again.
    pub fn average(&mut self, seen: &Seen, id: u64) -> Option<(Vec3, Vec3)> {
        if id != self.id {
            self.id = id;
            self.boxes.clear();
        }
        let body = seen.body(id)?;
        let b = body.bounds.unwrap_or((Vec3::ZERO, Vec3::ZERO));
        self.boxes.push_back(b);
        while self.boxes.len() > AVERAGE_FRAMES {
            self.boxes.pop_front();
        }
        #[allow(clippy::cast_precision_loss)]
        let n = self.boxes.len() as f32;
        let sum = |f: fn(&(Vec3, Vec3)) -> Vec3| {
            let s = self.boxes.iter().map(f).fold(Vec3::ZERO, |a, v| {
                Vec3::new(a.x + v.x, a.y + v.y, a.z + v.z)
            });
            Vec3::new(s.x / n, s.y / n, s.z / n)
        };
        Some((sum(|b| b.0), sum(|b| b.1)))
    }
}

/// Where the selection `id`, whose box in its own frame is `bounds`, is on a screen of `screen`
/// pixels as `seen` drew it; none when it was not drawn (or has faded out) or there is no camera.
#[must_use]
pub fn locate(seen: &Seen, id: u64, bounds: (Vec3, Vec3), screen: (f32, f32)) -> Option<Where> {
    let camera = seen.camera.as_ref()?;
    let body = seen.body(id).filter(|b| !b.faded)?;
    let (w, h) = screen;
    let mut lo = (f32::MAX, f32::MAX);
    let mut hi = (f32::MIN, f32::MIN);
    let mut any = false;
    for c in body.corners(bounds) {
        if let Some((x, y, _)) = camera.project(c, w, h) {
            any = true;
            lo = (lo.0.min(x), lo.1.min(y));
            hi = (hi.0.max(x), hi.1.max(y));
        }
    }
    let middle = |b: (Vec3, Vec3)| {
        body.to_block(Vec3::new(
            (b.0.x + b.1.x) * 0.5,
            (b.0.y + b.1.y) * 0.5,
            (b.0.z + b.1.z) * 0.5,
        ))
    };
    if any {
        let b = Rect::new(lo.0, lo.1, hi.0 - lo.0, hi.1 - lo.1);
        if b.right() >= 0.0 && b.x <= w && b.bottom() >= 0.0 && b.y <= h {
            return Some(Where::Box(b));
        }
        let toward = unit((b.x + b.w / 2.0 - w / 2.0, b.y + b.h / 2.0 - h / 2.0));
        return Some(Where::Off { toward });
    }
    // All of it behind the camera: the way to it across the screen, as the camera turns to it.
    let (_, right, up) = camera.axes();
    let d = auberean_world::math::sub(middle(bounds), camera.position);
    let toward = unit((
        auberean_world::math::dot(d, right),
        -auberean_world::math::dot(d, up),
    ));
    Some(Where::Off { toward })
}

/// `v` made one long; straight down when it has no length.
fn unit(v: (f32, f32)) -> (f32, f32) {
    let l = (v.0 * v.0 + v.1 * v.1).sqrt();
    if l < 1e-6 {
        (0.0, 1.0)
    } else {
        (v.0 / l, v.1 / l)
    }
}

/// How far a picture pointing down is turned clockwise to point along `toward`.
fn turn_toward(toward: (f32, f32)) -> f32 {
    fm::atan2f(-toward.0, toward.1)
}

/// The edge arrow for a selection off the screen the way `toward` goes, on a screen of `screen`
/// pixels at `k` pixels a layout unit: its square, on the way out from the screen's middle where
/// it meets the edges kept clear, and how far it is turned from pointing down.
#[must_use]
pub fn edge_rect(screen: (f32, f32), k: f32, toward: (f32, f32)) -> (Rect, f32) {
    let side = EDGE * k;
    let (x0, y0) = (EDGE_INSET * k + side / 2.0, EDGE_INSET * k + side / 2.0);
    let (x1, y1) = (
        screen.0 - EDGE_INSET * k - side / 2.0,
        screen.1 - EDGE_FOOT * k - side / 2.0,
    );
    let (cx, cy) = (screen.0 / 2.0, screen.1 / 2.0);
    let reach = |d: f32, from: f32, lo: f32, hi: f32| {
        if d > 1e-6 {
            (hi - from) / d
        } else if d < -1e-6 {
            (lo - from) / d
        } else {
            f32::MAX
        }
    };
    let t = reach(toward.0, cx, x0, x1)
        .min(reach(toward.1, cy, y0, y1))
        .max(0.0);
    let (x, y) = (
        (cx + toward.0 * t).clamp(x0, x1.max(x0)),
        (cy + toward.1 * t).clamp(y0, y1.max(y0)),
    );
    (
        Rect::new(x - side / 2.0, y - side / 2.0, side, side),
        turn_toward(toward),
    )
}

/// An arrow in `r` turned `turn` from pointing down: the art's arrow in its own colors, or where
/// the art has none a plain square.
fn arrow(p: &mut Painter<'_>, r: Rect, turn: f32) {
    match p.piece(ARROW_PIECE) {
        Some(s) => {
            let h = r.h;
            let w = h * s.w / s.h.max(1.0);
            let r = Rect::new(r.x + (r.w - w) / 2.0, r.y, w, h);
            p.sprite_turned(&s, r, UNTINTED, turn);
        }
        None => p.fill(r.inset(r.w * 0.25), MARK),
    }
}

/// Where the name over box `b` stands, `line` pixels high at `k` pixels a layout unit: centered
/// over the box, its foot [`NAME_GAP`] over the box's top.
#[must_use]
pub fn name_rect(b: Rect, k: f32, line: f32) -> Rect {
    Rect::new(
        b.x - 200.0 * k,
        b.y - NAME_GAP * k - line,
        b.w + 400.0 * k,
        line,
    )
}

/// Where the arrow over box `b` stands at `time` seconds, the top of what it points at being
/// `top` (the name's, or the box's with no name): centered over the box, pointing down, bobbing.
/// The art's arrow's shape, or a square where it has none.
#[must_use]
pub fn arrow_over(p: &Painter<'_>, b: Rect, top: f32, time: f64) -> Rect {
    let piece = p.piece(ARROW_PIECE).unwrap_or(dereth_ui_kit::art::Sprite {
        tex: 0,
        src: Rect::new(0.0, 0.0, 1.0, 1.0),
        w: selection::ARROW_HEIGHT,
        h: selection::ARROW_HEIGHT,
        scale: 1.0,
    });
    selection::arrow_rect(&piece, b.x, top, b.w, p.scale, time)
}

/// The selection's marks, where `state` says it is, at `time` seconds.
pub fn draw(p: &mut Painter<'_>, state: &GameState, time: f64) {
    let (Some(target), Some(at)) = (&state.target, state.selection_at) else {
        return;
    };
    let k = p.scale;
    match at {
        Where::Box(b) => {
            let from = p.list.mark();
            let mut top = b.y;
            if !target.name.is_empty() {
                let name = TextStyle::new(Family::Body, 14.0, 0xFFFF_FFFF).edge(0xFF00_0000);
                let line = p.line_height(&name);
                let r = name_rect(b, k, line);
                p.text_in(&name, r, Align::Centre, &target.name);
                top = r.y;
            }
            let r = arrow_over(p, b, top, time);
            arrow(p, r, 0.0);
            let middle = (b.x + b.w / 2.0, b.y + b.h / 2.0);
            p.list.anchor(GROUP, Anchor::Origin, from, middle, middle);
        }
        Where::Off { toward } => {
            let (r, turn) = edge_rect(p.screen, k, toward);
            arrow(p, r, turn);
        }
    }
}

/// The marks the world lays on the ground for `state`'s selection at `time` seconds: the ring
/// under a player or a creature, in the art's own colors, with its inner layer under it where the
/// art has one (`inner`); none for a thing, or with nothing selected.
#[must_use]
pub fn ground(state: &GameState, time: f64, inner: bool) -> Vec<GroundMarker<u64>> {
    let target = state
        .target
        .as_ref()
        .filter(|_| state.ringed)
        .map(|t| (t.id, UNTINTED));
    selection::markers(target, time, inner)
}

/// Move the marks drawn in `list` to where the selection's box is now, `now` (where the world as
/// it is drawn this frame puts it), or hide them when it is no longer on the screen.
pub fn follow(list: &mut DrawList, now: Option<Where>) {
    list.follow(|_, _| match now {
        Some(Where::Box(b)) => Some(Spot::at(b.x + b.w / 2.0, b.y + b.h / 2.0)),
        _ => None,
    });
}
