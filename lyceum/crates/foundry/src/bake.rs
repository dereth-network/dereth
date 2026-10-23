//! Bake: a world's overlay written into a copy of a player's own data files, for a client or a
//! server that reads only its own files.
//!
//! The player's files are only read. Each file the overlay changes is copied into the folder the
//! player names, the overlay is written into the copy with the same writer the retail client's
//! patching follows (its deletions first, then every record with its own version, date and
//! iteration, then the iteration list), and the copy is checked before it takes the file's name:
//!
//! - no block of the file is reached twice, and the directory's tree is sound;
//! - the iteration list reads back as the base's with the overlay's revisions, one run from 1;
//! - every record of the overlay reads back byte for byte, and every record it deletes is gone;
//! - the file holds exactly the records the base with the overlay over it holds;
//! - the overlay's records pass the checks ([`crate::validate`]).
//!
//! The files the overlay does not change are copied as they are, unless only the changed ones are
//! asked for. Where the folder is, and what else is in it, is the player's business. A file
//! `dereth-bake.toml` beside the copies names the world, the base and the overlay they came from.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use dereth_dat::overlay::{self, Layer, OverlayDir};
use dereth_dat::{iteration, DatFile, DatWriter, SaveOutcome, ITERATION_LIST};
use dereth_primitives::DataId;

use crate::dats::DatSet;
use crate::error::Error;
use crate::validate::{self, Scope};

/// What a bake wrote.
#[derive(Clone, Debug, Default)]
pub struct Baked {
    /// Each file the overlay changed: its path, the records written and the records deleted.
    pub written: Vec<(PathBuf, usize, usize)>,
    /// The files copied as they are.
    pub copied: Vec<PathBuf>,
    /// The world the overlay belongs to.
    pub world_key: String,
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Write the overlay's records and deletions into the writer, and its iteration list.
fn apply(w: &mut DatWriter, layer: &Layer, iterations: &[u32], date: u32) -> Result<(), Error> {
    for t in layer.tombstones() {
        if t.mask == 0 {
            w.delete_data(DataId(t.id), 0)?;
        } else {
            w.delete_data_by_mask(DataId(t.id), t.mask)?;
        }
    }
    for (id, e) in layer.records() {
        let bytes = layer.read(id)?;
        let version = u16::try_from(e.bits >> 16).unwrap_or(1).max(1);
        if w.save(id, &bytes, version, e.iteration, e.date)? == SaveOutcome::RefusedOlderIteration {
            // A world's own numbering may run below the base's: the world's record still wins.
            w.delete_data(id, 0)?;
            w.save(id, &bytes, version, e.iteration, e.date)?;
        }
    }
    w.save(ITERATION_LIST, &iteration::encode(iterations), 1, 0, date)?;
    Ok(())
}

/// Check the baked copy at `path` against the base with the overlay over it.
fn check(path: &Path, layered: &DatFile, layer: &Layer, iterations: &[u32]) -> Result<(), Error> {
    let fail = |why: String| Error::Dat(format!("{}: {why}", path.display()));
    let mut w = DatWriter::open(path)?;
    let audit = w.audit()?;
    if !audit.is_sound() {
        return Err(fail(format!(
            "{} block(s) are reached twice",
            audit.double_allocated.len()
        )));
    }
    drop(w);
    let baked = DatFile::open(path)?;
    let structure = baked.verify_structure()?;
    if !structure.is_sound() {
        return Err(fail("its directory is not a sound tree".into()));
    }
    if baked.iteration_list()? != iterations {
        return Err(fail("its iteration list is not the world's".into()));
    }
    for (id, _) in layer.records() {
        if baked.read(id)? != layer.read(id)? {
            return Err(fail(format!(
                "{:#010X} does not read back as written",
                id.raw()
            )));
        }
    }
    let want: BTreeSet<u32> = layered.iter_ids().map(DataId::raw).collect();
    let have: BTreeSet<u32> = baked.iter_ids().map(DataId::raw).collect();
    if let Some(id) = want.symmetric_difference(&have).next() {
        return Err(fail(format!(
            "it holds {} record(s) where the world holds {}; {id:#010X} differs",
            have.len(),
            want.len()
        )));
    }
    Ok(())
}

fn file_hash(path: &Path) -> Result<String, Error> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    Ok(overlay::hex(&overlay::record_hash(&bytes)))
}

