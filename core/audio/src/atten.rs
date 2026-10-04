//! The two exact formulas -- the distance attenuation and the pan arithmetic -- that every
//! played sound goes through.
//!
//! There is no 3D audio anywhere in the client. It obtains an
//! `IDirectSound3DListener`, calls `SetRolloffFactor`/`SetOrientation`/`CommitDeferredSettings` once
//! and never touches it again, and every secondary buffer is created with its 3D flag clear. The
//! distance and pan calculations below are the client's complete spatialization model.

use dereth_primitives::num::math;
use dereth_primitives::num::{to_i32, to_i32_f64};

use crate::prefs::{Category, Prefs, SoundFeatures};

/// The near-field radius, `5.0f`: inside it every sound is at full gain and centred.
pub const VOL_MIN_DIST: f32 = 5.0;

/// The distance at which attenuation starts. Not a stored constant in retail: start-up computes it
/// as `5.0f * 5.0f`.
pub const VOL_MIN_DIST_SQ: f32 = 25.0;

/// The floor the attenuation clamps to. Note this is **not** DirectSound's `DSBVOLUME_MIN` (-10000).
pub const VOL_MIN_DB: i32 = -50;

/// The reciprocal of the natural log of two. Also runtime-initialised: start-up computes
/// `1.0 / ln 2` at extended precision, which is bit-identical to `f64::consts::LOG2_E`.
pub const INV_LOG_OF_2: f64 = std::f64::consts::LOG2_E;

/// The client's own constant. `20*log10(2)` is `6.020599913279624`, so this is
/// a *rounded* constant and the curve is fractionally steeper than the textbook one; the difference
/// is far below the 1 dB quantisation everywhere it is used.
pub const DB_PER_OCTAVE: f64 = 6.0206;

/// The client's degrees-to-radians factor, loaded as a **32-bit** float approximation of
/// pi/180 that is short by 7.75e-9 relative. See [`pan`] for why that is observable.
pub const DEG_TO_RAD: f32 = 0.017_453_292;

/// The client's pan scale, a 64-bit `-15.0`.
pub const PAN_SCALE: f64 = -15.0;

/// The client clamps the pan to this before `SetPan(pan * 100)`.
pub const PAN_LIMIT: i32 = 15;

/// The attenuation in dB, returning `None` where the original returns 0.
///
/// `None` means **inaudible**: every caller that checks the return value drops the sound entirely
/// rather than playing it at the floor. Three shipped behaviours live in these eight lines and all
/// three must survive:
///
/// 1. Beyond 5 m the *amplitude* goes as `25/d^2`, which is **-12 dB per doubling of distance** —
///    twice as steep as physical inverse-square falloff.
/// 2. `ceil` before the integer truncation quantises the gain to **whole decibels rounded up**.
/// 3. [`Category::Ambient`] multiplies by `ambient_volume` here, and the ambient play path has
///    *already* multiplied the caller's volume by the same number — so the ambient slider is
///    quadratic. ([`crate::trigger`] owns the caller half.)
/// 4. [`Category::Interface`] multiplies by `effect_volume`, because the interface volume
///    has exactly one reference in the whole client, its own preference registration.
///
///
/// Retail computes `ln(a) · INV_LOG_OF_2 · 6.0206`, rounds that to `f64` *before* `ceil`, then
/// truncates to an integer; this follows the same steps rather than approximating them.
#[must_use]
pub fn attenuation(dist: f32, volume: f32, cat: Category, prefs: &Prefs) -> Option<i32> {
    // The >= branch is the far field.
    let mut a: f64 = if dist >= VOL_MIN_DIST {
        (f64::from(VOL_MIN_DIST_SQ) * f64::from(volume)) / (f64::from(dist) * f64::from(dist))
    } else {
        f64::from(volume)
    };
    if a > 1.0 {
        a = 1.0;
    }
    a *= f64::from(prefs.category_volume(cat));
    // A <= 0 exits with VOL_MIN and a 0 return, and so does an
    // unordered compare, which is why this is a negated `>` and not a `<=`.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(a > 0.0) {
        return None;
    }
    let db = to_i32_f64((math::log(a) * INV_LOG_OF_2 * DB_PER_OCTAVE).ceil());
    if db >= VOL_MIN_DB {
        Some(db)
    } else {
        None
    }
}

