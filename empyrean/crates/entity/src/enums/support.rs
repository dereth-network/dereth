//! The .NET `System.Enum` behaviour every generated ACE enum shares.
//!
//! Hand-written, not ported: this is the runtime side of the generated enums. A generated
//! enum is a transparent newtype with associated constants and three tables (`ALL`, `NAMES`,
//! `NAME_ORDER`); [`ace_enum!`] turns those into the methods below and [`ace_enum_from!`] adds the
//! lossless integer conversions. Unknown values are carried
//! unchanged, so a value read from the wire or the database always round-trips.
//!
//! What .NET does, and what we do:
//!
//! - **`Enum.GetValues` order** is ascending by the value as its unsigned storage type (signed
//!   values sort after the positive ones). `ALL` is in that order: declaration order when the
//!   values are already ascending, else the order .NET's unstable `Array.Sort` leaves them in
//!   (the generator runs the same introsort), so aliases can appear in any order.
//! - **`Enum.GetName`**, and **`ToString()`** of a non-`[Flags]` enum, look a value up with .NET's
//!   `FindDefinedIndex`: a linear `IndexOf` over `ALL` for up to 32 members (the first alias in
//!   `ALL` order), else a binary search, which can land on any alias ([`find_defined_index`]).
//! - **`ToString()` of a `[Flags]` enum** walks `ALL` from the top: a defined value is named by its
//!   **last** alias in `ALL` order (0 by the first member when it is 0); anything else is
//!   decomposed into member names joined with `", "`, largest value first, printed smallest
//!   first, the algorithm of .NET's flag formatter.
//! - **`ToString()` of an undefined value** is the decimal number.
//! - **`Enum.Parse`/`TryParse` by name** is case-sensitive and exact here ([`AceEnum::from_name`]).

/// The trait behind every generated ACE enum. Use the inherent methods; this exists so generic
/// code (tests, the property-bag models) can walk any enum.
pub trait AceEnum: Copy + Eq + 'static {
    /// ACE's type name.
    const TYPE_NAME: &'static str;
    /// Whether the C# enum is `[Flags]`.
    const IS_FLAGS: bool;
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    const MEMBERS: &'static [Self];
    /// The member names, parallel to [`Self::MEMBERS`].
    const MEMBER_NAMES: &'static [&'static str];

    /// The value as .NET's `ToUInt64` sees it: sign-extended for signed underlying types.
    fn key(self) -> u64;
    /// The value's decimal representation, as .NET prints an undefined enum value.
    fn numeric_string(self) -> String;
    /// Index of `name` in [`Self::MEMBER_NAMES`] (ordinal, case-sensitive).
    fn name_index(name: &str) -> Option<usize>;

    /// The member name of this value, or `None` if no member has it: .NET `Enum.GetName`
    /// ([`find_defined_index`]).
    fn name(self) -> Option<&'static str> {
        find_defined_index::<Self>(self.key()).map(|i| Self::MEMBER_NAMES[i])
    }

    /// The member called `name`, exactly as ACE spells it.
    fn from_name(name: &str) -> Option<Self> {
        Self::name_index(name).map(|i| Self::MEMBERS[i])
    }

    /// .NET `Enum.ToString()`: `FormatFlagNames` for a `[Flags]` enum, else `GetNameInlined`;
    /// the number when neither names the value.
    fn to_dotnet_string(self) -> String {
        if Self::IS_FLAGS {
            if let Some(s) = flags_string::<Self>(self.key()) {
                return s;
            }
        } else if let Some(n) = self.name() {
            return n.to_owned();
        }
        self.numeric_string()
    }
}

