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
use lyceum_foundry::edit::Session;

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
    for _ in 0..64 {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let p = s.local_addr().map_err(|e| e.to_string())?.port();
        drop(s);
        if p < 65_000 && std::net::UdpSocket::bind(("127.0.0.1", p + 1)).is_ok() {
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

/// The push that brings a content folder holding `pushed` up to `sql` (the compiled world's
/// filled SQL, by path under `sql`).
#[must_use]
pub fn plan(pushed: &Pushed, sql: &BTreeMap<PathBuf, String>) -> Push {
    let mut out = Push::default();
    let mut now: Pushed = BTreeMap::new();
    for (rel, text) in sql {
        if let Some((path, import)) = content_place(rel, text) {
            now.insert(path, (text.clone(), import));
        }
    }
    let mut imports: BTreeSet<String> = BTreeSet::new();
    let mut note = |import: &str, out: &mut Push| {
        imports.insert(import.to_owned());
        if let Some(c) = import
            .strip_prefix("import-sql weenie ")
            .and_then(|c| c.parse().ok())
        {
            out.classes.insert(c);
        }
        if let Some(lb) = import
            .strip_prefix("import-sql landblock ")
            .and_then(|l| u16::from_str_radix(l, 16).ok())
        {
            out.landblocks.insert(lb);
        }
    };
    for (path, (text, import)) in &now {
        if pushed.get(path).map(|p| &p.0) != Some(text) {
            out.writes
                .insert(path.clone(), (text.clone(), import.clone()));
            note(import, &mut out);
        }
    }
    // A file gone: its deletions alone, imported, take back what it put in (once).
    for (path, (old, import)) in pushed {
        let gone = deletions_of(old);
        if now.contains_key(path) || *old == gone {
            continue;
        }
        out.writes.insert(path.clone(), (gone, import.clone()));
        note(import, &mut out);
    }
    out.imports = imports.into_iter().collect();
    out
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
        let start = std::time::Instant::now();
        loop {
            if let Some(l) = self.log_from(from).into_iter().find(|l| l.contains(needle)) {
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

    /// Bring the server up to `session`'s world: its SQL written into the content folder and
    /// imported, its data files written as a new overlay folder when their records changed, and
    /// every landblock the change reaches built again: those whose cell records or spawns changed,
    /// those a changed weenie stands in, and, when a weenie or a model or texture changed, those
    /// `near` (where the creator plays, and where a generator may make the weenie). Answers the
    /// console lines it said.
    ///
    /// # Errors
    /// A file or the overlay would not write, or the console is closed.
    pub fn push(
        &mut self,
        session: &Session,
        near: &BTreeSet<u16>,
    ) -> Result<Option<Revision>, String> {
        if self.pending.is_some() {
            return Err("A live revision is still being adopted.".into());
        }
        let compiled = session.compiled();
        let mut p = plan(&self.pushed, &compiled.sql);
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
        let result = self.publish_pending();
        if let Err(error) = result {
            let recovery = self.reject_pending();
            if recovery.is_err() {
                self.stop();
            }
            return Err(match recovery {
                Ok(()) => error,
                Err(e) => format!("{error}; recovery failed: {e}"),
            });
        }
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
            let answer = self
                .wait_for("Imported ", mark)
                .ok_or_else(|| format!("No acknowledgement for {command}"))?;
            if self
                .log_from(mark)
                .iter()
                .any(|l| l.contains(" ERROR ") || l.contains("Failed to "))
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

    /// Restore the last published content when either consumer refuses a revision.
    pub fn reject_pending(&mut self) -> Result<(), String> {
        let Some(p) = self.pending.take() else {
            return Ok(());
        };
        let restore: Pushed = p
            .plan
            .writes
            .iter()
            .map(|(path, (text, command))| {
                (
                    path.clone(),
                    self.pushed
                        .get(path)
                        .cloned()
                        .unwrap_or_else(|| (deletions_of(text), command.clone())),
                )
            })
            .collect();
        self.import_files(&restore)?;
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
        let first = plan(&BTreeMap::new(), &sql);
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
        let second = plan(&pushed, &sql2);
        assert_eq!(second.imports, ["import-sql weenie 90001"]);
        assert!(second.landblocks.is_empty());
        assert_eq!(second.classes, [90001].into_iter().collect());
        assert!(plan(&pushed, &sql).writes.is_empty(), "nothing new");
        // A spawn's file gone: its deletion alone is imported.
        let mut sql3 = sql.clone();
        sql3.remove(Path::new("p/landblocks/A9B5 guide.sql"));
        let third = plan(&pushed, &sql3);
        let gone = Path::new("sql/landblocks/A9B5 p guide.sql");
        assert_eq!(
            third.writes[gone].0,
            "DELETE FROM `landblock_instance` WHERE `guid` = 0x7A9B5000;\n"
        );
        assert_eq!(third.imports, ["import-sql landblock A9B5"]);
        // Once.
        let mut after = pushed.clone();
        after.extend(third.writes.clone());
        assert!(plan(&after, &sql3).writes.is_empty());
    }
}
