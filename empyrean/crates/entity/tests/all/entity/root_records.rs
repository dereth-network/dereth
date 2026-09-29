//! Vectors: local character-create, object-description and entity record cases in this module
//! CharacterCreateInfo unpacks in ACE order; ObjDesc helpers; attack frame params hash; small
//! records; numerics.
//! Fixture: enum values and synthetic entity records.

#![allow(clippy::disallowed_methods)]

use empyrean_entity::enums::{
    HeritageGroup, MotionCommand, MotionStance, PropertyType, SkillAdvancementClass,
};
use empyrean_entity::linq_extensions::sum;
use empyrean_entity::models::{PropertiesAnimPart, PropertiesTextureMap};
use empyrean_entity::{
    AttackFrameParams, BinaryReader, CharacterCreateInfo, Frame, GenericPropertyId, ObjDesc,
    Quaternion, SpellBarPositions, Vector3,
};

fn u32s(buf: &mut Vec<u8>, vals: &[u32]) {
    for v in vals {
        buf.extend_from_slice(&v.to_le_bytes());
    }
}

/// A character-creation payload in ACE's `Unpack` order.
fn create_payload(name: &str) -> Vec<u8> {
    let mut b = Vec::new();
    u32s(&mut b, &[1, 2, 1]); // unknown constant, heritage, gender
    u32s(&mut b, &(1..=14).collect::<Vec<u32>>()); // appearance uints
    for h in [0.25f64, 0.5, 0.75, 1.0, 0.0, 0.125] {
        b.extend_from_slice(&h.to_le_bytes());
    }
    b.extend_from_slice(&(-1i32).to_le_bytes()); // template option
    u32s(&mut b, &[10, 20, 30, 40, 50, 60, 3, 7]); // abilities, slot, class
    u32s(&mut b, &[2, 1, 3]); // two skills: untrained, specialized
    let len = u16::try_from(name.len()).unwrap();
    b.extend_from_slice(&len.to_le_bytes());
    b.extend_from_slice(name.as_bytes());
    // Pad (2 + len) to a multiple of 4.
    let pad = (4 - (2 + name.len()) % 4) % 4;
    b.extend(std::iter::repeat_n(0xEE, pad));
    u32s(&mut b, &[2, 1, 0]); // start area, admin, sentinel
    b
}

#[test]
fn character_create_info_unpacks_in_ace_order() {
    let payload = create_payload("Abc");
    let mut info = CharacterCreateInfo::default();
    assert_eq!(info.unpack(&mut BinaryReader::new(&payload)), Some(()));
    assert_eq!(info.heritage, HeritageGroup(2));
    assert_eq!(info.gender, 1);
    assert_eq!(
        (info.appearance.eyes, info.appearance.footwear_color),
        (1, 14)
    );
    assert_eq!(
        (info.appearance.skin_hue, info.appearance.footwear_hue),
        (0.25, 0.125)
    );
    assert_eq!(info.template_option, -1);
    assert_eq!((info.strength_ability, info.self_ability), (10, 60));
    assert_eq!((info.character_slot, info.class_id), (3, 7));
    assert_eq!(
        info.skill_advancement_classes,
        [SkillAdvancementClass(1), SkillAdvancementClass(3)]
    );
    assert_eq!(info.name.as_deref(), Some("Abc"));
    assert_eq!(info.start_area, 2);
    assert!(info.is_admin);
    assert!(!info.is_sentinel);

    // A name of length 2: (2 + 2) is already a multiple of 4, no padding.
    let mut info = CharacterCreateInfo::default();
    assert_eq!(
        info.unpack(&mut BinaryReader::new(&create_payload("Ab"))),
        Some(())
    );
    assert_eq!((info.name.as_deref(), info.start_area), (Some("Ab"), 2));

    // Truncated: EndOfStreamException after the fields already read.
    let mut info = CharacterCreateInfo::default();
    assert_eq!(
        info.unpack(&mut BinaryReader::new(&payload[..payload.len() - 1])),
        None
    );
    assert_eq!(info.start_area, 2);
    assert!(info.is_admin);
}

#[test]
fn obj_desc_helpers() {
    let mut d = ObjDesc::default();
    let tm = PropertiesTextureMap {
        part_index: 1,
        old_texture: 2,
        new_texture: 3,
    };
    d.add_texture_change(tm.clone());
    d.add_texture_change(tm.clone());
    d.add_texture_change(PropertiesTextureMap {
        new_texture: 4,
        ..tm
    });
    assert_eq!(d.texture_changes.len(), 2);

    let ap = |index, animation_id| PropertiesAnimPart {
        index,
        animation_id,
    };
    d.add_anim_part_change(ap(1, 10));
    d.add_anim_part_change(ap(2, 20));
    d.add_anim_part_change(ap(1, 10)); // identical: removed and re-appended
    d.add_anim_part_change(ap(2, 21)); // same index, other id: kept alongside
    assert_eq!(d.anim_part_changes, [ap(2, 20), ap(1, 10), ap(2, 21)]);
}

