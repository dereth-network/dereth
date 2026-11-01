//! The programs of a run: where each is, the ports they take, starting them with their logs kept,
//! and stopping them however the run ends.

use std::fs::File;
use std::io::{BufRead as _, BufReader, Read, Write as _};
use std::net::{Ipv4Addr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// A guard against a hang, not the time a step is expected to take.
pub const HANG: Duration = Duration::from_secs(900);

/// How long a program a run starts lives before it stops by itself, in seconds: a stop for one a
/// run that was killed left behind, far past what a run takes on a loaded machine.
pub const LIFE: u32 = 1_800;

/// A program: the variable that names it, else the file of that name beside this test binary's
/// folder of dependencies, where cargo puts every program of the workspace it builds.
pub fn program(var: &str, name: &str) -> PathBuf {
    if let Some(p) = std::env::var_os(var).filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    let exe = std::env::current_exe().expect("the test binary");
    let folder = exe
        .parent()
        .and_then(Path::parent)
        .expect("the test binary's profile folder");
    let path = folder.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "{name} is not built at {} (set {var}, or run `cargo xtask gate auberean-network`, which \
         builds it first)",
        path.display()
    );
    path
}

/// A folder of the run's own under cargo's temporary folder for tests, emptied first. Removed when
/// the run ends well; kept, and named, when it fails, for its logs.
pub struct Scratch {
    pub dir: PathBuf,
}

