//! A data file as the client keeps one: opened, created, written, and put right after a write
//! that stopped half way.
//!
//! [`DatFile`](crate::DatFile) reads a container; this module is the client's own disk controller,
//! which also writes one: the records the server sends while the game runs are stored in the data
//! files, a record already there is replaced, and records are removed. Four layers, each the
//! client's own:
//! - [`device`]: a file the host opens, read and written at an offset;
//! - [`alloc`]: the block allocator: a record is a chain of blocks (`[next][payload]`), taken from
//!   the head of the free list, given back at its tail, the file grown when the list runs short;
//! - [`tree`]: the directory, an order-62 B-tree of `{id, first block, size, date}` entries,
//!   split on the way down when inserting and rebalanced on the way down when removing, with a
//!   hundred nodes kept in memory;
//! - [`controller`]: the file as the data cache sees it: open or create, look up, load, save,
//!   delete, and the list of data versions the file holds;
//! - [`shared`]: each file behind a lock, the four the client opens, and which one a record is
//!   in; [`lookfile`]: where a file is looked for; [`client_files`]: the four files the client
//!   opens; [`download`]: the data download the server drives, stored through those files.
//!
//! **A write is a transaction.** Every change to the directory first writes a record of what it
//! is about to do into the 44-byte slot at `0x100`, does it, and clears the slot. A file opened
//! with the slot not clear finishes or undoes that change before anything else
//! ([`Directory::recover_transaction`]).
//!
//! The file's last-used list (the header's `young`, `old` and in-use words) is never in use: the
//! client turns it off whenever it opens or creates a file, so the paths that would maintain it
//! are not taken here either.

pub mod alloc;
pub mod client_files;
pub mod controller;
pub mod device;
pub mod download;
pub mod intset;
pub mod lookfile;
pub mod node;
pub mod overlay;
pub mod shared;
pub mod tree;

use core::fmt;

pub use alloc::{Blocks, StoreHeader};
pub use client_files::{ClientFiles, PackVersions, CLIENT_VERSIONS};
pub use controller::{OpenSpec, Store};
/// A file's storage, read and written at an offset, and one kept in memory.
pub use dereth_dat::{DatStorageMut, MemoryFile};
pub use device::{Files, Handle, OpenMode};
pub use download::{DddState, Download, MissingIteration, Output};
pub use intset::VersionSet;
pub use lookfile::{FileSearch, LookHost};
pub use node::{DirEntry, DirNode};
pub use overlay::Overlay;
pub use shared::{Opened, SharedStore, Slot, StoreSet};
pub use tree::Directory;

/// The header's magic word, `"BT"`.
pub const MAGIC: u32 = 0x5442;
/// Where the transaction slot is.
pub const TRANSACTION_AT: u32 = 0x100;
/// Where the header is.
pub const INFO_AT: u32 = 300;

/// How an operation on a data file failed, each with the client's own code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskError {
    /// A generic failure: a node not found, a load that failed, a rotation (which always answers
    /// this, done or not).
    Failed,
    /// The file did not open.
    NotOpened,
    /// The file's data versions are not the program's.
    Incompatible,
    /// A chain ran into a block already free, or an entry was to go into a node that is not a
    /// leaf.
    Corrupt,
    /// The header's magic word is wrong.
    BadMagic,
    /// A read came back short.
    ShortRead,
    /// A write went out short.
    ShortWrite,
    /// The transaction slot holds a kind of change the client does not know.
    UnknownTransaction,
    /// The file did not close.
    CloseFailed,
    /// The file has no open handle to read or write through.
    NoHandle,
}

impl DiskError {
    /// The client's code for the failure.
    #[must_use]
    pub fn code(self) -> i32 {
        match self {
            Self::Failed => -1,
            Self::NotOpened => -2,
            Self::Incompatible => -6,
            Self::Corrupt => -0x65,
            Self::BadMagic | Self::ShortRead => -0x66,
            Self::ShortWrite | Self::UnknownTransaction => -0x67,
            Self::CloseFailed => -0x68,
            Self::NoHandle => -0x69,
        }
    }
}

impl fmt::Display for DiskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?} ({})", self.code())
    }
}

impl std::error::Error for DiskError {}

/// What every operation answers.
pub type Result<T> = core::result::Result<T, DiskError>;

/// The record of a change in progress, as the transaction slot holds it: a kind and ten words
/// whose meaning the kind gives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Transaction {
    pub kind: u32,
    pub w: [u32; 10],
}

impl Transaction {
    /// The slot's size.
    pub const BYTES: usize = 0x2C;

    /// An entry added to a leaf.
    pub const INSERT: u32 = 4;
    /// The undo of an entry's blocks stored and not yet in a leaf (never written by this build).
    pub const INSERT_ROLLBACK: u32 = 5;
    /// An entry removed from a leaf.
    pub const DELETE_LEAF: u32 = 6;
    /// An entry removed from an inner node, replaced by its neighbor from a leaf.
    pub const DELETE_INTERNAL: u32 = 7;
    /// Two children merged around their separator.
    pub const MERGE: u32 = 8;
    /// A record's data replaced.
    pub const UPDATE: u32 = 9;
    /// A full child split in two.
    pub const SPLIT: u32 = 10;
    /// An entry moved through the parent from one child to its neighbor.
    pub const ROTATE: u32 = 11;

    /// A transaction of `kind` with the given words (`w[0]` is the first word after the kind).
    #[must_use]
    pub fn new(kind: u32, words: &[u32]) -> Self {
        let mut w = [0; 10];
        w[..words.len()].copy_from_slice(words);
        Self { kind, w }
    }

    /// Word `n` (1..=10).
    #[must_use]
    pub fn ul(&self, n: usize) -> u32 {
        self.w[n - 1]
    }

    /// The slot's bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::BYTES] {
        let mut b = [0; Self::BYTES];
        b[..4].copy_from_slice(&self.kind.to_le_bytes());
        for (i, w) in self.w.iter().enumerate() {
            b[4 + 4 * i..8 + 4 * i].copy_from_slice(&w.to_le_bytes());
        }
        b
    }

    /// The transaction a slot holds.
    #[must_use]
    pub fn from_bytes(b: &[u8; Self::BYTES]) -> Self {
        let word =
            |i: usize| u32::from_le_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
        let mut w = [0; 10];
        for (i, v) in w.iter_mut().enumerate() {
            *v = word(i + 1);
        }
        Self { kind: word(0), w }
    }
}
