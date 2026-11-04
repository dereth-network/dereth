//! The settings the swept-sphere core takes: the values and switches by which one world's
//! transition differs from another's.
//!
//! The core's algorithm is one algorithm. What a world may choose is where a surface stops being
//! walkable, how long a sub-step may be, how long its landblocks are, how far apart placement tries
//! its scatter points, and a handful of small behaviors. [`TransitionRules::default`] is this
//! client's own choice of every one of them, so a transition that never sets its rules behaves
//! exactly as it always has.

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::geom::sphere::Sphere;
use crate::globals::{BLOCK_LENGTH, EPSILON, FLOOR_Z};

use super::{Transition, TransitionState};

/// How a world tunes the transition.
///
/// Set on a [`super::Transition`] with [`super::Transition::set_rules`] after it is initialized for
/// a move; [`super::Transition::init`] keeps them, as a pooled transition belongs to one world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionRules {
    /// A surface is walkable when its normal's vertical part reaches this, with no tolerance.
    pub walkable_z: f32,
    /// The longest sub-step of a move, as a fraction of the mover's first sphere's radius. A move
    /// is split into equal sub-steps no longer than this.
    pub step_fraction: f32,
    /// The side of a landblock, in meters: the offset between two cells' frames is this times the
    /// difference of their landblock coordinates.
    pub block_length: f32,
    /// The smallest radius placement scatters its candidate points by.
    pub placement_radius_floor: f32,
    /// A move that comes to rest against something while the last ground it stood on is still
    /// under it stops dead: the restored contact also kills the mover's velocity. When `false`
    /// the ground is restored and the velocity kept.
    pub stop_on_restored_contact: bool,
    /// A cell is asked for its collisions up to the attempt budget, again after each adjustment,
    /// before the other cells are; when `false` it is asked once and the transition's own retries
    /// take it from there.
    pub retry_each_cell: bool,
    /// A landing found walkable is re-checked as a placement with the attempts of the insert that
    /// found it; when `false`, with a single attempt.
    pub recheck_landing_with_budget: bool,
    /// When an edge slide probes down from the current position, the way back from the checked
    /// position is measured between the two positions, across landblocks; when `false` it is the
    /// difference of the two sphere centers, each in its own landblock's frame.
    pub edge_slide_offset_across_blocks: bool,
    /// A step down onto ground whose drop so far (the step-down amount times the interpolation
    /// reached) is within 0.0002 of zero lowers the sphere without narrowing the interpolation;
    /// when `false` the drop divides as it is, and a zero drop refuses the step.
    pub step_down_guards_zero_drop: bool,
    /// A placement's insert asks the cell it is in and then the other cells, up to three times,
    /// starting again after any adjustment (each time an attempt of its own); when `false` it asks
    /// the cell up to three times and the others once, and an adjustment the others make is its
    /// answer.
    pub placement_retries_other_cells: bool,
    /// How a move turns its sub-steps from the rotation it begins at to the one it ends at.
    pub turning: Turning,
    /// The placement scatter's rings are counted by this fraction of the radius it scatters by:
    /// `4 / (fraction x radius)` of them, rounded up, over its 4 m.
    pub placement_ring_fraction: f32,
    /// A placement that may slide, refused where it was asked, is tried once more 0.1 m above it
    /// before the scatter.
    pub placement_lifts_before_scatter: bool,
    /// How near to lying in a polygon's plane a movement or the up vector may come (their dot
    /// product with its normal, either way) before the polygon tests take it as lying in it: a
    /// movement that near is not adjusted onto the polygon when stepping or brought against it
    /// when meeting it, and an up vector that near finds no footprint and no crossed edge. At 0
    /// only an exactly parallel movement is refused, and the up vector is not tested at all.
    pub parallel_band: f32,
    /// A mover not on the ground that an interior cell's tree stops or moves, with no collision
    /// normal yet, collided with what is under it: its collision normal is straight up.
    pub env_collision_normal_up: bool,
    /// The world's own slide along what the mover meets ([`Slide`]), where it works the slide
    /// otherwise than this client; when `None`, the shared slide
    /// ([`super::collide::slide_sphere`]).
    pub slide: Option<Slide>,
}

