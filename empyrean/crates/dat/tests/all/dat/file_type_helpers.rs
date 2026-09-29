//! Vectors: local spell, chargen, palette, motion and terrain cases in this module
//! ACE helpers on dat types: spell ComputeHash, formula decrypt and per-account taper
//! randomisation, bad-word check, CharGen getters, palette by shade, visual priority, motion
//! lookup, terrain word fields.
//! Fixture: synthetic dat records and locally constructed geometry.

use std::collections::BTreeMap;

use dereth_assets::motion::ObjectEffect;
use dereth_assets::tables::{CombatManeuver, GearItem, HairStyle, ObjDesc};
use dereth_primitives::DataId;
use empyrean_dat::fake::sample;
use empyrean_dat::file_types::cell_landblock::{get_road, get_scenery, get_type};
use empyrean_dat::file_types::clothing_table::ClothingTableExt;
use empyrean_dat::file_types::combat_maneuver_table::CombatManeuverTableExt;
use empyrean_dat::file_types::palette_set::PaletteSetExt;
use empyrean_dat::file_types::sex_cg::SexCgExt;
use empyrean_dat::file_types::spell_table::{
    compute_hash, decrypt_formula, get_spell_formula, get_spell_words, SpellBaseExt,
};
use empyrean_dat::file_types::taboo_table::TabooTableExt;
use empyrean_dat::file_types::{ClothingTable, CombatManeuverTable, PaletteSet, TabooTable};
use empyrean_dat::AceThrow;
use empyrean_entity::enums::CoverageMask as Cm;

// ------------------------------------------------------------------------------ SpellTable

/// `ComputeHash`: `result = c + (result << 4)` over signed cp1252 chars, folding the top nibble.
#[test]
fn compute_hash_matches_hand_derived_values() {
    assert_eq!(compute_hash(""), 0);
    assert_eq!(compute_hash("a"), 97);
    assert_eq!(compute_hash("ab"), 97 * 16 + 98);
    // 'é' is cp1252 0xE9, the signed char -23: result = -23; its top nibble is set, so
    // (-23 ^ 0xF0) & 0x0FFFFFFF = 0x0FFFFF19.
    assert_eq!(compute_hash("\u{e9}"), 0x0FFF_FF19);
    // '€' encodes to cp1252 0x80, the signed char -128: (-128 ^ 0xF0) & 0x0FFFFFFF.
    assert_eq!(compute_hash("\u{20ac}"), 0x0FFF_FF70);
    // A character with no cp1252 byte encodes as '?' (63).
    assert_eq!(compute_hash("\u{4e00}"), 63);
    // Eight chars push bits past 28: "aaaaaaaa" folds; compare with the shared client hash.
    let s = "aaaaaaaa";
    assert_eq!(
        compute_hash(s),
        dereth_assets::tables::spell_hash(s.as_bytes())
    );
}

/// Compute hash agrees with the shared spell hash.
#[test]
fn compute_hash_agrees_with_the_shared_spell_hash() {
    let samples = [
        "Strength Other I",
        "Incantation of Blade Bane",
        "Aerfalle's Touch",
        "\u{e9}t\u{e9}",
        "Zojak Tugakquaril",
        "a long description with, punctuation. and CAPITALS!",
        "\u{2019}",
    ];
    for s in samples {
        let bytes: Vec<u8> = s
            .chars()
            .map(|c| match c {
                '\u{2019}' => 0x92,
                c => u8::try_from(u32::from(c)).expect("latin-1 sample"),
            })
            .collect();
        assert_eq!(
            compute_hash(s),
            dereth_assets::tables::spell_hash(&bytes),
            "{s:?}"
        );
    }
}

#[test]
fn decrypt_formula_subtracts_the_key_and_repairs_ids_above_198() {
    let key = (compute_hash("n") % 0x1210_7680).wrapping_add(compute_hash("d") % 0xBEAD_CF45);
    let raw = [
        key.wrapping_add(5),
        key.wrapping_add(198),
        key.wrapping_add(0x305),
    ];
    // 0x305 > 198, so ACE keeps its low byte: 0x05.
    assert_eq!(decrypt_formula(&raw, "n", "d"), vec![5, 198, 5]);
}

