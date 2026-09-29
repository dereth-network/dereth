//! The transcendental functions: the same bits on every host.
//!
//! `std`'s `f32::sin`, `f64::atan2`, `f64::powf` and the rest are calls into the host's C runtime,
//! and C runtimes disagree in the last bit: Windows' UCRT, glibc, macOS's libm and musl each round
//! differently, and one of them picks a code path at run time by CPU feature. A one-ulp difference
//! in a heading or a friction factor compounds tick by tick, so two hosts running the same
//! simulation drift apart. Every transcendental call in the workspace goes through this module
//! instead (the workspace `clippy.toml` bans `std`'s), and by default every function here is
//! **portable**: pure Rust, the same bits on every host and architecture.
//!
//! Each function is the candidate closest to the Windows UCRT (the runtime the original goldens
//! were recorded against) over the arguments the engine uses, measured per function:
//!
//! * `pxfm` (BSD-3-Clause OR Apache-2.0; pure-Rust ports of CORE-MATH and LLVM libc, correctly
//!   rounded): every `f32` function, and `tan`, `atan2`, `exp`, `log`, `log2`, `log10`, `pow` and
//!   `hypot` in `f64`. A correctly rounded result does not depend on how it is computed, so it is
//!   the same whether `pxfm` takes its FMA path (chosen at run time on x86, always on ARM) or not.
//! * `libm` (MIT; the Rust port of musl, itself from FreeBSD's msun/fdlibm): `sin`, `cos`, `asin`,
//!   `acos` and `atan` in `f64`, whose UCRT versions are faithfully rounded and agree with fdlibm's
//!   more often than with the correctly rounded result. `libm` uses only IEEE basic operations,
//!   never FMA, so it is the same everywhere.
//!
//! Three guards keep the bits identical everywhere:
//!
//! * `pxfm`'s `f64` `exp` misrounds subnormal results (by up to 2^51 ulp), so below
//!   `ln(f64::MIN_POSITIVE)` `exp` uses `libm`'s, which matches the UCRT there.
//! * `pxfm`'s `pow` and `powf` take a different path on ARM for a negative base, and give some
//!   results the wrong sign there (a huge exponent's oddness is read through a saturating integer
//!   cast). So a negative base's power is computed from the base's magnitude, and the sign is set
//!   here by the rule: negative exactly when the exponent is an odd integer, NaN when it is not an
//!   integer and the base is finite and non-zero.
//! * A NaN result's sign and payload depend on the code path and the CPU, so every NaN result is
//!   the one quiet NaN (`0x7FF8_0000_0000_0000` / `0x7FC0_0000`), whatever the input.
//!
//! The cargo feature `host-libm` restores the host C runtime (through `std`): bit-exact with the
//! UCRT on Windows and host-dependent elsewhere. It exists to measure against, not to ship.
//!
//! `sqrt`, `floor`, `abs`, `powi` and the rest of `std`'s float methods are not here: IEEE 754
//! requires `sqrt` to be correctly rounded, and the others are exact or pure Rust.

#[cfg(not(feature = "host-libm"))]
mod imp {
    /// Below this, `exp` is subnormal (`ln(f64::MIN_POSITIVE)`), where `pxfm` 0.1.30 misrounds.
    const EXP_SUBNORMAL: f64 = -708.396_418_532_264_1;
    /// From here up every `f32` is an even integer.
    const EVEN_F32: f32 = 16_777_216.0;
    /// From here up every `f64` is an even integer.
    const EVEN_F64: f64 = 9_007_199_254_740_992.0;

    /// `pow` for a negative base (sign bit set), from `pow_abs` of its magnitude.
    macro_rules! negative_base {
        ($x:expr, $y:expr, $even:expr, $int:ty, $nan:expr, $pow_abs:expr) => {{
            let (x, y) = ($x, $y);
            let r = $pow_abs(x.abs(), y);
            if y.trunc() == y {
                // An integer, or infinite (even).
                // Exact: an integer below the even threshold fits the integer type.
                #[allow(clippy::cast_possible_truncation)]
                let odd = y.abs() < $even && (y as $int) & 1 != 0;
                if odd {
                    -r
                } else {
                    r
                }
            } else if x == 0.0 || x.is_infinite() {
                // -0 and -inf to a non-integer power are +0 or +inf.
                r
            } else {
                $nan
            }
        }};
    }