/// A world's own slide: how the mover's sphere slides along a surface it meets, and how a physics
/// tree's polygon normal is turned into the world before the tree's step up and its slide take it.
///
/// It is what slides the mover wherever the transition slides it along a surface: along the face
/// it could not step up onto (a tree's or a back face's), along the second sphere's face, along
/// a back face, and along the edge of the ground it would walk off. The slides of the shared
/// sphere and cylinder tests keep their own.
#[derive(Clone, Copy)]
pub struct Slide {
    /// The mover's sphere `sphere` slides along the surface whose normal is `normal`, from its
    /// center at the start of the step `curr`. The slide may turn `normal`; the transition keeps
    /// the turned normal where it keeps the one it handed over (the step-up normal, a back face's
    /// normal).
    pub along: fn(&mut Transition, &Sphere, &mut Vec3, Vec3) -> TransitionState,
    /// A physics tree's polygon normal turned from the tree's frame into the landblock's, for the
    /// tree's step up and its second sphere's slide.
    pub turn: fn(&Frame, Vec3) -> Vec3,
}

impl std::fmt::Debug for Slide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Slide { .. }")
    }
}

impl PartialEq for Slide {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::fn_addr_eq(self.along, other.along) && std::ptr::fn_addr_eq(self.turn, other.turn)
    }
}

/// How a move's sub-steps turn from where it begins to where it ends, and how a rotation is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Turning {
    /// The arc's weights in double precision, a straight blend within `0.0002` of no turn or when
    /// a weight leaves 0 to 1 ([`crate::math::interpolate_rotation`]); a rotation that comes out
    /// invalid leaves the frame's as it was ([`crate::math::set_rotate`]).
    #[default]
    Guarded,
    /// The arc's weights in single precision, a straight blend within `0.001` of no turn
    /// ([`crate::math::slerp_rotation_plain`]); a rotation is made unit length only when it has
    /// some length ([`crate::math::set_rotate_plain`]).
    Plain,
}

impl Turning {
    /// Set `f`'s rotation to `q`.
    pub fn set_rotate(self, f: &mut Frame, q: Quat) {
        match self {
            Self::Guarded => crate::math::set_rotate(f, q.w, q.x, q.y, q.z),
            Self::Plain => crate::math::set_rotate_plain(f, q),
        }
    }

    /// Set `out`'s rotation `t` of the way from `a`'s to `b`'s.
    pub fn blend(self, out: &mut Frame, a: &Frame, b: &Frame, t: f32) {
        match self {
            Self::Guarded => crate::math::interpolate_rotation(out, a, b, t),
            Self::Plain => crate::math::slerp_rotation_plain(out, a, b, t),
        }
    }
}

impl Default for TransitionRules {
    fn default() -> Self {
        Self {
            walkable_z: FLOOR_Z,
            step_fraction: 1.0,
            block_length: BLOCK_LENGTH,
            placement_radius_floor: 0.48,
            stop_on_restored_contact: true,
            retry_each_cell: true,
            recheck_landing_with_budget: true,
            edge_slide_offset_across_blocks: false,
            step_down_guards_zero_drop: false,
            placement_retries_other_cells: false,
            turning: Turning::Guarded,
            placement_ring_fraction: 1.0,
            placement_lifts_before_scatter: false,
            parallel_band: EPSILON,
            env_collision_normal_up: false,
            slide: None,
        }
    }
}

impl TransitionRules {
    /// Whether a surface with normal `n` is walkable: exactly `n.z >= walkable_z`.
    #[inline]
    #[must_use]
    pub fn is_valid_walkable(&self, n: Vec3) -> bool {
        n.z >= self.walkable_z
    }

    /// The offset from `from`'s landblock frame to `to`'s.
    #[inline]
    #[must_use]
    pub fn block_offset(&self, from: CellId, to: CellId) -> Vec3 {
        block_offset(from, to, self.block_length)
    }

    /// The offset between two positions, crossing landblocks.
    #[inline]
    #[must_use]
    pub fn get_offset(&self, from: &Position, to: &Position) -> Vec3 {
        use crate::math::V3;
        self.block_offset(from.cell, to.cell)
            .add(to.frame.origin)
            .sub(from.frame.origin)
    }
}

