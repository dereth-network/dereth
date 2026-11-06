//! Behaviour: none (examinations in the order the ac1999 server writes them).

use dereth_protocol::{read_body, read_checked, write_blob};

use super::super::write_string;
use super::*;

fn int_of(p: &AppraisalProfile, key: u32) -> Option<i32> {
    p.tables
        .ints
        .as_ref()?
        .entries
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

fn string_of(p: &AppraisalProfile, key: u32) -> Option<&str> {
    p.tables
        .strings
        .as_ref()?
        .entries
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.as_str())
}

fn head(flags: u32) -> Writer {
    let mut w = Writer::body();
    w.u32(77);
    w.u32(flags);
    w.u32(1);
    w
}

#[test]
fn value_and_burden_become_properties_and_absent_ones_are_left_out() {
    let mut w = head(section::BURDEN_VALUE);
    w.u32(40);
    w.u32(ABSENT);
    w.u32(5);
    let info = read_body::<AppraiseInfo>(&w.into_inner()).unwrap().0;
    assert_eq!(info.object, ObjectId(77));
    assert_eq!(info.profile.success_flag, 1);
    assert_eq!(int_of(&info.profile, int::MASS), Some(40));
    assert_eq!(int_of(&info.profile, int::VALUE), None);
    assert_eq!(int_of(&info.profile, int::ENCUMBRANCE), Some(5));
    assert_ne!(info.profile.flags & later::INT, 0);
}

#[test]
fn a_creature_keeps_its_health_and_ten_attributes() {
    let mut w = head(section::CREATURE);
    w.u32(8);
    w.u32(12);
    w.u32(30);
    w.u32(40);
    for i in 0..10 {
        w.u32(i);
    }
    let p = read_body::<AppraiseInfo>(&w.into_inner())
        .unwrap()
        .0
        .profile;
    let c = p.creature_profile.as_ref().unwrap();
    assert_eq!((c.health, c.max_health), (30, 40));
    assert_eq!(c.attributes.unwrap()[9], 9);
    assert_eq!(int_of(&p, int::LEVEL), Some(12));
    assert_ne!(p.flags & later::CREATURE_PROFILE, 0);
}

#[test]
fn a_character_gives_its_rank_and_words_and_skips_what_the_later_panel_has_no_place_for() {
    let mut w = head(section::CREATURE);
    w.u32(2 | 1);
    w.u32(5);
    w.u32(10);
    w.u32(10);
    w.u32(3);
    w.u32(4);
    for _ in 0..3 {
        w.u32(0);
    }
    for text in ["Male", "Aluvian", "Adventurer", "Band", "King"] {
        write_string(&mut w, text, MAX_NEWS).unwrap();
    }
    for _ in 0..6 {
        w.u32(0);
    }
    w.u32(0x20);
    let p = read_body::<AppraiseInfo>(&w.into_inner())
        .unwrap()
        .0
        .profile;
    assert_eq!(int_of(&p, int::ALLEGIANCE_RANK), Some(3));
    assert_eq!(string_of(&p, string::HERITAGE), Some("Aluvian"));
    assert_eq!(string_of(&p, string::TEMPLATE), Some("Adventurer"));
    assert_eq!(p.creature_profile.unwrap().enchantment_bitfield, Some(0x20));
}

#[test]
fn a_weapon_keeps_its_numbers_and_its_defense_mod_as_a_property() {
    let mut w = head(section::WEAPON);
    w.u32(1);
    w.i32(30);
    w.u32(45);
    w.i32(9);
    w.f64(0.5);
    w.f64(1.5);
    w.f64(0.0);
    w.f64(1.1);
    w.f64(1.2);
    let p = read_body::<AppraiseInfo>(&w.into_inner())
        .unwrap()
        .0
        .profile;
    let weapon = p.weapon_profile.unwrap();
    assert_eq!((weapon.weapon_damage, weapon.weapon_offense), (9, 1.2));
    assert_eq!(
        p.tables.floats.unwrap().entries,
        [(WEAPON_DEFENSE_FLOAT, 1.1)]
    );
}

#[test]
fn an_items_magic_gives_its_mana_and_its_spell_line_goes_in_the_long_description() {
    let mut w = head(section::LONG_DESC | section::MAGIC);
    write_string(&mut w, "A wand.", MAX_NEWS).unwrap();
    for v in [50, 60, ABSENT, 100, 200, ABSENT, 0] {
        w.u32(v);
    }
    write_string(&mut w, "", MAX_NEWS).unwrap();
    w.u32(ABSENT);
    w.u32(0);
    write_string(&mut w, "Spells: Flame Bolt I", MAX_NEWS).unwrap();
    write_string(&mut w, "", MAX_NEWS).unwrap();
    w.f64(-1.0);
    let p = read_body::<AppraiseInfo>(&w.into_inner())
        .unwrap()
        .0
        .profile;
    assert_eq!(int_of(&p, int::CUR_MANA), Some(100));
    assert_eq!(int_of(&p, int::RANK_LIMIT), None);
    assert_eq!(
        string_of(&p, string::LONG_DESC),
        Some("A wand.\nSpells: Flame Bolt I")
    );
}

#[test]
fn the_result_encodes_as_the_later_message() {
    let mut w = head(section::BURDEN_VALUE | section::USE | section::ARMOR);
    for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0] {
        w.f32(v);
    }
    w.u32(1);
    w.u32(2);
    w.u32(3);
    write_string(&mut w, "Use it.", MAX_NEWS).unwrap();
    let info = read_body::<AppraiseInfo>(&w.into_inner()).unwrap().0;
    let later_form = write_blob(&info).unwrap();
    let back = read_checked::<ItemSetAppraiseInfo>(&later_form).unwrap();
    assert_eq!(back, info);
}

#[test]
fn a_failed_examination_has_its_success_word_zero() {
    let mut w = Writer::body();
    w.u32(77);
    w.u32(0);
    w.u32(0);
    let info = read_body::<AppraiseInfo>(&w.into_inner()).unwrap().0;
    assert_eq!(info.profile.success_flag, 0);
}
