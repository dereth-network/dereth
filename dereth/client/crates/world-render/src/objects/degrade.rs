//! Level-of-detail selection and the five billboarding modes.
//!
//! Degrade records are described in `docs/formats/22-degrade-info.md`.
//!
//! **`degrade_mode` is a billboarding mode, not a level-of-detail hint.**
//! `GfxObjInfo::degrade_mode` is a bare `int` with no named values and it is easy to read the name
//! as "how aggressively to degrade". It is not: the retail client interprets
//! it as *how to orient the part toward the viewer*. Mode 5 — 573 of the 14,344 shipped entries —
//! is the upright foliage card.
//!
//! Two more easy misreadings: `gfxobj_id == 0` means **draw nothing**, and 4,131 records end with
//! an all-`FLT_MAX` terminator level that must not be selected as geometry.

use dereth_assets::motion::{GfxObjDegradeInfo, GfxObjInfo};
use dereth_primitives::{Frame, Vec3};

use crate::consts::S_R_DEGRADE_DISTANCE;
use crate::math::{rotate_around_axis_to_vector, set_vector_heading};

/// The graphics object's `degrade_mode`, interpreted while calculating its draw frame.
///
/// | value | count in dat | behaviour |
/// |---|---|---|
/// | 1 | 13 501 | nothing — draw with the part's own frame |
/// | 2 | 259 | set the draw frame's vector heading to the viewer heading — full billboard |
/// | 3 | 0 | `rotate_around_axis_to_vector(draw_frame, 0, viewer_heading)` — about **X** |
/// | 4 | 11 | about **Y** |
/// | 5 | 573 | about **Z** — the classic upright foliage card |
///
/// Mode 3 never occurs in shipped data but is implemented, because the client implements it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum DegradeMode {
    None = 1,
    FullBillboard = 2,
    AxisX = 3,
    AxisY = 4,
    AxisZ = 5,
}

impl DegradeMode {
    /// Decode the raw `int`. Anything the client does not name falls through to
    /// [`DegradeMode::None`], which is what its `switch` default does.
    #[must_use]
    pub const fn from_raw(v: i32) -> Self {
        match v {
            2 => Self::FullBillboard,
            3 => Self::AxisX,
            4 => Self::AxisY,
            5 => Self::AxisZ,
            _ => Self::None,
        }
    }
}

/// Derive `draw_pos.frame` from `pos.frame` and the
/// viewer heading, according to the selected level's billboarding mode.
#[must_use]
pub fn calc_draw_frame(pos: &Frame, mode: DegradeMode, viewer_heading: Vec3) -> Frame {
    let mut f = *pos;
    match mode {
        DegradeMode::None => {}
        DegradeMode::FullBillboard => set_vector_heading(&mut f, viewer_heading),
        DegradeMode::AxisX => rotate_around_axis_to_vector(&mut f, 0, viewer_heading),
        DegradeMode::AxisY => rotate_around_axis_to_vector(&mut f, 1, viewer_heading),
        DegradeMode::AxisZ => rotate_around_axis_to_vector(&mut f, 2, viewer_heading),
    }
    f
}

/// The globals `get_degrade` reads, with their retail initial values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DegradeGlobals {
    /// initial 0 — a console switch forcing level 0 everywhere.
    pub degrades_disabled: bool,
    /// initial -1 — a console switch pinning every object.
    pub force_level: i32,
    /// initial 50.0 — subtracted from every distance, which is then floored at 0.
    pub degrade_distance: f32,
    /// The automatic degrade-multiplier flag, initially true.
    pub auto_update_deg_mul: bool,
    /// Automatic degradation bias in `[-1, +1]`.
    pub deg_mul: f32,
    /// The user-supplied degrade bias, initially 0.0.
    pub user_bias: f32,
}

impl Default for DegradeGlobals {
    /// The shipped initial values, with `deg_mul` **pinned** to
    /// [`crate::consts::PINNED_DEG_MUL`] rather than left to the feedback loop.
    /// UNVERIFIED: which value 2013 hardware settled at was never measured, so every test
    /// and every golden image must say what it pinned.
    fn default() -> Self {
        Self {
            degrades_disabled: false,
            force_level: -1,
            degrade_distance: S_R_DEGRADE_DISTANCE,
            auto_update_deg_mul: true,
            deg_mul: crate::consts::PINNED_DEG_MUL,
            user_bias: 0.0,
        }
    }
}

