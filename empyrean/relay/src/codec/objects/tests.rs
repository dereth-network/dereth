//! Behaviour: none (independent synthetic ac1999 object vectors and admission bounds).

use super::*;
use dereth_protocol::{read_checked, write_blob};

fn word(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend(value.to_le_bytes());
}
fn short(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend(value.to_le_bytes());
}
fn align_bytes(bytes: &mut Vec<u8>) {
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
}

fn vector() -> Vec<u8> {
    let mut b = vec![0x45, 0xf7, 0, 0, 10, 0, 0, 0];
    // Every fixed asset id has its high bit set, which is not a continuation flag here.
    b.extend([
        0x11, 1, 1, 1, 1, 0x90, 2, 0x90, 3, 0, 4, 3, 0x90, 4, 0x90, 5, 5, 0x90,
    ]);
    align_bytes(&mut b);
    assert_eq!(b.len(), 28);
    word(&mut b, 0x8001);
    word(&mut b, 0);
    word(&mut b, 0x0101_0001);
    for value in [1.0f32, 2.0, 3.0, 1.0, 0.0, 0.0, 0.0] {
        b.extend(value.to_le_bytes());
    }
    word(&mut b, 0x0200_0001);
    for stamp in 1..=9 {
        short(&mut b, stamp);
    }
    align_bytes(&mut b);
    assert_eq!(b.len(), 92);
    word(&mut b, 0x00ff_ffbf);
    short(&mut b, 0); // No alignment after an empty name or before fixed public fields.
    short(&mut b, 0x9006);
    short(&mut b, 0x9007);
    word(&mut b, 16);
    word(&mut b, 8);
    b.extend([2, 0, b'A', 0]); // Plural name, followed by absolute alignment.
    align_bytes(&mut b);
    b.extend([102, 7]);
    word(&mut b, 300); // Value.
    word(&mut b, 8); // Useability.
    b.extend(1.5f32.to_le_bytes());
    word(&mut b, 16); // Target before effects, not ascending bit order.
    word(&mut b, 32);
    b.extend([3, 4]); // Byte ammo and combat fields.
    for value in [5, 6, 7, 8] {
        short(&mut b, value);
    }
    for value in [20, 30, 40, 50, 60] {
        word(&mut b, value);
    }
    b.extend([2, 3]); // Blip/radar, before the vendor and spell words.
    short(&mut b, 0x9008);
    short(&mut b, 123);
    align_bytes(&mut b);
    b
}

#[test]
fn the_ac1999_object_ids_strings_field_widths_and_vendor_semantics_match_the_independent_vector() {
    let bytes = vector();
    let value = read_checked::<CreateObject>(&bytes).unwrap();
    let p = &value.object;
    assert_eq!(p.id, ObjectId(10));
    assert_eq!(p.objdesc.palette_id, 0x0400_9001);
    assert_eq!(p.objdesc.subpalettes[0].sub_id, 0x0400_9002);
    assert_eq!(p.objdesc.subpalettes[0].num_colors, 0);
    assert_eq!(p.objdesc.texture_changes[0].new_tex_id, 0x0500_9004);
    assert_eq!(p.objdesc.anim_part_changes[0].part_id, 0x0100_9005);
    assert_eq!(p.physicsdesc.timestamps.instance, 9);
    assert_eq!(p.physicsdesc.position.unwrap().frame.origin.y, 2.0);
    assert_eq!((p.wdesc.wcid, p.wdesc.icon_id), (0x9006, 0x0600_9007));
    assert_eq!(p.wdesc.name, "");
    assert_eq!(p.wdesc.plural_name.as_deref(), Some("A"));
    assert_eq!(p.wdesc.ammo_type, Some(3));
    assert_eq!(p.wdesc.target_type, Some(16));
    assert_eq!(p.wdesc.radar_enum, Some(3));
    assert_eq!(p.wdesc.spell_id, Some(123));
    assert_eq!(value.vendor_class, Some(0x9008));
    assert_eq!(p.wdesc.burden, None);
    assert_eq!(p.wdesc.header & VENDOR_CLASS, 0);
    assert_eq!(write_blob(&value).unwrap(), bytes);
    assert!(read_checked::<dereth_protocol::objects::ItemCreateObject>(&bytes).is_err());
}

