//! Arithmetic natives (the C library's minimum, maximum, absolute value, floor and ceiling) and
//! dice (the engine's random number generator).
//!
//! These natives look at no receiver. Their arguments are below it, a 64-bit one taking two slots
//! low word first; the result goes below the arguments, tagged as a native's (a 64-bit result
//! tags only its low slot, as the engine does, and leaves the high slot's tag as it was).
//!
//! The engine does its floating-point work at double precision, rounding to nearest: a
//! computation on single floats is carried out on doubles and rounded to a single float once,
//! when the result is stored.
//!
//! **The transcendental functions** are the floating-point unit's own (sine, cosine, tangent, arc
//! tangent, logarithms, the power of two behind the exponential) or the C library's (arc sine and
//! arc cosine), worked at extended precision and rounded to a double once. Here they are the
//! workspace's own functions, which give the same bits on every machine and the nearest double
//! almost always: the client's last bit can differ from them now and then. What the unit does
//! that a library function does not is kept: a sine, cosine or tangent of a number too large for
//! the unit (from 2 to the 63rd) is the number itself; an operation that is invalid answers the
//! unit's own not-a-number (all ones in the exponent, the sign and the top fraction bit set); a
//! not-a-number given is answered quieted.
//!
//! **Comparisons as the engine makes them**: the larger of two values is the first when it is
//! greater than the second, else the second; the smaller is the first when it is less, else the
//! second. So when either is not a number, both give the second.
//!
//! **Dice** draw from the client's one generator ([`Random`]): never seeded, so every process
//! starts from the same state, and every draw, by scripts or by anything else in the client, takes
//! the next number. The host lends it ([`Host::random`]).
//!
//! [`Host::random`]: auberean_script::native::Host::random

use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::value::{join, split, tag};

const MATH: u32 = 0x44;
const RANDOM: u32 = 0x5F;

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (MATH, 0x01, sin),
    (MATH, 0x02, cos),
    (MATH, 0x03, tan),
    (MATH, 0x04, acos),
    (MATH, 0x05, asin),
    (MATH, 0x06, atan),
    (MATH, 0x0B, log),
    (MATH, 0x0C, log10),
    (MATH, 0x0D, exp),
    (MATH, 0x0F, sqrt),
    (MATH, 0x10, map_to_unit_interval),
    (MATH, 0x11, fmod),
    (MATH, 0x07, ceil),
    (MATH, 0x08, floor),
    (MATH, 0x09, abs),
    (MATH, 0x0A, fabs),
    (MATH, 0x0E, pow),
    (MATH, 0x12, min),
    (MATH, 0x13, max),
    (MATH, 0x14, imin),
    (MATH, 0x15, imax),
    (MATH, 0x16, labs),
    (MATH, 0x17, fmin),
    (MATH, 0x18, fmax),
    (MATH, 0x19, limin),
    (MATH, 0x1A, limax),
    (MATH, 0x1B, ulimin),
    (MATH, 0x1C, ulimax),
    (RANDOM, 0x01, roll_percentile),
    (RANDOM, 0x02, roll_dice),
    (RANDOM, 0x03, roll_dice_int),
    (RANDOM, 0x04, roll_gaussian_dice),
    (RANDOM, 0x05, roll_skewed_number),
    (RANDOM, 0x06, roll_skewed_number_ex),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

// ---------------------------------------------------------------------------------------------
// Slots
// ---------------------------------------------------------------------------------------------

fn long_at(c: &Call<'_>, k: i64) -> Result<u64, Stop> {
    Ok(join(c.cx.at(k)?, c.cx.at(k + 1)?))
}

fn double_at(c: &Call<'_>, k: i64) -> Result<f64, Stop> {
    Ok(f64::from_bits(long_at(c, k)?))
}

fn float_at(c: &Call<'_>, k: i64) -> Result<f32, Stop> {
    Ok(f32::from_bits(c.cx.at(k)?))
}

/// A 64-bit result at `fp[k]` and `fp[k + 1]`: the low slot tagged as a native's long, the high
/// slot's tag left as it was.
fn long_result(c: &mut Call<'_>, k: i64, v: u64) -> Result<Status, Stop> {
    let (lo, hi) = split(v);
    c.write(k, lo, tag::LONG | tag::MIXED)?;
    let i = c.cx.frame_index(k + 1)?;
    c.cx.data[i] = hi;
    Ok(Status::Success)
}

