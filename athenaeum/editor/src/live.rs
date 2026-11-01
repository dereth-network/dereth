//! Playing the world: the editor's own Yalain, started on this machine's loopback address in the
//! world's playtest folder and serving the world as the editor holds it, and a client started into
//! it as one of the world's playtest characters ([`crate::characters`]).
//!
//! **The playtest folder** is `.playtest` in the world's folder, which neither the world's reader
//! nor the server reads as part of the world: `world/`, the world's files as the editor holds them,
//! saved or not, which the server serves over the world's base; `characters/`, each playtest
//! character's own store; the server's log (and the one before it); and the client's log, settings
//! and screenshots. One
//! editor at a time plays a world: a second is refused while the first holds the folder's lock.
//!
//! **Live.** Each change made while the server runs is published: the changed files are written
//! into `world/` and the server is told `reload` at its console. It compiles the world again and
//! takes at once what can change with players in the world (templates, loot, stores, quests, what
//! people say, the places maps name, and more); the placed people and objects and the camps
//! change only when it starts again, which [`Live::restart`] does: the server is stopped as its
//! operator stops it, keeping every character where it stands, and started again, and a client
//! the editor started enters again where its character stood.
//!
//! **Stopping** is the server's own clean stop ([`crate::stop`]), after which it keeps every
//! character still in the world and ends; one that has not ended within [`STOP_WAIT`] is ended.
//!
//! The server listens on 127.0.0.1 alone, on ports no other program holds, and our client reaches
//! it from a loopback address: nothing is open to another computer, and no firewall asks. A client
//! that is Auberean logs in by itself as the character; the retail client is told the server,
//! account and ticket as the launcher tells it.

use std::fs::File;
use std::io::Write as _;
use std::net::{Ipv4Addr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use yalain_world_db::Files;

use crate::characters::{self, Recipe};
use crate::stop;

/// The account every playtest character is played from (each has a store of its own) and its
/// ticket.
pub const ACCOUNT: &str = "builder";
pub const TICKET: &str = "athenaeum";

/// The playtest folder, in the world's folder.
pub const FOLDER: &str = ".playtest";

/// How long a server asked to stop is given before it is ended: a guard against a hang, far past
/// what keeping a playtest's characters takes.
pub const STOP_WAIT: Duration = Duration::from_secs(30);

/// What starting the server and a client needs.
#[derive(Debug, Clone, Default)]
pub struct Programs {
    /// The server.
    pub yalain: Option<PathBuf>,
    /// The install the server reads (a copy no client runs from).
    pub server_dats: Option<PathBuf>,
    /// Our client, and the install it reads.
    pub client: Option<PathBuf>,
    pub client_dats: Option<PathBuf>,
    /// The retail client, run from its own folder.
    pub retail: Option<PathBuf>,
    /// More arguments for whichever client starts.
    pub client_args: Vec<String>,
    /// The first login port tried; the world's is two above.
    pub port: u16,
}

/// A program beside this one, when there is one.
#[must_use]
pub fn beside_me(name: &str) -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    let p = me.with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    p.is_file().then_some(p)
}

/// Where the player enters: a cell, a point in its landblock, a heading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    pub cell: u32,
    pub at: [f32; 3],
    pub heading: f32,
}

/// What one start of the server is told.
#[derive(Debug, Clone)]
pub struct Start<'a> {
    /// The world it serves, and the base it lies over.
    pub world: &'a Path,
    pub base: Option<&'a Path>,
    /// The character's store.
    pub store: &'a Path,
    pub port: u16,
    pub recipe: &'a Recipe,
    /// Whether the character is to be made.
    pub make: bool,
    /// Where it enters; `None` where it last left the world.
    pub entry: Option<Entry>,
}

/// The server's arguments for a start.
#[must_use]
pub fn server_args(dats: &Path, s: &Start<'_>) -> Vec<String> {
    let name = &s.recipe.name;
    let mut a = vec![
        "--dats".to_string(),
        dats.display().to_string(),
        "--world-db".into(),
        s.world.display().to_string(),
    ];
    if let Some(b) = s.base {
        a.extend(["--world-base".into(), b.display().to_string()]);
    }
    a.extend([
        "--data".into(),
        s.store.display().to_string(),
        "--bind".into(),
        "127.0.0.1".into(),
        "--login-port".into(),
        s.port.to_string(),
        "--world-port".into(),
        (s.port + 2).to_string(),
        "--world-name".into(),
        "Athenaeum".into(),
    ]);
    if s.make {
        a.extend([
            "--add-character".into(),
            format!("{ACCOUNT}:{name}:{}:{}", s.recipe.race, s.recipe.sex),
        ]);
    }
    if let Some(e) = s.entry {
        a.extend([
            "--place".into(),
            format!(
                "{ACCOUNT}:{name}:{:08X}:{}:{}:{}:{}",
                e.cell, e.at[0], e.at[1], e.at[2], e.heading
            ),
        ]);
    }
    a
}

