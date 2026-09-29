//! Writing a `.dat` container the way the retail client writes one.
//!
//! This is the container half of DDD support: put a resource record into a dat file, replace one
//! that is already there, remove one, keep the free list, the B-tree and the header exactly as
//! block allocation, tree indexing, and disk control keep them and leave a file the ordinary reader can
//! still open if the process dies half way through. The DDD protocol itself -- asking the server
//! for data, decoding `DDD_DataMessage`, invalidating live caches -- is **not** here; it lives in
//! the client.
//!
//! # The native rules this module implements
//!
//! The container layout is described in `docs/formats/01-dat-container.md`. The allocator,
//! b-tree and disk-controller behavior follow retail's exactly.
//!
//! * **Allocation is FIFO off the free list.** The store path is handed
//!   the block to start at -- always the free-list head -- and walks the free chain, so a record's
//!   blocks are exactly the first *n* blocks of the free list, in free-list order. It decrements
//!   the free-block count and sets the head to the first block it did *not* take, then writes the
//!   0x50-byte header at 0x140, and only then writes the chain's **last** block -- the
//!   terminating zero link reaches disk after the header, never before it.
//! * **Deallocation is to the tail.** The free path walks the chain
//!   setting bit 31 on every link, links the old free-list tail to the head of the freed chain,
//!   and moves the tail and the free-block count.
//! * **Growth appends.** The expand path writes `bytes / block_size`
//!   zeroed blocks from the old file size on, each pre-linked to its successor with bit 31,
//!   terminates the last with `0x80000000`, links the old free tail to the first new block and
//!   moves the file size, the free tail and the free-block count. The room check calls
//!   it while `free_count <= size / (block_size - 4) + 0x33`.
//! * **The directory is an order-62 B-tree split top-down.** The insert walk
//!   splits any node that already holds 61 entries *before* descending into it, so an insert never
//!   has to propagate a split upwards. A split is 30 / 1 / 30:
//!   entries 31..=60 and child links 31..=61 move to a freshly allocated node, entry 30 is
//!   promoted into the parent, and both halves are left with an entry count of `0x1e`.
//! * **Replacement writes first and frees last.** The replace path stores the
//!   new chain at the free-list head, repoints the entry, writes the node, and only then frees
//!   the old chain (`delete_blocks`). That ordering is why an interrupted replace is recoverable
//!   in both directions and never double-allocates a block; see [`Fault`].
//! * **The entry's first dword is `version << 16`.** The save path builds
//!   `BtEntry { bits: version << 16, id, offset: 0, size: 0, date: 0, iteration }` and
//!   clears the compressed flag, fills `date` with the wall clock when it is 0, and
//!   refuses a version of 0 or an `iteration` older than the stored one.
//!
//! # What this module deliberately does not do
//!
//! * **No transaction journal records.** Native serialises a transaction record into the
//!   64-byte slot at 0x100 before each structural step and replays it on the next open.
//!   This writer only *checks* that the slot is `NO_TRANS` before it starts and writes a
//!   `NO_TRANS` record when it finishes; it refuses to touch a file with a pending transaction
//!   rather than replaying one. Replay is a separate piece of work and the recoverability this
//!   module does claim is demonstrated without it.
//! * **No compression.** Every retail entry has the compressed flag at 0 and the DDD save path
//!   decompresses before storing, so nothing this writer produces sets the flag.
//! * **No LRU.** The LRU-in-use flag is 0 in all four retail dats and initialization forces it
//!   false, so the LRU list is never used.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use dereth_primitives::DataId;

use crate::btree::{BtEntry, NODE_SIZE};
use crate::container::{DiskFileInfo, FIRST_BLOCK, HEADER_OFFSET, TRANSACTION_OFFSET};
use crate::error::DatError;

/// A node's entry count may not exceed this; the node search rejects 0x3E and above.
pub const MAX_ENTRIES: usize = 61;
/// A node's 62 child links -- the tree's fan-out.
pub const FAN_OUT: usize = 62;
/// Each half of a split keeps this many entries (`0x1E`), with one entry promoted between them.
pub const SPLIT_HALF: usize = 30;

/// The fewest entries a node that is not the root may hold.
///
/// It is `SPLIT_HALF` by construction -- a split leaves both halves at 30 --
/// and is what keeps it true on the way down:
/// The descend-to-delete step descends only into a child holding **31**
/// or more, and repairs anything smaller first.
pub const MIN_ENTRIES: usize = SPLIT_HALF;

/// The count the delete walk (`descend_to_delete`) requires before it will descend: `MIN_ENTRIES + 1`.
const MIN_DESCEND: usize = MIN_ENTRIES + 1;

/// A rail with no native counterpart: a directory walk that visits more nodes than this has a
/// cycle in it. 61-way fan-out means a real tree is never more than six deep even at
/// `u32::MAX` entries.
const MAX_DEPTH: usize = 64;

const ENTRY_SIZE: usize = 24;
const COUNT_OFFSET: usize = FAN_OUT * 4;
const ENTRIES_OFFSET: usize = COUNT_OFFSET + 4;
/// Bit 31 of a block link: "this block is on the free list".
const FREE_BIT: u32 = 0x8000_0000;
/// The journal's magic number, the `"PL"` in every journal record.
const TRANSACT_MAGIC: u32 = 0x0000_4C50;
/// The room check keeps this many blocks in hand above what the payload needs.
const ROOM_RESERVE: u32 = 0x33;
/// The room check grows an expandable file by 1 MB at a time.
const EXPAND_BYTES: u32 = 0x0010_0000;

/// A point at which a caller can make the writer stop, to show what an interrupted write leaves
/// behind. Test-only: nothing in the client asks for one.
///
/// The two points bracket the entry update, which is the only step that changes which bytes a
/// reader sees. The client's replace path runs them in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Fault {
    /// Stop after the new chain is on disk and the free list has been advanced past it, but before
    /// the directory entry points at it. A reader still sees the **old** record.
    AfterChainWrite,
    /// Stop after the directory entry has been repointed, but before the old chain is returned to
    /// the free list. A reader sees the **new** record; the old chain is orphaned -- unreachable
    /// and, because it never reached the free list, never handed out again.
    AfterEntryUpdate,
    /// Stop inside a remove, after the record's chain has gone back to the free list
    /// and before the directory entry that named it is shifted out.
    /// The client's leaf delete has the same window: the block free first, the entry shift and
    /// the node write after.
    AfterRecordFreed,
}

/// A read-only view of one `BTNode`, for callers that need to see the shape of the tree rather
/// than just look an id up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeView {
    /// The block the node starts at.
    pub offset: u32,
    /// Child links `0..=count`, or empty for a leaf (first child link 0).
    pub children: Vec<u32>,
    /// The ids of entries `0..count`, ascending.
    pub ids: Vec<u32>,
}

/// Where every data block of the file went. The invariant a writer must never break is that no
/// block is reachable twice -- once from the free list and once from a chain, or from two chains.
///
/// An interrupted write can leave a block *orphaned*: reachable from neither. That leaks space and
/// is what native's transaction journal exists to undo, but it can never hand the same block to two
/// records, which is the failure that would corrupt a file rather than waste it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockAudit {
    /// `(file_size - 0x400) / block_size`.
    pub total: usize,
    /// Blocks on the free chain from its head.
    pub free: usize,
    /// Blocks reachable from a directory entry's offset.
    pub in_records: usize,
    /// Blocks holding `BTNode`s.
    pub in_nodes: usize,
    /// Blocks reached more than once. Must always be empty.
    pub double_allocated: Vec<u32>,
    /// Blocks reached from nowhere.
    pub orphaned: Vec<u32>,
}

impl BlockAudit {
    /// No block is reachable twice and the four counts plus the orphans account for the file.
    #[must_use]
    pub fn is_sound(&self) -> bool {
        self.double_allocated.is_empty()
            && self.free + self.in_records + self.in_nodes + self.orphaned.len() == self.total
    }
}