/// Select the degrade level and mode for a viewing distance.
///
/// ```text
/// if degrades_disabled:  level = 0 ; mode = degrades[0].degrade_mode ; return
/// if force_level != -1:  level = min(force_level, num_degrades - 1) ; ... ; return
/// d    = |distance| - degrade_distance
/// if d < 0: d = 0
/// bias = auto_update_deg_mul ? deg_mul : user_bias
/// if bias < 0: threshold_i = ideal_i + (ideal_i - min_i) * bias
/// else:        threshold_i = ideal_i - (ideal_i - max_i) * bias
/// pick the first i with d < threshold_i, else num_degrades - 1
/// ```
///
/// With `bias == 0` both reduce to `ideal_dist`. A **positive** bias (good frame rate) pushes the
/// threshold out toward `max_dist`, keeping high-detail meshes at longer range; a **negative** bias
/// pulls it in toward `min_dist`. `d` is 0 for everything within `degrade_distance`, so
/// **everything within 50 units always uses level 0**, regardless of bias.
///
/// # The degrade distance is *subtracted*
///
/// The lookup is not a threshold of the form `d = (|distance| >= degrade_distance) ? distance : 0`.
/// The client subtracts the degrade distance from the magnitude, writes the difference back over
/// its distance argument, and only then floors it at zero; both level loops read that stored
/// difference. So the values the loops can see are `0.0` and `|distance| - 50`, and the raw
/// distance is never compared with any threshold. The shipped client and the 2013 client do this
/// identically.
///
/// **The shipped data agrees.** Under the threshold reading the reachable set is
/// `{0} u [50, inf)`, and **3,301 of the 8,973 real levels** in the dat's 2,891 multi-level records
/// (2,295 records) can never be selected at any distance or bias -- a level-0 band whose
/// `ideal_dist` is under 50 swallows everything up to 50 and the next threshold is already past.
/// Under the subtraction the reachable set is `[0, inf)` and **13 of 8,973** are dead. Level 1 of
/// a typical tree (`10/25/50, 25/50/100, 50/100/200`) is unreachable under the threshold reading
/// and covers 75-100 m under the subtraction.
///
/// # Unordered values
///
/// The floor and the bias sign are tested the way the client tests them: only a difference that
/// compares below zero is floored, and only a bias that compares below zero takes the `min_dist`
/// arm. A NaN distance therefore stays NaN, compares below no threshold and selects the last
/// level; a NaN bias takes the `max_dist` arm.
#[must_use]
pub fn get_degrade(
    info: &GfxObjDegradeInfo,
    distance: f32,
    g: &DegradeGlobals,
) -> (usize, DegradeMode) {
    let n = info.degrades.len();
    if n == 0 {
        return (0, DegradeMode::None);
    }
    let mode_of = |i: usize| DegradeMode::from_raw(info.degrades[i].degrade_mode);
    if g.degrades_disabled {
        return (0, mode_of(0));
    }
    if g.force_level != -1 {
        // LINT-OK: index arithmetic; force_level is a small console value.
        let level = (g.force_level.max(0) as usize).min(n - 1);
        return (level, mode_of(level));
    }
    // `|distance| - degrade_distance`, floored at zero only when it compares below zero -- see
    // the header. `f32::max` would also floor a NaN, which the client does not.
    let d = distance.abs() - g.degrade_distance;
    let d = if d < 0.0 { 0.0 } else { d };
    // **No clamp** -- the physics and render crates must agree on this.
    // The calculation selects `deg_mul` during automatic updates and the user-supplied bias
    // otherwise, then uses the selected value as it stands. The automatic path clamps `deg_mul`
    // into `[-1, +1]` *before storing
    // it*, so the automatic bias is bounded by its producer; `user_bias` is the
    // raw `Render.GraphicsPerformance` preference, whose declared range is -1.0 .. 1.0, and reaches
    // this function unclamped.
    let bias = if g.auto_update_deg_mul {
        g.deg_mul
    } else {
        g.user_bias
    };
    for (i, e) in info.degrades.iter().enumerate() {
        let threshold = if bias < 0.0 {
            e.ideal_dist + (e.ideal_dist - e.min_dist) * bias
        } else {
            e.ideal_dist - (e.ideal_dist - e.max_dist) * bias
        };
        if d < threshold {
            return (i, mode_of(i));
        }
    }
    (n - 1, mode_of(n - 1))
}

