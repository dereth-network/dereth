//! The distance weight and direction pick, plus the six constants they read.
//!
//! Every constant below is the retail client's own value, not copied from the document.

use dereth_primitives::Vec3;

/// The distance at which an ambient sound is at full weight.
pub const AMBIENT_SOUND_MIN_DIST: f32 = 20.0;
/// The same distance, squared, as the client stores it.
pub const AMBIENT_SOUND_MIN_DIST_SQ: f32 = 400.0;
/// The distance beyond which an ambient sound contributes nothing.
pub const AMBIENT_SOUND_MAX_DIST: f32 = 120.0;
/// The same distance, squared, as the client stores it.
pub const AMBIENT_SOUND_MAX_DIST_SQ: f32 = 14400.0;
/// The audibility floor a constant sound must reach. The stored float is `0.029999999329447746`, which
/// is what `0.03` rounds to in `f32`, so the literal is exact.
pub const AMBIENT_SOUND_MIN_VOL: f32 = 0.03;
/// The half-width of a direction sector -- pi/8, 22.5 degrees. The stored float is `0.39269909262657166`,
/// which is what this literal rounds to. It is a data value, not a derivation: `std::f32::consts`
/// happens to agree today and substituting it would hide a divergence if it ever did not.
#[allow(clippy::approx_constant)]
pub const DIR_ANGLE_IN_RAD: f32 = 0.392_699_1;
/// The client's `0.0002` (exactly `0.00019999999494757503` as a float), the near-zero guard
/// in [`calc_dir`].
pub const DIR_EPSILON: f32 = 0.000_2;
/// The client's `2.0`, the aspect-ratio test in the direction pick.
pub const DIR_RATIO: f32 = 2.0;

/// Numeric direction values used by the ambient scheduler.
///
/// The numbering matters: direction calculation returns these values and intermittent sounds key their
/// eight-slot set on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Direction {
    InViewerBlock = 0,
    NorthOfViewer = 1,
    SouthOfViewer = 2,
    EastOfViewer = 3,
    WestOfViewer = 4,
    NorthwestOfViewer = 5,
    SouthwestOfViewer = 6,
    NortheastOfViewer = 7,
    SoutheastOfViewer = 8,
    Unknown = 9,
}

/// The eight compass directions seeded for an in-viewer-block cell, **in the
/// order it calls `add_dir`** — that order decides the contents of `sound_dir[]`, and therefore which
/// direction a uniform index picks.
pub const IN_BLOCK_DIRECTIONS: [Direction; 8] = [
    Direction::NorthOfViewer,
    Direction::SouthOfViewer,
    Direction::EastOfViewer,
    Direction::WestOfViewer,
    Direction::NorthwestOfViewer,
    Direction::NortheastOfViewer,
    Direction::SouthwestOfViewer,
    Direction::SoutheastOfViewer,
];

/// The distance weight of one cell.
///
/// ```text
/// d2 = |off|^2                 (3-D, including z)
/// if d2 > 14400 return 0       // max_dist_sq
/// if d2 <  400  return 1       // min_dist_sq
/// return 400 / d2
/// ```
///
/// Full weight inside 20 m, inverse-square from 20 to 120 m (1 down to 0.0278), nothing beyond.
#[must_use]
pub fn calc_weight(off: Vec3) -> f32 {
    let d2 = off.x * off.x + off.y * off.y + off.z * off.z;
    if d2 > AMBIENT_SOUND_MAX_DIST_SQ {
        return 0.0;
    }
    if d2 < AMBIENT_SOUND_MIN_DIST_SQ {
        return 1.0;
    }
    AMBIENT_SOUND_MIN_DIST_SQ / d2
}

