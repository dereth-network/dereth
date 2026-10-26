//! A known weenie's look, worked out from its world database rows and the retail dats, is the
//! object description the retail server sent for the same class in the recorded sessions.
//!
//! Fixture: the retail dats in `$DERETH_TEST_DAT_DIR` and the recorded sessions'
//! messages (`fixtures/message-corpus`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use dereth_protocol::types::ObjDesc;
use dereth_protocol::Message;
use lyceum_editor::look::{look_of, DatFiles};
use lyceum_foundry::content::SqlDoc;

/// A class's look as ACE's world database has it.
struct Known {
    class: u32,
    weenie_type: u32,
    setup: u32,
    palette: u32,
    clothing: u32,
    scheme: i32,
    shade: Option<f64>,
}

const fn known(
    class: u32,
    weenie_type: u32,
    setup: u32,
    palette: u32,
    clothing: u32,
    scheme: i32,
    shade: Option<f64>,
) -> Known {
    Known {
        class,
        weenie_type,
        setup,
        palette,
        clothing,
        scheme,
        shade,
    }
}

/// Creatures, a pack, a sack, ammunition, weapons and a helm, whose looks the server never
/// varies.
const CLASSES: &[Known] = &[
    known(
        184,
        10,
        0x0200_0E08,
        0x0400_1425,
        0x1000_0412,
        17,
        Some(0.5),
    ),
    known(197, 10, 0x0200_07CA, 0x0400_0F69, 0x1000_020D, 9, Some(0.5)),
    known(
        213,
        10,
        0x0200_0AAB,
        0x0400_1114,
        0x1000_02E4,
        82,
        Some(0.5),
    ),
    known(
        217,
        10,
        0x0200_1121,
        0x0400_18FE,
        0x1000_0564,
        21,
        Some(0.5),
    ),
    known(93, 1, 0x0200_0162, 0x0400_0BEF, 0x1000_015E, 4, None),
    known(136, 21, 0x0200_0151, 0x0400_0BEF, 0x1000_019B, 21, None),
    known(166, 21, 0x0200_0181, 0x0400_0BEF, 0x1000_0179, 21, None),
    known(300, 5, 0x0200_0124, 0x0400_0BEF, 0x1000_034F, 4, None),
    known(301, 6, 0x0200_0125, 0x0400_0BEF, 0x1000_0143, 20, None),
    known(303, 6, 0x0200_0127, 0x0400_0BEF, 0x1000_017D, 20, None),
    known(307, 3, 0x0200_0129, 0x0400_0BEF, 0x1000_0130, 20, None),
    known(331, 6, 0x0200_013A, 0x0400_0BEF, 0x1000_0150, 20, None),
    known(340, 6, 0x0200_013E, 0x0400_0BEF, 0x1000_0135, 20, None),
    known(359, 6, 0x0200_014E, 0x0400_0BEF, 0x1000_0140, 20, None),
    known(362, 6, 0x0200_0188, 0x0400_0BEF, 0x1000_0141, 20, None),
    known(74, 2, 0x0200_0330, 0x0400_007E, 0x1000_00AD, 20, Some(0.66)),
];

/// A known class's look rows, written as a pack writes a weenie.
fn sql(k: &Known) -> String {
    let Known {
        class: c,
        weenie_type: ty,
        setup,
        palette,
        clothing,
        scheme,
        shade,
    } = *k;
    let mut s = format!(
        "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({c}, 'known', {ty}, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({c}, 3, {scheme}) /* PaletteTemplate */;
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({c}, 1, {setup}) /* Setup */
     , ({c}, 6, {palette}) /* PaletteBase */
     , ({c}, 7, {clothing}) /* ClothingBase */;
"
    );
    if let Some(v) = shade {
        s.push_str(&format!(
            "INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({c}, 12, {v}) /* Shade */;
"
        ));
    }
    s
}