/// .NET's `Enum.FindDefinedIndex` over `ALL`: `IndexOf` (the first equal entry) for up to 32
/// members, else `SpanHelpers.BinarySearch`, which stops at whichever equal entry it probes first.
/// `ALL` is ascending by key, so comparing keys as `u64` orders them as .NET's storage type does.
pub fn find_defined_index<E: AceEnum>(key: u64) -> Option<usize> {
    const NUMBER_OF_VALUES_THRESHOLD: usize = 32;
    let m = E::MEMBERS;
    if m.len() <= NUMBER_OF_VALUES_THRESHOLD {
        return m.iter().position(|e| e.key() == key);
    }
    let (mut lo, mut hi) = (0isize, isize::try_from(m.len()).unwrap_or(isize::MAX) - 1);
    while lo <= hi {
        #[allow(clippy::cast_sign_loss)] // 0 <= lo <= i <= hi < len
        let i = ((lo + hi) >> 1) as usize;
        match key.cmp(&m[i].key()) {
            std::cmp::Ordering::Equal => return Some(i),
            std::cmp::Ordering::Greater => lo = isize::try_from(i).unwrap_or(isize::MAX) + 1,
            std::cmp::Ordering::Less => hi = isize::try_from(i).unwrap_or(isize::MAX) - 1,
        }
    }
    None
}

/// .NET's `FormatFlagNames`: 0 is the first member's name when that member is 0; otherwise walk
/// the sorted members from the top, and a member equal to the value names it (the last alias);
/// else take each member whose bits are all still present, and succeed only if every bit was
/// named.
fn flags_string<E: AceEnum>(value: u64) -> Option<String> {
    let m = E::MEMBERS;
    if value == 0 {
        return (!m.is_empty() && m[0].key() == 0).then(|| E::MEMBER_NAMES[0].to_owned());
    }
    // `GetSingleFlagsEnumNameForValue`
    for i in (0..m.len()).rev() {
        let cur = m[i].key();
        if cur <= value {
            if cur == value {
                return Some(E::MEMBER_NAMES[i].to_owned());
            }
            break;
        }
    }
    let m = E::MEMBERS;
    let mut remaining = value;
    let mut found = Vec::new();
    for i in (0..m.len()).rev() {
        let cur = m[i].key();
        if i == 0 && cur == 0 {
            break;
        }
        if remaining & cur == cur {
            remaining -= cur;
            found.push(i);
            if remaining == 0 {
                break;
            }
        }
    }
    if remaining != 0 {
        return None;
    }
    let names: Vec<&str> = found.iter().rev().map(|&i| E::MEMBER_NAMES[i]).collect();
    Some(names.join(", "))
}

/// .NET `Enum.GetName(typeof(E), value)` for an integer `value` of any type, converted as .NET's
/// `ToUInt64` converts it (sign-extending signed values).
pub fn get_name<E: AceEnum>(value: i64) -> Option<&'static str> {
    #[allow(clippy::cast_sign_loss)]
    let key = value as u64;
    find_defined_index::<E>(key).map(|i| E::MEMBER_NAMES[i])
}

