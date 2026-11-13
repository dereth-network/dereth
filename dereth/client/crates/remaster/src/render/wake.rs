//! The water's wake near the eye: where its square of simulated water lies, how many steps the
//! scene's clock asks of it each frame, and what stirs it in each step. The steps run on the
//! device (`cs_wake` in `shaders/water.slang`); the water pass records them before it draws the
//! water, which reads the cells they leave.
//!
//! The square is centered on the player's own body where it stands in or near the water, otherwise
//! on the body nearest the eye, or on the eye where none stands near, and moves a whole cell at a
//! time. It steps at a fixed rate, by the whole steps the scene's clock has
//! passed and a few a frame at the most, so a held clock holds the water still, a long pause does
//! not run it on to catch up, and the same clock always draws the same water. Each body pushes the
//! water from where it stood at the last step to where it stands, spread over the frame's steps; a
//! body new to the water, or come from too far to have walked, pushes nothing.

use dereth_primitives::num::math::{cos, exp};
use glam::Vec3;

use crate::scene::ObjectKey;

/// The square's side, cells.
pub const SIDE: u32 = 256;
/// A cell's side, meters.
pub const CELL: f64 = 0.08;
/// Steps a second.
pub const RATE: f64 = 60.0;
/// The most steps a frame runs.
pub const MOST_STEPS: usize = 4;
/// How far about a cell its neighbors pull it, cells.
pub const REACH: usize = 5;
/// The most bodies that stir it.
pub const BODIES: usize = 8;
/// The rows of one step's constants: the square, the water's pace and the bodies' count, then each
/// body where the step starts and where it ends.
pub const STEP_ROWS: usize = 3 + 2 * BODIES;
/// The rows the kernel takes, four weights a row.
pub const KERNEL_ROWS: usize = (REACH + 1) * (REACH + 1) / 4;
/// How fast gravity pulls, meters a second a second.
const GRAVITY: f64 = 9.81;
/// The kernel's center weight before it is leveled to one: the integral of `q² e^(-q²)` over the
/// wave numbers, `√π / 4`.
const KERNEL_CENTER: f64 = 0.443_113_462_726_379_1;
/// The share of gravity the water steps with: the leveled kernel pulls `1 / KERNEL_CENTER` as hard
/// as water's own vertical derivative, which taken back leaves the ripples running as fast as
/// water's of their length; and a little slower again, so the small ripples the cells hold have
/// the pace the eye expects of larger ones.
const PACE: f64 = KERNEL_CENTER * 0.75;
/// How much of its motion the water loses a second.
const DAMPING: f64 = 0.4;
/// How far from the eye a body stands for the square to be centered on it, meters.
const CENTER_WITHIN: f32 = 30.0;
/// How far a body may move between steps and still push the water from where it stood, meters.
const FOLLOW: f64 = 1.5;
/// The steps the bodies' sway repeats over.
const SWAY_STEPS: i64 = 600;

/// The wake's state between frames.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Wake {
    /// The clock's whole steps at the last frame.
    tick: Option<i64>,
    /// The square's first world cell as the last step left it.
    first: Option<[i64; 2]>,
    /// Where each body stood at the last step, world meters: its feet.
    bodies: Vec<(ObjectKey, [f64; 3])>,
}

/// What a frame asks of the wake.
#[derive(Debug, Clone, PartialEq)]
pub struct WakePlan {
    /// Each step's constants, in order.
    pub steps: Vec<[[u32; 4]; STEP_ROWS]>,
    /// The square the water is drawn from, as the last step left it: its first world cell and its
    /// south-west corner in the frame's space. `None` before any step.
    pub square: Option<([i32; 2], [f32; 2])>,
}

/// What stirs the water this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stir<'a> {
    /// The scene's clock, seconds.
    pub clock: f64,
    /// Where the frame's space's origin lies in the world, meters.
    pub global: [f64; 2],
    /// The eye, the frame's space.
    pub eye: Vec3,
    /// The bodies near the eye, nearest first: each one's feet, the frame's space.
    pub bodies: &'a [(ObjectKey, Vec3)],
    /// Whether the player's own body ([`ObjectKey::Body`]) stands in or near the water: the square
    /// then follows it rather than the body nearest the eye.
    pub own_wet: bool,
}

