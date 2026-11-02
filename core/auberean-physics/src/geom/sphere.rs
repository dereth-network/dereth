//! This world's own sphere sweeps, where they differ from the shared engine's.

use dereth_primitives::Vec3;

use super::EPS;

/// When a sphere moving by `movement` first touches another whose center lies `delta` away, the
/// two radii summing to `radius`: the time along the move (1 the whole move), worked wide as the
/// client works it; -1 when they already overlap within 0.0002, the move is shorter than 0.0002
/// (squared, kept wide: the shared engine compares with the rounded square), or they never meet.
// D2-04213
#[must_use]
pub fn time_of_collision(movement: Vec3, delta: Vec3, radius: f32) -> f64 {
    let w = f64::from;
    let (m, d) = (movement, delta);
    let a = (w(m.x) * w(m.x) + w(m.y) * w(m.y)) + w(m.z) * w(m.z);
    let b = -((w(d.z) * w(m.z) + w(d.y) * w(m.y)) + w(m.x) * w(d.x));
    let c = ((w(d.x) * w(d.x) + w(d.y) * w(d.y)) + w(d.z) * w(d.z)) - w(radius) * w(radius);
    // Each test refuses only an ordered comparison that fails; a number that is not one passes.
    if c < w(EPS) {
        return -1.0;
    }
    let disc = b * b - c * a;
    if a < w(EPS) * w(EPS) || disc < 0.0 {
        return -1.0;
    }
    let q = disc.sqrt();
    let t = b - q;
    if t >= 0.0 {
        t / a
    } else {
        (q + b) / a
    }
}
