//! C# explicit numeric conversions in an `unchecked` context, as the net10.0 x64 runtime ACE
//! targets executes them.
//!
//! * **Integer to integer** (`(uint)someInt`, `(byte)someLong`, ...): two's-complement
//!   truncation or sign/zero extension. This is exactly Rust's `as`.
//! * **Floating point to `int`/`uint`/`long`/`ulong`**: since .NET 9 these *saturate* on x86/x64:
//!   the value is truncated toward zero, out-of-range values clamp to the target's `MinValue` or
//!   `MaxValue`, and NaN becomes 0. Breaking-change note: "Floating point-to-integer conversions
//!   are saturating" (learn.microsoft.com/dotnet/core/compatibility/jit/9.0/fp-to-integer). Before
//!   .NET 9 an out-of-range or NaN `(int)double` produced `int.MinValue` (0x80000000) on x64; that
//!   behaviour is **not** what ACE on net10.0 does, so it is not reproduced. Rust's float `as`
//!   casts saturate with the same rules, so they are used directly.
//! * **Floating point to a small integer** (`sbyte`, `byte`, `short`, `ushort`): RyuJIT lowers the
//!   conversion as a (saturating) conversion to `int` followed by an integer narrowing, so
//!   `(byte)300.0 == 44` and `(byte)-1.0 == 255`. Rust's `as u8` would saturate instead, so these
//!   go through `i32` explicitly.
//! * `(float)double` rounds to nearest-even, as Rust's `as f32` does. `(float)ulong` is emitted as
//!   `conv.r.un; conv.r4` and rounds through `double` first.
//!
//! `dereth_primitives::num` (reached through `dereth_primitives::num`) models the *retail x86 client's* x87
//! conversions, whose out-of-range results differ from net10.0's, so it is deliberately not used
//! here.
//!
//! Use it as `let x: u32 = value.cs_cast();`.

/// A C# explicit conversion `(T)value` in an unchecked context. See the module documentation.
pub trait CsCast<T> {
    /// `(T)self`.
    fn cs_cast(self) -> T;
}

macro_rules! int_to_int {
    ($($from:ty),* => $to:ty) => {$(
        impl CsCast<$to> for $from {
            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap, clippy::cast_lossless)]
            fn cs_cast(self) -> $to {
                self as $to
            }
        }
    )*};
}

macro_rules! all_ints_to {
    ($($to:ty),*) => {$(
        int_to_int!(i8, u8, i16, u16, i32, u32, i64, u64 => $to);
    )*};
}

all_ints_to!(i8, u8, i16, u16, i32, u32, i64, u64);

macro_rules! float_to_wide {
    ($($from:ty),* => $($to:ty),*) => {
        float_to_wide!(@each [$($from),*] [$($to),*]);
    };
    (@each [$($from:ty),*] $tos:tt) => {$(
        float_to_wide!(@one $from $tos);
    )*};
    (@one $from:ty [$($to:ty),*]) => {$(
        impl CsCast<$to> for $from {
            /// Saturating, truncating toward zero, NaN to 0 (.NET 9+ x64).
            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            fn cs_cast(self) -> $to {
                self as $to
            }
        }
    )*};
}

float_to_wide!(f32, f64 => i32, u32, i64, u64);

macro_rules! float_to_small {
    ($($from:ty),* => $($to:ty),*) => {
        float_to_small!(@each [$($from),*] [$($to),*]);
    };
    (@each [$($from:ty),*] $tos:tt) => {$(
        float_to_small!(@one $from $tos);
    )*};
    (@one $from:ty [$($to:ty),*]) => {$(
        impl CsCast<$to> for $from {
            /// Saturating conversion to `int`, then two's-complement narrowing.
            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap)]
            fn cs_cast(self) -> $to {
                (self as i32) as $to
            }
        }
    )*};
}

float_to_small!(f32, f64 => i8, u8, i16, u16);

macro_rules! to_float {
    ($($from:ty),* => $to:ty) => {$(
        impl CsCast<$to> for $from {
            /// Round to nearest, ties to even.
            #[inline]
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_lossless)]
            fn cs_cast(self) -> $to {
                self as $to
            }
        }
    )*};
}

to_float!(i8, u8, i16, u16, i32, u32, i64, f64 => f32);

impl CsCast<f32> for u64 {
    /// `(float)ulongValue` compiles to `conv.r.un; conv.r4`: the value is rounded to `double`
    /// first and then to `float`, so it can round twice.
    #[inline]
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    fn cs_cast(self) -> f32 {
        (self as f64) as f32
    }
}
to_float!(i8, u8, i16, u16, i32, u32, i64, u64, f32 => f64);
