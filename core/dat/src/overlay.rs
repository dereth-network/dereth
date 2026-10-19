//! A world's overlay over the locked data files: the records a world adds, replaces or deletes,
//! kept apart from the files themselves.
//!
//! The files a player installs are never written. Everything a world changes lives in an overlay
//! folder of its own, one container per file it changes:
//!
//! | file | overlay container |
//! |---|---|
//! | `client_portal.dat` or `portal.dat` | `overlay_portal.dat` |
//! | `client_cell_1.dat` or `cell.dat` | `overlay_cell.dat` |
//! | `client_local_English.dat` | `overlay_local.dat` |
//! | `client_highres.dat` | `overlay_highres.dat` |
//!
//! Each container is an ordinary dat container in the later layout, written by the same writer
//! as any other ([`crate::write::DatWriter`]). Three of its records are its own, never a world
//! record:
//!
//! - `0xFFFF0001`, the overlay's iterations: the revisions it has applied. The file's iterations
//!   as read through the overlay are the base file's and these together, or, for an overlay that
//!   carries its world's own iteration list ([`ContainerManifest::exact_iterations`]), these
//!   alone: a world made outside Dereth names its iterations as its own files do, and a client
//!   reports exactly those.
//! - `0xFFFF0002`, the tombstones ([`Tombstone`]): a record id, or a family of ids under a mask,
//!   that the world has deleted, with the revision that deleted it.
//! - `0xFFFF0003`, the manifest ([`ContainerManifest`]): which world the overlay belongs to, which
//!   base file it was made against (its name, its [`fingerprint`] and its [`records_digest`]), and
//!   the SHA-256 of every record it holds.
//!
//! A read through a file carrying its overlay ([`crate::DatFile::layered`]) asks the overlay
//! first: a record it holds is the overlay's, a record a tombstone covers is not there, and every
//! other record is the base file's. The records in an overlay are in the base file's own record
//! layout: an overlay over a file from before Throne of Destiny holds records of that time.
//!
//! An overlay opens only over the base it was made against: a different base is refused
//! ([`OverlayError::BaseMismatch`]), as is a folder holding another world's overlay
//! ([`OverlayError::OtherWorld`]) and a folder holding base files ([`OverlayError::BaseFolder`]).
//! A base is known by its records, not by where they lie in the file: the same records written in
//! another order, as two copies of one file can be, are the same base ([`is_named_base`]).
//!
//! The folder is a [`DatFolder`]: a folder on disk ([`OverlayDir::new`]), or any other storage
//! that keeps named containers ([`OverlayDir::in_folder`]), such as a browser's. The containers
//! and every read and write of them are the same in each.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dereth_primitives::DataId;
use sha2::{Digest, Sha256};

use crate::btree::BtEntry;
use crate::container::{ContainerEra, DatFile};
use crate::cursor::Cursor;
use crate::divine::ITERATION_LIST;
use crate::error::DatError;
use crate::folder::{DatFolder, DiskFolder};
use crate::locate::ModernDat;
use crate::write::{DatWriter, SaveOutcome};

/// The overlay's tombstones record.
pub const TOMBSTONES: DataId = DataId(0xFFFF_0002);
/// The overlay's manifest record.
pub const MANIFEST: DataId = DataId(0xFFFF_0003);

/// Whether `id` is one of a container's own records rather than a world record: the iteration
/// list, the tombstones or the manifest.
#[must_use]
pub fn is_reserved(id: DataId) -> bool {
    id == ITERATION_LIST || id == TOMBSTONES || id == MANIFEST
}

/// The file name of the overlay container over `target`'s file.
#[must_use]
pub const fn container_name(target: ModernDat) -> &'static str {
    match target {
        ModernDat::Portal => "overlay_portal.dat",
        ModernDat::Cell => "overlay_cell.dat",
        ModernDat::Local => "overlay_local.dat",
        ModernDat::HighRes => "overlay_highres.dat",
    }
}

/// A deletion: `id` alone when `mask` is 0, else every id whose bits under `mask` are `id`'s (a
/// whole landblock with its cells is `mask = 0xFFFF0000`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tombstone {
    pub id: u32,
    pub mask: u32,
    /// The revision that deleted it.
    pub iteration: u32,
}

impl Tombstone {
    /// Whether this deletion covers `id`.
    #[must_use]
    pub fn covers(&self, id: u32) -> bool {
        if self.mask == 0 {
            id == self.id
        } else {
            id & self.mask == self.id & self.mask
        }
    }
}

/// The tombstones record: a count, then `(id, mask, iteration)` per deletion.
#[must_use]
pub fn encode_tombstones(t: &[Tombstone]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + t.len() * 12);
    #[allow(clippy::cast_possible_truncation)]
    out.extend_from_slice(&(t.len() as u32).to_le_bytes());
    for x in t {
        out.extend_from_slice(&x.id.to_le_bytes());
        out.extend_from_slice(&x.mask.to_le_bytes());
        out.extend_from_slice(&x.iteration.to_le_bytes());
    }
    out
}

/// The tombstones record, read.
///
/// # Errors
/// A record shorter or longer than its count says.
pub fn decode_tombstones(bytes: &[u8]) -> Result<Vec<Tombstone>, DatError> {
    let mut c = Cursor::new(bytes);
    let n = c.u32()?;
    let mut out = Vec::with_capacity((n as usize).min(bytes.len() / 12));
    for _ in 0..n {
        out.push(Tombstone {
            id: c.u32()?,
            mask: c.u32()?,
            iteration: c.u32()?,
        });
    }
    c.expect_end()?;
    Ok(out)
}

