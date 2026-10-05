//! The adaptive-degrade feedback loop and the frame-rate meter that
//! feeds it.
//!
//! Degrade records are described in `docs/formats/22-degrade-info.md`. The feedback loop runs at
//! frame end, before overlay and presentation.
//!
//! **This is a function of frame rate.** `deg_mul` slides every LOD threshold, the
//! static/dynamic light counts and the two share distances at once. A modern machine drifts to
//! `deg_mul = +1` where a 2013 machine did not.
//!
//! which value the loop settled at on 2013 hardware was never measured.
//! [`DegradeGovernor::pinned`] is what every test and every golden image must use, and
//! [`dereth_terrain::consts::PINNED_DEG_MUL`] is the value it pins to.
//!
//! **How a rebuild keeps a capture reproducible.** The loop's only input is the frame-rate meter's
//! answer, and the meter measures *whatever clock it is pushed*. The client's `--headless` steps
//! advances by a fixed quantum, so a headless frame pushes exactly that quantum, the
//! meter reports the same number on every machine, and the loop's whole trajectory is a function
//! of the frame *count* rather than of elapsed time. Nothing in this module reads a wall clock.

/// The three response-curve targets: minimum, ideal, and maximum frame rate.
/// The three frame-rate preferences the degrade loop reads.
///
/// **These are constants in the shipped client, not preferences.** An earlier format description
/// called them "user preferences"; nothing in the client ever writes them. Their values are
/// `8 / 10 / 20` in retail.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FramerateTargets {
    pub min: f32,
    pub ideal: f32,
    pub max: f32,
}

impl Default for FramerateTargets {
    /// The retail values: minimum 8, ideal 10 and maximum 20 frames per second,
    /// confirmed against the client itself.
    fn default() -> Self {
        Self {
            min: 8.0,
            ideal: 10.0,
            max: 20.0,
        }
    }
}

/// The 30-entry history the anti-oscillation dead-band consults.
pub const DEG_MUL_HISTORY: usize = 30;

/// How many of those entries the dead-band scan actually reads.
///
/// The scan walks the history while the index is below 29, i.e. indices `0..=28`. Index 29 is
/// written *after* the decision and is never
/// compared against the candidate that was just accepted.
pub const DEG_MUL_HISTORY_SCANNED: usize = 29;

/// The dead-band: a candidate within this of any recent value is ignored.
pub const DEG_MUL_DEAD_BAND: f32 = 0.01;

/// Object-distance threshold: `(25 + 17*deg_mul)²` for `deg_mul >= 0`,
/// `(25 + 25*deg_mul)²` below — the squared **horizontal** distance inside which every part of an
/// object measures its own viewer distance and heading; at or past it every part is handed the
/// object's own. **It is not a cull.** See [`crate::objects::parts::shared_viewer_distance`].
#[must_use]
pub fn object_distance_2dsq(deg_mul: f32) -> f32 {
    let d = deg_mul.clamp(-1.0, 1.0);
    let r = if d >= 0.0 {
        25.0 + 17.0 * d
    } else {
        25.0 + 25.0 * d
    };
    r * r
}

/// All thresholds controlled by `deg_mul`, in one place.
///
/// | Output | `d >= 0` | `d < 0` |
/// |---|---|---|
/// | `max_static_lights` | `int(40 + 20d)` | `int(40 + 20d)` |
/// | `max_dynamic_lights` | `int(7 + 2d)` | `int(7 + 3d)` |
/// | `object_distance_2dsq` | `(25 + 17d)²` | `(25 + 25d)²` |
/// | `particle_distance_2dsq` | `(16 + 8d)²` | `(16 + 9d)²` |
///
/// Arithmetic of retail's adaptive degrade. The two
/// distances are **not culls**: past them the client stops
/// measuring each part individually and hands every part the object's own distance and heading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DegradeLevel {
    /// Degradation multiplier, clamped to `[-1, +1]`.
    pub deg_mul: f32,
    pub max_static_lights: usize,
    pub max_dynamic_lights: usize,
    pub object_distance_2dsq: f32,
    pub particle_distance_2dsq: f32,
}

