//! One retail `.dat` file: the header, the block chains, and the directory walk.
//!
//! The client's container open, block-chain load and directory search. See
//! `docs/formats/01-dat-container.md`.
//!
//! The file is opened read-only and read with positional reads (`pread` on Unix, an offset
//! `ReadFile` on Windows) rather than memory-mapped: the crate is `#![forbid(unsafe_code)]` and a
//! mapping would need either `unsafe` or a dependency that buys nothing here — the whole
//! directory is walked once at open and payload reads are one read per block, which is what the
//! client does too. A positional read carries its own offset, so readers on many threads share
//! one handle without a lock, and no `Mutex<File>` is needed to make `seek`+`read` atomic.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use dereth_primitives::DataId;

use crate::btree::{BtEntry, BtNode, NODE_SIZE, PRE_TOD_NODE_SIZE};
use crate::error::DatError;

/// Where a container's bytes are kept: a positional read that fills `buf` from `offset` or fails.
///
/// A file on disk is the usual storage. A platform with no file system the client can open by path
/// (a browser, which keeps the player's files in its own storage) supplies its own, and
/// [`DatFile::from_storage`] walks it exactly as [`DatFile::open`] walks a file.
pub trait DatStorage: std::fmt::Debug + Send + Sync {
    /// Fill all of `buf` from `offset`; end of data before `buf` is full is `UnexpectedEof`.
    ///
    /// # Errors
    /// Whatever the underlying storage reports.
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()>;
}

impl DatStorage for File {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        read_exact_at(self, offset, buf)
    }
}

/// A container held whole in memory.
impl DatStorage for Vec<u8> {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        let start = usize::try_from(offset).map_err(|_| std::io::ErrorKind::UnexpectedEof)?;
        let end = start
            .checked_add(buf.len())
            .filter(|&end| end <= self.len())
            .ok_or(std::io::ErrorKind::UnexpectedEof)?;
        buf.copy_from_slice(&self[start..end]);
        Ok(())
    }
}

/// A platform with neither positional-read call has no file the client can open by path; its
/// containers come through [`DatFile::from_storage`].
#[cfg(not(any(unix, windows)))]
fn read_exact_at(_file: &File, _offset: u64, _buf: &mut [u8]) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// Fill `buf` from `offset` without moving a shared cursor. A short read is retried from where it
/// stopped; end of file before `buf` is full is `UnexpectedEof`, as `read_exact` reports it.
#[cfg(unix)]
fn read_exact_at(file: &File, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, buf, offset)
}

