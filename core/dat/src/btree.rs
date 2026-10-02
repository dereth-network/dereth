//! The directory: `BTree`, `BTNode`, `BTEntry`.
//!
//! An order-62 B-tree: at most 61 keys and 62 children per node, keys sorted ascending by `GID_`.
//! A zero first child pointer marks a leaf.
//!
//! See `docs/formats/01-dat-container.md`.

use crate::error::DatError;

/// `BTEntry`, 24 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BtEntry {
    /// The raw first dword: a 1-bit compressed flag, 15 reserved bits, a 16-bit version.
    pub bits: u32,
    /// `GID_` — the file id.
    pub id: u32,
    /// The offset of the first block of the chain.
    pub offset: u32,
    /// `size_` — the payload size in bytes.
    pub size: u32,
    /// `date_` — Unix time of the write.
    pub date: u32,
    /// `iter_` — the iteration that introduced this version of the file.
    pub iteration: u32,
}

impl BtEntry {
    /// The compressed flag (bit 0): the payload is zlib-compressed. Zero for all 887,455 retail
    /// entries.
    #[must_use]
    pub fn compressed(self) -> bool {
        self.bits & 1 != 0
    }

    /// The reserved bits (1..15). Always 0 in retail.
    #[must_use]
    pub fn reserved(self) -> u16 {
        u16::try_from((self.bits >> 1) & 0x7FFF).unwrap_or(0)
    }

    /// The version field (bits 16..31): the DBObj pack version the payload was written with.
    #[must_use]
    pub fn version(self) -> u16 {
        u16::try_from(self.bits >> 16).unwrap_or(0)
    }

    /// A directory entry of the container before Throne of Destiny (`portal.dat`, `cell.dat`): 12
    /// bytes, id, first block and size, with no flags, date or iteration, which read as zero.
    pub(crate) fn parse_pre_tod(b: &[u8]) -> Self {
        let w = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        Self {
            bits: 0,
            id: w(0),
            offset: w(4),
            size: w(8),
            date: 0,
            iteration: 0,
        }
    }

    pub(crate) fn parse(b: &[u8]) -> Self {
        let w = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        Self {
            bits: w(0),
            id: w(4),
            offset: w(8),
            size: w(12),
            date: w(16),
            iteration: w(20),
        }
    }

    /// The inverse of [`BtEntry::parse`] -- the 24 bytes a writer puts back into a `BTNode`.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 24] {
        let mut b = [0u8; 24];
        for (i, v) in [
            self.bits,
            self.id,
            self.offset,
            self.size,
            self.date,
            self.iteration,
        ]
        .iter()
        .enumerate()
        {
            b[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        b
    }
}

/// `BTNode`, `0x6B4` = 1716 bytes, stored as a block chain like any other file.
#[derive(Debug, Clone)]
pub struct BtNode {
    /// The 62 child block pointers; `children[0] == 0` means this is a leaf.
    pub children: [u32; 62],
    /// The entries, as many as the node's entry count, 0..=61.
    pub entries: Vec<BtEntry>,
}

/// `62 * 4 + 4 + 61 * 24`.
pub const NODE_SIZE: usize = 62 * 4 + 4 + 61 * 24;

/// The pre-Throne-of-Destiny node: `62 * 4 + 4 + 61 * 12` = 984 bytes, the same children and count
/// with 12-byte entries.
pub const PRE_TOD_NODE_SIZE: usize = 62 * 4 + 4 + 61 * 12;

impl BtNode {
    #[must_use]
    pub fn is_leaf(&self) -> bool {
        self.children[0] == 0
    }

    /// Parse a node from its `NODE_SIZE` payload bytes.
    ///
    /// The client treats a count above 0x3D as invalid and returns false; here it
    /// is an error, because a rebuild that silently swallowed it would lose entries.
    pub fn parse(buf: &[u8], offset: u32) -> Result<Self, DatError> {
        debug_assert_eq!(buf.len(), NODE_SIZE);
        Self::parse_with(buf, offset, 24, BtEntry::parse)
    }

