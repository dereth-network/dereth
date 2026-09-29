//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Portable math gives the same bits on every host (digests); exact cases are exact; NaN results
//! are canonical.
//! Fixture: synthetic state, geometry and reference vectors.

use dereth_primitives::num::math;

struct Inputs(u64);

impl Inputs {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in `[a, b)`, every eighth draw an arbitrary bit pattern instead.
    #[allow(clippy::cast_precision_loss)]
    fn f64(&mut self, a: f64, b: f64) -> f64 {
        let u = self.next();
        if u.is_multiple_of(8) {
            return f64::from_bits(self.next());
        }
        a + (b - a) * ((u >> 11) as f64 / (1u64 << 53) as f64)
    }
    #[allow(clippy::cast_possible_truncation)]
    fn f32(&mut self, a: f64, b: f64) -> f32 {
        let u = self.next();
        if u.is_multiple_of(8) {
            return f32::from_bits(self.next() as u32);
        }
        self.f64(a, b) as f32
    }
}

const N: usize = 1 << 15;
const BASIS: u64 = 0xCBF2_9CE4_8422_2325;

fn fold(h: u64, bits: u64) -> u64 {
    (h ^ bits).wrapping_mul(0x0100_0000_01B3)
}

fn digest1(seed: u64, a: f64, b: f64, f: fn(f64) -> f64) -> u64 {
    let mut r = Inputs(seed);
    (0..N).fold(BASIS, |h, _| fold(h, f(r.f64(a, b)).to_bits()))
}

fn digest2(seed: u64, a: f64, b: f64, c: f64, d: f64, f: fn(f64, f64) -> f64) -> u64 {
    let mut r = Inputs(seed);
    (0..N).fold(BASIS, |h, _| {
        let x = r.f64(a, b);
        fold(h, f(x, r.f64(c, d)).to_bits())
    })
}

fn digest_f32(seed: u64, a: f64, b: f64, f: fn(f32) -> f32) -> u64 {
    let mut r = Inputs(seed);
    (0..N).fold(BASIS, |h, _| fold(h, u64::from(f(r.f32(a, b)).to_bits())))
}

fn digest2_f32(seed: u64, a: f64, b: f64, c: f64, d: f64, f: fn(f32, f32) -> f32) -> u64 {
    let mut r = Inputs(seed);
    (0..N).fold(BASIS, |h, _| {
        let x = r.f32(a, b);
        fold(h, u64::from(f(x, r.f32(c, d)).to_bits()))
    })
}

fn digests() -> Vec<(&'static str, u64)> {
    use std::f64::consts::PI;
    vec![
        ("sinf", digest_f32(1, -8.0 * PI, 8.0 * PI, math::sinf)),
        ("cosf", digest_f32(2, -8.0 * PI, 8.0 * PI, math::cosf)),
        ("sinf huge", digest_f32(3, 5.0e6, 1.0e30, math::sinf)),
        ("cosf huge", digest_f32(4, 5.0e6, 1.0e30, math::cosf)),
        ("sin", digest1(5, -4.0 * PI, 4.0 * PI, math::sin)),
        ("cos", digest1(6, -4.0 * PI, 4.0 * PI, math::cos)),
        ("acos", digest1(7, -1.0, 1.0, math::acos)),
        (
            "atan2",
            digest2(8, -1.0e3, 1.0e3, -1.0e3, 1.0e3, math::atan2),
        ),
        ("exp", digest1(9, -750.0, 710.0, math::exp)),
        ("log", digest1(10, 0.0, 1.0e7, math::log)),
        ("pow", digest2(11, 0.0, 1.0e3, -20.0, 20.0, math::pow)),
        (
            "pow 1/3",
            digest2(12, 0.0, 1.0e6, 1.0 / 3.0, 1.0 / 3.0, math::pow),
        ),
        (
            "hypot",
            digest2(13, -1.0e3, 1.0e3, -1.0e3, 1.0e3, math::hypot),
        ),
        (
            "hypotf",
            digest2_f32(14, -500.0, 500.0, -500.0, 500.0, math::hypotf),
        ),
        ("tanf", digest_f32(15, -PI, PI, math::tanf)),
        ("asinf", digest_f32(16, -1.0, 1.0, math::asinf)),
        ("acosf", digest_f32(17, -1.0, 1.0, math::acosf)),
        ("atanf", digest_f32(18, -20.0, 20.0, math::atanf)),
        (
            "atan2f",
            digest2_f32(19, -1.0e3, 1.0e3, -1.0e3, 1.0e3, math::atan2f),
        ),
        ("expf", digest_f32(20, -104.0, 89.0, math::expf)),
        ("logf", digest_f32(21, 0.0, 1.0e7, math::logf)),
        ("powf", digest2_f32(22, 0.0, 1.0e3, -20.0, 20.0, math::powf)),
        ("tan", digest1(23, -PI, PI, math::tan)),
        ("asin", digest1(24, -1.0, 1.0, math::asin)),
        ("atan", digest1(25, -20.0, 20.0, math::atan)),
        ("log10", digest1(26, 0.0, 1.0e9, math::log10)),
        ("log2", digest1(27, 0.0, 1.0e9, math::log2)),
    ]
}