/// Fill `buf` from `offset` without a lock. `seek_read` moves the handle's cursor as a side
/// effect, which no read here depends on: every read names its own offset.
#[cfg(windows)]
fn read_exact_at(file: &File, mut offset: u64, mut buf: &mut [u8]) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        match file.seek_read(buf, offset) {
            Ok(0) => return Err(std::io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => {
                buf = &mut buf[n..];
                offset += n as u64;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// The container header, `0x50` bytes at file offset `0x140`.
/// See `docs/formats/01-dat-container.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiskFileInfo {
    /// The magic; must be `0x5442`.
    pub magic: u32,
    /// The block size, including the 4-byte link. `0x400` portal/local/highres, `0x100` cell.
    pub block_size: u32,
    /// The file size in bytes.
    pub file_size: u32,
    /// The data set: 1 `PORTAL_DATFILE`, 2 `CELL_DATFILE`, 3 `LOCAL_DATFILE`.
    pub data_set: u32,
    /// The data subset: region / language id; `0x69466948` (`"HiFi"`) identifies the high-res dat.
    pub data_subset: u32,
    /// The first block of the free chain.
    pub free_head: u32,
    /// The last block of the free chain.
    pub free_tail: u32,
    /// The number of free blocks.
    pub free_count: u32,
    /// The block of the directory's root node.
    pub btree_root: u32,
    /// The young end of the header's LRU list.
    pub young_lru: u32,
    /// The old end of the header's LRU list.
    pub old_lru: u32,
    /// Whether the LRU list is in use. 0 in all four retail files.
    pub use_lru: bool,
    /// The master map id. Portal: `0x25000000`; 0 elsewhere.
    pub master_map_id: u32,
    /// The engine pack version. 110 portal/local/highres, 22 cell.
    pub eng_pack_vnum: i32,
    /// The game pack version. 0 in all four.
    pub game_pack_vnum: i32,
    /// The major version id, a raw 16-byte GUID.
    pub version_major: [u8; 16],
    /// The minor version id. `0x1A01` in all four.
    pub version_minor: u32,
}

/// `DATFILE_TYPE`, the header's data set.
pub const PORTAL_DATFILE: u32 = 1;
/// `CELL_DATFILE`.
pub const CELL_DATFILE: u32 = 2;
/// `LOCAL_DATFILE`.
pub const LOCAL_DATFILE: u32 = 3;
/// The high-res dat's data subset, `"HiFi"`.
pub const HIRES_SUBSET: u32 = 0x6946_6948;

pub(crate) const HEADER_OFFSET: u64 = 0x140;
/// Where the header of a file from before Throne of Destiny starts: 44 bytes at `0x12C`.
pub(crate) const PRE_TOD_HEADER_OFFSET: u64 = 0x12C;

/// Which of the two container layouts a file uses. It is the primitives' era, shared with every
/// reader of records, since the layout of a few record types changed at the same time. Blocks,
/// chains and the order-62 directory are the same in both.
pub use dereth_primitives::ContainerEra;

/// Read the pre-Throne-of-Destiny header: magic, block size, file size, iteration, the free chain's
/// head, tail and count, and the directory root, then three words that are zero in every shipped
/// file. The file names no data set; its block size tells them apart (`0x400` portal, `0x100`
/// cell), so the header's `data_set` is inferred from it and every field the layout lacks is zero.
fn parse_pre_tod_header(b: &[u8; 0x2C]) -> Result<(DiskFileInfo, u32), DatError> {
    let w = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let magic = w(0x00);
    if magic != 0x5442 {
        return Err(DatError::BadMagic(magic));
    }
    let block_size = w(0x04);
    if block_size < 8 || block_size % 4 != 0 {
        return Err(DatError::BadBlockSize(block_size));
    }
    let data_set = if block_size == 0x100 {
        CELL_DATFILE
    } else {
        PORTAL_DATFILE
    };
    let header = DiskFileInfo {
        magic,
        block_size,
        file_size: w(0x08),
        data_set,
        data_subset: 0,
        free_head: w(0x10),
        free_tail: w(0x14),
        free_count: w(0x18),
        btree_root: w(0x1C),
        young_lru: 0,
        old_lru: 0,
        use_lru: false,
        master_map_id: 0,
        eng_pack_vnum: 0,
        game_pack_vnum: 0,
        version_major: [0; 16],
        version_minor: 0,
    };
    Ok((header, w(0x0C)))
}
pub(crate) const FIRST_BLOCK: u32 = 0x400;
/// The 64-byte transaction journal slot.
pub(crate) const TRANSACTION_OFFSET: u64 = 0x100;

impl DiskFileInfo {
    pub(crate) fn parse(b: &[u8; 0x50]) -> Result<Self, DatError> {
        let w = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let magic = w(0x00);
        if magic != 0x5442 {
            return Err(DatError::BadMagic(magic));
        }
        let block_size = w(0x04);
        if block_size < 8 || block_size % 4 != 0 {
            return Err(DatError::BadBlockSize(block_size));
        }
        let mut version_major = [0u8; 16];
        version_major.copy_from_slice(&b[0x3C..0x4C]);
        Ok(Self {
            magic,
            block_size,
            file_size: w(0x08),
            data_set: w(0x0C),
            data_subset: w(0x10),
            free_head: w(0x14),
            free_tail: w(0x18),
            free_count: w(0x1C),
            btree_root: w(0x20),
            young_lru: w(0x24),
            old_lru: w(0x28),
            use_lru: b[0x2C] != 0,
            master_map_id: w(0x30),
            #[allow(clippy::cast_possible_wrap)]
            eng_pack_vnum: w(0x34) as i32,
            #[allow(clippy::cast_possible_wrap)]
            game_pack_vnum: w(0x38) as i32,
            version_major,
            version_minor: w(0x4C),
        })
    }

    /// `(data_set << 32) | data_subset`.
    #[must_use]
    pub fn dat_file_id(&self) -> u64 {
        (u64::from(self.data_set) << 32) | u64::from(self.data_subset)
    }

    /// Payload bytes per block: the block size less the 4-byte link.
    #[must_use]
    pub fn payload_per_block(&self) -> usize {
        self.block_size as usize - 4
    }
}

/// What `verify_structure` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureReport {
    /// Number of directory entries.
    pub entries: usize,
    /// Number of B-tree nodes visited.
    pub nodes: usize,
    /// How many entries appeared out of ascending order during the in-order walk. Must be zero.
    pub out_of_order: usize,
    /// The distinct depths at which leaves were found, with a count each. A healthy B-tree has one.
    pub leaf_depths: BTreeMap<usize, usize>,
    /// Highest node entry count seen.
    pub max_entries_per_node: usize,
    /// Length of the free chain walked from its first block.
    pub free_chain_len: usize,
    /// The header's free-block count, for comparison.
    pub header_free_count: u32,
    /// `(file_size - 0x400) / block_size`.
    pub total_data_blocks: usize,
    /// Blocks accounted for by files plus directory nodes.
    pub blocks_used: usize,
}

impl StructureReport {
    /// Every invariant `verify_structure` checks, in one place.
    #[must_use]
    pub fn is_sound(&self) -> bool {
        self.out_of_order == 0
            && self.leaf_depths.len() == 1
            && self.max_entries_per_node <= 61
            && self.free_chain_len == self.header_free_count as usize
    }
}

/// One retail `.dat` file, opened read-only.
#[derive(Debug)]
pub struct DatFile {
    path: PathBuf,
    storage: Box<dyn DatStorage>,
    header: DiskFileInfo,
    /// The whole directory, ascending by id. The B-tree in-order walk and ascending id order are
    /// the same thing, which `verify_structure` checks.
    dir: BTreeMap<u32, BtEntry>,
    node_offsets: Vec<u32>,
    era: ContainerEra,
    header_iteration: Option<u32>,
}

impl DatFile {
    /// Open and walk the directory. The walk costs one read per node (2 blocks in the 0x400-block
    /// files, 7 in the cell dat) and is what makes every later lookup a `BTreeMap` hit.
    pub fn open(path: &Path) -> Result<Self, DatError> {
        Self::from_storage(path.to_path_buf(), Box::new(File::open(path)?))
    }

    /// Open a container whose bytes are kept in `storage` rather than a file, and walk its
    /// directory. `path` names it in errors and is what [`DatFile::reload`] reopens.
    ///
    /// # Errors
    /// The header and directory errors [`DatFile::open`] reports, and the storage's read errors.
    pub fn from_storage(path: PathBuf, storage: Box<dyn DatStorage>) -> Result<Self, DatError> {
        let mut me = Self {
            path,
            storage,
            header: DiskFileInfo {
                magic: 0,
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
            },
            dir: BTreeMap::new(),
            node_offsets: Vec::new(),
            era: ContainerEra::Tod,
            header_iteration: None,
        };
        let mut hdr = [0u8; 0x50];
        me.read_exact_at(HEADER_OFFSET, &mut hdr)?;
        match DiskFileInfo::parse(&hdr) {
            Ok(header) => me.header = header,
            // The layout is told by where the magic is: at 0x140 the file is from Throne of
            // Destiny on; at 0x12C it is older. Anything else is refused with the 0x140 reading.
            Err(DatError::BadMagic(magic)) => {
                let mut old = [0u8; 0x2C];
                me.read_exact_at(PRE_TOD_HEADER_OFFSET, &mut old)?;
                if u32::from_le_bytes([old[0], old[1], old[2], old[3]]) != 0x5442 {
                    return Err(DatError::BadMagic(magic));
                }
                let (header, iteration) = parse_pre_tod_header(&old)?;
                me.header = header;
                me.era = ContainerEra::PreTod;
                me.header_iteration = Some(iteration);
            }
            Err(e) => return Err(e),
        }
        me.load_directory()?;
        Ok(me)
    }

    /// Which container layout the file uses.
    #[must_use]
    pub fn era(&self) -> ContainerEra {
        self.era
    }

    /// The whole file's iteration, which a file from before Throne of Destiny keeps in its header
    /// (2112 in the February 2005 portal, 1593 in its cell file). `None` from Throne of Destiny on,
    /// where the iteration is the `0xFFFF0001` list ([`DatFile::iteration_list`]).
    #[must_use]
    pub fn header_iteration(&self) -> Option<u32> {
        self.header_iteration
    }

    /// The directory node's size in this layout.
    fn node_size(&self) -> usize {
        match self.era {
            ContainerEra::PreTod => PRE_TOD_NODE_SIZE,
            ContainerEra::Tod => NODE_SIZE,
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Re-read the header and re-walk the directory, for a file a [`crate::write::DatWriter`] has
    /// changed since [`DatFile::open`].
    ///
    /// **This is the container's half of the cache invalidation.** `DatFile` caches the whole
    /// B-tree in `dir` at open, so a record replaced on disk is invisible to an already-open
    /// reader: the entry it holds still names the *old* chain, and that chain is on the free list.
    /// Reading it after a patch yields [`DatError::FreeBlockInChain`] at best and a stale record at
    /// worst.
    /// Native has the same problem in a different shape and solves it the same way, by reopening:
    /// the client restarts the language interface and re-sets the region
    /// rather than patching what is in memory.
    ///
    /// The file handle is reopened too, because a writer may have extended the file
    /// and a handle opened before that still reports the old length on
    /// some platforms.
    ///
    /// # Errors
    ///
    /// Whatever [`DatFile::open`] returns: the file may have been replaced by something that is no
    /// longer a container, in which case the old contents are **kept** and the error reported, so
    /// a failed reload degrades to a stale reader rather than to no reader at all.
    pub fn reload(&mut self) -> Result<(), DatError> {
        let fresh = Self::open(&self.path)?;
        *self = fresh;
        Ok(())
    }

    #[must_use]
    pub fn header(&self) -> &DiskFileInfo {
        &self.header
    }

    /// The 64-byte transaction journal at `0x100`. The journal only binds a writer, but a reader
    /// that wants to check the file is quiescent looks at the leading type byte and magic.
    pub fn transaction_slot(&self) -> Result<[u8; 0x40], DatError> {
        let mut b = [0u8; 0x40];
        self.read_exact_at(0x100, &mut b)?;
        Ok(b)
    }

    /// The directory entry for an id, if present.
    #[must_use]
    pub fn entry(&self, id: DataId) -> Option<&BtEntry> {
        self.dir.get(&id.raw())
    }

    #[must_use]
    pub fn contains(&self, id: DataId) -> bool {
        self.dir.contains_key(&id.raw())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.dir.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dir.is_empty()
    }

    /// Directory order == B-tree order == ascending `DataID`.
    pub fn iter_ids(&self) -> impl Iterator<Item = DataId> + '_ {
        self.dir.keys().copied().map(DataId)
    }

    /// Every entry, ascending by id.
    pub fn iter_entries(&self) -> impl Iterator<Item = (DataId, &BtEntry)> + '_ {
        self.dir.iter().map(|(k, v)| (DataId(*k), v))
    }

    /// The payload of one file, with the container's framing removed.
    ///
    /// A compressed entry would be inflated here; no retail file
    /// is stored compressed, so this reader parses the flag and refuses rather than carrying an
    /// inflate path that nothing can test.
    pub fn read(&self, id: DataId) -> Result<Vec<u8>, DatError> {
        let e = *self.dir.get(&id.raw()).ok_or(DatError::NotFound(id))?;
        if e.compressed() {
            return Err(DatError::CompressionUnsupported(id));
        }
        self.read_chain(id, e.offset, e.size as usize)
    }

    /// Each block is `[next:4][payload:blockSize-4]`; a
    /// link with bit 31 set means the walk stepped into a free block, which is a hard failure.
    fn read_chain(&self, id: DataId, mut offset: u32, size: usize) -> Result<Vec<u8>, DatError> {
        let per = self.header.payload_per_block();
        let mut out = Vec::with_capacity(size);
        let mut block = vec![0u8; self.header.block_size as usize];
        let mut remaining = size;
        while remaining > 0 && offset != 0 {
            let take = remaining.min(per);
            // One read of link + payload, exactly as SyncRead(dst - 4, n + 4, offset) does.
            self.read_exact_at(u64::from(offset), &mut block[..take + 4])?;
            let link = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
            if link & 0x8000_0000 != 0 {
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

    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> Result<(), DatError> {
        self.storage.read_exact_at(offset, buf)?;
        Ok(())
    }

    /// The `0xFFFF0001` iteration list, decoded.
    pub fn iteration_list(&self) -> Result<Vec<u32>, DatError> {
        let raw = self.read(crate::divine::ITERATION_LIST)?;
        crate::iteration::decode(&raw)
    }

    /// Read every node of the tree into `self.dir`.
    ///
    /// Iterative rather than recursive: dat walks use a work list throughout this crate.
    fn load_directory(&mut self) -> Result<(), DatError> {
        let mut stack = vec![self.header.btree_root];
        let mut nodes = Vec::new();
        while let Some(offset) = stack.pop() {
            let node = self.load_node(offset)?;
            nodes.push(offset);
            for e in &node.entries {
                self.dir.insert(e.id, *e);
            }
            if !node.is_leaf() {
                stack.extend_from_slice(&node.children[..=node.entries.len()]);
            }
        }
        self.node_offsets = nodes;
        Ok(())
    }

    fn load_node(&self, offset: u32) -> Result<BtNode, DatError> {
        if offset < FIRST_BLOCK || u64::from(offset) >= u64::from(self.header.file_size) {
            return Err(DatError::BlockOutOfRange(offset));
        }
        let raw = self.read_chain(DataId(0), offset, self.node_size())?;
        match self.era {
            ContainerEra::PreTod => BtNode::parse_pre_tod(&raw, offset),
            ContainerEra::Tod => BtNode::parse(&raw, offset),
        }
    }

    /// The actual descend-and-binary-search, kept so that the search
    /// itself can be tested against the cached directory rather than assumed.
    pub fn lookup_via_tree(&self, id: DataId) -> Result<Option<BtEntry>, DatError> {
        let mut offset = self.header.btree_root;
        let target = id.raw();
        if target == 0 {
            return Ok(None);
        }
        loop {
            let node = self.load_node(offset)?;
            if node.entries.is_empty() {
                return Ok(None);
            }
            match node.search(target) {
                Ok(i) => return Ok(Some(node.entries[i])),
                Err(i) => {
                    if node.is_leaf() {
                        return Ok(None);
                    }
                    offset = node.children[i];
                }
            }
        }
    }

    /// Structural self-check: B-tree invariants plus the free chain.
    pub fn verify_structure(&self) -> Result<StructureReport, DatError> {
        let mut out_of_order = 0usize;
        let mut leaf_depths: BTreeMap<usize, usize> = BTreeMap::new();
        let mut max_entries = 0usize;
        let mut nodes = 0usize;

        // A genuine in-order walk, so that "keys strictly increasing" is an assertion about the
        // tree and not about the `BTreeMap` the open populated.
        enum Step {
            Node(u32, usize),
            Entry(u32),
        }
        let mut last: Option<u32> = None;
        let mut stack = vec![Step::Node(self.header.btree_root, 0)];
        while let Some(step) = stack.pop() {
            match step {
                Step::Entry(id) => {
                    if last.is_some_and(|l| id <= l) {
                        out_of_order += 1;
                    }
                    last = Some(id);
                }
                Step::Node(offset, depth) => {
                    let node = self.load_node(offset)?;
                    nodes += 1;
                    max_entries = max_entries.max(node.entries.len());
                    let leaf = node.is_leaf();
                    if leaf {
                        *leaf_depths.entry(depth).or_insert(0) += 1;
                    }
                    let mut items = Vec::with_capacity(node.entries.len() * 2 + 1);
                    for (i, e) in node.entries.iter().enumerate() {
                        if !leaf {
                            items.push(Step::Node(node.children[i], depth + 1));
                        }
                        items.push(Step::Entry(e.id));
                    }
                    if !leaf {
                        items.push(Step::Node(node.children[node.entries.len()], depth + 1));
                    }
                    // Pushed in reverse so they pop in order.
                    stack.extend(items.into_iter().rev());
                }
            }
        }

        let free_chain_len = self.walk_free_chain()?;
        let per = self.header.payload_per_block();
        let blocks_used: usize = self
            .dir
            .values()
            .map(|e| (e.size as usize).div_ceil(per))
            .sum::<usize>()
            + self.node_offsets.len() * self.node_size().div_ceil(per);

        Ok(StructureReport {
            entries: self.dir.len(),
            nodes,
            out_of_order,
            leaf_depths,
            max_entries_per_node: max_entries,
            free_chain_len,
            header_free_count: self.header.free_count,
            total_data_blocks: (self.header.file_size as usize - FIRST_BLOCK as usize)
                / self.header.block_size as usize,
            blocks_used,
        })
    }

    /// Walk the free chain from `free_head` to `free_tail`, every link carrying bit 31.
    fn walk_free_chain(&self) -> Result<usize, DatError> {
        let mut offset = self.header.free_head;
        let mut n = 0usize;
        let mut link_buf = [0u8; 4];
        while offset != 0 {
            self.read_exact_at(u64::from(offset), &mut link_buf)?;
            let link = u32::from_le_bytes(link_buf);
            if link & 0x8000_0000 == 0 && link != 0 {
                return Err(DatError::BlockOutOfRange(offset));
            }
            offset = link & 0x7FFF_FFFF;
            n += 1;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write::DatWriter;

    /// A container read through a [`DatStorage`] other than a file walks and reads exactly as the
    /// same bytes opened from disk.
    #[test]
    fn a_container_held_in_memory_reads_like_the_file_it_came_from() {
        let dir = std::env::temp_dir().join(format!("dereth-dat-storage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("memory.dat");
        let id = DataId(0x0600_0001);
        {
            let mut w = DatWriter::create(&path, 0x400, 1, 0, 0x400 + 0x400 * 16).expect("create");
            w.save(id, b"payload across the chain", 1, 1, 1)
                .expect("save");
        }
        let bytes = std::fs::read(&path).expect("read back");
        let from_disk = DatFile::open(&path).expect("open file");
        let in_memory = DatFile::from_storage(path.clone(), Box::new(bytes)).expect("open memory");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(in_memory.header(), from_disk.header());
        assert_eq!(in_memory.len(), from_disk.len());
        assert_eq!(
            in_memory.read(id).expect("read"),
            b"payload across the chain"
        );
        assert_eq!(
            in_memory.read(id).expect("read"),
            from_disk.read(id).expect("read")
        );
    }

    /// Storage that ends before the header does is an I/O error, not a panic.
    #[test]
    fn a_short_storage_is_an_error() {
        let err = DatFile::from_storage(PathBuf::from("short"), Box::new(vec![0u8; 0x100]));
        assert!(err.is_err());
    }
}
