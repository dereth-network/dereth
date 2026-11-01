//! A ray through a physics tree: where it first reaches the tree's solid side.
//!
//! The ray is tested against the root's bounding sphere first. A node sends a ray that stays on one
//! side of its plane (within a tenth of a meter of it counting as on it) to that side alone; a ray
//! that crosses is cut where it meets the plane, and the near piece is tried before the far one. A
//! solid leaf is met at once.

use dereth_physics::geom::{BspNodeKind, BspTree};
use dereth_primitives::Vec3;

use crate::geom::{Ray, SphereOps, EPS};

/// How far from a plane a point still counts as on it.
const ON_PLANE: f32 = 0.1;

/// Which side of a node's plane a point is on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Front,
    Behind,
    On,
}

/// Where `ray` first reaches the solid side of `tree`, as a fraction of its length, in `fraction`;
/// `normal` takes the plane met when the solid side starts right at a cut (the plane's normal facing
/// back toward where the ray came from), and is left as it was otherwise. False, with the fraction
/// 1, when the ray reaches nothing solid; false, with the fraction untouched, when it misses the
/// tree's bounding sphere.
// D2-04116
pub fn trace_ray(tree: &BspTree, ray: &Ray, fraction: &mut f32, normal: &mut Vec3) -> bool {
    let Some(root) = tree.root() else {
        return false;
    };
    if root.sphere.trace_ray(ray).is_none() {
        return false;
    }
    node_trace_ray(tree, 0, ray, fraction, normal)
}

/// [`trace_ray`] without the bounding sphere first: the ray goes straight to the root's plane, as
/// a room traces its own walls. False, with the fraction untouched, for an empty tree.
pub fn trace_ray_from_root(
    tree: &BspTree,
    ray: &Ray,
    fraction: &mut f32,
    normal: &mut Vec3,
) -> bool {
    tree.root().is_some() && node_trace_ray(tree, 0, ray, fraction, normal)
}

/// One node's part of [`trace_ray`]: a leaf is met at its start when it is solid; a node's plane
/// sends the ray to one side or cuts it.
// D2-04147, D2-04159
fn node_trace_ray(
    tree: &BspTree,
    at: u32,
    ray: &Ray,
    fraction: &mut f32,
    normal: &mut Vec3,
) -> bool {
    let Some(node) = tree.nodes.get(at as usize) else {
        *fraction = 1.0;
        return false;
    };
    if let BspNodeKind::Leaf { solid, .. } = node.kind {
        *fraction = if solid { 0.0 } else { 1.0 };
        return solid;
    }
    let start = ray.point;
    let len = ray.length;
    let end = Vec3::new(
        len * ray.dir.x + start.x,
        len * ray.dir.y + start.y,
        len * ray.dir.z + start.z,
    );
    let p = &node.splitting_plane;
    let dist = |v: Vec3| ((v.z * p.normal.z + v.y * p.normal.y) + v.x * p.normal.x) + p.d;
    let d_start = dist(start);
    let from = if d_start > ON_PLANE {
        Side::Front
    } else if d_start >= -ON_PLANE {
        Side::On
    } else {
        Side::Behind
    };
    let d_end = dist(end);
    let to = if d_end > ON_PLANE {
        Side::Front
    } else if d_end < -ON_PLANE {
        Side::Behind
    } else {
        Side::On
    };
    if let Some(front) = node.pos_child {
        if from != Side::Behind && to != Side::Behind {
            return node_trace_ray(tree, front, ray, fraction, normal);
        }
    }
    if let Some(back) = node.neg_child {
        if from != Side::Front && to != Side::Front {
            return node_trace_ray(tree, back, ray, fraction, normal);
        }
    }
    let (near, far) = if from == Side::Behind {
        (node.neg_child, node.pos_child)
    } else {
        (node.pos_child, node.neg_child)
    };
    let crossing = d_start - d_end;
    let t = if crossing.abs() > EPS {
        d_start / crossing
    } else {
        0.0
    };
    let mid = Vec3::new(
        (end.x - start.x) * t + start.x,
        (end.y - start.y) * t + start.y,
        (end.z - start.z) * t + start.z,
    );
    let facing = |normal: &mut Vec3| {
        *normal = if from == Side::Behind {
            Vec3::new(-p.normal.x, -p.normal.y, -p.normal.z)
        } else {
            p.normal
        };
    };
    let mut sub = 0.0;
    let first = Ray::from_offset(
        start,
        Vec3::new(mid.x - start.x, mid.y - start.y, mid.z - start.z),
    );
    if let Some(n) = near {
        if node_trace_ray(tree, n, &first, &mut sub, normal) {
            *fraction = t * sub;
            if sub.abs() < EPS {
                facing(normal);
            }
            return true;
        }
    }
    let second = Ray::from_offset(mid, Vec3::new(end.x - mid.x, end.y - mid.y, end.z - mid.z));
    if let Some(f) = far {
        if node_trace_ray(tree, f, &second, &mut sub, normal) {
            *fraction = (1.0 - t) * sub + t;
            if sub.abs() < EPS {
                facing(normal);
            }
            return true;
        }
    }
    *fraction = 1.0;
    false
}
