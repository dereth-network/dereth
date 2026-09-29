//! ACE: Source/ACE.Entity/ObjectGuid.cs::ObjectGuid
//! ObjectGuid ranges classify at their edges; low/high/display.
//! Fixture: enum values and synthetic entity records.

use empyrean_entity::{GuidType, ObjectGuid};

#[test]
fn ranges_classify_at_their_edges() {
    let cases = [
        (0x0000_0000, GuidType::Undef),
        (0x5000_0000, GuidType::Undef),
        (0x5000_0001, GuidType::Player),
        (0x5FFF_FFFF, GuidType::Player),
        (0x6000_0000, GuidType::Undef),
        (0x6FFF_FFFF, GuidType::Undef),
        (0x7000_0000, GuidType::Static),
        (0x7FFF_FFFF, GuidType::Static),
        (0x8000_0000, GuidType::Dynamic),
        (0xFFFF_FFFE, GuidType::Dynamic),
        (0xFFFF_FFFF, GuidType::Undef),
    ];
    for (full, want) in cases {
        assert_eq!(ObjectGuid::new(full).guid_type(), want, "{full:08X}");
    }
}

#[test]
fn low_high_and_display() {
    let g = ObjectGuid::new(0x7AAB_B001);
    assert_eq!(g.low(), 0xAB_B001);
    assert_eq!(g.high(), 0x7A);
    assert_eq!(g.to_string(), "7AABB001");
    assert_eq!(ObjectGuid::new(0x1F).to_string(), "0000001F");
    assert!(g.is_static() && !g.is_player() && !g.is_dynamic());
    assert_eq!(ObjectGuid::INVALID.full(), 0);
}
