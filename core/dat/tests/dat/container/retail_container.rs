//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Container headers retain their reference bytes; every directory and payload agrees with the input file.
//! Fixture: the shipped retail DAT records and recorded inputs.

use dereth_dat::container::{CELL_DATFILE, HIRES_SUBSET, LOCAL_DATFILE, PORTAL_DATFILE};
use dereth_dat::{
    classify_cell_id, dat_for_type, divine_type, DatFile, DatKind, DbType, RetailDatStore,
    ITERATION_LIST,
};
use dereth_primitives::DataId;

fn store() -> RetailDatStore {
    let s = dereth_dat::testing::open_store_or_fail();
    assert!(
        s.grant_highres().expect("client_highres.dat opens"),
        "client_highres.dat is present"
    );
    s
}

/// Headers match the documented retail values.
#[test]
fn headers_match_the_documented_retail_values() {
    let s = store();
    let p = s.portal().header();
    assert_eq!(p.magic, 0x5442);
    assert_eq!(p.block_size, 0x400);
    assert_eq!(p.file_size, 0x3740_0000);
    assert_eq!(p.data_set, PORTAL_DATFILE);
    assert_eq!(p.data_subset, 0);
    assert_eq!(p.free_head, 0x3738_6800);
    assert_eq!(p.free_tail, 0x3738_1C00);
    assert_eq!(p.free_count, 2052);
    assert_eq!(p.btree_root, 0x0232_D400);
    assert_eq!(p.master_map_id, 0x2500_0000);
    assert_eq!(p.eng_pack_vnum, 110);
    assert_eq!(p.game_pack_vnum, 0);
    assert_eq!(p.version_minor, 0x1A01);
    assert!(!p.use_lru);
    assert_eq!(p.dat_file_id(), 0x0000_0001_0000_0000);

    let c = s.cell().header();
    assert_eq!(c.block_size, 0x100);
    assert_eq!(c.file_size, 0x14C0_0000);
    assert_eq!(c.data_set, CELL_DATFILE);
    assert_eq!(c.data_subset, 1);
    assert_eq!(c.free_head, 0x14B3_5A00);
    assert_eq!(c.free_tail, 0x14BF_FF00);
    assert_eq!(c.free_count, 3238);
    assert_eq!(c.btree_root, 0x01B7_7400);
    assert_eq!(c.eng_pack_vnum, 22);
    assert_eq!(c.dat_file_id(), 0x0000_0002_0000_0001);

    let l = s.local().header();
    assert_eq!(l.block_size, 0x400);
    assert_eq!(l.file_size, 0x0010_0000);
    assert_eq!(l.data_set, LOCAL_DATFILE);
    assert_eq!(l.data_subset, 1);
    assert_eq!(l.free_count, 270);
    assert_eq!(l.btree_root, 0x0002_B000);
    assert_eq!(l.dat_file_id(), 0x0000_0003_0000_0001);

    let h = s.highres().expect("client_highres.dat").header();
    assert_eq!(h.block_size, 0x400);
    assert_eq!(h.file_size, 0x07F0_0000);
    assert_eq!(h.data_set, PORTAL_DATFILE);
    assert_eq!(h.data_subset, HIRES_SUBSET);
    assert_eq!(h.free_count, 363);
    assert_eq!(h.btree_root, 0x0681_1800);
    assert_eq!(h.dat_file_id(), 0x0000_0001_6946_6948);

    // All four carry the same version-stamp GUID and minor version.
    assert_eq!(p.version_major, c.version_major);
    assert_eq!(p.version_major, l.version_major);
    assert_eq!(p.version_major, h.version_major);
    assert_eq!(
        p.version_major,
        [
            0xD2, 0xD7, 0xA7, 0x34, 0x2F, 0x72, 0x46, 0x4C, 0x8A, 0xB4, 0xEF, 0x51, 0x4F, 0x85,
            0x6F, 0xFD
        ]
    );

    assert!(s.headers_match_retail());
}