/// What [`DatWriter::save`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SaveOutcome {
    /// The id was not in the directory; a new entry was inserted.
    Added,
    /// The id was there; the payload was rewritten and the old chain freed.
    Replaced,
    /// The incoming iteration is non-zero and older than the stored one, so
    /// nothing was written. Native reports this as success, and so does this.
    RefusedOlderIteration,
}

// -------------------------------------------------------------------------------------------
// One B-tree node, as bytes.
// -------------------------------------------------------------------------------------------

/// A `BTNode` kept as its 0x6B4 on-disk bytes rather than as parsed fields.
///
/// Native writes the whole `BTNode` struct back, including the
/// entries and child links past the entry count, which are whatever the previous occupant of that
/// node left there. Keeping the bytes means a node this writer rewrites differs from the one it
/// read in exactly the fields it changed -- which is what makes the header/node byte comparisons in
/// the tests meaningful.
#[derive(Debug, Clone)]
struct RawNode {
    bytes: [u8; NODE_SIZE],
}

impl RawNode {
    fn empty() -> Self {
        Self {
            bytes: [0u8; NODE_SIZE],
        }
    }

    fn parse(buf: &[u8]) -> Self {
        let mut bytes = [0u8; NODE_SIZE];
        bytes.copy_from_slice(buf);
        Self { bytes }
    }

    fn dword(&self, off: usize) -> u32 {
        u32::from_le_bytes([
            self.bytes[off],
            self.bytes[off + 1],
            self.bytes[off + 2],
            self.bytes[off + 3],
        ])
    }

