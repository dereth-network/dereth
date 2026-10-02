//! The content overlay: an editable store beside the immutable
//! `world.pack`, read before the pack behind the [`crate::WorldDatabase`] API.
//!
//! Not ACE-derived. ACE's developer content commands (`import-sql`, `import-json`, `createinst`,
//! …) write SQL files to the content folder and run them against the MySQL world database; here
//! they run against the overlay instead ([`ContentOverlay::import_sql`]):
//!
//! 1. The first write loads the base inputs `world.pack` was built from (the dump, then any
//!    patches) into the import SQL store and replays the overlay's journal, so a write has exactly the
//!    semantics of a content patch (see [`author`]).
//! 2. The file runs on the store; the pack records it touched are laid out again as `empyrean-import`
//!    lays them out ([`author::Author::materialize`]).
//! 3. The file and those records are committed to the overlay file in one transaction
//!    ([`file`](mod@file)), and the in-memory [`layer::Layer`] the world database reads is updated.
//!
//! A record the overlay rewrote replaces the pack's; one it deleted hides the pack's; one it added
//! is added. Nothing is cached here: the world database's caches are cleared by the commands,
//! exactly where ACE clears them. [`ContentOverlay::publish`] bakes the base inputs plus the
//! journal into a new pack, byte-identical to `empyrean-import` over the same inputs and the
//! journal's files ([`ContentOverlay::export_journal`]). The pack the server runs on is never
//! rewritten.

pub mod author;
pub mod file;
pub mod layer;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard};

use empyrean_common::dotnet::DotNetDateTime;

use crate::error::{ImportError, PackError};
use crate::import::patch::{self, Applied, Input, InputKind, Source};
use crate::import::{build_from_for, Imported};
use crate::pack::{hex, PackHeader};

use author::Author;
pub use author::BaseInputs;
use file::{JournalEntry, Meta, OverlayFile};
use layer::Layer;