/// The direction one cell falls in, relative to the viewer.
///
/// **Not an even 45-degree split.** The `ratio > 2` tests make the four cardinal sectors 53.13 degrees
/// wide and the four diagonal sectors 36.87 degrees wide, and anything closer than
/// `sqrt(400 * 0.5)` = 14.14 m in the XY plane is `InViewerBlock` regardless of direction.
///
#[must_use]
pub fn calc_dir(off: Vec3) -> Direction {
    // `min_dist_sq * 0.5` -- 200.0, and the test is on the XY plane only.
    if off.x * off.x + off.y * off.y < AMBIENT_SOUND_MIN_DIST_SQ * 0.5 {
        return Direction::InViewerBlock;
    }
    let ax = off.x.abs();
    let ay = off.y.abs();
    if ax < DIR_EPSILON || ay / ax > DIR_RATIO {
        return if off.y >= 0.0 {
            Direction::NorthOfViewer
        } else {
            Direction::SouthOfViewer
        };
    }
    if ay < DIR_EPSILON || ax / ay > DIR_RATIO {
        return if off.x >= 0.0 {
            Direction::EastOfViewer
        } else {
            Direction::WestOfViewer
        };
    }
    if off.x < 0.0 {
        if off.y >= 0.0 {
            Direction::NorthwestOfViewer
        } else {
            Direction::SouthwestOfViewer
        }
    } else if off.y >= 0.0 {
        Direction::NortheastOfViewer
    } else {
        Direction::SoutheastOfViewer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered description, cross-read against the client's own weight arithmetic.
    #[test]
    fn the_weight_is_flat_to_20_metres_then_inverse_square_to_120() {
        assert_eq!(calc_weight(Vec3::new(0.0, 0.0, 0.0)), 1.0);
        assert_eq!(calc_weight(Vec3::new(19.99, 0.0, 0.0)), 1.0);
        // At exactly 20 m, d2 == 400 is not < 400, so the divide branch runs and gives 1.0 anyway.
        assert_eq!(calc_weight(Vec3::new(20.0, 0.0, 0.0)), 1.0);
        assert!((calc_weight(Vec3::new(40.0, 0.0, 0.0)) - 0.25).abs() < 1e-6);
        let far = calc_weight(Vec3::new(120.0, 0.0, 0.0));
        assert!(
            (far - 400.0 / 14400.0).abs() < 1e-6,
            "1/36 at the outer radius"
        );
        assert_eq!(
            calc_weight(Vec3::new(120.1, 0.0, 0.0)),
            0.0,
            "nothing beyond 120 m"
        );
    }

    /// The z component counts: the distance is 3-D. Oracle: the three squared components summed in
    /// the client's own weight arithmetic.
    #[test]
    fn the_weight_uses_the_full_three_dimensional_distance() {
        assert_eq!(calc_weight(Vec3::new(0.0, 0.0, 121.0)), 0.0);
        assert!((calc_weight(Vec3::new(30.0, 40.0, 0.0)) - 400.0 / 2500.0).abs() < 1e-6);
    }

    /// The 53.13/36.87 split. `ay/ax > 2` is `atan(2) = 63.43` degrees from the x axis, so the
    /// north sector spans 90 +/- 26.57 = 53.13 degrees wide. Oracle: section 4.5's note and
    /// contract 10.5.
    #[test]
    fn the_direction_sectors_are_53_and_37_degrees_not_45() {
        let at = |deg: f32| {
            let r = deg.to_radians();
            // 30 m out, so well past the in-viewer-block radius.
            calc_dir(Vec3::new(
                30.0 * dereth_primitives::num::math::cosf(r),
                30.0 * dereth_primitives::num::math::sinf(r),
                0.0,
            ))
        };
        // Due north +/- 26.5 degrees is still NORTH.
        assert_eq!(at(90.0), Direction::NorthOfViewer);
        assert_eq!(at(90.0 - 26.0), Direction::NorthOfViewer);
        assert_eq!(at(90.0 + 26.0), Direction::NorthOfViewer);
        // Past 26.57 degrees it becomes a diagonal.
        assert_eq!(at(90.0 - 27.0), Direction::NortheastOfViewer);
        assert_eq!(at(45.0), Direction::NortheastOfViewer);
        assert_eq!(at(27.0), Direction::NortheastOfViewer);
        // And past 63.43 degrees from north it is EAST.
        assert_eq!(at(26.0), Direction::EastOfViewer);
        assert_eq!(at(0.0), Direction::EastOfViewer);
        assert_eq!(at(180.0), Direction::WestOfViewer);
        assert_eq!(at(-90.0), Direction::SouthOfViewer);
        assert_eq!(at(-135.0), Direction::SouthwestOfViewer);
        assert_eq!(at(-45.0), Direction::SoutheastOfViewer);
        assert_eq!(at(135.0), Direction::NorthwestOfViewer);
    }

    /// The in-viewer-block radius is `sqrt(200)` = 14.142 m in the XY plane, and z does not count
    /// towards it. Oracle: the client halves the squared minimum distance at that point.
    #[test]
    fn anything_within_14_metres_horizontally_is_in_the_viewer_block() {
        assert_eq!(
            calc_dir(Vec3::new(14.14, 0.0, 0.0)),
            Direction::InViewerBlock
        );
        assert_eq!(
            calc_dir(Vec3::new(14.15, 0.0, 0.0)),
            Direction::EastOfViewer
        );
        // z is excluded from this test even though it counts for the weight.
        assert_eq!(
            calc_dir(Vec3::new(0.0, 0.0, 100.0)),
            Direction::InViewerBlock
        );
    }

    /// The order of the eight `add_dir` calls for an in-block cell is the order the direction set ends
    /// up in, and a uniform index over that set is what picks a direction. Oracle:
    /// Oracle: the client's per-cell accumulation for an intermittent sound.
    #[test]
    fn the_in_block_direction_order_is_the_order_add_to_calls_add_dir() {
        assert_eq!(
            IN_BLOCK_DIRECTIONS,
            [
                Direction::NorthOfViewer,
                Direction::SouthOfViewer,
                Direction::EastOfViewer,
                Direction::WestOfViewer,
                Direction::NorthwestOfViewer,
                Direction::NortheastOfViewer,
                Direction::SouthwestOfViewer,
                Direction::SoutheastOfViewer,
            ]
        );
    }
}
