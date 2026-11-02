//! An object's physics and weenie descriptions as the client keeps them: the values a field the
//! bits leave out holds, and what a copy holds.

use auberean_protocol::text::StringInfo;
use auberean_protocol::types::{PhysicsDesc, WeenieDesc};
use auberean_protocol::wire::{Field, Reader};

/// A physics description that sends nothing but its stamps.
fn bare_physics() -> PhysicsDesc {
    let mut b = vec![0u8; 4];
    b.extend_from_slice(&[1, 0, 2, 0]);
    b.extend_from_slice(&[3, 0, 4, 0, 5, 0, 6, 0]);
    let mut r = Reader::new(&b);
    PhysicsDesc::read(&mut r).expect("a physics description")
}

/// A physics description that leaves its speed, jump and head scales out has each at 1; it has
/// no place and hangs from nothing, and its four last stamps answer by number.
///
/// Behaviour: protocol.physics-desc.a-scale-left-out-is-one
#[test]
fn a_physics_description_without_its_scales_has_each_at_one() {
    let p = bare_physics();
    assert_eq!(
        (p.velocity_scale, p.jump_scale, p.head_x, p.head_z),
        (1.0, 1.0, 1.0, 1.0)
    );
    assert_eq!(p.position().cell, 0);
    assert_eq!(
        (p.parent_id(), p.location_id(), p.parent_instance_stamp()),
        (0, 0, 0)
    );
    assert_eq!((p.stamp(0), p.stamp(3)), (3, 6));
    assert_eq!(
        (p.instance_stamp, p.visual_ordering_stamp),
        (1, 2),
        "the two stamps sent first"
    );
}

/// A weenie description that sends no package and no quantity answers a quantity of 1 and a
/// package of all ones underneath its mask.
///
/// Behaviour: protocol.weenie-desc.a-new-one-has-a-quantity-of-one-and-no-package
#[test]
fn a_weenie_description_without_a_package_or_quantity_has_none_and_one() {
    let b = [0u8; 4];
    let w = WeenieDesc::read(&mut Reader::new(&b)).expect("a weenie description");
    assert_eq!((w.mask, w.package_id, w.numbers[0]), (0, u32::MAX, 1));
    assert_eq!(w.numbers[1..], [0; 12]);
}

/// A copy of a weenie description holds its package only when it has one that is not all ones
/// and its entity only when it is not 0, and marks its flag word held when any of the word's four
/// low flags is on; the rest is copied as it stands.
///
/// Behaviour: protocol.weenie-desc.a-copy-holds-only-what-the-original-has
#[test]
fn a_copied_weenie_description_holds_only_the_package_entity_and_flags_the_original_has() {
    let original = WeenieDesc {
        mask: WeenieDesc::PACKAGE | WeenieDesc::ENTITY | WeenieDesc::NAME,
        package_id: u32::MAX,
        entity_did: 0,
        bitfield: 0x4,
        name: StringInfo::literal("Drudge"),
        icon: 7,
        ..WeenieDesc::default()
    };
    let c = original.copy();
    assert_eq!(
        c.mask,
        WeenieDesc::NAME | WeenieDesc::BITFIELD,
        "{:#x}",
        c.mask
    );
    assert_eq!(
        (c.package_id, c.entity_did, c.bitfield, c.icon),
        (u32::MAX, 0, 0x4, 7)
    );
    assert_eq!(c.name, original.name);

    let held = WeenieDesc {
        mask: WeenieDesc::PACKAGE | WeenieDesc::ENTITY,
        package_id: 0x1234,
        entity_did: 0x4700_0001,
        bitfield: 0x10,
        ..WeenieDesc::default()
    };
    let mut into = original.copy();
    into.assign(&held);
    assert_eq!(into.mask, WeenieDesc::PACKAGE | WeenieDesc::ENTITY);
    assert_eq!((into.package_id, into.entity_did), (0x1234, 0x4700_0001));
    assert_eq!(into.name, StringInfo::default(), "everything is taken");

    into.set_package_id(u32::MAX);
    into.set_entity_did(0);
    assert_eq!(into.mask, 0, "all ones and 0 are none");
}
