//! The server under test: this executable on the September 2013 install and a world database, on
//! loopback ports of its own, its status endpoint read for the world's step times and objects, its
//! memory and processor time read from the system.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

/// The executable under test.
pub const EXE: &str = env!("CARGO_BIN_EXE_yalain");

/// How long a server may take to listen, and the players to enter, before the run fails.
pub const START_LIMIT: Duration = Duration::from_secs(240);

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

/// A place players are put: a cell and a point in it.
#[derive(Debug, Clone, Copy)]
pub struct Place {
    pub cell: u32,
    pub at: [f32; 3],
    /// The creatures the landblock's camps keep.
    pub population: u32,
}

/// What the world's tables say: the landblocks its camps keep creatures in, the most populated first
/// (in each, the center of its most populated camp), the creatures in all, every template a camp
/// makes, and the objects that take a player somewhere (its portals), each with its place.
pub struct World {
    pub places: Vec<Place>,
    pub creatures: u32,
    pub templates: BTreeSet<u32>,
    pub portals: Vec<(u64, u32, [f32; 3])>,
}

/// Read the world's spawn and content tables, as the executable's own `world compile` writes them.
pub fn world(install: &Path, world_db: &str, dir: &Path) -> World {
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
    let (cell, x, y, z, population, kinds) = (
        col("cell"),
        col("x"),
        col("y"),
        col("z"),
        col("population"),
        col("templates"),
    );
    let mut blocks: BTreeMap<u32, (u32, Place)> = BTreeMap::new();
    let mut creatures = 0;
    let mut templates = BTreeSet::new();
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        let c = u32::from_str_radix(f[cell], 16).expect("a cell in hex");
        let n: u32 = f[population].parse().expect("a population");
        let num = |i: usize| f[i].parse::<f32>().expect("a number");
        for t in f[kinds].split(',').filter(|t| !t.is_empty()) {
            let id = t.split('*').next().unwrap_or_default();
            if let Ok(id) = u32::from_str_radix(id, 16) {
                templates.insert(id);
            }
        }
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
    // The world's objects with a destination.
    let content = std::fs::read_to_string(out.join("content.tsv")).expect("the content table");
    let mut lines = content.lines();
    let header: Vec<&str> = lines.next().expect("a header").split('\t').collect();
    let col = |name: &str| header.iter().position(|h| *h == name).expect("a column");
    let (object, cell, x, y, z, to) = (
        col("object"),
        col("cell"),
        col("x"),
        col("y"),
        col("z"),
        col("destination"),
    );
    let portals = lines
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            if f.get(to).is_none_or(|d| d.is_empty()) {
                return None;
            }
            let num = |i: usize| f[i].parse::<f32>().ok();
            Some((
                u64::from_str_radix(f[object], 16).ok()?,
                u32::from_str_radix(f[cell], 16).ok()?,
                [num(x)?, num(y)?, num(z)?],
            ))
        })
        .collect();
    World {
        places: by_block
            .into_iter()
            .map(|(n, p)| Place { population: n, ..p })
            .collect(),
        creatures,
        templates,
        portals,
    }
}

/// The weapon every player is made with and wields: the starting sword.
pub const SWORD: u32 = 0x4700_051C;

/// The operator's recipe for player `i`, made at `place` with a sword in its pack.
pub fn recipe(dir: &Path, i: usize, name: &str, place: Place) -> PathBuf {
    let path = dir.join(format!("player-{i}.toml"));
    let [x, y, z] = place.at;
    std::fs::write(
        &path,
        format!(
            "account = \"soak{i}\"\nname = \"{name}\"\nrace = 1\nsex = 4096\nlevel = 50\n\
             items = [{{ template = 0x{SWORD:08X} }}]\n\
             place = {{ cell = 0x{:08X}, x = {x}, y = {y}, z = {z}, heading = 0 }}\n",
            place.cell
        ),
    )
    .expect("the recipe is written");
    path
}

/// The server under test: its process, its console's input, and where it answers.
pub struct Server {
    child: Child,
    console: Option<ChildStdin>,
    pub login: SocketAddr,
    status: SocketAddr,
    log: PathBuf,
}

impl Server {
    /// Start the executable with these players' recipes, and wait until it listens and its world has
    /// taken its first step.
    pub fn start(install: &Path, world_db: &str, dir: &Path, recipes: &[PathBuf]) -> Self {
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
        let server = Self {
            child,
            console,
            login: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            status: SocketAddr::from((Ipv4Addr::LOCALHOST, status)),
            log,
        };
        while server.status()["world_step"]["steps"].as_u64().unwrap_or(0) == 0 {
            assert!(
                started.elapsed() < START_LIMIT,
                "the world never took a step"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        server
    }

    /// The status endpoint's answer, read.
    pub fn status(&self) -> serde_json::Value {
        let mut s = TcpStream::connect(self.status).expect("the status endpoint");
        s.set_read_timeout(Some(Duration::from_secs(60)))
            .expect("set");
        write!(s, "GET /status HTTP/1.1\r\nHost: x\r\n\r\n").expect("sent");
        let mut text = String::new();
        s.read_to_string(&mut text).expect("an answer");
        let body = text.split("\r\n\r\n").nth(1).expect("a body");
        serde_json::from_str(body).expect("JSON")
    }

    /// The processor time the process has used and its resident memory, in seconds and megabytes,
    /// where the system says: Linux's process status, else `ps`.
    pub fn usage(&self) -> Option<(f64, f64)> {
        let pid = self.child.id();
        if let (Ok(stat), Ok(status)) = (
            std::fs::read_to_string(format!("/proc/{pid}/stat")),
            std::fs::read_to_string(format!("/proc/{pid}/status")),
        ) {
            let rest = stat.rsplit_once(')')?.1;
            let f: Vec<&str> = rest.split_whitespace().collect();
            let ticks = f.get(11)?.parse::<f64>().ok()? + f.get(12)?.parse::<f64>().ok()?;
            let rss = status
                .lines()
                .find_map(|l| l.strip_prefix("VmRSS:"))
                .and_then(|v| v.split_whitespace().next())
                .and_then(|v| v.parse::<f64>().ok())?;
            return Some((ticks / 100.0, rss / 1024.0));
        }
        let out = Command::new("ps")
            .args(["-o", "time=,rss=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut f = text.split_whitespace();
        let time = f.next()?;
        let rss: f64 = f.next()?.parse().ok()?;
        // [[days-]hours:]minutes:seconds
        let mut seconds = 0.0;
        for part in time.replace('-', ":").split(':') {
            seconds = seconds * 60.0 + part.parse::<f64>().ok()?;
        }
        Some((seconds, rss / 1024.0))
    }

    /// Whether the server is still running.
    pub fn running(&mut self) -> bool {
        self.child.try_wait().expect("the server's state").is_none()
    }

    /// Stop it at once, keeping no one; killed if it does not stop. Returns its log.
    pub fn stop(mut self) -> String {
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
