//! The editor's own world: an Empyrean server it starts on the loopback, in a folder of its own,
//! over the player's data files with the world's compiled overlay laid over them and a world
//! database of its own, made from the server's SQL. Each change is pushed to it as the server's own tools take it: the packs'
//! SQL written into its content folder and imported (`import-sql`), the data files written as a
//! new overlay folder and read again (`reload-dat-overlay`), and every landblock the change
//! reaches built again. A Dereth client started against it plays the world as it is being made.
//!
//! The server is a separate program: the editor writes its configuration, talks to its console
//! through its standard input, and reads its log from its output. It never touches another
//! server, and its process ends with the editor's.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use dereth_dat::ModernDat;
use lyceum_foundry::compile::Records;
use lyceum_foundry::edit::{ServerSpawn, Session};
use lyceum_foundry::sql_ids;

/// What the editor's server is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSetup {
    /// The server program, and its import tool.
    pub server: PathBuf,
    pub import: PathBuf,
    /// The server's SQL (a dump) its world database is made from.
    pub sql: PathBuf,
    /// The data files of the era the world is built on, and the era.
    pub dats: PathBuf,
    pub era: dereth_primitives::EraId,
    /// The player's later data files, which the client plays a world of an older era with
    /// beside the world's own.
    pub client_dats: PathBuf,
    /// The folder the server runs in (made afresh), and whether it stays when the server ends.
    pub dir: PathBuf,
    pub keep: bool,
}

/// A program of the server's or the client's, `name`: in `dir` when one is named, else beside the
/// editor; `None` when it is not there.
#[must_use]
pub fn program(dir: Option<&Path>, name: &str) -> Option<PathBuf> {
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    let dir = match dir {
        Some(d) => d.to_path_buf(),
        None => std::env::current_exe().ok()?.parent()?.to_path_buf(),
    };
    Some(dir.join(file)).filter(|p| p.is_file())
}

/// A path as the server's console takes it: forward slashes, in quotes when it has a space.
#[must_use]
pub fn console_path(p: &Path) -> String {
    let s = p.display().to_string().replace('\\', "/");
    if s.contains(' ') {
        format!("\"{s}\"")
    } else {
        s
    }
}

/// The server starting: it comes up off the frame loop.
#[derive(Debug)]
pub struct Starting {
    rx: mpsc::Receiver<Result<Live, String>>,
}

impl Starting {
    /// The server, once it is up (or why it did not come up); `None` while it is starting.
    #[must_use]
    pub fn poll(&self) -> Option<Result<Live, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("the server's start ended".into())),
        }
    }

    /// The server, waited for.
    ///
    /// # Errors
    /// Why it did not come up.
    pub fn wait(self) -> Result<Live, String> {
        self.rx
            .recv()
            .map_err(|_| "the server's start ended".to_owned())?
    }
}

/// The server, running.
pub struct Live {
    child: Child,
    _lock: std::fs::File,
    stdin: ChildStdin,
    log: Arc<Mutex<Vec<String>>>,
    /// The folder it runs in, and the port it listens on (and the next).
    pub dir: PathBuf,
    pub port: u16,
    /// The overlay folder it reads now, and how many it has had.
    pub overlay: PathBuf,
    turns: u32,
    /// The SQL files pushed to its content folder, by path there: as pushed, and the import
    /// that takes them.
    pushed: Pushed,
    /// The records and deletions of the overlay it reads now.
    records: Records,
    tombstones: BTreeMap<ModernDat, BTreeSet<(u32, u32)>>,
    /// Whether the folder stays when the server ends; whether a client has made the player's
    /// character there yet.
    keep: bool,
    pub made_character: bool,
    /// What the last push said, and where the log stood before it.
    pending: Option<Pending>,
    pub revision: u64,
    pub error: Option<String>,
    content: PathBuf,
    /// The era of the world's files, and those files when the client plays them beside its own
    /// later ones.
    era: dereth_primitives::EraId,
    world_dats: PathBuf,
}

