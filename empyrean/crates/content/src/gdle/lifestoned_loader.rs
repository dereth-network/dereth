// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Adapter/Lifestoned/LifestonedLoader.cs
//! `LifestonedLoader`: Lifestoned JSON weenie files read into `LSDWeenie`s, and the `...Converted`
//! variants that turn them into World weenies through `LifestonedConverter.TryConvert`.
//! `AppendMetadata` (the export side) is in [`crate::export::json::lifestoned`].
//!
//! As in [`super::loader`], a C# `false` is an `Err`, and a JSON `null` document loads as `None`.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::loader::read_all_text;
use crate::import::json::lifestoned;
use crate::import::json::models::LsdWeenie;
use crate::import::json::value::{self, R};
use crate::import::json::world::Weenie;
use crate::import::patch::{expand, Input, InputKind};

/// `Directory.GetFiles(folder, "*.json", SearchOption.AllDirectories)`, in path order.
fn json_files(folder: &Path) -> R<Vec<PathBuf>> {
    let meta = std::fs::metadata(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    if !meta.is_dir() {
        return Err(format!("{} is not a directory", folder.display()));
    }
    expand(&Input {
        kind: InputKind::Json,
        path: folder.to_path_buf(),
    })
    .map_err(|e| e.to_string())
}

/// `new FileInfo(f).CreationTime`.
fn creation_time(f: &Path) -> SystemTime {
    // DIVERGE: FileInfo.CreationTime never throws (a file without one reads as 1601-01-01); a
    // platform or file system without creation times sorts as that same earliest instant here.
    std::fs::metadata(f)
        .and_then(|m| m.created())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// A file's creation time, as [`try_load_weenies_by`] and [`try_load_weenies_converted_by`] take it.
pub type CreationTime<'a> = &'a dyn Fn(&Path) -> SystemTime;

/// `...OrderByDescending(f => new FileInfo(f).CreationTime).ToList()`: newest first; a stable
/// sort, so files created at the same instant keep their listing order.
fn newest_first(files: Vec<PathBuf>, created: CreationTime) -> Vec<PathBuf> {
    // DIVERGE: ties keep this port's path order, where ACE keeps the file system's listing order.
    let mut keyed: Vec<(SystemTime, PathBuf)> =
        files.into_iter().map(|f| (created(&f), f)).collect();
    keyed.sort_by_key(|k| std::cmp::Reverse(k.0));
    keyed.into_iter().map(|(_, f)| f).collect()
}

/// `JsonSerializer.Deserialize<LSDWeenie>(File.ReadAllText(file))`.
fn deserialize(file: &Path) -> R<Option<LsdWeenie>> {
    let text = read_all_text(file)?;
    let doc = value::parse(&text)?;
    LsdWeenie::read(&doc)
}

// ACE: LifestonedLoader.TryLoadWeenie
pub fn try_load_weenie(file: &Path) -> R<Option<LsdWeenie>> {
    deserialize(file)
}

/// The weenies of every file under `folder` that loads, newest file first.
pub fn try_load_weenies(folder: &Path) -> R<Vec<Option<LsdWeenie>>> {
    try_load_weenies_by(folder, &creation_time)
}

/// [`try_load_weenies`] with the files' creation times from `created`. Not ACE: a seam for hosts
/// where a program cannot set a file's creation time (every host but Windows), so the tests can
/// give the files the times ACE's vectors were recorded with.
// ACE: LifestonedLoader.TryLoadWeenies
pub fn try_load_weenies_by(folder: &Path, created: CreationTime) -> R<Vec<Option<LsdWeenie>>> {
    let mut results = Vec::new();

    let files = newest_first(json_files(folder)?, created);

    for file in files {
        if let Ok(result) = try_load_weenie(&file) {
            results.push(result);
        }
    }

    Ok(results)
}

// ACE: LifestonedLoader.TryLoadWeeniesInParallel
pub fn try_load_weenies_in_parallel(folder: &Path) -> R<Vec<Option<LsdWeenie>>> {
    let files = json_files(folder)?;

    // DIVERGE: Parallel.ForEach into a ConcurrentBag (unspecified order); files in path order here.
    let mut weenies = Vec::new();
    for file in files {
        if let Ok(result) = try_load_weenie(&file) {
            weenies.push(result);
        }
    }

    Ok(weenies)
}

// ACE: LifestonedLoader.TryLoadWeenieConverted
pub fn try_load_weenie_converted(file: &Path, correct_for_enum_shift: bool) -> R<Weenie> {
    let lifestoned_model = deserialize(file)?;

    // A null document throws on `input.WeenieId`, which the catch turns into false.
    let lifestoned_model = lifestoned_model.ok_or("the document is null")?;

    lifestoned::try_convert_with(&lifestoned_model, correct_for_enum_shift)
}

/// The converted weenies of every file under `folder`, newest file first.
pub fn try_load_weenies_converted(folder: &Path, correct_for_enum_shift: bool) -> R<Vec<Weenie>> {
    try_load_weenies_converted_by(folder, correct_for_enum_shift, &creation_time)
}

/// [`try_load_weenies_converted`] with the files' creation times from `created` (the seam of
/// [`try_load_weenies_by`]).
// ACE: LifestonedLoader.TryLoadWeeniesConverted
pub fn try_load_weenies_converted_by(
    folder: &Path,
    correct_for_enum_shift: bool,
    created: CreationTime,
) -> R<Vec<Weenie>> {
    let mut results = Vec::new();

    let files = newest_first(json_files(folder)?, created);

    for file in files {
        if let Ok(result) = try_load_weenie_converted(&file, correct_for_enum_shift) {
            results.push(result);
        }
    }

    Ok(results)
}

// ACE: LifestonedLoader.TryLoadWeeniesConvertedInParallel
pub fn try_load_weenies_converted_in_parallel(
    folder: &Path,
    correct_for_enum_shift: bool,
) -> R<Vec<Weenie>> {
    let files = json_files(folder)?;

    // DIVERGE: Parallel.ForEach into a ConcurrentBag (unspecified order); files in path order here.
    let mut weenies = Vec::new();
    for file in files {
        if let Ok(result) = try_load_weenie_converted(&file, correct_for_enum_shift) {
            weenies.push(result);
        }
    }

    Ok(weenies)
}