impl Wake {
    /// Plan the frame `s` describes: how many steps run, where the square lies, and what stirs
    /// each step. A clock run back starts the water calm.
    #[must_use]
    pub fn plan(&mut self, s: &Stir<'_>) -> WakePlan {
        // LINT-OK: the clock's whole steps, well within an i64.
        #[allow(clippy::cast_possible_truncation)]
        let tick = (s.clock * RATE).floor() as i64;
        if self.tick.is_some_and(|t| tick < t) {
            *self = Self::default();
        }
        let last = self.tick.unwrap_or(tick);
        self.tick = Some(tick);
        // LINT-OK: a handful of steps.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = (tick - last).clamp(0, MOST_STEPS as i64) as usize;
        let world: Vec<(ObjectKey, [f64; 3])> = s
            .bodies
            .iter()
            .take(BODIES)
            .map(|(k, b)| {
                (
                    *k,
                    [
                        f64::from(b.x) + s.global[0],
                        f64::from(b.y) + s.global[1],
                        f64::from(b.z),
                    ],
                )
            })
            .collect();
        let eye = s.eye.truncate();
        let own = s
            .bodies
            .iter()
            .find(|(k, _)| s.own_wet && *k == ObjectKey::Body)
            .map(|(_, b)| b.truncate())
            .filter(|b| b.distance(eye) < CENTER_WITHIN);
        let center = own
            .or_else(|| {
                s.bodies
                    .iter()
                    .map(|(_, b)| b.truncate())
                    .filter(|b| b.distance(eye) < CENTER_WITHIN)
                    .min_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)))
            })
            .unwrap_or(eye);
        let half = i64::from(SIDE / 2);
        let first = [
            cell_of(f64::from(center.x) + s.global[0]) - half,
            cell_of(f64::from(center.y) + s.global[1]) - half,
        ];
        // Each body where it stood at the last step; where it stands, where it stood nowhere near.
        let came: Vec<[f64; 3]> = world
            .iter()
            .map(|(k, now)| {
                self.bodies
                    .iter()
                    .find(|(was, _)| was == k)
                    .map(|(_, p)| *p)
                    .filter(|p| {
                        let (dx, dy) = (p[0] - now[0], p[1] - now[1]);
                        (dx * dx + dy * dy).sqrt() < FOLLOW
                    })
                    .unwrap_or(*now)
            })
            .collect();
        // LINT-OK: the frame's space's meters, f32 as the shader reads them.
        #[allow(clippy::cast_possible_truncation)]
        let corner = |c: [i64; 2]| [0, 1].map(|i| (c[i] as f64 * CELL - s.global[i]) as f32);
        // LINT-OK: the water's pace over a step, small.
        #[allow(clippy::cast_possible_truncation)]
        let pace = [
            (GRAVITY * PACE / CELL / (RATE * RATE)) as f32,
            (DAMPING / RATE) as f32,
        ];
        let mut out = Vec::with_capacity(steps);
        for step in 0..steps {
            // The first step clears the cells it newly holds; the rest hold what it held.
            let before = match self.first {
                Some(f) if step == 0 => f,
                None if step == 0 => [first[0] + 4 * i64::from(SIDE), first[1]],
                _ => first,
            };
            let mut rows = [[0u32; 4]; STEP_ROWS];
            // LINT-OK: world cells, a few hundred thousand.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                rows[0] = [first[0], first[1], before[0], before[1]].map(|v| v as i32 as u32);
            }
            let [x, y] = corner(first);
            rows[1] = [x, y, pace[0], pace[1]].map(f32::to_bits);
            // LINT-OK: the sway's count, under 600, and a few bodies.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                rows[2] = [
                    (last + step as i64).rem_euclid(SWAY_STEPS) as u32,
                    world.len() as u32,
                    0,
                    0,
                ];
            }
            // LINT-OK: a few steps.
            #[allow(clippy::cast_precision_loss)]
            let (a, b) = (step as f64 / steps as f64, (step + 1) as f64 / steps as f64);
            for (k, ((_, now), was)) in world.iter().zip(&came).enumerate() {
                let at = |t: f64| {
                    // LINT-OK: the frame's space's meters, f32 as the shader reads them.
                    #[allow(clippy::cast_possible_truncation)]
                    let p = [0, 1, 2].map(|i| {
                        let off = if i < 2 { s.global[i] } else { 0.0 };
                        (was[i] + (now[i] - was[i]) * t - off) as f32
                    });
                    [p[0], p[1], p[2], 1.0].map(f32::to_bits)
                };
                rows[3 + k] = at(a);
                rows[3 + BODIES + k] = at(b);
            }
            out.push(rows);
        }
        if steps > 0 {
            self.first = Some(first);
            self.bodies = world;
        }
        // LINT-OK: world cells, a few hundred thousand.
        #[allow(clippy::cast_possible_truncation)]
        let square = self.first.map(|f| ([f[0] as i32, f[1] as i32], corner(f)));
        WakePlan { steps: out, square }
    }
}

