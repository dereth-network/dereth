//! Export: a world written out whole for a server that runs it apart from Lyceum.
//!
//! | for | what it writes |
//! |---|---|
//! | Empyrean | `world.pack`, the world database of the world's era with the world's content in it, built by the server's import tool; `overlay/`, what the world changes in the data files; `sql/`, its content as ACE's SQL, one file per object |
//! | ACE | `dats/`, the data files the world changes with its changes written in ([`crate::bake`]); `sql/`, its content laid out as ACE's content folder |
//! | GDLE | `dats/`, as for ACE; `json/`, its content as GDLE's files (the import tool's `gdle`), with `quests.json`, the quests the world adds or changes, merged into the server's own `quests.json` when it is given |
//!
//! Each export also writes a `README.txt` that says where the files go and what players need. A
//! retail client reads only plain files: its players need the baked files, or a server that patches
//! theirs.
//!
//! The export folder is new, empty, or an earlier export, which it replaces; never the data files'
//! own folder. The world database the content goes over is a dump named by the caller, else the
//! era's newest in the import tool's per-user cache, downloaded by the tool when there is none.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::error::Error;

/// The file that marks a folder as an export, so a later export may replace it.
pub const MARKER: &str = ".lyceum-export";

/// The folder an export builds in, inside the export folder; gone when the export ends.
const STAGING: &str = ".lyceum-staging";

/// The server a world is exported for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Empyrean,
    Ace,
    Gdle,
}

impl Target {
    /// Every target, in the order they are offered.
    pub const ALL: [Self; 3] = [Self::Empyrean, Self::Ace, Self::Gdle];

    /// The target a word names: `empyrean`, `ace` or `gdle`.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        let word = word.to_ascii_lowercase();
        Self::ALL.into_iter().find(|t| t.word() == word)
    }

    /// The word for it on a command line and in a folder's name.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Empyrean => "empyrean",
            Self::Ace => "ace",
            Self::Gdle => "gdle",
        }
    }

    /// Its name, as people write it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Empyrean => "Empyrean",
            Self::Ace => "ACE",
            Self::Gdle => "GDLE",
        }
    }

    /// Whether the export runs the server's import tool over a world database.
    #[must_use]
    pub const fn needs_world_database(self) -> bool {
        !matches!(self, Self::Ace)
    }
}

/// The era whose world database the world's content goes over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Era {
    /// The end of retail.
    #[default]
    Eor,
    /// Infiltration, February 2005.
    Infiltration,
}

impl Era {
    /// The era a word names: `eor` or `infiltration`.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word.to_ascii_lowercase().as_str() {
            "eor" => Some(Self::Eor),
            "infiltration" => Some(Self::Infiltration),
            _ => None,
        }
    }

    /// The word for it, as the server's configuration and import tool take it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Eor => "eor",
            Self::Infiltration => "infiltration",
        }
    }

    /// The era whose world database the dump at `path` is, by its name: the February 2005 line's
    /// is Infiltration's, any other the end of retail's.
    #[must_use]
    pub fn of_database(path: &Path) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.starts_with(Self::Infiltration.dump_prefix()) {
            Self::Infiltration
        } else {
            Self::Eor
        }
    }

    /// The start of the names of this era's world database dumps.
    const fn dump_prefix(self) -> &'static str {
        match self {
            Self::Eor => "ACE-World-Database-v",
            Self::Infiltration => "ACE-World-16PY-db-v",
        }
    }

    /// The world, as a sentence names it.
    const fn world(self) -> &'static str {
        match self {
            Self::Eor => "the end-of-retail world",
            Self::Infiltration => "the Infiltration world (February 2005)",
        }
    }

    /// The era of the data files and of the world, as the rest of Dereth names it.
    #[must_use]
    pub const fn id(self) -> dereth_primitives::EraId {
        match self {
            Self::Eor => dereth_primitives::EraId::Eor,
            Self::Infiltration => dereth_primitives::EraId::Infiltration,
        }
    }

    /// The export's era for a world built on `era`.
    #[must_use]
    pub const fn of(era: dereth_primitives::EraId) -> Self {
        match era {
            dereth_primitives::EraId::Eor => Self::Eor,
            dereth_primitives::EraId::Infiltration => Self::Infiltration,
        }
    }

    /// The data files the era's world is drawn from, as a README names them.
    const fn data_files(self) -> &'static str {
        match self {
            Self::Eor => "the end-of-retail data files",
            Self::Infiltration => "the Infiltration data files, portal.dat and cell.dat",
        }
    }
}

/// What runs the server's import tool, `empyrean-import`.
pub trait Importer {
    /// Run the tool with `args`: what it printed, or what it said when it failed.
    ///
    /// # Errors
    /// The tool's refusal, or a tool that does not start.
    fn run(&self, args: &[OsString]) -> Result<String, String>;
}

/// The import tool as a program on this machine.
#[derive(Clone, Debug)]
pub struct ImportProgram(pub PathBuf);

impl Importer for ImportProgram {
    fn run(&self, args: &[OsString]) -> Result<String, String> {
        let mut cmd = std::process::Command::new(&self.0);
        cmd.args(args).stdin(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No console window of its own.
            cmd.creation_flags(0x0800_0000);
        }
        let out = cmd
            .output()
            .map_err(|e| format!("{}: {e}", self.0.display()))?;
        let said = |b: &[u8]| String::from_utf8_lossy(b).trim().to_owned();
        if out.status.success() {
            Ok(said(&out.stdout))
        } else {
            Err(said(&out.stderr))
        }
    }
}

