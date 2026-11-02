//! A directory node as the file holds it: 62 child links, an entry count, and 61 entries of
//! `{id, first block, size, date}`; and a node as memory keeps it.

/// A node's bytes.
pub const NODE_BYTES: usize = 62 * 4 + 4 + 61 * 16;
/// A node's child links.
pub const FAN_OUT: usize = 62;
/// The most entries a node holds.
pub const MAX_ENTRIES: i32 = 61;
/// The fewest entries a node other than the root holds; a split leaves this many each side.
pub const MIN_ENTRIES: i32 = 30;

const COUNT_WORD: usize = FAN_OUT;
const ENTRY_WORD: usize = FAN_OUT + 1;

/// One directory entry; a new one is all zeros.
// D2-17664
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirEntry {
    pub id: u32,
    /// The first block of the record's chain.
    pub offset: u32,
    pub size: u32,
    /// When it was stored, in seconds since 1970.
    pub date: u32,
}

/// A node's bytes, read and written in place: whatever a slot held before stays where nothing
/// overwrote it, as it does in the file.
#[derive(Clone, PartialEq, Eq)]
pub struct DirNode(pub [u8; NODE_BYTES]);

impl Default for DirNode {
    fn default() -> Self {
        Self([0; NODE_BYTES])
    }
}

impl core::fmt::Debug for DirNode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DirNode")
            .field("count", &self.count())
            .field("leaf", &self.is_leaf())
            .finish()
    }
}

impl DirNode {
    fn word(&self, i: usize) -> u32 {
        let b = &self.0[4 * i..4 * i + 4];
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }

    fn set_word(&mut self, i: usize, v: u32) {
        self.0[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// Child link `i`.
    #[must_use]
    pub fn next(&self, i: usize) -> u32 {
        self.word(i)
    }

    pub fn set_next(&mut self, i: usize, v: u32) {
        self.set_word(i, v);
    }

    /// The entry count.
    #[must_use]
    pub fn count(&self) -> i32 {
        self.word(COUNT_WORD).cast_signed()
    }

    pub fn set_count(&mut self, n: i32) {
        self.set_word(COUNT_WORD, n.cast_unsigned());
    }

    /// A leaf: its first child link is 0.
    #[must_use]
    pub fn is_leaf(&self) -> bool {
        self.next(0) == 0
    }

    /// Entry `i`.
    #[must_use]
    pub fn entry(&self, i: usize) -> DirEntry {
        let w = ENTRY_WORD + 4 * i;
        DirEntry {
            id: self.word(w),
            offset: self.word(w + 1),
            size: self.word(w + 2),
            date: self.word(w + 3),
        }
    }

    pub fn set_entry(&mut self, i: usize, e: DirEntry) {
        let w = ENTRY_WORD + 4 * i;
        self.set_word(w, e.id);
        self.set_word(w + 1, e.offset);
        self.set_word(w + 2, e.size);
        self.set_word(w + 3, e.date);
    }

    /// Entry `i`'s id.
    #[must_use]
    pub fn id(&self, i: usize) -> u32 {
        self.word(ENTRY_WORD + 4 * i)
    }

    /// Move `n` entries from `from` to `to` (overlapping as a memory move does).
    pub fn move_entries(&mut self, to: usize, from: usize, n: usize) {
        let e = 4 * ENTRY_WORD;
        self.0
            .copy_within(e + 16 * from..e + 16 * (from + n), e + 16 * to);
    }

    /// Move `n` child links from `from` to `to`.
    pub fn move_links(&mut self, to: usize, from: usize, n: usize) {
        self.0.copy_within(4 * from..4 * (from + n), 4 * to);
    }
}

/// A node held in memory: when it was last used, whether it is to be written, and where it is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Kept {
    pub stamp: i32,
    pub dirty: bool,
    /// The node's first block; 0 for a slot holding none.
    pub offset: u32,
    pub node: DirNode,
}

impl Kept {
    /// A slot holding nothing.
    // D2-17671
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Emptied as a slot is when its node goes: the stamp, the flag, the place, the count and the
    /// first child link cleared, the rest of the node left as it was.
    pub fn clear_head(&mut self) {
        self.stamp = 0;
        self.dirty = false;
        self.offset = 0;
        self.node.set_count(0);
        self.node.set_next(0, 0);
    }
}
