//! Every held data capture, from the October 1999 retail CD to December 2017, keys its motion
//! tables in the command numbering its human motion table tells: every full command id in every
//! table is one of that numbering's commands, every packed key's halves are inside it, and every
//! key translates into the final numbering, but for the two emotes the January 2002 files have
//! that no later client kept. The combat manoeuvres and spell gestures of February 2005 are in the
//! same numbering as its motion tables.
//! Fixture: the historical dat captures (`DERETH_TEST_DAT_CAPTURES_DIR`), the February 2005 dats
//! (`DERETH_TEST_PRETOD_DAT_DIR`) and the retail dats.

use std::collections::BTreeMap;
use std::path::Path;

use dereth_animation::command::{CommandNumbering, MotionCommand};
use dereth_assets::tables::{CombatManeuverTable, SpellComponentTable};
use dereth_assets::{Decode, MotionTable};
use dereth_dat::{DatFile, DbType, RetailDatStore};
use dereth_primitives::DataId;
use dereth_world_data::command_numbering as numbering;

/// Every motion table of one file, decoded in its own container's layouts.
fn motion_tables(path: &Path) -> BTreeMap<u32, MotionTable> {
    let f = DatFile::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    f.iter_ids()
        .filter(|id| id.0 >> 24 == 0x09)
        .map(|id| {
            let b = f.read(id).expect("the record reads");
            let t = MotionTable::decode_payload_in(f.era(), id, &b)
                .unwrap_or_else(|e| panic!("{}: {id:?}: {e}", path.display()));
            (id.0, t)
        })
        .collect()
}

/// The numbering each capture's files are measured to use: the January 2002 client's for the
/// files of that winter, the final client's from 2014 (and for the 1999 files, which keep no
/// command above `0x10E` and so read the same under every numbering), the older one otherwise.
fn expected(capture: &str) -> CommandNumbering {
    match capture {
        "2002-01-30" => CommandNumbering::January2002,
        c if !("2000".."2014").contains(&c) => CommandNumbering::Final,
        _ => CommandNumbering::Before2015,
    }
}

/// Whether a packed `(style, motion)` key's halves are inside the numbering, the style half a
/// style.
fn packed_inside(n: CommandNumbering, key: u32) -> bool {
    let style = u16::try_from(key >> 16).unwrap_or(u16::MAX);
    let motion = u16::try_from(key & 0xFFFF).unwrap_or(u16::MAX);
    let style_ok = style == 0 || n.id_at(style).is_some_and(|id| id & 0x8000_0000 != 0);
    style_ok && motion < n.command_count()
}

#[test]
fn every_held_capture_keys_its_motion_tables_in_the_numbering_its_human_table_tells() {
    let portals: Vec<_> = dereth_dat::testing::dat_captures_or_fail()
        .into_iter()
        .filter(|(_, n, _)| n == "portal.dat" || n == "client_portal.dat")
        .collect();
    assert_eq!(portals.len(), 28, "every held portal capture");
    let mut dropped = Vec::new();
    for (capture, _, path) in &portals {
        let tables = motion_tables(path);
        let human = &tables[&numbering::HUMAN_MOTION_TABLE.0];
        let n = numbering::of_human_table(human);
        assert_eq!(n, expected(capture), "{capture}");
        for (id, t) in &tables {
            for full in numbering::full_ids(t).filter(|&f| f != 0) {
                assert!(
                    n.holds(full),
                    "{capture} {id:#010X}: {full:#010X} is not {n:?}'s"
                );
            }
            for key in t
                .cycles
                .iter()
                .chain(&t.modifiers)
                .map(|m| m.key)
                .chain(t.links.keys().copied())
            {
                assert!(packed_inside(n, key), "{capture} {id:#010X}: {key:#010X}");
            }
            let mut out = t.clone();
            numbering::motion_table(&mut out, n);
            for full in numbering::full_ids(&out).filter(|&f| f != 0) {
                assert!(
                    CommandNumbering::Final.holds(full),
                    "{capture} {id:#010X}: {full:#010X} translated outside the final table"
                );
            }
            let entries = |t: &MotionTable| {
                t.cycles.len() + t.modifiers.len() + t.links.values().map(Vec::len).sum::<usize>()
            };
            if entries(&out) != entries(t) {
                dropped.push((capture.clone(), *id, entries(t) - entries(&out)));
            }
        }
    }
    // The January 2002 human table has four entries that play PointUpState or PointUp, which no
    // later client has.
    assert_eq!(
        dropped,
        [("2002-01-30".to_string(), numbering::HUMAN_MOTION_TABLE.0, 4)],
        "only the two emotes no later client kept leave"
    );
}

