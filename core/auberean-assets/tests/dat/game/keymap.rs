//! The key maps and the action map: their records written back, and every key map through the
//! text form the client saves its own key maps in.

use auberean_assets::keymap::{input_name, ReadKeyMap};
use auberean_assets::ui::{ActionBindings, KeyMap};
use auberean_dat::locate::DatName;
use auberean_dat::{tree, DatFile, Writer};

use super::database::database;
use crate::common::installs;

/// The input actions enumeration the key maps name their actions by.
const INPUT_ACTIONS: u32 = 0x4000_001D;

/// The portal file of each checked set (the key maps and the action map are there).
fn portals() -> Vec<&'static DatFile> {
    installs()
        .iter()
        .flat_map(|i| i.dats.iter())
        .filter(|(n, _)| *n == DatName::Portal)
        .map(|(_, d)| d)
        .collect()
}

/// Every key map and the action map, read and written back, give their records' bytes (the action
/// map's lists in key order, as the client keeps them).
///
/// Behaviour: data.key-map.records-write-back-to-their-bytes
#[test]
fn every_key_map_and_the_action_map_write_back_to_their_bytes() {
    let db = database();
    let mut maps = 0;
    let mut action_maps = 0;
    for dat in portals() {
        for e in dat.entries_in(0x1D00_0000, 0x1DFF_FFFF) {
            let bytes = dat.read_entry(e).expect("read");
            let m = db.get::<KeyMap>(e.id).expect("the key map reads");
            let mut w = Writer::new();
            m.write(e.id, &mut w);
            assert_eq!(w.finish(), bytes, "{:08X}", e.id);
            maps += 1;
        }
        for e in dat.entries_in(0x5100_0000, 0x51FF_FFFF) {
            let bytes = dat.read_entry(e).expect("read");
            let a = db
                .get::<ActionBindings>(e.id)
                .expect("the action map reads");
            let mut w = Writer::new();
            a.write(e.id, &mut w);
            assert_eq!(w.finish(), bytes, "{:08X}", e.id);
            action_maps += 1;
        }
    }
    assert_eq!((maps, action_maps), (19, 1));
}

/// Every key map of the data, written as the client writes a key map's file and read back as it
/// reads one, keeps its name, its identity and every binding in order: each action's name finds
/// the action in the input actions enumeration and each input has a name.
///
/// Behaviour: data.key-map.every-record-reads-back-through-its-file
#[test]
fn every_key_map_reads_back_through_the_file_the_client_writes() {
    let db = database();
    let action_of = |n: &str| db.enumeration_value(INPUT_ACTIONS, n);
    let mut n = 0;
    for dat in portals() {
        for e in dat.entries_in(0x1D00_0000, 0x1DFF_FFFF) {
            let m = db.get::<KeyMap>(e.id).expect("the key map reads");
            for b in &m.bindings {
                assert!(
                    input_name(b.input).is_some(),
                    "{:08X} {:08X}",
                    e.id,
                    b.input
                );
            }
            let mut t = m.to_tree();
            let bytes = tree::to_file_bytes(&mut t);
            let mut back = tree::parse_bytes(b"Main.keymap", &bytes).expect("parses");
            let ReadKeyMap {
                map,
                version,
                updated,
            } = KeyMap::from_tree(&mut back, &action_of, &[]).expect("reads");
            assert_eq!(map, *m, "{:08X}", e.id);
            assert_eq!((version, updated), (1, false));
            n += 1;
        }
    }
    assert_eq!(n, 19);
}