/// Where [`Live::poll_ready`] is in asking whether a playtest can log in.
#[derive(Debug, Clone, Default)]
pub struct ReadyAsk {
    stage: ReadyStage,
    /// Where the log stood when the question was asked, while its answer is awaited.
    asked: Option<usize>,
    /// When to ask again, after a "not yet".
    again: Option<std::time::Instant>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ReadyStage {
    /// The world holds no player.
    #[default]
    Empty,
    /// The account is free to log in.
    Account,
}

#[derive(Debug, Clone)]
pub struct Revision {
    pub number: u64,
    pub overlay: PathBuf,
    pub blocks: BTreeSet<u16>,
}

struct Pending {
    revision: Revision,
    plan: Push,
    records: Records,
    tombstones: BTreeMap<ModernDat, BTreeSet<(u32, u32)>>,
}

fn revision_answer(line: &str, number: u64) -> Option<Result<(), String>> {
    let (_, answer) = line.split_once(&format!("Overlay revision {number}: "))?;
    if answer.starts_with("applied;") {
        Some(Ok(()))
    } else {
        answer
            .strip_prefix("rejected; ")
            .map(|error| Err(error.to_owned()))
    }
}

impl std::fmt::Debug for Live {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Live")
            .field("dir", &self.dir)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

/// How long the editor waits for the server to say a thing before it gives up on it (a hang
/// guard: the waits end on the server's own words).
const PATIENCE: Duration = Duration::from_secs(120);

/// The server's configuration: its world named `Editor`, listening on the loopback at `port`,
/// over `dats` with the overlay in `overlay` laid over them (never patched into clients: the
/// editor's client reads the same folder), its world database `pack` (made from `sql`) with the
/// content imports kept in its folder `dir`, accounts made at their first login as
/// administrators, everything else the defaults.
#[must_use]
pub fn config_text(
    (dats, era): (&Path, dereth_primitives::EraId),
    (pack, sql): (&Path, &Path),
    dir: &Path,
    port: u16,
    overlay: &Path,
) -> String {
    let p = |p: &Path| p.display().to_string().replace('\\', "/");
    let era = match era {
        dereth_primitives::EraId::Eor => String::new(),
        other => format!("\n[era]\nprofile = \"{}\"\n", other.name()),
    };
    format!(
        "# The world editor's own server: the loopback only, written by the editor.\n\
         [server]\n\
         world_name = \"Lyceum\"\n\
         dat_files_directory = \"{dats}\"\n\
         world_pack_path = \"{pack}\"\n\
         world_overlay_path = \"{run}/content.overlay\"\n\
         world_base_sql = \"{sql}\"\n\
         interactive_console = true\n\
         landblock_preloading = false\n\
         log_level = \"info\"\n\
         update = \"off\"\n\
         \n\
         [server.network]\n\
         host = \"127.0.0.1\"\n\
         port = {port}\n\
         \n\
         [server.accounts]\n\
         allow_auto_account_creation = true\n\
         default_access_level = 5\n\
         \n\
         [database]\n\
         shard_db_path = \"{dir}/shard.db\"\n\
         auth_db_path = \"{dir}/auth.db\"\n\
         \n\
         [dat_overlay]\n\
         path = \"{overlay}\"\n\
         patching = false\n\
         \n\
         [status]\n\
         udp_ping = false\n{era}",
        run = p(overlay.parent().unwrap_or(dir)),
        dats = p(dats),
        pack = p(pack),
        sql = p(sql),
        dir = p(dir),
        overlay = p(overlay),
    )
}

/// A port on the loopback with the next one free too (the server listens on both), as the
/// system hands one out.
fn free_port() -> Result<u16, String> {
    port_pair(
        || {
            Ok(std::net::UdpSocket::bind("127.0.0.1:0")?
                .local_addr()?
                .port())
        },
        |p| std::net::UdpSocket::bind(("127.0.0.1", p)).is_ok(),
    )
}

/// The first port `next` hands out below 65000 whose next one is `free`. A system may hand ports
/// out in turn, so one near the top of the range takes some hundreds more to come round.
fn port_pair(
    mut next: impl FnMut() -> std::io::Result<u16>,
    free: impl Fn(u16) -> bool,
) -> Result<u16, String> {
    for _ in 0..2048 {
        let p = next().map_err(|e| e.to_string())?;
        if p < 65_000 && free(p + 1) {
            return Ok(p);
        }
    }
    Err("no free port on the loopback".into())
}

/// The SQL files in a content folder, by path there: their text, and the import that takes them.
pub type Pushed = BTreeMap<PathBuf, (String, String)>;

/// Where a pack's filled SQL file goes in the server's content folder, and the import that takes
/// it (`None` for a file the server's import does not take).
pub use lyceum_foundry::export::content_place;

/// What changed between two overlays' records (`old`, `new`): the landblocks whose cell records
/// changed, and whether any other file's did (a model, a texture, which any landblock may show).
#[must_use]
pub fn dat_changes(old: &Records, new: &Records) -> (BTreeSet<u16>, bool) {
    let mut blocks = BTreeSet::new();
    let mut other = false;
    let empty = BTreeMap::new();
    for file in old.keys().chain(new.keys()) {
        let (a, b) = (
            old.get(file).unwrap_or(&empty),
            new.get(file).unwrap_or(&empty),
        );
        for id in a.keys().chain(b.keys()) {
            if a.get(id) == b.get(id) {
                continue;
            }
            if *file == ModernDat::Cell {
                blocks.insert(u16::try_from(id >> 16).unwrap_or(0));
            } else {
                other = true;
            }
        }
    }
    (blocks, other)
}

fn tombstone_changes(
    old: &BTreeMap<ModernDat, BTreeSet<(u32, u32)>>,
    new: &BTreeMap<ModernDat, BTreeSet<(u32, u32)>>,
    blocks: &mut BTreeSet<u16>,
    other: &mut bool,
) {
    let empty = BTreeSet::new();
    for kind in old.keys().chain(new.keys()) {
        let a = old.get(kind).unwrap_or(&empty);
        let b = new.get(kind).unwrap_or(&empty);
        for (_, id) in a.symmetric_difference(b) {
            if *kind == ModernDat::Cell {
                blocks.insert((id >> 16) as u16);
            } else {
                *other = true;
            }
        }
    }
}

/// The statements that take away what `text` (a pack's SQL file now gone) put in: its own
/// deletions, written alone.
#[must_use]
pub fn deletions_of(text: &str) -> String {
    text.split(';')
        .map(str::trim)
        .filter(|s| {
            s.lines()
                .filter(|l| !l.trim_start().starts_with("--") && !l.trim_start().starts_with("/*"))
                .any(|l| {
                    l.trim_start()
                        .to_ascii_uppercase()
                        .starts_with("DELETE FROM")
                })
        })
        .map(|s| {
            // Only the statement, not the comments before it.
            let at = s.to_ascii_uppercase().find("DELETE FROM").unwrap_or(0);
            format!("{};\n", &s[at..])
        })
        .collect()
}

/// What a pack's file in the content folder stands in for, by the import that takes it and the
/// text pushed: a weenie class, a quest or a loot table, which is the server's own unless the
/// pack made it. Once the file goes, the server takes its own back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StandsFor {
    Weenie(u32),
    Quest(String),
    Loot(lyceum_foundry::loot::LootKind, u32),
}

/// What the file `import` takes, holding `text`, stands in for; `None` for a spawn's file.
#[must_use]
pub fn stands_for(import: &str, text: &str) -> Option<StandsFor> {
    use lyceum_foundry::loot::LootKind;
    if let Some(c) = import.strip_prefix("import-sql weenie ") {
        return c.trim().parse().ok().map(StandsFor::Weenie);
    }
    if import.starts_with("import-sql quest ") {
        let quests = lyceum_foundry::sql_ids::quests_in(text);
        return quests.into_keys().next().map(StandsFor::Quest);
    }
    let (kind, id) = import.strip_prefix("import-treasure ")?.split_once(' ')?;
    let kind = LootKind::ALL.into_iter().find(|k| k.word() == kind)?;
    Some(StandsFor::Loot(kind, id.trim().parse().ok()?))
}

/// The files of a content folder holding `pushed` that the compiled world's filled SQL (`sql`)
/// no longer has: each one's place, the text pushed and the import that took it.
#[must_use]
pub fn gone(pushed: &Pushed, sql: &BTreeMap<PathBuf, String>) -> Vec<(PathBuf, String, String)> {
    let now = placed(sql);
    pushed
        .iter()
        .filter(|(path, _)| !now.contains_key(*path))
        .map(|(path, (text, import))| (path.clone(), text.clone(), import.clone()))
        .collect()
}

/// Where each of `sql`'s files goes in the content folder: its text and the import that takes it.
fn placed(sql: &BTreeMap<PathBuf, String>) -> Pushed {
    sql.iter()
        .filter_map(|(rel, text)| {
            content_place(rel, text).map(|(path, import)| (path, (text.clone(), import)))
        })
        .collect()
}

/// One draw of a loot table, as the server says it: each thing it made, its class, how many, and
/// its name.
pub type Draw = Vec<(u32, i32, String)>;

/// The server's answer to a roll of loot table `id` in `lines` (its log from where the roll was
/// asked): each draw, once it has said all of them; or why it drew none.
#[must_use]
pub fn rolled_from(lines: &[String], id: u32) -> Option<Result<Vec<Draw>, String>> {
    let end = format!("Rolled treasure {id}: ");
    let mut draws: Vec<Draw> = Vec::new();
    for line in lines {
        if line.contains("Unknown command: roll-treasure") {
            return Some(Err(
                "this server has no roll-treasure; it needs a newer Empyrean".into(),
            ));
        }
        if line.contains("Draw a table 1 to ") {
            return Some(Err(line
                .split_once("] ")
                .map_or(line.as_str(), |(_, s)| s)
                .trim()
                .to_owned()));
        }
        if let Some((_, rest)) = line.split_once(&end) {
            return Some(if rest.contains("there is no loot table") {
                Err(rest.trim().to_owned())
            } else {
                Ok(draws)
            });
        }
        let Some((_, rest)) = line.split_once("Treasure roll ") else {
            continue;
        };
        let Some((_, made)) = rest.split_once(": ") else {
            continue;
        };
        let made = made.trim();
        if made == "nothing" {
            draws.push(Vec::new());
            continue;
        }
        draws.push(
            made.split("; ")
                .filter_map(|thing| {
                    let (class, rest) = thing.split_once(' ')?;
                    let class: u32 = class.parse().ok()?;
                    match rest.strip_prefix('x').and_then(|r| r.split_once(' ')) {
                        Some((n, name)) if n.parse::<i32>().is_ok() => {
                            Some((class, n.parse().unwrap_or(1), name.to_owned()))
                        }
                        _ => Some((class, 1, rest.to_owned())),
                    }
                })
                .collect(),
        );
    }
    None
}

/// What a push does: the files written into the content folder (their text, and the import that
/// takes them), the imports, the landblocks whose spawns changed, and the weenies that changed
/// (whose landblocks are built again too).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Push {
    pub writes: Pushed,
    pub imports: Vec<String>,
    pub landblocks: BTreeSet<u16>,
    pub classes: BTreeSet<u32>,
}

/// What the server has of its own, given back where a pack's change of it goes: its own text for
/// a file by its place in the content folder (a weenie class, a quest or a loot table a pack had a
/// copy of), its own spawn of a landblock by guid, and the date the rows written are stamped with.
pub struct Own<'a> {
    pub file: &'a dyn Fn(&Path) -> Option<String>,
    pub spawn: &'a dyn Fn(u16, u32) -> Option<ServerSpawn>,
    pub today: &'a str,
}

impl std::fmt::Debug for Own<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Own")
            .field("today", &self.today)
            .finish_non_exhaustive()
    }
}

/// The landblock whose spawns the import `import` takes.
fn landblock_import(import: &str) -> Option<u16> {
    import
        .strip_prefix("import-sql landblock ")
        .and_then(|l| u16::from_str_radix(l.trim(), 16).ok())
}

/// The guids of the spawns `text` writes again or takes away.
fn spawn_guids(text: &str) -> BTreeSet<u32> {
    let placed = sql_ids::instances(text);
    let placed = placed.iter().map(|i| i.guid.as_str());
    let deleted = sql_ids::deleted_instances(text);
    placed
        .chain(deleted.iter().map(String::as_str))
        .filter_map(sql_ids::number)
        .collect()
}

/// The spawns a content folder's landblock files change: those any of them writes again or takes
/// away, and those taken away and not written again, which are not in the world.
#[derive(Debug, Default)]
struct Changed {
    spawns: BTreeSet<u32>,
    absent: BTreeSet<u32>,
}

