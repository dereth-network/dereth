//! Values: 32-bit slots, each with a tag byte saying what it holds.
//!
//! A 64-bit value takes two consecutive slots, low word first, both tagged with the long bit set.
//! The machine never checks a tag before using a value: adding two references adds their ids.
//! Tags matter where values are copied as they are (stores, loads, packing) and where the
//! collector looks for references.

/// What a slot holds.
pub mod tag {
    /// Nothing written yet.
    pub const UNDEFINED: u8 = 0;
    /// A reference id.
    pub const REFERENCE: u8 = 1;
    /// A 32-bit integer.
    pub const INT: u8 = 2;
    /// A 32-bit float.
    pub const FLOAT: u8 = 3;
    /// Written by a native: the script knows the type, the slot does not say.
    pub const MIXED: u8 = 4;
    /// Set on both slots of a 64-bit value.
    pub const LONG: u8 = 0x80;
    /// Both slots of a 64-bit integer.
    pub const LONG_INT: u8 = 0x82;
    /// Both slots of a 64-bit float.
    pub const DOUBLE: u8 = 0x83;
    /// Both slots of a time value: a 64-bit float.
    pub const TIME: u8 = 0xC3;
    /// A local slot just reserved by a function's entry and not written since.
    pub const FRESH: u8 = 0xCD;

    /// Whether a slot with this tag holds a reference the collector and the packer follow.
    #[must_use]
    pub const fn is_reference(t: u8) -> bool {
        t != FRESH && t & 0x0F == REFERENCE
    }
}

/// The null reference.
pub const NULL: u32 = 0xFFFF_FFFF;

/// A function id: the package that declares the function in bits 16-30, its number in bits 0-15,
/// and bit 31 set when the call names the function outright instead of through the receiver.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionId(pub u32);

impl FunctionId {
    /// The bit that makes a call name its function outright.
    pub const DIRECT: u32 = 0x8000_0000;

    /// The package that declares the function.
    #[must_use]
    pub const fn package(self) -> u32 {
        (self.0 >> 16) & 0x7FFF
    }

    /// The function's number within its package.
    #[must_use]
    pub const fn number(self) -> u32 {
        self.0 & 0xFFFF
    }

    /// Whether a call names the function outright.
    #[must_use]
    pub const fn is_direct(self) -> bool {
        self.0 & Self::DIRECT != 0
    }

    /// A function id from its package and number.
    #[must_use]
    pub const fn new(package: u32, number: u32) -> Self {
        Self(((package & 0x7FFF) << 16) | (number & 0xFFFF))
    }

    /// The same function, named outright.
    #[must_use]
    pub const fn direct(self) -> Self {
        Self(self.0 | Self::DIRECT)
    }
}

/// A native's id: its native package in the high half, its number in the low half. The same
/// numbering as [`FunctionId`]; a native has no code in the library, only this id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NativeId(pub u32);

impl NativeId {
    /// A native's id from its package and number.
    #[must_use]
    pub const fn new(package: u32, number: u32) -> Self {
        Self(((package & 0xFFFF) << 16) | (number & 0xFFFF))
    }

    /// The native package.
    #[must_use]
    pub const fn package(self) -> u32 {
        self.0 >> 16
    }

    /// The native's number within its package.
    #[must_use]
    pub const fn number(self) -> u32 {
        self.0 & 0xFFFF
    }
}

impl core::fmt::Display for NativeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#x}/{}", self.package(), self.number())
    }
}

/// A float to an integer the way the client converts: truncated toward zero to 64 bits, with
/// every value outside the 64-bit range (and every NaN) giving the lowest 64-bit integer.
#[must_use]
pub fn float_to_long(x: f64) -> i64 {
    // 2^63: the first value past the range on the positive side.
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    if x.is_nan() || x >= LIMIT || x < -LIMIT {
        i64::MIN
    } else {
        // In range: the cast truncates toward zero and cannot saturate.
        #[allow(clippy::cast_possible_truncation)]
        let v = x.trunc() as i64;
        v
    }
}

/// A float to a 32-bit integer the way the client converts: the low word of the 64-bit
/// conversion.
#[must_use]
pub fn float_to_int(x: f64) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    let low = float_to_long(x) as u32;
    low
}

/// The low and high words of a 64-bit value.
#[must_use]
pub const fn split(v: u64) -> (u32, u32) {
    #[allow(clippy::cast_possible_truncation)]
    let low = v as u32;
    (low, (v >> 32) as u32)
}

/// A 64-bit value from its low and high words.
#[must_use]
pub const fn join(low: u32, high: u32) -> u64 {
    (high as u64) << 32 | low as u64
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (value helpers)
    use super::*;

    #[test]
    fn a_function_id_splits_into_package_number_and_direct_bit() {
        let f = FunctionId(0x8204_000B);
        assert_eq!((f.package(), f.number(), f.is_direct()), (0x204, 0xB, true));
        assert_eq!(FunctionId::new(0x204, 0xB).direct(), f);
    }

    #[test]
    fn a_float_out_of_range_converts_to_the_lowest_long() {
        assert_eq!(float_to_long(-2.9), -2);
        assert_eq!(float_to_long(f64::NAN), i64::MIN);
        assert_eq!(float_to_long(1e30), i64::MIN);
        assert_eq!(float_to_int(3e9), 3_000_000_000);
        assert_eq!(float_to_int(-1.5), 0xFFFF_FFFF);
    }
}