#[test]
fn portable_math_is_the_same_on_every_host() {
    if !math::PORTABLE {
        return;
    }
    const RECORDED: [(&str, u64); 27] = [
        ("sinf", 0x4e49_afc6_b8f6_d4b8),
        ("cosf", 0x4bc4_3c45_055c_7f67),
        ("sinf huge", 0x9e5d_f7a4_8c82_2fe5),
        ("cosf huge", 0xdf88_cd21_c9eb_5f5e),
        ("sin", 0xd835_bf0e_d262_766c),
        ("cos", 0xd2dc_8db5_97ef_1a94),
        ("acos", 0x7aa3_23f5_8b56_8d04),
        ("atan2", 0x4c93_778f_fdbf_f76f),
        ("exp", 0x0306_526a_540b_249f),
        ("log", 0xa510_2b96_8af8_9771),
        ("pow", 0x0a26_baa1_ddf0_fcfe),
        ("pow 1/3", 0xef3e_dd93_6739_e340),
        ("hypot", 0xf86a_e5ad_bab3_f598),
        ("hypotf", 0xb312_3a72_0938_4d94),
        ("tanf", 0x965c_2237_a436_46a0),
        ("asinf", 0x1920_f18f_525e_ef7c),
        ("acosf", 0xa492_f431_eb8d_d5b1),
        ("atanf", 0xc5dc_a88f_34e6_249a),
        ("atan2f", 0x0c51_d0ab_b89a_2333),
        ("expf", 0x5071_70b7_b420_63c6),
        ("logf", 0x46a7_71a3_bb4f_6857),
        ("powf", 0xd8f4_014c_5666_df79),
        ("tan", 0x2aa3_8f05_7a69_1ac9),
        ("asin", 0xa17a_4f4e_6037_7988),
        ("atan", 0x7fa3_a2d6_62a5_9dd2),
        ("log10", 0x74bf_2144_c0f8_33b4),
        ("log2", 0x8ab4_238d_6b47_a27e),
    ];
    let got = digests();
    let bad: Vec<String> = got
        .iter()
        .zip(RECORDED)
        .filter(|((_, g), (_, want))| g != want)
        .map(|((name, g), (_, want))| format!("(\"{name}\", {g:#018x}), recorded {want:#018x}"))
        .collect();
    assert!(
        bad.is_empty(),
        "portable math differs from the recorded digests:\n  {}",
        bad.join("\n  ")
    );
}

/// Identities that hold for the C runtime and the portable functions alike: exact results stay
/// exact, and the paired functions are their two halves.
#[test]
fn exact_cases_are_exact() {
    assert_eq!(math::pow(10.0, 2.0), 100.0);
    assert_eq!(math::pow(2.0, 31.0), 2_147_483_648.0);
    assert_eq!(math::pow(10.0, -1.0), 0.1);
    assert_eq!(math::powf(10.0, 2.0), 100.0);
    assert_eq!(math::log(1.0), 0.0);
    assert_eq!(math::log10(1000.0), 3.0);
    assert_eq!(math::log2(1024.0), 10.0);
    assert_eq!(math::exp(0.0), 1.0);
    assert_eq!(math::atan2(0.0, -1.0), std::f64::consts::PI);
    assert_eq!(math::atan2f(0.0, -1.0), std::f32::consts::PI);
    assert_eq!(math::acos(1.0), 0.0);
    assert_eq!(math::asin(1.0), std::f64::consts::FRAC_PI_2);
    assert_eq!(math::sin_cosf(0.75), (math::sinf(0.75), math::cosf(0.75)));
    assert_eq!(math::sin_cos(0.75), (math::sin(0.75), math::cos(0.75)));
    assert_eq!(math::sinf(0.0).to_bits(), 0);
    assert_eq!(math::sinf(-0.0).to_bits(), (-0.0f32).to_bits());
    assert_eq!(math::hypotf(3.0, 4.0), 5.0);
}

