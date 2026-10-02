//! Integer narrowing for the interface's arithmetic: a list's length or an index becomes a pixel
//! offset, a wire field or a data id. Each conversion saturates at the target's limit, which no
//! list, index or count the interface holds comes near, so an in-range value is unchanged.

/// `n` as an `i32`, saturating at `i32::MAX`.
#[inline]
#[must_use]
pub fn i32_from(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// A chosen index as a signed record field: no choice (`usize::MAX`) is `-1`.
#[inline]
#[must_use]
pub fn choice_i32(n: usize) -> i32 {
    if n == usize::MAX {
        -1
    } else {
        i32_from(n)
    }
}

/// `n` as a `u32`, saturating at `u32::MAX`.
#[inline]
#[must_use]
pub fn u32_from(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// `n` as an `i32`, saturating at `i32::MIN` and `i32::MAX`.
#[inline]
#[must_use]
pub fn i32_from_i64(n: i64) -> i32 {
    i32::try_from(n).unwrap_or(if n < 0 { i32::MIN } else { i32::MAX })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (integer narrowing helpers).
    use super::*;

    #[test]
    fn an_unchosen_index_becomes_minus_one_and_others_saturate() {
        assert_eq!(choice_i32(usize::MAX), -1);
        assert_eq!(choice_i32(3), 3);
        assert_eq!(i32_from(usize::MAX), i32::MAX);
        assert_eq!(u32_from(7), 7);
        assert_eq!(i32_from_i64(i64::MIN), i32::MIN);
    }
}