/// Implements [`AceEnum`], the inherent API, `Debug`, `Display` and (for `flags`) the bit
/// operators for one generated enum.
///
/// Exported (hidden) so that empyrean-tables' generated enums use this one copy.
#[macro_export]
#[doc(hidden)]
macro_rules! ace_enum {
    ($ty:ident, $repr:ty, $kind:ident) => {
        impl $crate::enums::AceEnum for $ty {
            const TYPE_NAME: &'static str = stringify!($ty);
            const IS_FLAGS: bool = $crate::ace_enum!(@is_flags $kind);
            const MEMBERS: &'static [Self] = $ty::ALL;
            const MEMBER_NAMES: &'static [&'static str] = $ty::NAMES;

            #[inline]
            #[allow(clippy::cast_sign_loss, clippy::cast_lossless)]
            fn key(self) -> u64 {
                self.0 as i64 as u64
            }

            fn numeric_string(self) -> String {
                self.0.to_string()
            }

            fn name_index(name: &str) -> Option<usize> {
                $ty::NAME_ORDER
                    .binary_search_by(|&i| $ty::NAMES[i as usize].cmp(name))
                    .ok()
                    .map(|j| $ty::NAME_ORDER[j] as usize)
            }
        }

        impl $ty {
            /// The raw underlying value.
            #[inline]
            pub const fn bits(self) -> $repr {
                self.0
            }

            /// The member name for this value (.NET `Enum.GetName`), or `None`.
            #[inline]
            pub fn name(self) -> Option<&'static str> {
                <Self as $crate::enums::AceEnum>::name(self)
            }

            /// The member called `name`, exactly as ACE spells it.
            #[inline]
            pub fn from_name(name: &str) -> Option<Self> {
                <Self as $crate::enums::AceEnum>::from_name(name)
            }

            /// .NET `ToString()`: the name, the `", "`-joined flag names, or the number.
            #[inline]
            pub fn to_dotnet_string(self) -> String {
                <Self as $crate::enums::AceEnum>::to_dotnet_string(self)
            }

            /// .NET `Enum.IsDefined`.
            #[inline]
            pub fn is_defined(self) -> bool {
                self.name().is_some()
            }
        }

        impl ::core::fmt::Debug for $ty {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self.name() {
                    Some(n) => write!(f, "{}::{}", stringify!($ty), n),
                    None => write!(f, "{}({})", stringify!($ty), self.0),
                }
            }
        }

        impl ::core::fmt::Display for $ty {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(&self.to_dotnet_string())
            }
        }

        $crate::ace_enum!(@ops $ty, $kind);
    };
    (@is_flags flags) => { true };
    (@is_flags plain) => { false };
    (@ops $ty:ident, plain) => {};
    (@ops $ty:ident, flags) => {
        impl $ty {
            /// Whether every bit of `other` is set (.NET `HasFlag`).
            #[inline]
            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }

            /// Whether any bit of `other` is set.
            #[inline]
            pub const fn intersects(self, other: Self) -> bool {
                self.0 & other.0 != 0
            }

            /// Whether no bit is set.
            #[inline]
            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }
        }

        impl ::core::ops::BitOr for $ty {
            type Output = Self;
            #[inline]
            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }

        impl ::core::ops::BitAnd for $ty {
            type Output = Self;
            #[inline]
            fn bitand(self, rhs: Self) -> Self {
                Self(self.0 & rhs.0)
            }
        }

        impl ::core::ops::BitXor for $ty {
            type Output = Self;
            #[inline]
            fn bitxor(self, rhs: Self) -> Self {
                Self(self.0 ^ rhs.0)
            }
        }

        impl ::core::ops::Not for $ty {
            type Output = Self;
            #[inline]
            fn not(self) -> Self {
                Self(!self.0)
            }
        }

        impl ::core::ops::BitOrAssign for $ty {
            #[inline]
            fn bitor_assign(&mut self, rhs: Self) {
                self.0 |= rhs.0;
            }
        }

        impl ::core::ops::BitAndAssign for $ty {
            #[inline]
            fn bitand_assign(&mut self, rhs: Self) {
                self.0 &= rhs.0;
            }
        }

        impl ::core::ops::BitXorAssign for $ty {
            #[inline]
            fn bitxor_assign(&mut self, rhs: Self) {
                self.0 ^= rhs.0;
            }
        }
    };
}

pub(crate) use crate::ace_enum;

/// The lossless `From` conversions of one generated enum: to and from its storage type, and into
/// each wider integer the generator lists after `=>` (only types the storage type widens into
/// without loss, so a signed enum never converts to an unsigned integer). This lets code that
/// fills a shared-crate record's raw integer field pass `value.into()`.
#[macro_export]
#[doc(hidden)]
macro_rules! ace_enum_from {
    ($ty:ident, $repr:ty $(=> $($wide:ty),+)?) => {
        impl ::core::convert::From<$ty> for $repr {
            #[inline]
            fn from(value: $ty) -> Self {
                value.0
            }
        }

        impl ::core::convert::From<$repr> for $ty {
            #[inline]
            fn from(value: $repr) -> Self {
                Self(value)
            }
        }

        $($(
            impl ::core::convert::From<$ty> for $wide {
                #[inline]
                fn from(value: $ty) -> Self {
                    <$wide>::from(value.0)
                }
            }
        )+)?
    };
}

pub(crate) use crate::ace_enum_from;
