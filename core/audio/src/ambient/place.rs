//! Where an ambient sound is placed, and the compass heading a direction maps to.
//!
//! **This is the observable draw order.** Three `ran2` values come out of the same generator the
//! sound-table row pick uses, in exactly this sequence:
//!
//! 1. the **direction index**, `floor(roll_f32(0, (float)num_dir))`;
//! 2. the **angle** jitter, `roll_f32(0, DIR_ANGLE_IN_RAD)`;
//! 3. the **radius** parameter `u`, `roll_f32(0, 1)`.
//!
//! That is the order the retail client draws them in: the direction roll (floored, then truncated
//! to an integer), then the angle roll, then the radius roll. Getting the order wrong changes every
//! ambient sound's position for a given seed.

use dereth_primitives::num::math;
use dereth_primitives::num::rng::Ran2;
use dereth_primitives::num::{floor_to_i32, to_i32};
use dereth_primitives::Vec3;

use super::weight::{Direction, DIR_ANGLE_IN_RAD};

/// The generator's cap -- the largest value `ran2` can return.
pub const RNMX: f64 = 0.999_999_88;

/// The same cap after the random roll narrows its bounds to `f32`.
#[allow(clippy::cast_possible_truncation)] // LINT-OK: the roll narrows the f64 draw to f32 itself
pub const RNMX_F32: f32 = RNMX as f32;

/// The heading of a [`Direction`], in radians, compass convention (0 = north, clockwise).
///
/// These are the eight float constants the client's direction switch selects between, so they
/// are the retail values and not derived from pi. They happen to agree with `f32` roundings of
/// multiples of pi/4, which is why the `approx_constant` lint has to be silenced: substituting
/// `std::f32::consts` would be replacing a data table with a formula.
///
/// `InViewerBlock`, `Unknown` and anything unrecognised return `0.0`; the heading jump
/// table covers only `Direction` 1..8, and the `default:` arm loads 0.0, which is also what
/// `NorthOfViewer` returns.
#[must_use]
#[allow(clippy::approx_constant)] // the eight constants the client's heading table holds
pub fn heading(dir: Direction) -> f32 {
    match dir {
        Direction::NorthOfViewer => 0.0,
        Direction::NortheastOfViewer => 0.785_398_2,
        Direction::EastOfViewer => 1.570_796_4,
        Direction::SoutheastOfViewer => 2.356_194_5,
        Direction::SouthOfViewer => 3.141_592_7,
        Direction::SouthwestOfViewer => 3.926_990_7,
        Direction::WestOfViewer => 4.712_389,
        Direction::NorthwestOfViewer => 5.497_787,
        Direction::InViewerBlock | Direction::Unknown => 0.0,
    }
}

/// The direction set an [`IntermitState`](super::IntermitState) accumulates, and the radius band for
/// each entry. The client keeps at most eight, keyed by [`Direction`], and
/// widens the band of one already present.
///
/// There is **no bounds check on `num_dir`** in the original. It is safe only because there are nine
/// `Direction` values and `InViewerBlock` seeds the other eight, so at most eight distinct entries
/// can ever be added. The fixed-size arrays here make that structural rather than incidental.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirSet {
    pub sound_dir: [Direction; 8],
    pub min_dist: [f32; 8],
    pub max_dist: [f32; 8],
    pub num_dir: usize,
}

impl Default for DirSet {
    fn default() -> Self {
        Self {
            sound_dir: [Direction::Unknown; 8],
            min_dist: [0.0; 8],
            max_dist: [0.0; 8],
            num_dir: 0,
        }
    }
}

impl DirSet {
    /// Add a direction: linear search; on a hit widen `[min, max]`, on a miss
    /// append.
    pub fn add_dir(&mut self, dir: Direction, lo: f32, hi: f32) {
        let found = self.sound_dir[..self.num_dir]
            .iter()
            .position(|&d| d == dir);
        match found {
            Some(i) => {
                if lo < self.min_dist[i] {
                    self.min_dist[i] = lo;
                }
                if self.max_dist[i] < hi {
                    self.max_dist[i] = hi;
                }
            }
            None => {
                // The original writes past the end if this ever exceeds 8; it cannot, because there
                // are only nine Direction values and InViewerBlock adds the other eight.
                if self.num_dir < 8 {
                    self.sound_dir[self.num_dir] = dir;
                    self.min_dist[self.num_dir] = lo;
                    self.max_dist[self.num_dir] = hi;
                    self.num_dir += 1;
                }
            }
        }
    }

