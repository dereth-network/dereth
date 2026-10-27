//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! A container in a layout the caller describes (where the header is and where each of its fields
//! sits, the directory entry's size and fields, a transaction slot the writer leaves alone) is
//! created, written, reopened and walked through that layout; its bytes are where the layout puts
//! them, and the built-in detection does not mistake it for either dat set.
//! Fixture: containers written in memory by the crate's own writer and built byte by byte in the
//! test.

use dereth_dat::container::DiskFileInfo;
use dereth_dat::folder::MemoryFile;
use dereth_dat::write::{DatWriter, SaveOutcome};
use dereth_dat::{
    ContainerEra, ContainerLayout, DatError, DatFile, EntryLayout, HeaderLayout, TransactionSlot,
};
use dereth_primitives::DataId;

/// A layout neither dat set uses: the header at `0x180` with its fields in another order, a
/// 20-byte entry with the date first and no flags word, and a 48-byte transaction slot that holds
/// no journal.
const OWN: ContainerLayout = ContainerLayout {
    header: HeaderLayout {
        at: 0x180,
        bytes: 0x48,
        file_size: 0x10,
        data_set: Some(0x1C),
        data_subset: Some(0x20),
        free_head: 0x08,
        free_tail: 0x0C,
        free_count: 0x14,
        btree_root: 0x18,
        young_lru: None,
        old_lru: None,
        use_lru: Some(0x44),
        master_map_id: Some(0x24),
        eng_pack_vnum: Some(0x3C),
        game_pack_vnum: Some(0x40),
        version_major: Some(0x2C),
        version_minor: Some(0x28),
        iteration: None,
    },
    entry: EntryLayout {
        bytes: 20,
        bits: None,
        id: 4,
        offset: 12,
        size: 8,
        date: Some(0),
        iteration: Some(16),
    },
    transaction: Some(TransactionSlot {
        at: 0x100,
        bytes: 0x30,
        journal: false,
    }),
    era: ContainerEra::Modern,
};

fn info(block_size: u32, blocks: u32) -> DiskFileInfo {
    DiskFileInfo {
        magic: 0x5442,
        block_size,
        file_size: 0x400 + block_size * blocks,
        data_set: 7,
        data_subset: 0x0BAD_F00D,
        free_head: 0,
        free_tail: 0,
        free_count: 0,
        btree_root: 0,
        young_lru: 0,
        old_lru: 0,
        use_lru: false,
        master_map_id: 0x1234_5678,
        eng_pack_vnum: 42,
        game_pack_vnum: 9,
        version_major: *b"0123456789abcdef",
        version_minor: 0x77,
    }
}

fn payload(id: u32) -> Vec<u8> {
    let len = 1 + (id as usize * 37) % 900;
    (0..len)
        .map(|k| u8::try_from((k + id as usize) % 251).unwrap())
        .collect()
}

/// Two hundred records in 0x100-byte blocks: the root splits, every node spans six blocks, and the
/// container reopens through the layout with every record, date and iteration where it was put.
#[test]
fn a_container_in_a_layout_of_its_own_is_written_reopened_and_walked() {
    let file = MemoryFile::default();
    let ids: Vec<u32> = (0..200u32)
        .map(|i| 0x0600_0000 + (i * 7919) % 4001)
        .collect();
    {
        let mut w = DatWriter::create_in_as(
            "own.dat".into(),
            Box::new(file.clone()),
            &OWN,
            &info(0x100, 4096),
        )
        .expect("create");
        for (i, id) in ids.iter().enumerate() {
            let i = u32::try_from(i).unwrap();
            let got = w
                .save(DataId(*id), &payload(*id), 3, 10 + i, 1_000_000 + i)
                .expect("save");
            assert_eq!(got, SaveOutcome::Added);
        }
    }
    let bytes = file.bytes();
    let dat = DatFile::from_storage_as("own.dat".into(), Box::new(bytes.clone()), &OWN)
        .expect("opens in its layout");

    assert_eq!(dat.layout(), &OWN);
    let h = dat.header();
    assert_eq!(
        (h.block_size, h.data_set, h.data_subset),
        (0x100, 7, 0x0BAD_F00D)
    );
    assert_eq!(
        (h.master_map_id, h.eng_pack_vnum, h.game_pack_vnum),
        (0x1234_5678, 42, 9)
    );
    assert_eq!(
        (&h.version_major, h.version_minor),
        (b"0123456789abcdef", 0x77)
    );
    assert_eq!(dat.len(), ids.len());
    for (i, id) in ids.iter().enumerate() {
        let i = u32::try_from(i).unwrap();
        assert_eq!(dat.read(DataId(*id)).expect("reads"), payload(*id));
        let e = dat.entry(DataId(*id)).expect("entry");
        assert_eq!((e.date, e.iteration), (1_000_000 + i, 10 + i));
        assert_eq!(
            e.bits, 0,
            "the layout has no flags word to keep the version in"
        );
        assert_eq!(dat.lookup_via_tree(DataId(*id)).expect("search"), Some(*e));
    }
    let report = dat.verify_structure().expect("walks");
    assert!(report.is_sound(), "{report:?}");
    assert!(report.nodes > 1, "the root split");
    assert_eq!(
        report.blocks_used + report.free_chain_len,
        report.total_data_blocks
    );

    // The header is where the layout puts it, and the slot without a journal was left alone.
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!(word(0x180), 0x5442);
    assert_eq!(word(0x180 + 0x10), h.file_size);
    assert_eq!(word(0x180 + 0x18), h.btree_root);
    assert!(bytes[0x100..0x130].iter().all(|b| *b == 0));
    assert_eq!(dat.transaction_slot().expect("slot"), vec![0u8; 0x30]);

    // Neither built-in layout reads it.
    assert!(matches!(
        DatFile::from_storage("own.dat".into(), Box::new(bytes)),
        Err(DatError::BadMagic(_))
    ));
}

