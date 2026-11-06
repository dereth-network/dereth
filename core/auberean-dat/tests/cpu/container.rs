//! The container read back, in this game's layout, from a small file built in memory: a header, one directory node and
//! records chained across blocks, read by id, by tree descent and by a full walk.
//! Behaviour: none (the container's mechanics on a synthetic file)

// The files built here are a few kilobytes: every length and offset fits in 32 bits.
#![allow(clippy::cast_possible_truncation)]

use auberean_dat::container::{FIRST_BLOCK, HEADER_OFFSET, NODE_SIZE};
use auberean_dat::{DatError, DatFile};

const BLOCK: usize = 0x40;

/// A file of one leaf node at the first block and the given records, each chained across as many
/// blocks as it needs, the rest of the file free.
pub(crate) fn build(records: &[(u32, Vec<u8>)], spare_blocks: usize) -> Vec<u8> {
    let per = BLOCK - 4;
    let node_blocks = NODE_SIZE.div_ceil(per);
    let record_blocks: usize = records
        .iter()
        .map(|(_, b)| b.len().div_ceil(per).max(1))
        .sum();
    let total = node_blocks + record_blocks + spare_blocks;
    let mut f = vec![0u8; FIRST_BLOCK as usize + total * BLOCK];
    let block_at = |i: usize| FIRST_BLOCK as usize + i * BLOCK;
    let write_chain = |f: &mut Vec<u8>, first: usize, data: &[u8]| {
        let n = data.len().div_ceil(per).max(1);
        for k in 0..n {
            let at = block_at(first + k);
            let next = if k + 1 < n {
                block_at(first + k + 1) as u32
            } else {
                0
            };
            f[at..at + 4].copy_from_slice(&next.to_le_bytes());
            let chunk = &data[(k * per).min(data.len())..((k + 1) * per).min(data.len())];
            f[at + 4..at + 4 + chunk.len()].copy_from_slice(chunk);
        }
        n
    };
    let mut node = vec![0u8; NODE_SIZE];
    node[248..252].copy_from_slice(&(records.len() as u32).to_le_bytes());
    let mut next = node_blocks;
    for (i, (id, data)) in records.iter().enumerate() {
        let e = 252 + i * 16;
        node[e..e + 4].copy_from_slice(&id.to_le_bytes());
        node[e + 4..e + 8].copy_from_slice(&(block_at(next) as u32).to_le_bytes());
        node[e + 8..e + 12].copy_from_slice(&(data.len() as u32).to_le_bytes());
        node[e + 12..e + 16].copy_from_slice(&0x5000_0000u32.to_le_bytes());
        next += write_chain(&mut f, next, data);
    }
    write_chain(&mut f, 0, &node);
    // The spare blocks, chained as the free list (bit 31 set on every link but the last's 0).
    for k in 0..spare_blocks {
        let at = block_at(next + k);
        let link = if k + 1 < spare_blocks {
            block_at(next + k + 1) as u32 | 0x8000_0000
        } else {
            0x8000_0000
        };
        f[at..at + 4].copy_from_slice(&link.to_le_bytes());
    }
    let h = HEADER_OFFSET as usize;
    let words: [u32; 21] = [
        0x5442,
        BLOCK as u32,
        f.len() as u32,
        1,
        0,
        if spare_blocks > 0 {
            block_at(next) as u32
        } else {
            0
        },
        0,
        spare_blocks as u32,
        FIRST_BLOCK,
        0,
        0,
        0,
        0,
        104,
        3,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    for (i, w) in words.iter().enumerate() {
        f[h + i * 4..h + i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    f
}

fn sample() -> Vec<(u32, Vec<u8>)> {
    vec![
        (0x0600_0001, (0u8..10).collect()),
        (0x0600_0002, (0u8..200).collect()),
        (
            0xFFFF_0001,
            [3i32, -3, 1].iter().flat_map(|w| w.to_le_bytes()).collect(),
        ),
    ]
}

#[test]
fn a_record_chained_across_blocks_reads_back_whole() {
    let dat = DatFile::from_bytes(build(&sample(), 2)).unwrap();
    assert_eq!(dat.len(), 3);
    assert_eq!(
        dat.read(0x0600_0002).unwrap(),
        (0u8..200).collect::<Vec<u8>>()
    );
    assert_eq!(
        dat.read(0x0600_0001).unwrap(),
        (0u8..10).collect::<Vec<u8>>()
    );
}

#[test]
fn a_tree_descent_finds_what_the_directory_holds_and_nothing_else() {
    let dat = DatFile::from_bytes(build(&sample(), 0)).unwrap();
    for e in dat.entries() {
        assert_eq!(dat.lookup_via_tree(e.id).unwrap().as_ref(), Some(e));
    }
    assert_eq!(dat.lookup_via_tree(0x0600_0003).unwrap(), None);
    assert!(
        matches!(dat.read(0x0600_0003), Err(DatError::NotFound(id)) if id.raw() == 0x0600_0003),
        "an id the directory does not hold is not found"
    );
}

#[test]
fn a_well_formed_file_walks_soundly_and_counts_its_free_blocks() {
    let dat = DatFile::from_bytes(build(&sample(), 3)).unwrap();
    let s = dat.verify().unwrap();
    assert_eq!(s.free_chain_len, 3);
    assert!(auberean_dat::container::sound(&s), "{s:?}");
}

#[test]
fn the_iteration_list_reads_from_its_record() {
    let dat = DatFile::from_bytes(build(&sample(), 0)).unwrap();
    assert_eq!(dat.iterations().unwrap(), vec![1, 2, 3]);
}

#[test]
fn a_file_without_the_magic_is_refused() {
    let mut f = build(&sample(), 0);
    f[HEADER_OFFSET as usize] = 0;
    assert!(matches!(DatFile::from_bytes(f), Err(DatError::BadMagic(_))));
}

#[test]
fn the_start_of_a_record_is_what_its_first_block_holds() {
    let long: Vec<u8> = (0..200).map(|i| i as u8).collect();
    let d = DatFile::from_bytes(build(&[(0x0100, long.clone()), (0x0200, vec![7; 5])], 1))
        .expect("opens");
    assert_eq!(d.read_start(0x0100).expect("read"), long[..BLOCK - 4]);
    assert_eq!(d.read_start(0x0200).expect("read"), [7; 5]);
    assert!(matches!(d.read_start(0x0300), Err(DatError::NotFound(_))));
}
