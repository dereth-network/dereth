//! How near a portal a body may stand without being taken: the portal's reach.
//!
//! A portal takes a body that touches it. A portal's body does not block (it is ethereal), so
//! touching is plain overlap: the walking body's spheres against the portal's collision shapes,
//! each scaled by its owner's scale, with no stepping or sliding. The portal's collision is one
//! kind of shape, whichever its setup has first of a mesh, cylinders or spheres:
//!
//! - a sphere is touched by a body sphere whose centre is within the two radii's sum, less
//!   0.0002;
//! - a cylinder by a body sphere within its radius (less 0.0002) plus the sphere's across, and
//!   whose centre lies within the cylinder's height, the sphere's radius (less 0.0002) beyond
//!   either end;
//! - a mesh by a body sphere that reaches its polygons. Its reach here is bounded by each mesh
//!   part's own bounding sphere, placed where the part hangs, taken as a sphere: a body that
//!   reaches the polygons reaches the sphere around them, so a body beyond that sphere's reach is
//!   beyond the mesh's too. The bound can only be longer than the true reach, never shorter.
//!
//! The reach is measured for a body standing on the ground the portal stands on, as the walk to
//! it ends: the furthest across that any of the body's spheres touches any of the portal's shapes.

use dereth_physics::SetupGeometry;
use dereth_primitives::frame::V3;
use dereth_primitives::num::math::{cosf, hypotf, sinf};
use dereth_primitives::Vec3;

/// The radii's sum is short by this, in every test of touching.
const TOUCH_EPSILON: f32 = 0.0002;

/// How much further than a portal's reach a walk that is not taking it ends, meters: a body
/// stops within a third of a meter of where it was walked to, and the server's last word of where
/// it stood may lag where it stopped.
pub const MARGIN: f32 = 1.0;

/// A body sphere, as it hangs from the body's origin: its height above it and its radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodySphere {
    pub height: f32,
    pub radius: f32,
}

/// The body of a human character: two spheres of 0.48 m, centred 0.475 m and 1.35 m up. The one
/// the reach is measured for until the character's own is known.
pub const HUMAN_BODY: [BodySphere; 2] = [
    BodySphere {
        height: 0.475,
        radius: 0.48,
    },
    BodySphere {
        height: 1.35,
        radius: 0.48,
    },
];

/// The body spheres of `setup`, a body's collision geometry, at `scale`. Empty for a setup with
/// none.
#[must_use]
pub fn body_of(setup: &SetupGeometry, scale: f32) -> Vec<BodySphere> {
    setup
        .spheres
        .iter()
        .map(|s| BodySphere {
            height: s.center.z * scale,
            radius: s.radius * scale,
        })
        .collect()
}

/// A shape of the portal's, in the portal's own frame, scaled.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Shape {
    Sphere { center: Vec3, radius: f32 },
    Cylinder { low: Vec3, radius: f32, height: f32 },
}

/// The shapes a body may touch of a portal of collision geometry `setup` at `scale`: its mesh
/// parts' bounding spheres when it collides as a mesh, else its cylinders, else its spheres.
fn shapes(setup: &SetupGeometry, scale: f32) -> Vec<Shape> {
    if setup.caches_physics_bsp() {
        return setup
            .parts
            .iter()
            .filter_map(|p| {
                let s = p.physics_bsp.as_ref()?.root()?.sphere;
                // Only the part scale's height reaches collision.
                let part_scale = p.default_scale.z;
                let at = dereth_primitives::frame::localtoglobal(
                    &p.placement_frame,
                    s.center.mul(part_scale),
                );
                Some(Shape::Sphere {
                    center: at.mul(scale),
                    radius: s.radius * part_scale * scale,
                })
            })
            .collect();
    }
    if !setup.cyl_spheres.is_empty() {
        return setup
            .cyl_spheres
            .iter()
            .map(|c| Shape::Cylinder {
                low: c.low_pt.mul(scale),
                radius: c.radius * scale,
                height: c.height * scale,
            })
            .collect();
    }
    setup
        .spheres
        .iter()
        .map(|s| Shape::Sphere {
            center: s.center.mul(scale),
            radius: s.radius * scale,
        })
        .collect()
}

/// How far across from a portal of collision geometry `setup` at `scale` a body of `body` touches
/// it, standing on the portal's ground; `0` for a portal no such body can touch.
#[must_use]
pub fn reach(setup: &SetupGeometry, scale: f32, body: &[BodySphere]) -> f32 {
    let mut furthest = 0.0_f32;
    for shape in shapes(setup, scale) {
        for b in body {
            let across = match shape {
                Shape::Sphere { center, radius } => {
                    let sum = radius + b.radius - TOUCH_EPSILON;
                    let dz = center.z - b.height;
                    (dz.abs() <= sum)
                        .then(|| hypotf(center.x, center.y) + (sum * sum - dz * dz).sqrt())
                }
                Shape::Cylinder {
                    low,
                    radius,
                    height,
                } => {
                    let half = height * 0.5;
                    let up = b.height - low.z;
                    ((half - up).abs() <= (b.radius - TOUCH_EPSILON) + half)
                        .then(|| hypotf(low.x, low.y) + (radius - TOUCH_EPSILON) + b.radius)
                }
            };
            if let Some(a) = across {
                furthest = furthest.max(a);
            }
        }
    }
    furthest
}

