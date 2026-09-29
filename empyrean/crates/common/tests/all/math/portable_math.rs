//! Vectors: local exact float cases and fixed-input bit digests in this module
//! Portable math functions produce host-independent bits (digest over fixed inputs), exact cases
//! are exact, NaN results canonical.
//! Fixture: fixed input ranges, exact edge cases and recorded bit digests.

use empyrean_common::math;

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

fn fold(h: u64, bits: u64) -> u64 {
    (h ^ bits).wrapping_mul(0x0100_0000_01B3)
}

fn digest1(seed: u64, a: f64, b: f64, f: fn(f64) -> f64) -> u64 {
    let mut r = Inputs(seed);
    (0..N).fold(0xCBF2_9CE4_8422_2325, |h, _| {
        fold(h, f(r.f64(a, b)).to_bits())
    })
}

fn digest2(seed: u64, a: f64, b: f64, c: f64, d: f64, f: fn(f64, f64) -> f64) -> u64 {
    let mut r = Inputs(seed);
    (0..N).fold(0xCBF2_9CE4_8422_2325, |h, _| {
        let x = r.f64(a, b);
        fold(h, f(x, r.f64(c, d)).to_bits())
    })
}

fn digest_f32(seed: u64, a: f64, b: f64, f: fn(f32) -> f32) -> u64 {
    let mut r = Inputs(seed);
    (0..N).fold(0xCBF2_9CE4_8422_2325, |h, _| {
        fold(h, u64::from(f(r.f32(a, b)).to_bits()))
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
        ("hypotf", {
            let mut r = Inputs(14);
            (0..N).fold(0xCBF2_9CE4_8422_2325, |h, _| {
                let x = r.f32(-500.0, 500.0);
                fold(
                    h,
                    u64::from(math::hypotf(x, r.f32(-500.0, 500.0)).to_bits()),
                )
            })
        }),
    ]
}

#[test]
fn portable_math_is_the_same_on_every_host() {
    if !math::PORTABLE {
        return;
    }
    const RECORDED: [(&str, u64); 14] = [
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
    ];
    let got = digests();
    let bad: Vec<String> = got
        .iter()
        .zip(RECORDED)
        .filter(|((_, g), (_, want))| g != want)
        .map(|((name, g), (_, want))| format!("{name}: {g:#018x}, recorded {want:#018x}"))
        .collect();
    assert!(
        bad.is_empty(),
        "portable math differs from the recorded digests:\n  {}",
        bad.join("\n  ")
    );
}

/// ACE-facing identities that hold for the C runtime and the portable functions alike: exact
/// results stay exact, and `sin_cosf` is `(sinf, cosf)`.
#[test]
fn exact_cases_are_exact() {
    assert_eq!(math::pow(10.0, 2.0), 100.0);
    assert_eq!(math::pow(2.0, 31.0), 2_147_483_648.0);
    assert_eq!(math::pow(10.0, -1.0), 0.1);
    assert_eq!(math::pow(1000.0, 1.0 / 3.0), 9.999_999_999_999_998);
    assert_eq!(math::log(1.0), 0.0);
    assert_eq!(math::exp(0.0), 1.0);
    assert_eq!(math::atan2(0.0, -1.0), std::f64::consts::PI);
    assert_eq!(math::acos(1.0), 0.0);
    assert_eq!(math::sin_cosf(0.75), (math::sinf(0.75), math::cosf(0.75)));
    assert_eq!(math::sinf(0.0).to_bits(), 0);
    assert_eq!(math::sinf(-0.0).to_bits(), (-0.0f32).to_bits());
}

/// Every NaN result is the one quiet NaN, whatever the input's payload or sign.
#[test]
fn nan_results_are_canonical() {
    if !math::PORTABLE {
        return;
    }
    for bits in [0x7FAB_6705u32, 0xFFA0_5718, 0x7FC0_0001, 0xFFC0_0000] {
        let x = f32::from_bits(bits);
        assert_eq!(
            (math::sinf(x).to_bits(), math::cosf(x).to_bits()),
            (0x7FC0_0000, 0x7FC0_0000),
            "{bits:#x}"
        );
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
            math::acos(x),
            math::atan2(x, 1.0),
            math::exp(x),
            math::log(x),
            math::pow(x, 2.0),
            math::hypot(x, 1.0),
        ] {
            assert_eq!(r.to_bits(), 0x7FF8_0000_0000_0000, "{bits:#x}");
        }
    }
    assert_eq!(math::log(-1.0).to_bits(), 0x7FF8_0000_0000_0000);
    assert_eq!(math::acos(2.0).to_bits(), 0x7FF8_0000_0000_0000);
}