/// Bake the overlay in `overlay_dir` into copies of the files in `dats`, in `to`. With
/// `touched_only`, the files the overlay does not change are not copied.
///
/// # Errors
/// [`Error::Refused`] when `to` is the files' own folder; an overlay made against other files; a
/// copy that fails a check (nothing is renamed then); and the writer's errors.
pub fn bake(
    overlay_dir: &Path,
    dats: &Path,
    to: &Path,
    touched_only: bool,
) -> Result<Baked, Error> {
    if same_dir(dats, to) {
        return Err(Error::Refused(format!(
            "{} is the files' own folder; a bake writes copies somewhere else",
            to.display()
        )));
    }
    let base = DatSet::open(dats)?;
    if base.layout() == dereth_dat::ContainerEra::Classic {
        return Err(Error::Refused(format!(
            "{} holds {}; a bake writes only the end-of-retail files",
            dats.display(),
            crate::dats::files_of(base.era)
        )));
    }
    let od = OverlayDir::new(overlay_dir)?;
    let layered = base.with_overlay(overlay_dir, None)?;
    // The overlay's own records must pass the checks before a client sees them.
    let mut ids = std::collections::BTreeMap::new();
    let mut deletes = false;
    for (t, f) in layered.files() {
        if let Some(l) = f.layer() {
            deletes |= !l.tombstones().is_empty();
            ids.insert(
                t,
                l.records().map(|(id, _)| id.raw()).collect::<BTreeSet<_>>(),
            );
        }
    }
    let scope = if deletes {
        Scope::All
    } else {
        Scope::Only(ids)
    };
    let report = validate::validate(&layered, &scope);
    if !report.problems.is_empty() {
        return Err(Error::Invalid(report.problems));
    }
    std::fs::create_dir_all(to).map_err(|e| Error::io(to, e))?;
    let mut baked = Baked {
        world_key: od.world_key().unwrap_or_default(),
        ..Baked::default()
    };
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut manifest = format!(
        "# Written by lyceum bake: these files are the base below with the world's overlay.\n\
         world = {:?}\n",
        baked.world_key
    );
    let result = (|| -> Result<(), Error> {
        for (t, f) in layered.files() {
            let source = t.in_dir(dats);
            let target = t.in_dir(to);
            let staging = to.join(format!("{}.baking", t.file_name()));
            match f.layer() {
                Some(layer) => {
                    std::fs::copy(&source, &staging).map_err(|e| Error::io(&staging, e))?;
                    staged.push((staging.clone(), target.clone()));
                    let iterations = layer.iterations().to_vec();
                    let date = base
                        .file(t)
                        .and_then(|b| b.base_entry(ITERATION_LIST).map(|e| e.date))
                        .unwrap_or(0);
                    let mut w = DatWriter::open(&staging)?;
                    apply(&mut w, layer, &iterations, date)?;
                    drop(w);
                    check(&staging, f, layer, &iterations)?;
                    baked
                        .written
                        .push((target, layer.records().count(), layer.tombstones().len()));
                    let _ = writeln!(
                        manifest,
                        "\n[{}]\nbase = \"records {}\"\noverlay = {:?}\niterations = {}",
                        t.file_name(),
                        overlay::hex(&layer.manifest().base_records.unwrap_or_default()),
                        file_hash(layer.path())?,
                        iterations.len()
                    );
                }
                None if touched_only => {}
                None => {
                    std::fs::copy(&source, &staging).map_err(|e| Error::io(&staging, e))?;
                    staged.push((staging, target.clone()));
                    baked.copied.push(target);
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        for (staging, _) in &staged {
            let _ = std::fs::remove_file(staging);
        }
        return Err(e);
    }
    for (staging, target) in &staged {
        if target.exists() {
            std::fs::remove_file(target).map_err(|e| Error::io(target, e))?;
        }
        std::fs::rename(staging, target).map_err(|e| Error::io(target, e))?;
    }
    let path = to.join("dereth-bake.toml");
    std::fs::write(&path, manifest).map_err(|e| Error::io(&path, e))?;
    Ok(baked)
}
