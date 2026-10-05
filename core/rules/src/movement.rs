//! The movement system's formulas: the run rate, the jump height and the jump's stamina cost.
//!
//! **One implementation**, used by the motion interpreter (`dereth-animation` re-exports these), by the
//! gameplay rules and by the server. The load modifier is [`crate::burden::load_mod`].

use crate::burden::load_mod;

/// The run rate.
///
/// ```text
/// if (runSkill == 800) return 4.5              // <-- hard-coded early return
/// return ((runSkill / (runSkill + 200.0)) * 11.0 * LoadMod(load) + 4.0) / scale / 4.0
/// ```
///
/// **The `runSkill == 800` branch is a hard-coded exception, not a limit of the curve.** The
/// general expression at 800 gives `((800/1000)·11 + 4)/4 = 3.2`; the early return substitutes
/// 4.5. The function is therefore discontinuous and *non-monotonic*: skill 800 runs about 41 %
/// faster than 801. Movement is server-validated, so smoothing the curve moves the character at a
/// speed the server disagrees with. The comparison is on the *float* conversion of
/// the skill, so it fires only at exactly 800.
///
/// Encumbrance enters only through `LoadMod`, which multiplies the skill term but **not** the
/// `+ 4.0`, so the rate can never fall below 1.0: a fully encumbered character still runs at the
/// 4 m/s baseline.
#[must_use]
pub fn get_run_rate(load: f32, run_skill: i32, scale: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let skill = run_skill as f32;
    if skill == 800.0 {
        return 4.5;
    }
    ((skill / (skill + 200.0)) * 11.0 * load_mod(load) + 4.0) / scale / 4.0
}

/// The jump height.
///
/// ```text
/// e = clamp(extent, 0, 1)
/// h = ((jumpSkill / (jumpSkill + 1300.0)) * 22.2 + 0.05) * LoadMod(load) * e / scale
/// return max(h, 0.35)
/// ```
///
/// **The 0.35 m floor is applied last**, after the extent and load multipliers, so a
/// feather-touch jump and a zero-skill jump both reach exactly 0.35 m. A NaN extent also gives the
/// floor.
///
/// The constants are single precision, but the expression is evaluated at double precision and
/// rounded to `float` once, when the height is stored; [`jump_velocity`] takes the root of the
/// unrounded height.
#[must_use]
pub fn get_jump_height(load: f32, jump_skill: i32, extent: f32, scale: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let skill = jump_skill as f32;
    jump_height_of(load, skill, extent, scale)
}

/// [`get_jump_height`] over a skill already converted to `float`, for a caller whose skill is not a
/// signed 32-bit value (the server's is unsigned); the formula is the same one.
#[must_use]
pub fn jump_height_of(load: f32, skill: f32, extent: f32, scale: f32) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let height = jump_height_wide(load, skill, extent, scale) as f32;
    height
}

/// The launch speed of a jump: `sqrt(height * 19.6)`, i.e. `v = sqrt(2gh)` with `g = 9.8`.
///
/// The height is not rounded to `float` first: the product and the root are taken at double
/// precision on the unrounded height and the speed is rounded once. Rounding the height first,
/// or taking the root in `float`, is one `float` step off for roughly a fifth of jumps.
#[must_use]
pub fn jump_velocity(load: f32, jump_skill: i32, extent: f32, scale: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let skill = jump_skill as f32;
    jump_velocity_of(load, skill, extent, scale)
}

/// [`jump_velocity`] over a skill already converted to `float`.
#[must_use]
pub fn jump_velocity_of(load: f32, skill: f32, extent: f32, scale: f32) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let velocity = (jump_height_wide(load, skill, extent, scale) * 19.6).sqrt() as f32;
    velocity
}

/// The jump height before it is rounded to `float`.
fn jump_height_wide(load: f32, skill: f32, extent: f32, scale: f32) -> f64 {
    const FLOOR: f32 = 0.35;
    let e = f64::from(extent.clamp(0.0, 1.0));
    let s = f64::from(skill);
    let h = ((s / (s + f64::from(1300.0f32))) * f64::from(22.2f32) + f64::from(0.05f32))
        * f64::from(load_mod(load))
        * e
        / f64::from(scale);
    // `h >= floor` is false for NaN, which also takes the floor.
    if h >= f64::from(FLOOR) {
        h
    } else {
        f64::from(FLOOR)
    }
}

/// Returns `ceil((load + 0.5) × extent × 8 + 2)` in the
/// normal case; the "free" flag returns the raw extent truncated toward zero.
#[must_use]
pub fn jump_stamina_cost(load: f32, extent: f32, free: bool) -> i32 {
    if free {
        return dereth_primitives::num::to_i32(extent);
    }
    dereth_primitives::num::to_i32(((load + 0.5) * extent * 8.0 + 2.0).ceil())
}

/// The maximum run rate = `GetRunRate(0, 9999, 1)`.
#[must_use]
pub fn inq_max_run_rate() -> f32 {
    get_run_rate(0.0, 9999, 1.0)
}

/// The maximum run rate under a world's rules: `GetRunRate(0, 9999, run scale)`.
#[must_use]
pub fn inq_max_run_rate_in(rules: &dereth_primitives::WorldRules) -> f32 {
    get_run_rate(0.0, 9999, rules.run_scale)
}