    fn set_dword(&mut self, off: usize, v: u32) {
        self.bytes[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn child(&self, i: usize) -> u32 {
        self.dword(i * 4)
    }

    fn set_child(&mut self, i: usize, v: u32) {
        self.set_dword(i * 4, v);
    }

    /// A first child link of 0 marks a leaf.
    fn is_leaf(&self) -> bool {
        self.child(0) == 0
    }

    fn num_entries(&self, offset: u32) -> Result<usize, DatError> {
        let n = self.dword(COUNT_OFFSET);
        if n as usize > MAX_ENTRIES {
            return Err(DatError::BadNodeEntryCount { offset, count: n });
        }
        Ok(n as usize)
    }

    fn set_num_entries(&mut self, n: usize) {
        #[allow(clippy::cast_possible_truncation)]
        self.set_dword(COUNT_OFFSET, n as u32);
    }

    fn entry(&self, i: usize) -> BtEntry {
        let o = ENTRIES_OFFSET + i * ENTRY_SIZE;
        BtEntry::parse(&self.bytes[o..o + ENTRY_SIZE])
    }

    fn set_entry(&mut self, i: usize, e: BtEntry) {
        let o = ENTRIES_OFFSET + i * ENTRY_SIZE;
        self.bytes[o..o + ENTRY_SIZE].copy_from_slice(&e.to_bytes());
    }

    /// Entry `dst` = entry `src`, the 24-byte struct copy the native shift loops perform.
    fn move_entry(&mut self, dst: usize, src: usize) {
        let s = ENTRIES_OFFSET + src * ENTRY_SIZE;
        let d = ENTRIES_OFFSET + dst * ENTRY_SIZE;
        self.bytes.copy_within(s..s + ENTRY_SIZE, d);
    }
}

// -------------------------------------------------------------------------------------------
// The writer.
// -------------------------------------------------------------------------------------------

/// A `.dat` container open for writing.
///
/// [`DatWriter::open`] **refuses any path in a directory declared read-only** with
/// [`crate::protect_install`]: a pristine install is not disposable. Work on a copy.
#[derive(Debug)]
pub struct DatWriter {
    path: PathBuf,
    file: File,
    /// The header's 0x50 bytes exactly as they are on disk, so that the fields native never touches
    /// -- the 0xCD padding after the LRU-in-use flag, the version GUID -- survive a rewrite.
    raw_header: [u8; 0x50],
    header: DiskFileInfo,
    fault: Option<Fault>,
}

impl DatWriter {
    /// Open an existing container for writing.
    ///
    /// # Errors
    ///
    /// [`DatError::RetailDatRefused`] when the path is inside a read-only install,
    /// [`DatError::PendingTransaction`] when the journal slot at 0x100 holds an unfinished
    /// operation this writer cannot replay, plus the usual header and I/O errors.
    pub fn open(path: &Path) -> Result<Self, DatError> {
        Self::refuse_owner_dat(path)?;
        // A reader that walked this file may be in the process-wide table, and
        // what follows may change the file without changing its length or its mtime tick. Drop
        // the entry rather than trust a timestamp.
        crate::store::shared::forget(path);
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let mut me = Self {
            path: path.to_path_buf(),
            file,
            raw_header: [0u8; 0x50],
            header: blank_header(),
            fault: None,
        };
        let mut hdr = [0u8; 0x50];
        me.read_at(HEADER_OFFSET, &mut hdr)?;
        me.raw_header = hdr;
        me.header = DiskFileInfo::parse(&hdr)?;
        me.require_quiescent()?;
        Ok(me)
    }

    /// Build a new container: create the file, then create an empty tree in it.
    ///
    /// `file_size` is the total size including the 0x400-byte prologue and must be a whole number
    /// of blocks past it, exactly as retail's file creation assumes when it computes
    /// `free_count = (file_size - 0x400) / block_size`. The version stamp is the one all four
    /// retail files carry, so that a reader cannot tell a created file apart by its header.
    ///
    /// # Errors
    ///
    /// [`DatError::BadBlockSize`] or [`DatError::BadCreateSize`] for parameters retail's file
    /// creation could not have produced, plus I/O errors.
    pub fn create(
        path: &Path,
        block_size: u32,
        data_set: u32,
        data_subset: u32,
        file_size: u32,
    ) -> Result<Self, DatError> {
        Self::refuse_owner_dat(path)?;
        if block_size < 8 || !block_size.is_multiple_of(4) {
            return Err(DatError::BadBlockSize(block_size));
        }
        if file_size <= FIRST_BLOCK || !(file_size - FIRST_BLOCK).is_multiple_of(block_size) {
            return Err(DatError::BadCreateSize {
                file_size,
                block_size,
            });
        }
        // As in `open`: this truncates, and a path can be reused.
        crate::store::shared::forget(path);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        let mut me = Self {
            path: path.to_path_buf(),
            file,
            raw_header: retail_shaped_header(block_size, data_set, data_subset, file_size),
            header: blank_header(),
            fault: None,
        };
        me.header = DiskFileInfo::parse(&me.raw_header)?;

        // The 0x400-byte prologue: banner area, journal slot, header, tail padding.
        me.write_at(0, &[0u8; FIRST_BLOCK as usize])?;
        me.clear_transaction()?;

        // File creation: one free block per block size, each linked to the next with bit 31,
        // the last terminated with `0x80000000`.
        let blocks = (file_size - FIRST_BLOCK) / block_size;
        self_create_free_list(&mut me, block_size, blocks)?;
        me.save_file_info()?;

        // Tree creation: the root node takes the first free block.
        let root = me.header.free_head;
        me.header.btree_root = root;
        let empty = RawNode::empty();
        me.store_data(&empty.bytes, root)?;
        me.save_file_info()?;
        Ok(me)
    }

    /// Make the next [`DatWriter::save`] stop at `fault` and report
    /// [`DatError::Interrupted`]. The flag clears itself when it fires.
    pub fn inject_fault(&mut self, fault: Fault) {
        self.fault = Some(fault);
    }

    #[must_use]
    pub fn header(&self) -> &DiskFileInfo {
        &self.header
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stop here if this is the point the caller asked to be interrupted at.
    ///
    /// Peeks rather than takes: a `save` passes several fault points and only the matching one may
    /// clear the flag, otherwise the first point reached would swallow every later one.
    fn fire(&mut self, at: Fault) -> Result<(), DatError> {
        if self.fault == Some(at) {
            self.fault = None;
            return Err(DatError::Interrupted(at));
        }
        Ok(())
    }

    /// Refuse to open anything inside a read-only install for writing.
    fn refuse_owner_dat(path: &Path) -> Result<(), DatError> {
        if crate::locate::is_protected(path) {
            return Err(DatError::RetailDatRefused(path.to_path_buf()));
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------------------
    // Raw I/O. The client's synchronous write is `SetFilePointer` + `WriteFile` with no flush,
    // which is what `seek` + `write_all` on an unbuffered `File` is.
    // ---------------------------------------------------------------------------------------

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), DatError> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(buf)?;
        Ok(())
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), DatError> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(buf)?;
        Ok(())
    }

    fn link_at(&mut self, block: u32) -> Result<u32, DatError> {
        self.check_block(block)?;
        let mut b = [0u8; 4];
        self.read_at(u64::from(block), &mut b)?;
        Ok(u32::from_le_bytes(b))
    }

    fn write_link(&mut self, block: u32, link: u32) -> Result<(), DatError> {
        self.check_block(block)?;
        self.write_at(u64::from(block), &link.to_le_bytes())
    }

    /// Native trusts the free-list head and walks off the end of the free list into offset 0 when
    /// it is wrong (its data store has no bound check at all). Refusing is the whole difference
    /// between "safe writer" and "writer".
    fn check_block(&self, block: u32) -> Result<(), DatError> {
        if block < FIRST_BLOCK || u64::from(block) >= u64::from(self.header.file_size) {
            return Err(DatError::BlockOutOfRange(block));
        }
        if !(block - FIRST_BLOCK).is_multiple_of(self.header.block_size) {
            return Err(DatError::BlockOutOfRange(block));
        }
        Ok(())
    }

    /// Write the 0x50-byte file info back, synchronously, at 0x140.
    fn save_file_info(&mut self) -> Result<(), DatError> {
        let mut raw = self.raw_header;
        raw[0x08..0x0C].copy_from_slice(&self.header.file_size.to_le_bytes());
        raw[0x14..0x18].copy_from_slice(&self.header.free_head.to_le_bytes());
        raw[0x18..0x1C].copy_from_slice(&self.header.free_tail.to_le_bytes());
        raw[0x1C..0x20].copy_from_slice(&self.header.free_count.to_le_bytes());
        raw[0x20..0x24].copy_from_slice(&self.header.btree_root.to_le_bytes());
        self.raw_header = raw;
        self.write_at(HEADER_OFFSET, &raw)
    }

    /// Clear the transaction journal -- a `NO_TRANS` record, five bytes, leaving
    /// the tail of whatever longer record was there before.
    fn clear_transaction(&mut self) -> Result<(), DatError> {
        let mut rec = [0u8; 5];
        rec[1..5].copy_from_slice(&TRANSACT_MAGIC.to_le_bytes());
        self.write_at(TRANSACTION_OFFSET, &rec)
    }

    /// The journal read, used as a precondition rather than as a replay trigger.
    fn require_quiescent(&mut self) -> Result<(), DatError> {
        let mut rec = [0u8; 5];
        self.read_at(TRANSACTION_OFFSET, &mut rec)?;
        let magic = u32::from_le_bytes([rec[1], rec[2], rec[3], rec[4]]);
        if magic != TRANSACT_MAGIC {
            return Err(DatError::BadTransactionMagic(magic));
        }
        if rec[0] != 0 {
            return Err(DatError::PendingTransaction(rec[0]));
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------------------
    // Block chains.
    // ---------------------------------------------------------------------------------------

    /// Store a payload as a block chain.
    ///
    /// `head` is the block the chain starts at and is always the free-list head; the rest of the
    /// chain is the free list's next blocks, taken in order. The header is written *before* the
    /// last block, which is what makes a crash inside this function leave a consistent free list.
    fn store_data(&mut self, payload: &[u8], head: u32) -> Result<u32, DatError> {
        let per = self.header.payload_per_block();
        if payload.is_empty() {
            return Ok(0);
        }
        let mut remaining = payload.len();
        let mut src = 0usize;
        let mut dest = head;
        let mut taken: u32 = 1;
        let mut next = self.link_at(dest)? & !FREE_BIT;

        while remaining > 0 {
            let (link, chunk) = if per < remaining {
                if next == 0 {
                    return Err(DatError::FreeListExhausted);
                }
                let after = self.link_at(next)? & !FREE_BIT;
                taken += 1;
                let link = next;
                next = after;
                (link, per)
            } else {
                self.header.free_count = self
                    .header
                    .free_count
                    .checked_sub(taken)
                    .ok_or(DatError::FreeListExhausted)?;
                self.header.free_head = next;
                self.save_file_info()?;
                (0u32, remaining)
            };
            let mut block = Vec::with_capacity(chunk + 4);
            block.extend_from_slice(&link.to_le_bytes());
            block.extend_from_slice(&payload[src..src + chunk]);
            self.write_at(u64::from(dest), &block)?;
            remaining -= chunk;
            src += chunk;
            dest = link;
        }
        Ok(taken)
    }

    /// The block-chain load, the same walk [`crate::DatFile`] does.
    fn load_data(&mut self, id: DataId, mut offset: u32, size: usize) -> Result<Vec<u8>, DatError> {
        let per = self.header.payload_per_block();
        let mut out = Vec::with_capacity(size);
        let mut remaining = size;
        let mut block = vec![0u8; self.header.block_size as usize];
        while remaining > 0 && offset != 0 {
            let take = remaining.min(per);
            self.read_at(u64::from(offset), &mut block[..take + 4])?;
            let link = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
            if link & FREE_BIT != 0 {
                return Err(DatError::FreeBlockInChain { id, offset });
            }
            out.extend_from_slice(&block[4..take + 4]);
            remaining -= take;
            offset = link;
        }
        if remaining != 0 {
            return Err(DatError::ChainTooShort {
                id,
                shortfall: remaining,
            });
        }
        Ok(out)
    }

    /// Free a block chain. Returns the number of blocks freed.
    ///
    /// `allow_free` is native's allow-free flag: `false` makes a chain that already walks into a free
    /// block an error (-0x65), which is how a double free is caught.
    fn delete_blocks(&mut self, head: u32, allow_free: bool) -> Result<u32, DatError> {
        if head == 0 {
            return Ok(0);
        }
        let mut freed = 0u32;
        let mut cur = head;
        let tail;
        loop {
            let link = self.link_at(cur)?;
            if !allow_free && link & FREE_BIT != 0 {
                return Err(DatError::DoubleFree(cur));
            }
            self.write_link(cur, link | FREE_BIT)?;
            freed += 1;
            let next = link & !FREE_BIT;
            if next == 0 {
                tail = cur;
                break;
            }
            cur = next;
        }
        let old_tail = self.header.free_tail;
        self.write_link(old_tail, head | FREE_BIT)?;
        self.header.free_tail = tail;
        self.header.free_count += freed;
        self.save_file_info()?;
        Ok(freed)
    }

    /// Grow the file by whole blocks and link them onto the free tail.
    fn expand_file(&mut self, bytes: u32) -> Result<u32, DatError> {
        let bs = self.header.block_size;
        let n = bytes / bs;
        if n == 0 {
            return Ok(0);
        }
        if self.header.free_tail == 0 {
            return Err(DatError::FreeListExhausted);
        }
        self.header.free_count += n;
        let mut cur = self.header.file_size;
        let mut last = cur;
        let mut block = vec![0u8; bs as usize];
        for _ in 0..n {
            last = cur;
            cur = last.checked_add(bs).ok_or(DatError::BadCreateSize {
                file_size: last,
                block_size: bs,
            })?;
            block[..4].copy_from_slice(&(cur | FREE_BIT).to_le_bytes());
            self.write_at(u64::from(last), &block)?;
        }
        let old_tail = self.header.free_tail;
        let first_new = self.header.file_size;
        self.write_at(u64::from(last), &FREE_BIT.to_le_bytes())?;
        self.header.file_size = cur;
        self.header.free_tail = last;
        self.write_at(u64::from(old_tail), &(first_new | FREE_BIT).to_le_bytes())?;
        self.save_file_info()?;
        Ok(n)
    }

    /// The room check, expandable branch.
    fn check_room(&mut self, size: usize) -> Result<(), DatError> {
        #[allow(clippy::cast_possible_truncation)]
        let need = (size / self.header.payload_per_block()) as u32 + ROOM_RESERVE;
        while self.header.free_count <= need {
            if self.expand_file(EXPAND_BYTES)? == 0 {
                return Err(DatError::FreeListExhausted);
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------------------
    // The directory.
    // ---------------------------------------------------------------------------------------

    fn read_node(&mut self, offset: u32) -> Result<RawNode, DatError> {
        self.check_block(offset)?;
        let raw = self.load_data(DataId(0), offset, NODE_SIZE)?;
        Ok(RawNode::parse(&raw))
    }

    /// Write a node back through the block allocator's update path.
    ///
    /// A node is always exactly 0x6B4 bytes and its chain was allocated for exactly that, so the
    /// only branch of the allocator's update a node write can reach is "rewrite the existing chain
    /// in place". The grow and shrink branches have no other caller in the client, so rather than
    /// carry two untestable paths this refuses a chain of the wrong length.
    fn write_node(&mut self, offset: u32, node: &RawNode) -> Result<(), DatError> {
        let per = self.header.payload_per_block();
        let want = NODE_SIZE.div_ceil(per);
        let mut dest = offset;
        let mut src = 0usize;
        let mut used = 0usize;
        while src < NODE_SIZE {
            used += 1;
            if used > want {
                return Err(DatError::NodeChainLength {
                    offset,
                    blocks: used,
                    want,
                });
            }
            let chunk = (NODE_SIZE - src).min(per);
            let link = if src + chunk >= NODE_SIZE {
                0
            } else {
                self.link_at(dest)?
            };
            if link & FREE_BIT != 0 {
                return Err(DatError::FreeBlockInChain {
                    id: DataId(0),
                    offset: dest,
                });
            }
            if link == 0 && src + chunk < NODE_SIZE {
                return Err(DatError::NodeChainLength {
                    offset,
                    blocks: used,
                    want,
                });
            }
            let mut buf = Vec::with_capacity(chunk + 4);
            buf.extend_from_slice(&link.to_le_bytes());
            buf.extend_from_slice(&node.bytes[src..src + chunk]);
            self.write_at(u64::from(dest), &buf)?;
            src += chunk;
            dest = link;
        }
        if used != want {
            return Err(DatError::NodeChainLength {
                offset,
                blocks: used,
                want,
            });
        }
        Ok(())
    }

    /// Take the free-list head and store an empty node there.
    ///
    /// Native reuses an evicted in-memory node and only resets its entry count and first child
    /// link, so the block it writes carries the previous occupant's bytes past those two fields.
    /// Here a fresh node is all zeroes. Nothing can read the difference: search examines only
    /// child links `0..=count` and entries `0..count` and no further.
    fn allocate_empty_node(&mut self) -> Result<u32, DatError> {
        let offset = self.header.free_head;
        self.check_block(offset)?;
        let empty = RawNode::empty();
        self.store_data(&empty.bytes, offset)?;
        Ok(offset)
    }

    /// The lookup: descend, then binary-search the node, returning where
    /// the entry lives so that a replace can rewrite it.
    fn find(&mut self, id: u32) -> Result<Option<(u32, usize, BtEntry)>, DatError> {
        if id == 0 {
            return Ok(None);
        }
        let mut offset = self.header.btree_root;
        loop {
            let node = self.read_node(offset)?;
            let n = node.num_entries(offset)?;
            let mut lo = 0usize;
            let mut hi = n;
            let mut hit = None;
            while lo < hi {
                let mid = (lo + hi) / 2;
                let key = node.entry(mid).id;
                if key == id {
                    hit = Some(mid);
                    break;
                } else if key < id {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
            if let Some(i) = hit {
                return Ok(Some((offset, i, node.entry(i))));
            }
            if node.is_leaf() {
                return Ok(None);
            }
            offset = node.child(lo);
        }
    }

    /// Split child `idx` of the node at `parent_offset`.
    fn split_child(&mut self, parent_offset: u32, idx: usize) -> Result<(), DatError> {
        let mut parent = self.read_node(parent_offset)?;
        let parent_n = parent.num_entries(parent_offset)?;
        let full_offset = parent.child(idx);
        let mut full = self.read_node(full_offset)?;
        let full_n = full.num_entries(full_offset)?;
        if full_n != MAX_ENTRIES {
            return Err(DatError::SplitOfUnfullNode {
                offset: full_offset,
                entries: full_n,
            });
        }

        // The right half: entries 31..=60 and, for an internal node, child links 31..=61.
        let right_offset = self.allocate_empty_node()?;
        let mut right = RawNode::empty();
        for k in 0..SPLIT_HALF {
            right.set_entry(k, full.entry(SPLIT_HALF + 1 + k));
        }
        if !full.is_leaf() {
            for k in 0..=SPLIT_HALF {
                right.set_child(k, full.child(SPLIT_HALF + 1 + k));
            }
        }
        right.set_num_entries(SPLIT_HALF);
        self.write_node(right_offset, &right)?;

        // The parent gains entry 30 at `idx` and the new node as child `idx + 1`.
        let promoted = full.entry(SPLIT_HALF);
        for k in ((idx + 1)..=parent_n).rev() {
            parent.set_child(k + 1, parent.child(k));
            parent.move_entry(k, k - 1);
        }
        parent.set_child(idx + 1, right_offset);
        parent.set_entry(idx, promoted);
        parent.set_num_entries(parent_n + 1);
        self.write_node(parent_offset, &parent)?;

        // The left half keeps entries 0..=29. Native only lowers the entry count, leaving the rest
        // of the struct alone, and so does this.
        full.set_num_entries(SPLIT_HALF);
        self.write_node(full_offset, &full)?;
        Ok(())
    }

    /// Insert a brand new entry, splitting full nodes on the way
    /// down so that the insert itself never has to propagate one back up.
    fn descend_to_add(&mut self, entry: BtEntry, payload: &[u8]) -> Result<(), DatError> {
        let id = entry.id;

        // Root split. Native allocates an empty node, swaps the in-memory root into it, and makes
        // the *new* block the root with the old root as its only child; the result on disk is a
        // new root block pointing at the old one.
        let root_n = {
            let root = self.read_node(self.header.btree_root)?;
            root.num_entries(self.header.btree_root)?
        };
        if root_n == MAX_ENTRIES {
            let old_root = self.header.btree_root;
            let new_root = self.allocate_empty_node()?;
            let mut node = RawNode::empty();
            node.set_child(0, old_root);
            node.set_num_entries(0);
            self.header.btree_root = new_root;
            self.save_file_info()?;
            self.write_node(new_root, &node)?;
            self.split_child(new_root, 0)?;
        }

        let mut offset = self.header.btree_root;
        loop {
            let node = self.read_node(offset)?;
            let n = node.num_entries(offset)?;
            if node.is_leaf() {
                return self.insert_into_leaf(offset, node, n, entry, payload);
            }
            // The number of entries whose id is <= the target, as native counts it.
            let mut i = n;
            while i > 0 && node.entry(i - 1).id > id {
                i -= 1;
            }
            let child_offset = node.child(i);
            let child_n = {
                let child = self.read_node(child_offset)?;
                child.num_entries(child_offset)?
            };
            if child_n == MAX_ENTRIES {
                self.split_child(offset, i)?;
                let parent = self.read_node(offset)?;
                if parent.entry(i).id < id {
                    i += 1;
                }
                offset = parent.child(i);
            } else {
                offset = child_offset;
            }
        }
    }

    /// Shift the leaf's entries up, drop the new one in, store
    /// the payload at the free-list head, then write the node.
    fn insert_into_leaf(
        &mut self,
        offset: u32,
        mut node: RawNode,
        n: usize,
        entry: BtEntry,
        payload: &[u8],
    ) -> Result<(), DatError> {
        let mut i = n;
        while i > 0 && node.entry(i - 1).id > entry.id {
            node.move_entry(i, i - 1);
            i -= 1;
        }
        let head = self.header.free_head;
        let mut e = entry;
        e.offset = head;
        #[allow(clippy::cast_possible_truncation)]
        {
            e.size = payload.len() as u32;
        }
        node.set_entry(i, e);
        node.set_num_entries(n + 1);

        self.store_data(payload, head)?;
        self.fire(Fault::AfterChainWrite)?;
        self.write_node(offset, &node)?;
        self.fire(Fault::AfterEntryUpdate)?;
        Ok(())
    }

    /// New chain, repointed entry, then the old chain freed.
    fn replace_entry(
        &mut self,
        node_offset: u32,
        index: usize,
        entry: BtEntry,
        payload: &[u8],
        old_offset: u32,
    ) -> Result<(), DatError> {
        let head = self.header.free_head;
        let mut node = self.read_node(node_offset)?;
        let mut e = entry;
        e.offset = head;
        #[allow(clippy::cast_possible_truncation)]
        {
            e.size = payload.len() as u32;
        }
        node.set_entry(index, e);

        self.store_data(payload, head)?;
        self.fire(Fault::AfterChainWrite)?;
        self.write_node(node_offset, &node)?;
        self.fire(Fault::AfterEntryUpdate)?;
        // Native passes `allow_free = true` here (its object update does, because it may be
        // replaying a half-finished transaction and the chain may already be free). With no replay
        // path this writer has no such case, so it passes `false` and lets `delete_blocks` report a
        // chain that is already on the free list rather than splicing the free list into itself.
        self.delete_blocks(old_offset, false)?;
        Ok(())
    }

    // ---------------------------------------------------------------------------------------
    // The public write.
    // ---------------------------------------------------------------------------------------

    /// The public save, with `flags = 1`.
    ///
    /// `version` becomes the entry's version field and must not be 0; `date` of 0 is filled with
    /// the wall clock the way the client's save path does. A zero-length payload is refused: native
    /// would store an entry whose offset points at a block still on the free list, which the next
    /// allocation would hand out again.
    ///
    /// # Errors
    ///
    /// [`DatError::ZeroVersion`] / [`DatError::EmptyPayload`] for a record native would reject or
    /// mis-store, [`DatError::Interrupted`] when a [`Fault`] was injected, plus I/O errors.
    pub fn save(
        &mut self,
        id: DataId,
        payload: &[u8],
        version: u16,
        iteration: u32,
        date: u32,
    ) -> Result<SaveOutcome, DatError> {
        if id.raw() == 0 {
            return Err(DatError::NotFound(id));
        }
        if version == 0 {
            return Err(DatError::ZeroVersion(id));
        }
        if payload.is_empty() {
            return Err(DatError::EmptyPayload(id));
        }
        let date = if date == 0 { now_unix() } else { date };
        #[allow(clippy::cast_possible_truncation)]
        let entry = BtEntry {
            bits: u32::from(version) << 16,
            id: id.raw(),
            offset: 0,
            size: payload.len() as u32,
            date,
            iteration,
        };

        self.check_room(payload.len())?;
        let outcome = match self.find(id.raw())? {
            None => {
                self.descend_to_add(entry, payload)?;
                SaveOutcome::Added
            }
            Some((node_offset, index, old)) => {
                if iteration != 0 && iteration < old.iteration {
                    return Ok(SaveOutcome::RefusedOlderIteration);
                }
                self.replace_entry(node_offset, index, entry, payload, old.offset)?;
                SaveOutcome::Replaced
            }
        };
        self.save_file_info()?;
        self.clear_transaction()?;
        Ok(outcome)
    }

    /// Read a record back through the writer's own copy of the directory walk.
    ///
    /// # Errors
    ///
    /// [`DatError::NotFound`] when the id is not in the directory, plus I/O errors.
    pub fn read(&mut self, id: DataId) -> Result<Vec<u8>, DatError> {
        let (_, _, e) = self.find(id.raw())?.ok_or(DatError::NotFound(id))?;
        if e.compressed() {
            return Err(DatError::CompressionUnsupported(id));
        }
        self.load_data(id, e.offset, e.size as usize)
    }

    /// The shape of one `BTNode`.
    ///
    /// # Errors
    ///
    /// [`DatError::BlockOutOfRange`] or [`DatError::BadNodeEntryCount`] for a node the reader would
    /// also refuse, plus I/O errors.
    pub fn node(&mut self, offset: u32) -> Result<NodeView, DatError> {
        let node = self.read_node(offset)?;
        let n = node.num_entries(offset)?;
        let children = if node.is_leaf() {
            Vec::new()
        } else {
            (0..=n).map(|i| node.child(i)).collect()
        };
        Ok(NodeView {
            offset,
            children,
            ids: (0..n).map(|i| node.entry(i).id).collect(),
        })
    }

    /// The root of the directory.
    #[must_use]
    pub fn root(&self) -> u32 {
        self.header.btree_root
    }

    /// The directory entry for an id, looked up through the tree.
    ///
    /// # Errors
    ///
    /// Whatever the directory walk returns, plus I/O errors.
    pub fn entry(&mut self, id: DataId) -> Result<Option<BtEntry>, DatError> {
        Ok(self.find(id.raw())?.map(|(_, _, e)| e))
    }

    /// Walk the free list and every chain in the file and say where each data block went.
    ///
    /// This is the instrument the recoverability tests read: it is the only way to tell an
    /// orphaned block (leaked, harmless) from a double-allocated one (corruption).
    ///
    /// # Errors
    ///
    /// Whatever the free-chain and directory walks return, plus I/O errors.
    pub fn audit(&mut self) -> Result<BlockAudit, DatError> {
        let bs = self.header.block_size;
        let total = (self.header.file_size as usize - FIRST_BLOCK as usize) / bs as usize;
        let mut seen = vec![0u8; total];
        let mut dupes = Vec::new();

        // The free chain.
        let mut free = 0usize;
        let mut cur = self.header.free_head;
        while cur != 0 {
            self.check_block(cur)?;
            mark_block(cur, bs, &mut seen, &mut dupes);
            free += 1;
            if free > total {
                return Err(DatError::DoubleFree(cur));
            }
            cur = self.link_at(cur)? & !FREE_BIT;
        }

        // Every node, and every record it names.
        let per = self.header.payload_per_block();
        let mut in_nodes = 0usize;
        let mut in_records = 0usize;
        let mut stack = vec![self.header.btree_root];
        while let Some(offset) = stack.pop() {
            let node = self.read_node(offset)?;
            let n = node.num_entries(offset)?;
            in_nodes += self.mark_chain(offset, NODE_SIZE, per, &mut seen, &mut dupes, bs)?;
            for i in 0..n {
                let e = node.entry(i);
                in_records +=
                    self.mark_chain(e.offset, e.size as usize, per, &mut seen, &mut dupes, bs)?;
            }
            if !node.is_leaf() {
                for i in 0..=n {
                    stack.push(node.child(i));
                }
            }
        }

        let mut orphaned = Vec::new();
        for (i, c) in seen.iter().enumerate() {
            if *c == 0 {
                #[allow(clippy::cast_possible_truncation)]
                orphaned.push(FIRST_BLOCK + (i as u32) * bs);
            }
        }
        Ok(BlockAudit {
            total,
            free,
            in_records,
            in_nodes,
            double_allocated: dupes,
            orphaned,
        })
    }

    /// Mark the `ceil(size / per)` blocks of one chain, returning how many there were.
    fn mark_chain(
        &mut self,
        head: u32,
        size: usize,
        per: usize,
        seen: &mut [u8],
        dupes: &mut Vec<u32>,
        bs: u32,
    ) -> Result<usize, DatError> {
        let mut remaining = size;
        let mut offset = head;
        let mut n = 0usize;
        while remaining > 0 && offset != 0 {
            self.check_block(offset)?;
            mark_block(offset, bs, seen, dupes);
            n += 1;
            remaining -= remaining.min(per);
            offset = self.link_at(offset)?;
            if offset & FREE_BIT != 0 {
                return Err(DatError::FreeBlockInChain {
                    id: DataId(0),
                    offset,
                });
            }
        }
        Ok(n)
    }

    /// The `0xFFFF0001` iteration set.
    ///
    /// # Errors
    ///
    /// [`DatError::NotFound`] when the dat has no iteration file, plus decode and I/O errors.
    pub fn iteration_list(&mut self) -> Result<Vec<u32>, DatError> {
        let raw = self.read(crate::divine::ITERATION_LIST)?;
        crate::iteration::decode(&raw)
    }

    /// Add one iteration to the set and rewrite
    /// `0xFFFF0001`. The patch path does this once every file for a revision has landed.
    ///
    /// The entry keeps version 1 and iteration 0, which is what every retail dat carries for this
    /// id.
    ///
    /// # Errors
    ///
    /// Whatever [`DatWriter::iteration_list`] and [`DatWriter::save`] return.
    pub fn add_iteration(&mut self, iteration: u32, date: u32) -> Result<(), DatError> {
        let mut set = self.iteration_list()?;
        set.push(iteration);
        set.sort_unstable();
        set.dedup();
        let bytes = crate::iteration::encode(&set);
        self.save(crate::divine::ITERATION_LIST, &bytes, 1, 0, date)?;
        Ok(())
    }

    // ---------------------------------------------------------------------------------------
    // Removal -- the tree's own delete path.
    //
    // The mirror image of the insert path. The insert walk splits a full node *before*
    // descending into it so an insert never propagates a split upward; the delete walk
    // fills a minimal node *before* descending into it so a delete never propagates an
    // underflow upward. Minimum degree is 31: a node holds 30..=61 entries, only the root may hold
    // fewer, and a merge of two minimal children plus their separator is 30 + 1 + 30 = 61, which
    // is exactly the node split run backwards.
    // ---------------------------------------------------------------------------------------

    /// Delete with no iteration limit, which is what the DDD
    /// purge path passes -- the mask delete passes a zero limit.
    ///
    /// Answers whether the id was there. The record's block chain goes back to the **tail** of the
    /// free list, per the allocator's FIFO rule.
    ///
    /// # Errors
    ///
    /// Whatever the directory walk and the block writes return.
    pub fn remove(&mut self, id: DataId) -> Result<bool, DatError> {
        self.delete_data(id, 0)
    }

    /// The delete, in full.
    ///
    /// `iteration_limit` is native's iteration limit: the check is skipped
    /// when it is 0, and the stored iteration is compared against it, refusing a record whose
    /// stored iteration is *newer* than the limit.
    ///
    /// # Errors
    ///
    /// Whatever the directory walk and the block writes return.
    pub fn delete_data(&mut self, id: DataId, iteration_limit: u32) -> Result<bool, DatError> {
        if id.raw() == 0 {
            return Ok(false);
        }
        let Some((_, _, entry)) = self.find(id.raw())? else {
            return Ok(false);
        };
        if iteration_limit != 0 && entry.iteration > iteration_limit {
            return Ok(false);
        }
        let removed = self.btree_remove(id.raw())?;
        if removed {
            self.save_file_info()?;
            self.clear_transaction()?;
        }
        Ok(removed)
    }

    /// Delete by mask -- the call
    /// the cache's DDD-request worker makes with `mask = 0xFFFF0000` to purge
    /// a whole landblock family.
    ///
    /// Native collects candidate ids first and only then deletes, because deleting rebalances the
    /// tree under the walk. This does the same. When the mask is one contiguous run of high bits
    /// it asks the tree for the range `[id & mask, (id & mask) | !mask]`
    /// and otherwise for every id; either way the `(candidate & mask) == target` filter at the
    /// delete decides, so the two paths select the same set.
    ///
    /// Returns how many records were removed. A `mask` of 0 removes nothing, exactly as native
    /// returns early on a zero mask.
    ///
    /// # Errors
    ///
    /// Whatever the directory walk and the block writes return.
    pub fn delete_data_by_mask(&mut self, id: DataId, mask: u32) -> Result<usize, DatError> {
        if mask == 0 {
            return Ok(0);
        }
        let target = id.raw() & mask;
        // Native's contiguity test, which for `0xFFFF0000` answers "yes".
        let contiguous =
            mask & 0x8000_0000 == 0 || mask.leading_ones() + mask.trailing_zeros() == 32;
        let candidates = if contiguous {
            self.ids_in_range(target, !mask | target)?
        } else {
            self.ids_in_range(0, u32::MAX)?
        };
        let mut removed = 0usize;
        for candidate in candidates {
            if candidate & mask == target && self.delete_data(DataId(candidate), 0)? {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Every id in `[lo, hi]`, ascending.
    ///
    /// # Errors
    ///
    /// Whatever the directory walk returns.
    pub fn ids_in_range(&mut self, lo: u32, hi: u32) -> Result<Vec<u32>, DatError> {
        let mut out = Vec::new();
        let mut stack = vec![self.header.btree_root];
        while let Some(offset) = stack.pop() {
            let node = self.read_node(offset)?;
            let n = node.num_entries(offset)?;
            for i in 0..n {
                let id = node.entry(i).id;
                if id >= lo && id <= hi {
                    out.push(id);
                }
            }
            if !node.is_leaf() {
                for i in 0..=n {
                    // Child `i` holds keys strictly between entries `i - 1` and `i`, so it
                    // can only matter when that interval meets `[lo, hi]`.
                    let below_hi = i == 0 || node.entry(i - 1).id < hi;
                    let above_lo = i == n || node.entry(i).id > lo;
                    if below_hi && above_lo {
                        stack.push(node.child(i));
                    }
                }
            }
        }
        out.sort_unstable();
        Ok(out)
    }

    /// Remove one id from the tree.
    fn btree_remove(&mut self, id: u32) -> Result<bool, DatError> {
        let mut start = self.header.btree_root;
        for _ in 0..MAX_DEPTH {
            let Some((node_offset, index)) = self.descend_to_delete(start, id)? else {
                return Ok(false);
            };
            let node = self.read_node(node_offset)?;
            if node.is_leaf() {
                self.delete_leaf(node_offset, index)?;
                return Ok(true);
            }

            // The left child holds more than 0x1e entries: the left subtree can spare its greatest
            // entry, so the predecessor is found and takes the target's place.
            let left_offset = node.child(index);
            let left_n = self.read_node(left_offset)?.num_entries(left_offset)?;
            if left_n > MIN_ENTRIES {
                let donor_id = self.find_max(left_offset)?;
                let (donor_offset, donor_index) = self
                    .descend_to_delete(left_offset, donor_id)?
                    .ok_or(DatError::MissingDonorEntry(donor_id))?;
                self.delete_internal(node_offset, index, donor_offset, donor_index)?;
                return Ok(true);
            }

            // Otherwise the right subtree's least entry.
            let right_offset = node.child(index + 1);
            let right_n = self.read_node(right_offset)?.num_entries(right_offset)?;
            if right_n > MIN_ENTRIES {
                let donor_id = self.find_min(right_offset)?;
                let (donor_offset, donor_index) = self
                    .descend_to_delete(right_offset, donor_id)?
                    .ok_or(DatError::MissingDonorEntry(donor_id))?;
                self.delete_internal(node_offset, index, donor_offset, donor_index)?;
                return Ok(true);
            }

            // Neither can spare one: merge them with the target pushed down between them and look
            // again in the merged node, which is what native's tail-recursive delete on the merged
            // node does.
            start = self.merge_nodes(node_offset, left_offset, right_offset, index)?;
        }
        Err(DatError::DirectoryLoop(start))
    }

    /// Walk down to the node holding `id`, repairing any
    /// child that is at the minimum *before* stepping into it.
    ///
    /// The repair order is native's, and it is asymmetric: borrow from the left sibling first,
    /// then from the right, then merge -- with the right sibling when there is one (a merge at
    /// `idx`), otherwise with the left (the same merge at `idx - 1`, after which the descent
    /// continues into the *left* node because that is the one that survived).
    fn descend_to_delete(&mut self, start: u32, id: u32) -> Result<Option<(u32, usize)>, DatError> {
        let mut node_offset = start;
        for _ in 0..MAX_DEPTH {
            let node = self.read_node(node_offset)?;
            let n = node.num_entries(node_offset)?;
            let (found, index) = has_entry(&node, n, id);
            if found {
                return Ok(Some((node_offset, index)));
            }
            if node.is_leaf() {
                return Ok(None);
            }

            let mut child_offset = node.child(index);
            let child_n = self.read_node(child_offset)?.num_entries(child_offset)?;
            if child_n < MIN_DESCEND {
                let mut filled = false;
                let has_left = index > 0;
                if has_left {
                    let sibling = node.child(index - 1);
                    if self.read_node(sibling)?.num_entries(sibling)? > MIN_ENTRIES {
                        self.rotate_entry(node_offset, index, true)?;
                        filled = true;
                    }
                }
                let has_right = !filled && index < n;
                if has_right {
                    let sibling = node.child(index + 1);
                    if self.read_node(sibling)?.num_entries(sibling)? > MIN_ENTRIES {
                        self.rotate_entry(node_offset, index, false)?;
                        filled = true;
                    }
                }
                if !filled {
                    if index < n {
                        self.merge_nodes(node_offset, child_offset, node.child(index + 1), index)?;
                    } else if has_left {
                        let left = node.child(index - 1);
                        self.merge_nodes(node_offset, left, child_offset, index - 1)?;
                        child_offset = left;
                    }
                }
            }
            if child_offset == 0 {
                return Ok(None);
            }
            node_offset = child_offset;
        }
        Err(DatError::DirectoryLoop(node_offset))
    }

    /// The least id of the subtree at `offset`.
    fn find_min(&mut self, offset: u32) -> Result<u32, DatError> {
        let mut at = offset;
        for _ in 0..MAX_DEPTH {
            let node = self.read_node(at)?;
            if node.is_leaf() {
                node.num_entries(at)?;
                return Ok(node.entry(0).id);
            }
            at = node.child(0);
        }
        Err(DatError::DirectoryLoop(at))
    }

    /// The greatest id of the subtree at `offset`.
    fn find_max(&mut self, offset: u32) -> Result<u32, DatError> {
        let mut at = offset;
        for _ in 0..MAX_DEPTH {
            let node = self.read_node(at)?;
            let n = node.num_entries(at)?;
            if node.is_leaf() {
                return Ok(node.entry(n.saturating_sub(1)).id);
            }
            at = node.child(n);
        }
        Err(DatError::DirectoryLoop(at))
    }

    /// Free the record's chain, shift the entry out, write the
    /// node.
    fn delete_leaf(&mut self, node_offset: u32, index: usize) -> Result<(), DatError> {
        let mut node = self.read_node(node_offset)?;
        let n = node.num_entries(node_offset)?;
        if index >= n {
            return Err(DatError::EntryIndexOutOfRange {
                offset: node_offset,
                index,
                entries: n,
            });
        }
        let entry = node.entry(index);
        // Native passes allow-free `true` (it may be replaying a journal record); with no replay
        // path this writer passes `false`, so a chain that is somehow already free is reported
        // rather than spliced into the free list twice. Same reasoning as `replace_entry`.
        self.delete_blocks(entry.offset, false)?;
        self.fire(Fault::AfterRecordFreed)?;
        extract_entry_shift(&mut node, index, n);
        self.write_node(node_offset, &node)?;
        Ok(())
    }

    /// Free the target's chain, copy the donor entry over
    /// the target, then shift the donor entry out of its own (leaf) node.
    fn delete_internal(
        &mut self,
        target_offset: u32,
        target_index: usize,
        donor_offset: u32,
        donor_index: usize,
    ) -> Result<(), DatError> {
        let mut target = self.read_node(target_offset)?;
        let target_n = target.num_entries(target_offset)?;
        if target_index >= target_n {
            return Err(DatError::EntryIndexOutOfRange {
                offset: target_offset,
                index: target_index,
                entries: target_n,
            });
        }
        let donor_node = self.read_node(donor_offset)?;
        let donor_n = donor_node.num_entries(donor_offset)?;
        if donor_index >= donor_n {
            return Err(DatError::EntryIndexOutOfRange {
                offset: donor_offset,
                index: donor_index,
                entries: donor_n,
            });
        }

        self.delete_blocks(target.entry(target_index).offset, false)?;
        self.fire(Fault::AfterRecordFreed)?;
        target.set_entry(target_index, donor_node.entry(donor_index));
        self.write_node(target_offset, &target)?;

        let mut donor = self.read_node(donor_offset)?;
        extract_entry_shift(&mut donor, donor_index, donor_n);
        self.write_node(donor_offset, &donor)?;
        Ok(())
    }

    /// Move one entry from a sibling up into the parent and
    /// the parent's separator down into the child that is at the minimum.
    ///
    /// `from_left` is native's left-sibling flag. The separator is entry `index - 1` for a left
    /// sibling and entry `index` for a right one, and the child link that travels with the borrowed
    /// entry is the sibling's outermost one -- its last child link on the left, its first on the
    /// right.
    fn rotate_entry(
        &mut self,
        parent_offset: u32,
        index: usize,
        from_left: bool,
    ) -> Result<(), DatError> {
        let mut parent = self.read_node(parent_offset)?;
        let parent_n = parent.num_entries(parent_offset)?;
        if index > parent_n || (from_left && index == 0) || (!from_left && index >= parent_n) {
            return Err(DatError::EntryIndexOutOfRange {
                offset: parent_offset,
                index,
                entries: parent_n,
            });
        }
        let node_offset = parent.child(index);
        let sibling_offset = parent.child(if from_left { index - 1 } else { index + 1 });
        let mut sibling = self.read_node(sibling_offset)?;
        let sibling_n = sibling.num_entries(sibling_offset)?;
        let mut node = self.read_node(node_offset)?;
        let node_n = node.num_entries(node_offset)?;
        if sibling_n == 0 {
            return Err(DatError::EntryIndexOutOfRange {
                offset: sibling_offset,
                index: 0,
                entries: 0,
            });
        }

        let (moved, link) = if from_left {
            let link = sibling.child(sibling_n);
            sibling.set_num_entries(sibling_n - 1);
            (sibling.entry(sibling_n - 1), link)
        } else {
            // Removing the least entry: the "is this internal" test reads the first child link
            // before the links are shifted, which is why it is taken here and not after.
            let moved = sibling.entry(0);
            let link = sibling.child(0);
            let internal = link != 0;
            sibling.set_num_entries(sibling_n - 1);
            for i in 0..sibling_n - 1 {
                sibling.move_entry(i, i + 1);
            }
            if internal {
                for i in 0..sibling_n {
                    sibling.set_child(i, sibling.child(i + 1));
                }
            }
            (moved, link)
        };

        let separator_index = if from_left { index - 1 } else { index };
        let separator = parent.entry(separator_index);
        parent.set_entry(separator_index, moved);

        if from_left {
            // Adding a least entry.
            let internal = node.child(0) != 0;
            for i in (0..node_n).rev() {
                node.move_entry(i + 1, i);
            }
            node.set_entry(0, separator);
            node.set_num_entries(node_n + 1);
            if internal {
                for i in (0..=node_n).rev() {
                    node.set_child(i + 1, node.child(i));
                }
                node.set_child(0, link);
            }
        } else {
            node.set_num_entries(node_n + 1);
            node.set_entry(node_n, separator);
            node.set_child(node_n + 1, link);
        }

        // Native's write order: the child, then the parent, then the sibling.
        self.write_node(node_offset, &node)?;
        self.write_node(parent_offset, &parent)?;
        self.write_node(sibling_offset, &sibling)?;
        Ok(())
    }

    /// Merge two nodes: the parent's separator at `index` moves down between
    /// `left` and `right`, `right`'s entries and child links follow it, and `right`'s own blocks
    /// go back to the free list.
    ///
    /// When the parent was the root and the extracted separator was its last entry, the merged
    /// node *becomes* the root: the header's B-tree root is repointed, the header is written, and
    /// the old root block is freed. Returns the merged node's offset, which is where the caller
    /// continues.
    fn merge_nodes(
        &mut self,
        parent_offset: u32,
        left_offset: u32,
        right_offset: u32,
        index: usize,
    ) -> Result<u32, DatError> {
        let mut parent = self.read_node(parent_offset)?;
        let parent_n = parent.num_entries(parent_offset)?;
        if index >= parent_n {
            return Err(DatError::EntryIndexOutOfRange {
                offset: parent_offset,
                index,
                entries: parent_n,
            });
        }
        let mut left = self.read_node(left_offset)?;
        let left_n = left.num_entries(left_offset)?;
        let right = self.read_node(right_offset)?;
        let right_n = right.num_entries(right_offset)?;
        // Native hardcodes the destination indices (entry 0x1e, child link 0x1f), which is
        // only correct because both nodes are always at the minimum here. This computes them, and
        // refuses the case native would have corrupted.
        if left_n + right_n + 1 > MAX_ENTRIES {
            return Err(DatError::MergeOverflow {
                left: left_n,
                right: right_n,
            });
        }

        let separator = extract_entry_shift(&mut parent, index, parent_n);
        left.set_child(left_n + 1, 0); // The new right child starts null.
        left.set_entry(left_n, separator);
        for i in 0..right_n {
            left.set_entry(left_n + 1 + i, right.entry(i));
        }
        for i in 0..=right_n {
            left.set_child(left_n + 1 + i, right.child(i));
        }
        left.set_num_entries(left_n + 1 + right_n);
        self.write_node(left_offset, &left)?;
        self.delete_blocks(right_offset, false)?;

        if parent_offset == self.header.btree_root && parent_n == 1 {
            self.header.btree_root = left_offset;
            self.save_file_info()?;
            self.delete_blocks(parent_offset, false)?;
        } else {
            self.write_node(parent_offset, &parent)?;
        }
        Ok(left_offset)
    }
}

/// The node's binary search, with the "which child would hold it" answer
/// the miss case needs.
///
/// Native reads the entry at the found index after the loop even when the node is empty, which can only happen
/// for a transiently empty root; this answers child 0 for that case instead.
fn has_entry(node: &RawNode, n: usize, id: u32) -> (bool, usize) {
    if n == 0 {
        return (false, 0);
    }
    let mut lo: i64 = 0;
    let mut hi: i64 = n as i64 - 1;
    let mut mid: i64 = 0;
    let mut found = false;
    while lo <= hi {
        mid = (hi + lo) / 2;
        let key = node
            .entry(usize::try_from(mid).expect("the search stays within 0..n"))
            .id;
        if key == id {
            found = true;
            break;
        } else if id < key {
            hi = mid - 1;
        } else {
            lo = mid + 1;
        }
    }
    let mut index = usize::try_from(mid).expect("the search stays within 0..n");
    if !found && node.entry(index).id < id {
        index += 1;
    }
    (found, index)
}

/// Take entry `index` out, shift the entries above it
/// down, and -- for an internal node -- drop the child link at `index + 1` with them.
///
/// The link that survives is the one at `index`, which is what makes this the right primitive for
/// a merge: the removed separator's right subtree has just been absorbed into its left one.
fn extract_entry_shift(node: &mut RawNode, index: usize, n: usize) -> BtEntry {
    let out = node.entry(index);
    let remaining = n - 1;
    node.set_num_entries(remaining);
    for i in index..remaining {
        node.move_entry(i, i + 1);
    }
    if node.child(0) != 0 {
        for i in (index + 1)..=remaining {
            node.set_child(i, node.child(i + 1));
        }
    }
    out
}

/// Count one visit to the block at `block` in [`DatWriter::audit`]'s tally.
fn mark_block(block: u32, block_size: u32, seen: &mut [u8], dupes: &mut Vec<u32>) {
    let i = ((block - FIRST_BLOCK) / block_size) as usize;
    if i < seen.len() {
        seen[i] = seen[i].saturating_add(1);
        if seen[i] > 1 {
            dupes.push(block);
        }
    }
}

/// The file creation, free-list half.
fn self_create_free_list(w: &mut DatWriter, block_size: u32, blocks: u32) -> Result<(), DatError> {
    w.header.free_count = blocks;
    w.header.free_head = FIRST_BLOCK;
    let mut block = vec![0u8; block_size as usize];
    let mut cur = FIRST_BLOCK;
    let mut last = FIRST_BLOCK;
    for _ in 0..blocks {
        last = cur;
        cur = last + block_size;
        block[..4].copy_from_slice(&(cur | FREE_BIT).to_le_bytes());
        w.write_at(u64::from(last), &block)?;
    }
    w.header.free_tail = last;
    w.write_at(u64::from(last), &FREE_BIT.to_le_bytes())?;
    w.header.file_size = cur;
    Ok(())
}

fn blank_header() -> DiskFileInfo {
    DiskFileInfo {
        magic: 0x5442,
        block_size: 8,
        file_size: 0,
        data_set: 0,
        data_subset: 0,
        free_head: 0,
        free_tail: 0,
        free_count: 0,
        btree_root: 0,
        young_lru: 0,
        old_lru: 0,
        use_lru: false,
        master_map_id: 0,
        eng_pack_vnum: 0,
        game_pack_vnum: 0,
        version_major: [0; 16],
        version_minor: 0,
    }
}

/// The 0x50 header bytes a freshly created file starts with: the retail version stamp, the caller's
/// geometry, and a free list `create` fills in immediately afterwards.
fn retail_shaped_header(
    block_size: u32,
    data_set: u32,
    data_subset: u32,
    file_size: u32,
) -> [u8; 0x50] {
    /// The major version id as it appears on disk in all four retail dats.
    const VERSION_MAJOR: [u8; 16] = [
        0xD2, 0xD7, 0xA7, 0x34, 0x2F, 0x72, 0x46, 0x4C, 0x8A, 0xB4, 0xEF, 0x51, 0x4F, 0x85, 0x6F,
        0xFD,
    ];
    let mut h = [0u8; 0x50];
    let mut put = |off: usize, v: u32| h[off..off + 4].copy_from_slice(&v.to_le_bytes());
    put(0x00, 0x5442);
    put(0x04, block_size);
    put(0x08, file_size);
    put(0x0C, data_set);
    put(0x10, data_subset);
    put(0x34, 110);
    h[0x3C..0x4C].copy_from_slice(&VERSION_MAJOR);
    h[0x4C..0x50].copy_from_slice(&0x1A01u32.to_le_bytes());
    h
}

/// Current real time in the form stored in each entry's date field.
fn now_unix() -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as u32)
}
