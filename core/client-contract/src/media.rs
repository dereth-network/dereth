//! The UI easing table: the 100-entry sine ramp every timed UI animation is indexed
//! against, and the lookup that indexes it.
//!
//! They live here because `dereth_client_contract::teleport`'s animation model needs them and a
//! contract crate may not name a presentation crate. They are pure arithmetic: `dereth_ui::media`
//! re-exports both.

use dereth_primitives::num::math;

/// 100 `short`s, built once at UI start-up.
///
/// The obvious description is "`t[i] = sin(i·π/99)` truncated to `short`, then replaced by
/// the running sum scaled so the last entry is 1024". That is **incomplete**: taken literally every
/// entry truncates to 0 and the scaling pass divides by zero. The client's own loop has one more
/// step: the sine is multiplied by **1024.0** before the truncation.
/// So the first pass is `t[i] = trunc(sin(i · 3.141592 / 99) · 1024)`, and only then does a second
/// pass replace each entry with `(running_sum << 10) / total` using a signed 32-bit division.
///
/// Note the π literal is the truncated **3.141592**, not `std::f64::consts::PI`.
#[must_use]
pub fn level_array() -> [i16; 100] {
    /// The client's own π. It is **not** `f64::consts::PI`: the compiled constant is
    /// truncated at seven digits, and using the exact one would move two table entries.
    #[allow(clippy::approx_constant)]
    const PI_LITERAL: f64 = 3.141_592;
    /// The client's `1/99`, to the last bit it stores.
    const ONE_OVER_99: f64 = 0.010_101_010_101_010_102;
    /// The scale the sine is multiplied by before truncation.
    const SCALE: f64 = 1024.0;

    let mut raw = [0_i32; 100];
    let mut total: i32 = 0;
    for (i, slot) in raw.iter_mut().enumerate() {
        let x = f64::from(i32::try_from(i).unwrap_or(0)) * PI_LITERAL * ONE_OVER_99;
        // The client's float-to-int helper truncates toward zero; `dereth_primitives::num::to_i32_f64` is that
        // conversion.
        let v = dereth_primitives::num::to_i32_f64(math::sin(x) * SCALE);
        *slot = v;
        total += v;
    }
    let mut out = [0_i16; 100];
    let mut running: i32 = 0;
    for i in 0..100 {
        running += raw[i];
        // Shift left by 10, signed 32-bit divide, then store the low 16 bits.
        let scaled = running.wrapping_mul(1024) / total;
        // The store keeps the low 16 bits of the quotient and nothing else.
        // LINT-OK: the narrowing *is* the client's; the values here are 0..=1024.
        #[allow(clippy::cast_possible_truncation)]
        {
            out[i] = scaled as i16;
        }
    }
    out
}

/// Index the easing table with `f ∈ [0,1]`.
///
/// Clamp below at 0.0 and above at 1.0, then take `trunc(f * 99)` as the index. The client spells
/// that as `i = trunc(f * -99.0)` and indexes `-i`; the double negation is how the compiler folded
/// the sign, and the result is the plain index.
#[must_use]
pub fn anim_level(table: &[i16; 100], f: f32) -> i16 {
    // Clamp below, then above, exactly in that order.
    let f = f.clamp(0.0, 1.0);
    let i = -dereth_primitives::num::to_i32_f64(f64::from(f) * -99.0);
    table[usize::try_from(i.clamp(0, 99)).unwrap_or(0)]
}
