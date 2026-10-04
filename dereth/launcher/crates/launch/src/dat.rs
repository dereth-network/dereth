//! Reading a dat's identity and iteration count straight off disk.
//!
//! A world compares the client's data files by one number per file: how many iterations the file
//! holds. The launcher reads the same number, which is how it can say "these files are older than
//! this world expects" before the client connects rather than after the server turns it away.
//!
//! The reading is the data-file crate's ([`dereth_dat::DatFile::read_iterations`]): the header,
//! then for a file from Throne of Destiny on one path down the directory to the iteration list
//! (`0xFFFF0001`), and for an older file (`portal.dat`, `cell.dat`) the iteration its header
//! keeps. A handful of small reads however large the file, so the pre-launch check can afford it
//! on every PLAY. Every length and offset is bounded before it is used: a truncated download or a
//! folder of something else comes back as an error, never as a panic or an unbounded allocation.

use std::path::Path;

pub use dereth_dat::{ContainerEra, DatError};

/// Which file a dat is, from its header rather than its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatKind {
    Portal,
    /// The landscape and dungeon cells for one region. Retail has only region 1; a file from
    /// before Throne of Destiny says 0.
    Cell {
        region: u32,
    },
    /// Strings for one language. English is 1.
    Local {
        language: u32,
    },
    /// The optional high-resolution texture overlay on the portal file.
    HighRes,
    /// A dat, but not one of the files the client opens.
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
    /// The container layout: the files before Throne of Destiny, or the later ones.
    pub era: ContainerEra,
    pub iterations: IterationSet,
}

impl From<dereth_dat::FileIterations> for DatInfo {
    fn from(f: dereth_dat::FileIterations) -> Self {
        Self {
            kind: DatKind::from_header(f.data_set, f.data_subset),
            era: f.era,
            iterations: IterationSet {
                count: f.count,
                highest: f.highest,
            },
        }
    }
}

/// Read the kind and iterations of the dat at `path`.
///
/// # Errors
/// [`DatError`] for a file that cannot be read, is not a dat, or does not add up.
pub fn read_dat(path: &Path) -> Result<DatInfo, DatError> {
    dereth_dat::DatFile::read_iterations(path).map(DatInfo::from)
}

/// The same, from bytes in memory. Tests build dats this way.
///
/// # Errors
/// As [`read_dat`].
pub fn read_dat_bytes(bytes: Vec<u8>) -> Result<DatInfo, DatError> {
    dereth_dat::DatFile::read_iterations_from("memory.dat".into(), Box::new(bytes))
        .map(DatInfo::from)
}

#[cfg(any(test, feature = "testing"))]
#[allow(clippy::cast_possible_truncation)]
pub mod testdat {
    //! Building a small but well-formed dat in memory, so the reader is tested on every host and
    //! not only where the retail files happen to be.

    use super::HIGHRES_SUBSET;

    /// The iteration list's id.
    pub const ITERATION_FILE_ID: u32 = 0xFFFF_0001;
    /// Where the header sits, after the 256-byte banner and the 64-byte journal slot.
    const HEADER_OFFSET: u64 = 0x140;
    /// The header's magic, `"BT"` read as bytes.
    const MAGIC: u32 = 0x5442;
    /// A directory node: 62 child offsets, an entry count, 61 entries of 24 bytes.
    const NODE_SIZE: usize = 0x6B4;
    const COUNT_AT: usize = 62 * 4;
    const ENTRIES_AT: usize = COUNT_AT + 4;
    const ENTRY_SIZE: usize = 24;

    /// The high-resolution portal's subset, for building one.
    pub const HIGHRES: u32 = HIGHRES_SUBSET;

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

