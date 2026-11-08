//! A soak run: this server's executable on the September 2013 install and a world database, with
//! players logged in over loopback, each placed at one of the world's most populated places, for a
//! stretch of time. It samples the world's step times from the status endpoint and the server's
//! resident memory, and writes them out. Feature `soak`; never in the default gate. The run is
//! ignored as an instrument: `cargo test -p yalain --features soak --test live instruments::soak --
//! --ignored --nocapture`.
//!
//! Behaviour: none (an instrument: it measures the world's step and the server's memory, and asserts only that every player entered the world and the server ran throughout)
//!
//! Its settings come from the environment:
//! - `YALAIN_SOAK_PLAYERS`: the numbers of players, a run each, a fresh server each (default `0,10`);
//! - `YALAIN_SOAK_SECONDS`: how long each run is sampled once its players are in (default 120);
//! - `YALAIN_SOAK_OUT`: the folder the report is written to (default cargo's folder for tests'
//!   files);
//! - `YALAIN_TEST_WORLD_DB`: the world database, a checkout; else the release this build pins.
//!
//! The players are the test kit's clients ([`Bot`]) on UDP sockets of their own. Each is made by
//! the operator's recipe at the center of the most populated camp of a landblock, one landblock
//! each, the most populated first; the places come from the world's spawn table, compiled by the
//! executable's own `world compile`. Each logs in once the world has taken its first step, tells
//! the server its scene is drawn as it enters, so creatures notice it and fight it (it never
//! fights back), and reports a place a step from its own every second, as a client standing about
//! does.
//!
//! Each sample is the status endpoint's figures over the last minute's steps, so a run is sampled
//! once a minute, and at its end; memory is read from the process's status (Linux only).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use auberean_primitives::Vec3;
use auberean_protocol::login::{DataHeld, Empty, PlayCharacter};
use auberean_protocol::text::NarrowString;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_net::LocalTime;
use yalain_testkit::bot::Bot;
use yalain_testkit::play::report;
use yalain_world::session::function::SHOW_TEXT;
use yalain_world::world::IDLE_MOTION;

/// The executable under test.
const EXE: &str = env!("CARGO_BIN_EXE_yalain");

/// How long a server may take to listen, and the players to enter, before the run fails.
const START_LIMIT: Duration = Duration::from_secs(180);

/// How long after the last player enters before sampling begins.
const SETTLE: Duration = Duration::from_secs(15);

/// Seconds between two samples of the step times: a minute's steps each.
const SAMPLE_EVERY: f64 = 60.0;

/// Seconds between two players' logins.
const LOGIN_EVERY: f64 = 0.1;

