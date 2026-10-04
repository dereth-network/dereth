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
//!   as read through the overlay are the base file's and these together.
//! - `0xFFFF0002`, the tombstones ([`Tombstone`]): a record id, or a family of ids under a mask,
//!   that the world has deleted, with the revision that deleted it.
//! - `0xFFFF0003`, the manifest ([`ContainerManifest`]): which world the overlay belongs to, which
//!   base file it was made against (its name and [`fingerprint`]), and the SHA-256 of every record
//!   it holds.
//!
//! A read through a file carrying its overlay ([`crate::DatFile::layered`]) asks the overlay
//! first: a record it holds is the overlay's, a record a tombstone covers is not there, and every
//! other record is the base file's. The records in an overlay are in the base file's own record
//! layout: an overlay over a file from before Throne of Destiny holds records of that time.
//!
//! An overlay opens only over the base it was made against: a different base is refused
//! ([`OverlayError::BaseMismatch`]), as is a folder holding another world's overlay
//! ([`OverlayError::OtherWorld`]) and a folder holding base files ([`OverlayError::BaseFolder`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_primitives::DataId;
use sha2::{Digest, Sha256};

use crate::btree::BtEntry;
use crate::container::{ContainerEra, DatFile};
use crate::cursor::Cursor;
use crate::divine::ITERATION_LIST;
use crate::error::DatError;
use crate::locate::RetailDat;
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
pub const fn container_name(target: RetailDat) -> &'static str {
    match target {
        RetailDat::Portal => "overlay_portal.dat",
        RetailDat::Cell => "overlay_cell.dat",
        RetailDat::Local => "overlay_local.dat",
        RetailDat::HighRes => "overlay_highres.dat",
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
/// The manifest layout this reader and writer speak.
const MANIFEST_VERSION: u32 = 1;

/// What an overlay container says about itself.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ContainerManifest {
    /// The world the overlay belongs to, as the world names itself.
    pub world_key: String,
    /// The base file's name (`client_portal.dat`, `portal.dat`, ...).
    pub base_name: String,
    /// The base file's [`fingerprint`].
    pub base_fingerprint: [u8; 32],
    /// How many iterations the base file carries.
    pub base_iterations: u32,
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
    /// The manifest record's bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.hashes.len() * 36);
        out.extend_from_slice(&MANIFEST_MAGIC.to_le_bytes());
        out.extend_from_slice(&MANIFEST_VERSION.to_le_bytes());
        put_str(&mut out, &self.world_key);
        put_str(&mut out, &self.base_name);
        out.extend_from_slice(&self.base_fingerprint);
        out.extend_from_slice(&self.base_iterations.to_le_bytes());
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
        if magic != MANIFEST_MAGIC || version != MANIFEST_VERSION {
            return Err(DatError::NotFound(MANIFEST));
        }
        let world_key = get_str(&mut c)?;
        let base_name = get_str(&mut c)?;
        let base_fingerprint = get_hash(&mut c)?;
        let base_iterations = c.u32()?;
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
            base_iterations,
            hashes,
        })
    }
}

