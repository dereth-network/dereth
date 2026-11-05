//! An Asheron's Call 2 data file: the shared container (`dereth_dat::DatFile`) read in this game's
//! layout.
//!
//! The container is the one Asheron's Call's files use (blocks chained into files, a free list,
//! an order-62 B-tree directory), with this game's own header and directory entry, which
//! [`LAYOUT`] places:
//! - `0x000`-`0x0FF`: zero; `0x100`-`0x12B`: a 44-byte transaction slot (zero in every retail file);
//! - `0x12C`-`0x17F`: the 0x54-byte header: the shared fields in the later files' order, then a
//!   24-byte stamp (a word that no field reads, a GUID, a minor version);
//! - from `0x400`: blocks of the header's block size, each `[next: u32][payload]`;
//! - directory nodes of 0x4CC bytes: 62 child offsets, an entry count, and up to 61 entries of 16
//!   bytes `{id, first block, size, unix time}`, with no flags or iteration word.
//!
//! [`DatFile`] opens a file only in that layout. The shared container, asked to tell the layout
//! itself, would take a header at `0x12C` for the earlier Asheron's Call files' and misread every
//! entry.

use std::path::PathBuf;
use std::sync::Arc;

use dereth_dat::{
    BtEntry, ContainerEra, ContainerLayout, DatError, DatStorage, DiskFileInfo, EntryLayout,
    HeaderLayout, StructureReport, TransactionSlot,
};
use dereth_primitives::DataId;

use crate::checked::Watch;

/// This game's container layout.
///
/// `era` is the shared container's choice among the first game's record layouts, which no decoder
/// of this game uses; it is set because a value is required.
pub const LAYOUT: ContainerLayout = ContainerLayout {
    header: HeaderLayout {
        at: 0x12C,
        bytes: 0x54,
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
        version_major: Some(0x40),
        version_minor: Some(0x50),
        iteration: None,
    },
    entry: EntryLayout {
        bytes: 16,
        bits: None,
        id: 0,
        offset: 4,
        size: 8,
        date: Some(12),
        iteration: None,
    },
    transaction: Some(TransactionSlot {
        at: 0x100,
        bytes: 0x2C,
        journal: false,
    }),
    era: ContainerEra::Modern,
};

/// Where the header starts.
pub const HEADER_OFFSET: u64 = 0x12C;
/// The header's size.
pub const HEADER_SIZE: usize = 0x54;
/// Where the stamp's first word is: whatever the writing tool had in memory there.
pub const STAMP_WORD_AT: u64 = HEADER_OFFSET + 0x3C;
/// The transaction slot's size.
pub const TRANSACTION_SIZE: usize = 0x2C;
/// The first data block.
pub const FIRST_BLOCK: u32 = 0x400;
/// A directory node: 62 children, a count, 61 entries of 16 bytes.
pub const NODE_SIZE: usize = 62 * 4 + 4 + 61 * 16;
/// The most entries a node holds.
pub const NODE_ENTRIES: usize = 61;
/// The iteration list's record id.
pub const ITERATION_LIST: u32 = 0xFFFF_0001;
/// The header's data set of a cell file.
pub const CELL_SET: u32 = dereth_dat::container::CELL_DATFILE;
/// The high-resolution picture file's subset word, `"HiFi"`.
pub const HIRES_SUBSET: u32 = dereth_dat::container::HIRES_SUBSET;

/// One directory entry: the shared container's, whose flags and iteration words this layout
/// does not have (they read as zero).
pub type Entry = BtEntry;
/// The header, as the shared container reads it in [`LAYOUT`]: `data_set` (1 portal and
/// high-resolution pictures, 2 cell, 3 local), `data_subset` (a cell file's region, a local
/// file's language word, `HiFi` for the high-resolution pictures, 0 for the portal), the free
/// list, the directory's root, the id map (`master_map_id`), the engine's and the game's data
/// versions (`eng_pack_vnum`, `game_pack_vnum`), and the stamp's GUID (`version_major`) and minor
/// version (`version_minor`).
pub type Header = DiskFileInfo;
/// What [`DatFile::verify`] found walking the whole file.
pub type Structure = StructureReport;

/// Every invariant of a walk: the shared container's (ascending keys, one leaf depth, at most 61
/// entries a node, the free chain as long as the header says) and every block either in a chain
/// or free.
#[must_use]
pub fn sound(s: &Structure) -> bool {
    s.is_sound() && s.blocks_used + s.free_chain_len == s.total_data_blocks
}

/// A stored GUID (the stamp's) in the usual braced form.
#[must_use]
pub fn guid_string(g: &[u8; 16]) -> String {
    let d1 = u32::from_le_bytes([g[0], g[1], g[2], g[3]]);
    let d2 = u16::from_le_bytes([g[4], g[5]]);
    let d3 = u16::from_le_bytes([g[6], g[7]]);
    format!(
        "{{{d1:08x}-{d2:04x}-{d3:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        g[8], g[9], g[10], g[11], g[12], g[13], g[14], g[15]
    )
}

/// One data file, read-only, its whole directory in memory, and what tells whether the file under
/// it changed ([`DatFile::watched`]).
#[derive(Debug, Clone)]
pub struct DatFile(dereth_dat::DatFile, Option<Arc<dyn Watch>>);