/// `DOVL`, the manifest record's first dword.
const MANIFEST_MAGIC: u32 = u32::from_le_bytes(*b"DOVL");
/// The first manifest layout, which names its base by its layout alone.
const MANIFEST_VERSION_LAYOUT: u32 = 1;
/// The manifest layout that also names its base by its records.
const MANIFEST_VERSION_RECORDS: u32 = 2;
/// The manifest's flag: the iterations are the world's own list.
const FLAG_EXACT_ITERATIONS: u32 = 1;
/// The manifest's flag: the base's [`records_digest`] follows the flags.
const FLAG_BASE_RECORDS: u32 = 2;

/// What an overlay container says about itself.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ContainerManifest {
    /// The world the overlay belongs to, as the world names itself.
    pub world_key: String,
    /// The base file's name (`client_portal.dat`, `portal.dat`, ...).
    pub base_name: String,
    /// The base file's [`fingerprint`]: the layout it was last written over.
    pub base_fingerprint: [u8; 32],
    /// The base file's [`records_digest`]; `None` in a manifest written before overlays named
    /// their bases by their records, which then opens over its layout alone.
    pub base_records: Option<[u8; 32]>,
    /// How many iterations the base file carries.
    pub base_iterations: u32,
    /// Whether the overlay's iteration list is the world's whole list, reported as it is, rather
    /// than revisions over the base's.
    pub exact_iterations: bool,
    /// The SHA-256 of every record the overlay holds, by id.
    pub hashes: BTreeMap<u32, [u8; 32]>,
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    let n = u16::try_from(b.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&b[..usize::from(n)]);
}

fn get_str(c: &mut Cursor<'_>) -> Result<String, DatError> {
    let n = usize::from(c.u16()?);
    let b = c.bytes(n)?;
    Ok(String::from_utf8_lossy(b).into_owned())
}

fn get_hash(c: &mut Cursor<'_>) -> Result<[u8; 32], DatError> {
    let mut h = [0u8; 32];
    h.copy_from_slice(c.bytes(32)?);
    Ok(h)
}

impl ContainerManifest {
    /// The manifest record's bytes: the first layout when it names no records of its base, which
    /// every reader of overlays reads, and the second when it does.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(96 + self.hashes.len() * 36);
        out.extend_from_slice(&MANIFEST_MAGIC.to_le_bytes());
        let version = if self.base_records.is_some() {
            MANIFEST_VERSION_RECORDS
        } else {
            MANIFEST_VERSION_LAYOUT
        };
        out.extend_from_slice(&version.to_le_bytes());
        put_str(&mut out, &self.world_key);
        put_str(&mut out, &self.base_name);
        out.extend_from_slice(&self.base_fingerprint);
        out.extend_from_slice(&self.base_iterations.to_le_bytes());
        let mut flags = 0;
        if self.exact_iterations {
            flags |= FLAG_EXACT_ITERATIONS;
        }
        if self.base_records.is_some() {
            flags |= FLAG_BASE_RECORDS;
        }
        out.extend_from_slice(&flags.to_le_bytes());
        if let Some(records) = &self.base_records {
            out.extend_from_slice(records);
        }
        #[allow(clippy::cast_possible_truncation)]
        out.extend_from_slice(&(self.hashes.len() as u32).to_le_bytes());
        for (id, h) in &self.hashes {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(h);
        }
        out
    }

    /// The manifest record, read.
    ///
    /// # Errors
    /// Bytes that are not a manifest of this layout.
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        let mut c = Cursor::new(bytes);
        let magic = c.u32()?;
        let version = c.u32()?;
        if magic != MANIFEST_MAGIC
            || !(MANIFEST_VERSION_LAYOUT..=MANIFEST_VERSION_RECORDS).contains(&version)
        {
            return Err(DatError::NotFound(MANIFEST));
        }
        let world_key = get_str(&mut c)?;
        let base_name = get_str(&mut c)?;
        let base_fingerprint = get_hash(&mut c)?;
        let base_iterations = c.u32()?;
        let flags = c.u32()?;
        let exact_iterations = flags & FLAG_EXACT_ITERATIONS != 0;
        let base_records = if version >= MANIFEST_VERSION_RECORDS && flags & FLAG_BASE_RECORDS != 0
        {
            Some(get_hash(&mut c)?)
        } else {
            None
        };
        let n = c.u32()?;
        let mut hashes = BTreeMap::new();
        for _ in 0..n {
            let id = c.u32()?;
            hashes.insert(id, get_hash(&mut c)?);
        }
        c.expect_end()?;
        Ok(Self {
            world_key,
            base_name,
            base_fingerprint,
            base_records,
            base_iterations,
            exact_iterations,
            hashes,
        })
    }
}

/// The SHA-256 of a record's bytes.
#[must_use]
pub fn record_hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// A file's layout as an overlay names its base: the SHA-256 of its header and of its whole
/// directory (every record's id, place in the file, size, version, date and iteration). Any write
/// to the file moves a record's place or date, so two files with the same fingerprint hold the
/// same records; it is read from the directory walk the open already made, never from the
/// records, so it costs next to nothing to check. Two files holding the same records in another
/// order have different fingerprints and the same [`records_digest`]. Always the file's own,
/// whatever overlay is over it.
#[must_use]
pub fn fingerprint(file: &DatFile) -> [u8; 32] {
    *file.identity().layout.get_or_init(|| layout_of(file))
}