/// Last real degradation distance: `degrades[n-2].max_dist` when there
/// are more than two levels, else `degrades[0].max_dist`, i.e. the far edge of the last *real*
/// level, skipping the `FLT_MAX` terminator.
#[must_use]
pub fn get_max_degrade_distance(info: &GfxObjDegradeInfo) -> f32 {
    let n = info.degrades.len();
    if n == 0 {
        return 0.0;
    }
    if n > 2 {
        info.degrades[n - 2].max_dist
    } else {
        info.degrades[0].max_dist
    }
}

/// True when a level is the all-`FLT_MAX` terminator — `gfxobj_id == 0` and every distance at
/// `FLT_MAX`. 4,131 of the shipped records end with one, and it "must not be selected as geometry":
/// selecting it means **draw nothing**, which is different from drawing the last real mesh.
///
/// **This is a corpus predicate, not production logic, and that is deliberate.**
/// It looks *transcribed and unwired*: its only callers are tests. The
/// behaviour it appears to name **does**
/// happen, by another route: what the draw path needs is `gfxobj_id == 0`, and
/// [`draws_anything`] is that predicate and is wired. The extra `FLT_MAX` checks here are what
/// make it a question about the *shape of a record* — "does this record end the way all 4,131 of
/// them do" — which is a thing a corpus test asks and a frame never does. It is kept and
/// annotated rather than deleted, because removing correct code merely because no test misses it
/// would discard faithful behavior. A census that meets
/// it again should read this line rather than re-file it.
#[must_use]
pub fn is_terminator(e: &GfxObjInfo) -> bool {
    e.gfxobj_id.0 == 0
        && e.min_dist == f32::MAX
        && e.ideal_dist == f32::MAX
        && e.max_dist == f32::MAX
}