/// The object share distance before the setter first runs: **25.0**, a 5 m horizontal radius.
///
/// This is the value the client starts with, and it is *not* the setter's answer at a zero bias
/// (`25² = 625`). The setter runs only when the automatic loop accepts a new bias, so a client with
/// automatic degrades off, or one whose loop has not yet moved, shares at 5 m.
pub const STARTUP_OBJECT_DISTANCE_2DSQ: f32 = 25.0;

/// The particle share distance before the setter first runs: **16.0**, a 4 m horizontal radius,
/// against the setter's `16² = 256` at a zero bias. See [`STARTUP_OBJECT_DISTANCE_2DSQ`].
pub const STARTUP_PARTICLE_DISTANCE_2DSQ: f32 = 16.0;

impl DegradeLevel {
    /// The whole of the client's internal degrade-level setter.
    #[must_use]
    pub fn new(deg_mul: f32) -> Self {
        let d = deg_mul.clamp(-1.0, 1.0);
        let (max_static_lights, max_dynamic_lights) = crate::lighting::pool_caps(d);
        Self {
            deg_mul: d,
            max_static_lights,
            max_dynamic_lights,
            object_distance_2dsq: object_distance_2dsq(d),
            particle_distance_2dsq: crate::particles::particle_distance_2dsq(d),
        }
    }

    /// The outputs as the client starts, before the setter has ever run: a zero bias, 40 static
    /// and 7 dynamic lights (the setter's own answer at zero), and the two share distances at
    /// their initial [`STARTUP_OBJECT_DISTANCE_2DSQ`] and [`STARTUP_PARTICLE_DISTANCE_2DSQ`],
    /// which the setter's zero-bias answer does **not** reproduce.
    #[must_use]
    pub fn startup() -> Self {
        Self {
            object_distance_2dsq: STARTUP_OBJECT_DISTANCE_2DSQ,
            particle_distance_2dsq: STARTUP_PARTICLE_DISTANCE_2DSQ,
            ..Self::new(0.0)
        }
    }

    /// The squared horizontal distance an object's parts share its viewer distance at: the
    /// particle distance for a particle emitter object, the object distance for everything else.
    #[must_use]
    pub fn share_distance_2dsq(&self, particle_emitter: bool) -> f32 {
        if particle_emitter {
            self.particle_distance_2dsq
        } else {
            self.object_distance_2dsq
        }
    }
}

impl Default for DegradeLevel {
    /// [`DegradeLevel::startup`]: what a pinned governor at the default bias holds.
    fn default() -> Self {
        DegradeGovernor::default().level()
    }
}

/// Calculates the degradation level and permits a pinned override.
///
/// `auto` off means `deg_mul` never moves, which is what every test does. With `auto` on the loop
/// runs exactly as the retail degradation rules do.
#[derive(Debug, Clone)]
pub struct DegradeGovernor {
    /// Whether automatic multiplier updates are enabled.
    pub auto: bool,
    /// Degradation multiplier in `[-1, +1]`.
    pub deg_mul: f32,
    pub targets: FramerateTargets,
    history: [f32; DEG_MUL_HISTORY],
    /// What the setter last produced, or [`DegradeLevel::startup`] if it has not run.
    level: DegradeLevel,
}

impl Default for DegradeGovernor {
    fn default() -> Self {
        Self::pinned(dereth_terrain::consts::PINNED_DEG_MUL)
    }
}