#[test]
fn the_ac1999_object_rejects_truncation_overlong_lists_and_nonfinite_geometry() {
    let bytes = vector();
    for end in 0..bytes.len() {
        assert!(
            read_checked::<CreateObject>(&bytes[..end]).is_err(),
            "{end}"
        );
    }
    // A child count larger than the bytes left.
    let mut bad = bytes.clone();
    bad[28..32].copy_from_slice(&(0x8001u32 | 0x40).to_le_bytes());
    assert!(read_checked::<CreateObject>(&bad).is_err());
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut bad = bytes.clone();
        bad[40..44].copy_from_slice(&value.to_le_bytes());
        assert!(read_checked::<CreateObject>(&bad).is_err());
    }
    let mut zero_rotation = bytes.clone();
    zero_rotation[52..68].fill(0);
    assert!(read_checked::<CreateObject>(&zero_rotation).is_err());
    let mut unknown_public = bytes.clone();
    unknown_public[92] |= 0x40;
    assert!(read_checked::<CreateObject>(&unknown_public).is_err());
    let mut trailing = bytes;
    trailing.push(0);
    assert!(read_checked::<CreateObject>(&trailing).is_err());
}

/// The vector's physics section replaced by one with a movement buffer, a child link and the bits
/// the client reads no field for: translucency, the timestamp flag, and an animation frame beside
/// a movement buffer.
fn moving_vector() -> Vec<u8> {
    let original = vector();
    let mut b = original[..28].to_vec();
    word(
        &mut b,
        0x8001 | 0x1_0000 | 0x40 | 0x4_0000 | 0x2_0000 | 0x400,
    );
    word(&mut b, 0);
    word(&mut b, 4);
    b.extend([1, 2, 3, 4]);
    word(&mut b, 1);
    b.extend(&original[36..68]);
    word(&mut b, 0x0200_0001);
    word(&mut b, 1);
    word(&mut b, 11);
    word(&mut b, 2);
    for stamp in 1..=9 {
        short(&mut b, stamp);
    }
    b.extend([7, 7]); // Padding the client does not check.
    b.extend(&original[92..]);
    b
}

#[test]
fn the_ac1999_object_reads_movement_children_and_ignores_fields_the_client_has_no_reader_for() {
    let bytes = moving_vector();
    let value = read_checked::<CreateObject>(&bytes).unwrap();
    let p = &value.object.physicsdesc;
    assert_eq!(p.movement, Some((vec![1, 2, 3, 4], 1)));
    assert_eq!(p.animframe_id, None);
    assert_eq!(p.translucency, None);
    assert_eq!(p.bitfield, 0x8001 | 0x1_0000 | 0x40);
    let children = p.children.as_ref().unwrap();
    assert_eq!(
        (children[0].child_id, children[0].location_id),
        (ObjectId(11), 2)
    );
    assert_eq!(p.setup_id, Some(0x0200_0001));
    assert_eq!(p.timestamps.instance, 9);
    assert_eq!(value.object.wdesc.spell_id, Some(123));
    // Written back with only the fields read, and zero padding.
    let written = write_blob(&value).unwrap();
    assert_eq!(read_checked::<CreateObject>(&written).unwrap(), value);
    let mut appearance_padding = vector();
    appearance_padding[26] = 1;
    assert!(read_checked::<CreateObject>(&appearance_padding).is_ok());
}

#[test]
fn the_ac1999_object_encoder_refuses_lossy_widths_and_mismatched_optional_fields() {
    let original = read_checked::<CreateObject>(&vector()).unwrap();
    let mut value = original.clone();
    value.object.wdesc.wcid = 0x1_0000;
    assert!(write_blob(&value).is_err());
    value = original.clone();
    value.object.wdesc.ammo_type = Some(256);
    assert!(write_blob(&value).is_err());
    value = original.clone();
    value.object.objdesc.palette_id = 0x0401_0000;
    assert!(write_blob(&value).is_err());
    value = original.clone();
    value.object.wdesc.stack_size = None;
    assert!(write_blob(&value).is_err());
    value = original;
    value.object.physicsdesc.setup_id = None;
    assert!(write_blob(&value).is_err());
}