impl Scratch {
    pub fn new(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        Self { dir }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("the run's folder is kept: {}", self.dir.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// A port the system hands out on loopback whose `extra` ports above it are free too, as UDP, and
/// the port itself as TCP when `tcp`; even when `even`. Each is let go before it is returned, as a
/// program takes its ports itself, so another program can take one first: [`settle`] starts a
/// program that lost one again on fresh ports.
fn system_ports(extra: u16, even: bool, tcp: bool) -> u16 {
    // Never a port handed out before in this run: each was let go when it was.
    static GIVEN: Mutex<Vec<(u16, u16)>> = Mutex::new(Vec::new());
    let mut given = GIVEN.lock().unwrap_or_else(PoisonError::into_inner);
    for _ in 0..1_000 {
        let Ok(first) = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)) else {
            continue;
        };
        let Ok(p) = first.local_addr().map(|a| a.port()) else {
            continue;
        };
        if (even && p % 2 != 0)
            || p.checked_add(extra).is_none()
            || given.iter().any(|&(a, b)| p <= b && a <= p + extra)
        {
            continue;
        }
        let held: Vec<UdpSocket> = (1..=extra)
            .filter_map(|i| UdpSocket::bind((Ipv4Addr::LOCALHOST, p + i)).ok())
            .collect();
        let stream = !tcp || TcpListener::bind((Ipv4Addr::LOCALHOST, p)).is_ok();
        if held.len() == usize::from(extra) && stream {
            given.push((p, p + extra));
            return p;
        }
    }
    panic!("no free ports on loopback");
}

/// A world of the first game's ports: `P` and `P + 1`.
pub fn first_game_ports() -> u16 {
    system_ports(1, false, false)
}

/// A world of this game's ports: the login's `P` and `P + 1`, the world's `P + 2` and `P + 3`,
/// and the friends stream on TCP at `P`.
pub fn second_game_ports() -> u16 {
    system_ports(3, true, true)
}

/// A program started by the run, its output kept line by line in memory and in a file; stopped
/// however the run ends.
pub struct Process {
    pub name: String,
    child: Mutex<Child>,
    console: Option<ChildStdin>,
    lines: Arc<Mutex<Vec<String>>>,
    /// The threads reading its output, each to the end of its stream.
    readers: Mutex<Vec<JoinHandle<()>>>,
    /// Stopped by the run, so its end is no fault.
    stopped: bool,
}

impl Process {
    /// Starts `cmd`, its output (both streams) kept in `log`; with its console on a pipe when
    /// `console`.
    pub fn start(name: &str, mut cmd: Command, log: &Path, console: bool) -> Self {
        let mut child = cmd
            .stdin(if console {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("{name} starts: {e}"));
        let lines = Arc::new(Mutex::new(Vec::new()));
        let file = Arc::new(Mutex::new(File::create(log).expect("a log file")));
        let out = child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>);
        let err = child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>);
        let mut readers = Vec::new();
        for stream in [out, err].into_iter().flatten() {
            let (lines, file) = (Arc::clone(&lines), Arc::clone(&file));
            readers.push(std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    if let Ok(mut f) = file.lock() {
                        let _ = writeln!(f, "{line}");
                    }
                    if let Ok(mut l) = lines.lock() {
                        l.push(line);
                    }
                }
            }));
        }
        let console = child.stdin.take();
        Self {
            name: name.to_owned(),
            child: Mutex::new(child),
            console,
            lines,
            readers: Mutex::new(readers),
            stopped: false,
        }
    }

    /// Whether the program has logged a line holding `needle`.
    pub fn saw(&self, needle: &str) -> bool {
        self.lines
            .lock()
            .is_ok_and(|l| l.iter().any(|x| x.contains(needle)))
    }

    /// How many logged lines hold `needle`.
    pub fn count(&self, needle: &str) -> usize {
        self.lines
            .lock()
            .map(|l| l.iter().filter(|x| x.contains(needle)).count())
            .unwrap_or_default()
    }

    /// The first logged line holding `needle`.
    pub fn line(&self, needle: &str) -> Option<String> {
        self.lines
            .lock()
            .ok()
            .and_then(|l| l.iter().find(|x| x.contains(needle)).cloned())
    }

    /// The last `n` lines it logged.
    pub fn tail(&self, n: usize) -> Vec<String> {
        self.lines
            .lock()
            .map(|l| l[l.len().saturating_sub(n)..].to_vec())
            .unwrap_or_default()
    }

    /// A line typed at its console.
    pub fn type_line(&mut self, line: &str) {
        let console = self.console.as_mut().expect("a console");
        writeln!(console, "{line}").expect("the console");
        console.flush().expect("the console");
    }

    /// Stops it at once, as a machine losing it would.
    pub fn kill(&mut self) {
        self.stopped = true;
        let child = self.child.get_mut().unwrap_or_else(PoisonError::into_inner);
        let _ = child.kill();
        let _ = child.wait();
    }

    /// Stops it at once without the run knowing, as a crash would.
    pub fn crash(&self) {
        let _ = self
            .child
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .kill();
    }

    /// How it ended, once it has and the run did not stop it, with all it logged read.
    pub fn ended(&self) -> Option<ExitStatus> {
        if self.stopped {
            return None;
        }
        let status = self
            .child
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .try_wait()
            .ok()
            .flatten()?;
        // Its output ends with it; the limit guards only against a stream something else holds.
        let until = Instant::now() + Duration::from_secs(10);
        let mut readers = self.readers.lock().unwrap_or_else(PoisonError::into_inner);
        while readers.iter().any(|r| !r.is_finished()) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        for r in std::mem::take(&mut *readers) {
            if r.is_finished() {
                let _ = r.join();
            }
        }
        Some(status)
    }

    /// Fails once it has ended and the run did not stop it: its name, how it ended and the last of
    /// what it logged.
    pub fn alive(&self) -> Result<(), String> {
        let Some(status) = self.ended() else {
            return Ok(());
        };
        let mut s = format!("{} ended ({status}); it logged, last:", self.name);
        for l in self.tail(40) {
            s.push_str("\n  ");
            s.push_str(&l);
        }
        Err(s)
    }

    /// Whether what it logged says a port it was to take was taken.
    fn lost_a_port(&self) -> bool {
        self.lines.lock().is_ok_and(|l| {
            l.iter().any(|x| {
                let x = x.to_lowercase();
                IN_USE.iter().any(|m| x.contains(m))
            })
        })
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.kill();
    }
}

/// What the systems say when a port is taken: Linux and macOS, and Windows (a port in use, and one
/// the system holds back).
const IN_USE: [&str; 3] = ["address already in use", "os error 10048", "os error 10013"];

/// How many times a program is started on fresh ports before the run gives up.
const STARTS: u32 = 5;

/// A program that has its ports: the program, its first port and how many starts it took.
pub struct Settled {
    pub process: Process,
    pub port: u16,
    pub starts: u32,
}

/// Waits until `process`, started on `port`, logs `ready`, which it says once it has its ports.
/// When it ends first because one of them was taken (another program took it after the run chose
/// it), `again` chooses fresh ports and starts it on them, and the wait begins anew. Fails when it
/// ends for anything else, or when a program of `watched` ends.
pub fn settle(
    process: Process,
    port: u16,
    ready: &str,
    watched: &[&Process],
    mut again: impl FnMut() -> (Process, u16),
) -> Settled {
    let (mut process, mut port, mut starts) = (process, port, 1);
    loop {
        let what = format!("{} has its ports", process.name);
        wait_for(
            &what,
            watched,
            || process.saw(ready) || process.ended().is_some(),
            || process.tail(40).join("\n"),
        );
        let Err(e) = process.alive() else {
            return Settled {
                process,
                port,
                starts,
            };
        };
        assert!(
            process.lost_a_port() && starts < STARTS,
            "{what}: it never did\n{e}"
        );
        eprintln!(
            "{} lost a port of {port} to another program; started again on fresh ports",
            process.name
        );
        (process, port) = again();
        starts += 1;
    }
}

