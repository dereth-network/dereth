//! The position hash and the split rule built on it.

use auberean_world::random::{hash16, scaled, sw_to_ne_cut, SPLIT_SEED};

/// The hash's sixteen bits agree with an independent transcription of the expression, evaluated
/// with unbounded integers and reduced to 32 bits, at points spread over the world and over the
/// seeds the landscape uses.
///
/// Behaviour: world.hash.the-position-hash-matches-its-transcription
#[test]
fn the_position_hash_matches_an_independent_transcription() {
    // (x, y, seed, sixteen bits) from the transcription.
    const CASES: &[(i32, i32, i32, u32)] = &[
        (0, 0, 0x02F1_0087, 0xAE22),
        (1, 0, 0x02F1_0087, 0x42E6),
        (0, 1, 0x02F1_0087, 0x63BD),
        (1234, 567, 0x02F1_0087, 0xB2BF),
        (4095, 4095, 0x02F1_0087, 0x00C6),
        (2035, 2475, 0x003C_47F7, 0x6A8A),
        (100, 200, 9_348_347, 0x4BDC),
        (2000, 30, 0x05DD_39FD, 0x766B),
        (7, 3, 0x0000_2473, 0xA1D9),
    ];
    for &(x, y, s, want) in CASES {
        assert_eq!(hash16(x, y, s), want, "({x}, {y}) seed {s:#x}");
    }
    assert_eq!(
        scaled(0, 0, SPLIT_SEED, 2.0),
        0xAE22 as f32 * 2.0 / 65_536.0
    );
}

/// The split rule is the hash at the split seed compared with one half, which is the same as the
/// top bit of the expression with the seed folded in: `((y·x²·K − K4)·y − x·K3) + K5`. The two
/// spellings agree at every seventh column and eleventh row of a 4096-square world.
///
/// Behaviour: world.terrain.a-square-is-split-by-the-hash-of-its-world-position
#[test]
fn the_split_is_the_folded_hash_of_the_global_square() {
    let folded = |x: i32, y: i32| -> bool {
        let (x, y) = (x as u32, y as u32);
        let v = y
            .wrapping_mul(x)
            .wrapping_mul(x)
            .wrapping_mul(0x3955_7FCF)
            .wrapping_sub(0x4A65_06DB)
            .wrapping_mul(y)
            .wrapping_sub(x.wrapping_mul(0x6B3C_5F11))
            .wrapping_add(0xAE22_6CEB);
        v >> 31 == 1
    };
    let mut cut = 0;
    for x in (0..4096).step_by(7) {
        for y in (0..4096).step_by(11) {
            assert_eq!(sw_to_ne_cut(x, y), folded(x, y), "({x}, {y})");
            cut += usize::from(sw_to_ne_cut(x, y));
        }
    }
    assert_eq!(
        cut, 109_297,
        "the transcription's count of south-west to north-east cuts"
    );
}
