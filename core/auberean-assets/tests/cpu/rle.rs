//! The run-length form: runs, single numbers, and what is refused.

use auberean_assets::rle::{decode, encode};

/// Behaviour: data.run-length.a-run-unpacks-to-its-rising-numbers
#[test]
fn a_run_unpacks_to_its_rising_numbers_and_packs_back() {
    assert_eq!(decode(&[3, 0x8005, 3, 9]), vec![3, 5, 6, 7, 9]);
    assert_eq!(encode(&[3, 5, 6, 7, 9]), Some(vec![3, 0x8005, 3, 9]));
    assert_eq!(
        encode(&[4, 4]),
        Some(vec![4, 4]),
        "equal numbers are no run"
    );
    assert_eq!(encode(&[5, 6]), Some(vec![0x8005, 2]), "two make a run");
    assert_eq!(encode(&[]), Some(vec![]));
    assert_eq!(decode(&[0x8001, 0]), Vec::<u16>::new(), "a run of none");
    assert_eq!(
        decode(&[0x8001, 0xFFFF]),
        Vec::<u16>::new(),
        "a length below zero"
    );
    assert_eq!(decode(&[0x8001]), Vec::<u16>::new(), "no length follows");
    assert_eq!(encode(&[0x7FFF]), None, "the largest number");
    // Every number on its own (each two apart), one too many for the form's limit.
    let spaced: Vec<u16> = (0..0x7FFFu32)
        .map(|n| u16::try_from(n % 2 * 2).unwrap())
        .collect();
    assert_eq!(encode(&spaced[..0x7FFE]).map(|v| v.len()), Some(0x7FFE));
    assert_eq!(encode(&spaced), None);
}