/// Structure is sound in all four dats.
#[test]
fn structure_is_sound_in_all_four_dats() {
    let s = store();
    for (name, f) in [
        ("portal", s.portal()),
        ("local", s.local()),
        ("highres", s.highres().unwrap()),
    ] {
        let r = f.verify_structure().unwrap();
        assert_eq!(
            r.entries,
            f.len(),
            "every directory entry is visited in {name}"
        );
        assert!(r.nodes > 0, "{name} has tree nodes");
        assert_eq!(r.out_of_order, 0, "{name} keys out of order");
        assert!(r.max_entries_per_node <= 61, "{name} node overfull");
        assert_eq!(
            r.leaf_depths.len(),
            1,
            "{name} leaves at mixed depths: {:?}",
            r.leaf_depths
        );
        assert_eq!(
            r.free_chain_len, r.header_free_count as usize,
            "{name} free chain length vs header freeCount"
        );
        assert!(
            r.blocks_used <= r.total_data_blocks,
            "{name} blocks used exceeds the data area"
        );
        assert!(r.is_sound(), "{name}");
    }
}

/// The cell dat is checked separately because its 26,657 nodes make the walk the slow part.
#[test]
fn cell_dat_structure_is_sound() {
    let s = store();
    let r = s.cell().verify_structure().unwrap();
    assert_eq!(r.entries, s.cell().len());
    assert!(r.nodes > 0, "the cell directory has tree nodes");
    assert_eq!(r.out_of_order, 0);
    assert_eq!(r.leaf_depths.len(), 1);
    assert_eq!(r.free_chain_len, r.header_free_count as usize);
    assert!(r.is_sound());
}

/// Iteration lists decode to the documented runs.
#[test]
fn iteration_lists_decode_to_the_documented_runs() {
    let s = store();
    for (name, f) in [
        ("portal", s.portal()),
        ("cell", s.cell()),
        ("local", s.local()),
        ("highres", s.highres().unwrap()),
    ] {
        let v = f.iteration_list().unwrap();
        assert!(!v.is_empty(), "{name} has iterations");
        let raw = f.read(ITERATION_LIST).unwrap();
        let n = u32::from_le_bytes(raw[0..4].try_into().unwrap());
        assert_eq!(v.len(), n as usize, "{name}");
        assert_eq!(
            v,
            (1..=n).collect::<Vec<_>>(),
            "{name} is not the run 1..={n}"
        );
    }
    // Section 11's worked example: the raw payload of the portal iteration file.
    let raw = s.portal().read(ITERATION_LIST).unwrap();
    assert_eq!(
        raw,
        [0x18, 0x08, 0, 0, 0xE8, 0xF7, 0xFF, 0xFF, 0x01, 0, 0, 0]
    );
    let e = s.portal().entry(ITERATION_LIST).unwrap();
    assert_eq!(e.offset, 0x3738_6400);
    assert_eq!(e.size, 12);
    assert_eq!(e.version(), 1);
    assert_eq!(e.iteration, 1);
}

