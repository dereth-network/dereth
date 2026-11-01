//! An iteration list with data versions below zero, as every 2005 file holds one, read back from a
//! small file built in memory.

use auberean_dat::container::ITERATION_LIST;
use auberean_dat::DatFile;

use crate::container::build;

/// The 2005 portal's list in shape: a run from -2005 to -2000, a run from -1014 to -1000, then its
/// positive versions from 1 to 4,862, each run written as `-length, first`.
fn list_of_2005() -> Vec<u8> {
    let positive = 4_862i32;
    [6 + 15 + positive, -6, -2005, -15, -1014, -positive, 1]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect()
}

/// Behaviour: data.container.iteration-list-holds-negative-versions
#[test]
fn versions_below_zero_read_back_negative_below_the_positive_run() {
    let dat = DatFile::from_bytes(build(&[(ITERATION_LIST, list_of_2005())], 0)).unwrap();
    let it = dat.iterations().unwrap();
    let negative: Vec<i32> = it.iter().copied().filter(|v| *v < 0).collect();
    let lowest_positive = it.iter().copied().filter(|v| *v > 0).min().unwrap();
    assert_eq!(it.len(), 4_883);
    assert_eq!(negative.len(), 21);
    assert_eq!(negative.first().copied(), Some(-2005));
    assert!(negative.iter().all(|v| *v < lowest_positive));
    assert_eq!(it.iter().max().copied(), Some(4_862));
}