    fn n64(r: f64) -> f64 {
        if r.is_nan() {
            f64::from_bits(0x7FF8_0000_0000_0000)
        } else {
            r
        }
    }
    fn n32(r: f32) -> f32 {
        if r.is_nan() {
            f32::from_bits(0x7FC0_0000)
        } else {
            r
        }
    }

    pub fn sinf(x: f32) -> f32 {
        n32(pxfm::f_sinf(x))
    }
    pub fn cosf(x: f32) -> f32 {
        n32(pxfm::f_cosf(x))
    }
    pub fn tanf(x: f32) -> f32 {
        n32(pxfm::f_tanf(x))
    }
    pub fn asinf(x: f32) -> f32 {
        n32(pxfm::f_asinf(x))
    }
    pub fn acosf(x: f32) -> f32 {
        n32(pxfm::f_acosf(x))
    }
    pub fn atanf(x: f32) -> f32 {
        n32(pxfm::f_atanf(x))
    }
    pub fn atan2f(y: f32, x: f32) -> f32 {
        n32(pxfm::f_atan2f(y, x))
    }
    pub fn expf(x: f32) -> f32 {
        n32(pxfm::f_expf(x))
    }
    pub fn logf(x: f32) -> f32 {
        n32(pxfm::f_logf(x))
    }
    pub fn powf(x: f32, y: f32) -> f32 {
        n32(if x.is_sign_negative() {
            negative_base!(x, y, EVEN_F32, i32, f32::NAN, pxfm::f_powf)
        } else {
            pxfm::f_powf(x, y)
        })
    }
    pub fn hypotf(x: f32, y: f32) -> f32 {
        n32(pxfm::f_hypotf(x, y))
    }

    pub fn sin(x: f64) -> f64 {
        n64(libm::sin(x))
    }
    pub fn cos(x: f64) -> f64 {
        n64(libm::cos(x))
    }
    pub fn tan(x: f64) -> f64 {
        n64(pxfm::f_tan(x))
    }
    pub fn asin(x: f64) -> f64 {
        n64(libm::asin(x))
    }
    pub fn acos(x: f64) -> f64 {
        n64(libm::acos(x))
    }
    pub fn atan(x: f64) -> f64 {
        n64(libm::atan(x))
    }
    pub fn atan2(y: f64, x: f64) -> f64 {
        n64(pxfm::f_atan2(y, x))
    }
    pub fn exp(x: f64) -> f64 {
        n64(if x < EXP_SUBNORMAL {
            libm::exp(x)
        } else {
            pxfm::f_exp(x)
        })
    }
    pub fn log(x: f64) -> f64 {
        n64(pxfm::f_log(x))
    }
    pub fn log10(x: f64) -> f64 {
        n64(pxfm::f_log10(x))
    }
    pub fn log2(x: f64) -> f64 {
        n64(pxfm::f_log2(x))
    }
    pub fn pow(x: f64, y: f64) -> f64 {
        n64(if x.is_sign_negative() {
            negative_base!(x, y, EVEN_F64, i64, f64::NAN, pxfm::f_pow)
        } else {
            pxfm::f_pow(x, y)
        })
    }
    pub fn hypot(x: f64, y: f64) -> f64 {
        n64(pxfm::f_hypot(x, y))
    }
}

#[cfg(feature = "host-libm")]
#[allow(clippy::disallowed_methods)] // the host C runtime, on purpose
mod imp {
    pub fn sinf(x: f32) -> f32 {
        x.sin()
    }
    pub fn cosf(x: f32) -> f32 {
        x.cos()
    }
    pub fn tanf(x: f32) -> f32 {
        x.tan()
    }
    pub fn asinf(x: f32) -> f32 {
        x.asin()
    }
    pub fn acosf(x: f32) -> f32 {
        x.acos()
    }
    pub fn atanf(x: f32) -> f32 {
        x.atan()
    }
    pub fn atan2f(y: f32, x: f32) -> f32 {
        y.atan2(x)
    }
    pub fn expf(x: f32) -> f32 {
        x.exp()
    }
    pub fn logf(x: f32) -> f32 {
        x.ln()
    }
    pub fn powf(x: f32, y: f32) -> f32 {
        x.powf(y)
    }
    pub fn hypotf(x: f32, y: f32) -> f32 {
        x.hypot(y)
    }