/// What an export is for and where it goes.
#[derive(Clone, Copy)]
pub struct Options<'a> {
    pub target: Target,
    /// The export folder.
    pub out: &'a Path,
    /// The era whose world database the content goes over.
    pub era: Era,
    /// The world database dump, when the caller names one.
    pub world_sql: Option<&'a Path>,
    /// The server's import tool: needed for Empyrean and GDLE.
    pub importer: Option<&'a dyn Importer>,
    /// GDLE's own `quests.json`, which the world's quests are merged into.
    pub gdle_quests: Option<&'a Path>,
}

impl std::fmt::Debug for Options<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options")
            .field("target", &self.target)
            .field("out", &self.out)
            .field("era", &self.era)
            .field("world_sql", &self.world_sql)
            .field("importer", &self.importer.is_some())
            .field("gdle_quests", &self.gdle_quests)
            .finish()
    }
}

/// What an export wrote.
#[derive(Clone, Debug, Default)]
pub struct Exported {
    /// The export folder.
    pub out: PathBuf,
    /// One line for each thing written.
    pub wrote: Vec<String>,
    /// What the build's checks warn of, which stops nothing.
    pub warnings: Vec<String>,
}

/// Build the world at `world` over the data files in `dats` and export it as `opts` says.
///
/// # Errors
/// An export folder that holds something else; the build's errors; and the bake's and the import
/// tool's.
pub fn export(world: &Path, dats: &Path, opts: &Options<'_>) -> Result<Exported, Error> {
    let w = crate::world::World::read(world)?;
    refuse_era(w.era, opts)?;
    refuse_inputs(opts)?;
    prepare(opts.out, dats)?;
    let staging = opts.out.join(STAGING);
    let result = (|| {
        let key = w.key.clone();
        let built = crate::compile::build(world, dats, Some(&staging), false)?;
        let sql = read_sql(&staging.join("sql"))?;
        let mut done = write(&staging.join("overlay"), &sql, &key, dats, opts)?;
        done.warnings = built.warnings;
        Ok(done)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result
}

/// Export a world already compiled: its overlay folder `overlay`, its filled SQL `sql` (by path
/// under the build's `sql` folder: the pack's name, then the file's path in the pack's
/// `server/sql`), and its key, over the data files in `dats`.
///
/// # Errors
/// As [`export`], without the build's.
pub fn export_compiled(
    overlay: &Path,
    sql: &BTreeMap<PathBuf, String>,
    key: &str,
    dats: &Path,
    opts: &Options<'_>,
) -> Result<Exported, Error> {
    refuse_inputs(opts)?;
    prepare(opts.out, dats)?;
    let result = write(overlay, sql, key, dats, opts);
    let _ = std::fs::remove_dir_all(opts.out.join(STAGING));
    result
}

/// Refuse inputs the export cannot read, before anything is written: GDLE's `quests.json`
/// named and not there.
fn refuse_inputs(opts: &Options<'_>) -> Result<(), Error> {
    match opts.gdle_quests {
        Some(q) if opts.target == Target::Gdle && !q.is_file() => Err(Error::Refused(format!(
            "{} is not a file; name GDLE's quests.json",
            q.display()
        ))),
        _ => Ok(()),
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Refuse an export a world built on `era` cannot have: its content over another era's world
/// database, or the baked data files of an era a bake does not write.
fn refuse_era(era: dereth_primitives::EraId, opts: &Options<'_>) -> Result<(), Error> {
    let named = crate::dats::era_name(era);
    if opts.era.id() != era {
        return Err(Error::Refused(format!(
            "this world is built on the {named} files, so its content goes over the {named} \
             world database, not {}",
            opts.era.world()
        )));
    }
    if opts.target != Target::Empyrean && era.container_era() == dereth_dat::ContainerEra::Classic {
        return Err(Error::Refused(format!(
            "{} needs baked data files, and a bake writes only the end-of-retail files; \
             a world built on the {named} files runs on Empyrean \
             (lyceum export --for empyrean)",
            opts.target.name()
        )));
    }
    Ok(())
}

/// Make `out` ready: new, empty, or an earlier export emptied. Never the data files' folder.
fn prepare(out: &Path, dats: &Path) -> Result<(), Error> {
    if same_dir(out, dats) {
        return Err(Error::Refused(format!(
            "{} is the data files' own folder; export to another",
            out.display()
        )));
    }
    if out.is_dir() {
        let mut entries = std::fs::read_dir(out).map_err(|e| Error::io(out, e))?;
        if entries.next().is_some() {
            if !out.join(MARKER).is_file() {
                return Err(Error::Refused(format!(
                    "{} holds other files; export to a new or empty folder",
                    out.display()
                )));
            }
            std::fs::remove_dir_all(out).map_err(|e| Error::io(out, e))?;
        }
    }
    std::fs::create_dir_all(out).map_err(|e| Error::io(out, e))?;
    let marker = out.join(MARKER);
    std::fs::write(
        &marker,
        "Written by lyceum export; a later export replaces this folder.\n",
    )
    .map_err(|e| Error::io(&marker, e))
}

/// Every file under `dir`, by its path there, as text.
fn read_sql(dir: &Path) -> Result<BTreeMap<PathBuf, String>, Error> {
    let mut out = BTreeMap::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| Error::io(&d, e))? {
            let p = e.map_err(|e| Error::io(&d, e))?.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(dir).unwrap_or(&p).to_path_buf();
                let text = std::fs::read_to_string(&p).map_err(|e| Error::io(&p, e))?;
                out.insert(rel, text);
            }
        }
    }
    Ok(out)
}

fn copy_tree(from: &Path, to: &Path) -> Result<usize, Error> {
    std::fs::create_dir_all(to).map_err(|e| Error::io(to, e))?;
    let mut n = 0;
    if !from.is_dir() {
        return Ok(n);
    }
    for e in std::fs::read_dir(from).map_err(|e| Error::io(from, e))? {
        let p = e.map_err(|e| Error::io(from, e))?.path();
        let dest = to.join(p.file_name().unwrap_or_default());
        if p.is_dir() {
            n += copy_tree(&p, &dest)?;
        } else {
            std::fs::copy(&p, &dest).map_err(|e| Error::io(&dest, e))?;
            n += 1;
        }
    }
    Ok(n)
}

/// Write `sql` under `dir`, each file where `place` puts it (a path under `dir`).
fn write_sql(
    sql: &BTreeMap<PathBuf, String>,
    dir: &Path,
    place: impl Fn(&Path, &str) -> PathBuf,
) -> Result<usize, Error> {
    let mut written: BTreeMap<PathBuf, &Path> = BTreeMap::new();
    for (rel, text) in sql {
        let to = place(rel, text);
        if let Some(other) = written.insert(to.clone(), rel) {
            return Err(Error::Refused(format!(
                "{} and {} would both be written as {}",
                other.display(),
                rel.display(),
                to.display()
            )));
        }
        let path = dir.join(&to);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        std::fs::write(&path, text).map_err(|e| Error::io(&path, e))?;
    }
    Ok(written.len())
}

/// Where a pack's filled SQL file (`rel`, its path under the build's `sql` folder: the pack's
/// name, then the file's path in the pack's `server/sql`; `text`, its text) goes in a server's
/// content folder, and the import that takes it: a weenie by its class id (`<class> <name>.sql`),
/// a landblock's by its landblock (`<XXYY> <pack> <name>.sql`, so two packs' files of one
/// landblock do not meet), a quest by its name. `None` for a file a server's import does not
/// take by its kind.
#[must_use]
pub fn content_place(rel: &Path, text: &str) -> Option<(PathBuf, String)> {
    let mut parts = rel.components();
    let pack = parts.next()?.as_os_str().to_str()?;
    let kind = parts.next()?.as_os_str().to_str()?;
    let name = rel.file_name()?.to_str()?;
    let stem = rel.file_stem()?.to_str()?;
    match kind {
        "weenies" => {
            let class = crate::sql_ids::weenie_facts(text)
                .keys()
                .find_map(|w| crate::sql_ids::number(w))?;
            let stem = stem
                .split_once(' ')
                .filter(|(c, _)| c.parse::<u32>().is_ok())
                .map_or(stem, |(_, s)| s);
            Some((
                Path::new("sql")
                    .join("weenies")
                    .join(format!("{class:05} {stem}.sql")),
                format!("import-sql weenie {class}"),
            ))
        }
        "landblocks" => {
            let lb = name
                .get(..4)
                .filter(|l| u16::from_str_radix(l, 16).is_ok())?;
            Some((
                Path::new("sql")
                    .join("landblocks")
                    .join(format!("{lb} {pack}{}", &name[4..])),
                format!("import-sql landblock {lb}"),
            ))
        }
        "quests" => Some((
            Path::new("sql").join("quests").join(name),
            format!("import-sql quest {stem}"),
        )),
        _ => None,
    }
}

/// Where a pack's filled SQL file goes in ACE's content folder: [`content_place`]'s place, else
/// under its kind's folder as the pack has it.
#[must_use]
pub fn ace_place(rel: &Path, text: &str) -> PathBuf {
    content_place(rel, text).map_or_else(
        || {
            let mut parts = rel.components();
            parts.next();
            Path::new("sql").join(parts.as_path())
        },
        |(path, _)| path,
    )
}

/// The folder `empyrean-import fetch` keeps world databases in on this machine.
#[must_use]
pub fn database_cache() -> Option<PathBuf> {
    database_folder(|n| std::env::var_os(n))
}

/// The folder `empyrean-import fetch` keeps world databases in, reading the environment through
/// `env`.
fn database_folder(env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let var = |name: &str| {
        env(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let base = if cfg!(windows) {
        var("LOCALAPPDATA")?.join("Empyrean")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Caches/Empyrean")
    } else {
        var("XDG_CACHE_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".cache")))?
            .join("empyrean")
    };
    Some(base.join("world-database"))
}

/// The version a dump's name carries after `prefix`: `<prefix><x>.<y>.<z>.sql`.
fn dump_version(name: &str, prefix: &str) -> Option<(u32, u32, u32)> {
    let version = name.strip_prefix(prefix)?.strip_suffix(".sql")?;
    let n: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    let [a, b, c] = n[..] else { return None };
    Some((a, b, c))
}

/// The newest world database in `dir`: the highest release of the end of retail's line
/// (`ACE-World-Database-v<x>.<y>.<z>.sql`), else of the February 2005 line.
#[must_use]
pub fn newest_database(dir: &Path) -> Option<PathBuf> {
    newest_database_for(dir, Era::Eor).or_else(|| newest_database_for(dir, Era::Infiltration))
}

/// The newest world database of `era` in `dir`.
#[must_use]
pub fn newest_database_for(dir: &Path, era: Era) -> Option<PathBuf> {
    let mut best: Option<((u32, u32, u32), PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(key) = dump_version(&name, era.dump_prefix()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(k, _)| key > *k) {
            best = Some((key, e.path()));
        }
    }
    best.map(|(_, p)| p)
}

/// The world database the content goes over: the one named, else the era's newest in the import
/// tool's cache, fetched by the tool when there is none.
fn world_database(importer: &dyn Importer, opts: &Options<'_>) -> Result<PathBuf, Error> {
    if let Some(path) = opts.world_sql {
        let era = Era::of_database(path);
        if era != opts.era {
            return Err(Error::Refused(format!(
                "{} is {}'s world database; this world's content goes over {}'s",
                path.display(),
                era.world(),
                opts.era.world()
            )));
        }
        return if path.is_file() {
            Ok(path.to_path_buf())
        } else {
            Err(Error::Refused(format!(
                "{} is not a world database dump",
                path.display()
            )))
        };
    }
    let cache = database_cache().ok_or_else(|| {
        Error::Refused("there is no cache folder for the world database; name a dump".into())
    })?;
    if let Some(path) = newest_database_for(&cache, opts.era) {
        return Ok(path);
    }
    let mut args = vec![OsString::from("fetch")];
    if opts.era == Era::Infiltration {
        args.extend(["--world".into(), "16py".into()]);
    }
    importer
        .run(&args)
        .map_err(|why| Error::Refused(format!("the world database did not download: {why}")))?;
    newest_database_for(&cache, opts.era).ok_or_else(|| {
        Error::Refused(format!(
            "the download left no world database in {}",
            cache.display()
        ))
    })
}

fn importer<'a>(opts: &Options<'a>) -> Result<&'a dyn Importer, Error> {
    opts.importer.ok_or_else(|| {
        Error::Refused(format!(
            "an export for {} needs the server's import tool, empyrean-import",
            opts.target.name()
        ))
    })
}

fn run(importer: &dyn Importer, args: &[&OsStr]) -> Result<String, Error> {
    let args: Vec<OsString> = args.iter().map(|a| (*a).to_owned()).collect();
    importer
        .run(&args)
        .map_err(|why| Error::Refused(format!("empyrean-import: {why}")))
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or_default().trim()
}

/// Bake the overlay into copies of the files it changes, in `out/dats`.
fn bake(overlay: &Path, dats: &Path, out: &Path, wrote: &mut Vec<String>) -> Result<(), Error> {
    let baked = crate::bake::bake(overlay, dats, &out.join("dats"), true)?;
    for (path, records, deleted) in &baked.written {
        wrote.push(format!(
            "{}: {records} record(s) written, {deleted} deletion(s)",
            path.display()
        ));
    }
    if baked.written.is_empty() {
        wrote.push(format!(
            "{}: the world changes none of the data files",
            out.join("dats").display()
        ));
    }
    Ok(())
}

fn write(
    overlay: &Path,
    sql: &BTreeMap<PathBuf, String>,
    key: &str,
    dats: &Path,
    opts: &Options<'_>,
) -> Result<Exported, Error> {
    let out = opts.out;
    let mut wrote = Vec::new();
    let readme = match opts.target {
        Target::Empyrean => {
            let importer = importer(opts)?;
            let dump = world_database(importer, opts)?;
            let files = copy_tree(overlay, &out.join("overlay"))?;
            wrote.push(format!(
                "{}: {files} file(s)",
                out.join("overlay").display()
            ));
            let sql_dir = out.join("sql");
            let n = write_sql(sql, &sql_dir, |rel, _| rel.to_path_buf())?;
            wrote.push(format!("{}: {n} file(s)", sql_dir.display()));
            let pack = out.join("world.pack");
            let mut args = vec![
                OsStr::new("--sql"),
                dump.as_os_str(),
                OsStr::new("--era"),
                OsStr::new(opts.era.word()),
            ];
            if n > 0 {
                args.extend([OsStr::new("--patches"), sql_dir.as_os_str()]);
            }
            args.extend([OsStr::new("--out"), pack.as_os_str()]);
            let said = run(importer, &args)?;
            if !pack.is_file() {
                return Err(Error::Refused(
                    "empyrean-import wrote no world database".into(),
                ));
            }
            wrote.push(first_line(&said).to_owned());
            let version = run(importer, &[OsStr::new("--version")])
                .map(|v| first_line(&v).to_owned())
                .unwrap_or_else(|_| "empyrean-import".into());
            empyrean_readme(key, opts.era, &version, &dump)
        }
        Target::Ace => {
            bake(overlay, dats, out, &mut wrote)?;
            let n = write_sql(sql, out, ace_place)?;
            wrote.push(format!("{}: {n} file(s)", out.join("sql").display()));
            ace_readme(key)
        }
        Target::Gdle => {
            let importer = importer(opts)?;
            let dump = world_database(importer, opts)?;
            bake(overlay, dats, out, &mut wrote)?;
            let json = out.join("json");
            let mut quests = Quests::default();
            if sql.is_empty() {
                wrote.push(format!("{}: the world has no content", json.display()));
            } else {
                let scratch = out.join(STAGING).join("gdle-sql");
                write_sql(sql, &scratch, |rel, _| rel.to_path_buf())?;
                let mut args = vec![
                    OsStr::new("gdle"),
                    OsStr::new("--sql"),
                    dump.as_os_str(),
                    OsStr::new("--patches"),
                    scratch.as_os_str(),
                ];
                if let Some(q) = opts.gdle_quests {
                    args.extend([OsStr::new("--quests"), q.as_os_str()]);
                    quests.given = Some(q.to_path_buf());
                }
                args.extend([OsStr::new("--out"), json.as_os_str()]);
                run(importer, &args)?;
                wrote.extend(gdle_layout(&json, &mut quests)?);
            }
            gdle_readme(key, &quests)
        }
    };
    let path = out.join("README.txt");
    std::fs::write(&path, readme).map_err(|e| Error::io(&path, e))?;
    wrote.push(path.display().to_string());
    Ok(Exported {
        out: out.to_path_buf(),
        wrote,
        warnings: Vec::new(),
    })
}

/// What an export for GDLE did with the quests: the server's `quests.json` it was given, the
/// quests the world adds to it and those it changes there, and those the world removes (the world
/// database has them and the world does not), by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Quests {
    pub given: Option<PathBuf>,
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
}

/// GDLE's quest list as names (in lower case, as GDLE reads them) and the entries they name.
fn quest_entries(
    list: &serde_json::Value,
) -> Option<BTreeMap<String, (String, serde_json::Value)>> {
    list.as_array()?
        .iter()
        .map(|e| {
            let key = e.get("key")?.as_str()?;
            let value = e.get("value")?;
            // A `fullname` left out is GDLE's empty one, as one written `null` is.
            let field = |n: &str| value.get(n).cloned().unwrap_or(serde_json::Value::Null);
            let value =
                serde_json::json!([field("fullname"), field("mindelta"), field("maxsolves")]);
            Some((key.to_lowercase(), (key.to_owned(), value)))
        })
        .collect()
}

/// Lay out the import tool's GDLE files in `json` as GDLE reads them: a landblock's spawns in
/// `spawnMaps`, and the quests in the one `quests.json` the tool writes, the server's own with
/// the world's merged in when `quests` names one (each quest's own file goes); what the world
/// adds to that list, changes in it and removes goes into `quests`, the last from the list of
/// them the tool writes beside it, which goes. Answers a line for each thing there.
fn gdle_layout(json: &Path, quests: &mut Quests) -> Result<Vec<String>, Error> {
    let landblocks = json.join("landblocks");
    if landblocks.is_dir() {
        std::fs::rename(&landblocks, json.join("spawnMaps"))
            .map_err(|e| Error::io(&landblocks, e))?;
    }
    let quest_files = json.join("quests");
    if quest_files.is_dir() {
        std::fs::remove_dir_all(&quest_files).map_err(|e| Error::io(&quest_files, e))?;
    }
    let mut wrote = Vec::new();
    for kind in ["weenies", "spawnMaps"] {
        let dir = json.join(kind);
        let n = std::fs::read_dir(&dir).map_or(0, Iterator::count);
        wrote.push(format!("{}: {n} file(s)", dir.display()));
    }
    let list = json.join("quests.json");
    let read = |path: &Path| -> Result<BTreeMap<String, (String, serde_json::Value)>, Error> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}'))
            .ok()
            .as_ref()
            .and_then(quest_entries)
            .ok_or_else(|| Error::Refused(format!("{} is not a list of quests", path.display())))
    };
    let merged = read(&list)?;
    wrote.push(format!("{}: {} quest(s)", list.display(), merged.len()));
    let gone = json.join("quests-removed.json");
    if gone.is_file() {
        let text = std::fs::read_to_string(&gone).map_err(|e| Error::io(&gone, e))?;
        quests.removed = serde_json::from_str::<Vec<String>>(&text).map_err(|_| {
            Error::Refused(format!("{} is not a list of quest names", gone.display()))
        })?;
        std::fs::remove_file(&gone).map_err(|e| Error::io(&gone, e))?;
    }
    if let Some(given) = quests.given.clone() {
        let had = read(&given)?;
        for (name, (key, value)) in &merged {
            match had.get(name) {
                None => quests.added.push(key.clone()),
                Some((_, was)) if was != value => quests.changed.push(key.clone()),
                Some(_) => {}
            }
        }
    }
    Ok(wrote)
}