/// The pan arithmetic the play path performs.
///
/// `bearing` is the compass heading **from the sound to the listener**, in degrees, and
/// `heading` is the listener's own frame heading. The listener is the *camera*, set once per
/// frame.
///
/// Sign convention: a sound due east of a listener facing north gives `a = -90` and a positive pan,
/// so **positive pan is right**, and front and back are indistinguishable.
///
/// Two truncations and one surprise:
///
/// * The gate is `abs(dist as integer) >= 5` — the float distance truncated first, not
///   `dist >= 5.0`. For a non-negative distance the two agree, and inside 5 m every sound is centred.
/// * The final float-to-int conversion truncates **toward zero**, so the magnitude is rounded down.
/// * The client's pi/180 is a `float`, so `90.0 * DEG_TO_RAD` lands 1.217e-8 radians short of
///   pi/2 and `sin` returns `1 - 2^-53` rather than 1.0. `(1 - 2^-53) * 15` is `14.999999999999998`,
///   which truncates to **14**. The pan therefore does not reach +/-15 at a cardinal bearing and the
///   +/-15 clamp never binds. The tests below pin this result.
#[must_use]
pub fn pan(bearing: f32, heading: f32, dist: f32, features: SoundFeatures) -> i32 {
    // Mono forces centre and nothing else changes.
    if features == SoundFeatures::Mono {
        return 0;
    }
    // C `fmod(bearing - heading, 360)`, sign of the dividend, which
    // is what Rust's `%` on f64 already is. Both operands widen from f32 exactly.
    let mut a = (f64::from(bearing) - f64::from(heading)) % 360.0;
    // Strictly greater only, so `a` ends in (-360, 180].
    if a > 180.0 {
        a -= 360.0;
    }
    // The distance is truncated to an integer, made absolute and compared against 5.
    // `wrapping_abs` rather than `abs` because the truncation yields i32::MIN for NaN and for
    // out-of-range input, and the original's absolute value leaves that value negative, which
    // fails the test anyway.
    if f64::from(to_i32(dist).wrapping_abs()) < f64::from(VOL_MIN_DIST) {
        return 0;
    }
    // Multiply by the 32-bit pi/180, take the sine, scale by -15.0, truncate toward zero.
    let p = to_i32_f64(math::sin(a * f64::from(DEG_TO_RAD)) * PAN_SCALE);
    // The client's own clamp: `pan = clamp(pan, -15, +15)` before `SetPan(pan * 100)`.
    p.clamp(-PAN_LIMIT, PAN_LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: a worked table read from the client's arithmetic, including the -12 dB per
    /// doubling anchor.
    ///
    /// Earlier values for the 90 m and 95 m columns were wrong. The values asserted here are what
    /// the formula actually evaluates to.
    #[test]
    fn the_distance_sweep_reproduces_the_documented_integer_db_table() {
        let p = Prefs::default();
        let table = [
            (1.0f32, Some(0)),
            (5.0, Some(0)),
            (6.0, Some(-3)),
            (7.0, Some(-5)),
            (10.0, Some(-12)),
            (20.0, Some(-24)),
            (40.0, Some(-36)),
            (80.0, Some(-48)),
            (90.0, Some(-50)),
            (95.0, None),
        ];
        for (d, want) in table {
            assert_eq!(attenuation(d, 1.0, Category::Effect, &p), want, "dist {d}");
        }
    }

    /// The whole-metre attenuation sweep gives `0,0,0,0,0,-3,-5,-8,-10,-12` for d = 1..10.
    /// The expected values evaluate the attenuation formula independently.
    #[test]
    fn the_whole_metre_sweep_is_flat_to_5_then_12_db_per_doubling() {
        let p = Prefs::default();
        let metres = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let got: Vec<i32> = metres
            .iter()
            .map(|&d| attenuation(d, 1.0, Category::Effect, &p).expect("audible"))
            .collect();
        assert_eq!(got, vec![0, 0, 0, 0, 0, -3, -5, -8, -10, -12]);
        for d in [10.0f32, 20.0, 40.0] {
            let a = attenuation(d, 1.0, Category::Effect, &p).expect("audible");
            let b = attenuation(d * 2.0, 1.0, Category::Effect, &p).expect("audible");
            assert_eq!(b - a, -12, "doubling from {d} m");
        }
    }

    /// The cut-off is where the exact value first *reaches* -51, because `ceil` rounds up:
    /// `20*log10(25/d^2) <= -51` at `d = 5 / 10^(-51/40)` = 94.18245 m. Oracle: section 5.2's
    /// closing paragraph, which derives the same number.
    #[test]
    fn the_drop_threshold_is_94_18_metres_and_minus_50_saturates_below_it() {
        let p = Prefs::default();
        assert_eq!(attenuation(94.18, 1.0, Category::Effect, &p), Some(-50));
        assert_eq!(attenuation(94.19, 1.0, Category::Effect, &p), None);
        for d in [89.0f32, 90.0, 92.0, 94.0] {
            assert_eq!(
                attenuation(d, 1.0, Category::Effect, &p),
                Some(-50),
                "dist {d}"
            );
        }
    }

    /// The rounding direction is the shipped bug: `ceil` before truncation means the gain is
    /// rounded **up**, towards louder. Oracle: the client's `ceil` immediately
    /// before its float-to-int conversion.
    #[test]
    fn the_gain_is_rounded_up_never_down() {
        let p = Prefs::default();
        // Exact dB at 6 m is -3.167; rounded down it would be -4.
        assert_eq!(attenuation(6.0, 1.0, Category::Effect, &p), Some(-3));
        // Exact dB at 7 m is -5.845; rounded down it would be -6.
        assert_eq!(attenuation(7.0, 1.0, Category::Effect, &p), Some(-5));
    }

    /// Contracts 12.1 and 12.7. Oracle: the recovered category table.
    /// The *second* application of the ambient volume is the caller's, in [`crate::trigger`]; this
    /// asserts the one inside `attenuation` and that Interface reads the effect slider.
    #[test]
    fn interface_uses_the_effect_volume_and_ambient_uses_the_ambient_one() {
        let p = Prefs {
            effect_volume: 0.5,
            ambient_volume: 0.25,
            interface_volume: 0.0,
            ..Prefs::default()
        };
        // 20*log10(0.5) = -6.02 -> ceil -6.
        assert_eq!(attenuation(0.0, 1.0, Category::Effect, &p), Some(-6));
        // Interface takes the same -6, not the silence that interface_volume = 0.0 would give.
        assert_eq!(attenuation(0.0, 1.0, Category::Interface, &p), Some(-6));
        // 20*log10(0.25) = -12.04 -> ceil -12.
        assert_eq!(attenuation(0.0, 1.0, Category::Ambient, &p), Some(-12));
    }

    /// A zero category volume is inaudible, not -50 dB: the `a <= 0` branch returns 0.
    #[test]
    fn a_zero_volume_is_dropped_rather_than_played_at_the_floor() {
        let p = Prefs {
            effect_volume: 0.0,
            ..Prefs::default()
        };
        assert_eq!(attenuation(1.0, 1.0, Category::Effect, &p), None);
        assert_eq!(
            attenuation(1.0, 0.0, Category::Effect, &Prefs::default()),
            None
        );
    }

    /// Oracle: the four cardinal bearings map east to right, west to left, and north and south to
    /// zero, with the magnitude corrected to 14. See the doc comment on [`pan`].
    #[test]
    fn the_cardinal_bearings_pan_east_positive_and_front_back_centre() {
        let s = SoundFeatures::Stereo;
        // Listener facing north (heading 0). `bearing` is FROM the sound TO the listener.
        assert_eq!(pan(270.0, 0.0, 10.0, s), 14, "sound due east -> hard right");
        assert_eq!(pan(90.0, 0.0, 10.0, s), -14, "sound due west -> hard left");
        assert_eq!(
            pan(180.0, 0.0, 10.0, s),
            0,
            "sound due north (ahead) -> centre"
        );
        assert_eq!(
            pan(0.0, 0.0, 10.0, s),
            0,
            "sound due south (behind) -> centre"
        );
    }

    /// Everything inside 5 m is centred, and the Mono override.
    #[test]
    fn inside_five_metres_and_in_mono_the_pan_is_zero() {
        assert_eq!(pan(270.0, 0.0, 4.999, SoundFeatures::Stereo), 0);
        assert_eq!(pan(270.0, 0.0, 5.0, SoundFeatures::Stereo), 14);
        assert_eq!(pan(270.0, 0.0, 100.0, SoundFeatures::Mono), 0);
        // The gate is on the *truncated* distance, so 5.9 m still pans.
        assert_eq!(pan(270.0, 0.0, 5.9, SoundFeatures::Stereo), 14);
    }

    /// The listener's own heading rotates the whole field: the client subtracts it from the bearing.
    #[test]
    fn the_listener_heading_rotates_the_pan_field() {
        let s = SoundFeatures::Stereo;
        // `bearing` runs from the sound to the listener, so bearing 0 means the listener is north of
        // the sound -- the sound is due *south*. A listener facing east hears the south on its right.
        assert_eq!(pan(0.0, 90.0, 10.0, s), 14);
        // bearing 180 puts the sound due north of the listener, which facing east is the left.
        assert_eq!(pan(180.0, 90.0, 10.0, s), -14);
        // Turning through 180 degrees swaps the sides.
        assert_eq!(pan(0.0, 270.0, 10.0, s), -14);
        assert_eq!(pan(180.0, 270.0, 10.0, s), 14);
    }

    /// The `fmod` leaves `a` in `(-360, 180]` rather than `(-180, 180]`, which is harmless only
    /// because `sin` is 360-periodic. Oracle: the first rule in the recovered pan calculation.
    #[test]
    fn the_fmod_normalisation_is_asymmetric_but_sine_makes_it_harmless() {
        let s = SoundFeatures::Stereo;
        for b in [-450.0f32, -90.0, 270.0, 630.0] {
            assert_eq!(pan(b, 0.0, 10.0, s), 14, "bearing {b}");
        }
    }

    /// A full 360-degree sweep never leaves the documented range, and is antisymmetric.
    #[test]
    fn the_full_bearing_sweep_stays_inside_the_documented_range() {
        let s = SoundFeatures::Stereo;
        for i in 0..3600u32 {
            let b = f32::from(u16::try_from(i).expect("< 3600")) / 10.0;
            let p = pan(b, 0.0, 10.0, s);
            assert!((-14..=14).contains(&p), "bearing {b} gave {p}");
        }
        for i in 1..1800u32 {
            let b = f32::from(u16::try_from(i).expect("< 1800")) / 10.0;
            // Reflecting the bearing about the north/south axis reflects the pan, because the
            // truncation is toward zero and therefore symmetric.
            assert_eq!(
                pan(b, 0.0, 10.0, s),
                -pan(360.0 - b, 0.0, 10.0, s),
                "bearing {b}"
            );
        }
    }
}
