//! A world repository published for its operator's server: the three builds in order, from the
//! repository and the operator's own installs, into one folder Empyrean serves.
//!
//! 1. **The land,** checked against the repository's pins (`[land]`): the converter's overlay,
//!    built by the operator from their own installs ([`crate::repo_world::check_land`]).
//! 2. **The served overlay:** the world at the layer, its records over the land, compiled into one
//!    overlay (`overlay/`) with the land's world files beside it. It is numbered from the
//!    repository's ledger (`[land] ledger`, `land/generations.tsv`), one numbering for everything
//!    the world has ever served ([`dereth_dat::generations`]): a digest of what it serves
//!    ([`served_digest`]) is its generation. Content the ledger lacks is refused: its author
//!    publishes it once with `number` and commits the ledger's new line, so every operator numbers
//!    it alike. An operator never numbers a generation. Published again into the same folder, it
//!    follows the overlay it served there: a record that reads as it did keeps its iteration, and
//!    only what changed takes the new generation's, so a client is sent what changed alone. A
//!    first publish into a new folder has none to follow: its clients take it whole, once.
//! 3. **The world pack** (`world.pack`), built by the server's own tool at the same layer
//!    (`empyrean-import world build`).
//!
//! Beside them, `empyrean.toml` names what the server is to read (the pack, the overlay, patching
//! on, since the land is streamed to every client, and the world's era and calendar), for the
//! operator to take into their own; and `README.txt` how it was built and every file's SHA-256.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use dereth_dat::generations::{self, Ledger};
use dereth_dat::ModernDat;
use sha2::{Digest, Sha256};

use crate::compile::Records;
use crate::error::Error;
use crate::export::Importer;
use crate::repo::Repo;
use crate::repo_world;

/// The ledger's place in the repository when its world file names none.
pub const LEDGER: &str = "land/generations.tsv";

/// What to publish, and how.
pub struct Publish<'a> {
    /// The layer, by id: the newest that can be built, when `None`.
    pub layer: Option<&'a str>,
    /// The land's overlay folder, for a repository that pins one.
    pub land: Option<&'a Path>,
    /// Whether content the ledger lacks is numbered, as its author does (else refused).
    pub number: bool,
    /// The server's import tool.
    pub importer: &'a dyn Importer,
}

impl std::fmt::Debug for Publish<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Publish")
            .field("layer", &self.layer)
            .field("land", &self.land)
            .field("number", &self.number)
            .finish_non_exhaustive()
    }
}

/// What [`publish`] made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Published {
    pub out: PathBuf,
    pub layer: String,
    /// The served overlay's generation, its content's digest, and whether this publish numbered it
    /// (a new line in the ledger, to commit).
    pub generation: u32,
    pub digest: String,
    pub numbered: bool,
    /// Every file written, by its path under `out`, with its SHA-256.
    pub files: Vec<(PathBuf, String)>,
    /// How the overlay the folder held was followed: the records kept in their iterations, those
    /// a client patched before is sent, and, when it couldn't be followed, why (every record is
    /// then the generation's, and that client takes the whole overlay again).
    pub followed: crate::compile::Followed,
}

/// The digest of what an overlay serves: each file's records in id order (its id, its version and
/// its bytes) and the ids it deletes, file by file.
#[must_use]
pub fn served_digest(
    records: &Records,
    tombstones: &std::collections::BTreeMap<ModernDat, std::collections::BTreeSet<(u32, u32)>>,
) -> [u8; 32] {
    let mut items: Vec<(u32, Vec<u8>)> = Vec::new();
    for t in ModernDat::ALL {
        let (recs, gone) = (records.get(&t), tombstones.get(&t));
        if recs.is_none_or(std::collections::BTreeMap::is_empty)
            && gone.is_none_or(std::collections::BTreeSet::is_empty)
        {
            continue;
        }
        // The file, so one record in two files is two things served.
        items.push((u32::MAX, t.file_name().as_bytes().to_vec()));
        for (id, s) in recs.into_iter().flatten() {
            let mut b = s.version.to_le_bytes().to_vec();
            b.extend(s.bytes.iter());
            items.push((*id, b));
        }
        let gone: Vec<u8> = gone
            .into_iter()
            .flatten()
            .flat_map(|(id, mask)| id.to_le_bytes().into_iter().chain(mask.to_le_bytes()))
            .collect();
        items.push((u32::MAX - 1, gone));
    }
    generations::digest(items.iter().map(|(id, b)| (*id, b.as_slice())))
}

