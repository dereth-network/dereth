//! The four retail files behind one lookup, in the client's own order.
//!
//! The cache classifies the request's type as portal, cell, or local to pick slot 0, 2, or 1. For a
//! portal type it asks the high-res controller **first** when that file is open and holds the id.
//!
//! `client_highres.dat` is a *disjoint partition* of the portal id space, not an override: its
//! 2,294 `0x06` ids and the portal dat's 20,684 have zero collisions. The ordering is implemented
//! because the client has it; an override-resolution mechanism would never be exercised.
//! The high-resolution store is searched only after the portal store.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use dereth_primitives::{AssetError, AssetSource, DataId, DataType};

use crate::container::{
    ContainerEra, DatFile, CELL_DATFILE, HIRES_SUBSET, LOCAL_DATFILE, PORTAL_DATFILE,
};
use crate::divine::{classify_cell_id, divine_type, DatKind, DbType};
use crate::error::DatError;
use crate::locate::{PreTodDat, RetailDat};

pub(crate) mod shared;

/// The four retail files behind one lookup.
///
/// **A handle, not the files.** Each container is an [`Arc<DatFile>`] out of the process-wide
/// table in [`shared`], so opening the same directory a second time is three `stat`s and three
/// refcount bumps rather than three directory walks. The `dat` test tiers open a store per
/// test — about forty times in one `dereth-testkit` batch, once per `App` elsewhere — and an
/// unshared open re-walks 887,455 B-tree entries across 1.4 GB: a measured tier run read 73 GB
/// without the sharing.
///
/// **Why sharing is safe.** A `DatFile` is opened read-only ([`std::fs::File::open`]) and every
/// read takes `&self` and is a positional read that names its own offset, so no reader can move
/// another's cursor and no `Mutex<File>` is needed to make `seek`+`read` atomic. The crate's
/// *only* writer,
/// [`crate::write::DatWriter`], refuses every path in a read-only install
/// ([`crate::protect_install`]) in its `refuse_owner_dat` guard, so a protected install cannot be
/// written in-process at all. The
/// things that are **not** shared are the per-store ones: the high-res grant below and
/// `client_dir`. That is deliberate — the grant is a *policy* decision a caller makes
/// (`dereth-assets`' retail-data gate grants itself one; a client without the DDD bit does not),
/// and sharing it would leak one test's grant into the next test in the same binary.
#[derive(Clone, Debug)]
pub struct RetailDatStore {
    portal: Arc<DatFile>,
    cell: Arc<DatFile>,
    local: Arc<DatFile>,
    /// `client_highres.dat`, open only once [`Self::grant_highres`] has been called -- the
    /// client's own high-res load, which the server-interrogation handler
    /// calls only when bit `0x4` of the DDD interrogation's product id is set. A `OnceLock`
    /// because the grant is monotone (retail never closes the file) and the store is shared
    /// behind an `Arc`. Per *handle*, not per file: see the type's note.
    highres: OnceLock<Arc<DatFile>>,
    /// Beside a dat set from before Throne of Destiny, the later `client_portal.dat`, which
    /// answers a portal read the older portal file has no record for: the interface's records
    /// (layout properties, fonts, the enum and id maps, interface images) that the later client's
    /// screens need and the older files do not have. `None` otherwise.
    later_portal: Option<Arc<DatFile>>,
    /// Beside a later world, a `portal.dat` from before Throne of Destiny that only presentation
    /// reads: the older regions' ground and sky, and the pictures and objects they name. No read
    /// of this store reaches it; [`Self::legacy_files`] and [`Self::object_files`] are the only
    /// ways in. `None` otherwise.
    legacy_portal: Option<Arc<DatFile>>,
    /// In the store [`Self::object_files`] makes, the world's own portal, which answers a portal
    /// read the other era's portal has no record for. `None` otherwise.
    fallback_portal: Option<Arc<DatFile>>,
    /// In the store [`Self::object_files`] makes, the later portal file whose image levels
    /// (`0x06`) a later image texture names: the older files hold images under the same ids that
    /// are other pictures in another layout, so a later texture's levels are read from its own
    /// files whichever portal answered the texture. `None` otherwise.
    levels_from: Option<Arc<DatFile>>,
    /// Where [`Self::grant_highres`] looks for the file; `None` for a store built from files.
    client_dir: Option<PathBuf>,
}