/// A container built byte by byte, with the entry's fields placed by hand where the layout says:
/// the reader takes each field from its own place, not from a built-in layout's.
#[test]
fn the_reader_takes_each_entry_field_from_where_the_layout_puts_it() {
    let block = 0x400usize;
    let mut bytes = vec![0u8; 0x400];
    // One record in one block at 0x400.
    let record = b"a record in a layout of its own".to_vec();
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&record);
    bytes.resize(0x400 + block, 0);
    // The root, a leaf of one entry, in two blocks at 0x800.
    let mut node = vec![0u8; OWN.node_bytes()];
    node[248..252].copy_from_slice(&1u32.to_le_bytes());
    let e = 252;
    for (at, v) in [
        (0, 1_500_000_000u32),
        (4, 0x0E00_0042),
        (8, u32::try_from(record.len()).unwrap()),
        (12, 0x400),
        (16, 5),
    ] {
        node[e + at..e + at + 4].copy_from_slice(&v.to_le_bytes());
    }
    let per = block - 4;
    bytes.extend_from_slice(&0xC00u32.to_le_bytes());
    bytes.extend_from_slice(&node[..per]);
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&node[per..]);
    bytes.resize(0x400 + 3 * block, 0);
    let size = u32::try_from(bytes.len()).unwrap();
    for (at, v) in [(0, 0x5442), (4, 0x400), (0x10, size), (0x18, 0x800)] {
        bytes[0x180 + at..0x180 + at + 4].copy_from_slice(&v.to_le_bytes());
    }

    let dat = DatFile::from_storage_as("hand.dat".into(), Box::new(bytes), &OWN).expect("opens");
    let got = dat.entry(DataId(0x0E00_0042)).expect("the entry");
    assert_eq!(
        (got.offset, got.size, got.date, got.iteration),
        (
            0x400,
            u32::try_from(record.len()).unwrap(),
            1_500_000_000,
            5
        )
    );
    assert_eq!(dat.read(DataId(0x0E00_0042)).expect("reads"), record);
    assert!(dat.verify_structure().expect("walks").is_sound());
}

/// The built-in layouts named explicitly open a file exactly as the detection does.
#[test]
fn naming_a_built_in_layout_opens_the_file_as_detection_does() {
    let file = MemoryFile::default();
    {
        let mut w = DatWriter::create_in(
            "modern.dat".into(),
            Box::new(file.clone()),
            0x400,
            1,
            0,
            0x400 + 0x400 * 16,
        )
        .expect("create");
        w.save(DataId(0x0600_0001), b"one", 1, 1, 1).expect("save");
    }
    let detected =
        DatFile::from_storage("modern.dat".into(), Box::new(file.bytes())).expect("opens");
    let named = DatFile::from_storage_as(
        "modern.dat".into(),
        Box::new(file.bytes()),
        &ContainerLayout::MODERN,
    )
    .expect("opens");
    assert_eq!(detected.layout(), &ContainerLayout::MODERN);
    assert_eq!(named.header(), detected.header());
    assert_eq!(
        named.iter_entries().collect::<Vec<_>>(),
        detected.iter_entries().collect::<Vec<_>>()
    );
    assert_eq!(named.transaction_slot().expect("slot").len(), 0x40);
}

/// A layout whose fields do not fit is refused before anything is read or written.
#[test]
fn a_layout_that_does_not_fit_is_refused() {
    let mut bad = OWN;
    bad.entry.iteration = Some(18);
    let bytes = vec![0u8; 0x800];
    assert!(matches!(
        DatFile::from_storage_as("bad.dat".into(), Box::new(bytes), &bad),
        Err(DatError::BadLayout)
    ));
    assert!(matches!(
        DatWriter::create_in_as(
            "bad.dat".into(),
            Box::new(MemoryFile::default()),
            &bad,
            &info(0x400, 4),
        ),
        Err(DatError::BadLayout)
    ));
}
