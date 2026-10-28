//! Import: any modified set of data files taken apart against the base it was made from, into
//! the overlay that reads as it when laid over the base.
//!
//! Bytes decide what changed ([`dereth_dat::decompose`]): a record the world adds or changes goes
//! into the overlay, one it removes is a deletion (a cell-file landblock the world keeps nothing of
//! is one deletion of the whole landblock). By default every change goes into one revision, one
//! past the base's iterations; a world that numbers its iterations its own way keeps its own list
//! and each record's own iteration instead ([`decompose::Iterations::World`]). A world lays an
//! imported folder under its packs (`imports` in its world file), so another editor's output and
//! the world's packs compile into the one overlay.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_dat::overlay::{self, OverlayDir};
use dereth_dat::{decompose, DatFile, FileSet, ModernDat, ITERATION_LIST};

use crate::error::Error;

/// What an import found and wrote, for one file.
#[derive(Clone, Debug)]
pub struct ImportedFile {
    pub file: ModernDat,
    /// The ids added, changed and removed.
    pub diff: decompose::Diff,
    /// The container written, and its highest iteration; `None` when the file is its base.
    pub written: Option<(PathBuf, u32)>,
    /// With `verify`: how many records were read back through the overlay as the world's.
    pub verified: Option<usize>,
}

/// `name` in `dir`, matched without regard to case.
fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let n = e.file_name().to_string_lossy().into_owned();
        (n.eq_ignore_ascii_case(name) && e.path().is_file()).then(|| e.path())
    })
}

/// Lay the container at `path` over `base` and check it reads as `world`: as many records, each
/// of the world's read as the world's, and with the world's own iterations, its iteration list.
fn verify(base: &DatFile, world: &DatFile, path: &Path, key: &str) -> Result<usize, Error> {
    let layer = overlay::Layer::over(base, DatFile::open(path)?, Some(key))?;
    let exact = layer.manifest().exact_iterations;
    let layered = base.layered(Arc::new(layer));
    let fail = |why: String| Error::Dat(format!("{}: {why}", path.display()));
    if layered.len() != world.len() {
        return Err(fail(format!(
            "{} records through the overlay, the world has {}",
            layered.len(),
            world.len()
        )));
    }
    let mut n = 0;
    for id in world.iter_ids() {
        if id == ITERATION_LIST {
            continue;
        }
        if layered.read(id).ok() != world.read(id).ok() {
            return Err(fail(format!(
                "{:#010X} does not read as the world's",
                id.raw()
            )));
        }
        n += 1;
    }
    if exact && layered.iteration_list().ok() != world.iteration_list().ok() {
        return Err(fail("the iterations are not the world's".into()));
    }
    Ok(n)
}

/// The names the file `t` has on disk in any of `sets`, in their order.
fn names(t: ModernDat, sets: &[FileSet]) -> Vec<&'static str> {
    sets.iter().filter_map(|s| s.name(t)).collect()
}

/// Take the files in `world` apart against the files in `base`, writing an overlay folder at
/// `out` for the world `key`. Each file present in both, the four later files or `portal.dat`
/// and `cell.dat` from before Throne of Destiny, is compared; an overlay already in `out` is
/// replaced file by file.
///
/// # Errors
/// `out` holding data files of its own, files in different layouts, and the reader's and the
/// writer's errors.
pub fn import(
    base: &Path,
    world: &Path,
    out: &Path,
    key: &str,
    iterations: decompose::Iterations,
    check: bool,
) -> Result<Vec<ImportedFile>, Error> {
    import_sets(
        base,
        world,
        out,
        key,
        iterations,
        check,
        &[FileSet::MODERN, FileSet::CLASSIC],
    )
}

/// [`import`] over files of any of `sets`: each role's file, under its name in the first of the
/// sets whose file both folders hold.
///
/// # Errors
/// As [`import`].
pub fn import_sets(
    base: &Path,
    world: &Path,
    out: &Path,
    key: &str,
    iterations: decompose::Iterations,
    check: bool,
    sets: &[FileSet],
) -> Result<Vec<ImportedFile>, Error> {
    let dir = OverlayDir::new(out)?;
    let mut done = Vec::new();
    for t in ModernDat::ALL {
        let Some((name, b, w)) = names(t, sets).into_iter().find_map(|n| {
            let (b, w) = (find(base, n)?, find(world, n)?);
            Some((n, b, w))
        }) else {
            continue;
        };
        let open =
            |p: &Path| DatFile::open(p).map_err(|e| Error::Dat(format!("{}: {e}", p.display())));
        let (bf, wf) = (open(&b)?, open(&w)?);
        if bf.era() != wf.era() {
            return Err(Error::Refused(format!(
                "{} and {} are in different layouts",
                b.display(),
                w.display()
            )));
        }
        let diff = decompose::diff(&bf, &wf)?;
        let mut file = ImportedFile {
            file: t,
            diff,
            written: None,
            verified: None,
        };
        if !file.diff.is_empty() {
            let path = dir.container(t);
            let cell = t == ModernDat::Cell;
            let revision =
                decompose::write(&bf, &wf, &file.diff, &path, name, key, cell, iterations)?;
            if check {
                file.verified = Some(verify(&bf, &wf, &path, key)?);
            }
            file.written = Some((path, revision));
        }
        done.push(file);
    }
    Ok(done)
}
