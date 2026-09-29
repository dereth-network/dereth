//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Ran2 and CRT-LCG streams reproduce pinned digests (seed 0 = seed 1, negatives untouched); float-
//! to-int matches the edge table where Rust would saturate; Perlin tables rebuild from CRT rand.
//! Fixture: synthetic state, geometry and reference vectors.

use dereth_primitives::num::rng::{CrtRand, Ran2};
use dereth_primitives::num::{floor_to_i32, to_i32, to_i32_f64};

/// FNV-1a 64 over a byte stream: the digest every pinned stream below uses.
fn fnv1a64(bytes: impl IntoIterator<Item = u8>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn digest_u64(words: &[u64]) -> u64 {
    fnv1a64(words.iter().flat_map(|w| w.to_le_bytes()))
}

fn digest_u32(words: &[u32]) -> u64 {
    fnv1a64(words.iter().flat_map(|w| w.to_le_bytes()))
}

fn digest_i32(words: &[i32]) -> u64 {
    fnv1a64(words.iter().flat_map(|w| w.to_le_bytes()))
}

fn digest_u16(words: &[u16]) -> u64 {
    fnv1a64(words.iter().flat_map(|w| w.to_le_bytes()))
}

/// One pinned `ran2` stream: 1000 `next_f64` draws, and 256 each of `roll_i32(1, 10)` and
/// `roll_f32(0.0, 1.0)`, each from a freshly seeded generator.
struct Ran2Stream {
    seed: i32,
    first_bits: [u64; 4],
    draws_digest: u64,
    roll_i32_first: [i32; 8],
    roll_i32_digest: u64,
    roll_f32_digest: u64,
}

#[rustfmt::skip]
const RAN2: [Ran2Stream; 6] = [
    Ran2Stream {
        seed: 1,
        first_bits: [0x3FD2_43AE_3F20_F1B6, 0x3FD0_3705_4444_897F, 0x3FB7_ED8D_BBE3_C01F, 0x3FE3_78CE_78AE_391C],
        draws_digest: 0x3B13_22A6_10D3_7D1D,
        roll_i32_first: [3, 3, 1, 7, 10, 2, 5, 10],
        roll_i32_digest: 0x5F04_106F_79CD_9A88,
        roll_f32_digest: 0x3598_087F_1D9D_4F9E,
    },
    Ran2Stream {
        seed: 12345,
        first_bits: [0x3F9B_DA23_BA7E_DBBA, 0x3FAE_2F0E_EC0B_3FE9, 0x3FCE_9D0D_7E54_4AF6, 0x3F98_EF0E_988E_BFB1],
        draws_digest: 0x09DB_A63F_544C_514E,
        roll_i32_first: [1, 1, 3, 1, 2, 8, 7, 5],
        roll_i32_digest: 0x93BD_F954_D973_D5C2,
        roll_f32_digest: 0xAE4A_D688_0962_607F,
    },
    Ran2Stream {
        seed: -42,
        first_bits: [0x3FD1_DB08_98DB_73B5, 0x3FD4_885F_8122_8F6C, 0x3FEE_44F8_30D9_CCD0, 0x3FDC_A1E6_4983_82ED],
        draws_digest: 0xAF89_7E4A_03F8_5CCC,
        roll_i32_first: [3, 4, 10, 5, 1, 6, 2, 8],
        roll_i32_digest: 0x98EB_A070_0AD5_80A0,
        roll_f32_digest: 0xADDC_CB38_A405_0798,
    },
    Ran2Stream {
        seed: 2147483647,
        first_bits: [0x3FA7_C676_07C9_CA61, 0x3FD0_91C5_CA80_CD58, 0x3FCB_CB81_0C75_23B2, 0x3FE4_4D1D_A33B_35AE],
        draws_digest: 0x5829_478F_607F_4294,
        roll_i32_first: [1, 3, 3, 7, 8, 2, 4, 10],
        roll_i32_digest: 0x4A08_E3DF_90C3_EE90,
        roll_f32_digest: 0xF7F9_FD50_AE37_3E6D,
    },
    Ran2Stream {
        seed: 1063984020,
        first_bits: [0x3FEB_0F09_3177_FC1B, 0x3FE5_AC72_B924_842F, 0x3FD5_CB71_58F9_1945, 0x3FEC_9CC5_C7C0_1B57],
        draws_digest: 0x4361_1191_9F95_C6E7,
        roll_i32_first: [9, 7, 4, 9, 5, 2, 5, 5],
        roll_i32_digest: 0x63EF_45ED_B587_DBCF,
        roll_f32_digest: 0x1189_F601_4873_0680,
    },
    Ran2Stream {
        seed: 7,
        first_bits: [0x3FDC_EE8E_88B6_6AA7, 0x3FEC_6EB2_B1E1_82AA, 0x3FD4_580C_7182_7843, 0x3FE3_15EB_2D6C_8E2C],
        draws_digest: 0x85E1_1401_6846_1D4E,
        roll_i32_first: [5, 9, 4, 6, 7, 10, 4, 5],
        roll_i32_digest: 0x9424_27EA_4F04_924C,
        roll_f32_digest: 0xE177_32C1_B9E9_D42F,
    },
];

#[test]
fn ran2_reproduces_every_pinned_stream() {
    for s in &RAN2 {
        let mut g = Ran2::new(s.seed);
        let bits: Vec<u64> = (0..1000).map(|_| g.next_f64().to_bits()).collect();
        assert_eq!(bits[..4], s.first_bits, "ran2 seed {} first draws", s.seed);
        assert_eq!(
            digest_u64(&bits),
            s.draws_digest,
            "ran2 seed {} digest of 1000 draws",
            s.seed
        );

        let mut gi = Ran2::new(s.seed);
        let rolls: Vec<i32> = (0..256).map(|_| gi.roll_i32(1, 10)).collect();
        assert_eq!(
            rolls[..8],
            s.roll_i32_first,
            "roll_i32(1, 10) seed {}",
            s.seed
        );
        assert_eq!(
            digest_i32(&rolls),
            s.roll_i32_digest,
            "roll_i32(1, 10) seed {} digest",
            s.seed
        );

        let mut gf = Ran2::new(s.seed);
        let rolls: Vec<u32> = (0..256).map(|_| gf.roll_f32(0.0, 1.0).to_bits()).collect();
        assert_eq!(
            digest_u32(&rolls),
            s.roll_f32_digest,
            "roll_f32(0.0, 1.0) seed {} digest",
            s.seed
        );
    }
}

/// Seed 0 becomes seed 1; a negative seed is **not** normalised (seed -42 has its own stream).
#[test]
fn zero_and_one_seed_ran2_to_the_same_stream_and_negative_seeds_are_left_alone() {
    let one = RAN2.iter().find(|s| s.seed == 1).expect("seed 1 is pinned");
    let mut zero = Ran2::new(0);
    let bits: Vec<u64> = (0..1000).map(|_| zero.next_f64().to_bits()).collect();
    assert_eq!(
        digest_u64(&bits),
        one.draws_digest,
        "seed 0 draws the seed-1 stream"
    );

    let neg = RAN2
        .iter()
        .find(|s| s.seed == -42)
        .expect("seed -42 is pinned");
    let mut g = Ran2::new(-42);
    let bits: Vec<u64> = (0..1000).map(|_| g.next_f64().to_bits()).collect();
    assert_eq!(digest_u64(&bits), neg.draws_digest);
    let mut pos = Ran2::new(42);
    let pos_bits: Vec<u64> = (0..1000).map(|_| pos.next_f64().to_bits()).collect();
    assert_ne!(pos_bits, bits, "-42 must not be folded onto 42");
}

/// The CRT LCG, `s = s * 214013 + 2531011; return (s >> 16) & 0x7FFF`: 1000 draws per seed.
#[rustfmt::skip]
const CRT: [(u32, [u16; 8], u64); 5] = [
    (0x0000_0000, [38, 7719, 21238, 2437, 8855, 11797, 8365, 32285], 0x87A2_0F83_7438_3730),
    (0x0000_0001, [41, 18467, 6334, 26500, 19169, 15724, 11478, 29358], 0x1B02_C5F1_535A_2AE8),
    (0x0000_3039, [7584, 19164, 25795, 22125, 5828, 23405, 27477, 5413], 0x416B_6E6B_39B8_6B70),
    (0xDEAD_BEEF, [18337, 16920, 18732, 19770, 16163, 20831, 31795, 19149], 0xF790_568F_D701_CC16),
    (0xFFFF_FFFF, [35, 29739, 3374, 11141, 31308, 7870, 5253, 2445], 0x09C3_2FD0_6E68_4383),
];

#[test]
fn crt_lcg_reproduces_every_pinned_stream() {
    for (seed, first, digest) in CRT {
        let mut g = CrtRand::new(seed);
        let draws: Vec<u16> = (0..1000).map(|_| g.next_u16()).collect();
        assert_eq!(draws[..8], first, "crt seed {seed:#010X} first draws");
        assert_eq!(
            digest_u16(&draws),
            digest,
            "crt seed {seed:#010X} digest of 1000 draws"
        );
    }
    // The two generators are separate and must never be merged: assert they actually differ.
    let mut c = CrtRand::new(1);
    let a: Vec<u16> = (0..16).map(|_| c.next_u16()).collect();
    let mut r = Ran2::new(1);
    let b: Vec<u16> = (0..16)
        .map(|_| u16::try_from(to_i32_f64(r.next_f64() * 32768.0)).unwrap_or(0))
        .collect();
    assert_ne!(a, b, "ran2 and the CRT LCG must be independent generators");
}

/// The full float-to-int edge table: `(bits, to_i32, floor_to_i32, what Rust's saturating `as`
/// gives)`. The client truncates toward zero and yields the x86 integer-indefinite `0x80000000` on
/// NaN and out of range, where Rust saturates; `floor_to_i32` is the separate `floor(); ftol2();`
/// idiom and differs from truncation for every negative non-integer.
#[rustfmt::skip]
const FTOL_F32: [(u32, i32, i32, i32); 43] = [
    (0x0000_0000, 0, 0, 0), // 0.0
    (0x8000_0000, 0, 0, 0), // -0.0
    (0x3F00_0000, 0, 0, 0), // 0.5
    (0xBF00_0000, 0, -1, 0), // -0.5
    (0x3F80_0000, 1, 1, 1), // 1.0
    (0xBF80_0000, -1, -1, -1), // -1.0
    (0x3FC0_0000, 1, 1, 1), // 1.5
    (0xBFC0_0000, -1, -2, -1), // -1.5
    (0x4020_0000, 2, 2, 2), // 2.5
    (0xC020_0000, -2, -3, -2), // -2.5
    (0x3EFF_FFFD, 0, 0, 0), // 0.49999991059303284
    (0xBEFF_FFFD, 0, -1, 0), // -0.49999991059303284
    (0x3F7F_FFFE, 0, 0, 0), // 0.9999998807907104
    (0xBF7F_FFFE, 0, -1, 0), // -0.9999998807907104
    (0x4AFF_FFFE, 8388607, 8388607, 8388607), // 8388607.0
    (0xCAFF_FFFE, -8388607, -8388607, -8388607), // -8388607.0
    (0x4B00_0000, 8388608, 8388608, 8388608), // 8388608.0
    (0xCB00_0000, -8388608, -8388608, -8388608), // -8388608.0
    (0x4B00_0001, 8388609, 8388609, 8388609), // 8388609.0
    (0xCB00_0001, -8388609, -8388609, -8388609), // -8388609.0
    (0x4B7F_FFFF, 16777215, 16777215, 16777215), // 16777215.0
    (0xCB7F_FFFF, -16777215, -16777215, -16777215), // -16777215.0
    (0x4B80_0000, 16777216, 16777216, 16777216), // 16777216.0
    (0xCB80_0000, -16777216, -16777216, -16777216), // -16777216.0
    (0x4EFF_FFFF, 2147483520, 2147483520, 2147483520), // 2147483520.0
    (0xCEFF_FFFF, -2147483520, -2147483520, -2147483520), // -2147483520.0
    (0x4F00_0000, -2147483648, -2147483648, 2147483647), // 2147483648.0
    (0xCF00_0000, -2147483648, -2147483648, -2147483648), // -2147483648.0
    (0x4F00_0004, -2147483648, -2147483648, 2147483647), // 2147484672.0
    (0xCF00_0004, -2147483648, -2147483648, -2147483648), // -2147484672.0
    (0x4F80_0000, -2147483648, -2147483648, 2147483647), // 4294967296.0
    (0xCF80_0000, -2147483648, -2147483648, -2147483648), // -4294967296.0
    (0x7149_F2CA, -2147483648, -2147483648, 2147483647), // 1.0000000150474662e+30
    (0xF149_F2CA, -2147483648, -2147483648, -2147483648), // -1.0000000150474662e+30
    (0x7F80_0000, -2147483648, -2147483648, 2147483647), // inf
    (0xFF80_0000, -2147483648, -2147483648, -2147483648), // -inf
    (0x7FC0_0000, -2147483648, -2147483648, 0), // nan
    (0x0080_0000, 0, 0, 0), // 1.1754943508222875e-38
    (0x8080_0000, 0, -1, 0), // -1.1754943508222875e-38
    (0x0000_0001, 0, 0, 0), // 1.401298464324817e-45
    (0x8000_0001, 0, -1, 0), // -1.401298464324817e-45
    (0x0040_0000, 0, 0, 0), // 5.877471754111438e-39
    (0x8040_0000, 0, -1, 0), // -5.877471754111438e-39
];

/// The f64 rows: `(bits, to_i32_f64, what Rust's saturating `as` gives)`.
#[rustfmt::skip]
const FTOL_F64: [(u64, i32, i32); 11] = [
    (0x0000_0000_0000_0000, 0, 0), // 0.0
    (0x8000_0000_0000_0000, 0, 0), // -0.0
    (0x4004_0000_0000_0000, 2, 2), // 2.5
    (0xC004_0000_0000_0000, -2, -2), // -2.5
    (0x41DF_FFFF_FFE0_0000, 2147483647, 2147483647), // 2147483647.5
    (0xC1E0_0000_0010_0000, -2147483648, -2147483648), // -2147483648.5
    (0x7E37_E43C_8800_759C, -2147483648, 2147483647), // 1e+300
    (0xFE37_E43C_8800_759C, -2147483648, -2147483648), // -1e+300
    (0x7FF0_0000_0000_0000, -2147483648, 2147483647), // inf
    (0xFFF0_0000_0000_0000, -2147483648, -2147483648), // -inf
    (0x7FF8_0000_0000_0000, -2147483648, 0), // nan
];

#[test]
#[allow(clippy::cast_possible_truncation)] // the saturating cast is the thing being contrasted
fn float_to_int_matches_the_edge_table_including_where_rust_would_saturate() {
    let mut diverging = 0usize;
    for (bits, trunc, floored, rust_as) in FTOL_F32 {
        let x = f32::from_bits(bits);
        assert_eq!(to_i32(x), trunc, "to_i32({x:e}) [{bits:#010X}]");
        assert_eq!(
            floor_to_i32(x),
            floored,
            "floor_to_i32({x:e}) [{bits:#010X}]"
        );
        assert_eq!(x as i32, rust_as, "Rust's own cast of {x:e}");
        if trunc != rust_as {
            diverging += 1;
        }
    }
    assert_eq!(
        diverging, 6,
        "the rows where a raw `as i32` is wrong: NaN, +inf and positive out-of-range"
    );
    for (bits, trunc, rust_as) in FTOL_F64 {
        let x = f64::from_bits(bits);
        assert_eq!(to_i32_f64(x), trunc, "to_i32_f64({x:e}) [{bits:#018X}]");
        assert_eq!(x as i32, rust_as, "Rust's own cast of {x:e}");
    }
}

/// Perlin tables rebuild from crt rand.
#[test]
fn perlin_tables_rebuild_from_crt_rand() {
    let mut r = CrtRand::new(0);
    let mut p = [0i32; 514];
    let mut g1 = [0f32; 514];
    let mut draws = 0u32;
    for i in 0..256usize {
        p[i] = i32::try_from(i).expect("0..255 fits");
        let v = i32::from(r.next_u16() & 0x1FF); // rand() % 512; the sign fixup is dead
        draws += 1;
        #[allow(clippy::cast_precision_loss)] // v - 256 is in -256..=255, exact in f32
        {
            g1[i] = (v - 256) as f32 * 0.003_906_25; // * 1/256
        }
    }
    for i in (1..256usize).rev() {
        let t = p[i];
        let j = usize::from(r.next_u16() & 0xFF); // rand() % 256
        draws += 1;
        p[i] = p[j];
        p[j] = t;
    }
    assert_eq!(draws, 511);
    for k in 0..258usize {
        g1[256 + k] = g1[k]; // forward, element-wise, overlapping
    }
    for k in 0..258usize {
        p[256 + k] = p[k];
    }

    assert_eq!(p[..8], [167, 95, 103, 144, 172, 151, 127, 72]);
    assert_eq!(
        digest_i32(&p),
        0x105A_35C9_FC41_D10D,
        "digest of the 514-entry permutation"
    );
    let g1_bits: Vec<u32> = g1.iter().map(|g| g.to_bits()).collect();
    assert_eq!(
        g1_bits[..4],
        [0xBF5A_0000, 0xBF59_0000, 0xBD20_0000, 0x3F05_0000]
    );
    assert_eq!(
        digest_u32(&g1_bits),
        0x5E78_FD74_077B_B9CA,
        "digest of the 514-entry gradient table"
    );

    assert_eq!(p[512], p[0]);
    assert_eq!(p[513], p[1]);
    let mut seen = [false; 256];
    for &x in &p[..256] {
        let idx = usize::try_from(x).expect("permutation entries are 0..255");
        assert!(
            !seen[idx],
            "p[0..256] is not a permutation: {x} appears twice"
        );
        seen[idx] = true;
    }
}
