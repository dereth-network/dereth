//! The water near the eye, simulated ([`SIM_WGSL`]): where the square of simulated water lies,
//! how many steps the game's clock asks of it each frame, and what disturbs it in each step.
//!
//! The square is centered on the body nearest the eye, or on the eye where none stands near, and
//! moves a whole cell at a time. It steps at a fixed rate by the game's clock, a few steps a frame
//! at the most, so a held clock holds the water still and a long pause does not run it on to catch
//! up. Bodies are followed from one frame to the next as the one nearest where a body stood: a body
//! moving pushes the water from where it stood to where it stands, spread over the frame's steps;
//! a body new to the frame, or come from too far to have walked, pushes nothing.

use dereth_primitives::num::math::{cos, exp};
use glam::{Vec2, Vec3};

/// The simulation's shader.
pub const SIM_WGSL: &str = include_str!("sim.wgsl");
/// The square's side, cells.
pub const SIDE: u32 = 256;
/// A cell's side, meters.
pub const CELL: f32 = 0.08;
/// A step, seconds.
pub const STEP: f64 = 1.0 / 60.0;
/// The most steps a frame runs.
pub const MOST_STEPS: usize = 4;
/// How fast gravity pulls, meters a second a second.
const GRAVITY: f32 = 9.81;
/// The share of gravity the simulation steps with: the kernel, leveled to its center's weight,
/// pulls `1 / KERNEL_CENTER` as hard as water's own vertical derivative, which taken back leaves
/// the waves running as fast as water's of their length; and a little slower than that again, so
/// the small waves the cells can hold have the pace the eye expects of larger ones.
const PACE: f32 = KERNEL_CENTER * 0.75;
/// The kernel's center weight before it is leveled, for a fade of the shortest waves of one:
/// the integral of `q² e^(-q²)`, `√π / 4`.
const KERNEL_CENTER: f32 = 0.443_113_5;
/// How much of its motion the water loses a second.
pub const DAMPING: f32 = 0.7;
/// How far about a cell the water's vertical derivative is taken, cells: the shader's own.
pub const REACH: usize = 5;
/// The most bodies that disturb it.
pub const BODIES: usize = 8;
/// How far from the eye a body stands for the water to be centered on it, meters.
const CENTER_WITHIN: f32 = 30.0;
/// How far a body may move from one frame to the next and still be the same body, meters.
const FOLLOW: f32 = 1.5;
/// The bytes of one step's constants: four vectors, then each body where it starts and ends.
pub const STEP_BYTES: usize = 16 * 4 + 2 * BODIES * 16;
/// The stride steps' constants are laid at: a multiple of every device's offset alignment.
pub const STEP_STRIDE: usize = 512;

/// The simulation's state between frames.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sim {
    /// The square's first cell, world cells, as the last step left it.
    origin: Option<[i64; 2]>,
    /// The clock at the last frame.
    clock: Option<f64>,
    /// The clock's time not yet stepped, seconds.
    owed: f64,
    /// Steps run, for the bodies' sway.
    count: u32,
    /// Where each body stood last frame, in the world.
    bodies: Vec<Vec3>,
}

/// What a frame asks of the simulation.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Each step's constants, in order.
    pub steps: Vec<Vec<u8>>,
    /// The square in render space: its south-west corner and its north-east one.
    pub area: [f32; 4],
    /// Where render space's origin lies in the world, wrapped to the square's side, meters.
    pub fold: Vec2,
}

/// What disturbs the water this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Disturbance<'a> {
    /// The game's clock, seconds.
    pub clock: f64,
    /// Where render space's origin lies in the world.
    pub global: Vec2,
    /// The eye, render space.
    pub eye: Vec3,
    /// Where the bodies stand, render space.
    pub bodies: &'a [Vec3],
    /// How strongly a body moving stirs the water: 0 not at all.
    pub stir: f32,
}

