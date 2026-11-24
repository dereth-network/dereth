//! A world's lockfile made whole again after a merge: two authors on two branches have each given
//! ids, and the lockfile holds both, in git's conflict markers. It is never put right by hand: it
//! is regenerated ([`rebuild`]).
//!
//! Every id either side gave is kept, but where the two gave one id to two symbols, or two ids to
//! one symbol:
//!
//! - **One id, two symbols:** the symbol of the earlier pack, in the world's order, keeps it; the
//!   other is numbered again, as a build numbers a new symbol, and the rows of its own pack that
//!   name its old id (a model's data id in a weenie, a cell in a placement) are written with the
//!   new one.
//! - **Two ids, one symbol:** it keeps the lower, and the rows of its pack that name the higher
//!   are written with the lower.
//!
//! What was numbered again, and which files were written, is said, to be looked over before the
//! merge is committed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_pack::symbol::SymbolKind;

use crate::error::{Error, Loc};
use crate::lock::Lock;
use crate::symbol::Kind;
use crate::world::World;

/// A symbol given another id by [`rebuild`]: its kind, its name, its id before and now, and the
/// files of its pack written with the id now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Renumbered {
    pub kind: Kind,
    pub symbol: String,
    pub from: u32,
    pub to: u32,
    pub rows: Vec<PathBuf>,
}

/// A symbol that gives up an id: its kind, its name, the id, and the id it keeps instead (two ids,
/// one symbol), else none (to be numbered again).
pub type GivenUp = (Kind, String, u32, Option<u32>);

/// Every entry a lockfile's text gives, both sides of a conflict's: each one's kind, symbol and id.
///
/// # Errors
/// A line that is no entry, section, comment or conflict marker.
pub fn entries(path: &Path, text: &str) -> Result<Vec<(Kind, String, u32)>, Error> {
    let mut out = Vec::new();
    let mut kind: Option<Kind> = None;
    for (n, line) in text.lines().enumerate() {
        let at = || Loc::new(path, n + 1);
        let l = line.trim();
        if l.is_empty()
            || l.starts_with('#')
            || ["<<<<<<<", "=======", ">>>>>>>", "|||||||"]
                .iter()
                .any(|m| l.starts_with(m))
        {
            continue;
        }
        if let Some(name) = l.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            kind = Some(Kind::from_name(name).ok_or_else(|| {
                Error::source(&at(), format!("[{name}] is not a kind of symbol"))
            })?);
            continue;
        }
        let entry: toml::Table =
            toml::from_str(l).map_err(|e| Error::source(&at(), e.to_string()))?;
        let (symbol, id) = entry
            .into_iter()
            .next()
            .ok_or_else(|| Error::source(&at(), "not an entry"))?;
        let id = id
            .as_integer()
            .and_then(|i| u32::try_from(i).ok())
            .ok_or_else(|| Error::source(&at(), format!("{symbol:?} is not given an id")))?;
        let k = kind.ok_or_else(|| Error::source(&at(), "an entry before any [kind]"))?;
        out.push((k, symbol, id));
    }
    Ok(out)
}

/// The lock `entries` make, every id kept but where two symbols share one; and the symbols that
/// give up an id (`(kind, symbol, id)`), to be numbered again, or (two ids, one symbol) the id
/// they keep instead. `packs` is the world's packs' names, in order: the earlier keeps an id.
#[must_use]
pub fn merged(entries: &[(Kind, String, u32)], packs: &[String]) -> (Lock, Vec<GivenUp>) {
    let place = |symbol: &str| {
        let pack = symbol.split_once(':').map_or("", |(p, _)| p);
        packs.iter().position(|p| p == pack).unwrap_or(usize::MAX)
    };
    let mut by_symbol: BTreeMap<(Kind, &str), BTreeSet<u32>> = BTreeMap::new();
    for (k, s, id) in entries {
        by_symbol.entry((*k, s.as_str())).or_default().insert(*id);
    }
    let mut lose = Vec::new();
    // Two ids, one symbol: the lower kept.
    let mut kept: BTreeMap<(Kind, &str), u32> = BTreeMap::new();
    for ((k, s), ids) in &by_symbol {
        let mut ids = ids.iter().copied();
        let low = ids.next().unwrap_or_default();
        for other in ids {
            lose.push((*k, (*s).to_owned(), other, Some(low)));
        }
        kept.insert((*k, s), low);
    }
    // One id, two symbols: the earlier pack's kept.
    let mut by_id: BTreeMap<(Kind, u32), Vec<&str>> = BTreeMap::new();
    for ((k, s), id) in &kept {
        by_id.entry((*k, *id)).or_default().push(s);
    }
    let mut lock = Lock::default();
    for ((k, id), mut symbols) in by_id {
        symbols.sort_by_key(|s| (place(s), (*s).to_owned()));
        lock.insert(k, symbols[0], id);
        for s in &symbols[1..] {
            lose.push((k, (*s).to_owned(), id, None));
        }
    }
    (lock, lose)
}