fn word_result(c: &mut Call<'_>, k: i64, v: u32) -> Result<Status, Stop> {
    c.write(k, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// The larger as the engine picks it: the first when it is greater, else the second.
fn larger<T: PartialOrd>(a: T, b: T) -> T {
    if a > b {
        a
    } else {
        b
    }
}

/// The smaller as the engine picks it: the first when it is less, else the second.
fn smaller<T: PartialOrd>(a: T, b: T) -> T {
    if a < b {
        a
    } else {
        b
    }
}

// ---------------------------------------------------------------------------------------------
// Arithmetic
// ---------------------------------------------------------------------------------------------

/// The smallest integer not below a double (`fp[-3..-2]`).
// D2-09453
fn ceil(c: &mut Call<'_>) -> Result<Status, Stop> {
    let x = double_at(c, -3)?;
    long_result(c, -5, x.ceil().to_bits())
}

/// The largest integer not above a double.
// D2-09460
fn floor(c: &mut Call<'_>) -> Result<Status, Stop> {
    let x = double_at(c, -3)?;
    long_result(c, -5, x.floor().to_bits())
}

/// A double without its sign.
// D2-09456
fn fabs(c: &mut Call<'_>) -> Result<Status, Stop> {
    let x = double_at(c, -3)?;
    long_result(c, -5, x.abs().to_bits())
}

/// A double raised to a double's power (no receiver read), as the C library's power gives it: a
/// whole power of a whole base rounded once to the nearest double (an exact tie toward zero); any
/// other, the workspace's own power (the same bits on every machine).
// D2-09468
fn pow(c: &mut Call<'_>) -> Result<Status, Stop> {
    let x = double_at(c, -3)?;
    let y = double_at(c, -5)?;
    long_result(c, -7, c_pow(x, y).to_bits())
}

fn c_pow(x: f64, y: f64) -> f64 {
    let whole = |v: f64| v.fract() == 0.0;
    if x >= 0.0 && whole(x) && x <= f64::from(u32::MAX) && whole(y) && y.abs() <= 1024.0 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        return auberean_text::crt::power(x as u32, dereth_primitives::num::to_i32_f64(y));
    }
    dereth_primitives::num::math::pow(x, y)
}

/// The floating-point unit's answer to an invalid operation, stored as a double.
const INDEFINITE: u64 = 0xFFF8_0000_0000_0000;

/// A result as the unit stores it: a not-a-number given comes back quieted; a not-a-number the
/// operation made is the unit's own.
fn unit_result(x: f64, r: f64) -> f64 {
    if x.is_nan() {
        f64::from_bits(x.to_bits() | 0x0008_0000_0000_0000)
    } else if r.is_nan() {
        f64::from_bits(INDEFINITE)
    } else {
        r
    }
}

/// A function of one double (`fp[-3..-2]`, no receiver read), answered at `fp[-5..-4]`.
fn one(c: &mut Call<'_>, f: fn(f64) -> f64) -> Result<Status, Stop> {
    let x = double_at(c, -3)?;
    long_result(c, -5, unit_result(x, f(x)).to_bits())
}

/// The largest magnitude the unit's sine, cosine and tangent take: from 2 to the 63rd, the number
/// is left as it was.
const UNIT_TRIG_LIMIT: f64 = 9_223_372_036_854_775_808.0;

/// A sine, cosine or tangent as the unit takes it.
fn unit_trig(x: f64, f: fn(f64) -> f64) -> f64 {
    if x.is_finite() && x.abs() >= UNIT_TRIG_LIMIT {
        x
    } else {
        f(x)
    }
}

/// The sine of a double, in radians.
// D2-09469
fn sin(c: &mut Call<'_>) -> Result<Status, Stop> {
    one(c, |x| unit_trig(x, dereth_primitives::num::math::sin))
}

/// The cosine of a double.
// D2-09454
fn cos(c: &mut Call<'_>) -> Result<Status, Stop> {
    one(c, |x| unit_trig(x, dereth_primitives::num::math::cos))
}

/// The tangent of a double.
// D2-09471
fn tan(c: &mut Call<'_>) -> Result<Status, Stop> {
    one(c, |x| unit_trig(x, dereth_primitives::num::math::tan))
}

/// The arc cosine of a double (outside -1 to 1: not a number).
// D2-09450
fn acos(c: &mut Call<'_>) -> Result<Status, Stop> {
    one(c, dereth_primitives::num::math::acos)
}

/// The arc sine of a double (outside -1 to 1: not a number).
// D2-09451
fn asin(c: &mut Call<'_>) -> Result<Status, Stop> {
    one(c, dereth_primitives::num::math::asin)
}

/// The arc tangent of a double.
// D2-09452
fn atan(c: &mut Call<'_>) -> Result<Status, Stop> {
    one(c, dereth_primitives::num::math::atan)
}

/// The natural logarithm of a double (of a negative: not a number; of 0: minus infinity).
// D2-09466
fn log(c: &mut Call<'_>) -> Result<Status, Stop> {
    one(c, dereth_primitives::num::math::log)
}

/// The base 10 logarithm of a double.
// D2-09467
fn log10(c: &mut Call<'_>) -> Result<Status, Stop> {
    one(c, dereth_primitives::num::math::log10)
}

/// The low part of the unit's base 2 logarithm of e, past a double's precision.
const LOG2_E_LOW: f64 = 2.038_300_084_272_748_3e-17;

/// e to a double's power as the unit works it out: the power times the base 2 logarithm of e,
/// rounded to a double; split into its nearest integer (ties to even) and the rest; two to the
/// rest, rounded to a double; scaled by two to the integer. An infinite power has no rest, so it
/// answers not a number.
fn unit_exp(x: f64) -> f64 {
    let t = x.mul_add(core::f64::consts::LOG2_E, x * LOG2_E_LOW);
    let n = t.round_ties_even();
    let m = dereth_primitives::num::math::pow(2.0, t - n);
    if !n.is_finite() {
        return m * n;
    }
    // A scale past the exponent's range runs to zero or infinity on the way.
    #[allow(clippy::cast_possible_truncation)]
    let mut k = dereth_primitives::num::to_i32_f64(n.clamp(-2200.0, 2200.0));
    let mut v = m;
    while k > 0 {
        let s = k.min(1000);
        v *= f64::from_bits(((1023 + s) as u64) << 52);
        k -= s;
    }
    while k < 0 {
        let s = (-k).min(1000);
        v *= f64::from_bits(((1023 - s) as u64) << 52);
        k += s;
    }
    v
}

/// e to a double's power.
// D2-09455
fn exp(c: &mut Call<'_>) -> Result<Status, Stop> {
    one(c, unit_exp)
}

/// The square root of a double (of a negative: not a number).
// D2-09470
fn sqrt(c: &mut Call<'_>) -> Result<Status, Stop> {
    one(c, f64::sqrt)
}

/// A double (`fp[-3..-2]`) over another (`fp[-5..-4]`) mapped into -1 to 1: the arc tangent of the
/// first times the reciprocal of the second, times two over pi; 0 when the second is 0. Answered at
/// `fp[-7..-6]`.
// D2-09446
fn map_to_unit_interval(c: &mut Call<'_>) -> Result<Status, Stop> {
    let x = double_at(c, -3)?;
    let y = double_at(c, -5)?;
    let r = if y == 0.0 {
        0.0
    } else {
        let r = dereth_primitives::num::math::atan(x * (1.0 / y)) * 0.636_619_772_367_584;
        match (x.is_nan(), y.is_nan()) {
            (true, _) => unit_result(x, r),
            (false, true) => unit_result(y, r),
            _ => unit_result(0.0, r),
        }
    };
    long_result(c, -7, r.to_bits())
}

/// The remainder of a double (`fp[-3..-2]`) divided by another (`fp[-5..-4]`), with the sign of the
/// first; by 0, not a number. Answered at `fp[-7..-6]`.
// D2-09461
fn fmod(c: &mut Call<'_>) -> Result<Status, Stop> {
    let x = double_at(c, -3)?;
    let y = double_at(c, -5)?;
    let r = if y.is_nan() {
        unit_result(y, x % y)
    } else {
        unit_result(x, x % y)
    };
    long_result(c, -7, r.to_bits())
}

/// An integer's absolute value (the most negative integer is its own).
// D2-09449
fn abs(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_wrap)]
    let x = c.cx.at(-2)? as i32;
    #[allow(clippy::cast_sign_loss)]
    word_result(c, -3, x.wrapping_abs() as u32)
}