/// Waits until `done` holds, looking again every few milliseconds, within the limit against a
/// hang; fails at once when a program of `watched` ends. `context` says what to show when it
/// fails.
pub fn wait_for(
    what: &str,
    watched: &[&Process],
    done: impl FnMut() -> bool,
    context: impl Fn() -> String,
) {
    wait_within(HANG, what, watched, done, context);
}

/// [`wait_for`], with `limit` against a hang.
pub fn wait_within(
    limit: Duration,
    what: &str,
    watched: &[&Process],
    mut done: impl FnMut() -> bool,
    context: impl Fn() -> String,
) {
    let until = Instant::now() + limit;
    while !done() {
        for p in watched {
            if let Err(e) = p.alive() {
                panic!("{what}: {e}\n{}", context());
            }
        }
        assert!(
            Instant::now() < until,
            "{what}: it never came\n{}",
            context()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The hub's registry, in the run's folder: a command of the hub's program on it.
fn hub_command(hub: &Path, dir: &Path, args: &[&str]) -> String {
    let out = Command::new(hub)
        .arg("--db")
        .arg(dir.join("hub.sqlite"))
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("{}: {e}", hub.display()));
    assert!(
        out.status.success(),
        "the hub's {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A world of `game` registered with the hub by its operator, on the network; its key.
pub fn register(hub: &Path, dir: &Path, slug: &str, game: &str, family: &str, port: u16) -> String {
    let file = registration(dir, slug, game, family, port);
    let printed = hub_command(hub, dir, &["register", &file.to_string_lossy()]);
    hub_command(hub, dir, &["federation", slug, "on"]);
    let at = printed.find("drth_").expect("the world's key printed");
    printed[at..]
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .next()
        .expect("a key")
        .to_owned()
}

/// A registered world's registration replaced by its operator, its port now `port`; its key kept.
pub fn moved(hub: &Path, dir: &Path, slug: &str, game: &str, family: &str, port: u16) {
    let file = registration(dir, slug, game, family, port);
    hub_command(hub, dir, &["update", &file.to_string_lossy()]);
}

/// A world's registration, written in the run's folder.
fn registration(dir: &Path, slug: &str, game: &str, family: &str, port: u16) -> PathBuf {
    let file = dir.join(format!("{slug}.json"));
    let registration = format!(
        r#"{{"slug": "{slug}", "name": "World {upper}", "description": "A loopback test world.",
            "ruleset": "PvE", "game": "{game}",
            "endpoint": {{"mode": "manual", "address": "127.0.0.1", "port": {port}}},
            "emulator": {{"family": "{family}"}}}}"#,
        upper = slug.to_uppercase()
    );
    std::fs::write(&file, registration).expect("a registration");
    file
}

/// The hub serving on a port of the system's, recording every link frame's type in `wire.log`.
pub struct Hub {
    pub process: Process,
    pub port: u16,
    wire: PathBuf,
}

/// The line the hub logs once it listens, its port after it.
pub const HUB_LISTENING: &str = "listening on http://127.0.0.1:";

/// The hub's program serving on `listen`, its registry in `dir`, every link frame's type recorded
/// in `wire.log` there.
pub fn serve(hub: &Path, dir: &Path, listen: &str) -> Command {
    let mut cmd = Command::new(hub);
    cmd.arg("--db")
        .arg(dir.join("hub.sqlite"))
        .args(["serve", "--listen", listen, "--run-for"])
        .arg(LIFE.to_string())
        .arg("--wire-log")
        .arg(dir.join("wire.log"));
    cmd
}

impl Hub {
    pub fn start(hub: &Path, dir: &Path) -> Self {
        let process = Process::start(
            "hub",
            serve(hub, dir, "127.0.0.1:0"),
            &dir.join("hub.log"),
            false,
        );
        wait_for(
            "the hub listening",
            &[&process],
            || process.saw(HUB_LISTENING),
            || process.tail(20).join("\n"),
        );
        let line = process.line(HUB_LISTENING).expect("the line");
        let port = line
            .split(HUB_LISTENING)
            .nth(1)
            .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|p| p.parse().ok())
            .expect("the hub's port");
        Self {
            process,
            port,
            wire: dir.join("wire.log"),
        }
    }

    /// Every link frame the hub carried, one line each: who to whom, and its type.
    pub fn wire(&self) -> Vec<String> {
        std::fs::read_to_string(&self.wire)
            .unwrap_or_default()
            .lines()
            .map(|l| l.split_once(' ').map_or(l, |(_, rest)| rest).to_owned())
            .collect()
    }

    /// How many frames `way` (`"bb -> hub: watch"`, say) the hub carried.
    pub fn carried(&self, way: &str) -> usize {
        self.wire().iter().filter(|l| *l == way).count()
    }
}

/// The first game's world database: `EMPYREAN_TEST_WORLD_PACK`, else built into the run's folder
/// by the importer from the world release in its cache (fetched there first if it is not).
pub fn first_game_world(dir: &Path) -> PathBuf {
    if let Some(p) = std::env::var_os("EMPYREAN_TEST_WORLD_PACK").filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    let pack = dir.join("world.pack");
    let import = program("EMPYREAN_IMPORT_BIN", "empyrean-import");
    let out = Command::new(&import)
        .args(["fetch", "--pack", "--out"])
        .arg(&pack)
        .output()
        .unwrap_or_else(|e| panic!("{}: {e}", import.display()));
    assert!(
        out.status.success() && pack.is_file(),
        "the first game's world database was not built: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    pack
}

/// What a world of the first game logs once it has its ports: the step after it binds them.
pub const FIRST_GAME_READY: &str = "Initializing WorldManager";

/// What a world of this game logs once it has its ports: its friends stream's, taken last.
pub const SECOND_GAME_READY: &str = "friends service listening on TCP";

/// A world of the first game: Empyrean on `port` and `port + 1`, on the network as `slug` through
/// the hub on `hub`, with its files in `dir/<slug>`.
pub fn start_first_game_world(
    dir: &Path,
    slug: &str,
    port: u16,
    hub: u16,
    key: &str,
    world: &Path,
) -> Process {
    let home = dir.join(slug);
    std::fs::create_dir_all(&home).expect("a world's folder");
    std::fs::write(home.join("network.key"), format!("{key}\n")).expect("the key file");
    let quoted = |p: &Path| format!("{:?}", p.to_string_lossy());
    let config = format!(
        "[server]\n\
         world_name = \"World {upper}\"\n\
         dat_files_directory = {dats}\n\
         world_pack_path = {world}\n\
         landblock_preloading = false\n\
         interactive_console = false\n\
         [server.network]\n\
         host = \"127.0.0.1\"\n\
         port = {port}\n\
         [database]\n\
         shard_db_path = \"shard.db\"\n\
         auth_db_path = \"auth.db\"\n\
         [federation]\n\
         enabled = true\n\
         hub_url = \"ws://127.0.0.1:{hub}\"\n\
         slug = \"{slug}\"\n\
         key_file = \"network.key\"\n",
        upper = slug.to_uppercase(),
        dats = quoted(&dereth_dat::testing::dat_dir()),
        world = quoted(world),
    );
    std::fs::write(home.join("empyrean.toml"), config).expect("empyrean.toml");
    let mut cmd = Command::new(program("EMPYREAN_SERVER_BIN", "empyrean-server"));
    cmd.args(["--config", "empyrean.toml", "--run-for"])
        .arg(LIFE.to_string())
        .current_dir(&home);
    Process::start(slug, cmd, &home.join("server.log"), false)
}

/// A world of this game: this server's executable on `port` (login) and `port + 2` (world), named
/// `World <SLUG>`, serving the authored world, on the network as `slug` through the hub on `hub`,
/// with its console on a pipe and its files in `dir/<slug>`.
pub fn start_second_game_world(dir: &Path, slug: &str, port: u16, hub: u16, key: &str) -> Process {
    let home = dir.join(slug);
    std::fs::create_dir_all(&home).expect("a world's folder");
    let key_file = home.join("hub.key");
    std::fs::write(&key_file, format!("{key}\n")).expect("the key file");
    let mut cmd = Command::new(program("YALAIN_BIN", "yalain"));
    cmd.arg("--dats")
        .arg(auberean_dat::testing::require_target_install())
        .args(["--memory", "--login-port", &port.to_string()])
        .args(["--world-port", &(port + 2).to_string()])
        .arg("--world-db")
        .arg(yalain_testkit::authored::database_dir())
        .args(["--world-name", &format!("World {}", slug.to_uppercase())])
        .args(["--hub", &format!("http://127.0.0.1:{hub}")])
        .args(["--hub-slug", slug, "--hub-key-file"])
        .arg(&key_file)
        .arg("--federation")
        .args(["--seconds", &LIFE.to_string()])
        .current_dir(&home);
    Process::start(slug, cmd, &home.join("server.log"), true)
}
