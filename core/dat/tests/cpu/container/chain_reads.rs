//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! A chain's blocks in a row come in one read, and a chain reads the same however it lies.
//! Fixture: containers built byte by byte in the test.

use std::sync::{Arc, Mutex};

use dereth_dat::container::{DatFile, DatStorage};
use dereth_dat::DatError;
use dereth_primitives::DataId;

const BLOCK: usize = 0x100;
const PER: usize = BLOCK - 4;
const ID: u32 = 0x0100_0001;

/// Bytes in memory, remembering the size of every read.
#[derive(Debug)]
struct Counted {
    bytes: Vec<u8>,
    reads: Arc<Mutex<Vec<usize>>>,
}

impl DatStorage for Counted {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.reads.lock().unwrap().push(buf.len());
        let at = usize::try_from(offset).unwrap();
        let end = at + buf.len();
        if end > self.bytes.len() {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        buf.copy_from_slice(&self.bytes[at..end]);
        Ok(())
    }
}

/// The payload's bytes: none repeats within a block's length of itself.
fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|k| u8::try_from(k % 251).unwrap()).collect()
}

/// A container with one record, its chain broken into runs of the given blocks, a free block between
/// each run. `spoil` sets a block of the chain free (its link's bit 31), by its place in the chain.
fn container(data: &[u8], runs: &[usize], spoil: Option<usize>) -> Vec<u8> {
    let mut bytes = vec![0u8; 0x400];
    let total: usize = runs.iter().sum();
    let mut places = Vec::new();
    let mut at = 0x400;
    for (r, run) in runs.iter().enumerate() {
        for _ in 0..*run {
            places.push(at);
            at += BLOCK;
        }
        if r + 1 < runs.len() {
            at += BLOCK;
        }
    }
    bytes.resize(at, 0);
    for (k, place) in places.iter().enumerate() {
        let next = places.get(k + 1).copied().unwrap_or(0);
        let mut link = u32::try_from(next).unwrap();
        if spoil == Some(k) {
            link |= 0x8000_0000;
        }
        bytes[*place..*place + 4].copy_from_slice(&link.to_le_bytes());
        let part = &data[(k * PER).min(data.len())..((k + 1) * PER).min(data.len())];
        bytes[*place + 4..*place + 4 + part.len()].copy_from_slice(part);
    }
    assert!(total * PER >= data.len());
    // The directory, one leaf: 62 child words of nothing, a count, one 12-byte entry.
    let node = {
        let mut n = vec![0u8; 984];
        n[248..252].copy_from_slice(&1u32.to_le_bytes());
        n[252..256].copy_from_slice(&ID.to_le_bytes());
        n[256..260].copy_from_slice(&0x400u32.to_le_bytes());
        n[260..264].copy_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        n
    };
    let root = bytes.len();
    let blocks = node.len().div_ceil(PER);
    for k in 0..blocks {
        let next = if k + 1 == blocks {
            0
        } else {
            u32::try_from(bytes.len() + BLOCK).unwrap()
        };
        let here = bytes.len();
        bytes.resize(here + BLOCK, 0);
        bytes[here..here + 4].copy_from_slice(&next.to_le_bytes());
        let part = &node[k * PER..((k + 1) * PER).min(node.len())];
        bytes[here + 4..here + 4 + part.len()].copy_from_slice(part);
    }
    let size = u32::try_from(bytes.len()).unwrap();
    let root = u32::try_from(root).unwrap();
    for (i, w) in [0x5442, 0x100, size, 1, 0, 0, 1, root, 0, 0, 0]
        .into_iter()
        .enumerate()
    {
        bytes[0x12C + i * 4..0x12C + i * 4 + 4].copy_from_slice(&u32::to_le_bytes(w));
    }
    bytes
}