fn layout_of(file: &DatFile) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"dereth dat fingerprint 1");
    let hd = file.header();
    for v in [
        hd.block_size,
        hd.data_set,
        hd.data_subset,
        hd.btree_root,
        hd.master_map_id,
        u32::from(file.era() == ContainerEra::Classic),
        file.base_header_iteration().unwrap_or(0),
    ] {
        h.update(v.to_le_bytes());
    }
    for (_, e) in file.base_entries() {
        for v in [e.id, e.bits, e.offset, e.size, e.date, e.iteration] {
            h.update(v.to_le_bytes());
        }
    }
    h.finalize().into()
}

/// What [`records_digest`] hashes first, so a change to what it covers changes every one.
const RECORDS_MAGIC: &[u8] = b"dereth dat records 1";

/// How much of the file one read of [`records_digest`] takes.
const RECORDS_WINDOW: usize = 1 << 20;

/// A file's records as an overlay names its base: the SHA-256 of `dereth dat records 1`, the
/// header iteration (or 0), then each of the file's own records ascending by id, as its id, its
/// size and the SHA-256 of its bytes. The iteration list is one of the records. Where a record
/// lies in the file is not, nor its entry's flag word, date and iteration, which are stamps of
/// the copy rather than the record: one file copied in another order, or patched on another day,
/// holds the same records.
///
/// It reads the whole file, in the order the records lie in it (about a second for a portal
/// file), once for each open of the file. Always the file's own, whatever overlay is over it.
///
/// # Errors
/// A record that will not read, and a record stored compressed.
pub fn records_digest(file: &DatFile) -> Result<[u8; 32], DatError> {
    if let Some(d) = file.identity().records.get() {
        return Ok(*d);
    }
    let d = records_of(file)?;
    Ok(*file.identity().records.get_or_init(|| d))
}

/// A run of the file's bytes held for the block reads that fall inside it.
struct Window<'a> {
    file: &'a DatFile,
    end: u64,
    at: u64,
    len: usize,
    buf: Vec<u8>,
}

impl Window<'_> {
    /// The `n` bytes at `offset`, read with the run after them when they are not held.
    fn get(&mut self, offset: u64, n: usize) -> Result<&[u8], DatError> {
        let held = offset >= self.at && offset + n as u64 <= self.at + self.len as u64;
        if !held {
            let rest = usize::try_from(self.end.saturating_sub(offset)).unwrap_or(usize::MAX);
            let want = rest.min(self.buf.len()).max(n);
            self.len = if self.file.read_raw(offset, &mut self.buf[..want]).is_ok() {
                want
            } else {
                // A header that claims more than the storage holds: the block alone.
                self.file.read_raw(offset, &mut self.buf[..n])?;
                n
            };
            self.at = offset;
        }
        let from = usize::try_from(offset - self.at).unwrap_or(0);
        Ok(&self.buf[from..from + n])
    }
}

fn records_of(file: &DatFile) -> Result<[u8; 32], DatError> {
    let hd = file.header();
    let per = hd.payload_per_block();
    let entries: Vec<(DataId, BtEntry)> = file.base_entries().map(|(id, e)| (id, *e)).collect();
    // Front to back through the file, so the reads run through it in large pieces.
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_unstable_by_key(|i| entries[*i].1.offset);
    let mut digests = vec![[0u8; 32]; entries.len()];
    let mut window = Window {
        file,
        end: u64::from(hd.file_size),
        at: 0,
        len: 0,
        buf: vec![0; RECORDS_WINDOW.max(hd.block_size as usize)],
    };
    for i in order {
        let (id, e) = entries[i];
        if e.compressed() {
            return Err(DatError::CompressionUnsupported(id));
        }
        let mut h = Sha256::new();
        let mut remaining = e.size as usize;
        let mut offset = e.offset;
        while remaining > 0 && offset != 0 {
            let take = remaining.min(per);
            let block = window.get(u64::from(offset), take + 4)?;
            let link = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
            if link & 0x8000_0000 != 0 {
                return Err(DatError::FreeBlockInChain { id, offset });
            }
            h.update(&block[4..]);
            remaining -= take;
            offset = link;
        }
        if remaining != 0 {
            return Err(DatError::ChainTooShort {
                id,
                shortfall: remaining,
            });
        }
        digests[i] = h.finalize().into();
    }
    let mut h = Sha256::new();
    h.update(RECORDS_MAGIC);
    h.update(file.base_header_iteration().unwrap_or(0).to_le_bytes());
    for ((id, e), d) in entries.iter().zip(&digests) {
        h.update(id.raw().to_le_bytes());
        h.update(e.size.to_le_bytes());
        h.update(d);
    }
    Ok(h.finalize().into())
}

/// Whether `base` is the file an overlay names as its base by `layout` (the base's
/// [`fingerprint`]) and, when it names them, `records` (its [`records_digest`]). The same layout
/// is the same file, known at once; another layout is the same base when its records are the
/// same, which reads the whole file once for this open of it. A file that will not read whole is
/// not the base.
#[must_use]
pub fn is_named_base(base: &DatFile, layout: &[u8; 32], records: Option<&[u8; 32]>) -> bool {
    fingerprint(base) == *layout
        || records.is_some_and(|want| records_digest(base).is_ok_and(|have| have == *want))
}

/// How a refusal names the base an overlay wants and the file found instead, in hex: by records
/// when the overlay names them, else by layout.
#[must_use]
pub fn mismatch_names(
    base: &DatFile,
    layout: &[u8; 32],
    records: Option<&[u8; 32]>,
) -> (String, String) {
    match records {
        Some(want) => (
            format!("records {}", hex(want)),
            records_digest(base).map_or_else(
                |e| format!("unreadable ({e})"),
                |have| format!("records {}", hex(&have)),
            ),
        ),
        None => (hex(layout), hex(&fingerprint(base))),
    }
}

