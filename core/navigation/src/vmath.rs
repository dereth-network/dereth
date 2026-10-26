//! The few vector operations the planner needs, as free functions over [`Vec3`].

use dereth_primitives::Vec3;

#[inline]
pub(crate) fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

#[inline]
pub(crate) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

#[inline]
pub(crate) fn scale(a: Vec3, s: f32) -> Vec3 {
    Vec3::new(a.x * s, a.y * s, a.z * s)
}

#[inline]
pub(crate) fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

#[inline]
pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

#[inline]
pub(crate) fn len(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}

#[inline]
pub(crate) fn dist(a: Vec3, b: Vec3) -> f32 {
    len(sub(a, b))
}

#[inline]
pub(crate) fn dist_xy(a: Vec3, b: Vec3) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

/// The squared distance from `p` to the segment `a..b`.
pub(crate) fn point_segment_dist_sq(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = sub(b, a);
    let ap = sub(p, a);
    let l2 = dot(ab, ab);
    let t = if l2 > 0.0 {
        (dot(ap, ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let c = add(a, scale(ab, t));
    let d = sub(p, c);
    dot(d, d)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (arithmetic helpers of the planner).
    use super::*;

    #[test]
    fn segment_distance_clamps_to_the_nearer_end() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(2.0, 0.0, 0.0);
        assert!((point_segment_dist_sq(Vec3::new(1.0, 1.0, 0.0), a, b) - 1.0).abs() < 1e-6);
        assert!((point_segment_dist_sq(Vec3::new(3.0, 0.0, 0.0), a, b) - 1.0).abs() < 1e-6);
        assert!((point_segment_dist_sq(Vec3::new(-2.0, 0.0, 0.0), a, b) - 4.0).abs() < 1e-6);
    }
}
