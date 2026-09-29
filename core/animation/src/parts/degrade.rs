//! Level-of-detail selection from `GfxObjDegradeInfo`.
//!
//! The renderer's globals arrive as [`DegradeSettings`] rather than as statics, because this crate
//! must not reach into the client — and because a test needs to sweep them.

use crate::data::DegradeInfo;

/// The renderer settings used by degradation selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DegradeSettings {
    /// Pin every part at level 0.
    pub degrades_disabled: bool,
    /// Forced level, or `-1` for "not forced".
    pub force_level: i32,
    /// initial **50.0** — *subtracted* from the distance
    /// and floored at zero, so everything inside this radius asks the record at `d = 0`.
    pub degrade_distance: f32,
    /// Automatic degradation uses this multiplier when `auto_update_deg_mul` is set; otherwise it
    /// uses the caller's fixed multiplier. A negative value selects the second branch below.
    ///
    /// **Not clamped here, because the client does not clamp it at this point.**
    /// Automatic multiplier updates clamp `deg_mul` to `[-1, +1]` before storing it, but
    /// the user-supplied degrade bias is the raw graphics-performance user preference and
    /// reaches this function unclamped.
    pub bias: f32,
}

impl Default for DegradeSettings {
    /// The shipped initial values are `degrades_disabled = 0`, `force_level = -1`,
    /// `degrade_distance = 50.0` and `deg_mul = 0.0`.
    fn default() -> Self {
        Self {
            degrades_disabled: false,
            force_level: -1,
            degrade_distance: S_R_DEGRADE_DISTANCE,
            bias: 0.0,
        }
    }
}

/// The retail initial value.
pub const S_R_DEGRADE_DISTANCE: f32 = 50.0;

/// The maximum degrade distance of a graphics object.
///
/// Note the `num_degrades > 2` test picks the **second to last** level's `max_dist`, not the last:
/// the last level's `max_dist` is `FLT_MAX`, a terminator sentinel.
#[must_use]
pub fn get_max_degrade_distance(d: &DegradeInfo) -> f32 {
    let n = d.degrades.len();
    if n == 0 {
        return 100.0;
    }
    if n > 2 {
        d.degrades[n - 2].max_dist
    } else {
        d.degrades[0].max_dist
    }
}