    /// Parse a pre-Throne-of-Destiny node from its [`PRE_TOD_NODE_SIZE`] payload bytes. Child slots
    /// past the count are fill (`0xCDCDCDCD` in the shipped files) and are never followed.
    pub fn parse_pre_tod(buf: &[u8], offset: u32) -> Result<Self, DatError> {
        debug_assert_eq!(buf.len(), PRE_TOD_NODE_SIZE);
        Self::parse_with(buf, offset, 12, BtEntry::parse_pre_tod)
    }

    fn parse_with(
        buf: &[u8],
        offset: u32,
        entry_size: usize,
        entry: fn(&[u8]) -> BtEntry,
    ) -> Result<Self, DatError> {
        let mut children = [0u32; 62];
        for (i, c) in children.iter_mut().enumerate() {
            let o = i * 4;
            *c = u32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
        }
        let count = u32::from_le_bytes([buf[248], buf[249], buf[250], buf[251]]);
        if count > 0x3D {
            return Err(DatError::BadNodeEntryCount { offset, count });
        }
        let entries = (0..count as usize)
            .map(|i| entry(&buf[252 + i * entry_size..252 + (i + 1) * entry_size]))
            .collect();
        Ok(Self { children, entries })
    }

    /// Binary search within the node. Returns `Ok(index)` on a hit
    /// and `Err(index)` for the child to descend into.
    pub fn search(&self, id: u32) -> Result<usize, usize> {
        self.entries.binary_search_by_key(&id, |e| e.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the entry for `0xFFFF0001` in the portal dat.
    #[test]
    fn bt_entry_fields_match_the_documented_worked_example() {
        let mut b = Vec::new();
        b.extend_from_slice(&0x0001_0000u32.to_le_bytes()); // bits: ver_ = 1, comp_ = 0
        b.extend_from_slice(&0xFFFF_0001u32.to_le_bytes());
        b.extend_from_slice(&0x3738_6400u32.to_le_bytes());
        b.extend_from_slice(&12u32.to_le_bytes());
        b.extend_from_slice(&1_434_144_313u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        let e = BtEntry::parse(&b);
        assert_eq!(e.id, 0xFFFF_0001);
        assert_eq!(e.offset, 0x3738_6400);
        assert_eq!(e.size, 12);
        assert_eq!(e.date, 1_434_144_313);
        assert_eq!(e.iteration, 1);
        assert_eq!(e.version(), 1);
        assert!(!e.compressed());
        assert_eq!(e.reserved(), 0);
    }

    #[test]
    fn node_size_is_0x6b4() {
        assert_eq!(NODE_SIZE, 0x6B4);
    }

    /// The pre-Throne-of-Destiny node is 984 bytes of 12-byte entries: id, first block, size.
    #[test]
    fn a_pre_tod_node_holds_twelve_byte_entries_with_no_date_or_iteration() {
        assert_eq!(PRE_TOD_NODE_SIZE, 984);
        let mut buf = vec![0xCDu8; PRE_TOD_NODE_SIZE];
        buf[..4].copy_from_slice(&0u32.to_le_bytes()); // a leaf
        buf[248..252].copy_from_slice(&2u32.to_le_bytes());
        for (i, (id, first, size)) in [(0x0100_0001u32, 0x800u32, 40u32), (0x0100_0002, 0xC00, 7)]
            .into_iter()
            .enumerate()
        {
            let o = 252 + i * 12;
            buf[o..o + 4].copy_from_slice(&id.to_le_bytes());
            buf[o + 4..o + 8].copy_from_slice(&first.to_le_bytes());
            buf[o + 8..o + 12].copy_from_slice(&size.to_le_bytes());
        }
        let node = BtNode::parse_pre_tod(&buf, 0x400).unwrap();
        assert!(node.is_leaf());
        assert_eq!(node.entries.len(), 2);
        let e = node.entries[1];
        assert_eq!((e.id, e.offset, e.size), (0x0100_0002, 0xC00, 7));
        assert_eq!((e.bits, e.date, e.iteration), (0, 0, 0));
    }

    #[test]
    fn an_impossible_entry_count_is_an_error_not_a_truncation() {
        let mut buf = vec![0u8; NODE_SIZE];
        buf[248..252].copy_from_slice(&0x3Eu32.to_le_bytes());
        assert!(matches!(
            BtNode::parse(&buf, 0x400),
            Err(DatError::BadNodeEntryCount { count: 0x3E, .. })
        ));
    }
}