/// The links out of `NonCombat`'s `Ready` in the human table, by destination, with the first
/// animation each plays.
fn ready_links(t: &MotionTable) -> BTreeMap<u32, u32> {
    let ready = (MotionCommand::NON_COMBAT.0 & 0xFFFF) << 16 | (MotionCommand::READY.0 & 0xFFFF);
    t.links[&ready]
        .iter()
        .map(|m| (m.key, m.anims.first().map_or(0, |a| a.anim_id.0)))
        .collect()
}

fn human_table(store: &RetailDatStore) -> MotionTable {
    let id = numbering::HUMAN_MOTION_TABLE;
    let b = store
        .read_typed(DbType::MTable, id)
        .expect("the human table reads");
    MotionTable::decode_payload_in(store.era_of(id), id, &b).expect("it decodes")
}

/// **The February 2005 logout is keyed as the older numbering's `LogOut`**, and in the final
/// numbering it is the final `LogOut` with the animation the 2005 files give it; the end-of-retail
/// table is left as it is.
#[test]
fn the_february_2005_logout_link_becomes_the_final_logout_with_its_own_animation() {
    let old = dereth_dat::testing::open_pre_tod_store_or_fail();
    let raw = human_table(&old);
    assert_eq!(numbering::of_store(&old), CommandNumbering::Before2015);
    assert_eq!(ready_links(&raw).get(&0x1000_011B), Some(&0x0300_07BE));
    let mut t = raw.clone();
    numbering::motion_table(&mut t, CommandNumbering::Before2015);
    assert_eq!(
        ready_links(&t).get(&MotionCommand::LOG_OUT.0),
        Some(&0x0300_07BE),
        "LogOut plays the 2005 departure"
    );
    assert_eq!(
        ready_links(&t).get(&MotionCommand::DOUBLE_SLASH_HIGH.0),
        None,
        "and is no longer read as the attack that took its number"
    );

    let now = dereth_dat::testing::open_store_or_fail();
    assert_eq!(numbering::of_store(&now), CommandNumbering::Final);
    let raw = human_table(&now);
    let mut t = raw.clone();
    numbering::motion_table(&mut t, CommandNumbering::Final);
    assert_eq!(t, raw);
    assert_eq!(
        ready_links(&t).get(&MotionCommand::LOG_OUT.0),
        Some(&0x0300_0C22)
    );
}

/// **The February 2005 combat manoeuvres and spell gestures are in the older numbering too**:
/// every style, motion and gesture is one of its commands, and a component both files have makes
/// the same gesture once translated as the end-of-retail file gives it.
#[test]
fn the_february_2005_manoeuvres_and_gestures_translate_with_the_motion_tables() {
    let old = dereth_dat::testing::open_pre_tod_store_or_fail();
    let now = dereth_dat::testing::open_store_or_fail();
    let n = numbering::of_store(&old);
    assert_eq!(n, CommandNumbering::Before2015);

    let mut motions = 0;
    let mut moved = 0;
    for id in old.ids_of(DbType::CombatTable) {
        let b = old.read_typed(DbType::CombatTable, id).expect("reads");
        let mut t =
            CombatManeuverTable::decode_payload_in(old.era_of(id), id, &b).expect("decodes");
        for m in &t.maneuvers {
            assert!(n.holds(m.style) && n.holds(m.motion), "{id:?}: {m:?}");
        }
        let before = t.maneuvers.clone();
        numbering::combat_maneuver_table(&mut t, n);
        assert_eq!(t.maneuvers.len(), before.len(), "{id:?}: nothing leaves");
        for (a, b) in before.iter().zip(&t.maneuvers) {
            assert_eq!(n.to_final(a.motion).map(|c| c.0), Some(b.motion));
            motions += 1;
            moved += usize::from(a.motion != b.motion);
        }
    }
    assert!(motions > 400, "{motions} manoeuvres");
    assert!(moved > 0, "the double and triple attacks move");

    let spells = DataId(0x0E00_000F);
    let read = |s: &RetailDatStore| {
        let b = s
            .read_typed(DbType::SpellComponentTable, spells)
            .expect("reads");
        SpellComponentTable::decode_payload_in(s.era_of(spells), spells, &b).expect("decodes")
    };
    let mut then = read(&old);
    for c in then.components.values() {
        assert!(c.gesture == 0 || n.holds(c.gesture), "{c:?}");
    }
    numbering::spell_component_table(&mut then, n);
    let later = read(&now);
    let mut shared = 0;
    let mut moved = 0;
    for (scid, c) in &then.components {
        if let Some(l) = later.components.get(scid) {
            assert_eq!(c.gesture, l.gesture, "component {scid}");
            shared += 1;
            moved += usize::from(u16::try_from(c.gesture & 0xFFFF).is_ok_and(|i| i >= 0x10F));
        }
    }
    assert!(shared > 100, "{shared} components in both");
    assert!(moved > 0, "the power-up gestures move");
}
