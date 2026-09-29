//! ACE: Source/ACE.Entity/LandblockId.cs::LandblockId
//! LandblockId equality, parts, indoors, byte-cast wrap, neighbours/adjacency, transitions stop
//! at 254.
//! Fixture: enum values and synthetic entity records.

use std::collections::HashSet;

use empyrean_entity::LandblockId;

#[test]
fn equality_compares_the_landblock_only() {
    let a = LandblockId::new(0xA9B4_0001);
    let b = LandblockId::new(0xA9B4_0123);
    let c = LandblockId::new(0xAAB4_0001);
    assert_eq!(a, b);
    assert_ne!(a, c);
    // Hash agrees with Eq (the divergence from ACE's GetHashCode).
    let set: HashSet<LandblockId> = [a, b, c].into_iter().collect();
    assert_eq!(set.len(), 2);
}

#[test]
fn parts_and_indoors() {
    let id = LandblockId::new(0xA9B4_0123);
    assert_eq!(id.raw(), 0xA9B4_0123);
    assert_eq!(id.landblock(), 0xA9B4);
    assert_eq!(id.landblock_x(), 0xA9);
    assert_eq!(id.landblock_y(), 0xB4);
    assert!(id.indoors());
    assert!(!LandblockId::new(0xA9B4_00FF).indoors());
    assert!(LandblockId::new(0xA9B4_0100).indoors());
    assert_eq!(LandblockId::from_xy(0x12, 0x34).raw(), 0x1234_0000);
    assert_eq!(id.to_string(), "A9B40123");
}

#[test]
fn landcell_wraps_through_the_byte_cast() {
    // Cell 0x0A: (0x0A & 0x3F) - 1 = 9 -> x 1, y 1.
    let id = LandblockId::new(0xA9B4_000A);
    assert_eq!(id.landcell(), 9);
    assert_eq!((id.landcell_x(), id.landcell_y()), (1, 1));
    // Cell 0: (0 - 1) as uint = 0xFFFFFFFF, (byte) = 255 -> x 7, y 7.
    let zero = LandblockId::new(0xA9B4_0000);
    assert_eq!(zero.landcell(), 255);
    assert_eq!((zero.landcell_x(), zero.landcell_y()), (7, 7));
}

#[test]
fn neighbours_and_adjacency() {
    let id = LandblockId::new(0xA9B4_0001);
    assert_eq!(id.east().raw(), 0xAAB4_0000);
    assert_eq!(id.west().raw(), 0xA8B4_0000);
    assert_eq!(id.north().raw(), 0xA9B5_0000);
    assert_eq!(id.south().raw(), 0xA9B3_0000);
    assert_eq!(id.north_east().raw(), 0xAAB5_0000);
    assert_eq!(id.north_west().raw(), 0xA8B5_0000);
    assert_eq!(id.south_east().raw(), 0xAAB3_0000);
    assert_eq!(id.south_west().raw(), 0xA8B3_0000);
    assert!(id.is_adjacent_to(LandblockId::new(0xAAB5_0100)));
    assert!(id.is_adjacent_to(id));
    assert!(!id.is_adjacent_to(LandblockId::new(0xABB4_0000)));
}

#[test]
#[should_panic(expected = "OverflowException")]
fn east_of_the_last_column_throws() {
    let _ = LandblockId::new(0xFF00_0000).east();
}

#[test]
#[should_panic(expected = "OverflowException")]
fn south_of_the_first_row_throws() {
    let _ = LandblockId::new(0x0000_0000).south();
}

#[test]
fn transitions_keep_the_cell_and_stop_at_254() {
    let id = LandblockId::new(0xA9B4_0012);
    assert_eq!(id.transition_x(1).map(LandblockId::raw), Some(0xAAB4_0012));
    assert_eq!(
        id.transition_x(-0xA9).map(LandblockId::raw),
        Some(0x00B4_0012)
    );
    assert_eq!(id.transition_x(-0xAA), None);
    assert_eq!(LandblockId::new(0xFE00_0000).transition_x(1), None);
    assert_eq!(id.transition_y(-2).map(LandblockId::raw), Some(0xA9B2_0012));
    assert_eq!(LandblockId::new(0x00FE_0001).transition_y(1), None);
    assert_eq!(
        LandblockId::new(0x00FD_0001)
            .transition_y(1)
            .map(LandblockId::raw),
        Some(0x00FE_0001)
    );
}
