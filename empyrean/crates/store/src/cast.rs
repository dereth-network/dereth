//! Small conversion helpers (not ACE).

use empyrean_common::dotnet::CsCast;

/// A `usize` index or count as the C# integer type `T` (C#'s `(T)i`, unchecked).
pub(crate) fn from_index<T>(i: usize) -> T
where
    u64: CsCast<T>,
{
    u64::try_from(i).unwrap_or(u64::MAX).cs_cast()
}
