//! The world's position hashes: deterministic numbers from a pair of grid coordinates and a seed.
//!
//! Everything the landscape chooses "at random" (which way a square is split, which material and
//! mask a vertex takes, the tint of its color, which day the sky shows) is one of these hashes of
//! global coordinates, so every block and every client agrees on it without sharing state.

/// The hash's top sixteen bits: `(s·K1 − (x·y²·s²·K2 + K3)·x − y·K4) >> 16`, in wrapping 32-bit
/// arithmetic.
// D2-03614
#[must_use]
pub fn hash16(x: i32, y: i32, seed: i32) -> u32 {
    let (x, y, s) = (x as u32, y as u32, seed as u32);
    let inner = x
        .wrapping_mul(y)
        .wrapping_mul(y)
        .wrapping_mul(s)
        .wrapping_mul(s)
        .wrapping_mul(0x2BD3_8901)
        .wrapping_add(0x6B3C_5F11);
    let v = s
        .wrapping_mul(0x6243_3D7D)
        .wrapping_sub(inner.wrapping_mul(x))
        .wrapping_sub(y.wrapping_mul(0x4A65_06DB));
    v >> 16
}

/// [`hash16`] as a fraction in `[0, 1)`: the sixteen bits times 2⁻¹⁶.
#[must_use]
pub fn fraction(x: i32, y: i32, seed: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // sixteen bits are exact in an f32
    let v = hash16(x, y, seed) as f32;
    v * (1.0 / 65_536.0)
}

/// [`hash16`] scaled to `[0, scale)`: the sixteen bits times `scale / 65536`, the form the
/// weighted pickers use with a list's frequency total.
// D2-12478
#[must_use]
pub fn scaled(x: i32, y: i32, seed: i32, scale: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // sixteen bits are exact in an f32
    let v = hash16(x, y, seed) as f32;
    v * scale / 65_536.0
}

/// The seed of the landscape's split hash.
pub const SPLIT_SEED: i32 = 0x02F1_0087;

/// Whether the square at global square coordinates `(x, y)` is cut from its south-west to its
/// north-east corner: the split hash's fraction is at least one half.
// D2-03722
#[must_use]
pub fn sw_to_ne_cut(x: i32, y: i32) -> bool {
    hash16(x, y, SPLIT_SEED) >= 0x8000
}