impl RetailDatStore {
    /// The files [`Self::open_dir`] cannot do without.
    ///
    /// `client_highres.dat` is deliberately **not** here: `open_dir` treats it as optional because
    /// the client only opens it when bit `0x4` of the DDD interrogation's product id is set,
    /// so requiring it would fail a machine this store works perfectly on. The tier-1 gates
    /// (`xtask`'s `RETAIL_DATS`) list all four for a different and equally good reason -- a partial
    /// install makes the retail-data check answer false to every tier-1 gate -- and the two lists
    /// differing is a fact worth knowing rather than a bug. `dereth-dat`'s own
    /// `tests/aaa_preflight.rs` asserts the stricter four and says so at the site.
    pub const REQUIRED_DATS: [&'static str; 3] = [
        RetailDat::Portal.file_name(),
        RetailDat::Cell.file_name(),
        RetailDat::Local.file_name(),
    ];

    /// Open the three required files from one directory. `client_highres.dat` is **not** opened
    /// here even when present: the client opens it only when bit `0x4` of the DDD
    /// interrogation's product id is set, and ACE sends `0x1` unless its `allow_highres_dat`
    /// property (default false) is on. [`Self::grant_highres`] is that call. Opening the file
    /// whenever it exists would resolve every two-level `SurfaceTexture` to art retail never shows
    /// on ACE.
    ///
    /// The three files come from [`shared::open`], so the second open of a
    /// directory this process has already opened costs three `stat`s instead of three directory
    /// walks. The returned store is still a value of its own -- nothing about the high-res grant
    /// or `client_dir` is shared -- and `shared::open` re-walks whenever the file's length or
    /// mtime has moved, which is what keeps `App::invalidate_after_ddd`'s reopen honest.
    pub fn open_dir(client_dir: &Path) -> Result<Self, DatError> {
        let open = |dat: RetailDat| -> Result<Arc<DatFile>, DatError> {
            let path = dat.in_dir(client_dir);
            let file = shared::open(&path)?;
            if file.era() != ContainerEra::Tod {
                return Err(DatError::UnexpectedContainerEra {
                    path,
                    found: file.era(),
                    expected: ContainerEra::Tod,
                });
            }
            Ok(file)
        };
        let portal = open(RetailDat::Portal)?;
        let cell = open(RetailDat::Cell)?;
        let local = open(RetailDat::Local)?;
        Ok(Self {
            portal,
            cell,
            local,
            highres: OnceLock::new(),
            later_portal: None,
            legacy_portal: None,
            fallback_portal: None,
            levels_from: None,
            client_dir: Some(client_dir.to_path_buf()),
        })
    }

    /// Open a dat set from before Throne of Destiny: `portal.dat` and `cell.dat` from one directory,
    /// each checked to be in the older container layout. There is no language file: the records
    /// that later moved there (strings, interface layouts) are in the portal file, so language-type
    /// reads go to it. There is no high-resolution file either, and [`Self::grant_highres`] finds
    /// none.
    ///
    /// # Errors
    ///
    /// A file is missing, does not open, or is in the later layout.
    pub fn open_pre_tod_dir(dir: &Path) -> Result<Self, DatError> {
        let open = |dat: PreTodDat| -> Result<Arc<DatFile>, DatError> {
            let path = dat.in_dir(dir);
            let file = shared::open(&path)?;
            if file.era() != ContainerEra::PreTod {
                return Err(DatError::UnexpectedContainerEra {
                    path,
                    found: file.era(),
                    expected: ContainerEra::PreTod,
                });
            }
            Ok(file)
        };
        let portal = open(PreTodDat::Portal)?;
        let cell = open(PreTodDat::Cell)?;
        Ok(Self {
            local: Arc::clone(&portal),
            portal,
            cell,
            highres: OnceLock::new(),
            later_portal: None,
            legacy_portal: None,
            fallback_portal: None,
            levels_from: None,
            client_dir: Some(dir.to_path_buf()),
        })
    }

