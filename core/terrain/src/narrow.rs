//! Width narrowing for values the surrounding bounds already constrain.
//!
//! Index arithmetic in this crate runs over grids whose sizes are fixed by the format — 9 vertices
//! to a landblock side, 8 cells, at most 31 blocks to a window side, at most 60 lights in a pool —
//! so narrowing an index to `u32`, `u16` or `u8` can never truncate. Writing `as u32` would say
//! that with a cast clippy cannot distinguish from a real truncation, and the workspace lint
//! deliberately refuses to make that distinction (`xtask/src/lint.rs`: "a lint that cries wolf gets
//! turned off").
//!
//! These helpers use `TryFrom` instead, so the conversion is checked, there is no cast to audit,
//! and an out-of-range value is loud rather than silently wrapped. The fallback is the type's
//! maximum: an index that large is already a bug, and saturating makes it index past the end of its
//! container rather than aliasing a valid element.
//!
//! This is *not* the float-to-int rule: that one lives in `dereth_primitives::num::to_i32` and reproduces the
//! client's truncating float-to-int conversion. These are integer-to-integer and have no
//! counterpart in the original.

/// A `usize` index as `u32`.
#[inline]
pub fn u32_of(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// A `usize` index as `u16`.
#[inline]
pub fn u16_of(v: usize) -> u16 {
    u16::try_from(v).unwrap_or(u16::MAX)
}

/// A `usize` index as `u8`.
#[inline]
pub fn u8_of(v: usize) -> u8 {
    u8::try_from(v).unwrap_or(u8::MAX)
}

/// A `usize` index as `i32`.
#[inline]
pub fn i32_of(v: usize) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// An `i32` index as `u16`. Negative is out of range and saturates to 0.
#[inline]
pub fn u16_of_i32(v: i32) -> u16 {
    u16::try_from(v).unwrap_or(0)
}

/// A `u32` value as `u8`.
#[inline]
pub fn u8_of_u32(v: u32) -> u8 {
    u8::try_from(v).unwrap_or(u8::MAX)
}

/// An `i64` offset as `usize`. Negative is out of range and saturates to 0.
#[inline]
pub fn usize_of_i64(v: i64) -> usize {
    usize::try_from(v).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the module's own contract — in range these are the identity, and out of range they
    /// saturate rather than wrap. A wrapping conversion would alias a valid index, which is the
    /// failure mode this module exists to prevent.
    #[test]
    fn narrowing_is_the_identity_in_range_and_saturates_outside_it() {
        assert_eq!(u32_of(0), 0);
        assert_eq!(u32_of(1_000_000), 1_000_000);
        assert_eq!(u16_of(65_535), 65_535);
        assert_eq!(
            u16_of(65_536),
            u16::MAX,
            "saturates rather than wrapping to 0"
        );
        assert_eq!(u8_of(255), 255);
        assert_eq!(u8_of(256), u8::MAX);
        assert_eq!(i32_of(7), 7);
        assert_eq!(u16_of_i32(-1), 0);
        assert_eq!(u16_of_i32(300), 300);
        assert_eq!(u8_of_u32(255), 255);
        assert_eq!(usize_of_i64(-5), 0);
        assert_eq!(usize_of_i64(5), 5);
    }
}