/// The ledger numbering what the world file `world_toml` serves: a repository's own (`[land]
/// ledger`, else [`LEDGER`]); for a world with no repository, one beside its world file and named
/// for it (`world.toml`'s is `world.generations.tsv`).
#[must_use]
pub fn ledger_path(world_toml: &Path) -> PathBuf {
    let root = world_toml.parent().unwrap_or_else(|| Path::new("."));
    if repo_world::is_repository(world_toml) {
        return root.join(
            std::fs::read_to_string(world_toml)
                .ok()
                .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
                .and_then(|t| t.get("land")?.get("ledger")?.as_str().map(str::to_owned))
                .unwrap_or_else(|| LEDGER.to_owned()),
        );
    }
    let stem = world_toml
        .file_stem()
        .map_or_else(|| "world".into(), |s| s.to_string_lossy());
    root.join(format!("{stem}.generations.tsv"))
}

/// An overlay's generation in its world's ledger ([`ledger_path`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Numbered {
    pub generation: u32,
    /// Its content's digest ([`served_digest`]), in hex.
    pub digest: String,
    pub ledger: PathBuf,
    /// The ledger with this content's new line, when it had none: [`Numbered::record`] writes it.
    added: Option<String>,
}

impl Numbered {
    /// The generation of `records` and the deletions `tombstones` in the ledger of the world file
    /// `world_toml`: the one it has, or the next when it has none.
    ///
    /// # Errors
    /// A ledger that doesn't read.
    pub fn of(
        world_toml: &Path,
        records: &Records,
        tombstones: &std::collections::BTreeMap<ModernDat, std::collections::BTreeSet<(u32, u32)>>,
    ) -> Result<Self, Error> {
        let ledger = ledger_path(world_toml);
        let digest = served_digest(records, tombstones);
        let mut l = match std::fs::read_to_string(&ledger) {
            Ok(text) => Ledger::parse(&text)
                .map_err(|e| Error::Refused(format!("{}: {e}", ledger.display())))?,
            Err(_) => Ledger::default(),
        };
        let (generation, new) = l.generation(&digest);
        Ok(Self {
            generation,
            digest: generations::hex(&digest),
            ledger,
            added: new.then(|| l.text()),
        })
    }

    /// Whether the ledger lacked this content.
    #[must_use]
    pub fn is_new(&self) -> bool {
        self.added.is_some()
    }

    /// Write the ledger's new line, if it lacked this content.
    ///
    /// # Errors
    /// A ledger that won't write.
    pub fn record(&self) -> Result<(), Error> {
        let Some(text) = &self.added else {
            return Ok(());
        };
        if let Some(d) = self.ledger.parent() {
            std::fs::create_dir_all(d).map_err(|e| Error::io(d, e))?;
        }
        std::fs::write(&self.ledger, text).map_err(|e| Error::io(&self.ledger, e))
    }
}

