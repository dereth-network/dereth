//! Behaviour: none (file-format decoders: the character-creation tables)
//!
//! A hand-built portal holding a small creation table, skill table, clothing table, string record,
//! palettes, a palette set and two face textures, read back whole: every kept field, both string
//! shapes, the clothing colours a sex offers, the face textures and the palettes they need.

use dereth_classic_dat::creation::{self, CreationData};
use dereth_classic_dat::ClassicPortal;

use crate::build::{container, indexed, palette, Enc};

const CLOTHING: u32 = 0x1000_0001;

fn creation_table() -> Vec<u8> {
    let mut e = Enc::default();
    e.u32(0x0E00_0002).words(&[1, 2, 3, 4, 5, 6, 7, 8]);
    // One starting area with one position.
    e.u32(1)
        .text("Holtburg")
        .u32(1)
        .u32(0xA9B4_0001)
        .bytes(&[0; 28]);
    // One heritage; its name in the padded legacy shape.
    e.u32(1)
        .legacy_text("Aluvian")
        .words(&[0x0600_0001, 0x0200_0001, 0x3100_0001, 0x0E00_0010]);
    e.list(&[0]).list(&[]);
    e.u32(1); // one sex
    e.text("Male")
        .words(&[0x0200_0002, 0x2000_0001, 0x0600_0002, 0x3100_0002]);
    e.objdesc(None);
    e.u32(0x0200_0003).u32(0x0500_0003);
    e.u32(1).text("unused");
    e.u32(1).text("first").text("second").u32(9);
    e.u32(50).u32(0xFFFF_FFFF).u32(40);
    e.u32(1).named("Shield", 0x0600_0004, 0);
    e.u32(1).u32(6).u32(2).u32(4);
    // One template with one profile.
    e.u32(1).named("Warrior", 0x0600_0005, 0x3100_0003);
    e.u32(1)
        .words(&[10, 20, 30, 40, 50, 60])
        .list(&[6])
        .list(&[])
        .list(&[7]);
    e.u32(1).words(&(0..17).collect::<Vec<u32>>());
    e.u32(0x0400_0001).u32(0x0F00_0001);
    e.list(&[0x0F00_0001]);
    e.u32(1).u32(0x0600_0006).u32(0x0600_0007).objdesc(None);
    e.list(&[0x0400_0002]);
    e.u32(1)
        .u32(0x0600_0008)
        .u32(0x0600_0009)
        .objdesc(Some(0x0010))
        .objdesc(None);
    e.u32(1).u32(0x0600_000A).objdesc(Some(0x0011));
    e.u32(0); // no mouths
    e.u32(1).named("Cap", CLOTHING, 0x0600_000B);
    e.u32(0).u32(0).u32(0);
    e.u32(2).u32(5).words(&[3, 1]);
    e.0
}

fn skill_table() -> Vec<u8> {
    let mut e = Enc::default();
    e.u32(0x0E00_0004).u32(0x0001_0001);
    e.u32(6)
        .legacy_text("Helps you block.")
        .legacy_text("Melee Defense")
        .words(&[0x0600_000C, 10, 20, 1, 1, 0])
        .words(&[1, 3, 0, 0, 0, 0])
        .f64(1.0)
        .f64(0.0)
        .f64(0.5);
    e.0
}

fn clothing_table() -> Vec<u8> {
    let mut e = Enc::default();
    e.u32(CLOTHING)
        .u32(1)
        .u32(0x0200_0002)
        .u32(1)
        .u32(0)
        .u32(0)
        .u32(1)
        .u32(0)
        .u32(0);
    e.u32(3);
    e.u32(3)
        .u32(0x0600_0103)
        .u32(1)
        .u32(1)
        .u32(0)
        .u32(8)
        .u32(0x0F00_0001);
    e.u32(9).u32(0x0600_0109).u32(0);
    e.u32(1).u32(0x0600_0101).u32(0);
    e.0
}

