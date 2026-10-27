//! How a container lays out its header, its directory entries and its transaction slot.
//!
//! Every container this crate reads and writes runs the same machine: blocks of the header's block
//! size from `0x400`, each starting with a link to the next, a free chain, and an order-62 B-tree
//! directory kept as a block chain like any record. What a layout decides is where the header is
//! and where each of its fields sits, the size of a directory entry and where its fields sit, and
//! the transaction slot. The two dat sets this crate knows are [`ContainerLayout::MODERN`] and
//! [`ContainerLayout::CLASSIC`], and [`DatFile::open`](crate::DatFile::open) tells them apart by
//! where the magic is. A container in another layout is opened by naming it:
//! [`DatFile::open_as`](crate::DatFile::open_as), [`DatWriter::open_as`](crate::DatWriter::open_as).
//!
//! See `docs/formats/01-dat-container.md`.

use dereth_primitives::ContainerEra;

/// Where the header is, and where each of its fields sits as a byte offset into it.
///
/// Every header starts with the magic (`0x5442`) and then the block size, so those two have no
/// offset here. A field the layout does not have is `None` and reads as zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HeaderLayout {
    /// Where the header starts in the file.
    pub at: u64,
    /// Its length in bytes.
    pub bytes: usize,
    pub file_size: usize,
    /// `None` when the file names no data set: then its block size tells it, `0x100` the cell
    /// file and anything else the portal file.
    pub data_set: Option<usize>,
    pub data_subset: Option<usize>,
    pub free_head: usize,
    pub free_tail: usize,
    pub free_count: usize,
    pub btree_root: usize,
    pub young_lru: Option<usize>,
    pub old_lru: Option<usize>,
    /// One byte.
    pub use_lru: Option<usize>,
    pub master_map_id: Option<usize>,
    pub eng_pack_vnum: Option<usize>,
    pub game_pack_vnum: Option<usize>,
    /// Sixteen bytes.
    pub version_major: Option<usize>,
    pub version_minor: Option<usize>,
    /// The whole file's iteration, for a layout that keeps it in the header rather than in the
    /// `0xFFFF0001` list.
    pub iteration: Option<usize>,
}

/// A directory entry's length, and where each of its fields sits as a byte offset into it. A field
/// the layout does not have is `None` and reads as zero: no flags word means no record is
/// compressed and every version is 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntryLayout {
    /// The entry's length in bytes.
    pub bytes: usize,
    /// The flags word: the compressed bit and the version.
    pub bits: Option<usize>,
    pub id: usize,
    /// The record's first block.
    pub offset: usize,
    /// The record's length in bytes.
    pub size: usize,
    pub date: Option<usize>,
    pub iteration: Option<usize>,
}

/// The transaction slot before the header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TransactionSlot {
    /// Where the slot starts in the file.
    pub at: u64,
    /// Its length in bytes.
    pub bytes: usize,
    /// Whether the slot holds the writer's journal records. When it does,
    /// [`DatWriter`](crate::DatWriter) refuses a file whose record is unfinished and leaves a
    /// finished one after each write; when it does not, the writer leaves the slot as it is.
    pub journal: bool,
}

/// How a container is laid out. Blocks, chains, the free list and the order-62 directory are the
/// same in every layout; see the module header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContainerLayout {
    pub header: HeaderLayout,
    pub entry: EntryLayout,
    /// `None` for a layout with no transaction slot.
    pub transaction: Option<TransactionSlot>,
    /// Which records the container holds: the dat set whose record layouts a decoder reads them
    /// with.
    pub era: ContainerEra,
}

/// A node's 62 child links and its entry count come before its entries.
const NODE_PREFIX: usize = 62 * 4 + 4;
/// A node holds at most this many entries.
const NODE_ENTRIES: usize = 61;

impl ContainerLayout {
    /// The `client_*.dat` files: the 80-byte header at `0x140`, the 64-byte journal at `0x100`,
    /// 24-byte entries (flags, id, first block, size, date, iteration).
    pub const MODERN: Self = Self {
        header: HeaderLayout {
            at: 0x140,
            bytes: 0x50,
            file_size: 0x08,
            data_set: Some(0x0C),
            data_subset: Some(0x10),
            free_head: 0x14,
            free_tail: 0x18,
            free_count: 0x1C,
            btree_root: 0x20,
            young_lru: Some(0x24),
            old_lru: Some(0x28),
            use_lru: Some(0x2C),
            master_map_id: Some(0x30),
            eng_pack_vnum: Some(0x34),
            game_pack_vnum: Some(0x38),
            version_major: Some(0x3C),
            version_minor: Some(0x4C),
            iteration: None,
        },
        entry: EntryLayout {
            bytes: 24,
            bits: Some(0),
            id: 4,
            offset: 8,
            size: 12,
            date: Some(16),
            iteration: Some(20),
        },
        transaction: Some(TransactionSlot {
            at: 0x100,
            bytes: 0x40,
            journal: true,
        }),
        era: ContainerEra::Modern,
    };