impl DatFile {
    /// Read a container's header and directory from storage the host provides. `name` names it in
    /// errors.
    ///
    /// # Errors
    /// The shared container's header and directory errors, and the storage's read errors.
    // D2-17720, D2-17675
    pub fn from_storage(name: PathBuf, storage: Box<dyn DatStorage>) -> Result<Self, DatError> {
        dereth_dat::DatFile::from_storage_as(name, storage, &LAYOUT).map(|f| Self(f, None))
    }

    /// The container with `watch` telling whether the file under it changed since it was opened,
    /// which [`DatFile::read_checked`] asks when a record fails.
    #[must_use]
    pub fn watched(self, watch: Arc<dyn Watch>) -> Self {
        Self(self.0, Some(watch))
    }

    /// What tells whether the file under the container changed, when it has one.
    #[must_use]
    pub fn watch(&self) -> Option<&dyn Watch> {
        self.1.as_deref()
    }

    /// A container over bytes already in memory.
    ///
    /// # Errors
    /// As [`DatFile::from_storage`].
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, DatError> {
        Self::from_storage(PathBuf::new(), Box::new(bytes))
    }

    /// The shared container underneath, for what this game's view does not offer.
    #[must_use]
    pub fn shared(&self) -> &dereth_dat::DatFile {
        &self.0
    }

    #[must_use]
    pub fn header(&self) -> &Header {
        self.0.header()
    }

    /// The stamp's first word.
    ///
    /// # Errors
    /// The storage's read errors.
    pub fn stamp_word(&self) -> Result<u32, DatError> {
        let mut w = [0u8; 4];
        self.0.read_raw(STAMP_WORD_AT, &mut w)?;
        Ok(u32::from_le_bytes(w))
    }

    /// The transaction slot's bytes.
    ///
    /// # Errors
    /// The storage's read errors.
    pub fn transaction_slot(&self) -> Result<Vec<u8>, DatError> {
        self.0.transaction_slot()
    }

    /// How many records the directory holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn entry(&self, id: u32) -> Option<&Entry> {
        self.0.entry(DataId(id))
    }

    /// Every entry, ascending by id.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> + '_ {
        self.0.iter_entries().map(|(_, e)| e)
    }

    /// The entries whose ids lie in `lo..=hi`, ascending.
    pub fn entries_in(&self, lo: u32, hi: u32) -> impl Iterator<Item = &Entry> + '_ {
        self.entries()
            .skip_while(move |e| e.id < lo)
            .take_while(move |e| e.id <= hi)
    }

    /// A record's bytes, by id.
    ///
    /// # Errors
    /// [`DatError::NotFound`] for an id the directory does not hold, and the chain's errors.
    // D2-17641, D2-17714
    pub fn read(&self, id: u32) -> Result<Vec<u8>, DatError> {
        self.0.read(DataId(id))
    }

    /// The start of a record's bytes: as much of it as its first block holds. What asks for a few
    /// words at a record's head need not read the whole chain of a long record.
    ///
    /// # Errors
    /// As [`DatFile::read`], but for a chain's later links, which are not walked.
    pub fn read_start(&self, id: u32) -> Result<Vec<u8>, DatError> {
        let e = self.entry(id).ok_or(DatError::NotFound(DataId(id)))?;
        let mut block = vec![0; e.size.min(self.header().block_size - 4) as usize];
        self.0.read_raw(u64::from(e.offset) + 4, &mut block)?;
        Ok(block)
    }

    /// A record's bytes, by its entry.
    ///
    /// # Errors
    /// As [`DatFile::read`].
    pub fn read_entry(&self, e: &Entry) -> Result<Vec<u8>, DatError> {
        self.read(e.id)
    }

    /// Find an id by descending the tree from the root, a binary search per node, as the client
    /// looks a record up, rather than asking the directory already in memory.
    ///
    /// # Errors
    /// A node that does not read.
    // D2-17692
    pub fn lookup_via_tree(&self, id: u32) -> Result<Option<Entry>, DatError> {
        self.0.lookup_via_tree(DataId(id))
    }

    /// Walk the whole file: the tree in order, the free chain, and the block accounting.
    ///
    /// # Errors
    /// A node or a free block that does not read.
    pub fn verify(&self) -> Result<Structure, DatError> {
        self.0.verify_structure()
    }

    /// The iteration list, decoded. Values are signed: the 2005 lists hold negative versions below
    /// the positive run.
    ///
    /// # Errors
    /// A missing or malformed list.
    // D2-17649, D2-17659
    pub fn iterations(&self) -> Result<Vec<i32>, DatError> {
        dereth_dat::iteration::decode_signed(&self.read(ITERATION_LIST)?)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (container layout constants)
    use super::*;

    #[test]
    fn a_node_is_0x4cc_bytes_and_the_layout_fits() {
        assert_eq!(NODE_SIZE, 0x4CC);
        assert!(LAYOUT.fits(), "the layout's fields lie inside its header");
        assert_eq!(LAYOUT.header.at, HEADER_OFFSET);
        assert_eq!(LAYOUT.header.bytes, HEADER_SIZE);
    }
}
