//! A body's size and the places on it: its radius and height (its setup's, scaled by its look's
//! height scale and its own), its collision spheres, its distance to another object's cylinder,
//! and a place on it raised part of its height and offset in its own frame, kept from going
//! through walls.

use dereth_primitives::shape::Sphere;
use dereth_primitives::{CellId, Position, Vec3};

use super::Body;
use crate::ethereal::SightLine;
use crate::geom::{self, Ray, EPS};

/// What a line of sight to a place on a body passes only: the cells' own walls and floors.
pub const ENVIRONMENT_ONLY: u32 = 2;

/// How far a line of sight is made to reach before it is set.
const FAR: f32 = 100_000.0;

/// The squared distance between two upright cylinders standing at `p1` and `p2` (radius and
/// height each): the gap across the ground and the gap between their spans of height, each at
/// least zero.
// D2-13188
#[must_use]
pub fn cylinder_distance_squared(
    r1: f32,
    h1: f32,
    p1: &Position,
    r2: f32,
    h2: f32,
    p2: &Position,
) -> f32 {
    let o = geom::offset(p1, p2);
    let mut across = (o.x * o.x + o.y * o.y).sqrt() - (r1 + r2);
    if across < 0.0 {
        across = 0.0;
    }
    let (z1, mut z2) = (p1.frame.origin.z, p2.frame.origin.z);
    let top = if z1 <= z2 {
        z1 + h1
    } else {
        let t = z2 + h2;
        z2 = z1;
        t
    };
    let mut up = z2 - top;
    if up < 0.0 {
        up = 0.0;
    }
    across * across + up * up
}

/// A place on an object standing at `at` with height `height` and scale `scale`: `rise` of its
/// height up and `offset` (by its scale when `scaled`) from its origin, turned with it. Straight
/// up from an outdoor place (nothing across, a rise from 0 to 1) it is just raised; else a line of
/// sight through the cells' walls (`los`) carries it from the object's origin as far as it gets.
/// The flag says whether nothing was in the way.
// D2-06130
#[must_use]
pub fn offset_position(
    at: &Position,
    height: f32,
    scale: Vec3,
    rise: f32,
    scaled: bool,
    offset: Vec3,
    los: &mut dyn FnMut(&mut SightLine),
) -> (Position, bool) {
    let mut p = *at;
    let mut o = offset;
    if scaled {
        o = Vec3::new(o.x * scale.x, o.y * scale.y, o.z * scale.z);
    }
    let m = geom::rotation_axes(p.frame.rotation);
    let up = f64::from(height) * f64::from(rise) + f64::from(o.z);
    let (x, y) = (f64::from(o.x), f64::from(o.y));
    #[allow(clippy::cast_possible_truncation)]
    let g = Vec3::new(
        (up * f64::from(m[6]) + x * f64::from(m[0]) + y * f64::from(m[3])) as f32,
        (up * f64::from(m[7]) + x * f64::from(m[1]) + y * f64::from(m[4])) as f32,
        (up * f64::from(m[8]) + x * f64::from(m[2]) + y * f64::from(m[5])) as f32,
    );
    if g.x.abs() < EPS && g.y.abs() < EPS && g.z.abs() < EPS {
        return (p, true);
    }
    let straight_up = offset.x.abs() < EPS
        && offset.y.abs() < EPS
        && offset.z.abs() < EPS
        && (0.0..=1.0).contains(&rise)
        && g.x.abs() < EPS
        && g.y.abs() < EPS;
    if straight_up && (p.cell.0 & 0xFFFF) < 0x100 {
        p.frame.origin.z += g.z;
        return (p, true);
    }
    let mut ray = Ray {
        length: FAR,
        ..Ray::default()
    };
    ray.init_from_offset(p.frame.origin, g);
    let mut line = SightLine::default();
    line.set_ray(ray, p.cell.0);
    line.set_intersect_type(ENVIRONMENT_ONLY);
    los(&mut line);
    let clear = line.trace_result == 0;
    p.frame.origin = line.end_point();
    p.cell = CellId(line.final_cell);
    (p, clear)
}

impl Body {
    /// Its setup's radius (its part group's, or the one it was given without one), scaled by its
    /// look's height scale and its own.
    // D2-05868
    #[must_use]
    pub fn radius(&self) -> f32 {
        let r = self
            .parts
            .as_deref()
            .map_or(self.setup_radius, |g| g.array.radius());
        r * self.look_scale.z * self.object_scale.z
    }

    /// Its setup's height (its part group's, or the one it was given without one), scaled by its
    /// look's height scale and its own.
    // D2-05869
    #[must_use]
    pub fn height(&self) -> f32 {
        let h = self
            .parts
            .as_deref()
            .map_or(self.setup_height, |g| g.array.height());
        h * self.look_scale.z * self.object_scale.z
    }

    /// How many collision spheres it has.
    // D2-05864, D2-06453
    #[must_use]
    pub fn num_sphere(&self) -> usize {
        self.config.spheres.len()
    }

    /// Its collision spheres, unscaled.
    // D2-05865, D2-06454
    #[must_use]
    pub fn spheres(&self) -> &[Sphere] {
        &self.config.spheres
    }

    /// The spheres a move or a placement carries the body by, and their scale: its own, scaled up
    /// by its whole scale (its look's times its own) along z; or, with none, one sphere of radius
    /// 0.1 just above its origin, unscaled.
    #[must_use]
    pub fn moving_spheres(&self) -> (&[Sphere], f32) {
        use dereth_physics::globals::{DUMMY_SPHERE_CENTER_Z, DUMMY_SPHERE_RADIUS};
        const NONE: [Sphere; 1] = [Sphere::new(
            Vec3::new(0.0, 0.0, DUMMY_SPHERE_CENTER_Z),
            DUMMY_SPHERE_RADIUS,
        )];
        if self.config.spheres.is_empty() {
            (&NONE, 1.0)
        } else {
            (&self.config.spheres, self.total_scale(None).z)
        }
    }

    /// The place on the ground under the body: its own place lowered by its height above what is
    /// walkable; out of the world, its place.
    #[must_use]
    pub fn ground_position(&self) -> Position {
        let mut p = self.position;
        if self.in_world {
            p.frame.origin.z -= self.height_above_walkable();
        }
        p
    }

    /// The squared distance from its cylinder to another (radius and height) standing at `p`.
    #[must_use]
    pub fn distance_to_object_squared(&self, p: &Position, radius: f32, height: f32) -> f32 {
        cylinder_distance_squared(
            self.radius(),
            self.height(),
            &self.position,
            radius,
            height,
            p,
        )
    }

    /// A place on the body: `rise` of its height up and `offset` (by its scale when `scaled`)
    /// from its origin, turned with it, kept from going through walls by `los` (see
    /// [`offset_position`]); with whether nothing was in the way.
    pub fn object_offset_position(
        &self,
        rise: f32,
        scaled: bool,
        offset: Vec3,
        los: &mut dyn FnMut(&mut SightLine),
    ) -> (Position, bool) {
        let s = self.config.scale;
        offset_position(
            &self.position,
            self.height(),
            Vec3::new(s, s, s),
            rise,
            scaled,
            offset,
            los,
        )
    }
}