/// Everything that can go wrong with an overlay.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OverlayError {
    #[error("overlay file: {0}")]
    Sqlite(String),
    /// The overlay does not belong to this pack, or its file is not an overlay.
    #[error("{0}")]
    Mismatch(String),
    /// A content file failed (the overlay is unchanged).
    #[error(transparent)]
    Import(#[from] ImportError),
    #[error(transparent)]
    Pack(#[from] PackError),
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The default clock of a new overlay: the fixed 2000-01-01 00:00:00 of
/// [`crate::import::default_now`], so a build is reproducible. No environment variable changes it
/// (`SOURCE_DATE_EPOCH` is not read).
#[must_use]
pub fn default_now() -> DotNetDateTime {
    crate::import::default_now()
}

/// Parse `yyyy-MM-dd HH:mm:ss`.
fn parse_now(s: &str) -> Option<DotNetDateTime> {
    let n = |r: std::ops::Range<usize>| s.get(r).and_then(|x| x.parse::<i32>().ok());
    if s.len() != 19 {
        return None;
    }
    Some(DotNetDateTime::new_hms(
        n(0..4)?,
        n(5..7)?,
        n(8..10)?,
        n(11..13)?,
        n(14..16)?,
        n(17..19)?,
    ))
}

/// An open overlay over one pack.
#[derive(Debug)]
pub struct ContentOverlay {
    layer: RwLock<Layer>,
    file: Mutex<OverlayFile>,
    author: Mutex<Option<Author>>,
    base: BaseInputs,
    now: DotNetDateTime,
    dataset_id: [u8; 16],
}

impl ContentOverlay {
    /// Open (or create) the overlay file at `path` over the pack whose header is `pack`. A new
    /// overlay records the pack's hash and `now`; an existing one must have been made over the
    /// same pack.
    pub fn open(
        path: &Path,
        base: BaseInputs,
        pack: &PackHeader,
        now: DotNetDateTime,
    ) -> Result<Self, OverlayError> {
        let mut file = OverlayFile::open(path)?;
        let content_hash = hex(&pack.content_hash);
        let dataset_id = hex(&pack.dataset_id);
        let now = match file.meta()? {
            None => {
                file.init(&Meta {
                    content_hash,
                    dataset_id,
                    now: patch::datetime_text(now),
                })?;
                now
            }
            Some(m) => {
                if m.content_hash != content_hash {
                    return Err(OverlayError::Mismatch(format!(
                        "the overlay {} was made over world.pack {}, but the server runs {content_hash}; \
                         publish it over its own pack or start a new overlay",
                        path.display(),
                        m.content_hash
                    )));
                }
                parse_now(&m.now).ok_or_else(|| {
                    OverlayError::Mismatch(format!("{}: bad `now` {}", path.display(), m.now))
                })?
            }
        };
        let layer = file.layer()?;
        Ok(Self {
            layer: RwLock::new(layer),
            file: Mutex::new(file),
            author: Mutex::new(None),
            base,
            now,
            dataset_id: pack.dataset_id,
        })
    }

    /// The records as the world database reads them.
    pub fn layer(&self) -> RwLockReadGuard<'_, Layer> {
        self.layer
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[must_use]
    pub fn path(&self) -> PathBuf {
        lock(&self.file).path().to_owned()
    }

    #[must_use]
    pub fn base(&self) -> &BaseInputs {
        &self.base
    }

    /// The clock that stands in for `CURRENT_TIMESTAMP`.
    #[must_use]
    pub fn now(&self) -> DotNetDateTime {
        self.now
    }

    /// Every file applied so far, in order.
    pub fn journal(&self) -> Result<Vec<JournalEntry>, OverlayError> {
        lock(&self.file).journal()
    }

    /// ACE's `ImportSQL` against the overlay: run one file of SQL, then commit it and the records
    /// it changed. A file that fails changes nothing (the error names the file and line).
    pub fn import_sql(&self, sql: &[u8], shown: &str) -> Result<Applied, OverlayError> {
        let mut author = lock(&self.author);
        if author.is_none() {
            let journal = self.journal()?;
            *author = Some(Author::load(
                &self.base,
                self.now,
                &self.dataset_id,
                &journal,
            )?);
        }
        let a = author.as_mut().expect("loaded above");
        let applied = match a.apply(sql, shown) {
            Ok(applied) => applied,
            Err(e) => {
                // Part of the file may have run: reload from the journal next time.
                *author = None;
                return Err(e.into());
            }
        };
        let records = match a.materialize(&applied.touched) {
            Ok(r) => r,
            Err(e) => {
                *author = None;
                return Err(e.into());
            }
        };
        if let Err(e) = lock(&self.file).commit(shown, sql, &applied.hash, &records) {
            *author = None;
            return Err(e);
        }
        let mut layer = self
            .layer
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (t, k, d) in records {
            layer.put(t, k, d);
        }
        Ok(applied)
    }

    /// The journal's files, as `empyrean-import` inputs would name them.
    fn journal_sources(&self) -> Result<Vec<Source>, OverlayError> {
        Ok(self
            .journal()?
            .into_iter()
            .map(|e| Source {
                kind: InputKind::Sql,
                path: PathBuf::from(e.path),
                bytes: e.sql,
            })
            .collect())
    }

    /// Bake the base inputs and the journal into pack bytes: `empyrean-import --sql <base> [base
    /// patches] --patches <journal files>`, the same function, so the same bytes.
    pub fn build(&self) -> Result<(Vec<u8>, Imported), OverlayError> {
        let dump =
            std::fs::File::open(&self.base.sql).map_err(|e| ImportError::io(&self.base.sql, e))?;
        let journal = self.journal_sources()?;
        let sources = patch::sources(&self.base.patches).chain(journal.into_iter().map(Ok));
        Ok(build_from_for(dump, sources, self.now, self.base.era)?)
    }

    /// Publish: [`publish`] of this overlay's file over its base inputs.
    pub fn publish(&self, out: &Path, report: Option<&Path>) -> Result<Imported, OverlayError> {
        publish(&self.base, &self.path(), out, report, None)
    }

    /// Write the journal as numbered SQL files into `dir` (see [`export_journal`]).
    pub fn export_journal(&self, dir: &Path) -> Result<Vec<PathBuf>, OverlayError> {
        export_journal(&self.journal()?, dir)
    }

    /// `empyrean-import` inputs for the base patches plus an exported journal directory.
    #[must_use]
    pub fn inputs_with(&self, journal_dir: &Path) -> Vec<Input> {
        let mut v = self.base.patches.clone();
        v.push(Input {
            kind: InputKind::Sql,
            path: journal_dir.to_owned(),
        });
        v
    }
}

impl ContentOverlay {
    /// Wrap for sharing.
    #[must_use]
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }
}

/// Write `journal` as numbered SQL files into `dir` (`00000001.sql`, …), in order: the patch files
/// that, after the base inputs, rebuild what [`publish`] builds (`empyrean-import --patches <dir>`).
pub fn export_journal(journal: &[JournalEntry], dir: &Path) -> Result<Vec<PathBuf>, OverlayError> {
    std::fs::create_dir_all(dir).map_err(|e| ImportError::io(dir, e))?;
    let mut out = Vec::new();
    for e in journal {
        let p = dir.join(format!("{:08}.sql", e.seq));
        std::fs::write(&p, &e.sql).map_err(|err| ImportError::io(&p, err))?;
        out.push(p);
    }
    Ok(out)
}

/// Read an existing overlay file: its meta and journal.
pub fn read_overlay(overlay: &Path) -> Result<(Meta, Vec<JournalEntry>), OverlayError> {
    if !overlay.is_file() {
        return Err(OverlayError::Mismatch(format!(
            "{} does not exist",
            overlay.display()
        )));
    }
    let file = OverlayFile::open(overlay)?;
    let meta = file.meta()?.ok_or_else(|| {
        OverlayError::Mismatch(format!("{} is not an overlay (no meta)", overlay.display()))
    })?;
    Ok((meta, file.journal()?))
}

/// `empyrean-import --overlay`: build the base inputs, then the overlay's journal, into pack bytes
/// with the overlay's clock. The base must be the one the overlay was made over: the dataset id
/// the base inputs alone give is checked against the overlay's (nothing is returned otherwise).
pub fn build_with_overlay(
    base: &BaseInputs,
    overlay: &Path,
) -> Result<(Vec<u8>, Imported, Meta, Vec<JournalEntry>), OverlayError> {
    let (meta, journal) = read_overlay(overlay)?;
    let now = parse_now(&meta.now).ok_or_else(|| {
        OverlayError::Mismatch(format!("{}: bad `now` {}", overlay.display(), meta.now))
    })?;
    let base_hash = {
        let mut h = blake3::Hasher::new();
        let mut f = std::fs::File::open(&base.sql).map_err(|e| ImportError::io(&base.sql, e))?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n =
                std::io::Read::read(&mut f, &mut buf).map_err(|e| ImportError::io(&base.sql, e))?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        *h.finalize().as_bytes()
    };
    let dump = std::fs::File::open(&base.sql).map_err(|e| ImportError::io(&base.sql, e))?;
    let entries = journal.iter().map(|e| {
        Ok(Source {
            kind: InputKind::Sql,
            path: PathBuf::from(&e.path),
            bytes: e.sql.clone(),
        })
    });
    let (bytes, imported) = build_from_for(
        dump,
        patch::sources(&base.patches).chain(entries),
        now,
        base.era,
    )?;
    let n_base = imported.applied.len() - journal.len();
    let base_id = if n_base == 0 {
        let mut id = [0u8; 16];
        id.copy_from_slice(&base_hash[..16]);
        id
    } else {
        patch::dataset_id(
            &base_hash,
            &patch::datetime_text(now),
            &imported.applied[..n_base],
        )
    };
    if hex(&base_id) != meta.dataset_id {
        return Err(OverlayError::Mismatch(format!(
            "the base inputs build dataset {}, but the overlay {} was made over dataset {}",
            hex(&base_id),
            overlay.display(),
            meta.dataset_id
        )));
    }
    Ok((bytes, imported, meta, journal))
}

/// Publish an overlay: [`build_with_overlay`], written to `out` with its report (and, with
/// `journal_dir`, the journal exported as patch files beside it). Refuses to write over the pack
/// the overlay sits on: the server never rewrites the pack it runs on.
pub fn publish(
    base: &BaseInputs,
    overlay: &Path,
    out: &Path,
    report: Option<&Path>,
    journal_dir: Option<&Path>,
) -> Result<Imported, OverlayError> {
    let (bytes, imported, meta, journal) = build_with_overlay(base, overlay)?;
    if let Ok(existing) = crate::pack::Pack::open(out) {
        if hex(&existing.header().content_hash) == meta.content_hash {
            return Err(OverlayError::Mismatch(format!(
                "{} is the pack the overlay sits on; publish to a new file",
                out.display()
            )));
        }
    }
    crate::import::write_pack_and_report(&bytes, &imported, out, report)?;
    if let Some(dir) = journal_dir {
        export_journal(&journal, dir)?;
    }
    Ok(imported)
}