/// Every object description the retail server sent for each class of `classes`, from the
/// recorded sessions' creates.
fn sent(classes: &BTreeSet<u32>) -> BTreeMap<u32, Vec<ObjDesc>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/message-corpus");
    let mut out: BTreeMap<u32, Vec<ObjDesc>> = BTreeMap::new();
    for e in std::fs::read_dir(&root).expect("the recorded sessions' messages") {
        let Ok(text) = std::fs::read_to_string(e.expect("an entry").path().join("blobs.jsonl"))
        else {
            continue;
        };
        for line in text.lines() {
            let v: serde_json::Value = serde_json::from_str(line).expect("a message");
            if v["dir"] != "s2c" || v["opcode"].as_u64() != Some(0xF745) {
                continue;
            }
            let hex = v["payload_hex"].as_str().expect("its bytes");
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
                .collect();
            let mut r = dereth_protocol::archive::Reader::new(&bytes[4..]);
            let Ok(create) = dereth_protocol::objects::ItemCreateObject::read(&mut r) else {
                continue;
            };
            let c = create.0;
            if classes.contains(&c.wdesc.wcid) {
                let seen = out.entry(c.wdesc.wcid).or_default();
                if !seen.contains(&c.objdesc) {
                    seen.push(c.objdesc);
                }
            }
        }
    }
    out
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats")]
fn a_known_weenies_look_is_the_description_the_retail_server_sent_for_it() {
    let files = DatFiles::new(Arc::new(dereth_dat::testing::open_store_or_fail()));
    let classes: BTreeSet<u32> = CLASSES.iter().map(|k| k.class).collect();
    let sent = sent(&classes);
    let mut wrong = Vec::new();
    for k in CLASSES {
        let doc = SqlDoc::parse(&sql(k));
        let ours = dereth_rules::appearance::object_description(&look_of(&doc), &[], &files);
        let theirs = sent.get(&k.class).map_or(&[][..], Vec::as_slice);
        assert!(
            !theirs.is_empty(),
            "class {} is in the recorded sessions",
            k.class
        );
        if !theirs.contains(&ours) {
            wrong.push(format!("class {}: ours {ours:?}\nsent {theirs:?}", k.class));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n\n"));
}

/// A clothing piece as ACE's world database has it.
struct Piece {
    class: u32,
    weenie_type: u32,
    item_type: u32,
    coverage: Option<u32>,
    locations: u32,
    armor: Option<i32>,
    setup: u32,
    clothing: u32,
    scheme: i32,
    shade: Option<f64>,
}

/// A clothing piece of weenie type 2 (clothing) and item type `item_type`.
const fn piece(
    class: u32,
    item_type: u32,
    (coverage, locations, armor): (u32, u32, i32),
    (setup, clothing): (u32, u32),
    (scheme, shade): (i32, f64),
) -> Piece {
    Piece {
        class,
        weenie_type: 2,
        item_type,
        coverage: Some(coverage),
        locations,
        armor: Some(armor),
        setup,
        clothing,
        scheme,
        shade: Some(shade),
    }
}

/// The pieces the vendors below wield, and a hand axe, which is held and dresses nothing.
const PIECES: &[Piece] = &[
    piece(
        115,
        2,
        (65536, 384, 130),
        (0x0200_00D0, 0x1000_0007),
        (4, 0.1),
    ),
    piece(117, 4, (19, 68, 0), (0x0200_0210, 0x1000_001B), (4, 0.6)),
    piece(
        118,
        4,
        (16384, 1, 20),
        (0x0200_00D3, 0x1000_0009),
        (4, 0.66),
    ),
    piece(124, 4, (8, 6, 0), (0x0200_01A6, 0x1000_001D), (4, 0.33)),
    piece(127, 4, (22, 196, 0), (0x0200_00DD, 0x1000_0002), (14, 0.33)),
    piece(130, 4, (104, 30, 0), (0x0200_00D4, 0x1000_0001), (8, 0.66)),
    piece(
        132,
        4,
        (65536, 256, 20),
        (0x0200_00DE, 0x1000_001F),
        (4, 0.6),
    ),
    piece(
        10696,
        4,
        (7168, 3584, 0),
        (0x0200_01A6, 0x1000_0059),
        (4, 0.8),
    ),
    Piece {
        class: 303,
        weenie_type: 6,
        item_type: 1,
        coverage: None,
        locations: 1_048_576,
        armor: None,
        setup: 0x0200_0127,
        clothing: 0x1000_017D,
        scheme: 20,
        shade: None,
    },
];

/// A vendor: its class, setup and sex, and what it wields, each `(class, color scheme, shade)`.
struct Vendor {
    class: u32,
    setup: u32,
    sex: &'static str,
    wields: &'static [(u32, i32, f32)],
}

/// Holtburg's barkeeper, blacksmith and grocer.
const VENDORS: &[Vendor] = &[
    Vendor {
        class: 710,
        setup: 0x0200_004E,
        sex: "Female",
        wields: &[
            (124, 7, 0.33),
            (127, 8, 0.67),
            (115, 18, 1.0),
            (10696, 6, 0.0),
        ],
    },
    Vendor {
        class: 712,
        setup: 0x0200_0001,
        sex: "Male",
        wields: &[
            (303, 0, 0.0),
            (124, 8, 0.67),
            (117, 8, 0.67),
            (132, 7, 0.33),
            (10696, 4, 0.5),
        ],
    },
    Vendor {
        class: 714,
        setup: 0x0200_0001,
        sex: "Male",
        wields: &[
            (130, 8, 0.67),
            (127, 7, 0.33),
            (132, 18, 1.0),
            (118, 8, 0.0),
            (10696, 9, 0.5),
        ],
    },
];

/// A piece's rows, written as a pack writes a weenie.
fn piece_sql(p: &Piece) -> String {
    let c = p.class;
    let mut ints = format!(
        "({c}, 1, {}), ({c}, 3, {}), ({c}, 9, {})",
        p.item_type, p.scheme, p.locations
    );
    if let Some(v) = p.coverage {
        ints.push_str(&format!(", ({c}, 4, {v})"));
    }
    if let Some(v) = p.armor {
        ints.push_str(&format!(", ({c}, 28, {v})"));
    }
    let mut s = format!(
        "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({c}, 'piece', {}, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES {ints};
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({c}, 1, {}), ({c}, 6, 67108990), ({c}, 7, {});
",
        p.weenie_type, p.setup, p.clothing
    );
    if let Some(v) = p.shade {
        s.push_str(&format!(
            "INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({c}, 12, {v});
"
        ));
    }
    s
}

/// A vendor's rows with the face one recorded session's server rolled for it, read from what it
/// sent: its head, its three face textures and its hair, skin and eye palettes.
fn vendor_sql(v: &Vendor, face: &ObjDesc) -> String {
    let c = v.class;
    let head = face.anim_part_changes[0].part_id;
    let t = &face.texture_changes;
    let p = &face.subpalettes;
    let rows: Vec<String> = v
        .wields
        .iter()
        .map(|(w, scheme, shade)| format!("({c}, 2, {w}, 0, {scheme}, {shade}, False)"))
        .collect();
    format!(
        "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({c}, 'vendor', 12, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({c}, 3, '{}'), ({c}, 4, 'Aluvian');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({c}, 1, {}), ({c}, 18, {head})
     , ({c}, 12, {}), ({c}, 9, {}), ({c}, 13, {}), ({c}, 10, {}), ({c}, 14, {}), ({c}, 11, {})
     , ({c}, 15, {}), ({c}, 17, {}), ({c}, 16, {});
INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)
VALUES {};
",
        v.sex,
        v.setup,
        t[0].old_tex_id,
        t[0].new_tex_id,
        t[1].old_tex_id,
        t[1].new_tex_id,
        t[2].old_tex_id,
        t[2].new_tex_id,
        p[0].sub_id,
        p[1].sub_id,
        p[2].sub_id,
        rows.join(", ")
    )
}

#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats")]
fn a_vendor_wears_what_its_create_list_wields_as_the_retail_server_dressed_it() {
    let files = DatFiles::new(Arc::new(dereth_dat::testing::open_store_or_fail()));
    let classes: BTreeSet<u32> = VENDORS.iter().map(|v| v.class).collect();
    let sent = sent(&classes);
    let pieces: BTreeMap<String, SqlDoc> = PIECES
        .iter()
        .map(|p| (p.class.to_string(), SqlDoc::parse(&piece_sql(p))))
        .collect();
    let mut checked = 0;
    let mut wrong = Vec::new();
    for v in VENDORS {
        let theirs = sent.get(&v.class).map_or(&[][..], Vec::as_slice);
        assert!(
            !theirs.is_empty(),
            "class {} is in the recorded sessions",
            v.class
        );
        for one in theirs {
            let doc = SqlDoc::parse(&vendor_sql(v, one));
            let (wields, unknown) =
                lyceum_editor::look::wields_of(&doc, &mut |c: &str| pieces.get(c).cloned());
            assert_eq!(unknown, 0);
            let worn = dereth_rules::appearance::dress(&wields);
            let ours = dereth_rules::appearance::object_description(&look_of(&doc), &worn, &files);
            checked += 1;
            if ours != *one {
                wrong.push(format!("class {}: ours {ours:?}\nsent {one:?}", v.class));
            }
        }
    }
    assert!(checked >= VENDORS.len());
    assert!(wrong.is_empty(), "{}", wrong.join("\n\n"));
}
