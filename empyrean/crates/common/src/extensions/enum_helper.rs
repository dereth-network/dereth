// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Extensions/EnumHelper.cs
//! `EnumHelper`. `GetFlags` walks a generated enum's `ALL` table (its `Enum.GetValues` order), which
//! the caller passes in, in place of .NET's reflection over the enum's members.

// ACE: EnumHelper.GetFlags
/// `Enum.GetValues(e.GetType()).Cast<Enum>().Where(e.HasFlag)`: every declared member, in
/// `Enum.GetValues` order (`members`), whose bits are all set in `value`. `key` gives a member's
/// value as .NET's `ToUInt64` sees it. A zero member always matches (`HasFlag(0)` is true).
#[must_use]
pub fn get_flags<T: Copy>(members: &[T], key: impl Fn(T) -> u64, value: u64) -> Vec<T> {
    members
        .iter()
        .copied()
        .filter(|&flag| value & key(flag) == key(flag))
        .collect()
}

// ACE: EnumHelper.NumFlags
/// The number of set bits, by clearing the lowest set bit until none remain.
#[must_use]
pub fn num_flags(enum_val: u32) -> i32 {
    let mut enum_val = enum_val;
    let mut cnt = 0;
    while enum_val != 0 {
        // remove the next set bit
        enum_val &= enum_val.wrapping_sub(1);
        cnt += 1;
    }
    cnt
}

// ACE: EnumHelper.HasMultiple
/// Whether more than one bit is set.
#[must_use]
pub fn has_multiple(enum_val: u32) -> bool {
    (enum_val & enum_val.wrapping_sub(1)) != 0
}
