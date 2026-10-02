//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The February 2005 dat set (portal.dat and cell.dat, before Throne of Destiny) opens in its own
//! container layout: every directory entry is found and every record reads whole.
//! Fixture: the February 2005 portal and cell files (`DERETH_TEST_PRETOD_DAT_DIR`).

use dereth_dat::container::{CELL_DATFILE, PORTAL_DATFILE};
use dereth_dat::{ContainerEra, DatFile, DatKind, DbType};
use dereth_primitives::DataId;

/// The headers name the layout, the block sizes and the whole file's iteration.
#[test]
fn the_february_2005_headers_carry_the_files_iteration() {
    let s = dereth_dat::testing::open_pre_tod_store_or_fail();
    assert_eq!(s.era(), ContainerEra::PreTod);

    let p = s.portal();
    assert_eq!(p.era(), ContainerEra::PreTod);
    assert_eq!(p.header_iteration(), Some(2112));
    let h = p.header();
    assert_eq!(h.block_size, 0x400);
    assert_eq!(h.file_size, 285_212_672);
    assert_eq!(h.data_set, PORTAL_DATFILE);
    assert_eq!(h.btree_root, 0x0108_0800);
    assert_eq!(h.free_count, 1410);

    let c = s.cell();
    assert_eq!(c.era(), ContainerEra::PreTod);
    assert_eq!(c.header_iteration(), Some(1593));
    let h = c.header();
    assert_eq!(h.block_size, 0x100);
    assert_eq!(h.file_size, 207_618_048);
    assert_eq!(h.data_set, CELL_DATFILE);
    assert_eq!(h.btree_root, 0x013D_A700);
    assert_eq!(h.free_count, 2507);

    // No iteration list: the header's word is the iteration.
    assert!(p.iteration_list().is_err());
    // Not the end-of-retail headers.
    assert!(!s.headers_match_retail());
}

/// Every directory entry of both files is found, the trees are sound, and every record's chain
/// yields exactly the size its entry declares.
#[test]
fn every_february_2005_record_reads_whole() {
    let s = dereth_dat::testing::open_pre_tod_store_or_fail();
    for (file, entries, nodes) in [(s.portal(), 51_001, 1404), (s.cell(), 524_954, 16_881)] {
        assert_eq!(file.len(), entries, "{}", file.path().display());
        let report = file.verify_structure().expect("the directory walks");
        assert!(report.is_sound(), "{report:?}");
        assert_eq!(report.nodes, nodes);
        read_every_record(file);
    }
}

fn read_every_record(file: &DatFile) {
    let mut bytes = 0usize;
    for (id, entry) in file.iter_entries() {
        let payload = file
            .read(id)
            .unwrap_or_else(|e| panic!("{id:?} in {}: {e}", file.path().display()));
        assert_eq!(payload.len(), entry.size as usize, "{id:?}");
        assert_eq!(
            file.lookup_via_tree(id)
                .expect("tree search")
                .map(|e| e.offset),
            Some(entry.offset),
            "{id:?}"
        );
        bytes += payload.len();
    }
    assert!(bytes > 0);
}

/// The portal file answers the portal and language types (strings lived there before Throne of
/// Destiny), and the cell file the landblocks.
#[test]
fn the_february_2005_store_routes_language_records_to_the_portal_file() {
    let s = dereth_dat::testing::open_pre_tod_store_or_fail();
    assert_eq!(
        s.file(DatKind::Local).map(DatFile::path),
        Some(s.portal().path())
    );
    // The skill table (portal) and Holtburg's landblock (cell).
    assert!(s
        .read_typed(DbType::SkillTable, DataId(0x0E00_0004))
        .is_ok());
    assert!(s.read_cell(DataId(0xA9B4_FFFF)).is_ok());
    assert!(!s.grant_highres().expect("no high-resolution file to open"));
}