/// What a retail player needs, said once for every export.
const RETAIL_PLAYERS: &str = "\
Retail players: their client reads only its own data files, never an overlay. They need the
data files baked with this world (lyceum export --for ace writes them), copied over a copy of
their own files.";

fn empyrean_readme(key: &str, era: Era, version: &str, dump: &Path) -> String {
    let dump = dump
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    format!(
        "{key}, a Lyceum world, exported for Empyrean.

  world.pack   the world database: {world} with this world's content
  overlay/     what this world changes in the data files
  sql/         this world's content, one file per object

In empyrean.toml (a path is relative to empyrean.toml's folder; give full paths when it is
elsewhere):

  [server]
  world_pack_path = \"world.pack\"
  dat_files_directory = \"<{data_files}>\"

  [era]
  profile = \"{era}\"

  [dat_overlay]
  path = \"overlay\"

world.pack was written by {version}; another release of Empyrean may refuse it. Build it
again over the same world database ({dump}, from empyrean-import fetch):

  empyrean-import --sql <{dump}> --era {era} --patches sql --out world.pack

Dereth players: Empyrean sends a Dereth client the overlay when it connects.

{retail}
",
        world = era.world(),
        era = era.word(),
        data_files = era.data_files(),
        retail = match era {
            Era::Eor => format!(
                "{RETAIL_PLAYERS} Empyrean can patch their own files instead: set\n\
                 enable_dat_patching = true under [ddd]."
            ),
            Era::Infiltration =>
                "Retail players: no baked data files are written for a world built on the \
                Infiltration\nfiles, so it is played with Dereth."
                    .to_owned(),
        },
    )
}