    pub fn sin(x: f64) -> f64 {
        x.sin()
    }
    pub fn cos(x: f64) -> f64 {
        x.cos()
    }
    pub fn tan(x: f64) -> f64 {
        x.tan()
    }
    pub fn asin(x: f64) -> f64 {
        x.asin()
    }
    pub fn acos(x: f64) -> f64 {
        x.acos()
    }
    pub fn atan(x: f64) -> f64 {
        x.atan()
    }
    pub fn atan2(y: f64, x: f64) -> f64 {
        y.atan2(x)
    }
    pub fn exp(x: f64) -> f64 {
        x.exp()
    }
    pub fn log(x: f64) -> f64 {
        x.ln()
    }
    pub fn log10(x: f64) -> f64 {
        x.log10()
    }
    pub fn log2(x: f64) -> f64 {
        x.log2()
    }
    pub fn pow(x: f64, y: f64) -> f64 {
        x.powf(y)
    }
    pub fn hypot(x: f64, y: f64) -> f64 {
        x.hypot(y)
    }
}

/// True when this build uses the portable functions (the default), false under `host-libm`.
pub const PORTABLE: bool = cfg!(not(feature = "host-libm"));

macro_rules! unary {
    ($($(#[$m:meta])* $name:ident: $t:ty;)*) => {$(
        $(#[$m])*
        #[inline]
        #[must_use]
        pub fn $name(x: $t) -> $t {
            imp::$name(x)
        }
    )*};
}
macro_rules! binary {
    ($($(#[$m:meta])* $name:ident($a:ident, $b:ident): $t:ty;)*) => {$(
        $(#[$m])*
        #[inline]
        #[must_use]
        pub fn $name($a: $t, $b: $t) -> $t {
            imp::$name($a, $b)
        }
    )*};
}

unary! {
    /// `sinf(x)`.
    sinf: f32;
    /// `cosf(x)`.
    cosf: f32;
    /// `tanf(x)`.
    tanf: f32;
    /// `asinf(x)`.
    asinf: f32;
    /// `acosf(x)`.
    acosf: f32;
    /// `atanf(x)`.
    atanf: f32;
    /// `expf(x)`.
    expf: f32;
    /// `logf(x)`, the natural logarithm.
    logf: f32;
    /// `sin(x)`.
    sin: f64;
    /// `cos(x)`.
    cos: f64;
    /// `tan(x)`.
    tan: f64;
    /// `asin(x)`.
    asin: f64;
    /// `acos(x)`.
    acos: f64;
    /// `atan(x)`.
    atan: f64;
    /// `exp(x)`.
    exp: f64;
    /// `log(x)`, the natural logarithm.
    log: f64;
    /// `log10(x)`.
    log10: f64;
    /// `log2(x)`.
    log2: f64;
}

binary! {
    /// `atan2f(y, x)`.
    atan2f(y, x): f32;
    /// `powf(x, y)`.
    powf(x, y): f32;
    /// `hypotf(x, y)`.
    hypotf(x, y): f32;
    /// `atan2(y, x)`.
    atan2(y, x): f64;
    /// `pow(x, y)`.
    pow(x, y): f64;
    /// `hypot(x, y)`.
    hypot(x, y): f64;
}

/// `(sinf(x), cosf(x))`, computed as the two calls (not a fused `sincosf`, whose bits may differ).
#[inline]
#[must_use]
pub fn sin_cosf(x: f32) -> (f32, f32) {
    (imp::sinf(x), imp::cosf(x))
}

/// `(sin(x), cos(x))`, computed as the two calls.
#[inline]
#[must_use]
pub fn sin_cos(x: f64) -> (f64, f64) {
    (imp::sin(x), imp::cos(x))
}