/// A setting from the environment, or its default.
fn setting(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// A folder of its own, emptied.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

/// An even port with the three above it free on loopback as UDP, and a TCP port for the status
/// endpoint.
fn free_ports() -> (u16, u16) {
    let start = 40_000 + u16::try_from(std::process::id() % 5_000).unwrap_or(0) * 2;
    for p in (start..60_000).step_by(4) {
        let held: Vec<UdpSocket> = (0..4)
            .filter_map(|i| UdpSocket::bind((Ipv4Addr::LOCALHOST, p + i)).ok())
            .collect();
        if held.len() == 4 {
            let status = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .and_then(|l| l.local_addr())
                .expect("a TCP port")
                .port();
            return (p, status);
        }
    }
    panic!("no four free ports on loopback");
}

/// A place a player is put: a cell and a point in it.
#[derive(Debug, Clone, Copy)]
struct Place {
    cell: u32,
    at: [f32; 3],
    /// The creatures the landblock's camps keep.
    population: u32,
}

/// The landblocks the world's camps keep creatures in, the most populated first: in each, the
/// center of its most populated camp. Read from the spawn table the executable compiles.
fn places(install: &Path, world_db: &str, dir: &Path) -> (Vec<Place>, u32) {
    let out = dir.join("tables");
    let done = Command::new(EXE)
        .args(["world", "compile", "--dats"])
        .arg(install)
        .args(["--world-db", world_db, "--out"])
        .arg(&out)
        .stdin(Stdio::null())
        .output()
        .expect("the executable runs");
    assert!(
        done.status.success(),
        "world compile: {}",
        String::from_utf8_lossy(&done.stderr)
    );
    let text = std::fs::read_to_string(out.join("spawns.tsv")).expect("the spawn table");
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().expect("a header").split('\t').collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .unwrap_or_else(|| panic!("no {name} column"))
    };
    let (cell, x, y, z, population) =
        (col("cell"), col("x"), col("y"), col("z"), col("population"));
    // By landblock: its creatures in all, and its most populated camp's place.
    let mut blocks: BTreeMap<u32, (u32, Place)> = BTreeMap::new();
    let mut creatures = 0;
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        let c = u32::from_str_radix(f[cell], 16).expect("a cell in hex");
        let n: u32 = f[population].parse().expect("a population");
        let num = |i: usize| f[i].parse::<f32>().expect("a number");
        let here = Place {
            cell: c,
            at: [num(x), num(y), num(z)],
            population: n,
        };
        creatures += n;
        let b = blocks.entry(c >> 16).or_insert((0, here));
        b.0 += n;
        if n > b.1.population {
            b.1 = here;
        }
    }
    let mut by_block: Vec<(u32, Place)> = blocks.into_values().collect();
    by_block.sort_by_key(|b| std::cmp::Reverse(b.0));
    (
        by_block
            .into_iter()
            .map(|(n, p)| Place { population: n, ..p })
            .collect(),
        creatures,
    )
}

/// A player's name: "Soak" and two letters of its number.
fn name(i: usize) -> String {
    let letter = |n: usize| char::from(b'a' + u8::try_from(n % 26).unwrap_or(0));
    let first = letter(i / 26).to_ascii_uppercase();
    format!("Soak {first}{}", letter(i))
}

/// The operator's recipe for player `i`, made at `place`.
fn recipe(dir: &Path, i: usize, place: Place) -> PathBuf {
    let path = dir.join(format!("player-{i}.toml"));
    let [x, y, z] = place.at;
    std::fs::write(
        &path,
        format!(
            "account = \"soak{i}\"\nname = \"{}\"\nrace = 1\nsex = 4096\nlevel = 50\n\
             place = {{ cell = 0x{:08X}, x = {x}, y = {y}, z = {z}, heading = 0 }}\n",
            name(i),
            place.cell
        ),
    )
    .expect("the recipe is written");
    path
}

/// The server under test: its process, its console's input, and where it answers.
struct Server {
    child: Child,
    console: Option<ChildStdin>,
    login: SocketAddr,
    status: SocketAddr,
    log: PathBuf,
    /// Its memory as it began to listen, before its world's first step.
    at_listening: Option<(f64, f64)>,
}