/// A fingerprint as lower-case hex, for log lines and file names.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The iterations a base file carries: one run from 1 for a file from before Throne of Destiny
/// (its header's count), else its `0xFFFF0001` list; none when it has neither.
#[must_use]
pub fn base_iterations(base: &DatFile) -> Vec<u32> {
    if let Some(n) = base.base_header_iteration() {
        return (1..=n).collect();
    }
    base.base().iteration_list().unwrap_or_default()
}

/// Why an overlay was not opened or written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OverlayError {
    #[error(transparent)]
    Dat(#[from] DatError),
    /// The overlay was made against another file than the base it would lie over.
    #[error(
        "{} was made against {base_name} {expected}, and this {base_name} is {found}",
        path.display()
    )]
    BaseMismatch {
        path: PathBuf,
        base_name: String,
        expected: String,
        found: String,
    },
    /// The folder holds another world's overlay.
    #[error("{} holds the overlay of the world {found:?}, not of {expected:?}", path.display())]
    OtherWorld {
        path: PathBuf,
        found: String,
        expected: String,
    },
    /// The folder holds base data files, which an overlay folder never does.
    #[error("{} holds base data files and is not an overlay folder", .0.display())]
    BaseFolder(PathBuf),
    /// The container has no manifest of this layout.
    #[error("{} is not an overlay container", .0.display())]
    NotOverlay(PathBuf),
}

/// One overlay container over the base file it was made against, as reads see it.
#[derive(Debug)]
pub struct Layer {
    path: PathBuf,
    file: DatFile,
    tombstones: Vec<Tombstone>,
    single: BTreeSet<u32>,
    /// The family deletions, by mask: the masked ids each deletes.
    families: BTreeMap<u32, BTreeSet<u32>>,
    manifest: ContainerManifest,
    iterations: Vec<u32>,
    own_iterations: Vec<u32>,
    len: usize,
    /// How many reads the overlay has answered with its own record, and how many with a deletion.
    served: AtomicU64,
    hidden: AtomicU64,
    /// The folder the container was opened from, and its name there, to reopen it from; `None`
    /// for a container opened by path, which is reopened by its path.
    source: Option<(Arc<dyn DatFolder>, &'static str)>,
}

impl Layer {
    /// The overlay container `overlay` over `base`. The container's manifest must name `base`
    /// (its fingerprint), and when `world_key` is given, that world.
    ///
    /// # Errors
    /// [`OverlayError::NotOverlay`] for a container with no manifest,
    /// [`OverlayError::BaseMismatch`] and [`OverlayError::OtherWorld`] as above, and read errors.
    pub fn over(
        base: &DatFile,
        overlay: DatFile,
        world_key: Option<&str>,
    ) -> Result<Self, OverlayError> {
        let path = overlay.path().to_path_buf();
        let manifest = overlay
            .read(MANIFEST)
            .and_then(|b| ContainerManifest::decode(&b))
            .map_err(|_| OverlayError::NotOverlay(path.clone()))?;
        if let Some(key) = world_key {
            if manifest.world_key != key {
                return Err(OverlayError::OtherWorld {
                    path,
                    found: manifest.world_key,
                    expected: key.to_owned(),
                });
            }
        }
        let (layout, records) = (&manifest.base_fingerprint, manifest.base_records.as_ref());
        if !is_named_base(base, layout, records) {
            let (expected, found) = mismatch_names(base, layout, records);
            return Err(OverlayError::BaseMismatch {
                path,
                base_name: manifest.base_name.clone(),
                expected,
                found,
            });
        }
        let tombstones = match overlay.read(TOMBSTONES) {
            Ok(b) => decode_tombstones(&b)?,
            Err(DatError::NotFound(_)) => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        let own = overlay.iteration_list().unwrap_or_default();
        let mut iterations = if manifest.exact_iterations {
            Vec::new()
        } else {
            base_iterations(base)
        };
        iterations.extend(own.iter().copied());
        iterations.sort_unstable();
        iterations.dedup();
        let single = tombstones
            .iter()
            .filter(|t| t.mask == 0)
            .map(|t| t.id)
            .collect();
        let mut families: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for t in tombstones.iter().filter(|t| t.mask != 0) {
            families.entry(t.mask).or_default().insert(t.id & t.mask);
        }
        let mut me = Self {
            path,
            file: overlay,
            tombstones,
            single,
            families,
            manifest,
            iterations,
            own_iterations: own,
            len: 0,
            served: AtomicU64::new(0),
            hidden: AtomicU64::new(0),
            source: None,
        };
        let kept = base
            .base_entries()
            .filter(|(id, _)| !me.hides(*id) && me.record(*id).is_none())
            .count();
        me.len = kept + me.records().count();
        Ok(me)
    }

    /// A container of the client's own records ([`crate::client_layer`]), which lies over any base
    /// of its layout: it has no deletions and no iterations, and names no base file.
    pub(crate) fn client(file: DatFile, manifest: ContainerManifest) -> Self {
        let len = file
            .base_entries()
            .filter(|(id, _)| !is_reserved(*id))
            .count();
        Self {
            path: file.path().to_path_buf(),
            file,
            tombstones: Vec::new(),
            single: BTreeSet::new(),
            families: BTreeMap::new(),
            manifest,
            iterations: Vec::new(),
            own_iterations: Vec::new(),
            len,
            served: AtomicU64::new(0),
            hidden: AtomicU64::new(0),
            source: None,
        }
    }

    /// This overlay reopened from its container over `base` (a base that has been reopened).
    ///
    /// # Errors
    /// As [`Self::over`].
    pub fn reopen_over(&self, base: &DatFile) -> Result<Self, OverlayError> {
        let file = match &self.source {
            Some((folder, name)) => DatFile::from_storage(
                self.path.clone(),
                folder.open(name).map_err(DatError::from)?,
            )?,
            None => DatFile::open(&self.path)?,
        };
        let mut me = Self::over(base, file, Some(&self.manifest.world_key))?;
        me.source.clone_from(&self.source);
        Ok(me)
    }

    /// The overlay's own entry for a world record `id`.
    #[must_use]
    pub fn record(&self, id: DataId) -> Option<&BtEntry> {
        if is_reserved(id) {
            return None;
        }
        self.file.base_entry(id)
    }

    /// Whether a tombstone covers `id`.
    #[must_use]
    pub fn hides(&self, id: DataId) -> bool {
        let id = id.raw();
        self.single.contains(&id)
            || self
                .families
                .iter()
                .any(|(mask, targets)| targets.contains(&(id & mask)))
    }

    /// Whether the overlay holds or deletes `id`.
    #[must_use]
    pub fn touches(&self, id: DataId) -> bool {
        self.record(id).is_some() || self.hides(id)
    }

    /// The overlay's world records, ascending by id.
    pub fn records(&self) -> impl Iterator<Item = (DataId, &BtEntry)> + Send + '_ {
        self.file.base_entries().filter(|(id, _)| !is_reserved(*id))
    }

