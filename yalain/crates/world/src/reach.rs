//! How far apart two bodies are, as a skill's reach is measured: each body a standing cylinder
//! (its look's radius and height, times its scale), the distance between the cylinders' sides
//! across the ground and between the lower one's top and the upper one's foot in height. A skill
//! is too far beyond its greatest range, too close within its least; a basic attack swings with
//! the attacks whose ranges hold that distance.

use yalain_store::{Store, StoredPosition};

use crate::Server;

/// A body's cylinder: its radius and height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub radius: f32,
    pub height: f32,
}

/// A player's body (every race's setups are this size, at the character's own scale; a new human's
/// scale is 1).
pub const PLAYER: Body = Body {
    radius: 0.7849,
    height: 2.06,
};

/// A creature's body when its look's is not known (the hall's practice dummy's, as its setup holds
/// it).
#[allow(clippy::approx_constant)]
pub const CREATURE: Body = Body {
    radius: 0.7071,
    height: 1.995,
};

/// A thing's body: its template's look's, times the thing's scale.
#[must_use]
pub fn body_of<S: Store>(s: &Server<S>, id: u64) -> Body {
    let Some(t) = s.things.get(id) else {
        return CREATURE;
    };
    let scale = t.description.scale.unwrap_or(1.0);
    s.catalog
        .bodies
        .get(&t.template)
        .map_or(CREATURE, |[r, h]| Body {
            radius: r * scale,
            height: h * scale,
        })
}

/// The distance between two bodies' cylinders, for places in one landblock or neighboring ones
/// (else none): across the ground less both radii, in height less the lower one's height, each
/// none when they overlap.
#[must_use]
pub fn gap(a: &StoredPosition, ab: Body, b: &StoredPosition, bb: Body) -> Option<f32> {
    let (la, lb) = (a.cell >> 16, b.cell >> 16);
    let dx = i64::from(la >> 8) - i64::from(lb >> 8);
    let dy = i64::from(la & 0xFF) - i64::from(lb & 0xFF);
    if dx.abs() > 1 || dy.abs() > 1 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let (ox, oy) = (dx as f32 * 160.0, dy as f32 * 160.0);
    let x = a.origin[0] + ox - b.origin[0];
    let y = a.origin[1] + oy - b.origin[1];
    let across = ((x * x + y * y).sqrt() - (ab.radius + bb.radius)).max(0.0);
    let (za, zb) = (a.origin[2], b.origin[2]);
    let up = if za >= zb {
        (za - (zb + bb.height)).max(0.0)
    } else {
        (zb - (za + ab.height)).max(0.0)
    };
    Some((across * across + up * up).sqrt())
}