    /// Resetting the count zeroes `num_dir` (and `play_chance`), leaving the
    /// arrays' contents behind — which is invisible, because nothing reads past `num_dir`.
    pub fn reset(&mut self) {
        self.num_dir = 0;
    }
}

/// The sound's position, returning the offset added to the listener's position.
///
/// ```text
/// i     = (int) floor( roll_f32(0, (float)num_dir) )       // draw 1
/// dir   = sound_dir[i]
/// theta = heading(dir) + roll_f32(0, DIR_ANGLE_IN_RAD) - DIR_ANGLE_IN_RAD * 0.5
///                                              ^-- draw 2
/// u     = roll_f32(0, 1)                                    // draw 3
/// r     = min_dist[i] + (max_dist[i] - min_dist[i]) * u * u
/// pos.x += r * sin(theta);  pos.y += r * cos(theta);  pos.z unchanged
/// ```
///
/// * The direction is picked **uniformly over the accumulated set**, not weighted by how much terrain
///   lies each way, and because the `ran2` draw is capped at `RNMX` the index can never reach
///   `num_dir`.
/// * The jitter is `+/- pi/16` (11.25 degrees) about the compass heading.
/// * The radius uses `u`**squared**, which biases the point towards `min_dist`, so ambient sounds
///   cluster near the listener. Combined with the -12 dB/doubling curve this is what gives AC's
///   ambience its "always something nearby" feel.
/// * `z` is the **listener's**, so ambient sounds are always at ear height.
///
/// Returns `None` when the set is empty, the only directional lookup failure. Periodic ambient sounds
/// bypass the lookup with a `return 0` and play from centre instead.
#[must_use]
pub fn get_sound_pos(set: &DirSet, rng: &mut Ran2) -> Option<Vec3> {
    if set.num_dir == 0 {
        // `roll_f32(0.0, 0.0)` returns lo without drawing, `floor(0) == 0`, and the client then
        // indexes `sound_dir[0]` -- an unwritten slot. Nothing reaches here: the play loop only
        // calls [`get_sound_pos`] when `can_hear` said `play_chance > 0`, and `play_chance` is only
        // non-zero after an `add_to` that also called `add_dir`.
        return None;
    }
    // Draw 1: the direction index. `roll_f32(0.0, num_dir)` then `floor` then truncation to an
    // integer.
    let n = u16::try_from(set.num_dir).unwrap_or(8);
    let i = floor_to_i32(rng.roll_f32(0.0, f32::from(n)));
    let i = usize::try_from(i).unwrap_or(0).min(set.num_dir - 1);
    let dir = set.sound_dir[i];
    // Draw 2: the angle jitter, centred by subtracting half the sector.
    let theta = heading(dir) + rng.roll_f32(0.0, DIR_ANGLE_IN_RAD) - DIR_ANGLE_IN_RAD * 0.5;
    // Draw 3: the radius parameter, then squared.
    let u = rng.roll_f32(0.0, 1.0);
    let r = set.min_dist[i] + (set.max_dist[i] - set.min_dist[i]) * u * u;
    Some(Vec3::new(r * math::sinf(theta), r * math::cosf(theta), 0.0))
}