/// A huge exponent is an even integer, so a negative base's power is positive: overflow to
/// `+inf`, underflow to `+0`.
#[test]
fn a_negative_base_to_a_huge_exponent_is_positive() {
    let big32 = 4_294_967_296.0f32;
    assert_eq!(math::powf(-2.0, big32), f32::INFINITY);
    assert_eq!(math::powf(-0.5, big32).to_bits(), 0);
    assert_eq!(math::powf(-2.0, -big32).to_bits(), 0);
    assert_eq!(math::powf(-1.0, big32), 1.0);
    assert_eq!(math::powf(-2.0, 16_777_216.0), f32::INFINITY);
    let big64 = 18_446_744_073_709_551_616.0f64;
    assert_eq!(math::pow(-2.0, big64), f64::INFINITY);
    assert_eq!(math::pow(-0.5, big64).to_bits(), 0);
    assert_eq!(math::pow(-1.0, big64), 1.0);
    // Below the thresholds the sign still follows the exponent's parity.
    assert_eq!(math::powf(-2.0, 3.0), -8.0);
    assert_eq!(math::pow(-2.0, 3.0), -8.0);
}

/// A negative base's power is negative exactly when the exponent is an odd integer, through
/// overflow and underflow too; a non-integer exponent gives NaN, except from -0 and -inf.
#[test]
fn a_negative_base_takes_its_sign_from_the_exponents_parity() {
    let q32 = 0x7FC0_0000u32;
    let cases_f32: [(f32, f32, u32); 12] = [
        (-2.0, -3.0, (-0.125f32).to_bits()),
        (-0.5, 301.0, 0x8000_0000),
        (-0.5, 300.0, 0),
        (-3.0, 101.0, f32::NEG_INFINITY.to_bits()),
        (-3.0, 100.0, f32::INFINITY.to_bits()),
        (-0.0, 3.0, 0x8000_0000),
        (-0.0, -3.0, f32::NEG_INFINITY.to_bits()),
        (-0.0, 0.5, 0),
        (f32::NEG_INFINITY, 3.0, f32::NEG_INFINITY.to_bits()),
        (f32::NEG_INFINITY, 2.0, f32::INFINITY.to_bits()),
        (-1.0, f32::INFINITY, 1.0f32.to_bits()),
        (-8.0, 1.0 / 3.0, q32),
    ];
    for (x, y, want) in cases_f32 {
        assert_eq!(math::powf(x, y).to_bits(), want, "powf({x}, {y})");
    }
    let q64 = 0x7FF8_0000_0000_0000u64;
    let cases_f64: [(f64, f64, u64); 6] = [
        (-0.5, 1101.0, 0x8000_0000_0000_0000),
        (-0.5, 1100.0, 0),
        (-3.0, 1001.0, f64::NEG_INFINITY.to_bits()),
        (-0.0, -3.0, f64::NEG_INFINITY.to_bits()),
        (f64::NEG_INFINITY, 0.5, f64::INFINITY.to_bits()),
        (-8.0, 1.0 / 3.0, q64),
    ];
    for (x, y, want) in cases_f64 {
        assert_eq!(math::pow(x, y).to_bits(), want, "pow({x}, {y})");
    }
}

/// Every NaN result is the one quiet NaN, whatever the input's payload or sign.
#[test]
fn nan_results_are_canonical() {
    if !math::PORTABLE {
        return;
    }
    const Q32: u32 = 0x7FC0_0000;
    const Q64: u64 = 0x7FF8_0000_0000_0000;
    for bits in [0x7FAB_6705u32, 0xFFA0_5718, 0x7FC0_0001, 0xFFC0_0000] {
        let x = f32::from_bits(bits);
        let unary: [fn(f32) -> f32; 8] = [
            math::sinf,
            math::cosf,
            math::tanf,
            math::asinf,
            math::acosf,
            math::atanf,
            math::expf,
            math::logf,
        ];
        for (i, f) in unary.iter().enumerate() {
            assert_eq!(f(x).to_bits(), Q32, "f32 unary #{i} of {bits:#x}");
        }
        for r in [
            math::atan2f(x, 1.0),
            math::powf(x, 2.0),
            math::hypotf(x, 1.0),
        ] {
            assert_eq!(r.to_bits(), Q32, "{bits:#x}");
        }
    }
    for bits in [
        0x7FF4_0000_0000_0001u64,
        0xFFF8_0000_0000_0000,
        0xFFF0_0000_0000_0001,
    ] {
        let x = f64::from_bits(bits);
        for r in [
            math::sin(x),
            math::cos(x),
            math::tan(x),
            math::asin(x),
            math::acos(x),
            math::atan(x),
            math::atan2(x, 1.0),
            math::exp(x),
            math::log(x),
            math::log10(x),
            math::log2(x),
            math::pow(x, 2.0),
            math::hypot(x, 1.0),
        ] {
            assert_eq!(r.to_bits(), Q64, "{bits:#x}");
        }
    }
    assert_eq!(math::log(-1.0).to_bits(), Q64);
    assert_eq!(math::acos(2.0).to_bits(), Q64);
    assert_eq!(math::asinf(2.0).to_bits(), Q32);
    assert_eq!(math::logf(-1.0).to_bits(), Q32);
}