impl Server {
    /// Start the executable with these players' recipes, and wait until it listens and its world has
    /// taken its first step.
    fn start(install: &Path, world_db: &str, dir: &Path, recipes: &[PathBuf]) -> Self {
        let (port, status) = free_ports();
        let log = dir.join("server.log");
        let file = std::fs::File::create(&log).expect("the log");
        let mut cmd = Command::new(EXE);
        cmd.arg("--dats")
            .arg(install)
            .args(["--world-db", world_db, "--memory", "--no-friends"])
            .args(["--secret-cost", "4"])
            .args(["--login-port", &port.to_string()])
            .args(["--world-port", &(port + 2).to_string()])
            .args(["--status", &format!("127.0.0.1:{status}")])
            .args(["--max-sessions", &(recipes.len() + 8).to_string()]);
        for r in recipes {
            cmd.arg("--make-character").arg(r);
        }
        let mut child = cmd
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(file.try_clone().expect("the log"))
            .stderr(file)
            .spawn()
            .expect("the server starts");
        let console = child.stdin.take();
        let started = Instant::now();
        loop {
            let text = std::fs::read_to_string(&log).unwrap_or_default();
            if text.contains(" [listening]") {
                break;
            }
            if let Some(exit) = child.try_wait().expect("the server's state") {
                panic!("the server stopped ({exit}) before it listened:\n{text}");
            }
            assert!(started.elapsed() < START_LIMIT, "the server never listened");
            std::thread::sleep(Duration::from_millis(100));
        }
        let mut server = Self {
            child,
            console,
            login: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            status: SocketAddr::from((Ipv4Addr::LOCALHOST, status)),
            log,
            at_listening: None,
        };
        server.at_listening = server.memory();
        // The players come once the world's first step has filled its camps.
        while server.step_times()["steps"].as_u64().unwrap_or(0) == 0 {
            assert!(
                started.elapsed() < START_LIMIT,
                "the world never took a step"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        server
    }

    /// The status endpoint's `world_step`, read.
    fn step_times(&self) -> serde_json::Value {
        let mut s = TcpStream::connect(self.status).expect("the status endpoint");
        s.set_read_timeout(Some(Duration::from_secs(60)))
            .expect("set");
        write!(s, "GET /status HTTP/1.1\r\nHost: x\r\n\r\n").expect("sent");
        let mut text = String::new();
        s.read_to_string(&mut text).expect("an answer");
        let body = text.split("\r\n\r\n").nth(1).expect("a body");
        let v: serde_json::Value = serde_json::from_str(body).expect("JSON");
        v["world_step"].clone()
    }

    /// The processor time the process has used, in seconds, where the system says (Linux's clock
    /// ticks, a hundred a second).
    fn cpu_seconds(&self) -> Option<f64> {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", self.child.id())).ok()?;
        // The fields after the command's name, which is in parentheses.
        let rest = stat.rsplit_once(')')?.1;
        let f: Vec<&str> = rest.split_whitespace().collect();
        let ticks = f.get(11)?.parse::<f64>().ok()? + f.get(12)?.parse::<f64>().ok()?;
        Some(ticks / 100.0)
    }

    /// The process's resident memory now and at its most, in megabytes, where the system says.
    fn memory(&self) -> Option<(f64, f64)> {
        let status = std::fs::read_to_string(format!("/proc/{}/status", self.child.id())).ok()?;
        let kb = |key: &str| {
            status
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .and_then(|v| v.split_whitespace().next())
                .and_then(|v| v.parse::<f64>().ok())
        };
        Some((kb("VmRSS:")? / 1024.0, kb("VmHWM:")? / 1024.0))
    }

    /// Whether the server is still running.
    fn running(&mut self) -> bool {
        self.child.try_wait().expect("the server's state").is_none()
    }

    /// Stop it at once, keeping no one; killed if it does not stop.
    fn stop(mut self) -> String {
        if let Some(mut c) = self.console.take() {
            let _ = c.write_all(b"stop-now\n");
        }
        let asked = Instant::now();
        while self.running() && asked.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(100));
        }
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }
}

impl Drop for Server {
    /// A run that fails leaves no server behind.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Where a player is in its login.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Waiting,
    LoggingIn,
    Listing,
    Entering,
    InWorld,
}

/// A player: its client, its socket, its account and where it stands.
struct Player {
    bot: Bot,
    socket: UdpSocket,
    account: String,
    stage: Stage,
    login_at: f64,
    character: u64,
    cell: u32,
    home: [f32; 3],
    teleport_stamp: u16,
    next_report: f64,
    reports: u32,
    /// Messages it has had on its world connection.
    world_messages: u64,
}

impl Player {
    fn new(i: usize, server: SocketAddr, place: Place) -> Self {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a socket");
        socket.set_nonblocking(true).expect("non-blocking");
        let addr = socket.local_addr().expect("its address");
        #[allow(clippy::cast_precision_loss)]
        let login_at = i as f64 * LOGIN_EVERY;
        Self {
            bot: Bot::new(addr, server),
            socket,
            account: format!("soak{i}"),
            stage: Stage::Waiting,
            login_at,
            character: 0,
            cell: place.cell,
            home: place.at,
            teleport_stamp: 0,
            next_report: 0.0,
            reports: 0,
            world_messages: 0,
        }
    }