/// The kernel the wake's cells pull each other through, the water's vertical derivative as the
/// cells hold it: by how far off a neighbor lies, a quarter of it, `REACH + 1` a row. Each weight
/// is the sum over wave numbers of each one's square, faded at the shortest, times the Bessel
/// function of how far off; leveled to the center's.
#[must_use]
pub fn kernel() -> [f32; (REACH + 1) * (REACH + 1)] {
    let n = REACH + 1;
    let at = |r: f64| -> f64 {
        let dq = 0.001;
        (1..=6_000)
            .map(|i| {
                let q = f64::from(i) * dq;
                q * q * exp(-q * q) * bessel_j0(q * r) * dq
            })
            .sum()
    };
    let center = at(0.0);
    let mut k = [0.0; (REACH + 1) * (REACH + 1)];
    for (i, w) in k.iter_mut().enumerate() {
        // LINT-OK: a few cells.
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = ((i % n) as f64, (i / n) as f64);
        // LINT-OK: kernel weights, small.
        #[allow(clippy::cast_possible_truncation)]
        {
            *w = (at((x * x + y * y).sqrt()) / center) as f32;
        }
    }
    k
}

/// The kernel as the shader reads it: four weights a row.
#[must_use]
pub fn kernel_rows() -> [[u32; 4]; KERNEL_ROWS] {
    let k = kernel();
    let mut rows = [[0; 4]; KERNEL_ROWS];
    for (i, w) in k.iter().enumerate() {
        rows[i / 4][i % 4] = w.to_bits();
    }
    rows
}

/// The Bessel function of the first kind and order nought, after Abramowitz and Stegun 9.4.1 and
/// 9.4.3: within a few millionths.
fn bessel_j0(x: f64) -> f64 {
    let x = x.abs();
    if x <= 3.0 {
        let y = (x / 3.0) * (x / 3.0);
        1.0 + y
            * (-2.249_999_7
                + y * (1.265_620_8
                    + y * (-0.316_386_6
                        + y * (0.044_447_9 + y * (-0.003_944_4 + y * 0.000_210_0)))))
    } else {
        let y = 3.0 / x;
        let f = 0.797_884_56
            + y * (-0.000_000_77
                + y * (-0.005_527_40
                    + y * (-0.000_095_12
                        + y * (0.001_372_37 + y * (-0.000_728_05 + y * 0.000_144_76)))));
        let t = x - std::f64::consts::FRAC_PI_4
            + y * (-0.041_663_97
                + y * (-0.000_039_54
                    + y * (0.002_625_73
                        + y * (-0.000_541_25 + y * (-0.000_293_33 + y * 0.000_135_58)))));
        f * cos(t) / x.sqrt()
    }
}