#[test]
fn the_sample_spell_decrypts_to_its_formula_and_speaks_its_words() {
    let table = sample::spell_table();
    let spell = &table.spells[&1];
    assert_eq!(spell.formula(), sample::SPELL_FORMULA.to_vec());
    let comps = sample::spell_components_table();
    // Herb "Zojak"; powder "Tugak" + potion lower-cased "quaril", first letter upper-cased.
    assert_eq!(
        spell.get_spell_words(&comps),
        Ok("Zojak Tugakquaril".to_owned())
    );
    assert_eq!(get_spell_words(&comps, None), Ok(String::new()));
    assert_eq!(
        get_spell_words(&comps, Some(&[30])),
        Ok("Quaril".to_owned()),
        "trimmed"
    );
    assert_eq!(
        get_spell_words(&comps, Some(&[10, 99])),
        Err(AceThrow::KeyNotFound(99))
    );
}

fn spell_table_with(formula: &[u32], version: u32) -> empyrean_dat::file_types::SpellTable {
    let mut t = sample::spell_table();
    let spell = t.spells.get_mut(&1).expect("sample spell");
    let key = (compute_hash(&spell.name) % 0x1210_7680)
        .wrapping_add(compute_hash(&spell.description) % 0xBEAD_CF45);
    spell.raw_comps = [0; 8];
    for (slot, c) in spell.raw_comps.iter_mut().zip(formula) {
        *slot = c.wrapping_add(key);
    }
    spell.formula_version = version;
    t
}

/// `GetSpellFormula` for account "a" (hash 97, so every seed is 97) and formula
/// `[1, 10, 20, 30, 40, 50, 60, 70]`, worked through `RandomizeVersion1..3` by hand.
#[test]
fn get_spell_formula_randomizes_the_tapers_per_account() {
    let f = [1, 10, 20, 30, 40, 50, 60, 70];
    // v0: the formula itself.
    assert_eq!(
        get_spell_formula(&spell_table_with(&f, 0), 1, "a"),
        Ok(f.to_vec())
    );
    // v1: herb 20 (index 2), powder 40 (index 4), potion 50, talisman 70 (index 7).
    //   [1] = (40 + 2*20 + 50 + 70 + 1) % 12 + 63 = 201 % 12 + 63 = 72
    //   [3] = (1 + 20 + 70 + 2*90) * (97 / 91) % 12 + 63 = 271 % 12 + 63 = 70
    //   [6] = (40 + 2*70 + 50 + 20 + 1) * (97 / 71) % 12 + 63 = 251 % 12 + 63 = 74
    assert_eq!(
        get_spell_formula(&spell_table_with(&f, 1), 1, "a"),
        Ok(vec![1, 72, 20, 70, 40, 50, 74, 70])
    );
    // v2: p1 1, c 40, x 50, a 70.
    //   [3] = (70 + 2 + 2*40*50 + 1 + 20 + 10) % 12 + 63 = 4103 % 12 + 63 = 74
    //   [6] = (70 + 2*1*20 + 100 + 20 + 40) * (97 / (10*70 + 80)) % 12 + 63 = 0 + 63
    assert_eq!(
        get_spell_formula(&spell_table_with(&f, 2), 1, "a"),
        Ok(vec![1, 10, 20, 74, 40, 50, 63, 70])
    );
    // v3: hashes (97 + comp) % 12: h0 2, h1 11, h2 9, h4 5, h5 3, h7 11.
    //   [3] = (2+11+9+5+3 + 9*3 + 2*11 + 11*6) % 12 + 63 = 145 % 12 + 63 = 64
    //   [6] = (27 + 97%12 + 11*(5*(594+7)+1) + 3 + 88 + 22 + 297) % 12 + 63 = 33504 % 12 + 63 = 63
    assert_eq!(
        get_spell_formula(&spell_table_with(&f, 3), 1, "a"),
        Ok(vec![1, 10, 20, 64, 40, 50, 63, 70])
    );
}