    /// A dat set from before Throne of Destiny with the later interface beside it: `portal.dat`
    /// and `cell.dat` from `dir` answer the world, and the later `client_local_English.dat` and
    /// `client_portal.dat` from `later_dir` answer the language reads and every portal read the
    /// older portal file has no record for. This is what lets the later screens run over the
    /// older world: their layouts, strings, fonts and interface images come from the later files,
    /// and everything the world draws from the older ones. [`Self::era_of`] says which layout each
    /// record is in.
    ///
    /// # Errors
    ///
    /// A file is missing, does not open, or is in the other layout.
    pub fn open_pre_tod_with_later(dir: &Path, later_dir: &Path) -> Result<Self, DatError> {
        let mut store = Self::open_pre_tod_dir(dir)?;
        let later = |dat: RetailDat| -> Result<Arc<DatFile>, DatError> {
            let path = dat.in_dir(later_dir);
            let file = shared::open(&path)?;
            if file.era() != ContainerEra::Tod {
                return Err(DatError::UnexpectedContainerEra {
                    path,
                    found: file.era(),
                    expected: ContainerEra::Tod,
                });
            }
            Ok(file)
        };
        store.local = later(RetailDat::Local)?;
        store.later_portal = Some(later(RetailDat::Portal)?);
        store.client_dir = None;
        Ok(store)
    }

    /// The container layout of the store's world files: the portal file's, which [`Self::open_dir`]
    /// and [`Self::open_pre_tod_dir`] each require the others to share.
    #[must_use]
    pub fn era(&self) -> ContainerEra {
        self.portal.era()
    }

    /// The layout of the record `id` as this store reads it: the world files' (the portal and cell
    /// files hold it), else the later files' (a store with the later interface beside an older
    /// world answers it from those). In an [`Self::object_files`] store an image level is the
    /// later files', and a record the other era lacks is the world's.
    #[must_use]
    pub fn era_of(&self, id: DataId) -> ContainerEra {
        if let Some(levels) = &self.levels_from {
            if divine_type(id) == Some(DbType::RenderSurface) {
                return levels.era();
            }
        }
        if self.portal.contains(id) {
            return self.portal.era();
        }
        if self.cell.contains(id) {
            return self.cell.era();
        }
        if let Some(f) = self.fallback_portal.as_ref().filter(|f| f.contains(id)) {
            return f.era();
        }
        let later = self.later_portal.as_ref().is_some_and(|f| f.contains(id))
            || (!Arc::ptr_eq(&self.local, &self.portal) && self.local.contains(id));
        if later {
            ContainerEra::Tod
        } else {
            self.portal.era()
        }
    }

    /// Whether this store answers from the later interface files beside an older world.
    #[must_use]
    pub fn has_later_interface(&self) -> bool {
        self.later_portal.is_some()
    }

    /// Beside an older world, the later files as a store of their own: the later portal and
    /// language files answer every portal and language read, including the records the older
    /// portal file also holds, so a record both sets carry is read as the later set has it. There
    /// is no later cell file beside an older world, so cell reads still go to the world's.
    /// `None` for a store with no later files beside it.
    #[must_use]
    pub fn later_files(&self) -> Option<Self> {
        let portal = Arc::clone(self.later_portal.as_ref()?);
        Some(Self {
            portal,
            cell: Arc::clone(&self.cell),
            local: Arc::clone(&self.local),
            highres: OnceLock::new(),
            later_portal: None,
            legacy_portal: None,
            fallback_portal: None,
            levels_from: None,
            client_dir: None,
        })
    }

