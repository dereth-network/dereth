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
use std::sync::{Arc, OnceLock};

use dereth_primitives::DataId;

use crate::btree::{BtEntry, BtNode};
use crate::error::DatError;
use crate::layout::{ContainerLayout, HeaderLayout};

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
        read_slice_at(self, offset, buf)
    }
}

/// A container built into the program.
impl DatStorage for &'static [u8] {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        read_slice_at(self, offset, buf)
    }
}

fn read_slice_at(bytes: &[u8], offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
    let start = usize::try_from(offset).map_err(|_| std::io::ErrorKind::UnexpectedEof)?;
    let end = start
        .checked_add(buf.len())
        .filter(|&end| end <= bytes.len())
        .ok_or(std::io::ErrorKind::UnexpectedEof)?;
    buf.copy_from_slice(&bytes[start..end]);
    Ok(())
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

/// The container header: `0x50` bytes at file offset `0x140` in the `client_*.dat` files, and
/// where a [`ContainerLayout`] puts each field in any other layout, a field the layout lacks
/// reading as zero. See `docs/formats/01-dat-container.md`.
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

/// Which of the two container layouts a file uses. It is the primitives' era, shared with every
/// reader of records, since the layout of a few record types changed at the same time. Blocks,
/// chains and the order-62 directory are the same in both.
pub use dereth_primitives::ContainerEra;

pub(crate) const FIRST_BLOCK: u32 = 0x400;

impl DiskFileInfo {
    /// Read the header's `layout.bytes` bytes as `layout` places its fields, and the whole file's
    /// iteration when the layout keeps it in the header.
    ///
    /// A layout with no data-set field names its files by block size: `0x100` is the cell file and
    /// anything else the portal file.
    ///
    /// # Panics
    /// When `b` is shorter than the layout's header, or the layout does not
    /// [fit](ContainerLayout::fits).
    pub(crate) fn parse_as(
        b: &[u8],
        layout: &HeaderLayout,
    ) -> Result<(Self, Option<u32>), DatError> {
        let w = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let opt = |at: Option<usize>| at.map_or(0, w);
        let magic = w(0x00);
        if magic != 0x5442 {
            return Err(DatError::BadMagic(magic));
        }
        let block_size = w(0x04);
        if block_size < 8 || block_size % 4 != 0 {
            return Err(DatError::BadBlockSize(block_size));
        }
        let mut version_major = [0u8; 16];
        if let Some(at) = layout.version_major {
            version_major.copy_from_slice(&b[at..at + 16]);
        }
        let data_set = match layout.data_set {
            Some(at) => w(at),
            None if block_size == 0x100 => CELL_DATFILE,
            None => PORTAL_DATFILE,
        };
        let header = Self {
            magic,
            block_size,
            file_size: w(layout.file_size),
            data_set,
            data_subset: opt(layout.data_subset),
            free_head: w(layout.free_head),
            free_tail: w(layout.free_tail),
            free_count: w(layout.free_count),
            btree_root: w(layout.btree_root),
            young_lru: opt(layout.young_lru),
            old_lru: opt(layout.old_lru),
            use_lru: layout.use_lru.is_some_and(|at| b[at] != 0),
            master_map_id: opt(layout.master_map_id),
            #[allow(clippy::cast_possible_wrap)]
            eng_pack_vnum: opt(layout.eng_pack_vnum) as i32,
            #[allow(clippy::cast_possible_wrap)]
            game_pack_vnum: opt(layout.game_pack_vnum) as i32,
            version_major,
            version_minor: opt(layout.version_minor),
        };
        Ok((header, layout.iteration.map(w)))
    }

    /// Write every field `layout` has into `out`, the header's `layout.bytes` bytes, leaving the
    /// bytes no field covers as they are. The inverse of [`DiskFileInfo::parse_as`], but for the
    /// header's iteration, which is not a field of this struct.
    ///
    /// # Panics
    /// When `out` is shorter than the layout's header, or the layout does not
    /// [fit](ContainerLayout::fits).
    pub(crate) fn write_as(&self, out: &mut [u8], layout: &HeaderLayout) {
        let mut put = |at: Option<usize>, v: u32| {
            if let Some(at) = at {
                out[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        };
        put(Some(0x00), self.magic);
        put(Some(0x04), self.block_size);
        put(Some(layout.file_size), self.file_size);
        put(layout.data_set, self.data_set);
        put(layout.data_subset, self.data_subset);
        put(Some(layout.free_head), self.free_head);
        put(Some(layout.free_tail), self.free_tail);
        put(Some(layout.free_count), self.free_count);
        put(Some(layout.btree_root), self.btree_root);
        put(layout.young_lru, self.young_lru);
        put(layout.old_lru, self.old_lru);
        put(layout.master_map_id, self.master_map_id);
        #[allow(clippy::cast_sign_loss)]
        put(layout.eng_pack_vnum, self.eng_pack_vnum as u32);
        #[allow(clippy::cast_sign_loss)]
        put(layout.game_pack_vnum, self.game_pack_vnum as u32);
        put(layout.version_minor, self.version_minor);
        if let Some(at) = layout.use_lru {
            out[at] = u8::from(self.use_lru);
        }
        if let Some(at) = layout.version_major {
            out[at..at + 16].copy_from_slice(&self.version_major);
        }
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

/// What [`DatFile::read_iterations`] reads: which file a dat is, from its header, and its
/// iterations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileIterations {
    /// Which dat set's records the file holds: its layout's [`ContainerLayout::era`].
    pub era: ContainerEra,
    /// The data set: [`PORTAL_DATFILE`], [`CELL_DATFILE`] or [`LOCAL_DATFILE`].
    pub data_set: u32,
    /// The subset: the cell file's region, the local file's language, the high-resolution
    /// portal's `HiFi` stamp; 0 for the portal file and for every file from before Throne of
    /// Destiny.
    pub data_subset: u32,
    /// How many iterations the file holds: the number a server compares.
    pub count: u32,
    /// The highest iteration present. Equal to `count` unless the list has gaps, which no retail
    /// file has.
    pub highest: u32,
}

/// One retail `.dat` file, opened read-only.
///
/// A file may carry a world's overlay ([`DatFile::layered`]): every read of a record then asks the
/// overlay first. A record the overlay holds is the overlay's (an addition or a replacement), a
/// record its tombstones cover is not there, and every other record is the file's own. The
/// container's structure (its header, its tree, its free chain) is always the file's own.
///
/// It may carry the client's own records too ([`DatFile::with_client_layer`]), beneath the
/// world's overlay and above the file: a record the overlay neither holds nor deletes is the
/// client's when the client layer holds it. The client's records are never in the file's
/// iterations, and [`DatFile::base`] leaves them out.
#[derive(Debug, Clone)]
pub struct DatFile {
    path: PathBuf,
    storage: Arc<dyn DatStorage>,
    header: DiskFileInfo,
    /// The whole directory, ascending by id. The B-tree in-order walk and ascending id order are
    /// the same thing, which `verify_structure` checks.
    dir: Arc<BTreeMap<u32, BtEntry>>,
    node_offsets: Arc<Vec<u32>>,
    layout: ContainerLayout,
    header_iteration: Option<u32>,
    /// The world's overlay over this file, when it has one.
    layer: Option<Arc<crate::overlay::Layer>>,
    /// The client's own records beneath the overlay, when it has them.
    client: Option<Arc<crate::overlay::Layer>>,
    /// The file's identity as an overlay names its base, worked out once for this open of the
    /// file and shared by every view of it.
    identity: Arc<FileIdentity>,
}

/// What names one open file as an overlay's base: its layout ([`crate::overlay::fingerprint`]) and
/// its records ([`crate::overlay::records_digest`]), each worked out when first asked for.
#[derive(Debug, Default)]
pub(crate) struct FileIdentity {
    pub(crate) layout: OnceLock<[u8; 32]>,
    pub(crate) records: OnceLock<[u8; 32]>,
}

impl DatFile {
    /// Open and walk the directory. The walk costs one read per node (2 blocks in the 0x400-block
    /// files, 7 in the cell dat) and is what makes every later lookup a `BTreeMap` hit.
    ///
    /// The file's layout is one of the two built in, told by where the magic is: at `0x140` the
    /// file is from Throne of Destiny on ([`ContainerLayout::MODERN`]); at `0x12C` it is older
    /// ([`ContainerLayout::CLASSIC`]). A container in any other layout opens with
    /// [`DatFile::open_as`].
    pub fn open(path: &Path) -> Result<Self, DatError> {
        Self::from_storage(path.to_path_buf(), Box::new(File::open(path)?))
    }

    /// Open a container in `layout`, and walk its directory.
    ///
    /// # Errors
    /// [`DatError::BadLayout`] for a layout that does not [fit](ContainerLayout::fits), and the
    /// header and directory errors [`DatFile::open`] reports.
    pub fn open_as(path: &Path, layout: &ContainerLayout) -> Result<Self, DatError> {
        Self::from_storage_as(path.to_path_buf(), Box::new(File::open(path)?), layout)
    }

    /// Open a container whose bytes are kept in `storage` rather than a file, and walk its
    /// directory. `path` names it in errors and is what [`DatFile::reload`] reopens.
    ///
    /// # Errors
    /// The header and directory errors [`DatFile::open`] reports, and the storage's read errors.
    pub fn from_storage(path: PathBuf, storage: Box<dyn DatStorage>) -> Result<Self, DatError> {
        let mut me = Self::header_only(path, storage, None)?;
        me.load_directory()?;
        Ok(me)
    }

    /// [`DatFile::from_storage`] for a container in `layout`.
    ///
    /// # Errors
    /// As [`DatFile::open_as`], with the storage's read errors.
    pub fn from_storage_as(
        path: PathBuf,
        storage: Box<dyn DatStorage>,
        layout: &ContainerLayout,
    ) -> Result<Self, DatError> {
        let mut me = Self::header_only(path, storage, Some(layout))?;
        me.load_directory()?;
        Ok(me)
    }

    /// The container with its header read and its directory not walked: what
    /// [`DatFile::read_iterations`] needs, and the first half of [`DatFile::from_storage`]. With
    /// no `layout`, the layout is told by where the magic is ([`DatFile::open`]).
    fn header_only(
        path: PathBuf,
        storage: Box<dyn DatStorage>,
        layout: Option<&ContainerLayout>,
    ) -> Result<Self, DatError> {
        let mut me = Self {
            path,
            storage: Arc::from(storage),
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
            dir: Arc::new(BTreeMap::new()),
            node_offsets: Arc::new(Vec::new()),
            layout: ContainerLayout::MODERN,
            header_iteration: None,
            layer: None,
            client: None,
            identity: Arc::default(),
        };
        if let Some(layout) = layout {
            me.read_header(layout)?;
            return Ok(me);
        }
        match me.read_header(&ContainerLayout::MODERN) {
            Ok(()) => {}
            // The layout is told by where the magic is: at 0x140 the file is from Throne of
            // Destiny on; at 0x12C it is older. Anything else is refused with the 0x140 reading.
            Err(DatError::BadMagic(magic)) => match me.read_header(&ContainerLayout::CLASSIC) {
                Err(DatError::BadMagic(_)) => return Err(DatError::BadMagic(magic)),
                other => other?,
            },
            Err(e) => return Err(e),
        }
        Ok(me)
    }

    /// Read the header as `layout` places it, and take `layout` as the file's.
    fn read_header(&mut self, layout: &ContainerLayout) -> Result<(), DatError> {
        if !layout.fits() {
            return Err(DatError::BadLayout);
        }
        let mut hdr = vec![0u8; layout.header.bytes];
        self.read_exact_at(layout.header.at, &mut hdr)?;
        let (header, iteration) = DiskFileInfo::parse_as(&hdr, &layout.header)?;
        self.header = header;
        self.header_iteration = iteration;
        self.layout = *layout;
        Ok(())
    }

    /// What a file is and how many iterations it holds, read in a few small reads however large
    /// the file: the header, then (from Throne of Destiny on) one path down the directory to the
    /// `0xFFFF0001` list and that list's blocks. A file from before Throne of Destiny keeps its
    /// iteration in its header. The whole directory is never walked, so a caller may afford it
    /// before every use of the file.
    ///
    /// Every length and offset read is bounded before it is used: a truncated file, a file of
    /// something else, a looping directory or an implausible list is an error, never a panic or a
    /// large allocation.
    ///
    /// # Errors
    /// The header errors [`DatFile::open`] reports; [`DatError::NotFound`] for a file with no list;
    /// [`DatError::Overrun`] for a list larger than any real one; [`DatError::DirectoryLoop`] for a
    /// directory deeper than any real one; the chain and decoding errors of reading it.
    pub fn read_iterations(path: &Path) -> Result<FileIterations, DatError> {
        Self::iterations_of(Self::header_only(
            path.to_path_buf(),
            Box::new(File::open(path)?),
            None,
        )?)
    }

    /// [`DatFile::read_iterations`] for a container in `layout`.
    ///
    /// # Errors
    /// As [`DatFile::read_iterations`], and [`DatError::BadLayout`] for a layout that does not
    /// [fit](ContainerLayout::fits).
    pub fn read_iterations_as(
        path: &Path,
        layout: &ContainerLayout,
    ) -> Result<FileIterations, DatError> {
        Self::iterations_of(Self::header_only(
            path.to_path_buf(),
            Box::new(File::open(path)?),
            Some(layout),
        )?)
    }

    /// [`DatFile::read_iterations`] over bytes kept in `storage`.
    ///
    /// # Errors
    /// As [`DatFile::read_iterations`].
    pub fn read_iterations_from(
        path: PathBuf,
        storage: Box<dyn DatStorage>,
    ) -> Result<FileIterations, DatError> {
        Self::iterations_of(Self::header_only(path, storage, None)?)
    }

    fn iterations_of(me: Self) -> Result<FileIterations, DatError> {
        /// The list is 12 bytes in every retail file; anything this large is not one.
        const MAX_LIST: u32 = 64 * 1024;
        /// Retail's deepest directory is 3 levels; deeper than this is a loop, not a tree.
        const MAX_DEPTH: usize = 16;
        let found = |count, highest| FileIterations {
            era: me.layout.era,
            data_set: me.header.data_set,
            data_subset: me.header.data_subset,
            count,
            highest,
        };
        if let Some(n) = me.header_iteration {
            return Ok(found(n, n));
        }
        let id = crate::divine::ITERATION_LIST;
        let mut offset = me.header.btree_root;
        for _ in 0..MAX_DEPTH {
            let node = me.load_node(offset)?;
            match node.search(id.raw()) {
                Ok(i) => {
                    let e = node.entries[i];
                    if e.compressed() {
                        return Err(DatError::CompressionUnsupported(id));
                    }
                    if e.size > MAX_LIST {
                        return Err(DatError::Overrun(e.size as usize));
                    }
                    let raw = me.read_chain(id, e.offset, e.size as usize)?;
                    let (count, highest) = crate::iteration::summarize(&raw)?;
                    return Ok(found(count, highest));
                }
                Err(i) => {
                    if node.is_leaf() || node.entries.is_empty() {
                        return Err(DatError::NotFound(id));
                    }
                    offset = node.children[i];
                }
            }
        }
        Err(DatError::DirectoryLoop(offset))
    }

    /// Which dat set's records the file holds: its layout's [`ContainerLayout::era`].
    #[must_use]
    pub fn era(&self) -> ContainerEra {
        self.layout.era
    }

    /// The file's container layout.
    #[must_use]
    pub fn layout(&self) -> &ContainerLayout {
        &self.layout
    }

    /// The whole file's iteration, which a file from before Throne of Destiny keeps in its header
    /// (2112 in the February 2005 portal, 1593 in its cell file). `None` from Throne of Destiny on,
    /// where the iteration is the `0xFFFF0001` list ([`DatFile::iteration_list`]), and for a file
    /// carrying an overlay, whose iterations are the file's and the overlay's together in that list.
    #[must_use]
    pub fn header_iteration(&self) -> Option<u32> {
        if self.layer.is_some() {
            return None;
        }
        self.header_iteration
    }

    /// The file's own header iteration, whether or not an overlay is over it.
    #[must_use]
    pub fn base_header_iteration(&self) -> Option<u32> {
        self.header_iteration
    }

    /// This file with `layer` over it in place of any overlay it had: every record read asks the
    /// overlay first. `layer` must have been built over this file
    /// ([`crate::overlay::Layer::over`]). The client's records, when the file carries them, stay
    /// beneath it.
    #[must_use]
    pub fn layered(&self, layer: Arc<crate::overlay::Layer>) -> Self {
        Self {
            layer: Some(layer),
            client: self.client.clone(),
            ..self.base()
        }
    }

    /// This file with no overlay over it: the client's records, when it carries them, stay.
    #[must_use]
    pub fn without_overlay(&self) -> Self {
        Self {
            layer: None,
            ..self.clone()
        }
    }

    /// This file with the client's own records (`client`, [`crate::client_layer`]) beneath any
    /// overlay over it, in place of any it carried.
    #[must_use]
    pub fn with_client_layer(&self, client: Arc<crate::overlay::Layer>) -> Self {
        Self {
            client: Some(client),
            ..self.clone()
        }
    }

    /// The client's records beneath this file's overlay, if any.
    #[must_use]
    pub fn client_layer(&self) -> Option<&crate::overlay::Layer> {
        self.client.as_deref()
    }

    pub(crate) fn client_layer_arc(&self) -> Option<Arc<crate::overlay::Layer>> {
        self.client.clone()
    }

    /// Whether the file holds `id` as the world reads it, leaving the client's own records out:
    /// the overlay's records and deletions over the file's own.
    #[must_use]
    pub fn world_contains(&self, id: DataId) -> bool {
        if let Some(layer) = &self.layer {
            if layer.record(id).is_some() {
                return true;
            }
            if layer.hides(id) {
                return false;
            }
        }
        self.dir.contains_key(&id.raw())
    }

    /// This file as it is on disk, with no overlay over it and none of the client's records.
    #[must_use]
    pub fn base(&self) -> Self {
        Self {
            path: self.path.clone(),
            storage: Arc::clone(&self.storage),
            header: self.header,
            dir: Arc::clone(&self.dir),
            node_offsets: Arc::clone(&self.node_offsets),
            layout: self.layout,
            header_iteration: self.header_iteration,
            layer: None,
            client: None,
            identity: Arc::clone(&self.identity),
        }
    }

    /// The file's identity as an overlay names its base, kept for this open of the file.
    pub(crate) fn identity(&self) -> &FileIdentity {
        &self.identity
    }

    /// The overlay over this file, if any.
    #[must_use]
    pub fn layer(&self) -> Option<&crate::overlay::Layer> {
        self.layer.as_deref()
    }

    /// The file's own directory entry for `id`, ignoring any overlay.
    #[must_use]
    pub fn base_entry(&self, id: DataId) -> Option<&BtEntry> {
        self.dir.get(&id.raw())
    }

    /// The file's own records, under any overlay, shared.
    pub(crate) fn base_dir(&self) -> Arc<BTreeMap<u32, BtEntry>> {
        Arc::clone(&self.dir)
    }

    /// Every one of the file's own entries, ascending by id, ignoring any overlay.
    pub fn base_entries(&self) -> impl Iterator<Item = (DataId, &BtEntry)> + Send + '_ {
        self.dir.iter().map(|(k, v)| (DataId(*k), v))
    }

    /// The directory node's size in this layout.
    fn node_size(&self) -> usize {
        self.layout.node_bytes()
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
        // A file in a built-in layout is told again by where its magic is, as `open` tells it.
        let built_in = [ContainerLayout::MODERN, ContainerLayout::CLASSIC].contains(&self.layout);
        let fresh = if built_in {
            Self::open(&self.path)?
        } else {
            Self::open_as(&self.path, &self.layout)?
        };
        *self = fresh;
        Ok(())
    }

    #[must_use]
    pub fn header(&self) -> &DiskFileInfo {
        &self.header
    }

    /// The transaction slot's bytes: the 64-byte journal at `0x100` in the `client_*.dat` files,
    /// empty in a layout with no slot. The journal only binds a writer, but a reader that wants to
    /// check the file is quiescent looks at the leading type byte and magic.
    pub fn transaction_slot(&self) -> Result<Vec<u8>, DatError> {
        let Some(slot) = self.layout.transaction else {
            return Ok(Vec::new());
        };
        let mut b = vec![0u8; slot.bytes];
        self.read_exact_at(slot.at, &mut b)?;
        Ok(b)
    }

    /// The directory entry for an id, if present: the overlay's when it holds the id, none when its
    /// tombstones cover it, else the client's when it holds it, else the file's own.
    #[must_use]
    pub fn entry(&self, id: DataId) -> Option<&BtEntry> {
        if let Some(layer) = &self.layer {
            if let Some(e) = layer.record(id) {
                return Some(e);
            }
            if layer.hides(id) {
                return None;
            }
        }
        if let Some(e) = self.client.as_ref().and_then(|c| c.record(id)) {
            return Some(e);
        }
        self.dir.get(&id.raw())
    }

    #[must_use]
    pub fn contains(&self, id: DataId) -> bool {
        self.entry(id).is_some()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        let world = self.layer.as_ref().map_or(self.dir.len(), |l| l.len());
        // The client's records the world does not hold and has not deleted.
        let client = self.client.as_ref().map_or(0, |c| {
            c.records()
                .filter(|(id, _)| {
                    !self.world_contains(*id) && !self.layer.as_ref().is_some_and(|l| l.hides(*id))
                })
                .count()
        });
        world + client
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Directory order == B-tree order == ascending `DataID`.
    pub fn iter_ids(&self) -> impl Iterator<Item = DataId> + Send + '_ {
        self.iter_entries().map(|(id, _)| id)
    }

    /// Every entry, ascending by id: the overlay's merged over the client's, merged over the
    /// file's own.
    pub fn iter_entries(&self) -> impl Iterator<Item = (DataId, &BtEntry)> + Send + '_ {
        type Entries<'a> = Box<dyn Iterator<Item = (DataId, &'a BtEntry)> + Send + 'a>;
        let own = self.dir.iter().map(|(k, v)| (DataId(*k), v));
        let below: Entries<'_> = match &self.client {
            None => Box::new(own),
            Some(client) => Box::new(crate::overlay::merge(
                own.filter(move |(id, _)| client.record(*id).is_none()),
                client.records(),
            )),
        };
        match &self.layer {
            None => below,
            Some(layer) => Box::new(crate::overlay::merge(
                below.filter(move |(id, _)| !layer.hides(*id) && layer.record(*id).is_none()),
                layer.records(),
            )),
        }
    }

    /// The payload of one file, with the container's framing removed: the overlay's record when it
    /// holds the id, not found when its tombstones cover it. With an overlay the `0xFFFF0001`
    /// iteration list is the file's iterations and the overlay's together.
    ///
    /// A compressed entry would be inflated here; no retail file
    /// is stored compressed, so this reader parses the flag and refuses rather than carrying an
    /// inflate path that nothing can test.
    pub fn read(&self, id: DataId) -> Result<Vec<u8>, DatError> {
        if let Some(layer) = &self.layer {
            if id == crate::divine::ITERATION_LIST {
                return Ok(crate::iteration::encode(layer.iterations()));
            }
            if layer.record(id).is_some() {
                return layer.read(id);
            }
            if layer.hides(id) {
                layer.note_hidden();
                return Err(DatError::NotFound(id));
            }
        }
        if let Some(client) = &self.client {
            if client.record(id).is_some() {
                return client.read(id);
            }
        }
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

    /// The container's own bytes from `offset`, filling all of `buf`, whatever storage holds
    /// them: what a caller hashing the whole file reads, up to the header's file size.
    ///
    /// # Errors
    /// The storage's read errors; end of data before `buf` is full is an I/O error.
    pub fn read_raw(&self, offset: u64, buf: &mut [u8]) -> Result<(), DatError> {
        self.read_exact_at(offset, buf)
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
        let mut dir = BTreeMap::new();
        while let Some(offset) = stack.pop() {
            let node = self.load_node(offset)?;
            nodes.push(offset);
            for e in &node.entries {
                dir.insert(e.id, *e);
            }
            if !node.is_leaf() {
                stack.extend_from_slice(&node.children[..=node.entries.len()]);
            }
        }
        self.dir = Arc::new(dir);
        self.node_offsets = Arc::new(nodes);
        Ok(())
    }

    fn load_node(&self, offset: u32) -> Result<BtNode, DatError> {
        if offset < FIRST_BLOCK || u64::from(offset) >= u64::from(self.header.file_size) {
            return Err(DatError::BlockOutOfRange(offset));
        }
        let raw = self.read_chain(DataId(0), offset, self.node_size())?;
        BtNode::parse_as(&raw, offset, &self.layout.entry)
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