#[test]
fn get_spell_formula_fails_where_ace_throws() {
    let t = spell_table_with(&[1, 10, 20, 30, 40], 2);
    assert_eq!(
        get_spell_formula(&t, 1, "a"),
        Err(AceThrow::IndexOutOfRange(5)),
        "comps[5]"
    );
    assert_eq!(
        get_spell_formula(&t, 7, "a"),
        Err(AceThrow::KeyNotFound(7)),
        "Spells[7]"
    );
}

// ------------------------------------------------------------------------------ TabooTable

fn taboo(first: &[&str], second: &[&str]) -> TabooTable {
    let list = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    TabooTable {
        id: DataId(0x0E00_001E),
        audiences: vec![
            (1, vec![(0x0001_0000, list(first))]),
            (2, vec![(0x0001_0000, list(second))]),
        ],
    }
}

#[test]
fn contains_bad_word_tests_each_word_against_the_first_entry_only() {
    let t = taboo(&["*bad*", "exact", "start*", "a.c"], &["other"]);
    assert!(t.contains_bad_word("a bad name"));
    assert!(t.contains_bad_word("xbadx"));
    assert!(t.contains_bad_word("Exact"), "the input is lower-cased");
    assert!(!t.contains_bad_word("exactly"), "anchored at both ends");
    assert!(t.contains_bad_word("startle"));
    assert!(!t.contains_bad_word("restart"));
    assert!(t.contains_bad_word("abc"), "'.' is the regex any-character");
    assert!(!t.contains_bad_word("ac"));
    assert!(
        !t.contains_bad_word("other"),
        "ACE breaks after the first entry"
    );
    assert!(!taboo(&[], &[]).contains_bad_word("bad"));
}

// ------------------------------------------------------------------------------ SkillBase / SexCG

fn objdesc(textures: &[(u32, u32)], parts: &[u32]) -> ObjDesc {
    ObjDesc {
        version: 0x11,
        palette: None,
        subpalettes: Vec::new(),
        texture_changes: textures
            .iter()
            .map(|(o, n)| (0, DataId(*o), DataId(*n)))
            .collect(),
        anim_part_changes: parts.iter().map(|p| (16, DataId(*p))).collect(),
    }
}

#[test]
fn sex_cg_getters_read_the_styles_and_fail_past_the_list() {
    let cg = sample::char_gen();
    let mut sex = cg.heritage_groups[&1].sexes[&1].clone();
    sex.hair_styles = vec![
        HairStyle {
            icon: 0,
            bald: 0,
            alternate_setup: DataId(0),
            objdesc: objdesc(&[(0x0500_0001, 0x0500_0002)], &[0x0100_0001]),
        },
        HairStyle {
            icon: 0,
            bald: 0,
            alternate_setup: DataId(0),
            objdesc: objdesc(&[], &[0x0100_0001, 0x0100_0002]),
        },
    ];
    sex.nose_strips = vec![(0, objdesc(&[(0x0500_0010, 0x0500_0011)], &[]))];
    sex.shirts = vec![GearItem {
        name: "Shirt".into(),
        clothing_table: DataId(0x1000_0001),
        weenie_default: 130,
    }];
    assert_eq!(sex.get_head_object(0), Ok(Some(0x0100_0001)));
    assert_eq!(sex.get_head_object(1), Ok(None), "two part changes");
    assert_eq!(sex.get_hair_texture(0), Ok(Some(0x0500_0002)));
    assert_eq!(sex.get_default_hair_texture(0), Ok(Some(0x0500_0001)));
    assert_eq!(sex.get_hair_texture(1), Ok(None), "no texture changes");
    assert_eq!(sex.get_nose_texture(0), Ok(0x0500_0011));
    assert_eq!(sex.get_default_nose_texture(0), Ok(0x0500_0010));
    assert_eq!(sex.get_shirt_weenie(0), Ok(130));
    assert_eq!(sex.get_shirt_clothing_table(0), Ok(0x1000_0001));
    assert_eq!(sex.get_shirt_weenie(1), Err(AceThrow::IndexOutOfRange(1)));
    assert_eq!(
        sex.get_eye_texture(0, false),
        Err(AceThrow::IndexOutOfRange(0))
    );
    assert_eq!(
        sex.get_pants_weenie(u32::MAX),
        Err(AceThrow::IndexOutOfRange(u64::from(u32::MAX)))
    );
}