    /// This store with a `portal.dat` from before Throne of Destiny beside it, for presentation
    /// alone: the older regions' ground and sky and what they name, read through
    /// [`Self::legacy_files`]. Every read of this store itself is unchanged, so the world it reads
    /// is not touched. The folder needs only that one file.
    ///
    /// # Errors
    ///
    /// The file is missing, does not open, or is in the later layout.
    pub fn with_legacy_portal(mut self, dir: &Path) -> Result<Self, DatError> {
        let path = PreTodDat::Portal.in_dir(dir);
        let file = shared::open(&path)?;
        if file.era() != ContainerEra::PreTod {
            return Err(DatError::UnexpectedContainerEra {
                path,
                found: file.era(),
                expected: ContainerEra::PreTod,
            });
        }
        self.legacy_portal = Some(file);
        Ok(self)
    }

    /// The files from before Throne of Destiny as a store of their own, for the older regions and
    /// the pictures and objects they name: an older world's own files, or the presentation portal
    /// beside a later world ([`Self::with_legacy_portal`]). `None` for a later world with none
    /// beside it.
    #[must_use]
    pub fn legacy_files(&self) -> Option<Self> {
        if self.era() == ContainerEra::PreTod {
            return Some(self.clone());
        }
        let portal = Arc::clone(self.legacy_portal.as_ref()?);
        Some(Self {
            local: Arc::clone(&portal),
            portal,
            cell: Arc::clone(&self.cell),
            highres: OnceLock::new(),
            later_portal: None,
            legacy_portal: None,
            fallback_portal: None,
            levels_from: None,
            client_dir: None,
        })
    }

    /// The files from Throne of Destiny on as a store of their own, for the later region and the
    /// pictures and objects it names: a later world's own files, or the later files beside an
    /// older world ([`Self::later_files`]). `None` for an older world with none beside it.
    #[must_use]
    pub fn modern_files(&self) -> Option<Self> {
        if self.era() == ContainerEra::Tod {
            return Some(self.clone());
        }
        self.later_files()
    }

    /// The files the world's objects draw with when they take the look of the files of `era`:
    /// that era's portal answers every portal record it holds, and the world's own portal every
    /// record it lacks, so an object the other era never had still draws as the world has it.
    /// Cell and language reads stay the world's, and a later image texture's levels are read
    /// from the later files whichever portal answered the texture. [`Self::era_of`] says which
    /// layout each record is in.
    ///
    /// The other era's files are the presentation portal beside a later world
    /// ([`Self::with_legacy_portal`]) for the files from before Throne of Destiny, and the later
    /// files beside an older world for the later ones. `None` when `era` is the world's own, or
    /// when those files are not here.
    #[must_use]
    pub fn object_files(&self, era: ContainerEra) -> Option<Self> {
        if era == self.era() {
            return None;
        }
        let (other, levels) = match era {
            ContainerEra::PreTod => (self.legacy_portal.as_ref()?, &self.portal),
            ContainerEra::Tod => {
                let later = self.later_portal.as_ref()?;
                (later, later)
            }
        };
        // The high-resolution partition holds later image levels only, so a world that was
        // granted it keeps it for the later textures it still answers.
        let highres = OnceLock::new();
        if let Some(h) = self.highres.get() {
            let _ = highres.set(Arc::clone(h));
        }
        Some(Self {
            portal: Arc::clone(other),
            cell: Arc::clone(&self.cell),
            local: Arc::clone(&self.local),
            highres,
            later_portal: None,
            legacy_portal: None,
            fallback_portal: Some(Arc::clone(&self.portal)),
            levels_from: Some(Arc::clone(levels)),
            client_dir: None,
        })
    }