fn sha256(path: &Path) -> Result<String, Error> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    Ok(Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// The README's first line, which marks a folder as a publish's: one it may write over.
const MARK: &str = "A world repository published for Empyrean by lyceum publish.";

/// Publish the world repository whose world file is `world_toml` into `out`, over the client's
/// files in `dats`, as `p` says.
///
/// # Errors
/// An output folder holding something else; a layer that can't be built; a land that isn't the
/// one pinned; a lockfile in conflict; content the ledger lacks, unless numbered; the world's
/// build errors, and the import tool's.
pub fn publish(
    world_toml: &Path,
    dats: &Path,
    out: &Path,
    p: &Publish<'_>,
) -> Result<Published, Error> {
    let refused = Error::Refused;
    let readme = out.join("README.txt");
    if out.exists()
        && std::fs::read_dir(out)
            .map_err(|e| Error::io(out, e))?
            .next()
            .is_some()
        && !std::fs::read_to_string(&readme).is_ok_and(|t| t.starts_with(MARK))
    {
        return Err(refused(format!(
            "{} holds something else: publish into a new folder, or an earlier publish's",
            out.display()
        )));
    }
    let repo = Repo::open(world_toml).map_err(refused)?;
    let at =
        match p.layer {
            Some(id) => repo.layers.iter().position(|l| l.id == id).ok_or_else(|| {
                refused(format!("no layer {id}; the layers are {:?}", repo.ids()))
            })?,
            None => repo.shown_layers() - 1,
        };
    let layer = repo.layers[at].id.clone();
    repo_world::check_lock(&repo, p.land).map_err(refused)?;
    let build = out.join(".build");
    let world = repo_world::world_of(&repo, at, p.land, &build).map_err(refused)?;
    repo_world::check_inputs(&repo, dats).map_err(refused)?;
    let key = world.key.clone();

    // The served overlay, numbered from the repository's ledger.
    let s = crate::edit::Session::open_world(world, dats, None, None)?;
    let c = s.compiled();
    let n = Numbered::of(world_toml, &c.records, &c.tombstones)?;
    let (generation, new, ledger_path) = (n.generation, n.is_new(), n.ledger.clone());
    if new && !p.number {
        return Err(refused(format!(
            "the world served at {layer} (content {}) has no generation in {}: its author publishes it with --number and commits the ledger's new line; an operator never numbers one",
            n.digest,
            ledger_path.display()
        )));
    }
    n.record()?;
    // Numbered after the overlay this folder last served, so a client is sent what changed alone.
    let overlay = out.join("overlay");
    let followed = crate::compile::write_records_after(
        &c.records,
        &c.tombstones,
        s.base(),
        &key,
        &overlay,
        generation,
        Some(&overlay),
    )?;
    crate::compile::carry_world_files(s.world(), &overlay)?;

    // The world pack, by the server's own tool.
    let pack = out.join("world.pack");
    let args: Vec<OsString> = vec![
        "world".into(),
        "build".into(),
        "--world".into(),
        world_toml.into(),
        "--layer".into(),
        layer.clone().into(),
        "--out".into(),
        pack.clone().into(),
    ];
    p.importer
        .run(&args)
        .map_err(|why| refused(format!("empyrean-import: {why}")))?;
    if !pack.is_file() {
        return Err(refused("empyrean-import wrote no world pack".into()));
    }

    // What the server reads.
    let era = repo.era;
    let launched = repo_world::launched_at(&repo).map_err(refused)?;
    // Whole paths: the server may be started anywhere.
    let slash = |p: &Path| {
        std::path::absolute(p)
            .unwrap_or_else(|_| p.to_path_buf())
            .display()
            .to_string()
            .replace('\\', "/")
    };
    let config = format!(
        "# What Empyrean reads to serve {key} at {layer}: take these into your empyrean.toml.\n\
         [server]\n\
         dat_files_directory = \"{dats}\"\n\
         world_pack_path = \"{pack}\"\n\n\
         [dat_overlay]\n\
         path = \"{overlay}\"\n\
         # The land is streamed to every client that joins: patching must stay on.\n\
         patching = true\n{era_lines}",
        dats = slash(dats),
        pack = slash(&pack),
        overlay = slash(&overlay),
        era_lines = match era {
            dereth_primitives::EraId::Eor => String::new(),
            other => format!(
                "\n[era]\nprofile = \"{}\"\n{}",
                other.name(),
                launched.map_or_else(String::new, |at| format!("launched_at = \"{at}\"\n"))
            ),
        },
    );
    let config_path = out.join("empyrean.toml");
    std::fs::write(&config_path, config).map_err(|e| Error::io(&config_path, e))?;

    let mut files = Vec::new();
    let mut todo = vec![overlay.clone()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| Error::io(&d, e))? {
            let path = e.map_err(|e| Error::io(&d, e))?.path();
            if path.is_dir() {
                todo.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.push(pack);
    files.push(config_path);
    files.sort();
    let files: Vec<(PathBuf, String)> = files
        .into_iter()
        .map(|f| {
            let rel = f.strip_prefix(out).unwrap_or(&f).to_path_buf();
            sha256(&f).map(|h| (rel, h))
        })
        .collect::<Result<_, _>>()?;
    let mut text = format!(
        "{MARK}\n\nWorld: {key}, at layer {layer}, from {}.\nServed overlay: generation {generation} in {} (content {hex}).\n\nFiles, by SHA-256:\n",
        world_toml.display(),
        ledger_path.display(),
        hex = n.digest,
    );
    for (f, h) in &files {
        text.push_str(&format!(
            "  {h}  {}\n",
            f.display().to_string().replace('\\', "/")
        ));
    }
    std::fs::write(&readme, text).map_err(|e| Error::io(&readme, e))?;
    let _ = std::fs::remove_dir_all(&build);
    Ok(Published {
        out: out.to_path_buf(),
        layer,
        generation,
        digest: n.digest,
        numbered: new,
        files,
        followed,
    })
}