    /// The overlay's copy of a world record.
    ///
    /// # Errors
    /// [`DatError::NotFound`] when it holds none, and read errors.
    pub fn read(&self, id: DataId) -> Result<Vec<u8>, DatError> {
        if is_reserved(id) {
            return Err(DatError::NotFound(id));
        }
        self.served.fetch_add(1, Ordering::Relaxed);
        self.file.read(id)
    }

    /// Count a read a deletion answered.
    pub(crate) fn note_hidden(&self) {
        self.hidden.fetch_add(1, Ordering::Relaxed);
    }

    /// How many reads the overlay has answered with its own record, and how many with one of
    /// its deletions, since it was opened.
    #[must_use]
    pub fn reads(&self) -> (u64, u64) {
        (
            self.served.load(Ordering::Relaxed),
            self.hidden.load(Ordering::Relaxed),
        )
    }

    /// The base file's iterations and the overlay's together, ascending.
    #[must_use]
    pub fn iterations(&self) -> &[u32] {
        &self.iterations
    }

    /// The overlay's own revisions, ascending.
    #[must_use]
    pub fn own_iterations(&self) -> &[u32] {
        &self.own_iterations
    }

    /// The overlay's deletions.
    #[must_use]
    pub fn tombstones(&self) -> &[Tombstone] {
        &self.tombstones
    }