    /// A store from files already open. A `highres` handed in here counts as granted, which is
    /// what a test that opened the file on purpose means by it.
    ///
    /// These files are **not** entered into [`shared`]'s table: the caller opened them itself and
    /// may have opened them for a reason (a copy, a fixture, a file it is about to patch), so a
    /// later `open_dir` of the same path must not be handed them.
    #[must_use]
    pub fn open_with(
        portal: DatFile,
        cell: DatFile,
        local: DatFile,
        highres: Option<DatFile>,
    ) -> Self {
        let lock = OnceLock::new();
        if let Some(h) = highres {
            let _ = lock.set(Arc::new(h));
        }
        Self {
            portal: Arc::new(portal),
            cell: Arc::new(cell),
            local: Arc::new(local),
            highres: lock,
            later_portal: None,
            legacy_portal: None,
            fallback_portal: None,
            levels_from: None,
            client_dir: None,
        }
    }

    /// Open `client_highres.dat` beside the other files and
    /// serve portal-type reads from it first. Idempotent; the
    /// client never closes the file once opened. Returns whether the store now has it -- `false`
    /// when the directory has no such file (ACE without the dat), or for a store built from
    /// files without one.
    ///
    /// # Errors
    ///
    /// The file exists and does not open.
    pub fn grant_highres(&self) -> Result<bool, DatError> {
        if self.highres.get().is_some() {
            return Ok(true);
        }
        let Some(dir) = &self.client_dir else {
            return Ok(false);
        };
        let path = RetailDat::HighRes.in_dir(dir);
        if !path.is_file() {
            return Ok(false);
        }
        let file = shared::open(&path)?;
        // A concurrent grant may have won; either file is the same file.
        let _ = self.highres.set(file);
        Ok(true)
    }

    /// Whether [`Self::grant_highres`] has opened the high-res dat -- the client's
    /// "is the high-res controller open" that asks
    /// through the asset cache (`0x69466948`, the `HiFi` subset).
    #[must_use]
    pub fn highres_granted(&self) -> bool {
        self.highres.get().is_some()
    }

    /// What is missing from `dir`, as one line, or `None` when it is complete.
    ///
    /// Takes the directory rather than finding one, so a caller can hand it a directory it knows
    /// is empty and see it answer "missing".
    #[must_use]
    pub fn shortfall_in(dir: &Path) -> Option<String> {
        let missing: Vec<&str> = Self::REQUIRED_DATS
            .iter()
            .copied()
            .filter(|n| !dir.join(n).is_file())
            .collect();
        if missing.is_empty() {
            return None;
        }
        Some(format!(
            "retail dats not found: {} under {}",
            missing.join(", "),
            dir.display()
        ))
    }