/// The degrade decision -- returns `(level, mode)`.
///
/// # The two arms walk the level array through **different** fields
///
/// This is the whole reason the two comparisons look gratuitously different. The level array is
/// 20-byte `GfxObjInfo` records (`gfxobj_id` 0, `degrade_mode` 4, `min_dist` 8, `ideal_dist` 12,
/// `max_dist` 16) walked by position rather than by field, so the two arms read different pairs:
/// `ideal_dist` and `max_dist` in the first, **`min_dist`** and **`ideal_dist`** in the second:
///
/// * `bias >= 0`: `d < ideal − (ideal − max) · bias`
/// * `bias < 0`:  `d < (ideal − min) · bias + ideal`
///
/// Both reduce to `d < ideal` at `bias == 0`, which is why a wrong negative arm is easy to miss: `d
/// < (max − ideal) · bias + max` — the *positive* arm's pair of fields, plus `max` where the client
/// adds `ideal` — agrees with retail at zero bias and nowhere else. A positive bias pushes the
/// threshold out toward `max_dist` (keep the detailed mesh further away); a negative bias pulls it
/// in toward `min_dist` (degrade sooner), and drives `deg_mul` negative whenever the frame rate
/// falls below `min_framerate * 0.75`.
///
/// # `d` is a subtraction, not a threshold
///
/// The clamp is `d = |distance| − degrade_distance`, floored at zero when it compares below zero,
/// not `(|distance| >= degrade_distance) ? distance : 0.0`: the client writes the difference back
/// over its distance argument before the floor, and both level loops read that stored value, never
/// the raw distance. The shipped data agrees (3,301 of 8,973 real levels unreachable under the
/// threshold reading against 13 under this one); `dereth_world_render::objects::degrade` is the
/// same lookup.
#[must_use]
pub fn get_degrade(d: &DegradeInfo, dist: f32, s: DegradeSettings) -> (u32, i32) {
    if d.degrades.is_empty() {
        return (0, 1);
    }
    let last = d.degrades.len() - 1;
    let level = |i: usize| (u32::try_from(i).unwrap_or(0), d.degrades[i].degrade_mode);

    if s.degrades_disabled {
        return level(0);
    }
    if s.force_level != -1 {
        let i = usize::try_from(s.force_level).unwrap_or(0).min(last);
        return level(i);
    }
    // Floored only when the difference compares below zero: `f32::max` would also floor a NaN,
    // which the client does not.
    let dd = dist.abs() - s.degrade_distance;
    let dd = if dd < 0.0 { 0.0 } else { dd };
    // The client takes the `min_dist` arm only for a bias that compares below zero, so a NaN bias
    // reads the `max_dist` arm. Moving the boundary to `bias <= 0.0` turns **no test red**: at
    // `bias == 0` both thresholds collapse to `ideal_dist`, so which arm runs is unobservable. The
    // boundary is kept as the client writes it and is recorded here as unfalsifiable rather than
    // deleted — see `the_two_arms_agree_in_the_limit_at_zero_bias`.
    if s.bias < 0.0 {
        // The client's field pointer sits on `ideal_dist`; the field before it is `min_dist`.
        for (i, g) in d.degrades.iter().enumerate() {
            if dd < (g.ideal_dist - g.min_dist) * s.bias + g.ideal_dist {
                return level(i);
            }
        }
    } else {
        // The client's field pointer sits on `max_dist`; the field before it is `ideal_dist`.
        for (i, g) in d.degrades.iter().enumerate() {
            if dd < g.ideal_dist - (g.ideal_dist - g.max_dist) * s.bias {
                return level(i);
            }
        }
    }
    level(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GfxObjInfo;
    use dereth_primitives::DataId;

    fn info(levels: &[(f32, f32, i32)]) -> DegradeInfo {
        DegradeInfo {
            degrades: levels
                .iter()
                .map(|(ideal, max, mode)| GfxObjInfo {
                    gfxobj_id: DataId(0x0100_0001),
                    degrade_mode: *mode,
                    min_dist: 0.0,
                    ideal_dist: *ideal,
                    max_dist: *max,
                })
                .collect(),
        }
    }

    fn triple(levels: &[(f32, f32, f32, i32)]) -> DegradeInfo {
        DegradeInfo {
            degrades: levels
                .iter()
                .map(|(min, ideal, max, mode)| GfxObjInfo {
                    gfxobj_id: DataId(0x0100_0001),
                    degrade_mode: *mode,
                    min_dist: *min,
                    ideal_dist: *ideal,
                    max_dist: *max,
                })
                .collect(),
        }
    }

    /// The distance the loops actually compare against, with the degrade radius
    /// (`degrade_distance`) taken out of the way, so that a case about level selection is not also
    /// a case about the clamp.
    const NO_RADIUS: DegradeSettings = DegradeSettings {
        degrades_disabled: false,
        force_level: -1,
        degrade_distance: 0.0,
        bias: 0.0,
    };

    /// ORACLE: the recovered part-array behavior,
    /// the degrade decision and the maximum degrade distance.
    #[test]
    fn a_zero_bias_selects_the_first_level_whose_ideal_distance_exceeds_the_distance() {
        let d = info(&[(10.0, 20.0, 1), (30.0, 40.0, 1), (60.0, f32::MAX, 5)]);
        let s = NO_RADIUS;
        assert_eq!(get_degrade(&d, 5.0, s), (0, 1));
        assert_eq!(get_degrade(&d, 15.0, s), (1, 1));
        assert_eq!(get_degrade(&d, 45.0, s), (2, 5));
        assert_eq!(
            get_degrade(&d, 1e9, s),
            (2, 5),
            "past every level: the last one"
        );
    }

    /// The degrade distance is subtracted rather than thresholded.
    #[test]
    fn the_degrade_distance_is_subtracted_rather_than_thresholded() {
        let d = info(&[(10.0, 20.0, 1), (30.0, 40.0, 1), (60.0, f32::MAX, 5)]);
        let s = DegradeSettings::default();
        assert_eq!(
            s.degrade_distance, 50.0,
            "the renderer's own degrade distance"
        );
        for dist in [0.0, 5.0, 15.0, 45.0, 50.0] {
            assert_eq!(
                get_degrade(&d, dist, s),
                (0, 1),
                "d = max({dist} - 50, 0) = 0"
            );
        }
        assert_eq!(get_degrade(&d, 65.0, s), (1, 1), "d = 15");
        assert_eq!(get_degrade(&d, 105.0, s), (2, 5), "d = 55");
        // Under the threshold reading (`d = dist` once `|dist| >= 50`) 65 m would have answered
        // level 2, because 65 is past both of the first two thresholds.
        assert_eq!(
            get_degrade(&d, 65.0, NO_RADIUS),
            (2, 5),
            "which is the reading it is not"
        );
    }

    /// ORACLE: 's two pointer walks, transcribed from the field offsets
    /// rather than from the code under test. `GfxObjInfo` is `gfxobj_id` +0, `degrade_mode` +4,
    /// `min_dist` +8, `ideal_dist` +12, `max_dist` +16.
    ///
    /// * `bias >= 0` starts at `&degrades->max_dist`, so its threshold is
    ///   `ideal - (ideal - max) * bias`.
    /// * `bias < 0` starts at `&degrades->ideal_dist`, so its threshold is
    ///   `(ideal - min) * bias + ideal` — **`min`**, not `max`, and it adds `ideal`, not `max`.
    ///
    /// Asserted at **interior** biases, not only at the ends: the wrong pair agrees with the right
    /// one at `bias == 0` (both are `ideal`), which is exactly why this survived.
    #[test]
    fn the_negative_bias_arm_reads_min_and_ideal_not_ideal_and_max() {
        let d = triple(&[(10.0, 40.0, 100.0, 1)]);
        for (bias, threshold) in [
            (-0.25_f32, 32.5_f32),
            (-0.5, 25.0),
            (-0.75, 17.5),
            (-1.0, 10.0),
        ] {
            let s = DegradeSettings {
                degrade_distance: 0.0,
                bias,
                ..DegradeSettings::default()
            };
            // Just inside the correct threshold: level 0.
            assert_eq!(get_degrade(&d, threshold - 0.1, s), (0, 1), "bias {bias}");
            // Just outside it: past every level, so the last one.
            assert_eq!(
                get_degrade(&d, threshold + 0.1, s),
                (0, 1),
                "one level: still 0"
            );
            // The discriminating point: between the correct threshold and the wrong one
            // (`100 + 60 * bias`), where the two readings disagree about whether the level is
            // still selected. With a second level below, the answer differs.
            let two = triple(&[(10.0, 40.0, 100.0, 1), (200.0, 300.0, 400.0, 5)]);
            let wrong = 100.0 + 60.0 * bias;
            let probe = f32::midpoint(threshold, wrong);
            assert!(
                probe > threshold && probe < wrong,
                "bias {bias}: probe {probe} is between"
            );
            assert_eq!(
                get_degrade(&two, probe, s),
                (1, 5),
                "bias {bias}: {probe} is past the correct threshold {threshold}; the pre-\
                 reading kept level 0 out to {wrong}"
            );
        }
    }

    /// At `bias == 0` the two arms are the same expression — both thresholds collapse to
    /// `ideal_dist` — so the defect was invisible at the shipped default, which is why it survived
    /// from the day it was written. Asserted at a bias one ULP below zero (`-0.0` takes the
    /// *positive* arm under IEEE `-0.0 >= 0.0`, so it would not exercise the branch at all).
    #[test]
    fn the_two_arms_agree_in_the_limit_at_zero_bias() {
        let d = triple(&[(10.0, 40.0, 100.0, 1), (200.0, 300.0, 400.0, 5)]);
        let plus = DegradeSettings {
            degrade_distance: 0.0,
            bias: 0.0,
            ..Default::default()
        };
        let minus = DegradeSettings {
            degrade_distance: 0.0,
            bias: -f32::MIN_POSITIVE,
            ..Default::default()
        };
        assert!(minus.bias < 0.0, "the negative arm is the one taken");
        for dist in [0.0_f32, 5.0, 39.9, 40.1, 100.0, 299.9, 300.1, 1e6] {
            assert_eq!(
                get_degrade(&d, dist, plus),
                get_degrade(&d, dist, minus),
                "at {dist}"
            );
        }
    }

    /// `degrades_disabled` and `force_level` short-circuit, and `force_level` clamps.
    #[test]
    fn the_render_overrides_short_circuit() {
        let d = info(&[(10.0, 20.0, 1), (30.0, 40.0, 2), (60.0, f32::MAX, 5)]);
        let s = DegradeSettings {
            degrades_disabled: true,
            ..DegradeSettings::default()
        };
        assert_eq!(get_degrade(&d, 1e9, s), (0, 1));
        let s = DegradeSettings {
            force_level: 1,
            ..DegradeSettings::default()
        };
        assert_eq!(get_degrade(&d, 0.0, s), (1, 2));
        let s = DegradeSettings {
            force_level: 99,
            ..DegradeSettings::default()
        };
        assert_eq!(
            get_degrade(&d, 0.0, s),
            (2, 5),
            "clamped to num_degrades - 1"
        );
    }

    /// Inside the degrade radius (`degrade_distance`) the distance is treated as zero, i.e. always
    /// the best LOD.
    #[test]
    fn inside_the_degrade_radius_the_best_level_is_always_chosen() {
        let d = info(&[(10.0, 20.0, 1), (30.0, 40.0, 2)]);
        let s = DegradeSettings {
            degrade_distance: 100.0,
            ..DegradeSettings::default()
        };
        assert_eq!(
            get_degrade(&d, 50.0, s),
            (0, 1),
            "50 < 100, so the distance becomes 0"
        );
        assert_eq!(get_degrade(&d, -50.0, s), (0, 1), "|-50| < 100 too");
        // And outside it the *remainder* is what the loops see: 125 - 100 = 25, past ideal 10.
        assert_eq!(get_degrade(&d, 125.0, s), (1, 2), "d = 25");
    }

    /// The max-degrade distance takes the second-to-last level when there are more than two,
    /// because the last carries the `FLT_MAX` terminator.
    #[test]
    fn max_degrade_distance_skips_the_flt_max_terminator() {
        let d = info(&[(10.0, 20.0, 1), (30.0, 40.0, 1), (60.0, f32::MAX, 1)]);
        assert_eq!(get_max_degrade_distance(&d), 40.0);
        let d = info(&[(10.0, 20.0, 1), (30.0, f32::MAX, 1)]);
        assert_eq!(get_max_degrade_distance(&d), 20.0);
    }
}