/// Whether the server's ports from `login` on are free on loopback: the login's and the world's
/// (each with the port above it) for datagrams, and the login's for the friends service's stream.
fn ports_free(login: u16) -> bool {
    let udp: Vec<_> = (0..4)
        .map_while(|i| UdpSocket::bind((Ipv4Addr::LOCALHOST, login + i)).ok())
        .collect();
    udp.len() == 4 && TcpListener::bind((Ipv4Addr::LOCALHOST, login)).is_ok()
}

/// The first even login port from `from` on whose ports no program holds.
///
/// # Errors
/// None free among the next thousand.
pub fn free_port(from: u16) -> Result<u16, String> {
    let first = from.max(1024) & !1;
    (0..500)
        .map(|i| first.saturating_add(i * 2))
        .take_while(|p| *p < u16::MAX - 4)
        .find(|p| ports_free(*p))
        .ok_or_else(|| format!("no free ports on 127.0.0.1 from {first} on"))
}

/// Write the world's `files` into `dir` where they differ from what was `written` there before,
/// and take away those it no longer has. Answers how many it wrote or took away.
///
/// # Errors
/// A file that will not write.
pub fn write_world(dir: &Path, files: &Files, written: &mut Files) -> Result<usize, String> {
    let mut n = 0;
    for (rel, text) in files {
        if written.get(rel) == Some(text) {
            continue;
        }
        let p = dir.join(rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
        written.insert(rel.clone(), text.clone());
        n += 1;
    }
    let gone: Vec<String> = written
        .keys()
        .filter(|k| !files.contains_key(*k))
        .cloned()
        .collect();
    for rel in gone {
        let p = dir.join(&rel);
        if p.exists() {
            std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        written.remove(&rel);
        n += 1;
    }
    Ok(n)
}

/// What the server answered a `reload`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reloaded {
    /// It compiled: the tables taken at once, and those changed that wait for a restart.
    Taken {
        applied: Vec<String>,
        restart: Vec<String>,
    },
    /// It did not compile, and the world is as it was: why.
    Refused(String),
}

/// The answer to a `reload` in the server's log since it was asked, once there is one: its first
/// whole line that answers.
#[must_use]
pub fn reload_answer(log: &str) -> Option<Reloaded> {
    const REFUSED: &str = "did not compile; the world is unchanged: ";
    const TAKEN: &str = ": reloaded: ";
    let list = |s: &str| -> Vec<String> {
        let s = s.trim();
        if s == "nothing" {
            Vec::new()
        } else {
            s.split(", ").map(str::to_string).collect()
        }
    };
    for line in log.split_inclusive('\n').filter(|l| l.ends_with('\n')) {
        let line = line.trim_end();
        if let Some(i) = line.find(REFUSED) {
            return Some(Reloaded::Refused(
                line[i + REFUSED.len()..].trim().to_string(),
            ));
        }
        if let Some(i) = line.find(TAKEN) {
            let rest = &line[i + TAKEN.len()..];
            let (applied, rest) = rest.split_once("; needs a restart: ")?;
            let (restart, _) = rest.split_once("; unchanged: ")?;
            return Some(Reloaded::Taken {
                applied: list(applied),
                restart: list(restart),
            });
        }
    }
    None
}

/// The playtest folder's lock, held while the server runs: a second editor on the same world is
/// refused.
///
/// # Errors
/// The folder will not make, or another editor holds it.
pub fn lock(folder: &Path) -> Result<File, String> {
    std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(folder.join("playtest.lock"))
        .map_err(|e| e.to_string())?;
    f.try_lock().map_err(|_| {
        format!(
            "another editor is playing this world (its playtest folder {} is in use)",
            folder.display()
        )
    })?;
    Ok(f)
}

/// Where a playtest of a world is kept, and what it lies over.
#[derive(Debug, Clone)]
pub struct Setup {
    pub programs: Programs,
    /// The playtest folder.
    pub folder: PathBuf,
    /// The world's base, when it lies over one.
    pub base: Option<PathBuf>,
}

impl Setup {
    /// The world the server serves.
    #[must_use]
    pub fn world(&self) -> PathBuf {
        self.folder.join("world")
    }

    /// The server's log.
    #[must_use]
    pub fn log(&self) -> PathBuf {
        self.folder.join("yalain.log")
    }

    /// The log of the server that ran before it (a restart's first half).
    #[must_use]
    pub fn previous_log(&self) -> PathBuf {
        self.folder.join("yalain-previous.log")
    }
}

/// The server running.
#[derive(Debug)]
struct Server {
    child: Child,
    console: ChildStdin,
    port: u16,
    recipe: Recipe,
    /// Whether the character is to be written down as made once the server has made it.
    making: bool,
    /// Whether it has said it listens.
    up: bool,
    /// The log's length when a `reload` was asked, while it is unanswered.
    reload_from: Option<usize>,
}

/// What happened since the last [`Live::tick`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The server listens.
    Up,
    /// A client started.
    Playing,
    /// The server answered a `reload`.
    Reloaded(Reloaded),
    /// The server stopped by itself: the log's last line.
    Stopped(String),
    /// Something the recipe asks that the server cannot do, in words.
    Note(String),
}