/// Directory entry bitfields match the retail statistics.
#[test]
fn directory_entry_bitfields_match_the_retail_statistics() {
    let s = store();
    let mut total = 0usize;
    let mut versions = std::collections::BTreeMap::new();
    for f in [s.portal(), s.cell(), s.local(), s.highres().unwrap()] {
        for (id, e) in f.iter_entries() {
            total += 1;
            assert!(!e.compressed(), "{id} is marked compressed");
            assert_eq!(e.reserved(), 0, "{id} has reserved bits set");
            *versions.entry(e.version()).or_insert(0usize) += 1;
            if e.version() == 1 {
                assert_eq!(
                    id, ITERATION_LIST,
                    "ver_ == 1 outside the iteration file: {id}"
                );
            }
        }
    }
    assert_eq!(
        total,
        [s.portal(), s.cell(), s.local(), s.highres().unwrap()]
            .iter()
            .map(|f| f.len())
            .sum::<usize>()
    );
    assert_eq!(versions[&1], 4, "one iteration file per dat");
    assert_eq!(versions.keys().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
}

/// The tree lookup must agree with the in-order walk that built the directory. Checked
/// over every id in the three small dats and a strided sample of the portal dat's 79,694.
#[test]
fn tree_lookup_agrees_with_the_directory() {
    let s = store();
    for f in [s.local(), s.highres().unwrap()] {
        for (id, e) in f.iter_entries() {
            assert_eq!(f.lookup_via_tree(id).unwrap().as_ref(), Some(e), "{id}");
        }
    }
    for (i, (id, e)) in s.portal().iter_entries().enumerate() {
        if i % 37 == 0 {
            assert_eq!(
                s.portal().lookup_via_tree(id).unwrap().as_ref(),
                Some(e),
                "{id}"
            );
        }
    }
    // A miss is a miss, not a panic.
    assert!(s
        .portal()
        .lookup_via_tree(DataId(0x0100_FFFE))
        .unwrap()
        .is_none());
    assert!(s.portal().lookup_via_tree(DataId(0)).unwrap().is_none());
}

/// Every id divines to the dat it was found in.
#[test]
fn every_id_divines_to_the_dat_it_was_found_in() {
    let s = store();
    for (name, f, want) in [
        ("portal", s.portal(), DatKind::Portal),
        ("local", s.local(), DatKind::Local),
        ("highres", s.highres().unwrap(), DatKind::Portal),
    ] {
        for id in f.iter_ids() {
            if id == ITERATION_LIST {
                assert_eq!(divine_type(id), None, "the iteration list has no type");
                continue;
            }
            let t = divine_type(id).unwrap_or_else(|| panic!("{name}: {id} divines to nothing"));
            assert_eq!(dat_for_type(t), want, "{name}: {id} -> {t:?}");
        }
    }
    for id in s.cell().iter_ids() {
        if id == ITERATION_LIST {
            continue;
        }
        let t = classify_cell_id(id).unwrap_or_else(|| panic!("cell: {id} classifies to nothing"));
        assert_eq!(dat_for_type(t), DatKind::Cell, "cell: {id} -> {t:?}");
    }
}

/// Every typed id comes from its corresponding container, with disjoint portal partitions.
#[test]
fn every_typed_id_matches_its_source_directory() {
    let s = store();
    let containers = [s.portal(), s.cell(), s.local(), s.highres().unwrap()];
    let mut expected: std::collections::BTreeMap<DbType, Vec<DataId>> =
        std::collections::BTreeMap::new();
    for (index, file) in containers.iter().enumerate() {
        for id in file.iter_ids().filter(|id| *id != ITERATION_LIST) {
            let kind = if index == 1 {
                classify_cell_id(id)
            } else {
                divine_type(id)
            }
            .expect("every asset has a type");
            expected.entry(kind).or_default().push(id);
        }
    }
    for (kind, mut ids) in expected {
        ids.sort_unstable();
        assert_eq!(
            s.ids_of(kind),
            ids,
            "every {kind:?} id is returned from its source container"
        );
    }
    let hi: std::collections::BTreeSet<DataId> = s.highres().unwrap().iter_ids().collect();
    let overlap: Vec<DataId> = s.portal().iter_ids().filter(|i| hi.contains(i)).collect();
    assert_eq!(
        overlap,
        vec![ITERATION_LIST],
        "portal and high-res object ids must not intersect"
    );
}

/// A `DatFile` that is not a dat must fail cleanly, not panic.
#[test]
fn a_non_dat_file_fails_with_bad_magic() {
    let dir = std::env::temp_dir().join("dereth_dat_bad_magic_test");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("not_a_dat.bin");
    std::fs::write(&p, vec![0u8; 0x400]).unwrap();
    assert!(matches!(
        DatFile::open(&p),
        Err(dereth_dat::DatError::BadMagic(0))
    ));
    let _ = std::fs::remove_file(&p);
}

#[test]
fn every_payload_in_every_dat_reads_without_error() {
    let s = store();
    let mut files = 0usize;
    let mut bytes = 0usize;
    for f in [s.portal(), s.cell(), s.local(), s.highres().unwrap()] {
        for (id, e) in f.iter_entries() {
            let b = f.read(id).unwrap_or_else(|err| panic!("{id}: {err}"));
            assert_eq!(b.len(), e.size as usize, "{id} short payload");
            files += 1;
            bytes += b.len();
        }
    }
    let containers = [s.portal(), s.cell(), s.local(), s.highres().unwrap()];
    assert_eq!(files, containers.iter().map(|f| f.len()).sum::<usize>());
    assert_eq!(
        bytes,
        containers
            .iter()
            .flat_map(|f| f.iter_entries())
            .map(|(_, entry)| entry.size as usize)
            .sum::<usize>()
    );
    // Sanity: the four files are 1.4 GB on disk and the payloads are most of that.
    assert!(bytes > 0, "only {bytes} payload bytes");
}