// ------------------------------------------------------------------------------ PaletteSet

/// `(int)((Count - 0.000001) * hue)`, clamped to the list.
#[test]
fn get_palette_id_picks_by_shade() {
    let set = PaletteSet {
        id: DataId(0x0F00_0001),
        palette_ids: (1..=4).map(|i| DataId(0x0400_0000 + i)).collect(),
    };
    assert_eq!(set.get_palette_id(0.0), 0x0400_0001);
    assert_eq!(
        set.get_palette_id(0.5),
        0x0400_0002,
        "3.999999 * 0.5 truncates to 1"
    );
    assert_eq!(
        set.get_palette_id(1.0),
        0x0400_0004,
        "3.999999 truncates to 3"
    );
    assert_eq!(set.get_palette_id(1.5), 0, "hue out of range");
    assert_eq!(set.get_palette_id(-0.1), 0);
    assert_eq!(
        PaletteSet {
            id: DataId(0),
            palette_ids: vec![]
        }
        .get_palette_id(0.5),
        0
    );
}

// ------------------------------------------------------------------------------ ClothingTable

#[test]
fn visual_priority_maps_body_parts_to_coverage() {
    let effect = |part_num| ObjectEffect {
        part_num,
        object_id: DataId(0x0100_0001),
        texture_effects: vec![],
    };
    let table = ClothingTable {
        id: DataId(0x1000_0001),
        clothing_base_buckets: 0,
        clothing_bases: BTreeMap::from([(
            DataId(0x0200_0001),
            vec![effect(0), effect(5), effect(9), effect(16), effect(20)],
        )]),
        palette_template_buckets: 0,
        palette_templates: BTreeMap::new(),
    };
    assert_eq!(
        table.get_visual_priority(0x0200_0001),
        Some(
            (Cm::OuterwearAbdomen | Cm::OuterwearUpperLegs | Cm::OuterwearChest | Cm::Head).bits()
        )
    );
    assert_eq!(table.get_visual_priority(0x0200_0002), None);
    assert_eq!(table.get_icon(3), 0, "no such palette template");
}

// ------------------------------------------------------------------------------ CombatManeuverTable

#[test]
fn get_motion_lists_every_match_in_file_order_or_invalid() {
    let m = |style, attack_height, attack_type, motion| CombatManeuver {
        style,
        attack_height,
        attack_type,
        min_skill_level: 0,
        motion,
    };
    let cmt = CombatManeuverTable {
        id: DataId(0x3000_0000),
        maneuvers: vec![
            m(0x3D, 2, 4, 0x62),
            m(0x3D, 1, 4, 0x63),
            m(0x3D, 2, 4, 0x65),
        ],
    };
    assert_eq!(cmt.get_motion(0x3D, 2, 4, 0), vec![0x62, 0x65]);
    assert_eq!(cmt.get_motion(0x3D, 1, 4, 0), vec![0x63]);
    assert_eq!(
        cmt.get_motion(0x3D, 3, 4, 0),
        vec![0],
        "MotionCommand.Invalid"
    );
}

// ------------------------------------------------------------------------------ CellLandblock

#[test]
fn terrain_word_fields() {
    // road bits 0..2, type bits 2..7, scenery bits 11..16.
    let t: u16 = (0b10101 << 11) | (0b10110 << 2) | 0b01;
    assert_eq!(get_road(t), 0b01);
    assert_eq!(get_type(t), 0b10110);
    assert_eq!(get_scenery(t), 0b10101);
}
