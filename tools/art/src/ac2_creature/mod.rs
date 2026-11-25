//! An Asheron's Call 2 creature as Asheron's Call's own records, made from the player's own
//! Asheron's Call 2 files into an overlay folder: its body (a setup, a model a part, and the
//! skinned body drawn in their place), an animation for each motion, its motion table and its
//! combat table. A world names the setup, the motion table and the combat table in a weenie, and
//! then the server and the client use the creature as they use any other.
//!
//! The creature's table (`tools/art/ac2-creatures/<name>.toml`) says which AC2 body and which of
//! its clips stand for each motion; [`table`] reads it and [`make`] makes the records.
//!
//! ```text
//! dereth-art ac2-creature --ac1 <install> --ac2 <install> --table <creature.toml> --out <folder>
//!     [--ids <first low id, hex>] [--world <key>] [--replace] [--weenies <folder>]
//! dereth-art ac2-creature-probe --ac2 <install> (--entity <id> | --setup <id> | --looks <id>)
//! ```
//!
//! `--weenies` names a world's weenie folder: each look's weenie there (`tyrant-2` is
//! `tyrant2.sql`) has its texture swaps written again, on every part that draws them
//! ([`weenie_swaps`]).
//!
//! [`mod@probe`] says what a table is written from: an entity's body and its animation map's clips,
//! or the entities drawn with a setup.

pub mod make;
pub mod probe;
pub mod table;
pub mod tint;

use std::path::PathBuf;

use dereth_dat::overlay::{container_name, OverlayWriter};
use dereth_dat::{DatFile, ModernDat};
use dereth_primitives::DataId;

use crate::ac2_look::source::Ac2;

/// The world key the overlay is written for when none is given.
pub const DEFAULT_WORLD: &str = "ac2-creatures";

/// The first low id of each kind of record made, when none is given: above the AC2 world's own.
pub const DEFAULT_IDS: u16 = 0xE000;

/// What to build.
#[derive(Debug, Clone)]
pub struct Args {
    /// The Asheron's Call install (acclient 00.00.11.6096's files).
    pub ac1: PathBuf,
    /// The Asheron's Call 2 install.
    pub ac2: PathBuf,
    /// The creature's table.
    pub table: PathBuf,
    /// The overlay folder written (made or added to).
    pub out: PathBuf,
    /// The low id each kind of record starts at.
    pub ids: u16,
    /// The world the overlay is for.
    pub world: String,
    /// Whether a record may write over one the overlay holds already with other contents (a
    /// creature made again); otherwise that is refused, as two creatures' ids colliding.
    pub replace: bool,
    /// A world's weenie folder whose look weenies have their texture swaps written again.
    pub weenies: Option<PathBuf>,
}

impl Args {
    /// The arguments after `ac2-creature`.
    ///
    /// # Errors
    /// One is missing or will not read.
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let (mut ac1, mut ac2, mut table, mut out) = (None, None, None, None);
        let mut ids = DEFAULT_IDS;
        let mut world = DEFAULT_WORLD.to_owned();
        let mut replace = false;
        let mut weenies = None;
        let mut a = args.iter();
        while let Some(k) = a.next() {
            if k == "--replace" {
                replace = true;
                continue;
            }
            let v = a
                .next()
                .ok_or_else(|| format!("{k} needs a value"))?
                .clone();
            match k.as_str() {
                "--ac1" => ac1 = Some(PathBuf::from(v)),
                "--ac2" => ac2 = Some(PathBuf::from(v)),
                "--table" => table = Some(PathBuf::from(v)),
                "--out" => out = Some(PathBuf::from(v)),
                "--ids" => {
                    ids = u16::from_str_radix(v.trim_start_matches("0x"), 16)
                        .map_err(|_| format!("--ids {v}: a low id in hex"))?;
                }
                "--world" => world = v,
                "--weenies" => weenies = Some(PathBuf::from(v)),
                _ => return Err(format!("{k} is not an ac2-creature option")),
            }
        }
        Ok(Self {
            ac1: ac1.ok_or("--ac1 names the Asheron's Call install")?,
            ac2: ac2.ok_or("--ac2 names the Asheron's Call 2 install")?,
            table: table.ok_or("--table names the creature's table")?,
            out: out.ok_or("--out names the overlay folder")?,
            ids,
            world,
            replace,
            weenies,
        })
    }
}

/// The marker a look weenie's texture swaps start at.
const SWAPS_AT: &str = "INSERT INTO `weenie_properties_texture_map`";

/// `sql`, a look's weenie, with its texture swaps written as `parts` (part, old, new) gives them,
/// under the weenie's own id. `None` when it has no texture swaps to write over.
#[must_use]
pub fn weenie_swaps(sql: &str, parts: &[(u16, DataId, DataId)]) -> Option<String> {
    let start = sql.find(SWAPS_AT)?;
    let end = start + sql[start..].find(';')? + 1;
    let block = &sql[start..end];
    let values = block.find("VALUES (")? + "VALUES (".len();
    let id = block[values..].split(',').next()?.trim();
    let mut rows = String::new();
    for (k, (part, old, new)) in parts.iter().enumerate() {
        rows.push_str(if k == 0 { "VALUES " } else { "\n     , " });
        rows.push_str(&format!(
            "({id}, {part}, 0x{:08X}, 0x{:08X})",
            old.raw(),
            new.raw()
        ));
    }
    Some(format!(
        "{}{SWAPS_AT} (`object_Id`, `index`, `old_Id`, `new_Id`)\n{rows};{}",
        &sql[..start],
        &sql[end..]
    ))
}