impl Changed {
    /// Those of `files`, each its text and the import that takes it.
    fn of<'a>(files: impl IntoIterator<Item = &'a (String, String)>) -> Self {
        let (mut placed, mut deleted) = (BTreeSet::new(), BTreeSet::new());
        for (text, import) in files {
            if landblock_import(import).is_none() {
                continue;
            }
            placed.extend(
                sql_ids::instances(text)
                    .iter()
                    .filter_map(|i| sql_ids::number(&i.guid)),
            );
            deleted.extend(
                sql_ids::deleted_instances(text)
                    .iter()
                    .filter_map(|g| sql_ids::number(g)),
            );
        }
        Self {
            spawns: placed.union(&deleted).copied().collect(),
            absent: deleted.difference(&placed).copied().collect(),
        }
    }
}

/// Whether `statement`, one of a file's deletions, takes away one of the spawns `guids` or a link
/// to one.
fn deletes_one_of(statement: &str, guids: &BTreeSet<u32>) -> bool {
    let flat = statement
        .replace('`', "")
        .replace('=', " = ")
        .to_ascii_lowercase();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.starts_with("delete from landblock_instance")
        && flat
            .rsplit_once("guid = ")
            .and_then(|(_, v)| sql_ids::number(v.trim_end_matches(';').trim()))
            .is_some_and(|g| guids.contains(&g))
}

/// The statements that write the server's own spawns among `back` of landblock `lb` again as it
/// places them, and which of them it has: each one taken away (its links as a generator go with
/// it) with its link from each generator that keeps one, then placed whole with those links and
/// its own. A link to a spawn not in the world is left out, and so is one from a generator another
/// file writes, which writes its links itself (`world` says which).
fn spawns_back(
    lb: u16,
    back: &BTreeSet<u32>,
    world: &Changed,
    own: &Own<'_>,
) -> (BTreeSet<u32>, String) {
    let found: BTreeMap<u32, ServerSpawn> = back
        .iter()
        .filter_map(|g| Some((*g, (own.spawn)(lb, *g)?)))
        .collect();
    let given: BTreeSet<u32> = found.keys().copied().collect();
    if given.is_empty() {
        return (given, String::new());
    }
    let keeps =
        |p: &u32| !given.contains(p) && !world.absent.contains(p) && !world.spawns.contains(p);
    let mut out =
        format!("\n/* The server's own spawns of landblock {lb:04X}, as it places them. */\n");
    for g in &given {
        out.push_str(&format!(
            "DELETE FROM `landblock_instance` WHERE `guid` = 0x{g:08X};\n"
        ));
    }
    let mut links: Vec<(u32, u32)> = Vec::new();
    for (g, s) in &found {
        for p in s.parents.iter().filter(|p| keeps(p)) {
            out.push_str(&format!(
                "DELETE FROM `landblock_instance_link` WHERE `parent_GUID` = 0x{p:08X} AND \
                 `child_GUID` = 0x{g:08X};\n"
            ));
            links.push((*p, *g));
        }
        links.extend(
            s.children
                .iter()
                .filter(|c| !world.absent.contains(c))
                .map(|c| (*g, *c)),
        );
    }
    let rows: Vec<String> = found
        .iter()
        .map(|(g, s)| {
            let i = &s.instance;
            let [w, x, y, z] = i.angles;
            format!(
                "(0x{g:08X}, {}, 0x{:08X}, {}, {}, {}, {w}, {x}, {y}, {z}, {}, '{}')",
                i.weenie,
                i.cell,
                i.origin[0],
                i.origin[1],
                i.origin[2],
                if i.link_child { "True" } else { "False" },
                own.today
            )
        })
        .collect();
    out.push_str(&format!(
        "\nINSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, \
         `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, \
         `last_Modified`)\nVALUES {};\n",
        rows.join("\n     , ")
    ));
    if !links.is_empty() {
        let rows: Vec<String> = links
            .iter()
            .map(|(p, c)| format!("(0x{p:08X}, 0x{c:08X}, '{}')", own.today))
            .collect();
        out.push_str(&format!(
            "\nINSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, \
             `last_Modified`)\nVALUES {};\n",
            rows.join("\n     , ")
        ));
    }
    (given, out)
}

/// The text file `path`, taken by `import`, is written with to go from holding `from` to what the
/// world has for it, `to` (`None`: nothing): `to`; else the server's own text for it; else
/// `from`'s deletions alone. A landblock's file also writes back, as the server places them, the
/// server's own spawns `from` changed that no file of the world (`world`) changes now; gone, it
/// keeps no deletion of those, nor of a spawn another file now writes.
fn file_text(
    path: &Path,
    (import, from, to): (&str, Option<&str>, Option<&str>),
    world: &Changed,
    own: &Own<'_>,
) -> String {
    let from = from.unwrap_or_default();
    let Some(lb) = landblock_import(import) else {
        return to
            .map(str::to_owned)
            .or_else(|| (own.file)(path))
            .unwrap_or_else(|| deletions_of(from));
    };
    let back: BTreeSet<u32> = spawn_guids(from)
        .difference(&world.spawns)
        .copied()
        .collect();
    let (given, rows) = spawns_back(lb, &back, world, own);
    match to {
        Some(text) => format!("{text}{rows}"),
        None => {
            let drop: BTreeSet<u32> = given.union(&world.spawns).copied().collect();
            let kept: String = deletions_of(from)
                .split_inclusive(";\n")
                .filter(|s| !deletes_one_of(s, &drop))
                .collect();
            format!("{kept}{rows}")
        }
    }
}

/// The push that brings a content folder holding `pushed` up to `sql` (the compiled world's
/// filled SQL, by path under `sql`). A file gone is taken back by its deletions alone, or, where
/// the server has its own text for it in `own` (a weenie class, a quest or a loot table the pack
/// had a copy of), by that. The server's own spawns a pack's file moved or took away and no
/// longer does are written back as the server places them, by that file.
#[must_use]
pub fn plan(pushed: &Pushed, sql: &BTreeMap<PathBuf, String>, own: &Own<'_>) -> Push {
    let mut out = Push::default();
    let now = placed(sql);
    let world = Changed::of(now.values());
    let mut imports: BTreeSet<String> = BTreeSet::new();
    let mut note = |import: &str, out: &mut Push| {
        imports.insert(import.to_owned());
        if let Some(c) = import
            .strip_prefix("import-sql weenie ")
            .and_then(|c| c.parse().ok())
        {
            out.classes.insert(c);
        }
        if let Some(lb) = landblock_import(import) {
            out.landblocks.insert(lb);
        }
    };
    for (path, (text, import)) in &now {
        let from = pushed.get(path).map(|p| p.0.as_str());
        let text: std::borrow::Cow<'_, str> = if landblock_import(import).is_some() {
            file_text(path, (import, from, Some(text)), &world, own).into()
        } else {
            text.into()
        };
        if from != Some(&*text) {
            out.writes
                .insert(path.clone(), (text.into_owned(), import.clone()));
            note(import, &mut out);
        }
    }
    // A file gone: its deletions alone, imported, take back what it put in (once); or the
    // server's own text for it.
    for (path, (old, import)) in pushed {
        if now.contains_key(path) {
            continue;
        }
        let gone = file_text(path, (import, Some(old), None), &world, own);
        if *old == gone {
            continue;
        }
        out.writes.insert(path.clone(), (gone, import.clone()));
        note(import, &mut out);
    }
    out.imports = imports.into_iter().collect();
    out
}

/// What takes the files a refused revision wrote (`writes`) back to what the content folder held
/// before it (`pushed`): each one's text before; a file new to the folder, the server's own text
/// for it in `own`, else its deletions alone; and the server's own spawns the revision moved or
/// took away written back as the server places them.
#[must_use]
pub fn undo(pushed: &Pushed, writes: &Pushed, own: &Own<'_>) -> Pushed {
    let world = Changed::of(pushed.values());
    writes
        .iter()
        .map(|(path, (text, import))| {
            let before = pushed.get(path).map(|p| p.0.as_str());
            let back = file_text(path, (import, Some(text), before), &world, own);
            (path.clone(), (back, import.clone()))
        })
        .collect()
}