/// The editor's own server and the client it started.
#[derive(Debug, Default)]
pub struct Live {
    lock: Option<File>,
    server: Option<Server>,
    client: Option<Child>,
    /// The client's kind when one runs: ours or the retail one.
    retail: bool,
    /// A client to start once the server listens: the retail one or ours.
    pending: Option<bool>,
    /// The world as last written into the playtest folder.
    written: Files,
    /// What the last start said, for the status line.
    pub said: String,
}

fn kill(c: &mut Option<Child>) {
    if let Some(mut c) = c.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// Ask the server to stop as its operator would, and wait for it (ending it past the guard).
/// Answers whether it ended by itself.
fn stop_server(s: &mut Server) -> bool {
    let asked = stop::ask(s.child.id());
    let began = Instant::now();
    while asked && began.elapsed() < STOP_WAIT {
        if let Ok(Some(_)) = s.child.try_wait() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = s.child.kill();
    let _ = s.child.wait();
    false
}

fn read_log(setup: &Setup) -> String {
    log_from(setup, 0)
}

/// The server's log from byte `from` on (a line it was writing may end in the middle).
fn log_from(setup: &Setup, from: usize) -> String {
    let bytes = std::fs::read(setup.log()).unwrap_or_default();
    String::from_utf8_lossy(bytes.get(from..).unwrap_or_default()).into_owned()
}

/// How long the server's log is.
fn log_len(setup: &Setup) -> usize {
    std::fs::metadata(setup.log()).map_or(0, |m| usize::try_from(m.len()).unwrap_or(usize::MAX))
}

impl Live {
    /// Whether the server runs.
    pub fn serving(&mut self) -> bool {
        match &mut self.server {
            Some(s) => matches!(s.child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Whether the server has said it listens.
    #[must_use]
    pub fn up(&self) -> bool {
        self.server.as_ref().is_some_and(|s| s.up)
    }

    /// The character the server serves, while it runs.
    #[must_use]
    pub fn character(&self) -> Option<&str> {
        self.server.as_ref().map(|s| s.recipe.name.as_str())
    }

    /// The login port, while the server runs.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        self.server.as_ref().map(|s| s.port)
    }

    /// Whether a client the editor started runs.
    pub fn playing(&mut self) -> bool {
        match &mut self.client {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Whether a `reload` waits for its answer.
    #[must_use]
    pub fn reloading(&self) -> bool {
        self.server
            .as_ref()
            .is_some_and(|s| s.reload_from.is_some())
    }

    /// Start the server on the world's `files` for the character of `recipe`, entering at `entry`
    /// (`None`: where it last left the world), stopping one already running first. The
    /// character is made when it has not been, and made again when its recipe's body changed.
    ///
    /// # Errors
    /// No server program or install, the folder in use or not writable, no free ports, or the
    /// server does not start.
    pub fn start(
        &mut self,
        setup: &Setup,
        files: &Files,
        recipe: &Recipe,
        entry: Option<Entry>,
    ) -> Result<(), String> {
        let yalain = setup
            .programs
            .yalain
            .clone()
            .ok_or("no server program: give --yalain <yalain.exe>")?;
        let dats = setup
            .programs
            .server_dats
            .clone()
            .ok_or("no install for the server: give --server-dats <install>")?;
        let port = self.server.as_ref().map_or(setup.programs.port, |s| s.port);
        self.stop_server();
        if self.lock.is_none() {
            self.lock = Some(lock(&setup.folder)?);
        }
        let world = setup.world();
        let _ = std::fs::remove_dir_all(&world);
        self.written.clear();
        write_world(&world, files, &mut self.written)?;
        let (make, wipe) = characters::to_make(&setup.folder, recipe);
        if wipe {
            characters::reset(&setup.folder, &recipe.name)?;
        }
        let store = characters::store(&setup.folder, &recipe.name);
        if let Some(d) = store.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        let port = free_port(port)?;
        let args = server_args(
            &dats,
            &Start {
                world: &world,
                base: setup.base.as_deref(),
                store: &store,
                port,
                recipe,
                make,
                entry,
            },
        );
        if setup.log().exists() {
            let _ = std::fs::rename(setup.log(), setup.previous_log());
        }
        let log =
            File::create(setup.log()).map_err(|e| format!("{}: {e}", setup.log().display()))?;
        let err = log.try_clone().map_err(|e| e.to_string())?;
        let mut cmd = Command::new(&yalain);
        cmd.args(&args)
            .current_dir(&setup.folder)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err));
        stop::askable(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("{}: {e}", yalain.display()))?;
        let console = child.stdin.take().ok_or("the server's console")?;
        self.said = format!(
            "Yalain on 127.0.0.1:{port} as {} (log {})",
            recipe.name,
            setup.log().display()
        );
        self.server = Some(Server {
            child,
            console,
            port,
            recipe: recipe.clone(),
            making: make,
            up: false,
            reload_from: None,
        });
        Ok(())
    }

    /// Type a line at the server's console.
    ///
    /// # Errors
    /// No server, or its console closed.
    pub fn command(&mut self, line: &str) -> Result<(), String> {
        let s = self.server.as_mut().ok_or("no server runs")?;
        writeln!(s.console, "{line}")
            .and_then(|()| s.console.flush())
            .map_err(|e| format!("the server's console: {e}"))
    }

    /// Start a client once the server listens (now, when it does).
    pub fn play_when_ready(&mut self, retail: bool) {
        self.pending = Some(retail);
    }

    /// Each frame: whether the server has come up (the character it was to make made, a hero
    /// made or unmade as its recipe says), a client waiting for it started, a `reload` answered,
    /// or the server stopped by itself.
    ///
    /// # Errors
    /// The character was not made, or the client does not start.
    pub fn tick(&mut self, setup: &Setup) -> Result<Vec<Event>, String> {
        let mut events = Vec::new();
        let Some(s) = &mut self.server else {
            return Ok(events);
        };
        if !matches!(s.child.try_wait(), Ok(None)) {
            let tail = read_log(setup)
                .lines()
                .rev()
                .find(|x| !x.trim().is_empty())
                .unwrap_or_default()
                .to_string();
            self.server = None;
            self.pending = None;
            self.lock = None;
            events.push(Event::Stopped(tail));
            return Ok(events);
        }
        if !s.up {
            let log = read_log(setup);
            if !log.contains("listening on") {
                return Ok(events);
            }
            s.up = true;
            let recipe = s.recipe.clone();
            let name = recipe.name.clone();
            if std::mem::take(&mut s.making) {
                let made = format!("character {name} made on {ACCOUNT}");
                if !log.contains(&made) {
                    let why = log
                        .lines()
                        .find(|l| l.contains(&format!("character {name}: ")))
                        .unwrap_or("the log says nothing of it")
                        .to_string();
                    return Err(format!("Yalain did not make {name}: {why}"));
                }
                let mut was = recipe.clone();
                was.hero = false;
                characters::mark_made(&setup.folder, &was)?;
            }
            events.push(Event::Up);
            if characters::made(&setup.folder, &name).is_some_and(|m| m.hero != recipe.hero) {
                if name.contains(' ') {
                    // The console's `hero` takes the name as one word.
                    events.push(Event::Note(format!(
                        "Yalain makes heroes of one-word names only: {name} is not made one"
                    )));
                } else {
                    self.command(&if recipe.hero {
                        format!("hero {name}")
                    } else {
                        format!("hero {name} off")
                    })?;
                    characters::mark_made(&setup.folder, &recipe)?;
                }
            }
        }
        if let Some(s) = &mut self.server {
            if let Some(from) = s.reload_from {
                if let Some(answer) = reload_answer(&log_from(setup, from)) {
                    s.reload_from = None;
                    events.push(Event::Reloaded(answer));
                }
            }
        }
        if self.up() {
            if let Some(retail) = self.pending.take() {
                self.play(setup, retail)?;
                events.push(Event::Playing);
            }
        }
        Ok(events)
    }

    /// Publish the world's `files`: write what changed into the playtest folder and ask the server
    /// to `reload`. Answers whether it was published (`false`: the server is not up yet, or a
    /// reload still runs; publish again later). Nothing changed asks nothing.
    ///
    /// # Errors
    /// A file that will not write, or the console closed.
    pub fn publish(&mut self, setup: &Setup, files: &Files) -> Result<bool, String> {
        if !self.up() || self.reloading() {
            return Ok(false);
        }
        if write_world(&setup.world(), files, &mut self.written)? == 0 {
            return Ok(true);
        }
        let from = log_len(setup);
        self.command("reload")?;
        if let Some(s) = &mut self.server {
            s.reload_from = Some(from);
        }
        Ok(true)
    }

    /// Start a client into the server as its character: ours, or the retail one with `retail`.
    ///
    /// # Errors
    /// No server, no such client, or it does not start.
    pub fn play(&mut self, setup: &Setup, retail: bool) -> Result<(), String> {
        kill(&mut self.client);
        let (port, name) = match &self.server {
            Some(s) => (s.port.to_string(), s.recipe.name.clone()),
            None => return Err("no server runs".into()),
        };
        let programs = &setup.programs;
        let run = &setup.folder;
        let mut cmd = if retail {
            let exe = programs
                .retail
                .as_deref()
                .ok_or("no retail client: give --retail <client.exe>")?;
            let mut c = Command::new(exe);
            c.args([
                "--host",
                "127.0.0.1",
                "--port",
                &port,
                "--account",
                ACCOUNT,
                "--glsticketdirect",
                TICKET,
                "--user",
                &name,
            ])
            .args(&programs.client_args)
            .current_dir(exe.parent().unwrap_or(Path::new(".")));
            c
        } else {
            let exe = programs
                .client
                .as_deref()
                .ok_or("no client: give --client <auberean.exe>")?;
            let dats = programs
                .client_dats
                .as_deref()
                .or(programs.server_dats.as_deref())
                .ok_or("no install for the client: give --client-dats <install>")?;
            let settings = run.join("client-settings");
            std::fs::create_dir_all(&settings).map_err(|e| e.to_string())?;
            let log = File::create(run.join("client.log")).map_err(|e| e.to_string())?;
            let err = log.try_clone().map_err(|e| e.to_string())?;
            let mut c = Command::new(exe);
            c.args([
                "--dats".to_string(),
                dats.display().to_string(),
                "--server".into(),
                format!("127.0.0.1:{port}"),
                "--dev-login".into(),
                format!("{ACCOUNT}:{TICKET}"),
                "--dev-character".into(),
                name.clone(),
                "--shots".into(),
                run.join("shots").display().to_string(),
            ])
            .args(&programs.client_args)
            .env("AUBEREAN_SETTINGS_DIR", &settings)
            .current_dir(run)
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err));
            c
        };
        stop::windowless(&mut cmd);
        let child = cmd.spawn().map_err(|e| format!("the client: {e}"))?;
        self.retail = retail;
        self.client = Some(child);
        self.said = format!(
            "Yalain on 127.0.0.1:{port}; {} client playing as {name}",
            if retail { "the retail" } else { "our" }
        );
        Ok(())
    }

    /// Start the server again on the world's `files`, the character entering where it stands,
    /// and a client the editor started with it: what the placed objects and camps need to change.
    ///
    /// # Errors
    /// As [`Live::start`].
    pub fn restart(&mut self, setup: &Setup, files: &Files) -> Result<(), String> {
        let Some(recipe) = self.server.as_ref().map(|s| s.recipe.clone()) else {
            return Err("no server runs".into());
        };
        let replay = self.playing() || self.pending.is_some();
        kill(&mut self.client);
        self.start(setup, files, &recipe, None)?;
        if replay {
            self.pending = Some(self.retail);
        }
        Ok(())
    }

    /// Stop the client the editor started.
    pub fn stop_client(&mut self) {
        self.pending = None;
        kill(&mut self.client);
    }

    /// Stop the server cleanly, keeping its characters. Answers whether it stopped by itself.
    pub fn stop_server(&mut self) -> bool {
        let Some(mut s) = self.server.take() else {
            return true;
        };
        stop_server(&mut s)
    }

    /// Stop everything the editor started, and let the playtest folder go.
    pub fn stop_all(&mut self) {
        self.stop_client();
        self.stop_server();
        self.lock = None;
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.stop_all();
    }
}