impl Sim {
    /// Plan the frame `d` describes: how many steps run, where the square lies, and what each
    /// step is disturbed by. A clock run back starts the water calm.
    #[must_use]
    pub fn plan(&mut self, d: &Disturbance<'_>) -> Plan {
        if self.clock.is_some_and(|c| d.clock < c) {
            *self = Self::default();
        }
        let elapsed = self.clock.map_or(0.0, |c| d.clock - c);
        self.clock = Some(d.clock);
        self.owed += elapsed;
        // LINT-OK: a handful of steps.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = ((self.owed / STEP).floor() as usize).min(MOST_STEPS);
        // LINT-OK: a handful of steps.
        #[allow(clippy::cast_precision_loss)]
        {
            self.owed -= steps as f64 * STEP;
        }
        // A long pause is not run on to catch up.
        if self.owed > STEP {
            self.owed = 0.0;
        }
        let shift = d.global.extend(0.0);
        let world: Vec<Vec3> = d.bodies.iter().take(BODIES).map(|b| *b + shift).collect();
        // The body nearest the eye, or the eye.
        let eye = d.eye.truncate();
        let center = d
            .bodies
            .iter()
            .map(|b| b.truncate())
            .filter(|b| (*b - eye).length() < CENTER_WITHIN)
            .min_by(|a, b| (*a - eye).length().total_cmp(&(*b - eye).length()))
            .unwrap_or(eye)
            + d.global;
        let half = i64::from(SIDE / 2);
        let first = [cell_of(center.x) - half, cell_of(center.y) - half];
        // Each body where it stood last frame, the one nearest it; none, where none stood near.
        let mut taken = vec![false; self.bodies.len()];
        let came: Vec<Vec3> = world
            .iter()
            .map(|b| {
                let nearest = self
                    .bodies
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !taken[*i])
                    .map(|(i, p)| (i, (*p - *b).length()))
                    .filter(|(_, l)| *l < FOLLOW)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                nearest.map_or(*b, |(i, _)| {
                    taken[i] = true;
                    self.bodies[i]
                })
            })
            .collect();
        let mut out = Vec::with_capacity(steps);
        // LINT-OK: a cell's side and a few hundred cells.
        #[allow(clippy::cast_precision_loss)]
        let side = f64::from(SIDE) * f64::from(CELL);
        for s in 0..steps {
            // The first step clears what it newly holds; the rest hold what it held.
            let held = if s == 0 {
                self.origin.unwrap_or([first[0] + 4 * half, first[1]])
            } else {
                first
            };
            // LINT-OK: a few steps.
            #[allow(clippy::cast_precision_loss)]
            let (a, b) = (s as f32 / steps as f32, (s + 1) as f32 / steps as f32);
            let mut c = Vec::with_capacity(STEP_BYTES);
            for v in [first[0], first[1], held[0], held[1]] {
                // LINT-OK: world cells, well within an i32.
                #[allow(clippy::cast_possible_truncation)]
                c.extend((v as i32).to_le_bytes());
            }
            // LINT-OK: meters in the world and in render space, f32 as the shader reads them.
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let corner = [
                first[0] as f64 * f64::from(CELL),
                first[1] as f64 * f64::from(CELL),
            ];
            #[allow(clippy::cast_possible_truncation)]
            let place = [
                corner[0] as f32,
                corner[1] as f32,
                (corner[0] - f64::from(d.global.x)) as f32,
                (corner[1] - f64::from(d.global.y)) as f32,
            ];
            // LINT-OK: a step, a sixtieth of a second.
            #[allow(clippy::cast_possible_truncation)]
            let step = STEP as f32;
            let pull = GRAVITY * PACE / CELL * step * step;
            for v in place.into_iter().chain([pull, DAMPING * step, 0.0, 0.0]) {
                c.extend(v.to_le_bytes());
            }
            // LINT-OK: a few bodies.
            #[allow(clippy::cast_possible_truncation)]
            for v in [self.count, world.len() as u32, 0, 0] {
                c.extend(v.to_le_bytes());
            }
            for t in [a, b] {
                for i in 0..BODIES {
                    let p = match (came.get(i), world.get(i)) {
                        (Some(f), Some(n)) => {
                            let at = f.lerp(*n, t) - shift;
                            [at.x, at.y, at.z, d.stir]
                        }
                        _ => [0.0; 4],
                    };
                    for v in p {
                        c.extend(v.to_le_bytes());
                    }
                }
            }
            debug_assert_eq!(c.len(), STEP_BYTES);
            out.push(c);
            self.count = self.count.wrapping_add(1);
        }
        if steps > 0 {
            self.origin = Some(first);
        }
        self.bodies = world;
        let held = self.origin.unwrap_or(first);
        // LINT-OK: render-space meters.
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let (x, y) = (
            (held[0] as f64 * f64::from(CELL) - f64::from(d.global.x)) as f32,
            (held[1] as f64 * f64::from(CELL) - f64::from(d.global.y)) as f32,
        );
        #[allow(clippy::cast_possible_truncation)]
        let span = side as f32;
        #[allow(clippy::cast_possible_truncation)]
        let fold = Vec2::new(
            f64::from(d.global.x).rem_euclid(side) as f32,
            f64::from(d.global.y).rem_euclid(side) as f32,
        );
        Plan {
            steps: out,
            area: [x, y, x + span, y + span],
            fold,
        }
    }
}

