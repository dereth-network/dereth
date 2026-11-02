//! How a moving body meets another object's spheres and upright cylinders, as this world's client
//! tests them.
//!
//! The shared transition (`dereth-physics`) tests a candidate object in one of three ways: its
//! parts' physics trees, else its cylinders, else its spheres. The trees are the shared engine's;
//! the sphere and cylinder tests here are this world's own, held to its client to the bit: they
//! differ from the shared ones in their thresholds (a slide refused only when the two surfaces are
//! parallel within 0.0002 squared, not 0.0002), in what they do when the mover is landing on what it
//! met (no refusal of a move too short to measure, the contact plane never marked as water), and
//! throughout in their arithmetic, which the client works in double precision and rounds to single
//! only where it stores. Each test's arms (the step down onto it, the step up, the slide along it,
//! the landing on it, the point collision of a mover that clips its path) are in [`sphere`] and
//! [`cylinder`]; [`object_geometry`] is the three-way choice with them in place of the shared ones.

pub mod cylinder;
pub mod sphere;

use dereth_physics::geom::Sphere;
use dereth_physics::transition::collide::{object_part_find_obj_collisions_scaled, ObjGeometry};
use dereth_physics::transition::{CollisionInfo, Transition, TransitionCtx, TreeScale};
use dereth_physics::{PhysicsState, TransitionState};
use dereth_primitives::shape::Plane;
use dereth_primitives::{Position, Vec3};

use crate::geom::{PlaneOps, EPS};

pub(crate) fn w(x: f32) -> f64 {
    f64::from(x)
}

/// A value rounded to single precision, as the client rounds where it stores.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn n(x: f64) -> f32 {
    x as f32
}

/// The square of 0.0002 as the client works it: the product kept wide.
pub(crate) fn eps_sq() -> f64 {
    w(EPS) * w(EPS)
}

/// How the transition steps the mover up onto what it meets: the shared transition's step up in a
/// world ([`WorldStepUp`]), something that answers as told in a test.
pub trait StepUp {
    /// Steps the mover of `t` up, the face it met turned toward `normal`: whether it found ground
    /// to stand on. Either way the contact plane is left cleared and `normal` is the step-up normal.
    fn step_up(&mut self, t: &mut Transition, normal: Vec3) -> bool;
}

/// The shared transition's step up, through the world's cells.
pub struct WorldStepUp<'c, 'a>(pub &'c TransitionCtx<'a>);

impl std::fmt::Debug for WorldStepUp<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorldStepUp")
    }
}

impl StepUp for WorldStepUp<'_, '_> {
    fn step_up(&mut self, t: &mut Transition, normal: Vec3) -> bool {
        dereth_physics::transition::walk::step_up(self.0, t, normal)
    }
}

/// Whether a sphere `d` away overlaps within `r`: the squared length summed x, y, z, wide, against
/// `r` squared.
// D2-04214
#[must_use]
pub fn overlaps(d: Vec3, r: f32) -> bool {
    w(r) * w(r) >= (w(d.x) * w(d.x) + w(d.y) * w(d.y)) + w(d.z) * w(d.z)
}

/// Sets the collision normal: `v` made unit length (wide, each part rounded as stored), or zero
/// when it is shorter than 0.0002. It counts as set either way.
// D2-03487
pub fn set_collision_normal(ci: &mut CollisionInfo, v: Vec3) {
    ci.collision_normal_valid = true;
    let len = ((w(v.x) * w(v.x) + w(v.y) * w(v.y)) + w(v.z) * w(v.z)).sqrt();
    ci.collision_normal = if len < w(EPS) {
        Vec3::ZERO
    } else {
        let k = 1.0 / len;
        Vec3::new(n(k * w(v.x)), n(k * w(v.y)), n(k * w(v.z)))
    };
}

/// Sets the contact plane: neither water, the land's ground nor an interior's own surface.
// D2-04113
pub fn set_contact_plane(ci: &mut CollisionInfo, plane: Plane) {
    ci.set_contact_plane_on(plane, false, false, false);
}

/// The sphere path's step-up slide: the contact plane forgotten, no longer stepping up, the first
/// sphere slid along the step-up normal ([`sphere::slide`]).
// D2-05815
pub fn step_up_slide(t: &mut Transition) -> TransitionState {
    t.collision_info.clear_contact_plane();
    t.sphere_path.step_up = false;
    let s = t.sphere_path.global_sphere[0];
    let mut normal = t.sphere_path.step_up_normal;
    let curr = t.sphere_path.global_curr_center[0];
    // A slide that collides against a parallel plane turns the step-up normal back along the move.
    let r = sphere::slide_turning(t, &s, &mut normal, curr);
    t.sphere_path.step_up_normal = normal;
    r
}

/// The plane through `point` facing `normal`, its offset worked wide.
pub(crate) fn plane_through(normal: Vec3, point: Vec3) -> Plane {
    Plane::through(normal, point)
}

/// One candidate object's geometry against the mover: every part's physics tree (the shared test,
/// at `trees`) when it has trees, else each of its cylinders, else each of its spheres (all of
/// them, not only the first two the mover's own path takes), with this world's tests; the first
/// answer that is not OK wins. Its parts are placed at `part_scale`; a cylinder or sphere at `pos`
/// at the scale `scale` along each axis. `creature` is whether the candidate is a creature or a
/// missile, which a mover never steps down onto nor settles onto after a landing.
#[allow(clippy::too_many_arguments)]
pub fn object_geometry(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    state: PhysicsState,
    geom: ObjGeometry<'_>,
    part_scale: f32,
    trees: TreeScale,
    pos: &Position,
    scale: Vec3,
    creature: bool,
) -> TransitionState {
    let mut up = WorldStepUp(ctx);
    if state.has_physics_bsp() {
        let ObjGeometry::Setup(g, frames) = geom else {
            return TransitionState::Ok;
        };
        for i in 0..g.parts.len() {
            let Some(part) = g.placed_part_posed(i, pos, part_scale, frames) else {
                break;
            };
            let r = object_part_find_obj_collisions_scaled(ctx, t, &part, trees);
            if r != TransitionState::Ok {
                return r;
            }
        }
        return TransitionState::Ok;
    }
    let (spheres, cylinders): (&[Sphere], &[dereth_physics::geom::CylSphere]) = match geom {
        ObjGeometry::Spheres(s) => (s, &[]),
        ObjGeometry::Setup(g, _) => (&g.spheres, &g.cyl_spheres),
    };
    if !cylinders.is_empty() {
        for c in cylinders {
            let r = cylinder::meet_placed(t, &mut up, c, pos, scale);
            if r != TransitionState::Ok {
                return r;
            }
        }
        return TransitionState::Ok;
    }
    for s in spheres {
        let r = sphere::meet_placed(t, &mut up, s, pos, scale, creature);
        if r != TransitionState::Ok {
            return r;
        }
    }
    TransitionState::Ok
}