/// A 64-bit integer's absolute value.
// D2-09457
fn labs(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_wrap)]
    let x = long_at(c, -3)? as i64;
    #[allow(clippy::cast_sign_loss)]
    long_result(c, -5, x.wrapping_abs() as u64)
}

/// The smaller of two doubles (`fp[-3..-2]`, `fp[-5..-4]`).
// D2-09448
fn min(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (a, b) = (double_at(c, -3)?, double_at(c, -5)?);
    long_result(c, -7, smaller(a, b).to_bits())
}

/// The larger of two doubles.
// D2-09447
fn max(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (a, b) = (double_at(c, -3)?, double_at(c, -5)?);
    long_result(c, -7, larger(a, b).to_bits())
}

/// The smaller of two integers (`fp[-2]`, `fp[-3]`).
// D2-09463
fn imin(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_wrap)]
    let (a, b) = (c.cx.at(-2)? as i32, c.cx.at(-3)? as i32);
    #[allow(clippy::cast_sign_loss)]
    word_result(c, -4, smaller(a, b) as u32)
}

/// The larger of two integers.
// D2-09462
fn imax(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_wrap)]
    let (a, b) = (c.cx.at(-2)? as i32, c.cx.at(-3)? as i32);
    #[allow(clippy::cast_sign_loss)]
    word_result(c, -4, larger(a, b) as u32)
}

