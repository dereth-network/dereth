//! The one error enum for this crate.
//!
//! Parsers return `Result`; they do not panic on malformed input, because they will meet malformed
//! input.

use dereth_primitives::{AssetError, DataId};

/// Everything that can go wrong reading a `.dat` container or walking a payload.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DatError {
    #[error("header magic {0:#x}, expected 0x5442")]
    BadMagic(u32),
    #[error("header block size {0:#x} is not a positive multiple of 4")]
    BadBlockSize(u32),
    #[error("{0:?} not found")]
    NotFound(DataId),
    #[error("payload declares id {found:#010X}, directory says {expected:#010X}")]
    IdEchoMismatch { expected: u32, found: u32 },
    #[error("decoder stopped {0} bytes before the end of the payload")]
    Shortfall(usize),
    #[error("decoder read {0} bytes past the end of the payload")]
    Overrun(usize),
    /// The entry's compressed flag is set. No shipped file is stored compressed, so this is
    /// unsupported while the flag itself is still parsed.
    #[error("{0:?} is zlib-compressed, which this reader does not implement")]
    CompressionUnsupported(DataId),
    /// The block-chain load fails when a chain walks into a block whose link has
    /// bit 31 set.
    #[error("block chain for {id:?} walked into a free block at {offset:#010X}")]
    FreeBlockInChain { id: DataId, offset: u32 },
    #[error("block chain for {id:?} ended {shortfall} bytes early")]
    ChainTooShort { id: DataId, shortfall: usize },
    #[error("block offset {0:#010X} lies outside the file")]
    BlockOutOfRange(u32),
    /// The client treats a count above 0x3D as invalid.
    #[error("b-tree node at {offset:#010X} declares {count} entries (max 61)")]
    BadNodeEntryCount { offset: u32, count: u32 },
    /// An intrusive hash table names a bucket size past the 23 the client knows.
    #[error("hash-table bucket-size index {0} is 23 or more")]
    BadBucketIndex(u8),
    /// A packed hash table declares entries but no buckets to hold them, which the client refuses:
    /// a table with no buckets reads as empty only when it also declares no entries.
    #[error("hash table declares {count} entries in {buckets} buckets")]
    BadHashTableHeader { buckets: u32, count: u32 },
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    // ----------------------------------------------------------------------------------------
    // The writer. Everything below is a refusal rather than a corruption: native
    // trusts its own header and walks off the end of the free list into offset 0 when it is
    // wrong, which is the difference a "safe" writer has to make.
    // ----------------------------------------------------------------------------------------
    /// `DatWriter` will not open a file in a directory declared read-only with
    /// [`crate::protect_install`].
    #[error("{0} is in a read-only retail install; write to a disposable copy instead")]
    RetailDatRefused(std::path::PathBuf),
    /// The journal slot at 0x100 holds an unfinished operation. Native would replay it
    /// on the next open; this writer refuses to build on top of it.
    #[error(
        "the transaction journal holds a pending record of type {0}; this writer cannot replay it"
    )]
    PendingTransaction(u8),
    /// The transaction journal requires the magic number `0x4C50`.
    #[error("transaction journal magic {0:#x}, expected 0x4C50")]
    BadTransactionMagic(u32),
    /// The free list ran out mid-chain. The next block written would land at offset 0.
    #[error("the free list ran out while writing a chain")]
    FreeListExhausted,
    /// Freeing a block that is already on the free list is refused rather than looping the chain.
    #[error("block {0:#010X} is already on the free list; freeing it again would loop the chain")]
    DoubleFree(u32),
    /// Creating a file computes `(file size - 0x400) / block size` and assumes it
    /// divides.
    #[error("file size {file_size:#x} is not 0x400 plus a whole number of {block_size:#x} blocks")]
    BadCreateSize { file_size: u32, block_size: u32 },
    /// A `BTNode` chain is not the length 0x6B4 bytes needs.
    #[error("b-tree node at {offset:#010X} occupies {blocks} blocks, expected {want}")]
    NodeChainLength {
        offset: u32,
        blocks: usize,
        want: usize,
    },
    /// A split only makes sense on a node holding 61 entries.
    #[error("b-tree node at {offset:#010X} holds {entries} entries, which is not a split")]
    SplitOfUnfullNode { offset: u32, entries: usize },
    /// The save path refuses a version of 0.
    #[error("{0:?} was offered with version 0, which the dat writer refuses")]
    ZeroVersion(DataId),
    /// A zero-length record would leave the entry's offset pointing at a block still on the free
    /// list.
    #[error("{0:?} was offered an empty payload, which would point the entry at a free block")]
    EmptyPayload(DataId),
    /// A [`crate::write::Fault`] fired. Test-only.
    #[error("the write was interrupted at the injected fault point {0:?}")]
    Interrupted(crate::write::Fault),

    // ----------------------------------------------------------------------------------------
    // Removal. The tree's removal path and its helpers index nodes with values
    // they trust; each of these is a case native would have written past the end of a node or
    // looped for ever on.
    // ----------------------------------------------------------------------------------------
    /// An entry index is past the node's entry count.
    #[error("b-tree node at {offset:#010X} was asked for entry {index} of {entries}")]
    EntryIndexOutOfRange {
        offset: u32,
        index: usize,
        entries: usize,
    },
    /// A merge assumes both nodes are at the 30-entry minimum, so the merged
    /// node holds exactly 61. Anything larger would not fit a `BTNode`.
    #[error("merging nodes of {left} and {right} entries would overflow a 61-entry node")]
    MergeOverflow { left: usize, right: usize },
    /// `find_max`/`find_min` named an id the donor subtree then did not hold.
    #[error("the donor entry {0:#010X} vanished from its own subtree")]
    MissingDonorEntry(u32),
    /// A directory walk visited more nodes than a 61-way tree can be deep.
    #[error("the b-tree walk from {0:#010X} did not terminate; the directory has a cycle")]
    DirectoryLoop(u32),

    // ----------------------------------------------------------------------------------------
    // DDD decompression. The client's decompressor answers `false` for every one of these and
    // the save path's compressed case then returns `false` too, so nothing is written.
    // ----------------------------------------------------------------------------------------
    /// The zlib/DEFLATE stream in a `DDD_DataMessage` is malformed.
    #[error("the compressed record is malformed: {0}")]
    Inflate(&'static str),
    /// The stream inflated to a length other than the one the sender declared.
    #[error("the compressed record inflated to {got} bytes, not the declared {want}")]
    InflateLength { got: usize, want: usize },
    /// The client's decompressor requires `size > 4` and `size - 4 >= 0x10` before it looks at the
    /// buffer at all.
    #[error("a compressed record of {0} bytes is below the 20 the client requires")]
    CompressedTooShort(usize),
}

impl DatError {
    /// The id this error is about, when it names one.
    #[must_use]
    pub fn data_id(&self) -> Option<DataId> {
        match self {
            Self::NotFound(id)
            | Self::CompressionUnsupported(id)
            | Self::FreeBlockInChain { id, .. }
            | Self::ChainTooShort { id, .. }
            | Self::ZeroVersion(id)
            | Self::EmptyPayload(id) => Some(*id),
            Self::IdEchoMismatch { expected, .. } => Some(DataId(*expected)),
            _ => None,
        }
    }
}

/// Bridge to the shared asset seam's error. `AssetSource::read` returns `AssetError`, so every
/// `DatError` has to be expressible as one.
impl From<DatError> for AssetError {
    fn from(e: DatError) -> Self {
        let id = e.data_id().unwrap_or(DataId(0));
        match e {
            DatError::NotFound(id) => AssetError::NotFound(id),
            DatError::Io(source) => AssetError::Io { id, source },
            other => AssetError::Malformed {
                id,
                reason: other.to_string(),
            },
        }
    }
}