/// Pack `dir`'s rows (but its header, its rows' meta file and the world builder's own folders)
/// with every integer `from` written `to`, in hexadecimal where it was. Answers the files written.
///
/// # Errors
/// A file that doesn't read, parse or write.
pub fn renumber_rows(dir: &Path, from: u32, to: u32) -> Result<Vec<PathBuf>, Error> {
    fn visit(v: &mut toml_edit::Value, from: i64, to: i64, hit: &mut bool) {
        match v {
            toml_edit::Value::Integer(n) if *n.value() == from => {
                let hex = n
                    .as_repr()
                    .and_then(|r| r.as_raw().as_str())
                    .is_some_and(|r| r.starts_with("0x"));
                let mut new = toml_edit::Formatted::new(to);
                if hex {
                    new = match format!("0x{to:08X}").parse::<toml_edit::Value>() {
                        Ok(toml_edit::Value::Integer(h)) => h,
                        _ => new,
                    };
                }
                *new.decor_mut() = n.decor().clone();
                *n = new;
                *hit = true;
            }
            toml_edit::Value::Array(a) => a.iter_mut().for_each(|v| visit(v, from, to, hit)),
            toml_edit::Value::InlineTable(t) => {
                t.iter_mut().for_each(|(_, v)| visit(v, from, to, hit));
            }
            _ => {}
        }
    }
    fn item(i: &mut toml_edit::Item, from: i64, to: i64, hit: &mut bool) {
        match i {
            toml_edit::Item::Value(v) => visit(v, from, to, hit),
            toml_edit::Item::Table(t) => t.iter_mut().for_each(|(_, i)| item(i, from, to, hit)),
            toml_edit::Item::ArrayOfTables(a) => a
                .iter_mut()
                .for_each(|t| t.iter_mut().for_each(|(_, i)| item(i, from, to, hit))),
            toml_edit::Item::None => {}
        }
    }
    let mut written = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let read = std::fs::read_dir(&d).map_err(|e| Error::io(&d, e))?;
        for e in read {
            let p = e.map_err(|e| Error::io(&d, e))?.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if p.is_dir() {
                if !(d == dir && crate::repo::BUILDER_FOLDERS.contains(&name)) {
                    todo.push(p);
                }
                continue;
            }
            if p.extension().is_none_or(|x| x != "toml")
                || (d == dir && matches!(name, "pack.toml" | "world-db.toml"))
            {
                continue;
            }
            let text = std::fs::read_to_string(&p).map_err(|e| Error::io(&p, e))?;
            let mut doc: toml_edit::DocumentMut =
                text.parse().map_err(|e: toml_edit::TomlError| {
                    Error::source(&Loc::new(&p, 0), e.to_string())
                })?;
            let mut hit = false;
            doc.iter_mut()
                .for_each(|(_, i)| item(i, i64::from(from), i64::from(to), &mut hit));
            if hit {
                std::fs::write(&p, doc.to_string()).map_err(|e| Error::io(&p, e))?;
                written.push(p);
            }
        }
    }
    written.sort();
    Ok(written)
}

/// Regenerate the lockfile of `world` (a world repository's, or any) from what it holds, its
/// conflict markers and all: every id kept, two symbols of one id and two ids of one symbol
/// settled as the module says, the world built (over the files in `dats`) to number what lost
/// its id, and the rows that named an id given up written with the new one. Answers what was
/// numbered again.
///
/// # Errors
/// A lockfile that doesn't read, a world that doesn't build, or a file that doesn't write.
pub fn rebuild(world: World, dats: &Path) -> Result<Vec<Renumbered>, Error> {
    let path = world.lock_path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let all = entries(&path, &text)?;
    let packs: Vec<(String, PathBuf)> = world
        .packs
        .iter()
        .map(|d| crate::source::read_pack(d).map(|p| (p.name, p.dir)))
        .collect::<Result<_, _>>()?;
    let names: Vec<String> = packs.iter().map(|(n, _)| n.clone()).collect();
    let (lock, lose) = merged(&all, &names);
    std::fs::write(&path, lock.render()).map_err(|e| Error::io(&path, e))?;
    // Built, the symbols without an id are numbered as a build numbers new ones.
    let mut s = crate::edit::Session::open_world(world, dats, None, None)?;
    s.save()?;
    let now = s.compiled().lock.clone();
    let mut out = Vec::new();
    for (kind, symbol, from, kept) in lose {
        let to = match kept.or_else(|| now.get(kind, &symbol)) {
            Some(to) => to,
            None => continue,
        };
        let pack = symbol.split_once(':').map_or("", |(p, _)| p);
        let rows = match packs.iter().find(|(n, _)| n == pack) {
            Some((_, dir)) if from != to => renumber_rows(dir, from, to)?,
            _ => Vec::new(),
        };
        out.push(Renumbered {
            kind,
            symbol,
            from,
            to,
            rows,
        });
    }
    Ok(out)
}
