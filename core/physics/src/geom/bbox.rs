//! Axis-aligned bounding boxes represented by minimum and maximum corners.
//!
//! Collision detection has a **bounding box arm** selected by `HAS_PHYSICS_BSP_PS` (`0x10000`),
//! and every mesh object in the game takes it. The two consumers are plane-box intersection and
//! [`crate::geom::BspTree::box_intersects_cell_bsp`].

use dereth_primitives::{Position, Vec3};

use crate::math;

/// An axis-aligned box in whatever frame its corners were computed in. The data, its constructor
/// and the hull over a point set are `dereth_primitives`'; the frame changes are [`BBoxExt`].
pub use dereth_primitives::shape::BBox;

/// The box's frame changes and the bounding-box fold. Bring the trait into scope to call them.
pub trait BBoxExt {
    /// Transform all **eight** corners between local frames and refit an axis-aligned box in `to`.
    ///
    /// This is not a transform of `min`/`max`: a rotated box's axis-aligned hull needs every
    /// corner, and the client walks all eight for that reason.
    #[must_use]
    fn local_to_local(&self, from: &Position, to: &Position) -> Self;

    /// The global form -- the same eight-corner walk as [`Self::local_to_local`], through the
    /// position's local-to-global instead of its local-to-local.
    ///
    /// **The difference is one statement, and it matters.** The local-to-local form converts each
    /// final point from global space into the destination frame; this form returns immediately
    /// after adding the destination landblock offset. Thus `to` contributes **only its
    /// `objcell_id`**, while its frame -- origin and rotation -- is never read.
    /// The result is therefore in `to`'s *landblock* coordinates, not in `to`'s own frame.
    ///
    /// That is what makes the outdoor bounding-box expansion work: it hands the land cell's own
    /// `pos` in as `to` and then reduces each corner with `cell_of(v) - ox`, which is
    /// a **within-block cell index** minus the object's own within-block cell index. It needs
    /// landblock-relative metres, and it gets them from a `pos` whose origin is the cell *centre*
    /// -- because the origin is not read. See [`crate::cell::Cell::land_pos`].
    #[must_use]
    fn local_to_global(&self, from: &Position, to: &Position) -> Self;

    /// The same eight-corner walk through a bare
    /// [`Frame`](dereth_primitives::Frame) rather than a `Position` pair.
    ///
    /// Seed both minimum and maximum from the `(min.x, min.y, min.z)` corner,
    /// then expand the bounds to include the other seven transformed corners.
    /// Only the frame rotation and origin are read; no cell id is involved,
    /// which is why this takes a `Frame` and not a `Position`.
    ///
    /// Part-array bounding-box construction is its only caller in this rebuild, and it passes
    /// each part's own `pos`.
    #[must_use]
    fn convert_to_global(&self, frame: &dereth_primitives::Frame) -> Self;

    /// Grow `out` to contain `self`, component by component.
    ///
    /// Six independent compares in the client, which is exactly `min`-wise min and `max`-wise max.
    /// It does **not** initialise `out`: the part-array caller seeds the box with the origin on
    /// both corners before the walk, so the result always contains the origin. That seeding is
    /// the caller's, and this crate's caller keeps it.
    fn build_bounding_box(&self, out: &mut Self);
}

impl BBoxExt for BBox {
    fn local_to_local(&self, from: &Position, to: &Position) -> Self {
        let mut out: Option<Self> = None;
        for cx in [self.min.x, self.max.x] {
            for cy in [self.min.y, self.max.y] {
                for cz in [self.min.z, self.max.z] {
                    let p = math::localtolocal(to, from, Vec3::new(cx, cy, cz));
                    match &mut out {
                        Some(b) => b.adjust(p),
                        None => out = Some(Self { min: p, max: p }),
                    }
                }
            }
        }
        out.unwrap_or_default()
    }

    fn local_to_global(&self, from: &Position, to: &Position) -> Self {
        let mut out: Option<Self> = None;
        for cx in [self.min.x, self.max.x] {
            for cy in [self.min.y, self.max.y] {
                for cz in [self.min.z, self.max.z] {
                    let p = math::pos_localtoglobal(to, from, Vec3::new(cx, cy, cz));
                    match &mut out {
                        Some(b) => b.adjust(p),
                        None => out = Some(Self { min: p, max: p }),
                    }
                }
            }
        }
        out.unwrap_or_default()
    }