impl DegradeGovernor {
    /// A governor that never moves. **Every test and every golden image uses this**, and says what
    /// it pinned to.
    ///
    /// Pinned at the startup bias of zero it is the client with automatic degrades off, whose
    /// setter never runs: its outputs are [`DegradeLevel::startup`]. Pinned anywhere else it is a
    /// client whose loop reached that bias through the setter and was then switched off, so its
    /// outputs are the setter's at that bias.
    #[must_use]
    pub fn pinned(deg_mul: f32) -> Self {
        let level = if deg_mul == 0.0 {
            DegradeLevel::startup()
        } else {
            DegradeLevel::new(deg_mul)
        };
        Self {
            auto: false,
            deg_mul,
            targets: FramerateTargets::default(),
            history: [deg_mul; DEG_MUL_HISTORY],
            level,
        }
    }

    /// The live loop, for a client that wants the original's behaviour. It starts where the
    /// client starts: a zero bias and [`DegradeLevel::startup`] until the loop first accepts a
    /// change.
    #[must_use]
    pub fn automatic(targets: FramerateTargets) -> Self {
        Self {
            auto: true,
            deg_mul: 0.0,
            targets,
            history: [0.0; DEG_MUL_HISTORY],
            level: DegradeLevel::startup(),
        }
    }

    /// The degrade-level setter's four outputs as they stand: set together whenever the loop
    /// accepts a bias, and [`DegradeLevel::startup`] until it first does.
    #[must_use]
    pub fn level(&self) -> DegradeLevel {
        self.level
    }

    /// The candidate bias for one measured frame rate, before the dead-band.
    ///
    /// ```text
    /// a  = min*0.75
    /// b  = min*0.25 + ideal*0.75
    /// c  = min*0.50 + ideal*0.50
    /// dd = max*0.50 + ideal*0.50
    /// e  = max*0.25 + ideal*0.75
    /// g  = max*1.25
    /// wa = (f >= a) ? max(0, 1 - |f - a| / (c - a)) : 1
    /// wb =            max(0, 1 - |2f - (b + min)|  / (b - min))
    /// wc =            max(0, 1 - |2f - (dd + c)|   / (dd - c))
    /// we =            max(0, 1 - |2f - (max + e)|  / (max - e))
    /// wg = (f <= g) ? max(0, 1 - |f - g| / (g - dd)) : 1
    /// delta = (wg*0.10 + we*0.01 + wc*0.00 - wa*0.15 - wb*0.02) / (wg + we + wc + wb + wa)
    /// cand  = clamp(deg_mul + delta, -1.0, +1.0)
    /// ```
    ///
    /// The asymmetry is deliberate: the "too slow" weights carry `-0.15` and `-0.02` while the "too
    /// fast" weights carry only `+0.10` and `+0.01`, so quality is shed roughly 1.5x faster than it
    /// is restored.
    ///
    /// `wc`'s coefficient is **zero**, and with the retail targets `wc` is the only weight with
    /// support over `[9.5, 12.5]` fps — so that band is an exact equilibrium, not an approximate
    /// one, and it is where the loop comes to rest.
    #[must_use]
    pub fn candidate(&self, fps: f32) -> f32 {
        let t = self.targets;
        let a = t.min * 0.75;
        let b = t.min * 0.25 + t.ideal * 0.75;
        let c = t.min * 0.50 + t.ideal * 0.50;
        let dd = t.max * 0.50 + t.ideal * 0.50;
        let e = t.max * 0.25 + t.ideal * 0.75;
        let g = t.max * 1.25;
        let ramp = |num: f32, den: f32| {
            if den == 0.0 {
                0.0
            } else {
                (1.0 - num / den).max(0.0)
            }
        };
        let wa = if fps >= a {
            ramp((fps - a).abs(), c - a)
        } else {
            1.0
        };
        let wb = ramp((2.0 * fps - (b + t.min)).abs(), b - t.min);
        let wc = ramp((2.0 * fps - (dd + c)).abs(), dd - c);
        let we = ramp((2.0 * fps - (t.max + e)).abs(), t.max - e);
        let wg = if fps <= g {
            ramp((fps - g).abs(), g - dd)
        } else {
            1.0
        };
        let total = wg + we + wc + wb + wa;
        if total == 0.0 {
            return self.deg_mul;
        }
        let delta = (wg * 0.10 + we * 0.01 + wc * 0.00 - wa * 0.15 - wb * 0.02) / total;
        (self.deg_mul + delta).clamp(-1.0, 1.0)
    }

