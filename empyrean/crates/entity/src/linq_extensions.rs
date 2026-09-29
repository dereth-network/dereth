// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/LINQExtensions.cs
//! `LINQExtensions`.

/// `IEnumerable<ulong>.Sum()`, which LINQ lacks. Unchecked, so it wraps on overflow.
// ACE: LINQExtensions.Sum
pub fn sum(values: impl IntoIterator<Item = u64>) -> u64 {
    let mut sum = 0u64;
    for value in values {
        sum = sum.wrapping_add(value);
    }
    sum
}