    fn convert_to_global(&self, frame: &dereth_primitives::Frame) -> Self {
        let mut out: Option<Self> = None;
        for cx in [self.min.x, self.max.x] {
            for cy in [self.min.y, self.max.y] {
                for cz in [self.min.z, self.max.z] {
                    let p = math::localtoglobal(frame, Vec3::new(cx, cy, cz));
                    match &mut out {
                        Some(b) => b.adjust(p),
                        None => out = Some(Self { min: p, max: p }),
                    }
                }
            }
        }
        out.unwrap_or_default()
    }

    fn build_bounding_box(&self, out: &mut Self) {
        out.adjust(self.min);
        out.adjust(self.max);
    }
}

#[cfg(test)]
mod tests {
    use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

    use super::{BBox, BBoxExt};

    /// A rotated box needs all eight corners.
    #[test]
    fn a_rotated_box_needs_all_eight_corners() {
        let cell = CellId(1);
        let ident = Position::new(cell, Frame::new(Vec3::ZERO, Quat::IDENTITY));
        let turned = Position::new(
            cell,
            // A 45-degree turn about z: w = cos(22.5 deg), z = sin(22.5 deg).
            Frame::new(
                Vec3::ZERO,
                Quat::new(
                    dereth_primitives::num::math::cosf(22.5_f32.to_radians()),
                    0.0,
                    0.0,
                    dereth_primitives::num::math::sinf(22.5_f32.to_radians()),
                ),
            ),
        );
        let b = BBox::new(Vec3::new(-0.5, -0.5, 0.0), Vec3::new(0.5, 0.5, 2.0));
        // The box is given in `turned`'s frame and asked for in the identity frame.
        let out = b.local_to_local(&turned, &ident);
        let half = 0.5_f32 * 2.0_f32.sqrt();
        assert!(
            (out.min.x - -half).abs() < 1e-4,
            "min.x = {}, want {}",
            out.min.x,
            -half
        );
        assert!(
            (out.max.x - half).abs() < 1e-4,
            "max.x = {}, want {}",
            out.max.x,
            half
        );
        assert!(
            (out.min.y - -half).abs() < 1e-4,
            "min.y = {}, want {}",
            out.min.y,
            -half
        );
        assert!(
            (out.max.y - half).abs() < 1e-4,
            "max.y = {}, want {}",
            out.max.y,
            half
        );
        // z is unaffected by a turn about z, which is the half that says the transform ran at all
        // rather than the box being replaced by something large.
        assert!((out.min.z - 0.0).abs() < 1e-4);
        assert!((out.max.z - 2.0).abs() < 1e-4);
    }

    /// The calibration in the other direction: an unrotated box is its own hull, so a wrong
    /// answer above cannot be "this function always widens".
    #[test]
    fn an_unrotated_box_is_unchanged_but_for_the_offset() {
        let cell = CellId(1);
        let at = Position::new(cell, Frame::new(Vec3::ZERO, Quat::IDENTITY));
        let from = Position::new(cell, Frame::new(Vec3::new(3.0, -4.0, 1.0), Quat::IDENTITY));
        let b = BBox::new(Vec3::new(-1.0, -2.0, 0.0), Vec3::new(1.0, 2.0, 5.0));
        let out = b.local_to_local(&from, &at);
        assert!((out.min.x - 2.0).abs() < 1e-4, "min.x = {}", out.min.x);
        assert!((out.max.x - 4.0).abs() < 1e-4, "max.x = {}", out.max.x);
        assert!((out.min.y - -6.0).abs() < 1e-4, "min.y = {}", out.min.y);
        assert!((out.max.y - -2.0).abs() < 1e-4, "max.y = {}", out.max.y);
        assert!((out.min.z - 1.0).abs() < 1e-4);
        assert!((out.max.z - 6.0).abs() < 1e-4);
    }

