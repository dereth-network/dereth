//! A world over a base database: the world's own packs laid on a released database (its `[base]`),
//! as a world builder makes them and the server serves them.
//!
//! The base is read, checked and merged on its own, once ([`Base::new`]). The world's packs are
//! read like any database's; their references may name the base's rows (`pack:key`, a base pack's
//! name), and every reference is checked against the base and the world together. Then they are
//! merged among themselves by the usual rule (two of the world's packs that set one item
//! differently stop the build unless an override says which wins) and laid over the base: a world
//! row that sets an item the base sets takes its place without a conflict (the base is released
//! rows a world corrects by key), every other world row comes after the base's, and the world's
//! `[[remove]]` rows take items away from both.

use std::collections::{BTreeMap, BTreeSet};

use crate::merge::{item_key, merge, Merged};
use crate::rows::{read_partly, Database, Located, Row};
use crate::{check, Files, Problem};

/// The folder name base problems are reported under, so they are told from the world's own.
pub const BASE: &str = "base";

/// A base database, read, checked and merged.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Base {
    pub db: Database,
    pub merged: Merged,
}

/// A problem of the base, its file named under [`BASE`].
fn in_base(mut p: Problem) -> Problem {
    p.file = format!("{BASE}/{}", p.file);
    p
}

impl Base {
    /// The base database of `files`.
    ///
    /// # Errors
    /// Every problem the base has (its files named under [`BASE`]): a base that does not check or
    /// merge is no base.
    pub fn new(files: &Files) -> Result<Self, Vec<Problem>> {
        let problems = check::check(files);
        if !problems.is_empty() {
            return Err(problems.into_iter().map(in_base).collect());
        }
        let db =
            crate::rows::read(files).map_err(|p| p.into_iter().map(in_base).collect::<Vec<_>>())?;
        let merged = merge(&db).map_err(|p| p.into_iter().map(in_base).collect::<Vec<_>>())?;
        Ok(Self { db, merged })
    }

    /// The base's key, as its world file names it.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.db.world.key
    }
}

/// A world's files read over `base`: the world as far as it reads, and every problem of its own.
/// A world file naming a base of another key, or a pack folder or name the base also has, is a
/// problem.
#[must_use]
pub fn read(base: &Base, world: &Files) -> (Option<Database>, Vec<Problem>) {
    let (db, mut problems) = read_partly(world);
    if let Some(db) = &db {
        if let Some(b) = &db.world.base {
            if b.db != base.key() {
                problems.push(Problem::new(
                    "world.toml",
                    0,
                    format!(
                        "base: the world names the database {:?}, but the base is {:?}",
                        b.db,
                        base.key()
                    ),
                ));
            }
        }
        let theirs: BTreeSet<&str> = base
            .db
            .packs
            .iter()
            .flat_map(|p| [p.folder.as_str(), p.name.as_str()])
            .collect();
        for p in &db.packs {
            for name in [&p.folder, &p.name] {
                if theirs.contains(name.as_str()) {
                    problems.push(Problem::new(
                        &format!("{}/pack.toml", p.folder),
                        0,
                        format!("the base has a pack {name} too: name this one differently"),
                    ));
                }
            }
        }
    }
    problems.sort();
    problems.dedup();
    (db, problems)
}

/// Check a world's files over `base`: what [`check::check`] checks of a database (the schema,
/// keys, references, provenance and the merge), with references free to name the base's rows.
/// Returns every problem, sorted; none means the world reads and merges over its base.
#[must_use]
pub fn check(base: &Base, world: &Files) -> Vec<Problem> {
    let (db, mut problems) = read(base, world);
    let Some(db) = db else {
        return problems;
    };
    if !problems.is_empty() {
        return problems;
    }
    problems.extend(check_world(base, &db));
    if let Err(p) = merge_over(base, &db) {
        problems.extend(p);
    }
    problems.sort();
    problems.dedup();
    problems
}

/// The checks after reading of the world's own rows, with the base's rows there to be referred
/// to: what [`check::check_database`] finds in the base and the world together that lies in the
/// world's files.
#[must_use]
pub fn check_world(base: &Base, world: &Database) -> Vec<Problem> {
    let mut both = world.clone();
    both.packs = base.db.packs.iter().chain(&world.packs).cloned().collect();
    let ours: BTreeSet<&str> = world.packs.iter().map(|p| p.folder.as_str()).collect();
    check::check_database(&both)
        .into_iter()
        .filter(|p| {
            p.file
                .split_once('/')
                .is_some_and(|(folder, _)| ours.contains(folder))
        })
        .collect()
}

/// Merge a world's packs over `base`.
///
/// # Errors
/// Every conflict between the world's own packs (two that set one item differently with no
/// override saying which wins) and every item one of them sets twice.
pub fn merge_over(base: &Base, world: &Database) -> Result<Merged, Vec<Problem>> {
    let mut removes: Vec<(String, Located)> = Vec::new();
    let mut kept = world.clone();
    for p in &mut kept.packs {
        p.rows.retain(|l| match &l.row {
            Row::Remove(items) => {
                removes.extend(items.iter().map(|i| (i.clone(), l.clone())));
                false
            }
            _ => true,
        });
    }
    let ours = merge(&kept)?;
    let mut rows = base.merged.rows.clone();
    let at: BTreeMap<String, usize> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, l)| item_key(l).map(|k| (k, i)))
        .collect();
    let mut overridden = base.merged.overridden.clone();
    let mut after = Vec::new();
    for l in ours.rows {
        match item_key(&l).and_then(|k| at.get(&k).map(|i| (k, *i))) {
            Some((k, i)) => {
                overridden.push(k);
                rows[i] = l;
            }
            None => after.push(l),
        }
    }
    rows.extend(after);
    let mut warnings = base.merged.warnings.clone();
    warnings.extend(ours.warnings);
    let gone: BTreeSet<&str> = removes.iter().map(|(i, _)| i.as_str()).collect();
    let present: BTreeSet<String> = rows.iter().filter_map(item_key).collect();
    for (item, l) in &removes {
        if !present.contains(item) {
            warnings.push(Problem::new(
                &l.file,
                l.line,
                format!("remove: neither the base nor a pack sets {item}"),
            ));
        }
    }
    rows.retain(|l| item_key(l).is_none_or(|k| !gone.contains(k.as_str())));
    overridden.sort();
    overridden.dedup();
    Ok(Merged {
        rows,
        warnings,
        overridden,
    })
}