/// Write each look's texture swaps into its weenie in `dir`, part by part; the weenies written.
///
/// # Errors
/// A look's weenie will not read or write, or has no texture swaps to write over.
pub fn write_weenie_swaps(
    dir: &std::path::Path,
    made: &make::Made,
) -> Result<Vec<PathBuf>, String> {
    let mut written = Vec::new();
    for l in &made.looks {
        if l.parts.is_empty() {
            continue;
        }
        let path = dir.join(format!("{}.sql", l.name.replace('-', "")));
        let sql = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let new = weenie_swaps(&sql, &l.parts)
            .ok_or_else(|| format!("{}: no texture swaps to write over", path.display()))?;
        if new != sql {
            std::fs::write(&path, new).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        written.push(path);
    }
    Ok(written)
}

/// Make the creature and write its records into the overlay's portal file.
///
/// # Errors
/// An install or the table will not read, a record will not make, or the overlay will not write.
pub fn build(args: &Args) -> Result<make::Made, String> {
    let text = std::fs::read_to_string(&args.table)
        .map_err(|e| format!("{}: {e}", args.table.display()))?;
    let table = table::Table::parse(&text)?;
    let ac2 = Ac2::open(&args.ac2)?;
    let mut ids = make::Ids::from(args.ids);
    let mut records = dereth_ac2_convert::records::Records::default();
    let made = make::make(&ac2, &table, &mut ids, &mut records)?;
    let portal_path = args.ac1.join(ModernDat::Portal.file_name());
    let portal =
        DatFile::open(&portal_path).map_err(|e| format!("{}: {e}", portal_path.display()))?;
    let iteration = dereth_dat::decompose::revision_over(&portal);
    std::fs::create_dir_all(&args.out).map_err(|e| format!("{}: {e}", args.out.display()))?;
    let mut w = OverlayWriter::open_or_create(
        &args.out.join(container_name(ModernDat::Portal)),
        &portal,
        "client_portal.dat",
        &args.world,
        0,
    )
    .map_err(|e| format!("portal overlay: {e:?}"))?;
    for (id, bytes) in &records.0 {
        if portal.base_entry(*id).is_some() {
            return Err(format!("{id:?} is a record the base file holds already"));
        }
        let ours = dereth_dat::overlay::record_hash(bytes);
        if !args.replace && w.hash_of(*id).is_some_and(|h| h != ours) {
            return Err(format!(
                "{id:?} is a record the overlay holds already: choose other ids (--ids), or \
                 --replace to make the creature again"
            ));
        }
        w.put(*id, bytes, 1, iteration, 0)
            .map_err(|e| format!("{id:?}: {e:?}"))?;
    }
    w.add_iteration(iteration, 0)
        .map_err(|e| format!("{e:?}"))?;
    w.flush(0).map_err(|e| format!("{e:?}"))?;
    if let Some(dir) = &args.weenies {
        write_weenie_swaps(dir, &made)?;
    }
    Ok(made)
}

/// `ac2-creature-probe`: an entity's body and animation map, the entities drawn with a setup, or
/// every look of an entity's body and the templates drawn as each.
///
/// # Errors
/// An argument is missing or will not read, or the install will not open.
pub fn probe(args: &[String]) -> Result<String, String> {
    let (mut ac2, mut entity, mut setup, mut looks) = (None, None, None, None);
    let hex = |k: &str, v: &str| {
        u32::from_str_radix(v.trim_start_matches("0x"), 16).map_err(|_| format!("{k} {v}: hex"))
    };
    let mut a = args.iter();
    while let Some(k) = a.next() {
        let v = a.next().ok_or_else(|| format!("{k} needs a value"))?;
        match k.as_str() {
            "--ac2" => ac2 = Some(PathBuf::from(v)),
            "--entity" => entity = Some(hex(k, v)?),
            "--setup" => setup = Some(hex(k, v)?),
            "--looks" => looks = Some(hex(k, v)?),
            _ => return Err(format!("{k} is not an ac2-creature-probe option")),
        }
    }
    let ac2 = Ac2::open(&ac2.ok_or("--ac2 names the Asheron's Call 2 install")?)?;
    match (entity, setup, looks) {
        (Some(e), _, _) => probe::entity(&ac2, e),
        (None, Some(s), _) => Ok(probe::drawn_with(&ac2, s)
            .iter()
            .map(|e| format!("{e:08X}\n"))
            .collect()),
        (None, None, Some(b)) => probe::looks(&ac2, b),
        (None, None, None) => Err("--entity, --setup or --looks names what to look at".to_owned()),
    }
}