    /// Local to global ignores the destination frame where local to local uses it.
    #[test]
    fn local_to_global_ignores_the_destination_frame_where_local_to_local_uses_it() {
        let cell = CellId(0xA9B4_0001);
        let from = Position::new(cell, Frame::new(Vec3::new(30.0, 40.0, 5.0), Quat::IDENTITY));
        let b = BBox::new(Vec3::new(-1.0, -2.0, 0.0), Vec3::new(1.0, 2.0, 3.0));

        let identity = Position::new(cell, Frame::new(Vec3::ZERO, Quat::IDENTITY));
        // The land cell's real `pos`: cell (0, 0)'s centre, plus -- to make the point about the
        // rotation as well as the origin -- a quarter turn it does not actually carry.
        let turned = Position::new(
            cell,
            Frame::new(
                Vec3::new(12.0, 12.0, 0.0),
                Quat::new(
                    dereth_primitives::num::math::cosf(45.0_f32.to_radians()),
                    0.0,
                    0.0,
                    dereth_primitives::num::math::sinf(45.0_f32.to_radians()),
                ),
            ),
        );

        assert_eq!(
            b.local_to_global(&from, &turned),
            b.local_to_global(&from, &identity),
            "localtoglobal reads only the destination's cell id, so its frame cannot matter"
        );
        assert_ne!(
            b.local_to_local(&from, &turned),
            b.local_to_local(&from, &identity),
            "and localtolocal does read it, or this test would be asserting nothing"
        );
        // And the value itself, so "equal" above cannot be two copies of the same wrong answer:
        // landblock metres, the box sitting on the part's own origin.
        let g = b.local_to_global(&from, &turned);
        assert!((g.min.x - 29.0).abs() < 1e-4, "min.x = {}", g.min.x);
        assert!((g.max.y - 42.0).abs() < 1e-4, "max.y = {}", g.max.y);
        assert!((g.max.z - 8.0).abs() < 1e-4, "max.z = {}", g.max.z);
    }

    /// Local to global also needs all eight corners.
    #[test]
    fn local_to_global_also_needs_all_eight_corners() {
        let cell = CellId(1);
        let dest = Position::new(cell, Frame::new(Vec3::new(50.0, 60.0, 0.0), Quat::IDENTITY));
        let turned = Position::new(
            cell,
            Frame::new(
                Vec3::ZERO,
                Quat::new(
                    dereth_primitives::num::math::cosf(22.5_f32.to_radians()),
                    0.0,
                    0.0,
                    dereth_primitives::num::math::sinf(22.5_f32.to_radians()),
                ),
            ),
        );
        let b = BBox::new(Vec3::new(-0.5, -0.5, 0.0), Vec3::new(0.5, 0.5, 2.0));
        let out = b.local_to_global(&turned, &dest);
        let half = 0.5_f32 * 2.0_f32.sqrt();
        assert!(
            (out.min.x - -half).abs() < 1e-4,
            "min.x = {}, want {}",
            out.min.x,
            -half
        );
        assert!(
            (out.max.x - half).abs() < 1e-4,
            "max.x = {}, want {}",
            out.max.x,
            half
        );
        assert!(
            (out.min.y - -half).abs() < 1e-4,
            "min.y = {}, want {}",
            out.min.y,
            -half
        );
        assert!(
            (out.max.y - half).abs() < 1e-4,
            "max.y = {}, want {}",
            out.max.y,
            half
        );
        assert!((out.min.z - 0.0).abs() < 1e-4);
        assert!((out.max.z - 2.0).abs() < 1e-4);
        // The calibration in the other direction, so a wrong answer above cannot be "this always
        // widens": unrotated, the box is its own hull.
        let flat = Position::new(cell, Frame::new(Vec3::ZERO, Quat::IDENTITY));
        let out = b.local_to_global(&flat, &dest);
        assert!((out.min.x - -0.5).abs() < 1e-4, "min.x = {}", out.min.x);
        assert!((out.max.y - 0.5).abs() < 1e-4, "max.y = {}", out.max.y);
    }

    /// The block offset is the one part of the destination that **is** read -- its `objcell_id` --
    /// so a destination in the block to the west moves the answer by 192 m. Without this,
    /// `local_to_global` could be "ignore the destination entirely" and still pass above.
    #[test]
    fn local_to_global_still_crosses_landblocks_through_the_destination_cell_id() {
        let here = CellId(0xA9B4_0001);
        let west = CellId(0xA8B4_0001);
        let from = Position::new(here, Frame::new(Vec3::new(10.0, 20.0, 0.0), Quat::IDENTITY));
        let b = BBox::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        let same = b.local_to_global(&from, &Position::new(here, Frame::default()));
        let over = b.local_to_global(&from, &Position::new(west, Frame::default()));
        assert!((same.min.x - 10.0).abs() < 1e-4, "min.x = {}", same.min.x);
        assert!(
            (over.min.x - (10.0 + 192.0)).abs() < 1e-3,
            "seen from the block to the west the same box is a landblock further east: {}",
            over.min.x
        );
    }
}