/// The direction index a given `ran2` draw selects, exposed so a test can assert the cap.
#[must_use]
pub fn direction_index(num_dir: usize, u: f32) -> usize {
    let n = u16::try_from(num_dir).unwrap_or(8);
    let i = to_i32((f32::from(n) * u).floor());
    usize::try_from(i).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of this module. Oracle: the client's own placement arithmetic, quoted in
    /// the module docs, and contract 10.5.
    ///
    /// Two `Ran2`s seeded identically: one drives `get_sound_pos`, the other is drawn by hand in the
    /// documented order. If the implementation drew in any other order the two would diverge.
    #[test]
    fn the_draw_order_is_direction_then_angle_then_radius() {
        let mut set = DirSet::default();
        set.add_dir(Direction::NorthOfViewer, 10.0, 30.0);
        set.add_dir(Direction::EastOfViewer, 20.0, 40.0);
        set.add_dir(Direction::SouthOfViewer, 5.0, 15.0);

        let seed = 20_130_918;
        let mut a = Ran2::new(seed);
        let got = get_sound_pos(&set, &mut a).expect("non-empty set");

        let mut b = Ran2::new(seed);
        let i = usize::try_from(floor_to_i32(b.roll_f32(0.0, 3.0))).expect("non-negative");
        let theta =
            heading(set.sound_dir[i]) + b.roll_f32(0.0, DIR_ANGLE_IN_RAD) - DIR_ANGLE_IN_RAD * 0.5;
        let u = b.roll_f32(0.0, 1.0);
        let r = set.min_dist[i] + (set.max_dist[i] - set.min_dist[i]) * u * u;
        let want = Vec3::new(r * math::sinf(theta), r * math::cosf(theta), 0.0);
        assert_eq!(got, want);

        // And the generators are left in the same state -- three draws, no more, no fewer.
        assert_eq!(a.next_f64(), b.next_f64());
    }

    /// The `ran2` draw is capped at `RNMX = 0.99999988`, so `floor(u * num_dir)` never reaches
    /// `num_dir`. Oracle: section 4.8's first bullet, and `dereth_primitives::num::Ran2`'s RNMX clamp.
    #[test]
    fn the_direction_index_can_never_reach_num_dir() {
        for n in 1..=8usize {
            let mut rng = Ran2::new(4242);
            for _ in 0..20_000 {
                let u = rng.roll_f32(0.0, 1.0);
                assert!(direction_index(n, u) < n, "num_dir {n}, u {u}");
            }
            // Even the exact cap.
            assert!(direction_index(n, RNMX_F32) < n);
        }
    }

    /// The `u^2` bias: half the draws land in the nearest quarter of the band. Oracle: section 4.8's
    /// "the density of r is proportional to 1/sqrt", and the squaring of `u` in
    /// the client's own radius draw.
    #[test]
    fn the_radius_is_biased_towards_min_dist_by_squaring_u() {
        let mut set = DirSet::default();
        set.add_dir(Direction::NorthOfViewer, 0.0, 100.0);
        let mut rng = Ran2::new(99);
        let mut near = 0;
        let n = 10_000;
        for _ in 0..n {
            let p = get_sound_pos(&set, &mut rng).expect("non-empty");
            let r = p.magnitude();
            if r < 25.0 {
                near += 1;
            }
        }
        // u < 0.5 gives r < 25, so exactly half by construction -- which is the bias: a uniform
        // radius would put only a quarter of the draws in the nearest quarter.
        let frac = f64::from(near) / f64::from(n);
        assert!(
            (frac - 0.5).abs() < 0.03,
            "expected about half within 25 m, got {frac}"
        );
    }

    /// `add_dir` widens rather than replaces, and the set holds at most one entry per direction.
    /// Oracle: the client's direction accumulation.
    #[test]
    fn add_dir_widens_an_existing_direction_and_never_duplicates_one() {
        let mut set = DirSet::default();
        set.add_dir(Direction::NorthOfViewer, 20.0, 40.0);
        set.add_dir(Direction::NorthOfViewer, 10.0, 30.0);
        set.add_dir(Direction::NorthOfViewer, 25.0, 50.0);
        assert_eq!(set.num_dir, 1);
        assert_eq!(set.min_dist[0], 10.0, "min widens downward");
        assert_eq!(set.max_dist[0], 50.0, "max widens upward");
        for d in super::super::weight::IN_BLOCK_DIRECTIONS {
            set.add_dir(d, 4.0, 10.0);
        }
        assert_eq!(set.num_dir, 8, "eight distinct directions is the ceiling");
    }

    /// Oracle: the heading table documented in section 4.8.
    #[test]
    fn the_compass_headings_match_the_documented_table() {
        assert_eq!(heading(Direction::NorthOfViewer), 0.0);
        assert_eq!(heading(Direction::EastOfViewer), 1.570_796_4);
        #[allow(clippy::approx_constant)] // the client stores this as a float constant
        let south = 3.141_592_7;
        assert_eq!(heading(Direction::SouthOfViewer), south);
        assert_eq!(heading(Direction::WestOfViewer), 4.712_389);
        assert_eq!(
            heading(Direction::InViewerBlock),
            0.0,
            "in-block falls to the default 0.0"
        );
        assert_eq!(heading(Direction::Unknown), 0.0);
        // Compass convention: x is sin(theta), y is cos(theta), so heading 0 is +Y (north).
        let mut set = DirSet::default();
        set.add_dir(Direction::NorthOfViewer, 10.0, 10.0);
        let mut rng = Ran2::new(7);
        let p = get_sound_pos(&set, &mut rng).expect("non-empty");
        assert!(p.y > 9.0, "north means +Y, got {p:?}");
        assert!(p.x.abs() < 2.0, "within the +/-11.25 degree jitter");
        assert_eq!(p.z, 0.0, "z is the listener's, never offset");
    }
}
