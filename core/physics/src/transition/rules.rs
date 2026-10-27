//! The settings the swept-sphere core takes: the values and switches by which one world's
//! transition differs from another's.
//!
//! The core's algorithm is one algorithm. What a world may choose is where a surface stops being
//! walkable, how long a sub-step may be, how long its landblocks are, how far apart placement tries
//! its scatter points, and a handful of small behaviors. [`TransitionRules::default`] is this
//! client's own choice of every one of them, so a transition that never sets its rules behaves
//! exactly as it always has.

use dereth_primitives::{CellId, Position, Vec3};

use crate::globals::{BLOCK_LENGTH, FLOOR_Z};

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
}