fn ace_readme(key: &str) -> String {
    format!(
        "{key}, a Lyceum world, exported for ACE.

  dats/   the end-of-retail data files this world changes, with its changes written in
  sql/    this world's content as ACE's SQL, laid out as ACE's content folder

The server: copy the files in dats/ over a copy of the end-of-retail data files, and point
DatFilesDirectory in Config.js at that copy.

The content: copy sql/ into ACE's content folder (ContentFolder in Config.js) and, as an
admin, type

  /import-sql weenie all
  /import-sql landblock all
  /import-sql quest all

or load each file into the world database with MySQL:

  mysql -u <user> -p ace_world < <file>

Players: the client needs the same data files as the server. Give players the files in dats/
to copy over a copy of their own. ACE can patch their files instead: set EnableDATPatching
under DDD in Config.js.
"
    )
}

fn gdle_readme(key: &str, quests: &Quests) -> String {
    let (listed, merge) = match &quests.given {
        Some(given) => {
            let mut said = format!(
                "Copy json/weenies and json/spawnMaps into GDLE's Data/json folder, and \
                 json/quests.json over\nData/json/quests.json.\n\n\
                 json/quests.json is {} with this world's quests merged in. Every other quest\n\
                 keeps that file's entry.",
                given.display()
            );
            if !quests.added.is_empty() {
                said.push_str(&format!(
                    "\n\nThis world adds: {}.",
                    quests.added.join(", ")
                ));
            }
            if !quests.changed.is_empty() {
                said.push_str(&format!(
                    "\n\nThis world changes quests that file already has, so its entries replace \
                     that file's:\n{}.",
                    quests.changed.join(", ")
                ));
            }
            ("GDLE's quests.json with this world's quests merged in", said)
        }
        None => (
            "the quests this world adds or changes",
            format!(
                "Copy json/weenies and json/spawnMaps into GDLE's Data/json folder. Then merge\n\
                 json/quests.json into Data/json/quests.json: add each of its entries, in place of the\n\
                 entry with the same key when there is one{}. Or export again naming GDLE's\n\
                 quests.json (lyceum export --gdle-quests <file>, or in the Export menu), and the\n\
                 merged list is written here.",
                if quests.removed.is_empty() {
                    ""
                } else {
                    ", and take out the entries of the\nquests this world removes, below"
                }
            ),
        ),
    };
    // The quests the world removes, under their own heading.
    let merge = if quests.removed.is_empty() {
        merge
    } else {
        let names: Vec<String> = quests.removed.iter().map(|q| format!("  {q}")).collect();
        format!(
            "{merge}\n\nQuests this world removes{}:\n{}",
            if quests.given.is_some() {
                " (their entries are left out of json/quests.json)"
            } else {
                ""
            },
            names.join("\n")
        )
    };
    format!(
        "{key}, a Lyceum world, exported for GDLE.

  dats/             the end-of-retail data files this world changes, with its changes written in
  json/weenies/     this world's weenies
  json/spawnMaps/   the spawns of each landblock this world changes
  json/quests.json  {listed}

The server: copy the files in dats/ over a copy of the end-of-retail data files GDLE reads.
{merge}

Players: the client needs the same data files as the server. Give players the files in dats/
to copy over a copy of their own.
"
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a world exported for a server, and where its world database is)
    use super::*;
    use std::cell::RefCell;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lyceum-export-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A world built on Infiltration's files is exported over that era's world database and for
    /// Empyrean alone: another era's database, and ACE or GDLE (which need baked files), are
    /// refused before anything is written; so is a dump of another era named for it.
    #[test]
    fn an_infiltration_world_is_exported_over_its_eras_database_for_empyrean_alone() {
        let dir = scratch("infiltration");
        let world = dir.join("world.toml");
        std::fs::write(
            &world,
            "key = \"w\"
era = \"infiltration\"
",
        )
        .unwrap();
        let out = dir.join("out");
        let opts = |target, era, world_sql| Options {
            target,
            out: &out,
            era,
            world_sql,
            importer: None,
            gdle_quests: None,
        };
        let e = export(&world, &dir, &opts(Target::Empyrean, Era::Eor, None))
            .unwrap_err()
            .to_string();
        assert!(e.contains("Infiltration world database"), "{e}");
        for target in [Target::Ace, Target::Gdle] {
            let e = export(&world, &dir, &opts(target, Era::Infiltration, None))
                .unwrap_err()
                .to_string();
            assert!(e.contains("lyceum export --for empyrean"), "{e}");
        }
        assert!(!out.exists(), "nothing is written");
        let dump = dir.join("ACE-World-Database-v0.9.295.sql");
        std::fs::write(&dump, b"-- dump").unwrap();
        let e = world_database(
            &Recorder::default(),
            &opts(Target::Empyrean, Era::Infiltration, Some(&dump)),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("end-of-retail world"), "{e}");
        assert_eq!(Era::of(Era::Infiltration.id()), Era::Infiltration);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An import tool that writes what it is asked for and keeps the arguments it was given.
    #[derive(Default)]
    struct Recorder {
        runs: RefCell<Vec<Vec<String>>>,
    }

    impl Importer for Recorder {
        fn run(&self, args: &[OsString]) -> Result<String, String> {
            let args: Vec<String> = args
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            self.runs.borrow_mut().push(args.clone());
            if args[0] == "--version" {
                return Ok("empyrean-import 9.9.9\ntarget: test".into());
            }
            if let Some(i) = args.iter().position(|a| a == "--out") {
                std::fs::write(&args[i + 1], b"pack").unwrap();
            }
            Ok("wrote world.pack".into())
        }
    }

    #[test]
    fn targets_and_eras_read_their_words() {
        for t in Target::ALL {
            assert_eq!(Target::parse(t.word()), Some(t));
        }
        assert_eq!(Target::parse("ACE"), Some(Target::Ace));
        assert_eq!(Target::parse("mysql"), None);
        assert!(!Target::Ace.needs_world_database());
        assert_eq!(Era::parse("Infiltration"), Some(Era::Infiltration));
        assert_eq!(Era::parse("tod"), None);
        assert_eq!(
            Era::of_database(Path::new("c/ACE-World-16PY-db-v0.8.8.sql")),
            Era::Infiltration
        );
        assert_eq!(
            Era::of_database(Path::new("c/ACE-World-Database-v0.9.295.sql")),
            Era::Eor
        );
    }

    #[test]
    fn an_export_goes_to_a_new_or_empty_folder_or_replaces_an_earlier_export() {
        let dir = scratch("prepare");
        let dats = dir.join("dats");
        std::fs::create_dir_all(&dats).unwrap();
        std::fs::write(dereth_dat::ModernDat::Cell.in_dir(&dats), b"cell").unwrap();
        assert!(
            prepare(&dats, &dats).is_err(),
            "never the files' own folder"
        );
        let other = dir.join("other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("notes.txt"), b"mine").unwrap();
        assert!(
            prepare(&other, &dats).is_err(),
            "never a folder of other files"
        );
        assert!(other.join("notes.txt").is_file());
        let out = dir.join("out");
        prepare(&out, &dats).unwrap();
        std::fs::write(out.join("old.txt"), b"earlier").unwrap();
        prepare(&out, &dats).unwrap();
        assert!(
            !out.join("old.txt").exists(),
            "an earlier export is replaced"
        );
        assert!(out.join(MARKER).is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_packs_sql_goes_where_the_servers_import_finds_it() {
        let weenie = "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (90001, 'x', 10, '2026-10-19 00:00:00');\n";
        assert_eq!(
            content_place(Path::new("holtburg/weenies/guide.sql"), weenie),
            Some((
                PathBuf::from("sql/weenies/90001 guide.sql"),
                "import-sql weenie 90001".into()
            ))
        );
        assert_eq!(
            content_place(
                Path::new("holtburg/weenies/90001 Drudge Skulker.sql"),
                weenie
            ),
            Some((
                PathBuf::from("sql/weenies/90001 Drudge Skulker.sql"),
                "import-sql weenie 90001".into()
            ))
        );
        assert_eq!(
            content_place(Path::new("holtburg/landblocks/A9B4 guide.sql"), ""),
            Some((
                PathBuf::from("sql/landblocks/A9B4 holtburg guide.sql"),
                "import-sql landblock A9B4".into()
            ))
        );
        assert_eq!(
            content_place(Path::new("holtburg/quests/GribbleGreeted.sql"), ""),
            Some((
                PathBuf::from("sql/quests/GribbleGreeted.sql"),
                "import-sql quest GribbleGreeted".into()
            ))
        );
        assert_eq!(content_place(Path::new("holtburg/recipes/x.sql"), ""), None);
        assert_eq!(
            content_place(Path::new("weenies/x.sql"), ""),
            None,
            "no pack"
        );
        assert_eq!(
            content_place(Path::new("holtburg/weenies/none.sql"), "-- nothing"),
            None
        );
        // ACE's content folder keeps any other kind under its own folder.
        assert_eq!(
            ace_place(Path::new("holtburg/recipes/1234 pie.sql"), ""),
            PathBuf::from("sql/recipes/1234 pie.sql")
        );
        assert_eq!(
            ace_place(Path::new("holtburg/landblocks/A9B4 guide.sql"), ""),
            PathBuf::from("sql/landblocks/A9B4 holtburg guide.sql")
        );
    }

    #[test]
    fn two_files_that_would_land_on_one_name_are_refused() {
        let dir = scratch("collide");
        let sql: BTreeMap<PathBuf, String> = [
            (PathBuf::from("a/recipes/pie.sql"), "-- a".to_owned()),
            (PathBuf::from("b/recipes/pie.sql"), "-- b".to_owned()),
        ]
        .into_iter()
        .collect();
        let e = write_sql(&sql, &dir, ace_place).unwrap_err().to_string();
        assert!(e.contains("would both be written"), "{e}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_export_for_empyrean_builds_the_eras_world_database_with_the_worlds_content() {
        let dir = scratch("empyrean");
        let dats = dir.join("dats");
        std::fs::create_dir_all(&dats).unwrap();
        let overlay = dir.join("overlay");
        std::fs::create_dir_all(&overlay).unwrap();
        std::fs::write(overlay.join("overlay_cell.dat"), b"cells").unwrap();
        let dump = dir.join("ACE-World-16PY-db-v0.8.8.sql");
        std::fs::write(&dump, b"-- dump").unwrap();
        let sql: BTreeMap<PathBuf, String> = [(
            PathBuf::from("holtburg/landblocks/A9B4 guide.sql"),
            "DELETE FROM `landblock_instance` WHERE `guid` = 0x7A9B4FFF;".to_owned(),
        )]
        .into_iter()
        .collect();
        let out = dir.join("out");
        let recorder = Recorder::default();
        let done = export_compiled(
            &overlay,
            &sql,
            "sample-world",
            &dats,
            &Options {
                target: Target::Empyrean,
                out: &out,
                era: Era::Infiltration,
                world_sql: Some(&dump),
                importer: Some(&recorder),
                gdle_quests: None,
            },
        )
        .unwrap();
        let runs = recorder.runs.borrow();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        assert_eq!(
            runs[0],
            [
                "--sql".to_owned(),
                s(&dump),
                "--era".into(),
                "infiltration".into(),
                "--patches".into(),
                s(&out.join("sql")),
                "--out".into(),
                s(&out.join("world.pack")),
            ]
        );
        assert!(out.join("world.pack").is_file());
        assert_eq!(
            std::fs::read(out.join("overlay/overlay_cell.dat")).unwrap(),
            b"cells"
        );
        assert!(out.join("sql/holtburg/landblocks/A9B4 guide.sql").is_file());
        let readme = std::fs::read_to_string(out.join("README.txt")).unwrap();
        for said in [
            "world_pack_path = \"world.pack\"",
            "profile = \"infiltration\"",
            "path = \"overlay\"",
            "empyrean-import 9.9.9",
            "Retail players",
        ] {
            assert!(readme.contains(said), "{said}: {readme}");
        }
        assert!(!out.join(STAGING).exists());
        assert!(
            done.wrote.iter().any(|l| l == "wrote world.pack"),
            "{:?}",
            done.wrote
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn gdles_files_take_its_folders_and_one_quest_list() {
        let dir = scratch("gdle-layout");
        let json = dir.join("json");
        for (rel, text) in [
            ("weenies/90001 - guide.json", "{}"),
            ("landblocks/A9B4.json", "{}"),
            ("quests/GuideGreeted.json", "{}"),
            (
                "quests.json",
                r#"[{"key": "AncientQuest", "value": {}}, {"key": "GuideGreeted", "value": {}}]"#,
            ),
        ] {
            let p = json.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let mut alone = Quests::default();
        let wrote = gdle_layout(&json, &mut alone).unwrap();
        assert!(json.join("spawnMaps/A9B4.json").is_file());
        assert!(!json.join("landblocks").exists() && !json.join("quests").exists());
        assert!(
            wrote.last().unwrap().ends_with("quests.json: 2 quest(s)"),
            "{wrote:?}"
        );
        assert_eq!(alone, Quests::default());
        let readme = gdle_readme("w", &alone);
        assert!(
            readme.contains("the quests this world adds or changes")
                && readme.contains("--gdle-quests <file>"),
            "{readme}"
        );
        std::fs::write(json.join("quests.json"), "not a list").unwrap();
        assert!(gdle_layout(&json, &mut Quests::default()).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Given GDLE's own quests.json, the README names the quests the world adds to it and those
    /// whose entries it replaces; a quest the merged list keeps as the given file has it is
    /// neither, whatever the case of its name. The quests the world removes have a heading of
    /// their own, with or without the server's list.
    #[test]
    fn a_gdle_export_over_the_servers_quests_says_what_the_world_adds_and_changes() {
        let dir = scratch("gdle-merge");
        let json = dir.join("json");
        std::fs::create_dir_all(&json).unwrap();
        let entry = |key: &str, full: &str, delta: u32| {
            format!(
                r#"{{"key": "{key}", "value": {{"fullname": "{full}", "mindelta": {delta}, "maxsolves": 1}}}}"#
            )
        };
        let given = dir.join("quests.json");
        std::fs::write(
            &given,
            format!(
                "[{}, {}]",
                entry("bessiemilked", "Milked", 600),
                entry("HostOnly", "Mine", 5)
            ),
        )
        .unwrap();
        std::fs::write(
            json.join("quests.json"),
            format!(
                "[{}, {}, {}]",
                entry("BessieMilked", "Milked", 60),
                entry("HostOnly", "Mine", 5),
                entry("GuideGreeted", "Greeted", 1)
            ),
        )
        .unwrap();
        // The world removes a quest the server's list has: the tool leaves its entry out and
        // names it beside the list.
        std::fs::write(json.join("quests-removed.json"), r#"["OldQuest"]"#).unwrap();
        let mut quests = Quests {
            given: Some(given.clone()),
            ..Quests::default()
        };
        gdle_layout(&json, &mut quests).unwrap();
        assert_eq!(quests.added, ["GuideGreeted"]);
        assert_eq!(quests.changed, ["BessieMilked"]);
        assert_eq!(quests.removed, ["OldQuest"]);
        assert!(
            !json.join("quests-removed.json").exists(),
            "the README says it; the file goes"
        );
        let readme = gdle_readme("w", &quests);
        for said in [
            "json/quests.json over\nData/json/quests.json",
            &format!("{} with this world's quests merged in", given.display()),
            "This world adds: GuideGreeted.",
            "its entries replace that file's:\nBessieMilked.",
            "\n\nQuests this world removes (their entries are left out of json/quests.json):\n  OldQuest\n",
        ] {
            assert!(readme.contains(said), "{said}: {readme}");
        }
        // Without the server's list, the README says to take them out too.
        let alone = Quests {
            removed: vec!["OldQuest".into()],
            ..Quests::default()
        };
        let readme = gdle_readme("w", &alone);
        for said in [
            "take out the entries of the\nquests this world removes, below",
            "\n\nQuests this world removes:\n  OldQuest\n",
        ] {
            assert!(readme.contains(said), "{said}: {readme}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_export_for_empyrean_without_the_import_tool_is_refused() {
        let dir = scratch("no-tool");
        let e = export_compiled(
            &dir.join("overlay"),
            &BTreeMap::new(),
            "w",
            &dir.join("dats"),
            &Options {
                target: Target::Empyrean,
                out: &dir.join("out"),
                era: Era::Eor,
                world_sql: None,
                importer: None,
                gdle_quests: None,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("empyrean-import"), "{e}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_newest_world_database_of_each_era_is_found_in_the_cache() {
        let dir = scratch("dbs");
        for f in [
            "ACE-World-16PY-db-v0.8.8.sql",
            "ACE-World-16PY-db-v0.8.10.sql",
            "ACE-World-Database-v0.9.295.sql",
            "ACE-World-Database-v0.9.30.sql",
            "ACE-World-Database-v0.10.1.sql.zip",
            "notes.txt",
        ] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        let name = |p: Option<PathBuf>| p.unwrap().file_name().unwrap().to_owned();
        assert_eq!(
            name(newest_database(&dir)),
            "ACE-World-Database-v0.9.295.sql"
        );
        assert_eq!(
            name(newest_database_for(&dir, Era::Infiltration)),
            "ACE-World-16PY-db-v0.8.10.sql"
        );
        std::fs::remove_file(dir.join("ACE-World-Database-v0.9.295.sql")).unwrap();
        std::fs::remove_file(dir.join("ACE-World-Database-v0.9.30.sql")).unwrap();
        assert_eq!(name(newest_database(&dir)), "ACE-World-16PY-db-v0.8.10.sql");
        assert_eq!(newest_database_for(&dir, Era::Eor), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_world_database_cache_is_the_import_tools_per_user_folder() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, v)| OsString::from(*v))
            }
        };
        if cfg!(windows) {
            assert_eq!(
                database_folder(env(&[("LOCALAPPDATA", "C:\\Users\\a\\AppData\\Local")])),
                Some(PathBuf::from(
                    "C:\\Users\\a\\AppData\\Local\\Empyrean\\world-database"
                ))
            );
            assert_eq!(database_folder(env(&[("LOCALAPPDATA", "rel")])), None);
            assert_eq!(database_folder(env(&[("LOCALAPPDATA", "")])), None);
        } else if !cfg!(target_os = "macos") {
            assert_eq!(
                database_folder(env(&[("HOME", "/home/a")])),
                Some(PathBuf::from("/home/a/.cache/empyrean/world-database"))
            );
            assert_eq!(
                database_folder(env(&[("XDG_CACHE_HOME", "/c"), ("HOME", "/home/a")])),
                Some(PathBuf::from("/c/empyrean/world-database"))
            );
        }
    }
}