/// Whether a body of `body` standing `across` metres from a portal of `setup` at `scale`, on its
/// ground and in the direction `heading` (radians from the portal's +x), touches it.
#[must_use]
pub fn touches(
    setup: &SetupGeometry,
    scale: f32,
    body: &[BodySphere],
    across: f32,
    heading: f32,
) -> bool {
    let at = Vec3::new(across * cosf(heading), across * sinf(heading), 0.0);
    shapes(setup, scale).iter().any(|shape| {
        body.iter().any(|b| {
            let c = Vec3::new(at.x, at.y, b.height);
            match *shape {
                Shape::Sphere { center, radius } => {
                    let d = c.sub(center);
                    let sum = radius + b.radius - TOUCH_EPSILON;
                    d.dot(d) <= sum * sum
                }
                Shape::Cylinder {
                    low,
                    radius,
                    height,
                } => {
                    let d = c.sub(low);
                    let sum = (radius - TOUCH_EPSILON) + b.radius;
                    let half = height * 0.5;
                    d.x * d.x + d.y * d.y <= sum * sum
                        && (half - d.z).abs() <= (b.radius - TOUCH_EPSILON) + half
                }
            }
        })
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the reach's arithmetic over hand-made shapes; the retail setups are
    //! measured against the physics in the dat tier).
    use super::*;
    use dereth_primitives::shape::{CylSphere, Sphere};

    /// One sphere of 1.357 m centred 1.42 m up: the common portal's body.
    fn sphere_portal() -> SetupGeometry {
        SetupGeometry {
            spheres: vec![Sphere {
                center: Vec3::new(0.0, 0.0, 1.42),
                radius: 1.357,
            }],
            ..SetupGeometry::default()
        }
    }

    #[test]
    fn a_sphere_portals_reach_is_its_upper_body_spheres_touch() {
        let g = sphere_portal();
        // The upper body sphere decides: 0.07 m below the portal sphere's centre.
        let sum: f32 = 1.357 + 0.48 - TOUCH_EPSILON;
        let expected = (sum * sum - 0.07 * 0.07).sqrt();
        let r = reach(&g, 1.0, &HUMAN_BODY);
        assert!((r - expected).abs() < 1e-4, "{r} against {expected}");
        for k in 0..8 {
            let heading = f32::from(u8::try_from(k).unwrap()) * 0.785;
            assert!(
                touches(&g, 1.0, &HUMAN_BODY, r - 0.01, heading),
                "just inside"
            );
            assert!(
                !touches(&g, 1.0, &HUMAN_BODY, r + 0.01, heading),
                "just outside"
            );
        }
        // Scaled, the shape grows and rises with it; the body does not.
        let big = reach(&g, 1.5, &HUMAN_BODY);
        let sum = 1.357 * 1.5 + 0.48 - TOUCH_EPSILON;
        let dz: f32 = 1.42 * 1.5 - 1.35;
        assert!((big - (sum * sum - dz * dz).sqrt()).abs() < 1e-4, "{big}");
        assert!(touches(&g, 1.5, &HUMAN_BODY, big - 0.01, 0.0));
        assert!(!touches(&g, 1.5, &HUMAN_BODY, big + 0.01, 0.0));
    }

    #[test]
    fn a_cylinder_portals_reach_is_its_radius_and_the_bodys() {
        let g = SetupGeometry {
            cyl_spheres: vec![CylSphere {
                low_pt: Vec3::new(0.0, 0.0, 0.0),
                radius: 0.587,
                height: 3.124,
            }],
            // A cylinder is the shape a body touches; the sphere beside it is not looked at.
            spheres: vec![Sphere {
                center: Vec3::new(0.0, 0.0, 1.0),
                radius: 5.0,
            }],
            ..SetupGeometry::default()
        };
        let r = reach(&g, 1.0, &HUMAN_BODY);
        assert!((r - (0.587 - TOUCH_EPSILON + 0.48)).abs() < 1e-5, "{r}");
        assert!(touches(&g, 1.0, &HUMAN_BODY, r - 0.01, 2.0));
        assert!(!touches(&g, 1.0, &HUMAN_BODY, r + 0.01, 2.0));
    }

    #[test]
    fn a_portal_with_no_shape_is_never_touched_and_one_overhead_out_of_reach() {
        let none = SetupGeometry::default();
        assert_eq!(reach(&none, 1.0, &HUMAN_BODY), 0.0);
        assert!(!touches(&none, 1.0, &HUMAN_BODY, 0.0, 0.0));
        let overhead = SetupGeometry {
            spheres: vec![Sphere {
                center: Vec3::new(0.0, 0.0, 6.0),
                radius: 1.0,
            }],
            ..SetupGeometry::default()
        };
        assert_eq!(reach(&overhead, 1.0, &HUMAN_BODY), 0.0);
        assert!(!touches(&overhead, 1.0, &HUMAN_BODY, 0.0, 0.0));
    }
}