    /// Take what arrived, answer what the login asks, and send what is due.
    fn turn(&mut self, now: LocalTime) {
        let mut buf = [0u8; 2048];
        while let Ok((n, from)) = self.socket.recv_from(&mut buf) {
            self.bot.on_datagram(from, &buf[..n], now);
        }
        let messages: Vec<(bool, ServerMessage)> = self
            .bot
            .inbox
            .drain(..)
            .filter_map(|r| r.message().map(|m| (r.world, m)))
            .collect();
        for (world, m) in messages {
            self.world_messages += u64::from(world);
            self.heard(world, &m, now);
        }
        match self.stage {
            Stage::Waiting if now.0 >= self.login_at => {
                self.bot.log_in(&self.account, b"soak", now);
                self.stage = Stage::LoggingIn;
            }
            Stage::InWorld if now.0 >= self.next_report => {
                self.next_report = now.0 + 1.0;
                self.reports += 1;
                // A step this way and back, as a client standing about reports.
                let side = if self.reports.is_multiple_of(2) {
                    0.5
                } else {
                    -0.5
                };
                let [x, y, z] = self.home;
                self.bot.send(
                    true,
                    queue::WEENIE,
                    &report(
                        self.cell,
                        Vec3::new(x + side, y, z),
                        0,
                        IDLE_MOTION,
                        self.teleport_stamp,
                    ),
                );
            }
            _ => {}
        }
        for (to, bytes) in self.bot.tick(now) {
            let _ = self.socket.send_to(&bytes, to);
        }
    }

    fn heard(&mut self, world: bool, m: &ServerMessage, now: LocalTime) {
        match (self.stage, m) {
            (Stage::LoggingIn, ServerMessage::DataQuery(_)) => {
                let held = ClientMessage::DataHeld(DataHeld {
                    language: 0x8000_0001,
                    files: Vec::new(),
                });
                self.bot.send(false, queue::DATABASE, &held);
                self.stage = Stage::Listing;
            }
            (Stage::Listing, ServerMessage::CharacterList(list)) => {
                let c = list.characters.first().expect("the recipe's character");
                self.character = c.id;
                let play = ClientMessage::PlayCharacter(PlayCharacter {
                    character: c.id,
                    account: NarrowString::from(self.account.as_str()),
                });
                self.bot.send(false, queue::LOGON, &play);
                self.stage = Stage::Entering;
            }
            (Stage::Entering, ServerMessage::PlayerCall(c))
                if world && c.function.0 == SHOW_TEXT =>
            {
                self.stage = Stage::InWorld;
                self.next_report = now.0 + 2.0;
                // Its scene is drawn: the player is free to move, and creatures see it.
                self.bot.send(
                    true,
                    queue::WEENIE,
                    &ClientMessage::SceneRenderingComplete(Empty),
                );
            }
            // Where the server last put the player: it is reported about from there.
            (_, ServerMessage::Position(p))
                if p.sender.id == self.character && p.teleport_stamp != self.teleport_stamp =>
            {
                self.teleport_stamp = p.teleport_stamp;
                self.cell = p.cell;
                self.home = [p.origin.x, p.origin.y, p.origin.z];
            }
            _ => {}
        }
    }
}

/// One sample of the step times: the status endpoint's figures over the last minute's steps.
#[derive(Debug, Clone)]
struct Sample {
    at: f64,
    figures: serde_json::Value,
    memory: Option<(f64, f64)>,
    /// The share of one processor the server used since the sample before.
    cpu: Option<f64>,
}

/// Milliseconds from a figure.
fn ms(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

/// The share of one processor the server used since `from` (its processor time and the clock
/// then), which becomes now.
fn busy(server: &Server, from: &mut Option<(f64, f64)>, now: f64) -> Option<f64> {
    let cpu = server.cpu_seconds()?;
    let share = from.map(|(c, t)| (cpu - c) / (now - t).max(1e-9));
    *from = Some((cpu, now));
    share
}

/// The machine's load averages.
fn load() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .map(|l| l.split_whitespace().take(3).collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|_| "unknown".into())
}