/// The offset from `from`'s landblock frame to `to`'s for landblocks `block_length` meters on a
/// side. The same arithmetic, in the same order, as the fixed-length offset.
#[inline]
#[must_use]
pub fn block_offset(from: CellId, to: CellId, block_length: f32) -> Vec3 {
    let (a, b) = (from.landblock(), to.landblock());
    Vec3::new(
        (f32::from(b.x()) - f32::from(a.x())) * block_length,
        (f32::from(b.y()) - f32::from(a.y())) * block_length,
        0.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::{Frame, Quat};

    /// The rules a transition starts with are this client's constants and choices.
    ///
    /// Behaviour: physics.transition.the-default-rules-are-this-clients
    #[test]
    fn the_default_rules_are_this_clients_constants() {
        let r = TransitionRules::default();
        assert_eq!(r.walkable_z.to_bits(), FLOOR_Z.to_bits());
        assert_eq!(r.step_fraction, 1.0);
        assert_eq!(r.block_length, 192.0);
        assert_eq!(r.placement_radius_floor, 0.48);
        assert!(r.stop_on_restored_contact);
        assert!(r.retry_each_cell);
        assert!(r.recheck_landing_with_budget);
        assert!(!r.edge_slide_offset_across_blocks);
        assert!(!r.step_down_guards_zero_drop);
        assert!(!r.placement_retries_other_cells);
        assert_eq!(r.turning, Turning::Guarded);
        assert_eq!(r.parallel_band.to_bits(), EPSILON.to_bits());
        assert!(r.slide.is_none());
    }

    #[test]
    fn the_default_block_offset_is_bit_for_bit_the_fixed_one() {
        let ids = [
            0x0000_0001_u32,
            0xA9B4_0001,
            0xA9B5_0012,
            0xAAB4_0040,
            0x01FF_0100,
            0xFFFF_0001,
        ];
        for &a in &ids {
            for &b in &ids {
                let (a, b) = (CellId(a), CellId(b));
                let ours = TransitionRules::default().block_offset(a, b);
                let fixed = crate::landdefs::get_block_offset(a, b);
                assert_eq!(
                    (ours.x.to_bits(), ours.y.to_bits(), ours.z.to_bits()),
                    (fixed.x.to_bits(), fixed.y.to_bits(), fixed.z.to_bits()),
                    "{a:?} -> {b:?}"
                );
            }
        }
    }

    #[test]
    fn the_default_position_offset_is_bit_for_bit_the_fixed_one() {
        let p = |c: u32, x: f32, y: f32, z: f32| {
            Position::new(CellId(c), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
        };
        let a = p(0xA9B4_0001, 191.7, 3.25, 40.1);
        let b = p(0xAAB3_0039, 0.3, 189.9, 41.7);
        let ours = TransitionRules::default().get_offset(&a, &b);
        let fixed = crate::math::get_offset(&a, &b);
        assert_eq!(
            (ours.x.to_bits(), ours.y.to_bits(), ours.z.to_bits()),
            (fixed.x.to_bits(), fixed.y.to_bits(), fixed.z.to_bits())
        );
    }

    #[test]
    fn a_shorter_block_shortens_the_offset_between_neighbouring_blocks() {
        let r = TransitionRules {
            block_length: 160.0,
            ..TransitionRules::default()
        };
        let o = r.block_offset(CellId(0x1020_0001), CellId(0x1121_0001));
        assert_eq!(o, Vec3::new(160.0, 160.0, 0.0));
    }

    #[test]
    fn walkability_is_a_bare_comparison_against_the_rules_limit() {
        let steeper = TransitionRules {
            walkable_z: 0.700_91,
            ..TransitionRules::default()
        };
        let n = Vec3::new(0.0, 0.719, 0.695);
        assert!(TransitionRules::default().is_valid_walkable(n));
        assert!(!steeper.is_valid_walkable(n));
        assert!(steeper.is_valid_walkable(Vec3::new(0.0, 0.0, 0.700_91)));
    }

    #[test]
    fn a_plain_turn_blends_straight_through_small_turns_and_keeps_a_rotation_of_no_length() {
        let a = Frame::new(Vec3::ZERO, Quat::IDENTITY);
        // A turn of 0.06 rad about z: 1 - cos(0.03) is between the two thresholds.
        let (c, s) = (
            dereth_primitives::num::math::cosf(0.03),
            dereth_primitives::num::math::sinf(0.03),
        );
        let b = Frame::new(Vec3::ZERO, Quat::new(c, 0.0, 0.0, s));
        let (mut guarded, mut plain) = (a, a);
        Turning::Guarded.blend(&mut guarded, &a, &b, 0.25);
        Turning::Plain.blend(&mut plain, &a, &b, 0.25);
        // Straight: three quarters of the one and a quarter of the other, made unit length.
        let (w, z) = (0.75 + 0.25 * c, 0.25 * s);
        let (w, z) = (f64::from(w), f64::from(z));
        let k = 1.0 / (z * z + w * w).sqrt();
        #[allow(clippy::cast_possible_truncation)]
        let unit = ((k * w) as f32, (k * z) as f32);
        assert_eq!((plain.rotation.w, plain.rotation.z), unit);
        assert!((guarded.rotation.z - plain.rotation.z).abs() > 1e-8);

        let zero = Quat::new(0.0, 0.0, 0.0, 0.0);
        let (mut g, mut p) = (b, b);
        Turning::Guarded.set_rotate(&mut g, zero);
        Turning::Plain.set_rotate(&mut p, zero);
        assert_eq!(g.rotation, b.rotation, "an invalid rotation is refused");
        assert_eq!(p.rotation, zero, "kept as it is");
    }
}