/// The world cell holding `meters`.
fn cell_of(meters: f64) -> i64 {
    // LINT-OK: world cells, a few hundred thousand.
    #[allow(clippy::cast_possible_truncation)]
    let c = (meters / CELL).floor() as i64;
    c
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    const GLOBAL: [f64; 2] = [31_000.0, 18_000.0];

    fn at<'a>(clock: f64, bodies: &'a [(ObjectKey, Vec3)]) -> Stir<'a> {
        Stir {
            clock,
            global: GLOBAL,
            eye: Vec3::new(0.0, -5.0, 3.0),
            bodies,
            own_wet: false,
        }
    }

    fn float(rows: &[[u32; 4]; STEP_ROWS], row: usize, i: usize) -> f32 {
        f32::from_bits(rows[row][i])
    }

    /// The step the shader takes, for open water with nothing stirring it: each cell pulled by its
    /// neighbors through the kernel, its motion damped, and the cells `land` says are land held
    /// at the water's level. Heights now and a step ago.
    fn step(now: &[(f32, f32)], n: usize, land: &dyn Fn(usize, usize) -> bool) -> Vec<(f32, f32)> {
        let k = kernel();
        // LINT-OK: the water's pace over a step, small.
        #[allow(clippy::cast_possible_truncation)]
        let (pull, a) = (
            (GRAVITY * PACE / CELL / (RATE * RATE)) as f32,
            (DAMPING / RATE) as f32,
        );
        let r = REACH as isize;
        let mut out = vec![(0.0, 0.0); n * n];
        for y in 0..n {
            for x in 0..n {
                if land(x, y) {
                    continue;
                }
                let mut d = 0.0;
                for dy in -r..=r {
                    for dx in -r..=r {
                        let (xx, yy) = (x as isize + dx, y as isize + dy);
                        if xx < 0 || yy < 0 || xx >= n as isize || yy >= n as isize {
                            continue;
                        }
                        let w = k[dy.unsigned_abs() * (REACH + 1) + dx.unsigned_abs()];
                        d += w * now[yy as usize * n + xx as usize].0;
                    }
                }
                let (h, before) = now[y * n + x];
                out[y * n + x] = ((h * (2.0 - a) - before - pull * d) / (1.0 + a), h);
            }
        }
        out
    }

    fn energy(cells: &[(f32, f32)]) -> f64 {
        cells
            .iter()
            .map(|(h, b)| f64::from(*h) * f64::from(*h) + f64::from(h - b) * f64::from(h - b))
            .sum()
    }

    #[test]
    fn the_kernel_pulls_hardest_at_the_center_and_back_from_a_few_cells_off() {
        let k = kernel();
        let n = REACH + 1;
        assert!((k[0] - 1.0).abs() < 1e-6, "{k:?}");
        assert!(k[0] > k[1] && k[1] > k[2] && k[1] > k[n + 1], "{k:?}");
        assert!(k[3] < 0.0 && k[n * n - 1] < 0.0, "{k:?}");
        assert!((bessel_j0(0.0) - 1.0).abs() < 1e-6 && bessel_j0(2.404_825_6).abs() < 1e-5);
        assert_eq!(f32::from_bits(kernel_rows()[0][1]), k[1]);
    }

    /// A ripple dropped in still water spreads out evenly every way, its front moving out step
    /// after step, and dies away as the water loses its motion; where it meets land it turns
    /// back.
    #[test]
    fn a_ripple_spreads_evenly_dies_away_and_turns_back_from_the_shore() {
        let n = 64;
        let mid = n / 2;
        let open = |_: usize, _: usize| false;
        let mut cells = vec![(0.0f32, 0.0f32); n * n];
        cells[mid * n + mid] = (0.05, 0.05);
        let front = |c: &[(f32, f32)]| {
            (0..mid)
                .rev()
                .find(|r| c[mid * n + mid + r].0.abs() > 1e-5)
                .unwrap_or(0)
        };
        let mut fronts = Vec::new();
        let mut energies = Vec::new();
        for s in 0..60 {
            cells = step(&cells, n, &open);
            if s % 20 == 19 {
                fronts.push(front(&cells));
                energies.push(energy(&cells));
            }
        }
        assert!(fronts.windows(2).all(|w| w[1] > w[0]), "{fronts:?}");
        // Even every way: east as west, north as south, along a diagonal as across it.
        let h = |x: usize, y: usize| cells[y * n + x].0;
        for r in 1..10 {
            assert!((h(mid + r, mid) - h(mid - r, mid)).abs() < 1e-6);
            assert!((h(mid + r, mid) - h(mid, mid + r)).abs() < 1e-6);
            assert!((h(mid + r, mid + r) - h(mid - r, mid - r)).abs() < 1e-6);
        }
        for _ in 0..600 {
            cells = step(&cells, n, &open);
        }
        assert!(
            energy(&cells) < energies[0] * 0.5,
            "{} of {energies:?}",
            energy(&cells)
        );
        // A wall of land east of the drop: the water west of it, nearer the drop, is stirred more
        // than the same water with no wall, by what the wall sends back.
        let wall = |x: usize, _: usize| x >= mid + 8;
        let (mut walled, mut free) = (vec![(0.0f32, 0.0f32); n * n], vec![(0.0f32, 0.0f32); n * n]);
        walled[mid * n + mid] = (0.05, 0.05);
        free[mid * n + mid] = (0.05, 0.05);
        let mut back = 0.0f64;
        for _ in 0..120 {
            walled = step(&walled, n, &wall);
            free = step(&free, n, &open);
            back = back.max(f64::from(
                (walled[mid * n + mid + 4].0 - free[mid * n + mid + 4].0).abs(),
            ));
        }
        assert!(back > 1e-4, "the wall sends the ripples back: {back}");
        assert!(walled
            .iter()
            .enumerate()
            .all(|(i, c)| i % n < mid + 8 || c.0 == 0.0));
    }

    #[test]
    fn the_clock_steps_the_water_by_its_whole_steps_and_a_held_clock_steps_none() {
        let mut w = Wake::default();
        let base = 306_486_294.727_053_6;
        assert!(
            w.plan(&at(base, &[])).steps.is_empty(),
            "the first frame owes none"
        );
        assert!(
            w.plan(&at(base, &[])).steps.is_empty(),
            "a held clock steps none"
        );
        // Steps of a thirtieth of a second run two each, over a long run.
        let mut total = 0;
        for f in 1..=300 {
            total += w.plan(&at(base + f64::from(f) / 30.0, &[])).steps.len();
        }
        assert!((598..=602).contains(&total), "{total}");
        // A long pause runs a few, not the whole of it.
        assert_eq!(w.plan(&at(base + 100.0, &[])).steps.len(), MOST_STEPS);
        // A clock run back starts the water calm, with nothing to draw.
        let back = w.plan(&at(base, &[]));
        assert!(back.steps.is_empty() && back.square.is_none());
    }

    #[test]
    fn the_same_clock_and_bodies_plan_the_same_steps() {
        let body = [(ObjectKey::Body, Vec3::new(2.0, 1.0, 0.0))];
        let walked = [(ObjectKey::Body, Vec3::new(2.1, 1.0, 0.0))];
        let run = || {
            let mut w = Wake::default();
            let _ = w.plan(&at(10.0, &body));
            w.plan(&at(10.0 + 3.5 / RATE, &walked))
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn the_square_follows_the_body_nearest_the_eye_by_whole_cells() {
        let mut w = Wake::default();
        let body = [(ObjectKey::Body, Vec3::new(4.0, 3.0, 0.0))];
        let _ = w.plan(&at(10.0, &body));
        let plan = w.plan(&at(10.1, &body));
        let (first, [x, y]) = plan.square.expect("a square");
        let side = f64::from(SIDE) * CELL;
        // LINT-OK: test meters.
        #[allow(clippy::cast_possible_truncation)]
        let side = side as f32;
        assert!(
            x < 4.0 && 4.0 < x + side && y < 3.0 && 3.0 < y + side,
            "{x} {y}"
        );
        // Its corner is its first cell's, in the frame's space.
        let back = f64::from(first[0]) * CELL - GLOBAL[0];
        assert!((back - f64::from(x)).abs() < 1e-3);
        // Far from any body, it lies about the eye.
        let far = [(ObjectKey::Body, Vec3::new(400.0, 0.0, 0.0))];
        let _ = w.plan(&at(10.2, &far));
        let (_, [x, y]) = w.plan(&at(10.3, &far)).square.expect("a square");
        assert!(x < 0.0 && 0.0 < x + side && y < -5.0 && -5.0 < y + side);
    }

    /// Where the player's own body stands in or near the water, the square follows it, though
    /// another body stands nearer the eye; elsewhere it follows the body nearest the eye.
    #[test]
    fn the_square_follows_the_players_own_body_where_it_stands_in_the_water() {
        let bodies = [
            (ObjectKey::Server(7), Vec3::new(0.0, -2.0, 0.0)),
            (ObjectKey::Body, Vec3::new(0.0, 20.0, 0.0)),
        ];
        let side = f64::from(SIDE) * CELL;
        // LINT-OK: test meters.
        #[allow(clippy::cast_possible_truncation)]
        let side = side as f32;
        let holds = |own_wet: bool, y: f32| {
            let mut w = Wake::default();
            let first = Stir {
                own_wet,
                ..at(10.0, &bodies)
            };
            let _ = w.plan(&first);
            let then = Stir {
                own_wet,
                ..at(10.1, &bodies)
            };
            let (_, [_, low]) = w.plan(&then).square.expect("a square");
            low < y && y < low + side
        };
        assert!(
            holds(true, 20.0) && !holds(true, -2.0),
            "the player's own body"
        );
        assert!(
            holds(false, -2.0) && !holds(false, 20.0),
            "the body nearest the eye"
        );
    }

    #[test]
    fn a_body_standing_still_pushes_nothing_and_one_walking_pushes_from_where_it_stood() {
        let mut w = Wake::default();
        let still = [(ObjectKey::Body, Vec3::new(2.0, 0.0, 0.0))];
        let _ = w.plan(&at(1.0, &still));
        let plan = w.plan(&at(1.0 + 1.5 / RATE, &still));
        let s = &plan.steps[0];
        assert_eq!(s[3], s[3 + BODIES]);
        let walked = [(ObjectKey::Body, Vec3::new(2.1, 0.0, 0.0))];
        let plan = w.plan(&at(1.0 + 3.5 / RATE, &walked));
        let (first, last) = (&plan.steps[0], &plan.steps[plan.steps.len() - 1]);
        assert!(
            (float(first, 3, 0) - 2.0).abs() < 1e-3,
            "it starts where it stood"
        );
        assert!(
            (float(last, 3 + BODIES, 0) - 2.1).abs() < 1e-3,
            "and ends where it stands"
        );
        // A body come from far off pushes nothing.
        let jumped = [(ObjectKey::Body, Vec3::new(9.0, 0.0, 0.0))];
        let plan = w.plan(&at(1.0 + 5.5 / RATE, &jumped));
        assert_eq!(plan.steps[0][3], plan.steps[0][3 + BODIES]);
    }

    /// The frame's space moving to another block leaves a body standing still where it stood.
    #[test]
    fn a_body_standing_still_across_a_change_of_block_pushes_nothing() {
        let mut w = Wake::default();
        let _ = w.plan(&at(1.0, &[(ObjectKey::Body, Vec3::new(191.9, 0.0, 0.0))]));
        let mut moved = at(1.0 + 1.5 / RATE, &[]);
        let body = [(ObjectKey::Body, Vec3::new(-0.1, 0.0, 0.0))];
        moved.bodies = &body;
        moved.global = [GLOBAL[0] + 192.0, GLOBAL[1]];
        moved.eye = Vec3::new(-192.0, -5.0, 3.0);
        let plan = w.plan(&moved);
        assert_eq!(plan.steps[0][3], plan.steps[0][3 + BODIES]);
    }

    /// The bodies' sway and the steps' count run on with the clock's whole steps alone.
    #[test]
    fn the_sway_counts_the_clocks_whole_steps() {
        let mut w = Wake::default();
        let _ = w.plan(&at(100.0, &[]));
        let plan = w.plan(&at(100.0 + 2.5 / RATE, &[]));
        let first = plan.steps[0][2][0];
        assert_eq!(plan.steps[1][2][0], (first + 1) % 600);
        // LINT-OK: the test's whole steps.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let expect = ((100.0 * RATE).floor() as i64).rem_euclid(600) as u32;
        assert_eq!(first, expect);
    }
}