/// The record read from the bytes, and the sizes of the reads that took.
fn read(bytes: Vec<u8>) -> (Result<Vec<u8>, DatError>, Vec<usize>) {
    let reads = Arc::new(Mutex::new(Vec::new()));
    let storage = Counted {
        bytes,
        reads: Arc::clone(&reads),
    };
    let file = DatFile::from_storage("cell.dat".into(), Box::new(storage)).expect("opens");
    reads.lock().unwrap().clear();
    let got = file.read(DataId(ID));
    let sizes = reads.lock().unwrap().clone();
    (got, sizes)
}

#[test]
fn a_chain_in_a_row_comes_in_one_read() {
    let data = payload(PER * 12 + 5);
    let (got, sizes) = read(container(&data, &[13], None));
    assert_eq!(got.unwrap(), data);
    assert_eq!(sizes, [BLOCK * 12 + 5 + 4]);
}

#[test]
fn a_long_chain_reads_at_most_a_limited_amount_at_once() {
    let data = payload(PER * 300);
    let (got, sizes) = read(container(&data, &[300], None));
    assert_eq!(got.unwrap(), data);
    // Whole blocks up to the limit; the last read asks only for what's left of the last block.
    assert_eq!(sizes.len(), 3);
    assert!(sizes.iter().all(|s| *s <= 32 * 1024), "{sizes:?}");
}

#[test]
fn a_chain_with_a_gap_takes_one_read_per_run_and_reads_the_same() {
    let data = payload(PER * 9);
    let (got, sizes) = read(container(&data, &[4, 3, 2], None));
    assert_eq!(got.unwrap(), data);
    assert_eq!(sizes.len(), 3);
}

#[test]
fn a_one_block_record_asks_for_only_its_own_bytes() {
    let data = payload(10);
    let (got, sizes) = read(container(&data, &[1], None));
    assert_eq!(got.unwrap(), data);
    assert_eq!(sizes, [14]);
}

#[test]
fn a_free_block_in_the_middle_of_a_run_stops_the_read() {
    let data = payload(PER * 8);
    let (got, _) = read(container(&data, &[8], Some(2)));
    let offset = u32::try_from(0x400 + 2 * BLOCK).unwrap();
    assert!(
        matches!(got, Err(DatError::FreeBlockInChain { offset: o, .. }) if o == offset),
        "{got:?}"
    );
}

#[test]
fn a_chain_that_ends_early_says_how_much_is_missing() {
    let data = payload(PER * 8);
    let mut bytes = container(&data, &[8], None);
    let place = 0x400 + 3 * BLOCK;
    bytes[place..place + 4].copy_from_slice(&0u32.to_le_bytes());
    let (got, _) = read(bytes);
    assert!(
        matches!(got, Err(DatError::ChainTooShort { shortfall, .. }) if shortfall == PER * 4),
        "{got:?}"
    );
}

#[test]
fn a_read_ahead_past_the_end_of_the_file_falls_back_to_the_one_block() {
    // The directory says six blocks, and the file ends after the record's first: read whole, that
    // block's link of nothing says what's wrong, as it does with every block read alone.
    let data = payload(10);
    let mut bytes = container(&data, &[1], None);
    let root = usize::try_from(u32::from_le_bytes(
        bytes[0x12C + 28..0x12C + 32].try_into().unwrap(),
    ))
    .unwrap();
    let last = bytes.len();
    bytes.resize(last + BLOCK, 0);
    let entry = root + BLOCK + 4;
    bytes[entry + 4..entry + 8].copy_from_slice(&u32::try_from(last).unwrap().to_le_bytes());
    bytes[entry + 8..entry + 12].copy_from_slice(&u32::try_from(PER * 6).unwrap().to_le_bytes());
    let size = u32::try_from(bytes.len()).unwrap();
    bytes[0x12C + 8..0x12C + 12].copy_from_slice(&size.to_le_bytes());
    let (got, sizes) = read(bytes);
    assert!(
        matches!(got, Err(DatError::ChainTooShort { shortfall, .. }) if shortfall == PER * 5),
        "{got:?}"
    );
    assert_eq!(sizes.len(), 2, "the read ahead, then the one block");
}
