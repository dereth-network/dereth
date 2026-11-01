//! Colors: four floats, red, green, blue and alpha, each 0 to 1 in use, held as a native object
//! of type 100 (its packed form: the four floats in that order). Scripts set one whole or its
//! alpha alone, read and write it as a 32-bit color (alpha, red, green, blue, a byte each, from
//! the top), and attenuate it (red, green and blue scaled; alpha kept).
//!
//! A color's natives look at the receiver only: a null receiver ends the script.

use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::NativeObject;
use auberean_script::value::{tag, NULL};

/// The color package, and the native type of a color.
pub const PACKAGE: u32 = 0x64;
/// The vector package: scaling a vector is the same machine code as attenuating a color.
const VECTOR: u32 = 0x70;

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (PACKAGE, 0x01, set),
    (PACKAGE, 0x02, attenuate),
    (PACKAGE, 0x03, set_alpha),
    (PACKAGE, 0x04, set_color32),
    (PACKAGE, 0x05, get_color32),
    (VECTOR, 0x09, attenuate),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// A color as its four floats (a color with no value yet is all zero).
#[must_use]
pub fn of(o: &NativeObject) -> [f32; 4] {
    match o {
        NativeObject::Packed { bytes, .. } if bytes.len() >= 16 => {
            let f = |i: usize| {
                f32::from_le_bytes([
                    bytes[i * 4],
                    bytes[i * 4 + 1],
                    bytes[i * 4 + 2],
                    bytes[i * 4 + 3],
                ])
            };
            [f(0), f(1), f(2), f(3)]
        }
        _ => [0.0; 4],
    }
}

/// A color as the native object holds it.
#[must_use]
pub fn object(c: [f32; 4]) -> NativeObject {
    NativeObject::Packed {
        ty: PACKAGE,
        bytes: c.iter().flat_map(|v| v.to_le_bytes()).collect(),
    }
}

/// The receiver's color, or `None` (the script ends) for the null reference.
fn this(c: &Call<'_>) -> Result<Option<(u32, [f32; 4])>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    Ok(Some((r, of(c.vm.heap.native(r)?))))
}

fn store(c: &mut Call<'_>, r: u32, v: [f32; 4]) -> Result<Status, Stop> {
    *c.vm.heap.native_mut(r)? = object(v);
    Ok(Status::Success)
}

/// Set the whole color: red, green, blue and alpha below the receiver.
// D2-08917
fn set(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, _)) = this(c)? else {
        return Ok(Status::Crash);
    };
    let f = |c: &Call<'_>, n| c.arg(n).map(f32::from_bits);
    let v = [f(c, 0)?, f(c, 1)?, f(c, 2)?, f(c, 3)?];
    store(c, r, v)
}

/// Set the alpha alone.
// D2-08918
fn set_alpha(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, mut v)) = this(c)? else {
        return Ok(Status::Crash);
    };
    v[3] = f32::from_bits(c.arg(0)?);
    store(c, r, v)
}

/// Scale red, green and blue by a factor, the alpha kept; on a vector, its three parts.
// D2-09014
fn attenuate(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let k = f32::from_bits(c.arg(0)?);
    if let NativeObject::Vector(v) = c.vm.heap.native_mut(r)? {
        for x in v.iter_mut() {
            *x = (f32::from_bits(*x) * k).to_bits();
        }
        return Ok(Status::Success);
    }
    let mut v = of(c.vm.heap.native(r)?);
    for x in &mut v[..3] {
        *x *= k;
    }
    store(c, r, v)
}

/// Set the color from a 32-bit color: alpha, red, green and blue, a byte each from the top, each
/// over 255.
// D2-08919
#[allow(clippy::cast_precision_loss)]
fn set_color32(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, _)) = this(c)? else {
        return Ok(Status::Crash);
    };
    let w = c.arg(0)?;
    let k = 1.0 / 255.0_f32;
    let byte = |s: u32| ((w >> s) & 0xFF) as f32 * k;
    let v = [byte(16), byte(8), byte(0), (w >> 24) as f32 * k];
    store(c, r, v)
}

/// The color as a 32-bit color, below the receiver: each part times 255 cut to a whole number,
/// alpha shifted to the top byte, then red, green and blue (each part's bits laid over the
/// shifted ones, as the original builds it).
// D2-08920
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn get_color32(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((_, v)) = this(c)? else {
        return Ok(Status::Crash);
    };
    // LINT-OK: the product is kept wide and cut toward zero, as the original's conversion does.
    let part = |x: f32| (f64::from(x) * 255.0) as i64 as i32 as u32;
    let mut w = part(v[3]);
    for x in [v[0], v[1], v[2]] {
        w = (w << 8) | part(x);
    }
    c.write(-2, w, tag::MIXED)?;
    Ok(Status::Success)
}