    /// One feedback-loop update at the end of each frame.
    ///
    /// The order is the client's and all three parts of it matter:
    ///
    /// 1. **The history shifts on every call**, before `auto_update_deg_mul` is even tested —
    ///    the degrade-level calculation's copy of `history[1..30]` down to `history[0..29]`
    ///    is the first    thing in the function. So it is a *sliding 30-frame window*, not a log of
    ///    accepted    changes, and a value the loop passed through is forgotten thirty frames later.
    /// 2. The dead-band scans `history[0..=28]` — [`DEG_MUL_HISTORY_SCANNED`] entries, not 30.
    /// 3. `history[29] = deg_mul` runs **after** the internal degrade-level setter, on every
    ///    path, so
    ///    the newest slot holds the *new* bias when the change was taken and the unchanged one
    ///    when it was suppressed.
    ///
    /// The 30-frame window plus the 0.01 dead-band is what stops the LOD level from flickering
    /// when the frame rate sits exactly on a threshold.
    pub fn use_time(&mut self, fps: f32) {
        // 1. `for (i = 0; i < 29; i++) history[i] = history[i + 1];` — the entry rotated
        //    into the last slot is overwritten unconditionally at step 3 and is never scanned.
        self.history.rotate_left(1);
        if self.auto {
            let cand = self.candidate(fps);
            // 2. The scan stops, i.e. after `history[28]`.
            let seen = self.history[..DEG_MUL_HISTORY_SCANNED]
                .iter()
                .any(|&h| (h - cand).abs() < DEG_MUL_DEAD_BAND);
            if !seen {
                // Apply the candidate degrade level; [`DegradeLevel`] is its other half.
                self.deg_mul = cand;
                self.level = DegradeLevel::new(cand);
            }
        }
        // 3. `history[29] = deg_mul;`
        self.history[DEG_MUL_HISTORY - 1] = self.deg_mul;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: "`DegradeGovernor` can be pinned to
    /// a constant and the whole suite runs with it pinned." A pinned governor must be inert whatever
    /// the frame rate, so no measurement becomes a function of the test machine's speed.
    #[test]
    fn a_pinned_governor_never_moves() {
        let mut g = DegradeGovernor::pinned(dereth_terrain::consts::PINNED_DEG_MUL);
        for fps in [1.0f32, 15.0, 30.0, 60.0, 240.0, 10_000.0] {
            g.use_time(fps);
            assert_eq!(
                g.deg_mul,
                dereth_terrain::consts::PINNED_DEG_MUL,
                "fps={fps}"
            );
        }
        assert_eq!(
            DegradeGovernor::default().deg_mul,
            dereth_terrain::consts::PINNED_DEG_MUL
        );
        assert!(!DegradeGovernor::default().auto);
    }

    /// Oracle: the client's own data for the three frame-rate preferences, which hold
    /// `8.0 / 10.0 / 20.0`, and every use of them in the client is a read. Nothing writes them,
    /// so a shipped
    /// client's curve is these numbers and no other.
    #[test]
    fn the_targets_are_the_retail_eight_ten_twenty() {
        let t = FramerateTargets::default();
        assert_eq!((t.min, t.ideal, t.max), (8.0, 10.0, 20.0));
    }

    /// Oracle: the automatic bias loop is deliberately asymmetric: the 'too slow' weights carry
    /// -0.15 and -0.02 while the 'too fast' weights carry only +0.10 and
    /// +0.01, so quality is shed roughly 1.5x faster than it is restored.
    #[test]
    fn quality_is_shed_faster_than_it_is_restored() {
        let g = DegradeGovernor::automatic(FramerateTargets::default());
        let slow = g.candidate(5.0);
        let fast = g.candidate(200.0);
        assert!(slow < 0.0, "a bad frame rate lowers the bias: {slow}");
        assert!(fast > 0.0, "a good frame rate raises it: {fast}");
        assert!(
            slow.abs() > fast.abs(),
            "shedding ({slow}) must be faster than restoring ({fast})"
        );
        assert!(
            (slow.abs() / fast.abs() - 1.5).abs() < 0.2,
            "roughly 1.5x: {slow} vs {fast}"
        );
    }

    /// Oracle: the feedback formula clamps `cand = clamp(deg_mul + delta, -1.0, +1.0)`, so the bias can never
    /// leave `[-1, +1]` however long the loop runs. A fast machine drifts to `+1`.
    #[test]
    fn the_bias_saturates_at_plus_and_minus_one() {
        let mut g = DegradeGovernor::automatic(FramerateTargets::default());
        for _ in 0..500 {
            g.use_time(10_000.0);
        }
        assert!(
            (g.deg_mul - 1.0).abs() < 1e-6,
            "a fast machine drifts to +1: {}",
            g.deg_mul
        );
        let mut g = DegradeGovernor::automatic(FramerateTargets::default());
        for _ in 0..500 {
            g.use_time(1.0);
        }
        assert!(
            (g.deg_mul + 1.0).abs() < 1e-6,
            "a slow machine drifts to -1: {}",
            g.deg_mul
        );
    }

    /// Oracle: if any recent bias value is within 0.01 of the candidate, the update does nothing.
    /// The dead-band is what stops the LOD level flickering on a threshold.
    ///
    /// With the retail targets the neutral frame rate is `ideal_framerate` itself: `wc` is the only
    /// weight with support there and its coefficient is 0, so `delta` is exactly zero.
    #[test]
    fn the_dead_band_suppresses_a_tiny_correction() {
        let mut g = DegradeGovernor::automatic(FramerateTargets::default());
        let ideal = g.targets.ideal;
        let cand = g.candidate(ideal);
        assert!(
            cand.abs() < DEG_MUL_DEAD_BAND,
            "the candidate is tiny: {cand}"
        );
        g.use_time(ideal);
        assert_eq!(g.deg_mul, 0.0, "and it was suppressed");
        assert_eq!(DEG_MUL_HISTORY, 30);
        assert_eq!(DEG_MUL_HISTORY_SCANNED, 29);
    }

    /// Oracle: [`DegradeGovernor::candidate`]'s five weight supports, evaluated. Only `wc` is
    /// non-zero on `[9.5, 12.5]` and its coefficient is `0.0`, so the loop has an exact rest band
    /// rather than a fixed point it hunts around.
    #[test]
    fn the_curve_has_an_exact_rest_band_between_nine_point_five_and_twelve_point_five() {
        let g = DegradeGovernor::automatic(FramerateTargets::default());
        for f in [9.5f32, 10.0, 11.0, 12.0, 12.5] {
            assert_eq!(g.candidate(f), 0.0, "fps={f} must be neutral");
        }
        assert!(g.candidate(9.0) < 0.0, "below the band the bias falls");
        assert!(g.candidate(20.0) > 0.0, "above it the bias rises");
    }

    /// Oracle: the history shift runs before `auto_update_deg_mul` is tested, and scans exactly 29
    /// entries.
    ///
    /// The consequence a rebuild gets wrong by treating `degmulhist` as a log of accepted changes:
    /// the client **forgets** a bias thirty frames after it last held it, so a loop that walks away
    /// from a value and comes back is not blocked by the dead-band for ever.
    #[test]
    fn the_history_is_a_sliding_thirty_frame_window_and_not_a_log_of_changes() {
        let mut g = DegradeGovernor::automatic(FramerateTargets::default());
        // Drive it up to +1 on a fast frame rate...
        for _ in 0..40 {
            g.use_time(1_000.0);
        }
        assert!((g.deg_mul - 1.0).abs() < 1e-6);
        // ..then hold it at the neutral rate for a whole window. The history fills with +1.
        for _ in 0..DEG_MUL_HISTORY {
            g.use_time(g.targets.ideal);
        }
        assert!(
            (g.deg_mul - 1.0).abs() < 1e-6,
            "the neutral band moves nothing"
        );
        // Now a slow machine. Every step is 0.15 wide, so nothing is within 0.01 of a stale entry
        // and the bias walks all the way back down: the values it passed through on the way up are
        // long gone from the window.
        for _ in 0..40 {
            g.use_time(1.0);
        }
        assert!(
            (g.deg_mul + 1.0).abs() < 1e-6,
            "it came back down to -1: {}",
            g.deg_mul
        );
    }

    /// Oracle: the recovered frame-composition table.
    #[test]
    fn set_degrade_level_internal_reproduces_its_table() {
        let neutral = DegradeLevel::new(0.0);
        assert_eq!(
            (neutral.max_static_lights, neutral.max_dynamic_lights),
            (40, 7)
        );
        assert!((neutral.object_distance_2dsq - 625.0).abs() < 1e-3);
        assert!((neutral.particle_distance_2dsq - 256.0).abs() < 1e-3);

        let fast = DegradeLevel::new(1.0);
        assert_eq!((fast.max_static_lights, fast.max_dynamic_lights), (60, 9));
        assert!((fast.object_distance_2dsq - 42.0 * 42.0).abs() < 1e-2);
        assert!((fast.particle_distance_2dsq - 24.0 * 24.0).abs() < 1e-2);

        let slow = DegradeLevel::new(-1.0);
        assert_eq!((slow.max_static_lights, slow.max_dynamic_lights), (20, 4));
        assert!((slow.object_distance_2dsq - 0.0).abs() < 1e-3);
        assert!((slow.particle_distance_2dsq - 49.0).abs() < 1e-3);

        assert_eq!(
            DegradeLevel::default().deg_mul,
            dereth_terrain::consts::PINNED_DEG_MUL
        );
    }

    /// Behaviour: rendering.degrade.the-share-distances-start-at-five-and-four-metres
    #[test]
    fn the_share_distances_start_at_five_and_four_metres_until_the_loop_first_moves_the_bias() {
        // A client with automatic degrades off never runs the setter.
        let mut pinned = DegradeGovernor::pinned(0.0);
        for fps in [1.0f32, 10.0, 1_000.0] {
            pinned.use_time(fps);
        }
        let l = pinned.level();
        assert_eq!(l.object_distance_2dsq, 25.0, "5 m, not the setter's 25 m");
        assert_eq!(l.particle_distance_2dsq, 16.0, "4 m, not the setter's 16 m");
        assert_eq!((l.max_static_lights, l.max_dynamic_lights), (40, 7));

        // The live loop starts there too, and a neutral frame rate never runs the setter.
        let mut g = DegradeGovernor::automatic(FramerateTargets::default());
        assert_eq!(g.level(), DegradeLevel::startup());
        g.use_time(g.targets.ideal);
        assert_eq!(
            g.level(),
            DegradeLevel::startup(),
            "a suppressed change sets nothing"
        );

        // The first accepted change sets all four outputs from the new bias.
        g.use_time(1_000.0);
        assert!(g.deg_mul > 0.0);
        assert_eq!(g.level(), DegradeLevel::new(g.deg_mul));
        assert!(g.level().object_distance_2dsq > 625.0);
    }

    /// A particle emitter object shares at the particle distance, every other object at the
    /// object distance.
    #[test]
    fn a_particle_emitter_shares_at_the_particle_distance_and_every_other_object_at_the_object_distance(
    ) {
        let l = DegradeLevel::new(1.0);
        assert_eq!(l.share_distance_2dsq(true), l.particle_distance_2dsq);
        assert_eq!(l.share_distance_2dsq(false), l.object_distance_2dsq);
        assert_ne!(l.particle_distance_2dsq, l.object_distance_2dsq);
    }
}