    #[must_use]
    pub fn manifest(&self) -> &ContainerManifest {
        &self.manifest
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// How many records the file holds through the overlay.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the file holds no record through the overlay.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Two id-ascending entry streams with no id in common, as one ascending stream.
pub fn merge<'a, A, B>(a: A, b: B) -> impl Iterator<Item = (DataId, &'a BtEntry)> + Send + 'a
where
    A: Iterator<Item = (DataId, &'a BtEntry)> + Send + 'a,
    B: Iterator<Item = (DataId, &'a BtEntry)> + Send + 'a,
{
    let mut a = a.peekable();
    let mut b = b.peekable();
    std::iter::from_fn(move || match (a.peek(), b.peek()) {
        (Some(x), Some(y)) => {
            if x.0 <= y.0 {
                a.next()
            } else {
                b.next()
            }
        }
        (Some(_), None) => a.next(),
        (None, Some(_)) => b.next(),
        (None, None) => None,
    })
}

/// A world's overlay folder.
#[derive(Debug, Clone)]
pub struct OverlayDir {
    dir: PathBuf,
    folder: Arc<dyn DatFolder>,
}

/// Two overlay folders are the same folder when they have the same name.
impl PartialEq for OverlayDir {
    fn eq(&self, other: &Self) -> bool {
        self.dir == other.dir
    }
}

impl Eq for OverlayDir {}

impl OverlayDir {
    /// The overlay folder at `dir` on disk, which need not exist yet.
    ///
    /// # Errors
    /// [`OverlayError::BaseFolder`] when `dir` holds base data files.
    pub fn new(dir: &Path) -> Result<Self, OverlayError> {
        Self::in_folder(Arc::new(DiskFolder::new(dir)))
    }

    /// The overlay folder kept in `folder`, wherever that keeps its containers.
    ///
    /// # Errors
    /// [`OverlayError::BaseFolder`] when the folder holds base data files.
    pub fn in_folder(folder: Arc<dyn DatFolder>) -> Result<Self, OverlayError> {
        let holds_base = ModernDat::ALL.iter().any(|d| folder.is_file(d.file_name()))
            || crate::ClassicDat::ALL
                .iter()
                .any(|d| folder.is_file(d.file_name()));
        let dir = folder.path().to_path_buf();
        if holds_base {
            return Err(OverlayError::BaseFolder(dir));
        }
        Ok(Self { dir, folder })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// The storage the folder's containers are kept in.
    #[must_use]
    pub fn folder(&self) -> &Arc<dyn DatFolder> {
        &self.folder
    }

    /// Where the overlay container over `target`'s file is.
    #[must_use]
    pub fn container(&self, target: ModernDat) -> PathBuf {
        self.dir.join(container_name(target))
    }

    /// Whether the folder holds the overlay container over `target`'s file.
    #[must_use]
    pub fn holds(&self, target: ModernDat) -> bool {
        self.folder.is_file(container_name(target))
    }

    /// The overlay container over `target`'s file, opened for reading.
    ///
    /// # Errors
    /// No such container, and its read errors.
    pub fn open(&self, target: ModernDat) -> Result<DatFile, DatError> {
        DatFile::from_storage(
            self.container(target),
            self.folder.open(container_name(target))?,
        )
    }

    /// Remove the overlay container over `target`'s file.
    ///
    /// # Errors
    /// No such container, and the storage's errors.
    pub fn remove(&self, target: ModernDat) -> std::io::Result<()> {
        self.folder.remove(container_name(target))
    }

    /// The writer of the overlay container over `target`'s file: [`OverlayWriter::open_or_create`]
    /// in this folder.
    ///
    /// # Errors
    /// As [`OverlayWriter::open_or_create`].
    pub fn writer(
        &self,
        target: ModernDat,
        base: &DatFile,
        base_name: &str,
        world_key: &str,
        date: u32,
    ) -> Result<OverlayWriter, OverlayError> {
        OverlayWriter::open_or_create_in(
            &*self.folder,
            container_name(target),
            base,
            base_name,
            world_key,
            date,
        )
    }

    fn manifest_of(&self, target: ModernDat) -> Option<ContainerManifest> {
        let f = self.open(target).ok()?;
        ContainerManifest::decode(&f.read(MANIFEST).ok()?).ok()
    }

    /// The world the folder's overlay belongs to, from the first container that says; `None` for
    /// a folder with no container yet.
    #[must_use]
    pub fn world_key(&self) -> Option<String> {
        ModernDat::ALL
            .iter()
            .find_map(|t| self.manifest_of(*t))
            .map(|m| m.world_key)
    }

    /// The dat set the folder's overlay was made against, from the first container's base file
    /// name: the files before Throne of Destiny for `portal.dat` or `cell.dat`, the later files
    /// otherwise. `None` for a folder with no container yet.
    #[must_use]
    pub fn base_era(&self) -> Option<ContainerEra> {
        ModernDat::ALL.iter().find_map(|t| {
            let m = self.manifest_of(*t)?;
            let older = crate::ClassicDat::ALL
                .iter()
                .any(|d| d.file_name().eq_ignore_ascii_case(&m.base_name));
            Some(if older {
                ContainerEra::Classic
            } else {
                ContainerEra::Modern
            })
        })
    }

    /// The overlay over `base`, which is `target`'s file, when the folder has one. `world_key`,
    /// when given, is the world it must belong to.
    ///
    /// # Errors
    /// As [`Layer::over`], and a container that will not open.
    pub fn layer_over(
        &self,
        target: ModernDat,
        base: &DatFile,
        world_key: Option<&str>,
    ) -> Result<Option<Layer>, OverlayError> {
        if !self.holds(target) {
            return Ok(None);
        }
        let file = self.open(target)?;
        let mut layer = Layer::over(&base.base(), file, world_key)?;
        layer.source = Some((Arc::clone(&self.folder), container_name(target)));
        Ok(Some(layer))
    }

    /// Every overlay container's path in the folder.
    #[must_use]
    pub fn containers(&self) -> Vec<PathBuf> {
        ModernDat::ALL
            .iter()
            .filter(|t| self.holds(**t))
            .map(|t| self.container(*t))
            .collect()
    }
}

/// Writes one overlay container: the records a world adds or replaces, its deletions and its
/// revisions. The base file is never opened for writing.
#[derive(Debug)]
pub struct OverlayWriter {
    writer: DatWriter,
    tombstones: Vec<Tombstone>,
    manifest: ContainerManifest,
    dirty: bool,
}

impl OverlayWriter {
    /// The container at `path` over `base` (named `base_name`) for the world `world_key`, made
    /// when it is not there yet.
    ///
    /// # Errors
    /// [`OverlayError::OtherWorld`] or [`OverlayError::BaseMismatch`] for a container made for
    /// another world or against another base, and the writer's errors.
    pub fn open_or_create(
        path: &Path,
        base: &DatFile,
        base_name: &str,
        world_key: &str,
        date: u32,
    ) -> Result<Self, OverlayError> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = path.parent().unwrap_or_else(|| Path::new(""));
        Self::open_or_create_in(
            &DiskFolder::new(dir),
            &name,
            base,
            base_name,
            world_key,
            date,
        )
    }

    /// [`Self::open_or_create`] for the container `name` in `folder`, wherever that keeps it.
    ///
    /// # Errors
    /// As [`Self::open_or_create`].
    pub fn open_or_create_in(
        folder: &dyn DatFolder,
        name: &str,
        base: &DatFile,
        base_name: &str,
        world_key: &str,
        date: u32,
    ) -> Result<Self, OverlayError> {
        let path = folder.path().join(name);
        let base = base.base();
        let fp = fingerprint(&base);
        if folder.is_file(name) {
            let mut writer = DatWriter::open_in(path.clone(), folder.open_mut(name)?)?;
            let mut manifest = writer
                .read(MANIFEST)
                .and_then(|b| ContainerManifest::decode(&b))
                .map_err(|_| OverlayError::NotOverlay(path.clone()))?;
            if manifest.world_key != world_key {
                return Err(OverlayError::OtherWorld {
                    path,
                    found: manifest.world_key,
                    expected: world_key.to_owned(),
                });
            }
            let (layout, records) = (&manifest.base_fingerprint, manifest.base_records.as_ref());
            if !is_named_base(&base, layout, records) {
                let (expected, found) = mismatch_names(&base, layout, records);
                return Err(OverlayError::BaseMismatch {
                    path,
                    base_name: manifest.base_name,
                    expected,
                    found,
                });
            }
            let mut dirty = false;
            // The same records laid out otherwise: the overlay is kept over this file now.
            if manifest.base_fingerprint != fp {
                manifest.base_fingerprint = fp;
                dirty = true;
            }
            // A container written before overlays named their bases by their records names its
            // base's records from now on.
            if manifest.base_records.is_none() {
                manifest.base_records = records_digest(&base).ok();
                dirty |= manifest.base_records.is_some();
            }
            let tombstones = match writer.read(TOMBSTONES) {
                Ok(b) => decode_tombstones(&b)?,
                Err(DatError::NotFound(_)) => Vec::new(),
                Err(e) => return Err(e.into()),
            };
            return Ok(Self {
                writer,
                tombstones,
                manifest,
                dirty,
            });
        }
        let hd = base.header();
        let block = hd.block_size.max(0x100);
        let mut writer = DatWriter::create_in(
            path,
            folder.create(name)?,
            block,
            hd.data_set,
            hd.data_subset,
            0x400 + block * 64,
        )?;
        writer.save(ITERATION_LIST, &crate::iteration::encode(&[]), 1, 0, date)?;
        #[allow(clippy::cast_possible_truncation)]
        let manifest = ContainerManifest {
            world_key: world_key.to_owned(),
            base_name: base_name.to_owned(),
            base_fingerprint: fp,
            base_records: records_digest(&base).ok(),
            base_iterations: base_iterations(&base).len() as u32,
            exact_iterations: false,
            hashes: BTreeMap::new(),
        };
        let mut me = Self {
            writer,
            tombstones: Vec::new(),
            manifest,
            dirty: true,
        };
        me.flush(date)?;
        Ok(me)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        self.writer.path()
    }

    #[must_use]
    pub fn manifest(&self) -> &ContainerManifest {
        &self.manifest
    }

    #[must_use]
    pub fn tombstones(&self) -> &[Tombstone] {
        &self.tombstones
    }

    /// Name the world the overlay belongs to: for a container made before the world named itself.
    pub fn set_world_key(&mut self, world_key: &str) {
        if self.manifest.world_key != world_key {
            world_key.clone_into(&mut self.manifest.world_key);
            self.dirty = true;
        }
    }

    /// The SHA-256 of the overlay's record `id`, as it was written.
    #[must_use]
    pub fn hash_of(&self, id: DataId) -> Option<[u8; 32]> {
        self.manifest.hashes.get(&id.raw()).copied()
    }

    /// Put a world record in the overlay. As the retail save rule has it, a record whose
    /// iteration is older than the one already in force (the overlay's, else the base's) is
    /// refused, and reported as [`SaveOutcome::RefusedOlderIteration`].
    ///
    /// # Errors
    /// [`DatError::NotFound`] for a reserved id, and the writer's errors.
    pub fn save(
        &mut self,
        base: &DatFile,
        id: DataId,
        payload: &[u8],
        version: u16,
        iteration: u32,
        date: u32,
    ) -> Result<SaveOutcome, DatError> {
        if is_reserved(id) {
            return Err(DatError::NotFound(id));
        }
        let hidden = self.tombstones.iter().any(|t| t.covers(id.raw()));
        if iteration != 0 && self.writer.entry(id)?.is_none() && !hidden {
            if let Some(e) = base.base_entry(id) {
                if iteration < e.iteration {
                    return Ok(SaveOutcome::RefusedOlderIteration);
                }
            }
        }
        let held = self.writer.entry(id)?.is_some();
        let mut out = self.writer.save(id, payload, version, iteration, date)?;
        // A record the base holds and the overlay did not is replaced as the world reads it.
        if out == SaveOutcome::Added && !held && !hidden && base.base_entry(id).is_some() {
            out = SaveOutcome::Replaced;
        }
        if out != SaveOutcome::RefusedOlderIteration {
            self.manifest.hashes.insert(id.raw(), record_hash(payload));
            self.dirty = true;
        }
        Ok(out)
    }

    /// Put a world record in the overlay as the world's own files carry it, whatever the base's
    /// iteration of it: what the decompose tool writes.
    ///
    /// # Errors
    /// [`DatError::NotFound`] for a reserved id, and the writer's errors.
    pub fn put(
        &mut self,
        id: DataId,
        payload: &[u8],
        version: u16,
        iteration: u32,
        date: u32,
    ) -> Result<SaveOutcome, DatError> {
        if is_reserved(id) {
            return Err(DatError::NotFound(id));
        }
        // An overlay record of an older iteration than one already there is still the world's.
        self.writer.delete_data(id, 0)?;
        let out = self.writer.save(id, payload, version, iteration, date)?;
        self.manifest.hashes.insert(id.raw(), record_hash(payload));
        self.dirty = true;
        Ok(out)
    }

    /// Take the world record `id` out of the overlay, so a read through it finds the base's again
    /// (unless a tombstone hides it). Unlike [`Self::tombstone`] nothing is deleted from the base.
    /// Answers whether the overlay held it.
    ///
    /// # Errors
    /// [`DatError::NotFound`] for a reserved id, and the writer's errors.
    pub fn revert(&mut self, id: DataId) -> Result<bool, DatError> {
        if is_reserved(id) {
            return Err(DatError::NotFound(id));
        }
        let held = self.writer.delete_data(id, 0)?;
        if self.manifest.hashes.remove(&id.raw()).is_some() || held {
            self.dirty = true;
        }
        Ok(held)
    }

    /// Make the overlay's iteration list the world's whole list, `iterations`, reported as it is.
    ///
    /// # Errors
    /// The writer's errors.
    pub fn set_exact_iterations(&mut self, iterations: &[u32], date: u32) -> Result<(), DatError> {
        let mut set = iterations.to_vec();
        set.sort_unstable();
        set.dedup();
        self.writer
            .save(ITERATION_LIST, &crate::iteration::encode(&set), 1, 0, date)?;
        self.manifest.exact_iterations = true;
        self.dirty = true;
        Ok(())
    }

    /// Delete `id` (or, with a `mask`, its whole family) in revision `iteration`: the overlay's
    /// own copies go, and a tombstone hides the base's. Answers how many of the overlay's own
    /// records went.
    ///
    /// # Errors
    /// The writer's errors.
    pub fn tombstone(&mut self, id: DataId, mask: u32, iteration: u32) -> Result<usize, DatError> {
        let removed = if mask == 0 {
            usize::from(!is_reserved(id) && self.writer.delete_data(id, 0)?)
        } else {
            let target = id.raw() & mask;
            let mut n = 0;
            // A mask of high bits names one run of ids.
            let contiguous = mask.leading_ones() + mask.trailing_zeros() == 32;
            let (lo, hi) = if contiguous {
                (target, target | !mask)
            } else {
                (0, u32::MAX)
            };
            for candidate in self.writer.ids_in_range(lo, hi)? {
                if candidate & mask == target
                    && !is_reserved(DataId(candidate))
                    && self.writer.delete_data(DataId(candidate), 0)?
                {
                    n += 1;
                }
            }
            n
        };
        self.manifest.hashes.retain(|k, _| {
            if mask == 0 {
                *k != id.raw()
            } else {
                *k & mask != id.raw() & mask
            }
        });
        let t = Tombstone {
            id: if mask == 0 { id.raw() } else { id.raw() & mask },
            mask,
            iteration,
        };
        if !self
            .tombstones
            .iter()
            .any(|x| x.id == t.id && x.mask == t.mask)
        {
            self.tombstones.push(t);
            self.tombstones.sort_unstable();
        }
        self.dirty = true;
        Ok(removed)
    }

    /// Record revision `iteration` as applied.
    ///
    /// # Errors
    /// The writer's errors.
    pub fn add_iteration(&mut self, iteration: u32, date: u32) -> Result<(), DatError> {
        self.writer.add_iteration(iteration, date)
    }

    /// The overlay's own revisions.
    ///
    /// # Errors
    /// The writer's errors.
    pub fn iterations(&mut self) -> Result<Vec<u32>, DatError> {
        self.writer.iteration_list()
    }

    /// Write the tombstones and the manifest when they have changed.
    ///
    /// # Errors
    /// The writer's errors.
    pub fn flush(&mut self, date: u32) -> Result<(), DatError> {
        if !self.dirty {
            return Ok(());
        }
        if self.tombstones.is_empty() {
            self.writer.delete_data(TOMBSTONES, 0)?;
        } else {
            self.writer
                .save(TOMBSTONES, &encode_tombstones(&self.tombstones), 1, 0, date)?;
        }
        self.writer
            .save(MANIFEST, &self.manifest.encode(), 1, 0, date)?;
        self.dirty = false;
        Ok(())
    }
}

impl Drop for OverlayWriter {
    fn drop(&mut self) {
        let _ = self.flush(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tombstones_and_the_manifest_read_back_as_written() {
        let t = vec![
            Tombstone {
                id: 0x0600_0001,
                mask: 0,
                iteration: 2073,
            },
            Tombstone {
                id: 0xA9B4_0000,
                mask: 0xFFFF_0000,
                iteration: 2074,
            },
        ];
        assert_eq!(decode_tombstones(&encode_tombstones(&t)).unwrap(), t);
        assert!(t[1].covers(0xA9B4_0105) && t[1].covers(0xA9B4_FFFF));
        assert!(!t[1].covers(0xA9B5_0105));
        assert!(t[0].covers(0x0600_0001) && !t[0].covers(0x0600_0002));
        let mut m = ContainerManifest {
            world_key: "a world".into(),
            base_name: "client_portal.dat".into(),
            base_fingerprint: [7; 32],
            base_records: Some([9; 32]),
            base_iterations: 2072,
            exact_iterations: true,
            hashes: BTreeMap::new(),
        };
        m.hashes.insert(0x0600_0001, record_hash(b"x"));
        let named = m.encode();
        assert_eq!(&named[4..8], &2u32.to_le_bytes(), "the second layout");
        assert_eq!(ContainerManifest::decode(&named).unwrap(), m);
        // A manifest that names no records is written in the first layout, which every reader of
        // overlays reads.
        m.base_records = None;
        let plain = m.encode();
        assert_eq!(&plain[4..8], &1u32.to_le_bytes(), "the first layout");
        assert_eq!(plain.len() + 32, named.len());
        assert_eq!(ContainerManifest::decode(&plain).unwrap(), m);
        assert!(ContainerManifest::decode(b"not a manifest at all").is_err());
    }
}