    /// Re-walk every open file's directory, for a store whose files a
    /// [`crate::write::DatWriter`] has patched.
    ///
    /// Takes `&mut self`, which is the honest signature and the one that names the problem: the
    /// client holds this store behind an `Arc` that many owners have cloned, so invalidating it
    /// means *replacing the `Arc`*, not mutating through it. `dereth_client::ddd` says which owners
    /// pick the replacement up and which do not.
    ///
    /// A container is shared, so reloading does not re-walk in place: it opens a new one and puts
    /// it in this handle's slot, which is what [`DatFile::reload`] does internally anyway
    /// (`*self = Self::open(&self.path)?`). Two consequences follow and both are wanted:
    /// another handle on the same file keeps the old view until it reloads too -- as it would if
    /// every handle had its own file -- and the fresh container
    /// is pushed into [`shared`]'s table, so the next `open_dir` of that directory sees the patch
    /// rather than the pre-patch walk.
    ///
    /// # Errors
    ///
    /// The first [`DatFile::reload`] that fails, with the remaining files left alone. A file that
    /// fails to reload keeps its old directory (see [`DatFile::reload`]), so the store is stale
    /// rather than broken.
    pub fn reload(&mut self) -> Result<(), DatError> {
        reload_one(&mut self.portal)?;
        reload_one(&mut self.cell)?;
        reload_one(&mut self.local)?;
        if let Some(hi) = self.highres.get_mut() {
            reload_one(hi)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn portal(&self) -> &DatFile {
        &self.portal
    }

    #[must_use]
    pub fn cell(&self) -> &DatFile {
        &self.cell
    }

    #[must_use]
    pub fn local(&self) -> &DatFile {
        &self.local
    }

    /// The high-res dat, once granted. `None` until [`Self::grant_highres`].
    #[must_use]
    pub fn highres(&self) -> Option<&DatFile> {
        self.highres.get().map(|f| &**f)
    }

    /// The file a `DatKind` names, if this store has it open.
    #[must_use]
    pub fn file(&self, kind: DatKind) -> Option<&DatFile> {
        match kind {
            DatKind::Portal => Some(&self.portal),
            DatKind::Cell => Some(&self.cell),
            DatKind::Local => Some(&self.local),
            DatKind::None => None,
        }
    }

    /// Read an id whose type is already known, which is what the client always has: every request
    /// is a `QualifiedDataID`.
    pub fn read_typed(&self, kind: DbType, id: DataId) -> Result<Vec<u8>, DatError> {
        match kind.dat() {
            DatKind::Portal if kind == DbType::RenderSurface && self.levels_from.is_some() => {
                if let Some(hi) = self.highres.get().filter(|h| h.contains(id)) {
                    return hi.read(id);
                }
                self.levels_from
                    .as_ref()
                    .map_or(Err(DatError::NotFound(id)), |f| f.read(id))
            }
            DatKind::Portal => self.read_portal(id),
            DatKind::Cell => self.cell.read(id),
            DatKind::Local => self.local.read(id),
            DatKind::None => Err(DatError::NotFound(id)),
        }
    }

    /// The portal-type path, high-res file first.
    pub fn read_portal(&self, id: DataId) -> Result<Vec<u8>, DatError> {
        if let Some(hi) = self.highres.get() {
            if hi.contains(id) {
                return hi.read(id);
            }
        }
        if let Some(later) = &self.later_portal {
            if !self.portal.contains(id) && later.contains(id) {
                return later.read(id);
            }
        }
        if let Some(world) = &self.fallback_portal {
            if !self.portal.contains(id) && world.contains(id) {
                return world.read(id);
            }
        }
        self.portal.read(id)
    }

    /// Read a cell-dat record.
    pub fn read_cell(&self, id: DataId) -> Result<Vec<u8>, DatError> {
        self.cell.read(id)
    }

    /// Resolve an id with no type in hand.
    ///
    /// The client never has to do this — every request carries its type — so this is a convenience
    /// with one documented ambiguity: cell ids are landblock-derived and can collide with a portal
    /// range (`0x0100FFFF` is landblock (1,0) and also lies inside `GFXOBJ`'s range). Portal and
    /// local membership is therefore checked first and the cell dat is the fallback. Cell consumers
    /// should call [`RetailDatStore::read_cell`].
    #[must_use]
    pub fn resolve(&self, id: DataId) -> Option<(DbType, DatKind)> {
        if let Some(t) = divine_type(id) {
            let kind = t.dat();
            if let Some(f) = self.file(kind) {
                let member = match kind {
                    DatKind::Portal => {
                        f.contains(id)
                            || self.highres.get().is_some_and(|h| h.contains(id))
                            || self.later_portal.as_ref().is_some_and(|l| l.contains(id))
                            || self
                                .fallback_portal
                                .as_ref()
                                .is_some_and(|l| l.contains(id))
                    }
                    _ => f.contains(id),
                };
                if member {
                    return Some((t, kind));
                }
            }
        }
        if self.cell.contains(id) {
            return classify_cell_id(id).map(|t| (t, DatKind::Cell));
        }
        None
    }

    /// Every id of a `DbType`, ascending. For portal types the high-res partition is included.
    #[must_use]
    pub fn ids_of(&self, kind: DbType) -> Vec<DataId> {
        let mut v: Vec<DataId> =
            match kind.dat() {
                DatKind::Portal => {
                    let mut ids: Vec<DataId> = self
                        .portal
                        .iter_ids()
                        .filter(|i| divine_type(*i) == Some(kind))
                        .collect();
                    if let Some(hi) = self.highres.get() {
                        ids.extend(hi.iter_ids().filter(|i| divine_type(*i) == Some(kind)));
                    }
                    if let Some(later) = &self.later_portal {
                        ids.extend(later.iter_ids().filter(|i| {
                            divine_type(*i) == Some(kind) && !self.portal.contains(*i)
                        }));
                    }
                    ids
                }
                DatKind::Local => self
                    .local
                    .iter_ids()
                    .filter(|i| divine_type(*i) == Some(kind))
                    .collect(),
                DatKind::Cell => self
                    .cell
                    .iter_ids()
                    .filter(|i| classify_cell_id(*i) == Some(kind))
                    .collect(),
                DatKind::None => Vec::new(),
            };
        v.sort_unstable();
        v
    }

    /// Sanity-check the four headers against the values the client expects.
    /// Expected values come from the container header format.
    #[must_use]
    pub fn headers_match_retail(&self) -> bool {
        let p = self.portal.header();
        let c = self.cell.header();
        let l = self.local.header();
        p.data_set == PORTAL_DATFILE
            && p.data_subset == 0
            && p.block_size == 0x400
            && p.master_map_id == 0x2500_0000
            && c.data_set == CELL_DATFILE
            && c.data_subset == 1
            && c.block_size == 0x100
            && l.data_set == LOCAL_DATFILE
            && l.data_subset == 1
            && l.block_size == 0x400
            && self.highres.get().is_none_or(|h| {
                h.header().data_set == PORTAL_DATFILE && h.header().data_subset == HIRES_SUBSET
            })
    }
}

/// One slot of [`RetailDatStore::reload`]: walk the file again and swap the result in.
///
/// The old container is left in place when the open fails, which is [`DatFile::reload`]'s
/// contract word for word -- "a failed reload degrades to a stale reader rather than to no reader
/// at all" -- and the reason the `?` is on a temporary rather than on the slot.
fn reload_one(slot: &mut Arc<DatFile>) -> Result<(), DatError> {
    let fresh = Arc::new(DatFile::open(slot.path())?);
    shared::replace(fresh.path(), &fresh);
    *slot = fresh;
    Ok(())
}

/// The `DbType`s a seam [`DataType`] stands for. `DataType` is the shared vocabulary and names only
/// what other crates consume; anything it does not name yields an empty set.
#[must_use]
pub fn db_types_for(kind: DataType) -> Vec<DbType> {
    const ALL: &[DbType] = &[
        DbType::LandBlock,
        DbType::Lbi,
        DbType::Cell,
        DbType::GfxObj,
        DbType::Setup,
        DbType::Anim,
        DbType::Palette,
        DbType::SurfaceTexture,
        DbType::RenderSurface,
        DbType::Surface,
        DbType::MTable,
        DbType::Wave,
        DbType::Environment,
        DbType::PalSet,
        DbType::Clothing,
        DbType::DegradeInfo,
        DbType::Scene,
        DbType::Region,
        DbType::STable,
        DbType::ParticleEmitter,
        DbType::PhysicsScript,
        DbType::PhysicsScriptTable,
    ];
    ALL.iter()
        .copied()
        .filter(|t| t.seam_type() == Some(kind))
        .collect()
}

impl AssetSource for RetailDatStore {
    fn read(&self, id: DataId) -> Result<Vec<u8>, AssetError> {
        let (kind, _) = self.resolve(id).ok_or(AssetError::NotFound(id))?;
        Ok(self.read_typed(kind, id)?)
    }

    fn exists(&self, id: DataId) -> bool {
        self.resolve(id).is_some()
    }

    fn container_era(&self) -> ContainerEra {
        self.era()
    }

    fn container_era_of(&self, id: DataId) -> ContainerEra {
        self.era_of(id)
    }

    fn iter_type(&self, kind: DataType) -> Box<dyn Iterator<Item = DataId> + '_> {
        let mut ids: Vec<DataId> = db_types_for(kind)
            .into_iter()
            .flat_map(|t| self.ids_of(t))
            .collect();
        ids.sort_unstable();
        Box::new(ids.into_iter())
    }
}