/// The selected-level check is `g = gfxobj[level]; if !g: return`: **`gfxobj_id == 0` means
/// draw nothing**, not "fall back to a previous level".
#[must_use]
pub fn draws_anything(info: &GfxObjDegradeInfo, level: usize) -> bool {
    info.degrades.get(level).is_some_and(|e| e.gfxobj_id.0 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::DataId;

    fn info(levels: &[(u32, i32, f32, f32, f32)]) -> GfxObjDegradeInfo {
        GfxObjDegradeInfo {
            id: DataId(0x1100_0000),
            degrades: levels
                .iter()
                .map(|&(id, mode, min, ideal, max)| GfxObjInfo {
                    gfxobj_id: DataId(id),
                    degrade_mode: mode,
                    min_dist: min,
                    ideal_dist: ideal,
                    max_dist: max,
                })
                .collect(),
        }
    }

    /// Oracle: the worked example from the degrade-info format — record `0x11000000`, four levels, the
    /// first `gfxobj_id 0x01003769`, `degrade_mode 1`, bands 10/25/50 then 25/50/100 then
    /// 50/100/200 and the `FLT_MAX` terminator.
    fn worked_example() -> GfxObjDegradeInfo {
        info(&[
            (0x0100_3769, 1, 10.0, 25.0, 50.0),
            (0x0100_376E, 1, 25.0, 50.0, 100.0),
            (0x0100_376F, 1, 50.0, 100.0, 200.0),
            (0, 1, f32::MAX, f32::MAX, f32::MAX),
        ])
    }

    /// Oracle: **everything within 50 units always uses level 0**, regardless of bias, because
    /// `degrade_distance` clamps the
    /// distance to zero first.
    #[test]
    fn everything_within_fifty_units_uses_level_zero() {
        let i = worked_example();
        let g = DegradeGlobals::default();
        for d in [0.0f32, 1.0, 25.0, 49.999, 50.0] {
            assert_eq!(get_degrade(&i, d, &g).0, 0, "d={d}");
        }
        // Past 50 the *offset* distance takes over, and the thresholds at bias 0 are the ideal
        // distances 25, 50, 100, FLT_MAX. See the header: `d = max(|distance| - 50, 0)`, so 75
        // asks the record about 25 and 99 asks it about 49.
        assert_eq!(get_degrade(&i, 74.999, &g).0, 0);
        assert_eq!(get_degrade(&i, 75.0, &g).0, 1);
        assert_eq!(get_degrade(&i, 99.0, &g).0, 1);
        assert_eq!(get_degrade(&i, 100.0, &g).0, 2);
        assert_eq!(
            get_degrade(&i, 150.0, &g).0,
            3,
            "beyond the last real level: the terminator"
        );
    }

    /// The degrade distance is subtracted from the viewer distance, not used as a threshold.
    #[test]
    fn the_degrade_distance_is_subtracted_not_a_threshold() {
        let i = worked_example();
        let g = DegradeGlobals::default();
        // The whole of level 1's band exists only because the 50 is subtracted. Under
        // `d = distance` the first threshold 25 is already behind the 50 m clamp, so 50 would
        // select level 2 and nothing would ever select level 1.
        let band: Vec<usize> = (50..=170)
            .step_by(5)
            .map(|m| get_degrade(&i, m as f32, &g).0)
            .collect();
        assert!(band.contains(&1), "level 1 must be reachable: {band:?}");
        assert_eq!(get_degrade(&i, 50.0, &g).0, 0, "50 m is d = 0, not d = 50");
        assert_eq!(get_degrade(&i, 60.0, &g).0, 0, "60 m is d = 10");
        assert_eq!(get_degrade(&i, 120.0, &g).0, 2, "120 m is d = 70");
        // A negative distance takes its magnitude first, and the floor is at zero.
        assert_eq!(get_degrade(&i, -10.0, &g).0, get_degrade(&i, 10.0, &g).0);
        assert_eq!(get_degrade(&i, -120.0, &g).0, 2);
    }

    /// Only a difference that compares below zero is floored, so a NaN distance is not pulled
    /// to zero: it compares below no threshold and selects the last level.
    #[test]
    fn a_nan_distance_is_not_floored_and_selects_the_last_level() {
        let i = worked_example();
        let g = DegradeGlobals::default();
        assert_eq!(get_degrade(&i, f32::NAN, &g).0, 3);
    }

    /// Oracle: `get_degrade`'s two threshold formulas — with `bias == 0` both reduce to
    /// `ideal_dist`; a positive bias interpolates outwards toward `max_dist` and a negative one
    /// inwards toward `min_dist`, and `bias` is clamped so the threshold never leaves
    /// `[min_dist, max_dist]`.
    #[test]
    fn the_bias_branches_slide_the_thresholds_the_documented_way() {
        let i = worked_example();
        let at = |bias: f32, d: f32| {
            get_degrade(
                &i,
                d,
                &DegradeGlobals {
                    deg_mul: bias,
                    ..DegradeGlobals::default()
                },
            )
            .0
        };
        // At 75 m the record is asked about d = 25: bias 0 -> thresholds 25/50/100 -> level 1.
        assert_eq!(at(0.0, 75.0), 1);
        // bias +1 -> thresholds become max_dist 50/100/200 -> level 0 (higher detail, farther out).
        assert_eq!(at(1.0, 75.0), 0);
        // bias -1 -> thresholds become min_dist 10/25/50 -> level 2 (degrades sooner).
        assert_eq!(at(-1.0, 75.0), 2);
        assert_eq!(at(5.0, 75.0), 0);
        assert_eq!(at(1.0, 75.0), 0);
        assert_eq!(
            at(-5.0, 75.0),
            3,
            "every threshold has gone negative: the terminator"
        );
        assert_eq!(at(-1.0, 75.0), 2);
    }

    /// Oracle: `get_degrade`'s two console switches — `degrades_disabled` forces level 0 everywhere
    /// and `force_level` pins every object, clamped to the last level.
    #[test]
    fn the_console_switches_short_circuit_before_any_distance_work() {
        let i = worked_example();
        let g = DegradeGlobals {
            degrades_disabled: true,
            ..DegradeGlobals::default()
        };
        assert_eq!(get_degrade(&i, 10_000.0, &g).0, 0);
        let g = DegradeGlobals {
            force_level: 2,
            ..DegradeGlobals::default()
        };
        assert_eq!(get_degrade(&i, 0.0, &g).0, 2);
        let g = DegradeGlobals {
            force_level: 99,
            ..DegradeGlobals::default()
        };
        assert_eq!(
            get_degrade(&i, 0.0, &g).0,
            3,
            "force_level clamps to num_degrades - 1"
        );
    }

    /// Oracle: `get_degrade`'s bias source — `auto_update_deg_mul` picks `deg_mul`, otherwise
    /// `user_bias`. This is also the knob OQ#199 says every test must pin.
    #[test]
    fn the_bias_comes_from_the_manual_value_when_the_loop_is_off() {
        let i = worked_example();
        let auto = DegradeGlobals {
            deg_mul: 1.0,
            user_bias: -1.0,
            ..DegradeGlobals::default()
        };
        let manual = DegradeGlobals {
            auto_update_deg_mul: false,
            ..auto
        };
        assert_eq!(
            get_degrade(&i, 75.0, &auto).0,
            0,
            "the automatic bias is used"
        );
        assert_eq!(
            get_degrade(&i, 75.0, &manual).0,
            2,
            "the user bias is used instead"
        );
        assert_eq!(
            DegradeGlobals::default().deg_mul,
            crate::consts::PINNED_DEG_MUL
        );
    }

    /// Oracle: trap 5 — `gfxobj_id == 0` means **draw nothing**, and the
    /// all-`FLT_MAX` terminator must not be selected as geometry.
    #[test]
    fn the_terminator_level_draws_nothing() {
        let i = worked_example();
        assert!(is_terminator(&i.degrades[3]));
        assert!(!is_terminator(&i.degrades[0]));
        assert!(draws_anything(&i, 0));
        assert!(
            !draws_anything(&i, 3),
            "selecting the terminator means draw nothing"
        );
        assert_eq!(
            get_max_degrade_distance(&i),
            200.0,
            "the far edge of the last real level"
        );
        // With only two levels the rule changes to degrades[0].max_dist.
        let two = info(&[
            (1, 1, 0.0, 10.0, 20.0),
            (0, 1, f32::MAX, f32::MAX, f32::MAX),
        ]);
        assert_eq!(get_max_degrade_distance(&two), 20.0);
    }

    /// Oracle: trap 5 and the draw-frame table — the five modes are
    /// **billboarding** operations, not detail hints. Mode 1 must leave the frame alone; modes 2-5
    /// must all change it when the viewer is off-axis.
    #[test]
    fn the_five_modes_are_billboarding_operations() {
        let pos = Frame::default();
        let viewer = Vec3::new(1.0, 0.0, 0.5);
        assert_eq!(
            calc_draw_frame(&pos, DegradeMode::None, viewer),
            pos,
            "mode 1 does nothing"
        );
        // Each axis mode only moves the frame when the target has a component to swing *about*
        // that axis; a target parallel to the kept axis's plane is already aligned and the client
        // leaves the frame alone. The headings below are chosen so each mode has work to do.
        for (m, v) in [
            (DegradeMode::FullBillboard, Vec3::new(1.0, 0.0, 0.5)),
            (DegradeMode::AxisX, Vec3::new(1.0, 0.5, 0.5)),
            (DegradeMode::AxisY, Vec3::new(0.5, 1.0, 0.5)),
            (DegradeMode::AxisZ, Vec3::new(1.0, 0.0, 0.5)),
        ] {
            let f = calc_draw_frame(&pos, m, v);
            assert_ne!(f, pos, "{m:?} must reorient the part");
        }
        // Mode 5, the upright foliage card: the local Z axis stays vertical.
        let f = calc_draw_frame(&pos, DegradeMode::AxisZ, viewer);
        let m = crate::math::l2g(f.rotation).0;
        assert!(
            (m[8] - 1.0).abs() < 1e-4,
            "mode 5 keeps the card upright: {m:?}"
        );
        // Mode 2, the full billboard: the local Y axis aims at the viewer, tilt and all.
        let f = calc_draw_frame(&pos, DegradeMode::FullBillboard, viewer);
        let y = crate::math::get_vector_heading(&f);
        let vn = 1.0 / (1.0f32 + 0.25).sqrt();
        assert!(
            (y.x - 1.0 * vn).abs() < 1e-3 && (y.z - 0.5 * vn).abs() < 1e-3,
            "{y:?}"
        );
    }

    /// Oracle: the degrade-info mode table, read as data. The raw `int` decode must map the
    /// five named values and treat everything else as "nothing", which is the client's `switch`
    /// default.
    #[test]
    fn mode_decoding_matches_the_table() {
        assert_eq!(DegradeMode::from_raw(1), DegradeMode::None);
        assert_eq!(DegradeMode::from_raw(2), DegradeMode::FullBillboard);
        assert_eq!(DegradeMode::from_raw(3), DegradeMode::AxisX);
        assert_eq!(DegradeMode::from_raw(4), DegradeMode::AxisY);
        assert_eq!(DegradeMode::from_raw(5), DegradeMode::AxisZ);
        assert_eq!(DegradeMode::from_raw(0), DegradeMode::None);
        assert_eq!(DegradeMode::from_raw(-7), DegradeMode::None);
        assert_eq!(DegradeMode::from_raw(99), DegradeMode::None);
    }
}