/// Even an unmodified world carries an empty container naming the playtest world.
fn write_play_overlay(session: &Session, dir: &Path) -> Result<(), String> {
    use dereth_dat::overlay::{container_name, OverlayDir, OverlayWriter};
    lyceum_foundry::compile::write_overlay(
        session.compiled(),
        session.base(),
        &session.world().key,
        dir,
    )
    .map_err(|e| e.to_string())?;
    if OverlayDir::new(dir)
        .map_err(|e| e.to_string())?
        .world_key()
        .is_none()
    {
        let kind = ModernDat::Cell;
        let mut writer = OverlayWriter::open_or_create(
            &dir.join(container_name(kind)),
            &session.base().cell,
            kind.file_name(),
            &session.world().key,
            0,
        )
        .map_err(|e| e.to_string())?;
        writer.flush(0).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Keep character databases and reserve a fresh overlay directory for this run.
fn prepare_folder(dir: &Path) -> Result<(std::fs::File, PathBuf), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("playtest.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock()
        .map_err(|e| format!("the playtest folder is already in use: {e}"))?;
    std::fs::create_dir_all(dir.join("content")).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut serial = 0_u64;
    let run = loop {
        let run = dir.join(format!("run-{}-{serial}", std::process::id()));
        match std::fs::create_dir(&run) {
            Ok(()) => break run,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => serial += 1,
            Err(e) => return Err(e.to_string()),
        }
    };
    Ok((lock, run))
}

impl Live {
    /// Start the server for `session`'s world as `setup` says: its folder and the data overlay
    /// written here, the rest off the frame loop. Once it is up, a [`Self::push`] gives it the
    /// packs' SQL.
    ///
    /// # Errors
    /// The folder or the overlay would not write.
    pub fn launch(setup: &LiveSetup, session: &Session) -> Result<Starting, String> {
        let dir = setup.dir.clone();
        let (lock, run) = prepare_folder(&dir)?;
        let overlay = run.join("overlay-0");
        let compiled = session.compiled();
        write_play_overlay(session, &overlay)?;
        let (records, tombstones) = (compiled.records.clone(), compiled.tombstones.clone());
        let setup = setup.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // A start nobody waits for any more ends its server as it is dropped.
            let _ = tx.send(Self::start(&setup, overlay, records, tombstones, lock));
        });
        Ok(Starting { rx })
    }

    /// The rest of the start: the world database, the configuration, the program, and the
    /// content folder set.
    fn start(
        setup: &LiveSetup,
        overlay: PathBuf,
        records: Records,
        tombstones: BTreeMap<ModernDat, BTreeSet<(u32, u32)>>,
        lock: std::fs::File,
    ) -> Result<Self, String> {
        let dir = setup.dir.clone();
        let pack = dir.join("world.pack");
        let mut import = Command::new(&setup.import);
        import
            .arg("--sql")
            .arg(&setup.sql)
            .arg("--era")
            .arg(setup.era.name())
            .arg("--out")
            .arg(&pack)
            .stdin(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            import.creation_flags(0x0800_0000);
        }
        let out = import
            .output()
            .map_err(|e| format!("{}: {e}", setup.import.display()))?;
        if !out.status.success() {
            return Err(format!(
                "{} made no world database of {}: {}",
                setup.import.display(),
                setup.sql.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let content = overlay.parent().unwrap().join("content");
        std::fs::create_dir_all(&content).map_err(|e| e.to_string())?;
        let port = free_port()?;
        let config = dir.join("empyrean.toml");
        std::fs::write(
            &config,
            config_text(
                (&setup.dats, setup.era),
                (&pack, &setup.sql),
                &dir,
                port,
                &overlay,
            ),
        )
        .map_err(|e| format!("{}: {e}", config.display()))?;
        let mut cmd = Command::new(&setup.server);
        cmd.arg("--config")
            .arg(&config)
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No console window of its own.
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("{}: {e}", setup.server.display()))?;
        let stdin = child.stdin.take().ok_or("the server's input")?;
        let log = Arc::new(Mutex::new(Vec::new()));
        // The log is kept in the folder too.
        let file = Arc::new(Mutex::new(
            std::fs::File::create(dir.join("server.log")).ok(),
        ));
        for out in [
            child
                .stdout
                .take()
                .map(|o| Box::new(o) as Box<dyn std::io::Read + Send>),
            child
                .stderr
                .take()
                .map(|o| Box::new(o) as Box<dyn std::io::Read + Send>),
        ]
        .into_iter()
        .flatten()
        {
            let (log, file) = (Arc::clone(&log), Arc::clone(&file));
            std::thread::spawn(move || {
                for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                    if let Some(f) = file
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .as_mut()
                    {
                        let _ = writeln!(f, "{line}");
                    }
                    log.lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(line);
                }
            });
        }
        let mut live = Self {
            child,
            _lock: lock,
            stdin,
            log,
            dir: dir.clone(),
            port,
            overlay: overlay.clone(),
            turns: 0,
            pushed: BTreeMap::new(),
            records,
            tombstones,
            keep: setup.keep,
            made_character: false,
            pending: None,
            revision: 0,
            error: None,
            content: overlay.parent().unwrap().join("content"),
            era: setup.era,
            world_dats: setup.dats.clone(),
        };
        live.wait_for("command prompt ready", 0).ok_or_else(|| {
            format!(
                "the server did not come up: {}",
                live.log_from(0).last().cloned().unwrap_or_default()
            )
        })?;
        let mark = live.log_len();
        live.command(&format!(
            "modifystring content_folder {}",
            console_path(&live.content)
        ))?;
        live.wait_for("content_folder", mark)
            .ok_or("the server did not take its content folder")?;
        Ok(live)
    }

    /// The content folder the packs' SQL is written into.
    #[must_use]
    pub fn content(&self) -> &Path {
        &self.content
    }

    /// How many lines the server has said.
    #[must_use]
    pub fn log_len(&self) -> usize {
        self.log
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// The server's lines from line `from` on.
    #[must_use]
    pub fn log_from(&self, from: usize) -> Vec<String> {
        self.log
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(from..)
            .map(<[String]>::to_vec)
            .unwrap_or_default()
    }

    /// The first line from line `from` on that holds `needle`, waiting for the server to say it
    /// (a hang guard ends the wait: `None`, or the server ended).
    #[must_use]
    pub fn wait_for(&mut self, needle: &str, from: usize) -> Option<String> {
        self.wait_for_any(&[needle], from)
    }

    /// The first line from line `from` on that holds any of `needles`, waited for as
    /// [`Self::wait_for`] waits.
    #[must_use]
    pub fn wait_for_any(&mut self, needles: &[&str], from: usize) -> Option<String> {
        let start = std::time::Instant::now();
        loop {
            if let Some(l) = self
                .log_from(from)
                .into_iter()
                .find(|l| needles.iter().any(|n| l.contains(n)))
            {
                return Some(l);
            }
            if start.elapsed() > PATIENCE || !self.running() {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Whether both consumers have adopted the last publication.
    pub fn settle(&mut self) -> bool {
        self.pending.is_none()
    }

    /// Wait for the world to have no player in it (a character that logged off is in it until
    /// its logout ends, and cannot log in again until then), asking the server its population.
    /// False when it did not empty.
    pub fn wait_empty(&mut self) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < PATIENCE {
            let mark = self.log_len();
            if self.command("pop").is_err() {
                return false;
            }
            let Some(line) = self.wait_for("Current world population:", mark) else {
                return false;
            };
            if line.trim_end().ends_with(": 0") {
                return true;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        false
    }

    /// Wait for the previous account connection to finish disconnecting.
    pub fn wait_account(&mut self) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < PATIENCE {
            let mark = self.log_len();
            if self.command("playtest-character account builder").is_err() {
                return false;
            }
            let Some(line) = self.wait_for("Playtest account:", mark) else {
                return false;
            };
            if line.trim_end().ends_with(": ready") {
                return true;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        false
    }

    /// One look at whether a playtest can log in now: the world holds none of its players (a
    /// character that logged off is in it until its logout ends) and the account is free. Asks the
    /// server again a quarter second after each "not yet", so the frame loop never waits on it.
    ///
    /// # Errors
    /// The server's console is closed, or the server ended.
    pub fn poll_ready(&mut self, ask: &mut ReadyAsk) -> Result<bool, String> {
        if !self.running() {
            return Err("the editor's world stopped".into());
        }
        let (question, needle, yes) = match ask.stage {
            ReadyStage::Empty => ("pop", "Current world population:", ": 0"),
            ReadyStage::Account => (
                "playtest-character account builder",
                "Playtest account:",
                ": ready",
            ),
        };
        if let Some(mark) = ask.asked {
            let Some(line) = self.log_from(mark).into_iter().find(|l| l.contains(needle)) else {
                return Ok(false);
            };
            ask.asked = None;
            if !line.trim_end().ends_with(yes) {
                ask.again = Some(std::time::Instant::now() + Duration::from_millis(250));
                return Ok(false);
            }
            if ask.stage == ReadyStage::Account {
                return Ok(true);
            }
            ask.stage = ReadyStage::Account;
            return self.poll_ready(ask);
        }
        if ask.again.is_some_and(|t| std::time::Instant::now() < t) {
            return Ok(false);
        }
        ask.again = None;
        ask.asked = Some(self.log_len());
        self.command(question)?;
        Ok(false)
    }

    /// Ask the server to draw loot table `id` `times` times, making nothing: where its log stood,
    /// to read the answer from ([`Self::rolled`]).
    ///
    /// # Errors
    /// The server's console is closed.
    pub fn roll(&mut self, id: u32, times: u32) -> Result<usize, String> {
        let mark = self.log_len();
        self.command(&format!("roll-treasure {id} {times}"))?;
        Ok(mark)
    }

    /// The server's answer to a roll of loot table `id` asked when its log stood at `mark`, once
    /// it has all of it.
    #[must_use]
    pub fn rolled(&self, id: u32, mark: usize) -> Option<Result<Vec<Draw>, String>> {
        rolled_from(&self.log_from(mark), id)
    }

    /// Keep the folder when the server ends (its log is there).
    pub fn keep_folder(&mut self) {
        self.keep = true;
    }

    /// Whether the server is still running.
    pub fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Say `line` at the server's console.
    ///
    /// # Errors
    /// The server's input is closed.
    pub fn command(&mut self, line: &str) -> Result<(), String> {
        writeln!(self.stdin, "{line}")
            .and_then(|()| self.stdin.flush())
            .map_err(|e| format!("the server's console: {e}"))
    }

    /// The files pushed that `session`'s world no longer has ([`gone`]): what [`Self::push`]
    /// takes back, each by the server's own text when it has one.
    #[must_use]
    pub fn gone(&self, session: &Session) -> Vec<(PathBuf, String, String)> {
        gone(&self.pushed, &session.compiled().sql)
    }

    /// Bring the server up to `session`'s world: its SQL written into the content folder and
    /// imported, its data files written as a new overlay folder when their records changed, and
    /// every landblock the change reaches built again: those whose cell records or spawns changed,
    /// those a changed weenie stands in, and, when a weenie or a model or texture changed, those
    /// `near` (where the creator plays, and where a generator may make the weenie). A file gone
    /// from the world is taken back by the server's own text for it in `restore` (by its place in
    /// the content folder), else by its deletions alone; the server's own spawns a pack's file no
    /// longer moves or takes away are written back as `session` knows them. Answers the console
    /// lines it said.
    ///
    /// # Errors
    /// A file or the overlay would not write, or the console is closed; or the server refused the
    /// revision, which is then left pending for [`Self::reject_pending`] to take back.
    pub fn push(
        &mut self,
        session: &Session,
        near: &BTreeSet<u16>,
        restore: &BTreeMap<PathBuf, String>,
    ) -> Result<Option<Revision>, String> {
        if self.pending.is_some() {
            return Err("A live revision is still being adopted.".into());
        }
        let compiled = session.compiled();
        let file = |path: &Path| restore.get(path).cloned();
        let spawn = |lb, guid| session.server_own_spawn(lb, guid);
        let own = Own {
            file: &file,
            spawn: &spawn,
            today: session.stamp(),
        };
        let mut p = plan(&self.pushed, &compiled.sql, &own);
        if !p.classes.is_empty() {
            p.landblocks.extend(session.landblocks_standing(&p.classes));
            p.landblocks.extend(near);
        }
        let (mut dat_blocks, mut other) = dat_changes(&self.records, &compiled.records);
        tombstone_changes(
            &self.tombstones,
            &compiled.tombstones,
            &mut dat_blocks,
            &mut other,
        );
        let changed = !dat_blocks.is_empty() || other || self.tombstones != compiled.tombstones;
        if other {
            p.landblocks.extend(near);
        }
        p.landblocks.extend(dat_blocks);
        if !changed && p.writes.is_empty() {
            return Ok(None);
        }
        self.turns += 1;
        let overlay = self
            .overlay
            .parent()
            .unwrap()
            .join(format!("overlay-{}", self.turns));
        write_play_overlay(session, &overlay)?;
        let revision = Revision {
            number: u64::from(self.turns),
            overlay,
            blocks: p.landblocks.clone(),
        };
        self.pending = Some(Pending {
            revision: revision.clone(),
            plan: p,
            records: compiled.records.clone(),
            tombstones: compiled.tombstones.clone(),
        });
        self.publish_pending()?;
        Ok(Some(revision))
    }

    fn import_files(&mut self, files: &Pushed) -> Result<(), String> {
        for (path, (text, command)) in files {
            let to = self.content.join(path);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&to, text).map_err(|e| e.to_string())?;
            let mark = self.log_len();
            self.command(command)?;
            let refused = "There was an error importing";
            let answer = self
                .wait_for_any(&["Imported ", "Failed to import ", refused], mark)
                .ok_or_else(|| format!("No acknowledgement for {command}"))?;
            if self
                .log_from(mark)
                .iter()
                .any(|l| l.contains(" ERROR ") || l.contains("Failed to ") || l.contains(refused))
            {
                return Err(format!("Import failed: {answer}"));
            }
        }
        Ok(())
    }

    fn reload_revision(&mut self, revision: &Revision) -> Result<(), String> {
        let mark = self.log_len();
        let blocks: Vec<_> = revision.blocks.iter().map(|b| format!("{b:04X}")).collect();
        self.command(&format!(
            "reload-dat-overlay --revision {} {} {}",
            revision.number,
            console_path(&revision.overlay),
            blocks.join(" ")
        ))?;
        let prefix = format!("Overlay revision {}:", revision.number);
        let line = self
            .wait_for(&prefix, mark)
            .ok_or("The server did not acknowledge the live revision.")?;
        revision_answer(&line, revision.number)
            .unwrap_or_else(|| Err("Unexpected revision acknowledgement.".into()))
    }

    fn publish_pending(&mut self) -> Result<(), String> {
        let pending = self.pending.as_ref().unwrap();
        let files = pending.plan.writes.clone();
        let revision = pending.revision.clone();
        self.import_files(&files)?;
        self.reload_revision(&revision)
    }

    /// Both the server and the playing client have adopted this revision.
    pub fn accept_pending(&mut self) {
        if let Some(p) = self.pending.take() {
            self.pushed.extend(p.plan.writes);
            self.records = p.records;
            self.tombstones = p.tombstones;
            self.overlay = p.revision.overlay;
            self.revision = p.revision.number;
        }
    }

    /// The files the revision pending wrote that the content folder did not hold before it, each
    /// one's place there, the text written and the import that took it: what a refusal of it
    /// takes away, each by the server's own text when it has one ([`Self::reject_pending`]).
    #[must_use]
    pub fn refused(&self) -> Vec<(PathBuf, String, String)> {
        let Some(p) = self.pending.as_ref() else {
            return Vec::new();
        };
        p.plan
            .writes
            .iter()
            .filter(|(path, _)| !self.pushed.contains_key(*path))
            .map(|(path, (text, import))| (path.clone(), text.clone(), import.clone()))
            .collect()
    }

    /// Put back the last published content when the server or the playing client refuses the
    /// revision pending ([`undo`]): a file new to the content folder by the server's own text for
    /// it in `restore` (by its place there), else by its deletions alone, and the server's own
    /// spawns the revision moved or took away as `session` knows them.
    ///
    /// # Errors
    /// A file would not write, the console is closed, or the server refused this too.
    pub fn reject_pending(
        &mut self,
        session: &Session,
        restore: &BTreeMap<PathBuf, String>,
    ) -> Result<(), String> {
        let Some(p) = self.pending.take() else {
            return Ok(());
        };
        let file = |path: &Path| restore.get(path).cloned();
        let spawn = |lb, guid| session.server_own_spawn(lb, guid);
        let own = Own {
            file: &file,
            spawn: &spawn,
            today: session.stamp(),
        };
        let back = undo(&self.pushed, &p.plan.writes, &own);
        self.import_files(&back)?;
        self.pushed.extend(back);
        self.turns += 1;
        self.reload_revision(&Revision {
            number: u64::from(self.turns),
            overlay: self.overlay.clone(),
            blocks: p.revision.blocks,
        })
    }

    /// The Dereth client's command line that plays this world: the account `builder` (made at
    /// its first login), its character `Builder` made the first time and taken in after, put at
    /// `cell` and the cell meters `at` once in the world and left there (the client logs off a
    /// day later); the data files `dats` with the server's overlay laid over them.
    #[must_use]
    pub fn play_args(&self, dats: &Path, cell: u32, at: [f32; 3]) -> Vec<String> {
        let mut v: Vec<String> = [
            "-h",
            &format!("127.0.0.1:{}", self.port),
            "-a",
            "builder",
            "-v",
            "builder",
            "--enter-world",
            "--linger",
            "86400",
            if self.made_character { "-u" } else { "-r" },
            "Builder",
            "--dat-dir",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        v.push(dats.display().to_string());
        if self.era.container_era() == dereth_dat::ContainerEra::Classic {
            // The world's own files, beside the player's later ones.
            v.push("--classic-dat-dir".into());
            v.push(self.world_dats.display().to_string());
        }
        if self.era != dereth_primitives::EraId::Eor {
            v.push("--era".into());
            v.push(self.era.name().into());
        }
        v.push("--overlay-dat-dir".into());
        v.push(self.overlay.display().to_string());
        v.push("--say".into());
        v.push(format!(
            "@teleloc 0x{cell:08X} [{:.3} {:.3} {:.3}] 1 0 0 0",
            at[0], at[1], at[2]
        ));
        v
    }

    /// End the server: `exit` at its console, then waited for (killed when it does not end).
    pub fn stop(&mut self) {
        let _ = self.command("exit");
        let start = std::time::Instant::now();
        while self.running() && start.elapsed() < Duration::from_secs(30) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        if self.running() {
            self.stop();
        }
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: what the world editor tells its own server)
    use super::*;

    fn no_file(_: &Path) -> Option<String> {
        None
    }

    fn no_spawn(_: u16, _: u32) -> Option<ServerSpawn> {
        None
    }

    /// Nothing of the server's own.
    fn none() -> Own<'static> {
        Own {
            file: &no_file,
            spawn: &no_spawn,
            today: "2026-10-25 00:00:00",
        }
    }

    /// The server's own spawns of landblock 0101: a generator, the instance it links, and one on
    /// its own.
    const GENERATOR: u32 = 0x7001_0000;
    const CHILD: u32 = 0x7001_0001;
    const ALONE: u32 = 0x7001_0002;

    fn server_spawn(lb: u16, guid: u32) -> Option<ServerSpawn> {
        let instance = |weenie: &str, x: f32, link_child: bool| sql_ids::Instance {
            guid: format!("0x{guid:08X}"),
            weenie: weenie.into(),
            cell: 0x0101_0019,
            cell_symbol: None,
            origin: [x, 4.373_15, 94.005],
            angles: [0.923_88, 0.0, 0.0, -0.382_683],
            link_child,
            line: 0,
        };
        if lb != 0x0101 {
            return None;
        }
        Some(match guid {
            GENERATOR => ServerSpawn {
                instance: instance("1154", 84.0067, false),
                children: vec![CHILD],
                parents: Vec::new(),
            },
            CHILD => ServerSpawn {
                instance: instance("7924", 86.0, true),
                children: Vec::new(),
                parents: vec![GENERATOR],
            },
            ALONE => ServerSpawn {
                instance: instance("7924", 88.0, false),
                children: Vec::new(),
                parents: Vec::new(),
            },
            _ => return None,
        })
    }

    /// The pack's override file of landblock 0101, as the session writes it: the generator moved
    /// (with its link), and each of `removed` taken away.
    fn overrides(moved: bool, removed: &[u32]) -> String {
        let mut out = String::from(
            "/* The server's own spawns of landblock 0101 this pack changes. */
",
        );
        if moved {
            out.push_str("
DELETE FROM `landblock_instance` WHERE `guid` = 0x70010000;

INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES (0x70010000, 1154, 0x01010019, 90.0, 10.0, 94.0, 1.0, 0.0, 0.0, 0.0, False, '2026-10-19 00:00:00');

INSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`)
VALUES (0x70010000, 0x70010001, '2026-10-19 00:00:00');
");
        }
        for g in removed {
            out.push_str(&format!(
                "
DELETE FROM `landblock_instance` WHERE `guid` = 0x{g:08X};
"
            ));
            if *g == CHILD {
                out.push_str(
                    "DELETE FROM `landblock_instance_link` WHERE `child_GUID` = 0x70010001;
",
                );
            }
        }
        out
    }

    #[test]
    fn deleting_or_restoring_a_cell_record_reloads_its_landblock() {
        let removed = [(ModernDat::Cell, [(1, 0xA9B40100)].into_iter().collect())]
            .into_iter()
            .collect();
        let mut blocks = BTreeSet::new();
        let mut other = false;
        tombstone_changes(&BTreeMap::new(), &removed, &mut blocks, &mut other);
        assert_eq!(blocks, [0xA9B4].into_iter().collect());
        assert!(!other);
        blocks.clear();
        tombstone_changes(&removed, &BTreeMap::new(), &mut blocks, &mut other);
        assert_eq!(blocks, [0xA9B4].into_iter().collect());
    }

    #[test]
    fn only_the_requested_revision_acknowledges_publication_and_rejection_is_not_success() {
        assert_eq!(
            revision_answer("Overlay revision 7: applied; 1 of 1 landblocks rebuilt.", 7),
            Some(Ok(()))
        );
        assert_eq!(
            revision_answer("Overlay revision 6: applied; 1 of 1 landblocks rebuilt.", 7),
            None
        );
        assert_eq!(
            revision_answer(
                "Overlay revision 70: applied; 1 of 1 landblocks rebuilt.",
                7
            ),
            None
        );
        assert_eq!(
            revision_answer("Overlay revision 7: rejected; invalid overlay", 7),
            Some(Err("invalid overlay".into()))
        );
        assert_eq!(
            revision_answer("The data overlay was not read again: invalid overlay", 7),
            None
        );
    }

    #[test]
    fn starting_a_playtest_preserves_characters_and_excludes_a_second_editor() {
        let dir = std::env::temp_dir().join(format!("lyceum-persistent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let database = dir.join("shard.db");
        std::fs::write(&database, b"saved character").unwrap();
        let (lock, first) = prepare_folder(&dir).unwrap();
        assert!(prepare_folder(&dir).is_err());
        assert_eq!(std::fs::read(&database).unwrap(), b"saved character");
        drop(lock);
        let (lock, second) = prepare_folder(&dir).unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read(&database).unwrap(), b"saved character");
        drop(lock);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_servers_configuration_is_the_loopback_its_folder_and_the_overlay_never_patched() {
        let t = config_text(
            (Path::new("C:\\dats"), dereth_primitives::EraId::Eor),
            (
                Path::new("D:/live/world.pack"),
                Path::new("D:\\db\\dump.sql"),
            ),
            Path::new("D:/live"),
            9123,
            Path::new("D:/live/overlay-0"),
        );
        for line in [
            "dat_files_directory = \"C:/dats\"",
            "world_pack_path = \"D:/live/world.pack\"",
            "world_base_sql = \"D:/db/dump.sql\"",
            "host = \"127.0.0.1\"",
            "port = 9123",
            "path = \"D:/live/overlay-0\"",
            "patching = false",
            "interactive_console = true",
            "shard_db_path = \"D:/live/shard.db\"",
        ] {
            assert!(t.contains(line), "{line}\n{t}");
        }
        assert!(t.parse::<toml::Table>().is_ok(), "TOML: {t}");
        assert!(!t.contains("[era]"), "the end of retail's is the default");
        // A world built on Infiltration's files plays that era over them.
        let t = config_text(
            (
                Path::new("C:/feb2005"),
                dereth_primitives::EraId::Infiltration,
            ),
            (Path::new("D:/live/world.pack"), Path::new("D:/db/16py.sql")),
            Path::new("D:/live"),
            9123,
            Path::new("D:/live/overlay-0"),
        );
        let table = t.parse::<toml::Table>().unwrap();
        assert_eq!(table["era"]["profile"].as_str(), Some("infiltration"));
        assert!(t.contains("dat_files_directory = \"C:/feb2005\""), "{t}");
    }

    #[test]
    fn a_path_with_a_space_reaches_the_servers_console_in_quotes() {
        assert_eq!(
            console_path(Path::new("C:\\w\\overlay-1")),
            "C:/w/overlay-1"
        );
        assert_eq!(
            console_path(Path::new("C:\\My Worlds\\overlay-1")),
            "\"C:/My Worlds/overlay-1\""
        );
    }

    #[test]
    fn a_changed_cell_record_names_its_landblock_and_any_other_file_names_none() {
        use lyceum_foundry::compile::Staged;
        let staged = |b: u8| Staged {
            bytes: vec![b],
            version: 1,
        };
        let records = |cell: &[(u32, u8)], portal: &[(u32, u8)]| -> Records {
            let mut r = Records::new();
            r.entry(ModernDat::Cell)
                .or_default()
                .extend(cell.iter().map(|(i, b)| (*i, staged(*b))));
            r.entry(ModernDat::Portal)
                .or_default()
                .extend(portal.iter().map(|(i, b)| (*i, staged(*b))));
            r
        };
        let a = records(&[(0xA9B4_FFFE, 1), (0xA9B5_FFFF, 1)], &[(0x0200_0001, 1)]);
        assert_eq!(dat_changes(&a, &a), (BTreeSet::new(), false));
        let b = records(&[(0xA9B4_FFFE, 2), (0xA9B5_FFFF, 1)], &[(0x0200_0001, 1)]);
        assert_eq!(dat_changes(&a, &b), ([0xA9B4].into_iter().collect(), false));
        // A record gone, and one new.
        let c = records(&[(0xA9B5_FFFF, 1), (0x0101_0100, 1)], &[(0x0200_0001, 1)]);
        assert_eq!(
            dat_changes(&a, &c),
            ([0xA9B4, 0x0101].into_iter().collect(), false)
        );
        let d = records(&[(0xA9B4_FFFE, 1), (0xA9B5_FFFF, 1)], &[(0x0200_0001, 3)]);
        assert_eq!(dat_changes(&a, &d), (BTreeSet::new(), true));
    }

    #[test]
    fn a_push_writes_and_imports_only_what_changed() {
        let weenie = |name: &str| {
            format!(
                "DELETE FROM `weenie` WHERE `class_Id` = 90001;\n\nINSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (90001, '{name}', 10, '2026-10-19 00:00:00');\n"
            )
        };
        let spawn = "/* one */\nDELETE FROM `landblock_instance` WHERE `guid` = 0x7A9B5000;\n\nINSERT INTO `landblock_instance` (`guid`) VALUES (0x7A9B5000);\n";
        let sql: BTreeMap<PathBuf, String> = [
            (PathBuf::from("p/weenies/guide.sql"), weenie("a")),
            (
                PathBuf::from("p/landblocks/A9B5 guide.sql"),
                spawn.to_owned(),
            ),
        ]
        .into_iter()
        .collect();
        // Everything, the first time.
        let first = plan(&BTreeMap::new(), &sql, &none());
        assert_eq!(first.writes.len(), 2);
        assert_eq!(
            first.imports,
            ["import-sql landblock A9B5", "import-sql weenie 90001"]
        );
        assert_eq!(first.landblocks, [0xA9B5].into_iter().collect());
        assert_eq!(first.classes, [90001].into_iter().collect());
        // Then only what changed: the weenie.
        let pushed = first.writes.clone();
        let mut sql2 = sql.clone();
        sql2.insert(PathBuf::from("p/weenies/guide.sql"), weenie("b"));
        let second = plan(&pushed, &sql2, &none());
        assert_eq!(second.imports, ["import-sql weenie 90001"]);
        assert!(second.landblocks.is_empty());
        assert_eq!(second.classes, [90001].into_iter().collect());
        assert!(
            plan(&pushed, &sql, &none()).writes.is_empty(),
            "nothing new"
        );
        // A spawn's file gone: its deletion alone is imported.
        let mut sql3 = sql.clone();
        sql3.remove(Path::new("p/landblocks/A9B5 guide.sql"));
        let third = plan(&pushed, &sql3, &none());
        let gone = Path::new("sql/landblocks/A9B5 p guide.sql");
        assert_eq!(
            third.writes[gone].0,
            "DELETE FROM `landblock_instance` WHERE `guid` = 0x7A9B5000;\n"
        );
        assert_eq!(third.imports, ["import-sql landblock A9B5"]);
        // Once.
        let mut after = pushed.clone();
        after.extend(third.writes.clone());
        assert!(plan(&after, &sql3, &none()).writes.is_empty());
    }

    #[test]
    fn a_file_gone_is_listed_with_what_it_stood_for_a_server_weenie_quest_or_table() {
        use lyceum_foundry::loot::LootKind;
        let weenie = "DELETE FROM `weenie` WHERE `class_Id` = 273;\n\nINSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (273, 'coinstack', 51, '2026-10-25 00:00:00');\n";
        let quest = "DELETE FROM `quest` WHERE `name` = 'drudgekills';\n\nINSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`, `message`, `last_Modified`)\nVALUES ('drudgekills', 0, -1, 'Done.', '2026-10-25 00:00:00');\n";
        let sql: BTreeMap<PathBuf, String> = [
            (
                PathBuf::from("p/weenies/00273 Pyreal.sql"),
                weenie.to_owned(),
            ),
            (PathBuf::from("p/quests/drudgekills.sql"), quest.to_owned()),
        ]
        .into_iter()
        .collect();
        let pushed = plan(&BTreeMap::new(), &sql, &none()).writes;
        assert!(gone(&pushed, &sql).is_empty(), "nothing gone");
        let mut left = sql.clone();
        left.remove(Path::new("p/weenies/00273 Pyreal.sql"));
        let g = gone(&pushed, &left);
        assert_eq!(g.len(), 1);
        let (path, text, import) = &g[0];
        assert_eq!(path, Path::new("sql/weenies/00273 Pyreal.sql"));
        assert_eq!(text, weenie);
        assert_eq!(stands_for(import, text), Some(StandsFor::Weenie(273)));
        let g = gone(&pushed, &BTreeMap::new());
        let (_, text, import) = &g[0];
        assert_eq!(import, "import-sql quest drudgekills");
        assert_eq!(
            stands_for(import, text),
            Some(StandsFor::Quest("drudgekills".into()))
        );
        assert_eq!(
            stands_for("import-treasure wielded 37", ""),
            Some(StandsFor::Loot(LootKind::Wielded, 37))
        );
        assert_eq!(stands_for("import-sql landblock A9B4", ""), None);
        // Given the server's own, the weenie's file gone writes it back, imported as it was.
        let own = "-- the server's 273";
        let at = Path::new("sql/weenies/00273 Pyreal.sql");
        let file = |p: &Path| (p == at).then(|| own.to_owned());
        let back = plan(
            &pushed,
            &left,
            &Own {
                file: &file,
                ..none()
            },
        );
        assert_eq!(back.writes[at].0, own);
        assert_eq!(back.imports, ["import-sql weenie 273"]);
        assert!(back.classes.contains(&273), "its landblocks built again");
    }

    #[test]
    fn a_loot_tables_copy_gone_gives_the_server_its_own_back_and_imports_by_kind_and_id() {
        let copy = "DELETE FROM `treasure_death` WHERE `treasure_Type` = 37;\n\nINSERT INTO `treasure_death` (`treasure_Type`, `tier`) VALUES (37, 6);\n";
        let sql: BTreeMap<PathBuf, String> =
            [(PathBuf::from("p/treasure_death/00037.sql"), copy.to_owned())]
                .into_iter()
                .collect();
        let first = plan(&BTreeMap::new(), &sql, &none());
        assert_eq!(first.imports, ["import-treasure death 37"]);
        let pushed = first.writes.clone();
        // Taken out of the pack: the server's own rows are imported in its place.
        let at = Path::new("sql/treasure_death/00037.sql");
        let server = "-- the server's 37";
        let file = |p: &Path| (p == at).then(|| server.to_owned());
        let gone = plan(
            &pushed,
            &BTreeMap::new(),
            &Own {
                file: &file,
                ..none()
            },
        );
        assert_eq!(gone.writes[at].0, server);
        assert_eq!(gone.imports, ["import-treasure death 37"]);
        // A table of the pack's own has no server's: its deletions alone.
        let own = plan(&pushed, &BTreeMap::new(), &none());
        assert_eq!(
            own.writes[at].0,
            "DELETE FROM `treasure_death` WHERE `treasure_Type` = 37;\n"
        );
    }

    #[test]
    fn a_rolls_answer_is_read_once_the_server_has_said_every_draw() {
        let lines: Vec<String> = [
            "2026-10-25 INFO [console] Rolling treasure 37: death treasure, tier 4, 3 time(s).",
            "2026-10-25 INFO [console] Treasure roll 1: 273 x200 Pyreal; 309 Club",
            "2026-10-25 INFO [console] Treasure roll 2: nothing",
            "2026-10-25 INFO [console] Treasure roll 3: 2366 Hide of the Deep",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        assert_eq!(rolled_from(&lines, 37), None, "not all said yet");
        let mut all = lines.clone();
        all.push("2026-10-25 INFO [console] Rolled treasure 37: 3 time(s).".into());
        assert_eq!(
            rolled_from(&all, 37),
            Some(Ok(vec![
                vec![(273, 200, "Pyreal".into()), (309, 1, "Club".into())],
                Vec::new(),
                vec![(2366, 1, "Hide of the Deep".into())],
            ]))
        );
        let none = ["Rolled treasure 9: there is no loot table 9.".to_owned()];
        assert_eq!(
            rolled_from(&none, 9),
            Some(Err("there is no loot table 9.".into()))
        );
        let old = ["Unknown command: roll-treasure".to_owned()];
        assert!(matches!(rolled_from(&old, 9), Some(Err(_))));
    }

    #[test]
    fn a_server_spawn_a_pack_no_longer_moves_or_takes_away_is_written_back_as_the_server_places_it()
    {
        let own = Own {
            spawn: &server_spawn,
            ..none()
        };
        let at = Path::new("sql/landblocks/0101 p overrides.sql");
        let sql = |text: Option<String>| -> BTreeMap<PathBuf, String> {
            text.map(|t| (PathBuf::from("p/landblocks/0101 overrides.sql"), t))
                .into_iter()
                .collect()
        };
        let push = |pushed: &mut Pushed, text: Option<String>| -> Option<String> {
            let p = plan(pushed, &sql(text), &own);
            pushed.extend(p.writes.clone());
            p.writes.get(at).map(|(t, _)| t.clone())
        };
        let mut pushed = Pushed::new();
        // The generator moved and the one alone taken away: written as the pack has it.
        let both = overrides(true, &[ALONE]);
        assert_eq!(push(&mut pushed, Some(both.clone())), Some(both));
        // The move put back, the file keeping the removal: the generator written again as the
        // server places it, its link to its instance with it; nothing of the one alone.
        let kept = overrides(false, &[ALONE]);
        let text = push(&mut pushed, Some(kept.clone())).unwrap();
        assert!(text.starts_with(&kept), "{text}");
        let back = &text[kept.len()..];
        for line in [
            "DELETE FROM `landblock_instance` WHERE `guid` = 0x70010000;",
            "VALUES (0x70010000, 1154, 0x01010019, 84.0067, 4.37315, 94.005, 0.92388, 0, 0, -0.382683, False, '2026-10-25 00:00:00');",
            "VALUES (0x70010000, 0x70010001, '2026-10-25 00:00:00');",
        ] {
            assert!(back.contains(line), "{line}\n{back}");
        }
        assert!(!back.contains("0x70010002"), "{back}");
        // Once: pushed again, nothing to write.
        assert!(plan(&pushed, &sql(Some(kept)), &own).writes.is_empty());
        // The file gone: the one alone written back, and the generator still the server's; no
        // deletion left alone.
        let gone = push(&mut pushed, None).unwrap();
        assert!(gone.contains("VALUES (0x70010000, 1154,"), "{gone}");
        assert!(
            gone.contains(", (0x70010002, 7924, 0x01010019, 88,"),
            "{gone}"
        );
        assert!(
            gone.trim_start().starts_with("/* The server's own spawns"),
            "{gone}"
        );
        assert!(plan(&pushed, &sql(None), &own).writes.is_empty(), "once");
        // A pack's file with nothing of the server's: its deletions alone, as before.
        let mut mine = Pushed::new();
        let own_spawn = "DELETE FROM `landblock_instance` WHERE `guid` = 0x70010009;\n\nINSERT INTO `landblock_instance` (`guid`) VALUES (0x70010009);\n";
        push(&mut mine, Some(own_spawn.into()));
        assert_eq!(
            push(&mut mine, None).as_deref(),
            Some("DELETE FROM `landblock_instance` WHERE `guid` = 0x70010009;\n")
        );
    }

    #[test]
    fn an_instance_put_back_is_linked_again_to_its_generator_unless_the_generator_is_not_in_the_world(
    ) {
        let own = Own {
            spawn: &server_spawn,
            ..none()
        };
        let file = |t: String| -> BTreeMap<PathBuf, String> {
            [(PathBuf::from("p/landblocks/0101 overrides.sql"), t)]
                .into_iter()
                .collect()
        };
        let at = Path::new("sql/landblocks/0101 p overrides.sql");
        // The instance taken away, then put back: its link from the generator, which kept it,
        // taken away and written again.
        let pushed = plan(&Pushed::new(), &file(overrides(false, &[CHILD])), &own).writes;
        let back = plan(&pushed, &BTreeMap::new(), &own).writes[at].0.clone();
        for line in [
            "DELETE FROM `landblock_instance_link` WHERE `parent_GUID` = 0x70010000 AND `child_GUID` = 0x70010001;",
            "VALUES (0x70010001, 7924, 0x01010019, 86, 4.37315, 94.005, 0.92388, 0, 0, -0.382683, True,",
            "VALUES (0x70010000, 0x70010001,",
        ] {
            assert!(back.contains(line), "{line}\n{back}");
        }
        // Put back while the file still takes the generator away: no link to it.
        let pushed = plan(
            &Pushed::new(),
            &file(overrides(false, &[GENERATOR, CHILD])),
            &own,
        )
        .writes;
        let back = plan(&pushed, &file(overrides(false, &[GENERATOR])), &own).writes[at]
            .0
            .clone();
        assert!(back.contains("VALUES (0x70010001, 7924,"), "{back}");
        assert!(!back.contains("landblock_instance_link"), "{back}");
    }

    #[test]
    fn a_refused_first_push_gives_back_the_servers_own_weenie_and_spawns_and_takes_away_the_packs_own(
    ) {
        let copy = "DELETE FROM `weenie` WHERE `class_Id` = 273;\n\nINSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (273, 'coinstack', 51, '2026-10-25 00:00:00');\n";
        let mine = "DELETE FROM `weenie` WHERE `class_Id` = 90001;\n\nINSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (90001, 'guide', 10, '2026-10-25 00:00:00');\n";
        let sql: BTreeMap<PathBuf, String> = [
            (PathBuf::from("p/weenies/00273 Pyreal.sql"), copy.to_owned()),
            (PathBuf::from("p/weenies/guide.sql"), mine.to_owned()),
            (
                PathBuf::from("p/landblocks/0101 overrides.sql"),
                overrides(true, &[]),
            ),
        ]
        .into_iter()
        .collect();
        let pyreal = Path::new("sql/weenies/00273 Pyreal.sql");
        let server = "-- the server's 273";
        let file = |p: &Path| (p == pyreal).then(|| server.to_owned());
        let own = Own {
            file: &file,
            spawn: &server_spawn,
            ..none()
        };
        let writes = plan(&Pushed::new(), &sql, &own).writes;
        assert_eq!(writes.len(), 3);
        let back = undo(&Pushed::new(), &writes, &own);
        assert_eq!(back[pyreal].0, server);
        assert_eq!(back[pyreal].1, "import-sql weenie 273");
        assert_eq!(
            back[Path::new("sql/weenies/90001 guide.sql")].0,
            "DELETE FROM `weenie` WHERE `class_Id` = 90001;\n"
        );
        let spawns = &back[Path::new("sql/landblocks/0101 p overrides.sql")].0;
        assert!(
            spawns.contains("VALUES (0x70010000, 1154, 0x01010019, 84.0067,"),
            "{spawns}"
        );
        // A file the folder held before: as it was.
        let before = plan(&Pushed::new(), &sql, &none()).writes;
        let mut later = sql.clone();
        later.insert(
            PathBuf::from("p/weenies/00273 Pyreal.sql"),
            copy.replace("coinstack", "hoard"),
        );
        let writes = plan(&before, &later, &own).writes;
        assert_eq!(undo(&before, &writes, &own)[pyreal].0, copy);
    }

    #[test]
    fn a_port_is_found_when_the_system_hands_them_out_in_turn_from_near_the_top() {
        let mut at = 65_000_u16;
        let next = || {
            let p = at;
            at = if at == u16::MAX { 49_152 } else { at + 1 };
            Ok(p)
        };
        assert_eq!(port_pair(next, |_| true), Ok(49_152));
        assert_eq!(port_pair(|| Ok(40_000), |p| p != 40_001).ok(), None);
    }
}