    /// A file from before Throne of Destiny holding nothing but its header: the iteration is kept
    /// there, and the block size tells the cell file (0x100) from the portal file (0x400).
    pub fn pre_tod(cell: bool, iteration: u32) -> Vec<u8> {
        let block: u32 = if cell { 0x100 } else { 0x400 };
        let mut out = vec![0u8; 0x400 + block as usize];
        let size = out.len() as u32;
        // Magic, block size, file size, iteration, free head, free tail, free count, tree root.
        for (i, v) in [MAGIC, block, size, iteration, 0, 0, 0, 0x400]
            .iter()
            .enumerate()
        {
            out[0x12C + i * 4..0x12C + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
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

    #[test]
    fn a_retail_shaped_list_reads_as_its_count() {
        let info = read_dat_bytes(eor(1, 0, 2072)).unwrap();
        assert_eq!(info.kind, DatKind::Portal);
        assert_eq!(info.era, ContainerEra::Tod);
        assert_eq!(
            info.iterations,
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
            (1, HIGHRES, DatKind::HighRes),
        ] {
            assert_eq!(read_dat_bytes(eor(set, sub, 5)).unwrap().kind, want);
        }
    }

    #[test]
    fn a_gap_in_the_list_shows_as_count_below_highest() {
        let info = read_dat_bytes(build(1, 0, &[1, 2, 3, 4, 7, 9])).unwrap();
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
        assert!(matches!(
            read_dat_bytes(vec![0u8; 0x1000]),
            Err(DatError::BadMagic(_))
        ));
    }

    #[test]
    fn a_truncated_dat_is_an_error_not_a_panic() {
        let mut dat = eor(1, 0, 10);
        dat.truncate(dat.len() - 0x180);
        assert!(read_dat_bytes(dat).is_err());
        assert!(read_dat_bytes(vec![0u8; 0x150]).is_err());
    }

    #[test]
    fn a_file_from_before_throne_of_destiny_reads_its_header_iteration() {
        let info = read_dat_bytes(pre_tod(false, 2112)).unwrap();
        assert_eq!(
            (info.kind, info.era, info.iterations.count),
            (DatKind::Portal, ContainerEra::PreTod, 2112)
        );
        let info = read_dat_bytes(pre_tod(true, 1593)).unwrap();
        assert_eq!(info.kind, DatKind::Cell { region: 0 });
        assert_eq!(info.iterations.count, 1593);
    }

    #[test]
    fn a_chain_that_runs_into_a_free_block_is_caught() {
        let mut dat = eor(1, 0, 10);
        // Mark the root node's second block as free.
        let at = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let root = at(&dat, 0x160) as usize;
        let next = at(&dat, root) | 0x8000_0000;
        dat[root..root + 4].copy_from_slice(&next.to_le_bytes());
        assert!(read_dat_bytes(dat).is_err());
    }

    /// The real files, when `DERETH_TEST_DAT_DIR` names them (the retail install is never in the
    /// repository), and the February 2005 pair when `DERETH_TEST_PRETOD_DAT_DIR` does.
    #[test]
    fn the_retail_dats_read_as_their_iterations_when_present() {
        use dereth_dat::{testing, PreTodDat, RetailDat};
        if testing::have_dats() {
            for (dat, want) in [
                (RetailDat::Portal, 2072),
                (RetailDat::Cell, 982),
                (RetailDat::Local, 994),
                (RetailDat::HighRes, 497),
            ] {
                let path = testing::dat_file(dat);
                let info = read_dat(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                assert_eq!(info.iterations.count, want, "{}", path.display());
                assert_eq!(info.era, ContainerEra::Tod);
            }
        }
        if testing::pre_tod_shortfall().is_none() {
            let dir = testing::pre_tod_dat_dir().unwrap_or_default();
            for (dat, kind, want) in [
                (PreTodDat::Portal, DatKind::Portal, 2112),
                (PreTodDat::Cell, DatKind::Cell { region: 0 }, 1593),
            ] {
                let info = read_dat(&dat.in_dir(&dir)).unwrap();
                assert_eq!(
                    (info.kind, info.era, info.iterations.count),
                    (kind, ContainerEra::PreTod, want)
                );
            }
        }
    }
}
