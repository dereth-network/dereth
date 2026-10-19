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
    /// The player's data files.
    pub dats: PathBuf,
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

/// Write the packs' content for GDLE: their filled SQL `sql` (by path under the build's `sql`
/// folder) written under `scratch`, applied over the server's SQL `dump` by the server's import
/// tool `import`, which writes the weenies, spawns and quests it reaches under `out` (made
/// afresh). Answers what the tool said it wrote.
///
/// # Errors
/// A file that would not write, or the tool's refusal.
pub fn export_gdle(
    import: &Path,
    dump: &Path,
    sql: &BTreeMap<PathBuf, String>,
    scratch: &Path,
    out: &Path,
) -> Result<String, String> {
    if sql.is_empty() {
        return Err("the packs have no server content to write".into());
    }
    let _ = std::fs::remove_dir_all(scratch);
    for (rel, text) in sql {
        let to = scratch.join(rel);
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        std::fs::write(&to, text).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    let _ = std::fs::remove_dir_all(out);
    let mut cmd = Command::new(import);
    cmd.arg("gdle")
        .arg("--sql")
        .arg(dump)
        .arg("--patches")
        .arg(scratch)
        .arg("--out")
        .arg(out)
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let ran = cmd
        .output()
        .map_err(|e| format!("{}: {e}", import.display()));
    let _ = std::fs::remove_dir_all(scratch);
    let ran = ran?;
    let said = |b: &[u8]| String::from_utf8_lossy(b).trim().to_owned();
    if ran.status.success() {
        Ok(said(&ran.stdout))
    } else {
        Err(said(&ran.stderr))
    }
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
    last_push: (usize, Vec<String>),
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
    dats: &Path,
    (pack, sql): (&Path, &Path),
    dir: &Path,
    port: u16,
    overlay: &Path,
) -> String {
    let p = |p: &Path| p.display().to_string().replace('\\', "/");
    format!(
        "# The world editor's own server: the loopback only, written by the editor.\n\
         [server]\n\
         world_name = \"Lyceum\"\n\
         dat_files_directory = \"{dats}\"\n\
         world_pack_path = \"{pack}\"\n\
         world_overlay_path = \"{dir}/content.overlay\"\n\
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
         udp_ping = false\n",
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

/// Where a pack's filled SQL file (`rel`, its path under the build's `sql` folder: the pack's
/// name, then the file's path in the pack's `server/sql`; `text`, its text) goes in the server's
/// content folder, and the import that takes it: a weenie by its class id (`<class> <name>.sql`),
/// a landblock's by its landblock (`<XXYY> <pack> <name>.sql`, so two packs' files of one
/// landblock do not meet), a quest by its name. `None` for a file the server's import does not
/// take.
#[must_use]
pub fn content_place(rel: &Path, text: &str) -> Option<(PathBuf, String)> {
    let mut parts = rel.components();
    let pack = parts.next()?.as_os_str().to_str()?;
    let kind = parts.next()?.as_os_str().to_str()?;
    let name = rel.file_name()?.to_str()?;
    let stem = rel.file_stem()?.to_str()?;
    match kind {
        "weenies" => {
            let class = lyceum_foundry::sql_ids::weenie_facts(text)
                .keys()
                .find_map(|w| lyceum_foundry::sql_ids::number(w))?;
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

impl Live {
    /// Start the server for `session`'s world as `setup` says: its folder and the data overlay
    /// written here, the rest off the frame loop. Once it is up, a [`Self::push`] gives it the
    /// packs' SQL.
    ///
    /// # Errors
    /// The folder or the overlay would not write.
    pub fn launch(setup: &LiveSetup, session: &Session) -> Result<Starting, String> {
        let dir = setup.dir.clone();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("content"))
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        let overlay = dir.join("overlay-0");
        let compiled = session.compiled();
        lyceum_foundry::compile::write_overlay(
            compiled,
            session.base(),
            &session.world().key,
            &overlay,
        )
        .map_err(|e| e.to_string())?;
        let (records, tombstones) = (compiled.records.clone(), compiled.tombstones.clone());
        let setup = setup.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // A start nobody waits for any more ends its server as it is dropped.
            let _ = tx.send(Self::start(&setup, overlay, records, tombstones));
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
    ) -> Result<Self, String> {
        let dir = setup.dir.clone();
        let pack = dir.join("world.pack");
        let mut import = Command::new(&setup.import);
        import
            .arg("--sql")
            .arg(&setup.sql)
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
        let port = free_port()?;
        let config = dir.join("empyrean.toml");
        std::fs::write(
            &config,
            config_text(&setup.dats, (&pack, &setup.sql), &dir, port, &overlay),
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
            stdin,
            log,
            dir: dir.clone(),
            port,
            overlay,
            turns: 0,
            pushed: BTreeMap::new(),
            records,
            tombstones,
            keep: setup.keep,
            made_character: false,
            last_push: (0, Vec::new()),
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
            console_path(&dir.join("content"))
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

    /// Wait for the server to have answered the last push (it takes its console's lines in
    /// order): the overlay read again when it was told to, else one import for each. False when
    /// it did not answer.
    pub fn settle(&mut self) -> bool {
        let (from, said) = self.last_push.clone();
        if said.iter().any(|l| l.starts_with("reload-dat-overlay")) {
            return self.wait_for("The data overlay", from).is_some();
        }
        let imports = said.iter().filter(|l| l.starts_with("import-sql")).count();
        let start = std::time::Instant::now();
        while self
            .log_from(from)
            .iter()
            .filter(|l| l.contains("Imported ") || l.contains(" ERROR "))
            .count()
            < imports
        {
            if start.elapsed() > PATIENCE || !self.running() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        true
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
    pub fn push(&mut self, session: &Session, near: &BTreeSet<u16>) -> Result<Vec<String>, String> {
        let mark = self.log_len();
        let compiled = session.compiled();
        let mut p = plan(&self.pushed, &compiled.sql);
        if !p.classes.is_empty() {
            p.landblocks.extend(session.landblocks_standing(&p.classes));
            p.landblocks.extend(near);
        }
        let (dat_blocks, other) = dat_changes(&self.records, &compiled.records);
        let dats_changed =
            !dat_blocks.is_empty() || other || self.tombstones != compiled.tombstones;
        if other {
            p.landblocks.extend(near);
        }
        for (path, (text, _)) in &p.writes {
            let to = self.dir.join("content").join(path);
            if let Some(d) = to.parent() {
                std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
            }
            std::fs::write(&to, text).map_err(|e| format!("{}: {e}", to.display()))?;
        }
        for (path, w) in &p.writes {
            self.pushed.insert(path.clone(), w.clone());
        }
        let mut said = Vec::new();
        for i in &p.imports {
            self.command(i)?;
            said.push(i.clone());
        }
        if dats_changed {
            // A new folder each time: a client playing may hold the last one open. The one
            // before that is no longer read by the server.
            self.turns += 1;
            let next = self.dir.join(format!("overlay-{}", self.turns));
            lyceum_foundry::compile::write_overlay(
                compiled,
                session.base(),
                &session.world().key,
                &next,
            )
            .map_err(|e| e.to_string())?;
            if let Some(old) = self.turns.checked_sub(2) {
                let _ = std::fs::remove_dir_all(self.dir.join(format!("overlay-{old}")));
            }
            self.overlay = next;
            self.records = compiled.records.clone();
            self.tombstones = compiled.tombstones.clone();
            p.landblocks.extend(dat_blocks);
        }
        if !p.landblocks.is_empty() {
            let blocks: Vec<String> = p.landblocks.iter().map(|b| format!("{b:04X}")).collect();
            let line = format!(
                "reload-dat-overlay {} {}",
                console_path(&self.overlay),
                blocks.join(" ")
            );
            self.command(&line)?;
            said.push(line);
        }
        if !said.is_empty() {
            self.last_push = (mark, said.clone());
        }
        Ok(said)
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
    fn the_servers_configuration_is_the_loopback_its_folder_and_the_overlay_never_patched() {
        let t = config_text(
            Path::new("C:\\dats"),
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
