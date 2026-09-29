//! Reading a dat's iteration number straight off disk.
//!
//! A world compares the client's data files by one number per file: the iteration count kept in
//! the file's iteration list, file id `0xFFFF0001`. The launcher reads the same number from the
//! same place, which is how it can say "these files are older than this world expects" before the
//! client connects rather than after the server turns it away.
//!
//! This is a reader for exactly that, not a general dat library. It reads the header, walks the
//! directory B-tree to one entry, follows that entry's block chain and decodes a few dozen bytes.
//! That is a handful of small reads however large the file is, so the pre-launch check can afford
//! to re-read on every PLAY. The layout is the container's: a header at 0x140, fixed-size blocks
//! chained by a leading link, a B-tree directory of 62-way nodes, and the iteration list as a
//! run-length integer set.
//!
//! Every length and offset read from the file is bounded before it is used. A dat is a file the
//! player pointed the launcher at, and a truncated download or a folder of something else must come
//! back as an error, never as a panic or an unbounded allocation.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// The id of the iteration list inside every dat.
pub const ITERATION_FILE_ID: u32 = 0xFFFF_0001;

/// Where the header sits, after the 256-byte banner and the 64-byte journal slot.
const HEADER_OFFSET: u64 = 0x140;
/// The header's magic, `"BT"` read as bytes.
const MAGIC: u32 = 0x5442;
/// A directory node: 62 child offsets, an entry count, 61 entries of 24 bytes.
const NODE_SIZE: usize = 0x6B4;
const NODE_CHILDREN: usize = 62;
const NODE_ENTRIES: usize = 61;
const ENTRY_SIZE: usize = 24;
const COUNT_AT: usize = NODE_CHILDREN * 4;
const ENTRIES_AT: usize = COUNT_AT + 4;
/// A directory deeper than this is a loop, not a tree. Retail's deepest is 3 levels.
const MAX_DEPTH: usize = 16;
/// The iteration list is 12 bytes in every retail file. Anything past this is not one.
const MAX_ITERATION_FILE: u32 = 64 * 1024;

/// Which of the four files a dat is, from its header rather than its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatKind {
    Portal,
    /// The landscape and dungeon cells for one region. Retail has only region 1.
    Cell {
        region: u32,
    },
    /// Strings for one language. English is 1.
    Local {
        language: u32,
    },
    /// The optional high-resolution texture overlay on the portal file.
    HighRes,
    /// A dat, but not one of the four the client opens.
    Other {
        data_set: u32,
        subset: u32,
    },
}

/// The subset value of the high-resolution portal file: `"HiFi"` read as a little-endian dword.
const HIGHRES_SUBSET: u32 = 0x6946_6948;

impl DatKind {
    fn from_header(data_set: u32, subset: u32) -> Self {
        match (data_set, subset) {
            (1, 0) => DatKind::Portal,
            (1, HIGHRES_SUBSET) => DatKind::HighRes,
            (2, region) => DatKind::Cell { region },
            (3, language) => DatKind::Local { language },
            (data_set, subset) => DatKind::Other { data_set, subset },
        }
    }
}

/// The parts of the header this reader uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatHeader {
    pub block_size: u32,
    pub file_size: u32,
    pub data_set: u32,
    pub data_subset: u32,
    pub btree_root: u32,
}

/// What the iteration list says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IterationSet {
    /// How many iterations the file holds. This is the number a world compares.
    pub count: u32,
    /// The highest iteration present. Equal to `count` unless the list has gaps, which no retail
    /// file has; a difference means the file was patched out of order.
    pub highest: u32,
}

/// One dat, read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatInfo {
    pub kind: DatKind,
    pub header: DatHeader,
    pub iterations: IterationSet,
}

#[derive(Debug)]
pub enum DatError {
    Io(io::Error),
    /// The magic is wrong: this is not a dat at all.
    NotADat,
    /// It is a dat, but something in it does not add up. The text says what.
    Corrupt(&'static str),
    /// The iteration list is stored compressed. No retail file does this, and reading it would
    /// need a decompressor the launcher otherwise has no use for.
    Compressed,
}

impl core::fmt::Display for DatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DatError::Io(e) => write!(f, "{e}"),
            DatError::NotADat => write!(f, "not a data file"),
            DatError::Corrupt(what) => write!(f, "damaged data file: {what}"),
            DatError::Compressed => write!(f, "the iteration list is compressed"),
        }
    }
}

impl std::error::Error for DatError {}

impl From<io::Error> for DatError {
    fn from(e: io::Error) -> Self {
        // A file shorter than its own header says it is: truncated, not unreadable.
        if e.kind() == io::ErrorKind::UnexpectedEof {
            DatError::Corrupt("the file ends early")
        } else {
            DatError::Io(e)
        }
    }
}