/// The smaller of two floats.
// D2-09459
fn fmin(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (a, b) = (float_at(c, -2)?, float_at(c, -3)?);
    word_result(c, -4, smaller(a, b).to_bits())
}

/// The larger of two floats.
// D2-09458
fn fmax(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (a, b) = (float_at(c, -2)?, float_at(c, -3)?);
    word_result(c, -4, larger(a, b).to_bits())
}

/// The smaller of two 64-bit integers (`fp[-3..-2]`, `fp[-5..-4]`).
// D2-09465
fn limin(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_wrap)]
    let (a, b) = (long_at(c, -3)? as i64, long_at(c, -5)? as i64);
    #[allow(clippy::cast_sign_loss)]
    long_result(c, -7, smaller(a, b) as u64)
}

/// The larger of two 64-bit integers.
// D2-09464
fn limax(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_wrap)]
    let (a, b) = (long_at(c, -3)? as i64, long_at(c, -5)? as i64);
    #[allow(clippy::cast_sign_loss)]
    long_result(c, -7, larger(a, b) as u64)
}

/// The smaller of two unsigned 64-bit integers.
// D2-09473
fn ulimin(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (a, b) = (long_at(c, -3)?, long_at(c, -5)?);
    long_result(c, -7, smaller(a, b))
}

/// The larger of two unsigned 64-bit integers.
// D2-09472
fn ulimax(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (a, b) = (long_at(c, -3)?, long_at(c, -5)?);
    long_result(c, -7, larger(a, b))
}

// ---------------------------------------------------------------------------------------------
// Dice
// ---------------------------------------------------------------------------------------------

pub use auberean_primitives::random::{percentile, roll_float, roll_int, Random};

/// A number from the normal distribution of `mean` and `deviation` by the ratio of uniforms, from
/// the draws `draw` gives: a pair `u`, `v` (`v` the second draw less a half, times 1.7156, each kept
/// as a single float) is taken at once inside the quadratic test's inner bound, refused outside its
/// outer bound, and between them taken when `v` squared is at most -4 ln(`u`) `u` squared; the
/// number is `v / u` times `deviation` plus `mean`.
// D2-12864
pub fn roll_gaussian(mut draw: impl FnMut() -> f64, mean: f32, deviation: f32) -> f64 {
    const HALF: f32 = 0.5;
    const SCALE: f32 = 1.715_6;
    const S: f32 = 0.449_871;
    const T: f32 = 0.386_595;
    const A: f32 = 0.196;
    const B: f32 = 0.254_72;
    const INNER: f64 = 0.275_97;
    const OUTER: f32 = 0.278_46;
    loop {
        #[allow(clippy::cast_possible_truncation)]
        let u = f64::from(draw() as f32);
        #[allow(clippy::cast_possible_truncation)]
        let v = f64::from(((draw() - f64::from(HALF)) * f64::from(SCALE)) as f32);
        let x = u - f64::from(S);
        let y = v.abs() + f64::from(T);
        let q = x * x + y * (f64::from(A) * y - f64::from(B) * x);
        let taken = q < INNER
            || (q <= f64::from(OUTER)
                && dereth_primitives::num::math::log(u) * u * u * -4.0 >= v * v);
        if taken {
            return v / u * f64::from(deviation) + f64::from(mean);
        }
    }
}

