//! Vendors: `VendorProfile`, the exact price formulas and the acceptability test.
//!
//! Three things matter for fidelity in the price formulas:
//!
//! 1. the stored `f32` rate is widened before multiplication; the arithmetic uses **`f64`**
//!    through `floor`/`ceil`, then truncates to an integer;
//! 2. the `± 0.1` fudge is applied **inside** the rounding, not outside;
//! 3. trade notes ignore the vendor's rate entirely — bought at face value, sold at **1.15×**.
//!
//! This module holds the two price formulas; the vendor profile, the shop and the window's
//! requests stay in `dereth-client-model`.

use crate::weenie::item_type;
use dereth_primitives::num::to_i32_f64;

/// Behavior: what the **vendor pays you**.
#[must_use]
pub fn buy_price(unit_value: i32, obj_type: u32, mut rate: f32, count: i32) -> i32 {
    if obj_type == item_type::PROMISSORY_NOTE {
        rate = 1.0;
    }
    // **f64, not f32, and the constant is a true `0.1`.** Retail loads the `f32` rate, then does
    // two *integer* multiplies straight into extended precision, with no f32 round-trip for
    // either operand -- then adds the `double` 0.1. The session runs with the FPU at 53-bit precision
    // (`D3DCREATE_FPU_PRESERVE`), so every intermediate is exactly an `f64` and `f64` reproduces the
    // chain step for step. Computing the product in `f32` rounds three times where the client
    // rounds at 53 bits, and `f64::from(0.1f32)` is 0.100000001490116..., not the `double` 0.1 the
    // client adds. Either mistake moves prices at the boundary.
    let product = f64::from(rate) * f64::from(unit_value) * f64::from(count);
    // The +0.1 fudge is inside the rounding.
    let p = to_i32_f64((product + 0.1).floor());
    if p == 0 {
        return 1;
    }
    if p < 0 {
        return -1;
    }
    p
}

/// Behavior: what **you pay the vendor**.
///
/// Note the asymmetry with [`buy_price`]: the zero-check is `p == 0 → 1` but the negative check is
/// `p < 1 → -1`, so a price of exactly 0 cannot reach the second arm. Transcribed as written.
#[must_use]
pub fn sell_price(unit_value: i32, obj_type: u32, mut rate: f32, count: i32) -> i32 {
    if obj_type == item_type::PROMISSORY_NOTE {
        rate = 1.15;
    }
    // f64 throughout, and a true `0.1` -- see [`buy_price`].
    let product = f64::from(rate) * f64::from(unit_value) * f64::from(count);
    let p = to_i32_f64((product - 0.1).ceil());
    if p == 0 {
        return 1;
    }
    if p < 1 {
        return -1;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The price chain is **f64**, and the fudge constant is a true `0.1`.
    ///
    /// Oracle: the client's form — the `f32` rate, two integer multiplies
    /// (no f32 round-trip), plus the `double` 0.1, `floor`, truncate — with the
    /// FPU at 53-bit precision for the whole session, so every intermediate is an `f64`.
    ///
    /// This case is why it matters rather than being a purist point. A vendor whose buy rate is
    /// `0.9` (which is `0.899999976158…` as an `f32`) buying an item worth 81:
    ///
    /// * f64: `0.899999976158… × 81 = 72.8999980688…`, `+ 0.1 → 72.9999980688…`, floor **72**
    /// * f32: the product rounds to `72.9000015…`, and `f64::from(0.1f32)` adds
    ///   `0.100000001490…`, giving `73.0000030…`, floor **73**
    ///
    /// One pyreal, on an ordinary sale, from two rounding slips that cancel in most cases and not
    /// in this one. Both slips were once present here.
    #[test]
    fn the_price_chain_is_f64_and_the_fudge_is_a_true_tenth() {
        let rate = 0.9f32;
        assert_eq!(
            buy_price(81, 0, rate, 1),
            72,
            "an f32 product rounds this up to 73"
        );
        assert_eq!(buy_price(91, 0, rate, 1), 81);
        assert_eq!(buy_price(1141, 0, rate, 1), 1026);

        // The fudge is inside the rounding, so an exact integer price is unmoved by it.
        assert_eq!(buy_price(100, 0, 1.0, 1), 100);
        assert_eq!(sell_price(100, 0, 1.0, 1), 100);
    }

    /// Evaluate both price formulas with the adjustment inside the rounding.
    /// The widened rate and all arithmetic intermediates use `f64`.
    #[test]
    fn the_two_price_formulas_round_the_way_the_client_does() {
        // 0.5 * 100 * 1 = 50.0; floor(50.1) = 50.
        assert_eq!(buy_price(100, item_type::MISC, 0.5, 1), 50);
        // 1.5 * 100 * 1 = 150.0; ceil(149.9) = 150.
        assert_eq!(sell_price(100, item_type::MISC, 1.5, 1), 150);

        // The fudge is what decides the boundary: 0.5 * 1 = 0.5; floor(0.6) = 0.
        assert_eq!(
            buy_price(1, item_type::MISC, 0.5, 1),
            1,
            "a zero price becomes 1"
        );
        // 0.5 * 3 = 1.5; floor(1.6) = 1.
        assert_eq!(buy_price(3, item_type::MISC, 0.5, 1), 1);
        // 1.5 * 1 = 1.5; ceil(1.4) = 2.
        assert_eq!(sell_price(1, item_type::MISC, 1.5, 1), 2);

        // A negative rate: buy floors below zero and clamps to -1.
        assert_eq!(buy_price(100, item_type::MISC, -1.0, 1), -1);
        assert_eq!(sell_price(100, item_type::MISC, -1.0, 1), -1);
    }

    /// Trade notes ignore the vendor's rate entirely.
    #[test]
    fn trade_notes_ignore_the_vendors_rate() {
        // Whatever the rate, a note is bought at face value and sold at 1.15x.
        assert_eq!(buy_price(1000, item_type::PROMISSORY_NOTE, 0.1, 1), 1000);
        assert_eq!(buy_price(1000, item_type::PROMISSORY_NOTE, 9.0, 1), 1000);
        // ceil(1.15 * 1000 - 0.1) = ceil(1149.9) = 1150.
        assert_eq!(sell_price(1000, item_type::PROMISSORY_NOTE, 0.1, 1), 1150);
    }
}