/// Read the kind and iteration list of the dat at `path`.
pub fn read_dat(path: &Path) -> Result<DatInfo, DatError> {
    read_dat_from(&mut File::open(path)?)
}

/// The same, from anything seekable. Tests build dats in memory.
pub fn read_dat_from<R: Read + Seek>(r: &mut R) -> Result<DatInfo, DatError> {
    let header = read_header(r)?;
    let entry =
        lookup(r, &header, ITERATION_FILE_ID)?.ok_or(DatError::Corrupt("no iteration list"))?;
    if entry.flags & 1 != 0 {
        return Err(DatError::Compressed);
    }
    if entry.size > MAX_ITERATION_FILE || entry.size < 4 {
        return Err(DatError::Corrupt("iteration list has an impossible size"));
    }
    let bytes = read_chain(r, &header, entry.offset, entry.size as usize)?;
    let iterations = decode_iterations(&bytes)?;
    Ok(DatInfo {
        kind: DatKind::from_header(header.data_set, header.data_subset),
        header,
        iterations,
    })
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn read_header<R: Read + Seek>(r: &mut R) -> Result<DatHeader, DatError> {
    let mut h = [0u8; 0x24];
    r.seek(SeekFrom::Start(HEADER_OFFSET))?;
    r.read_exact(&mut h)?;
    if u32_at(&h, 0) != MAGIC {
        return Err(DatError::NotADat);
    }
    let header = DatHeader {
        block_size: u32_at(&h, 0x04),
        file_size: u32_at(&h, 0x08),
        data_set: u32_at(&h, 0x0C),
        data_subset: u32_at(&h, 0x10),
        btree_root: u32_at(&h, 0x20),
    };
    // Retail uses 0x100 and 0x400. Anything outside a sane range would make every later
    // calculation meaningless, so it is refused here once.
    if !(0x40..=0x10000).contains(&header.block_size) {
        return Err(DatError::Corrupt("block size out of range"));
    }
    Ok(header)
}

/// Read `size` payload bytes from the block chain starting at `offset`.
///
/// Each block begins with a four-byte link to the next; the last block's link is zero, and a link
/// with the top bit set means the block is free, which a live chain must never reach.
fn read_chain<R: Read + Seek>(
    r: &mut R,
    header: &DatHeader,
    mut offset: u32,
    size: usize,
) -> Result<Vec<u8>, DatError> {
    let payload = header.block_size as usize - 4;
    let mut out = vec![0u8; size];
    let mut filled = 0;
    // A chain cannot have more blocks than the payload needs; one that tries is a loop.
    let mut budget = size / payload + 1;
    while filled < size {
        if offset == 0 || offset & 0x8000_0000 != 0 {
            return Err(DatError::Corrupt("a block chain breaks off"));
        }
        if budget == 0 {
            return Err(DatError::Corrupt("a block chain loops"));
        }
        budget -= 1;
        if u64::from(offset) + u64::from(header.block_size) > u64::from(header.file_size) {
            return Err(DatError::Corrupt("a block lies past the end of the file"));
        }
        r.seek(SeekFrom::Start(u64::from(offset)))?;
        let mut link = [0u8; 4];
        r.read_exact(&mut link)?;
        let n = payload.min(size - filled);
        r.read_exact(&mut out[filled..filled + n])?;
        filled += n;
        offset = u32::from_le_bytes(link);
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    flags: u32,
    offset: u32,
    size: u32,
}

/// Find `id` in the directory.
fn lookup<R: Read + Seek>(
    r: &mut R,
    header: &DatHeader,
    id: u32,
) -> Result<Option<Entry>, DatError> {
    let mut node_at = header.btree_root;
    for _ in 0..MAX_DEPTH {
        let node = read_chain(r, header, node_at, NODE_SIZE)?;
        let count = u32_at(&node, COUNT_AT) as usize;
        if count > NODE_ENTRIES {
            return Err(DatError::Corrupt("a directory node has too many entries"));
        }
        let key = |i: usize| u32_at(&node, ENTRIES_AT + i * ENTRY_SIZE + 4);
        // Entries are sorted ascending by id; find the first not below `id`.
        let (mut lo, mut hi) = (0, count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if key(mid) < id {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo < count && key(lo) == id {
            let at = ENTRIES_AT + lo * ENTRY_SIZE;
            return Ok(Some(Entry {
                flags: u32_at(&node, at),
                offset: u32_at(&node, at + 8),
                size: u32_at(&node, at + 12),
            }));
        }
        // A zero first child marks a leaf.
        if u32_at(&node, 0) == 0 {
            return Ok(None);
        }
        node_at = u32_at(&node, lo * 4);
    }
    Err(DatError::Corrupt(
        "the directory is deeper than any real one",
    ))
}

/// Decode the run-length integer set: a count, then items where a non-negative value stands for
/// itself and a negative `-k` is followed by `first` and stands for `first..first+k`.
fn decode_iterations(b: &[u8]) -> Result<IterationSet, DatError> {
    let count = u32_at(b, 0);
    let mut seen: u64 = 0;
    let mut highest: u32 = 0;
    let mut at = 4;
    while at + 4 <= b.len() && seen < u64::from(count) {
        let v = u32_at(b, at) as i32;
        at += 4;
        if v >= 0 {
            seen += 1;
            highest = highest.max(v as u32);
        } else {
            if at + 4 > b.len() {
                return Err(DatError::Corrupt("the iteration list ends inside a run"));
            }
            let first = u32_at(b, at);
            at += 4;
            let k = v.unsigned_abs();
            seen += u64::from(k);
            let last = u64::from(first) + u64::from(k) - 1;
            highest = highest
                .max(u32::try_from(last).map_err(|_| DatError::Corrupt("iteration overflows"))?);
        }
    }
    if seen != u64::from(count) {
        return Err(DatError::Corrupt(
            "the iteration list does not add up to its count",
        ));
    }
    Ok(IterationSet { count, highest })
}

#[cfg(any(test, feature = "testing"))]
#[allow(clippy::cast_possible_truncation)]
pub mod testdat {
    //! Building a small but well-formed dat in memory, so the reader is tested on every host and
    //! not only where the retail files happen to be.

    use super::*;

    /// A dat with the given header identity and iteration list, with its directory spread over a
    /// root and one child so the reader has to descend.
    pub fn build(data_set: u32, subset: u32, iterations: &[u32]) -> Vec<u8> {
        let block = 0x100u32;
        let payload = (block - 4) as usize;
        let mut blocks: Vec<Vec<u8>> = Vec::new();
        let base = 0x400u32;
        // Lay a payload out as a chain; returns the first block's offset.
        let chain = |data: &[u8], blocks: &mut Vec<Vec<u8>>| -> u32 {
            let n = data.len().div_ceil(payload).max(1);
            let first = base + blocks.len() as u32 * block;
            for i in 0..n {
                let next = if i + 1 == n {
                    0
                } else {
                    first + (i as u32 + 1) * block
                };
                let mut b = next.to_le_bytes().to_vec();
                let lo = i * payload;
                let hi = (lo + payload).min(data.len());
                b.extend_from_slice(&data[lo..hi]);
                b.resize(block as usize, 0);
                blocks.push(b);
            }
            first
        };
        let iter_file = chain(&encode_iterations(iterations), &mut blocks);
        let iter_size = encode_iterations(iterations).len() as u32;

        let node = |children: &[u32], entries: &[(u32, u32, u32)]| {
            let mut n = vec![0u8; NODE_SIZE];
            for (i, c) in children.iter().enumerate() {
                n[i * 4..i * 4 + 4].copy_from_slice(&c.to_le_bytes());
            }
            n[COUNT_AT..COUNT_AT + 4].copy_from_slice(&(entries.len() as u32).to_le_bytes());
            for (i, (id, off, size)) in entries.iter().enumerate() {
                let at = ENTRIES_AT + i * ENTRY_SIZE;
                n[at..at + 4].copy_from_slice(&(1u32 << 16).to_le_bytes());
                n[at + 4..at + 8].copy_from_slice(&id.to_le_bytes());
                n[at + 8..at + 12].copy_from_slice(&off.to_le_bytes());
                n[at + 12..at + 16].copy_from_slice(&size.to_le_bytes());
            }
            n
        };
        // The iteration list sorts last, so it lives in the right-hand child.
        let leaf_lo = chain(
            &node(
                &[],
                &[(0x0600_0001, iter_file, 4), (0x0600_0002, iter_file, 4)],
            ),
            &mut blocks,
        );
        let leaf_hi = chain(
            &node(&[], &[(ITERATION_FILE_ID, iter_file, iter_size)]),
            &mut blocks,
        );
        let root = chain(
            &node(&[leaf_lo, leaf_hi], &[(0x0E00_0001, iter_file, 4)]),
            &mut blocks,
        );

        let file_size = base + blocks.len() as u32 * block;
        let mut out = vec![0u8; base as usize];
        let h = HEADER_OFFSET as usize;
        for (at, v) in [
            (0, MAGIC),
            (4, block),
            (8, file_size),
            (0xC, data_set),
            (0x10, subset),
            (0x20, root),
        ] {
            out[h + at..h + at + 4].copy_from_slice(&v.to_le_bytes());
        }
        for b in blocks {
            out.extend_from_slice(&b);
        }
        out
    }

    /// The writer's rule: runs of three or more collapse, singles stand alone.
    pub fn encode_iterations(values: &[u32]) -> Vec<u8> {
        let mut out = (values.len() as u32).to_le_bytes().to_vec();
        let mut i = 0;
        while i < values.len() {
            let mut j = i + 1;
            while j < values.len() && values[j] == values[j - 1] + 1 {
                j += 1;
            }
            if j - i >= 3 {
                out.extend_from_slice(&(-((j - i) as i32)).to_le_bytes());
                out.extend_from_slice(&values[i].to_le_bytes());
            } else {
                for v in &values[i..j] {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
            i = j;
        }
        out
    }

    /// End of retail's shape: every iteration from 1 to `n`.
    pub fn eor(data_set: u32, subset: u32, n: u32) -> Vec<u8> {
        build(data_set, subset, &(1..=n).collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    use super::testdat::*;
    use super::*;
    use std::io::Cursor;

    #[test]
    fn a_retail_shaped_list_reads_as_its_count() {
        let dat = eor(1, 0, 2072);
        let info = read_dat_from(&mut Cursor::new(dat)).unwrap();
        assert_eq!(info.kind, DatKind::Portal);
        assert_eq!(
            info.iterations,
            IterationSet {
                count: 2072,
                highest: 2072
            }
        );
    }

    #[test]
    fn the_retail_payload_decodes_exactly() {
        // client_portal.dat's iteration list, byte for byte.
        let raw = [0x18, 0x08, 0, 0, 0xE8, 0xF7, 0xFF, 0xFF, 1, 0, 0, 0];
        assert_eq!(
            decode_iterations(&raw).unwrap(),
            IterationSet {
                count: 2072,
                highest: 2072
            }
        );
    }

    #[test]
    fn each_file_is_told_apart_by_its_header_not_its_name() {
        for (set, sub, want) in [
            (1, 0, DatKind::Portal),
            (2, 1, DatKind::Cell { region: 1 }),
            (3, 1, DatKind::Local { language: 1 }),
            (1, HIGHRES_SUBSET, DatKind::HighRes),
        ] {
            let info = read_dat_from(&mut Cursor::new(eor(set, sub, 5))).unwrap();
            assert_eq!(info.kind, want);
        }
    }

    #[test]
    fn a_gap_in_the_list_shows_as_count_below_highest() {
        let info = read_dat_from(&mut Cursor::new(build(1, 0, &[1, 2, 3, 4, 7, 9]))).unwrap();
        assert_eq!(
            info.iterations,
            IterationSet {
                count: 6,
                highest: 9
            }
        );
    }

    #[test]
    fn something_that_is_not_a_dat_says_so() {
        let junk = vec![0u8; 0x1000];
        assert!(matches!(
            read_dat_from(&mut Cursor::new(junk)),
            Err(DatError::NotADat)
        ));
    }

    #[test]
    fn a_truncated_dat_is_an_error_not_a_panic() {
        let mut dat = eor(1, 0, 10);
        dat.truncate(dat.len() - 0x180);
        assert!(read_dat_from(&mut Cursor::new(dat)).is_err());
        assert!(read_dat_from(&mut Cursor::new(vec![0u8; 0x150])).is_err());
    }

    #[test]
    fn a_chain_that_runs_into_a_free_block_is_caught() {
        let mut dat = eor(1, 0, 10);
        // Mark the root node's second block as free.
        let root = u32_at(&dat, 0x160) as usize;
        let next = u32_at(&dat, root) | 0x8000_0000;
        dat[root..root + 4].copy_from_slice(&next.to_le_bytes());
        assert!(matches!(
            read_dat_from(&mut Cursor::new(dat)),
            Err(DatError::Corrupt(_))
        ));
    }

    /// The real files, when `DERETH_TEST_DAT_DIR` names them (the retail install is never in the
    /// repository).
    #[test]
    fn the_retail_dats_read_as_end_of_retail_when_present() {
        use dereth_dat::{testing, RetailDat};
        if !testing::have_dats() {
            return;
        }
        for (dat, want) in [
            (RetailDat::Portal, 2072),
            (RetailDat::Cell, 982),
            (RetailDat::Local, 994),
            (RetailDat::HighRes, 497),
        ] {
            let path = testing::dat_file(dat);
            let info = read_dat(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(info.iterations.count, want, "{}", path.display());
        }
    }
}