/// A number from the normal distribution about `mean` that lies from `low` to `high`, tried
/// twice and `mean` itself when neither try does; a `deviation` of 0 is half the distance from
/// `high` to `mean`.
// D2-09533
pub fn roll_skewed(
    mut draw: impl FnMut() -> f64,
    low: f32,
    high: f32,
    mean: f32,
    deviation: f32,
) -> f64 {
    let deviation = if deviation == 0.0 {
        #[allow(clippy::cast_possible_truncation)]
        let d = ((f64::from(high) - f64::from(mean)).abs() * 0.5) as f32;
        d
    } else {
        deviation
    };
    for _ in 0..2 {
        let g = roll_gaussian(&mut draw, mean, deviation);
        if !(g < f64::from(low) || g > f64::from(high)) {
            return g;
        }
    }
    f64::from(mean)
}

fn draw(c: &mut Call<'_>) -> Result<f64, Stop> {
    c.host.random()
}

/// A draw for a roll that draws until it takes a number: a host that draws none gives a half
/// (which the rolls take at once) and its refusal is kept for after the roll.
fn host_draw(c: &mut Call<'_>, failed: &mut Option<Stop>) -> f64 {
    c.host.random().unwrap_or_else(|e| {
        failed.get_or_insert(e);
        0.5
    })
}

/// A float between two bounds (`fp[-2]`, `fp[-3]`).
// D2-09534
fn roll_dice(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (a, b) = (float_at(c, -2)?, float_at(c, -3)?);
    let v = if a == b {
        a
    } else {
        roll_float(draw(c)?, a, b)
    };
    word_result(c, -4, v.to_bits())
}

/// An integer between two bounds (`fp[-2]`, `fp[-3]`), both included.
// D2-09535
fn roll_dice_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_wrap)]
    let (a, b) = (c.cx.at(-2)? as i32, c.cx.at(-3)? as i32);
    let v = if a == b { a } else { roll_int(draw(c)?, a, b) };
    #[allow(clippy::cast_sign_loss)]
    word_result(c, -4, v as u32)
}

/// A whole number from 1 to 100, to `fp[-2]`.
// D2-09537
fn roll_percentile(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = percentile(draw(c)?);
    #[allow(clippy::cast_sign_loss)]
    word_result(c, -2, v as u32)
}

/// A number from the normal distribution of mean `fp[-2]` and deviation `fp[-3]`, to `fp[-4]`.
// D2-09536
fn roll_gaussian_dice(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (mean, deviation) = (float_at(c, -2)?, float_at(c, -3)?);
    let mut failed = None;
    let g = roll_gaussian(|| host_draw(c, &mut failed), mean, deviation);
    if let Some(e) = failed {
        return Err(e);
    }
    #[allow(clippy::cast_possible_truncation)]
    word_result(c, -4, (g as f32).to_bits())
}

/// A number about the mean `fp[-4]` between `fp[-2]` and `fp[-3]`, its deviation half the
/// distance from the second bound to the mean, to `fp[-5]`.
// D2-09538
fn roll_skewed_number(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (low, high, mean) = (float_at(c, -2)?, float_at(c, -3)?, float_at(c, -4)?);
    let mut failed = None;
    let g = roll_skewed(|| host_draw(c, &mut failed), low, high, mean, 0.0);
    if let Some(e) = failed {
        return Err(e);
    }
    #[allow(clippy::cast_possible_truncation)]
    word_result(c, -5, (g as f32).to_bits())
}

/// A number about the mean `fp[-4]` with deviation `fp[-5]` between `fp[-2]` and `fp[-3]`, to
/// `fp[-6]`.
// D2-09539
fn roll_skewed_number_ex(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (low, high) = (float_at(c, -2)?, float_at(c, -3)?);
    let (mean, deviation) = (float_at(c, -4)?, float_at(c, -5)?);
    let mut failed = None;
    let g = roll_skewed(|| host_draw(c, &mut failed), low, high, mean, deviation);
    if let Some(e) = failed {
        return Err(e);
    }
    #[allow(clippy::cast_possible_truncation)]
    word_result(c, -6, (g as f32).to_bits())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the comparisons on hand-picked values)
    use super::*;

    #[test]
    fn the_larger_and_smaller_of_a_number_and_not_a_number_are_the_second() {
        assert_eq!(larger(f64::NAN, 2.0), 2.0);
        assert!(larger(2.0, f64::NAN).is_nan());
        assert_eq!(smaller(f64::NAN, 2.0), 2.0);
        assert!(smaller(2.0, f64::NAN).is_nan());
        assert_eq!(larger(-1i32, 3), 3);
        assert_eq!(smaller(-1i32, 3), -1);
    }
}
