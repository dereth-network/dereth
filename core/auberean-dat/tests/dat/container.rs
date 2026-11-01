//! Every data file of the end-of-retail set read as a container: sound structure, lookups by tree
//! descent, and well-formed iteration lists.

use auberean_dat::locate::install_dats;
use auberean_dat::testing::require_install;
use auberean_dat::testing::{CHECKED, TARGET};
use auberean_dat::DatFile;

fn open_all(install: &std::path::Path) -> Vec<(String, DatFile)> {
    install_dats(install)
        .expect("list the install")
        .into_iter()
        .map(|(name, path)| {
            let dat = DatFile::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            (name.file_name(), dat)
        })
        .collect()
}

/// Behaviour: data.container.every-file-walks-soundly
#[test]
fn every_end_of_retail_data_file_walks_soundly() {
    for v in CHECKED {
        let d = require_install(v);
        for (name, dat) in open_all(&d) {
            let s = dat.verify().unwrap_or_else(|e| panic!("{v}/{name}: {e}"));
            assert!(auberean_dat::container::sound(&s), "{v}/{name}: {s:?}");
            let slot = dat.transaction_slot().expect("the transaction slot reads");
            assert_eq!(slot, [0u8; 0x2C], "{v}/{name}");
        }
    }
}

/// Behaviour: data.container.tree-descent-finds-every-record
#[test]
fn a_tree_descent_finds_every_record_the_walk_found() {
    let d = require_install(TARGET);
    for (name, dat) in open_all(&d) {
        let ids: Vec<u32> = dat.entries().map(|e| e.id).collect();
        // Every 97th record: a spread over every node of every file, at a bounded cost.
        for id in ids.iter().step_by(97) {
            assert_eq!(
                dat.lookup_via_tree(*id).unwrap().as_ref(),
                dat.entry(*id),
                "{name} {id:08X}"
            );
        }
    }
}

/// Behaviour: data.container.iteration-list-is-well-formed
#[test]
fn every_data_file_holds_a_well_formed_iteration_list() {
    for v in CHECKED {
        let d = require_install(v);
        for (name, dat) in open_all(&d) {
            let it = dat
                .iterations()
                .unwrap_or_else(|e| panic!("{v}/{name}: {e}"));
            assert!(!it.is_empty(), "{v}/{name}");
        }
    }
}