/// One run: `n` players at the world's most populated places, sampled for `seconds` once in.
#[allow(clippy::too_many_lines)]
fn run(install: &Path, world_db: &str, n: usize, seconds: f64, written: &mut String) {
    let dir = scratch(&format!("soak-{n}"));
    let (places, creatures) = places(install, world_db, &dir);
    let mine: Vec<Place> = (0..n).map(|i| places[i % places.len()]).collect();
    let recipes: Vec<PathBuf> = mine
        .iter()
        .enumerate()
        .map(|(i, p)| recipe(&dir, i, *p))
        .collect();
    let load_before = load();
    let mut server = Server::start(install, world_db, &dir, &recipes);
    let idle_memory = server.memory();
    let server_at_listening = server.at_listening;
    let started = Instant::now();
    let now = || LocalTime(started.elapsed().as_secs_f64());
    let mut players: Vec<Player> = mine
        .iter()
        .enumerate()
        .map(|(i, p)| Player::new(i, server.login, *p))
        .collect();
    // Every player enters.
    while players.iter().any(|p| p.stage != Stage::InWorld) {
        let t = now();
        for p in &mut players {
            p.turn(t);
        }
        if started.elapsed() > START_LIMIT {
            let stuck: Vec<String> = players
                .iter()
                .filter(|p| p.stage != Stage::InWorld)
                .map(|p| {
                    format!(
                        "{} {:?} (login {}, world {}, world messages {})",
                        p.account,
                        p.stage,
                        p.bot.logged_in(),
                        p.bot.in_world(),
                        p.world_messages
                    )
                })
                .collect();
            panic!(
                "{} of {n} players entered; not: {}",
                n - stuck.len(),
                stuck.join("; ")
            );
        }
        assert!(server.running(), "the server stopped while players entered");
        std::thread::sleep(Duration::from_millis(5));
    }
    let entered_after = started.elapsed().as_secs_f64();
    // They stand about a while, then a sample a minute.
    let settle_until = now().0 + SETTLE.as_secs_f64();
    let begin = settle_until;
    let end = begin + seconds;
    let mut first: Option<serde_json::Value> = None;
    // The server's processor time, and when it was read.
    let mut cpu_from: Option<(f64, f64)> = None;
    let mut samples: Vec<Sample> = Vec::new();
    let mut next_sample = begin + SAMPLE_EVERY;
    let mut peak_rss: f64 = 0.0;
    let mut next_memory = begin;
    while now().0 < end {
        let t = now();
        for p in &mut players {
            p.turn(t);
        }
        if first.is_none() && t.0 >= begin {
            first = Some(server.step_times());
            busy(&server, &mut cpu_from, t.0);
        }
        if t.0 >= next_memory {
            next_memory += 5.0;
            if let Some((rss, _)) = server.memory() {
                peak_rss = peak_rss.max(rss);
            }
        }
        if t.0 >= next_sample {
            next_sample += SAMPLE_EVERY;
            samples.push(Sample {
                at: t.0 - begin,
                figures: server.step_times(),
                memory: server.memory(),
                cpu: busy(&server, &mut cpu_from, t.0),
            });
        }
        assert!(server.running(), "the server stopped during the run");
        std::thread::sleep(Duration::from_millis(5));
    }
    let last = server.step_times();
    let memory = server.memory();
    let cpu_last = busy(&server, &mut cpu_from, now().0);
    // The last stretch, unless a sample has just covered it.
    if samples.last().is_none_or(|s| s.at < seconds - 1.0) {
        samples.push(Sample {
            at: seconds,
            figures: last.clone(),
            memory,
            cpu: cpu_last,
        });
    }
    let load_after = load();
    let in_world = players.iter().filter(|p| p.stage == Stage::InWorld).count();
    let log = server.stop();
    assert!(!log.contains("THE WORLD PANICKED"), "the world panicked");
    assert_eq!(in_world, n, "every player stayed in the world");

    // The report.
    let first = first.unwrap_or(serde_json::Value::Null);
    let steps = last["steps"].as_u64().unwrap_or(0) - first["steps"].as_u64().unwrap_or(0);
    let overruns = last["overruns"].as_u64().unwrap_or(0) - first["overruns"].as_u64().unwrap_or(0);
    let warned = log.matches("a world step's work took").count();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "## {n} players ({} places, {creatures} creatures kept by the camps)",
        places.len().min(n)
    );
    let _ = writeln!(
        out,
        "entered after {entered_after:.1} s; sampled {seconds:.0} s after {:.0} s more; load {load_before} -> {load_after}",
        SETTLE.as_secs_f64()
    );
    let _ = writeln!(
        out,
        "steps sampled {steps}, overruns {overruns}, warnings in the log {warned}, creatures acting at the last step {}, first step {} ms",
        last["creatures"], last["first_ms"]
    );
    let mb = |m: Option<(f64, f64)>| {
        m.map_or("unknown".into(), |(rss, hwm)| {
            format!("{rss:.0} MB (peak {hwm:.0} MB)")
        })
    };
    let _ = writeln!(
        out,
        "memory: listening {}, after the first step {}, end {}, highest sampled {peak_rss:.0} MB",
        mb(server_at_listening),
        mb(idle_memory),
        mb(memory)
    );
    let _ = writeln!(
        out,
        "| at s | steps | mean ms | p99 ms | worst ms | overruns | rss MB | cpu | {} |",
        STAGES.join(" | ")
    );
    for s in &samples {
        let r = &s.figures["recent"];
        let stages: Vec<String> = STAGES
            .iter()
            .map(|st| {
                format!(
                    "{:.2}/{:.1}",
                    ms(&r["stages"][st]["mean_ms"]),
                    ms(&r["stages"][st]["worst_ms"])
                )
            })
            .collect();
        let _ = writeln!(
            out,
            "| {:.0} | {} | {:.2} | {:.2} | {:.1} | {} | {} | {} | {} |",
            s.at,
            r["steps"],
            ms(&r["mean_ms"]),
            ms(&r["p99_ms"]),
            ms(&r["worst_ms"]),
            r["overruns"],
            s.memory.map_or("?".into(), |(rss, _)| format!("{rss:.0}")),
            s.cpu.map_or("?".into(), |c| format!("{:.0}%", c * 100.0)),
            stages.join(" | ")
        );
    }
    let _ = writeln!(out);
    println!("{out}");
    written.push_str(&out);
    let raw = serde_json::json!({
        "players": n,
        "seconds": seconds,
        "first": first,
        "last": last,
        "samples": samples.iter().map(|s| serde_json::json!({
            "at": s.at, "figures": s.figures, "memory_mb": s.memory.map(|m| m.0), "cpu": s.cpu,
        })).collect::<Vec<_>>(),
    });
    std::fs::write(dir.join("samples.json"), raw.to_string()).expect("the samples are written");
}