fn portal() -> ClassicPortal {
    let mut set = Enc::default();
    set.u32(0x0F00_0001).list(&[0x0400_0004]);
    let mut help = Enc::default();
    help.u32(0x3100_0002).legacy_text("Choose a name.");
    let records = vec![
        (0x0E00_0002, creation_table()),
        (0x0E00_0004, skill_table()),
        (CLOTHING, clothing_table()),
        (0x3100_0002, help.0),
        (0x0F00_0001, set.0),
        (0x0400_0001, palette(0x0400_0001)),
        (0x0400_0002, palette(0x0400_0002)),
        (0x0400_0003, palette(0x0400_0003)),
        (0x0400_0004, palette(0x0400_0004)),
        (
            0x0500_0010,
            indexed(0x0500_0010, 2, 2, &[1, 2, 3, 4], 0x0400_0003),
        ),
        (0x0500_0011, indexed(0x0500_0011, 1, 1, &[5], 0x0400_0003)),
    ];
    ClassicPortal::from_bytes(container(256, 1, &records, 20)).unwrap()
}

#[test]
fn the_creation_table_decodes_every_kept_field() {
    let data = creation::decode_table(&creation_table()).unwrap();
    assert_eq!(data.help_ids, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(data.areas.len(), 1);
    assert_eq!(data.areas[0].name, "Holtburg");
    let h = &data.heritages[0];
    assert_eq!(h.name, "Aluvian");
    assert_eq!(
        (h.icon, h.setup, h.description, h.environment),
        (0x0600_0001, 0x0200_0001, 0x3100_0001, 0x0E00_0010)
    );
    assert_eq!(
        (h.primary_areas.clone(), h.secondary_areas.len()),
        (vec![0], 0)
    );
    let s = &h.sexes[0];
    assert_eq!(s.name, "Male");
    assert_eq!(s.appearance, "11000000");
    assert_eq!((s.legacy_18, s.legacy_1c), (0x0200_0003, 0x0500_0003));
    assert_eq!(
        (s.attribute_credits, s.legacy_60, s.skill_credits),
        (50, -1, 40)
    );
    assert_eq!(s.legacy_68[0].name, "Shield");
    assert_eq!(
        (
            s.skill_discounts[0].skill,
            s.skill_discounts[0].trained,
            s.skill_discounts[0].specialized
        ),
        (6, 2, 4)
    );
    let t = &s.templates[0];
    assert_eq!((t.name.as_str(), t.resource), ("Warrior", 0x3100_0003));
    assert_eq!(t.profiles[0].attributes, [10, 20, 30, 40, 50, 60]);
    assert_eq!(t.profiles[0].trained, vec![6]);
    assert_eq!(t.profiles[0].legacy_third, vec![7]);
    assert_eq!(s.legacy_80, vec![(0..17).collect::<Vec<u32>>()]);
    assert_eq!((s.base_palette, s.skin_palette), (0x0400_0001, 0x0F00_0001));
    assert_eq!(s.hair_styles[0].bald, 0x0600_0007);
    assert_eq!(s.eyes[0].appearance, "110001000001001000000000");
    assert_eq!(s.eyes[0].bald_appearance, "11000000");
    assert_eq!(s.noses.len(), 1);
    assert!(s.mouths.is_empty());
    assert_eq!(s.headgear[0].icon, CLOTHING);
    assert_eq!(s.clothing_colors, vec![3, 1]);
}

#[test]
fn a_creation_table_with_bytes_left_over_is_refused() {
    let mut data = creation_table();
    data.push(0);
    assert!(creation::decode_table(&data).is_err());
    assert!(creation::decode_table(&data[..data.len() - 9]).is_err());
}

#[test]
fn the_skill_table_decodes_names_costs_and_formula() {
    let skills = creation::decode_skills(&skill_table()).unwrap();
    assert_eq!(skills.len(), 1);
    let s = &skills[0];
    assert_eq!((s.id, s.name.as_str()), (6, "Melee Defense"));
    assert_eq!(s.description, "Helps you block.");
    assert_eq!(
        (s.trained, s.specialized, s.chargen, s.min_level),
        (10, 20, 1, 0)
    );
    assert_eq!(s.formula, [1, 3, 0, 0, 0, 0]);
}

#[test]
fn reading_the_portal_fills_clothing_colours_face_textures_help_text_and_palettes() {
    let portal = portal();
    let data = creation::read(&portal).unwrap();
    let s = &data.heritages[0].sexes[0];
    let colors: Vec<(u32, u32, u32)> = s.headgear[0]
        .colors
        .iter()
        .map(|c| (c.key, c.icon, c.palette_set))
        .collect();
    assert_eq!(
        colors,
        vec![(1, 0x0600_0101, 0), (3, 0x0600_0103, 0x0F00_0001)]
    );
    assert_eq!(s.eyes[0].texture, "05000010-mirror");
    assert_eq!(s.eyes[0].bald_texture, "");
    assert_eq!(s.noses[0].texture, "05000011");
    assert_eq!(s.hair_styles[0].texture, "");
    assert_eq!(data.skills.len(), 1);
    assert_eq!(
        data.help_text.get("822083586").map(String::as_str),
        Some("Choose a name.")
    );
    let palettes: Vec<&str> = data
        .appearance
        .palettes
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(palettes, ["04000001", "04000002", "04000003", "04000004"]);
    assert_eq!(
        data.appearance.palette_sets.get("0F000001"),
        Some(&vec![0x0400_0004])
    );

    let assets = creation::indexed_assets(&portal, &data).unwrap();
    let keys: Vec<&str> = assets.textures.keys().map(String::as_str).collect();
    assert_eq!(keys, ["05000010", "05000010-mirror", "05000011"]);
    assert_eq!(
        assets.textures["05000010-mirror"].indices,
        vec![1, 2, 2, 1, 3, 4, 4, 3]
    );
    assert_eq!(assets.textures["05000011"].palette, 0x0400_0003);
}

#[test]
fn a_clothing_colour_keeps_its_first_sub_palettes_palette_set() {
    let mut e = Enc::default();
    e.u32(CLOTHING).u32(0);
    e.u32(2);
    // Two sub-palettes: the first one's set is the colour's.
    e.u32(5).u32(0x0600_0105).u32(2);
    e.u32(1).u32(0).u32(8).u32(0x0F00_0005);
    e.u32(2).u32(8).u32(8).u32(16).u32(8).u32(0x0F00_0006);
    // No sub-palettes: no set.
    e.u32(6).u32(0x0600_0106).u32(0);
    let colors = creation::clothing_colors(&e.0).unwrap();
    let got: Vec<(u32, u32)> = colors.iter().map(|c| (c.key, c.palette_set)).collect();
    assert_eq!(got, vec![(5, 0x0F00_0005), (6, 0)]);
}

#[test]
fn a_clothing_colour_without_a_palette_set_reads_from_older_json() {
    let color: creation::ClothingColor =
        serde_json::from_str(r#"{"key":3,"icon":100663555}"#).unwrap();
    assert_eq!((color.key, color.palette_set), (3, 0));
}

#[test]
fn creation_data_survives_a_json_round_trip() {
    let data = creation::read(&portal()).unwrap();
    let json = serde_json::to_value(&data).unwrap();
    let back: CreationData = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&back).unwrap(), json);
}

#[test]
fn a_string_record_must_echo_its_id() {
    let mut e = Enc::default();
    e.u32(0x3100_0005).legacy_text("Hi");
    assert_eq!(creation::decode_string(0x3100_0005, &e.0).unwrap(), "Hi");
    assert!(creation::decode_string(0x3100_0006, &e.0).is_err());
}