/// The SHA-256 of a record's bytes.
#[must_use]
pub fn record_hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// A file's identity as an overlay names its base: the SHA-256 of its header and of its whole
/// directory (every record's id, place in the file, size, version, date and iteration). Any write
/// to the file moves a record's place or date, so two files with the same fingerprint hold the
/// same records; it is read from the directory walk the open already made, never from the
/// records, so it costs nothing to check. Always the file's own, whatever overlay is over it.
#[must_use]
pub fn fingerprint(file: &DatFile) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"dereth dat fingerprint 1");
    let hd = file.header();
    for v in [
        hd.block_size,
        hd.data_set,
        hd.data_subset,
        hd.btree_root,
        hd.master_map_id,
        u32::from(file.era() == ContainerEra::PreTod),
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
    families: Vec<(u32, u32)>,
    manifest: ContainerManifest,
    iterations: Vec<u32>,
    len: usize,
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
        let found = fingerprint(base);
        if manifest.base_fingerprint != found {
            return Err(OverlayError::BaseMismatch {
                path,
                base_name: manifest.base_name.clone(),
                expected: hex(&manifest.base_fingerprint),
                found: hex(&found),
            });
        }
        let tombstones = match overlay.read(TOMBSTONES) {
            Ok(b) => decode_tombstones(&b)?,
            Err(DatError::NotFound(_)) => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        let own = overlay.iteration_list().unwrap_or_default();
        let mut iterations = base_iterations(base);
        iterations.extend(own);
        iterations.sort_unstable();
        iterations.dedup();
        let single = tombstones
            .iter()
            .filter(|t| t.mask == 0)
            .map(|t| t.id)
            .collect();
        let families = tombstones
            .iter()
            .filter(|t| t.mask != 0)
            .map(|t| (t.id & t.mask, t.mask))
            .collect();
        let mut me = Self {
            path,
            file: overlay,
            tombstones,
            single,
            families,
            manifest,
            iterations,
            len: 0,
        };
        let kept = base
            .base_entries()
            .filter(|(id, _)| !me.hides(*id) && me.record(*id).is_none())
            .count();
        me.len = kept + me.records().count();
        Ok(me)
    }

    /// This overlay reopened from its container over `base` (a base that has been reopened).
    ///
    /// # Errors
    /// As [`Self::over`].
    pub fn reopen_over(&self, base: &DatFile) -> Result<Self, OverlayError> {
        Self::over(
            base,
            DatFile::open(&self.path)?,
            Some(&self.manifest.world_key),
        )
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
        self.single.contains(&id) || self.families.iter().any(|(t, m)| id & m == *t)
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
        self.file.read(id)
    }

    /// The base file's iterations and the overlay's together, ascending.
    #[must_use]
    pub fn iterations(&self) -> &[u32] {
        &self.iterations
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayDir {
    dir: PathBuf,
}

impl OverlayDir {
    /// The overlay folder at `dir`, which need not exist yet.
    ///
    /// # Errors
    /// [`OverlayError::BaseFolder`] when `dir` holds base data files.
    pub fn new(dir: &Path) -> Result<Self, OverlayError> {
        let holds_base = RetailDat::ALL.iter().any(|d| d.in_dir(dir).is_file())
            || crate::PreTodDat::ALL
                .iter()
                .any(|d| d.in_dir(dir).is_file());
        if holds_base {
            return Err(OverlayError::BaseFolder(dir.to_path_buf()));
        }
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// Where the overlay container over `target`'s file is.
    #[must_use]
    pub fn container(&self, target: RetailDat) -> PathBuf {
        self.dir.join(container_name(target))
    }

    /// The world the folder's overlay belongs to, from the first container that says; `None` for
    /// a folder with no container yet.
    #[must_use]
    pub fn world_key(&self) -> Option<String> {
        RetailDat::ALL.iter().find_map(|t| {
            let f = DatFile::open(&self.container(*t)).ok()?;
            let m = ContainerManifest::decode(&f.read(MANIFEST).ok()?).ok()?;
            Some(m.world_key)
        })
    }

    /// The overlay over `base`, which is `target`'s file, when the folder has one. `world_key`,
    /// when given, is the world it must belong to.
    ///
    /// # Errors
    /// As [`Layer::over`], and a container that will not open.
    pub fn layer_over(
        &self,
        target: RetailDat,
        base: &DatFile,
        world_key: Option<&str>,
    ) -> Result<Option<Layer>, OverlayError> {
        let path = self.container(target);
        if !path.is_file() {
            return Ok(None);
        }
        let file = DatFile::open(&path)?;
        Layer::over(&base.base(), file, world_key).map(Some)
    }

    /// Every overlay container's path in the folder.
    #[must_use]
    pub fn containers(&self) -> Vec<PathBuf> {
        RetailDat::ALL
            .iter()
            .map(|t| self.container(*t))
            .filter(|p| p.is_file())
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
        let base = base.base();
        let fp = fingerprint(&base);
        if path.is_file() {
            let mut writer = DatWriter::open(path)?;
            let manifest = writer
                .read(MANIFEST)
                .and_then(|b| ContainerManifest::decode(&b))
                .map_err(|_| OverlayError::NotOverlay(path.to_path_buf()))?;
            if manifest.world_key != world_key {
                return Err(OverlayError::OtherWorld {
                    path: path.to_path_buf(),
                    found: manifest.world_key,
                    expected: world_key.to_owned(),
                });
            }
            if manifest.base_fingerprint != fp {
                return Err(OverlayError::BaseMismatch {
                    path: path.to_path_buf(),
                    base_name: manifest.base_name,
                    expected: hex(&manifest.base_fingerprint),
                    found: hex(&fp),
                });
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
                dirty: false,
            });
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(DatError::from)?;
        }
        let hd = base.header();
        let block = hd.block_size.max(0x100);
        let mut writer =
            DatWriter::create(path, block, hd.data_set, hd.data_subset, 0x400 + block * 64)?;
        writer.save(ITERATION_LIST, &crate::iteration::encode(&[]), 1, 0, date)?;
        #[allow(clippy::cast_possible_truncation)]
        let manifest = ContainerManifest {
            world_key: world_key.to_owned(),
            base_name: base_name.to_owned(),
            base_fingerprint: fp,
            base_iterations: base_iterations(&base).len() as u32,
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
            for candidate in self.writer.ids_in_range(0, u32::MAX)? {
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
            base_iterations: 2072,
            hashes: BTreeMap::new(),
        };
        m.hashes.insert(0x0600_0001, record_hash(b"x"));
        assert_eq!(ContainerManifest::decode(&m.encode()).unwrap(), m);
        assert!(ContainerManifest::decode(b"not a manifest at all").is_err());
    }
}