/// The water's vertical derivative as the simulation takes it, a cell's height pulled by each of
/// its neighbors' within [`REACH`] cells: by how far off they lie, a quarter of it, `REACH + 1`
/// a row. The sum over wave numbers of each one's square, faded at the shortest, times the Bessel
/// function of how far off; leveled to the center's.
#[must_use]
pub fn kernel() -> Vec<f32> {
    let n = REACH + 1;
    let at = |r: f64| -> f64 {
        let dq = 0.001;
        (1..=10_000)
            .map(|i| {
                let q = f64::from(i) * dq;
                q * q * exp(-q * q) * bessel_j0(q * r) * dq
            })
            .sum()
    };
    let center = at(0.0);
    let k: Vec<f64> = (0..n * n)
        .map(|i| {
            // LINT-OK: a few cells.
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = ((i % n) as f64, (i / n) as f64);
            at((x * x + y * y).sqrt()) / center
        })
        .collect();
    // LINT-OK: kernel weights, small.
    #[allow(clippy::cast_possible_truncation)]
    k.into_iter().map(|v| v as f32).collect()
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
fn cell_of(meters: f32) -> i64 {
    // LINT-OK: world cells, a few hundred thousand.
    #[allow(clippy::cast_possible_truncation)]
    let c = (f64::from(meters) / f64::from(CELL)).floor() as i64;
    c
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the water, an art-pack presentation's own)
    use super::*;

    fn at<'a>(clock: f64, bodies: &'a [Vec3]) -> Disturbance<'a> {
        Disturbance {
            clock,
            global: Vec2::new(31_000.0, 18_000.0),
            eye: Vec3::new(0.0, -5.0, 3.0),
            bodies,
            stir: 1.0,
        }
    }

    #[test]
    fn the_kernel_pulls_hardest_at_the_center_and_back_from_a_few_cells_off() {
        let k = kernel();
        let n = REACH + 1;
        assert!((k[0] - 1.0).abs() < 1e-5, "{k:?}");
        assert!(k[0] > k[1] && k[1] > k[2] && k[1] > k[n + 1], "{k:?}");
        assert!(
            k[3] < 0.0 && k[n * n - 1] < 0.0 && k[n * n - 1] > k[3],
            "{k:?}"
        );
        assert!((bessel_j0(0.0) - 1.0).abs() < 1e-6 && bessel_j0(2.404_825_6).abs() < 1e-5);
    }

    #[test]
    fn the_clock_runs_the_water_a_step_at_a_time_and_a_held_clock_runs_none() {
        let mut sim = Sim::default();
        assert!(
            sim.plan(&at(100.0, &[])).steps.is_empty(),
            "the first frame has nothing owed"
        );
        assert_eq!(sim.plan(&at(100.0 + 2.0 * STEP + 1e-6, &[])).steps.len(), 2);
        assert!(sim
            .plan(&at(100.0 + 2.0 * STEP + 1e-6, &[]))
            .steps
            .is_empty());
        // A long pause runs a few steps, not the whole of it.
        assert_eq!(sim.plan(&at(110.0, &[])).steps.len(), MOST_STEPS);
        assert!(sim
            .plan(&at(110.0, &[]))
            .steps
            .iter()
            .all(|s| s.len() == STEP_BYTES));
    }

    #[test]
    fn the_square_follows_the_body_nearest_the_eye_and_folds_with_the_world() {
        let mut sim = Sim::default();
        let body = [Vec3::new(4.0, 3.0, 0.0)];
        let _ = sim.plan(&at(10.0, &body));
        let plan = sim.plan(&at(10.1, &body));
        let [x0, y0, x1, y1] = plan.area;
        assert!(
            x0 < 4.0 && 4.0 < x1 && y0 < 3.0 && 3.0 < y1,
            "{:?}",
            plan.area
        );
        let side = SIDE as f32 * CELL;
        assert!((x1 - x0 - side).abs() < 1e-3);
        assert!((0.0..side).contains(&plan.fold.x) && (0.0..side).contains(&plan.fold.y));
    }

    #[test]
    fn a_body_standing_still_pushes_nothing_and_one_moving_pushes_from_where_it_stood() {
        let read = |step: &[u8], at: usize| {
            f32::from_le_bytes(step[at..at + 4].try_into().expect("four bytes"))
        };
        // The bodies' constants: `from` after the 64 bytes of the rest, `to` after `from`.
        let from_x = 64;
        let to_x = 64 + BODIES * 16;
        let mut sim = Sim::default();
        let _ = sim.plan(&at(1.0, &[Vec3::new(2.0, 0.0, 0.0)]));
        let still = sim.plan(&at(1.0 + 1.5 * STEP, &[Vec3::new(2.0, 0.0, 0.0)]));
        let s = &still.steps[0];
        assert!((read(s, from_x) - read(s, to_x)).abs() < 1e-4);
        let moving = sim.plan(&at(1.0 + 3.5 * STEP, &[Vec3::new(2.1, 0.0, 0.0)]));
        let (first, last) = (&moving.steps[0], &moving.steps[moving.steps.len() - 1]);
        assert!(
            (read(first, from_x) - 2.0).abs() < 1e-3,
            "it starts where it stood"
        );
        assert!(
            (read(last, to_x) - 2.1).abs() < 1e-3,
            "and ends where it stands"
        );
    }
}