/// The stages in the report's order (mean/worst ms each).
const STAGES: [&str; 9] = [
    "network",
    "timers",
    "camps",
    "creatures",
    "step",
    "fights",
    "saves",
    "other",
    "transport",
];

/// The soak run: for each number of players asked, a fresh server with that many players spread
/// over the world's most populated places, its step times and memory sampled and written out.
#[test]
#[ignore = "instrument: a soak run of the whole world; takes minutes"]
fn the_world_with_players_spread_over_its_populated_places_is_sampled() {
    let install = auberean_dat::testing::require_target_install();
    let world_db = setting("YALAIN_TEST_WORLD_DB", "pinned");
    let counts: Vec<usize> = setting("YALAIN_SOAK_PLAYERS", "0,10")
        .split(',')
        .map(|n| n.trim().parse().expect("a number of players"))
        .collect();
    let seconds: f64 = setting("YALAIN_SOAK_SECONDS", "120")
        .parse()
        .expect("a number of seconds");
    let out = std::env::var_os("YALAIN_SOAK_OUT")
        .map_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")), PathBuf::from);
    std::fs::create_dir_all(&out).expect("the report's folder");
    let mut written = format!("# Soak run: world {world_db}\n\n");
    for n in counts {
        run(&install, &world_db, n, seconds, &mut written);
        std::fs::write(out.join("soak-report.md"), &written).expect("the report is written");
    }
}