    /// `portal.dat` and `cell.dat`, from before Throne of Destiny: the 44-byte header at `0x12C`
    /// with the whole file's iteration and no data set, no transaction slot, 12-byte entries (id,
    /// first block, size).
    pub const CLASSIC: Self = Self {
        header: HeaderLayout {
            at: 0x12C,
            bytes: 0x2C,
            file_size: 0x08,
            data_set: None,
            data_subset: None,
            free_head: 0x10,
            free_tail: 0x14,
            free_count: 0x18,
            btree_root: 0x1C,
            young_lru: None,
            old_lru: None,
            use_lru: None,
            master_map_id: None,
            eng_pack_vnum: None,
            game_pack_vnum: None,
            version_major: None,
            version_minor: None,
            iteration: Some(0x0C),
        },
        entry: EntryLayout {
            bytes: 12,
            bits: None,
            id: 0,
            offset: 4,
            size: 8,
            date: None,
            iteration: None,
        },
        transaction: None,
        era: ContainerEra::Classic,
    };

    /// The built-in layout of a dat set.
    #[must_use]
    pub const fn of_era(era: ContainerEra) -> Self {
        match era {
            ContainerEra::Classic => Self::CLASSIC,
            ContainerEra::Modern => Self::MODERN,
        }
    }

    /// A directory node's length: the 62 child links, the entry count and 61 entries.
    #[must_use]
    pub const fn node_bytes(&self) -> usize {
        NODE_PREFIX + NODE_ENTRIES * self.entry.bytes
    }

    /// Whether every field fits where the layout puts it: the header's fields inside the header,
    /// the entry's inside the entry, the transaction slot and the header inside the `0x400`-byte
    /// prologue and clear of each other. A layout that does not is refused with
    /// [`DatError::BadLayout`](crate::DatError::BadLayout) before anything is read.
    #[must_use]
    pub fn fits(&self) -> bool {
        let h = &self.header;
        let fits_header = |at: usize, len: usize| at.checked_add(len).is_some_and(|e| e <= h.bytes);
        let words = [
            Some(0),
            Some(4),
            Some(h.file_size),
            h.data_set,
            h.data_subset,
            Some(h.free_head),
            Some(h.free_tail),
            Some(h.free_count),
            Some(h.btree_root),
            h.young_lru,
            h.old_lru,
            h.master_map_id,
            h.eng_pack_vnum,
            h.game_pack_vnum,
            h.version_minor,
            h.iteration,
        ];
        let header_ok = words.into_iter().flatten().all(|at| fits_header(at, 4))
            && h.use_lru.is_none_or(|at| fits_header(at, 1))
            && h.version_major.is_none_or(|at| fits_header(at, 16));
        let header_end = h.at.checked_add(h.bytes as u64);
        let prologue = u64::from(crate::container::FIRST_BLOCK);
        let placed = header_end.is_some_and(|end| end <= prologue);
        let e = &self.entry;
        let entry_ok = [
            e.bits,
            Some(e.id),
            Some(e.offset),
            Some(e.size),
            e.date,
            e.iteration,
        ]
        .into_iter()
        .flatten()
        .all(|at| at.checked_add(4).is_some_and(|end| end <= e.bytes));
        let slot_ok = self.transaction.is_none_or(|t| {
            t.at.checked_add(t.bytes as u64).is_some_and(|end| {
                end <= prologue && (end <= h.at || header_end.is_some_and(|he| t.at >= he))
            })
        });
        header_ok && placed && entry_ok && slot_ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The built-in layouts' nodes are the sizes the format documents.
    #[test]
    fn the_built_in_nodes_are_the_documented_sizes() {
        assert_eq!(
            ContainerLayout::MODERN.node_bytes(),
            crate::btree::NODE_SIZE
        );
        assert_eq!(
            ContainerLayout::CLASSIC.node_bytes(),
            crate::btree::CLASSIC_NODE_SIZE
        );
        assert!(ContainerLayout::MODERN.fits());
        assert!(ContainerLayout::CLASSIC.fits());
        assert_eq!(
            ContainerLayout::of_era(ContainerEra::Classic),
            ContainerLayout::CLASSIC
        );
    }

    /// A field past the end of its header or entry, or a header that overlaps the transaction
    /// slot, is not a layout.
    #[test]
    fn a_layout_whose_fields_do_not_fit_is_refused() {
        let mut l = ContainerLayout::MODERN;
        l.header.btree_root = 0x4E;
        assert!(!l.fits());
        let mut l = ContainerLayout::MODERN;
        l.entry.iteration = Some(22);
        assert!(!l.fits());
        let mut l = ContainerLayout::MODERN;
        l.header.at = 0x120;
        assert!(!l.fits(), "the header runs into the journal");
        let mut l = ContainerLayout::MODERN;
        l.header.at = 0x3C0;
        assert!(!l.fits(), "the header runs past the prologue");
    }
}