#[test]
fn attack_frame_params_hash_matches_ace() {
    let p = AttackFrameParams::new(
        0x0900_0001,
        MotionStance(0x8000_003C),
        MotionCommand(0x1000_0058),
    );
    // ((0 * 397 ^ 0x09000001) * 397 ^ (int)0x8000003C) * 397 ^ 0x10000058, unchecked int math.
    assert_eq!(p.get_hash_code(), 1_627_561_765);
    assert!(p.equals(&AttackFrameParams::new(
        0x0900_0001,
        MotionStance(0x8000_003C),
        MotionCommand(0x1000_0058)
    )));
    assert!(!p.equals(&AttackFrameParams::new(
        0x0900_0002,
        MotionStance(0x8000_003C),
        MotionCommand(0x1000_0058)
    )));
}

#[test]
fn small_records() {
    let s = SpellBarPositions::new(1, 3, 1234);
    assert_eq!(
        (s.spell_bar_id, s.spell_bar_position_id, s.spell_id),
        (0, 2, 1234)
    );
    assert_eq!(SpellBarPositions::new(0, 0, 1).spell_bar_id, u32::MAX);

    assert_eq!(sum([1u64, 2, 3]), 6);
    assert_eq!(sum([u64::MAX, 2]), 1);
    assert_eq!(sum(std::iter::empty()), 0);

    let f = Frame::new();
    assert_eq!(
        (f.origin, f.orientation),
        (Vector3::ZERO, Quaternion::IDENTITY)
    );

    let g = GenericPropertyId::new(5, PropertyType(1));
    assert_eq!(
        g,
        GenericPropertyId {
            property_id: 5,
            property_type: PropertyType(1)
        }
    );
}

#[test]
fn numerics_follow_system_numerics() {
    // Transform of UnitY by a 90 degree yaw about Z (z = w = sqrt(0.5)).
    let h = std::f32::consts::FRAC_1_SQRT_2;
    let q = Quaternion::new(0.0, 0.0, h, h);
    let v = Vector3::transform(Vector3::UNIT_Y, q);
    assert!((v.x + 1.0).abs() < 1e-6 && v.y.abs() < 1e-6 && v.z == 0.0);
    // Hamilton product, left operand first: i * j = k, j * i = -k.
    let i = Quaternion::new(1.0, 0.0, 0.0, 0.0);
    let j = Quaternion::new(0.0, 1.0, 0.0, 0.0);
    assert_eq!(i * j, Quaternion::new(0.0, 0.0, 1.0, 0.0));
    assert_eq!(j * i, Quaternion::new(0.0, 0.0, -1.0, 0.0));
    // Yaw about Y, pitch about X, roll about Z.
    let y = Quaternion::create_from_yaw_pitch_roll(std::f32::consts::PI, 0.0, 0.0);
    assert_eq!((y.x, y.z), (0.0, 0.0));
    assert_eq!(y.y, (std::f32::consts::PI * 0.5).sin());
    let p = Quaternion::create_from_yaw_pitch_roll(0.0, 1.0, 0.0);
    assert_eq!((p.x, p.w), (0.5f32.sin(), 0.5f32.cos()));
    let bits = |q: Quaternion| [q.x.to_bits(), q.y.to_bits(), q.z.to_bits(), q.w.to_bits()];
    let m1 = Quaternion::new(0.1, 0.2, 0.3, 0.9);
    let m2 = Quaternion::new(-0.4, 0.5, 0.25, 0.7);
    assert_eq!(
        bits(m1 * m2),
        [0xBEC7_AE14, 0x3EE3_D70A, 0x3F10_A3D8, 0x3EFD_70A4]
    );
    assert_eq!(
        bits(m2 * m1),
        [0xBE42_8F5B, 0x3F3C_28F5, 0x3E9C_28F6, 0x3EFD_70A4]
    );
    let v = Vector3::new(1.5, -2.25, 3.125);
    let t = Vector3::transform(v, m2);
    assert_eq!(
        [t.x.to_bits(), t.y.to_bits(), t.z.to_bits()],
        [0x4070_6666, 0x3FA5_5C29, 0xBE54_28F0]
    );
    let n = Vector3::normalize(v);
    assert_eq!(
        [n.x.to_bits(), n.y.to_bits(), n.z.to_bits()],
        [0x3EB9_D740, 0xBF0B_6170, 0x3F41_958E]
    );
    // Equals treats NaN as equal; == does not.
    let n = Vector3::new(f32::NAN, 0.0, 0.0);
    assert!(n.equals(n));
    assert_ne!(n, n);
}
