//! The UDP-mode smoke: one real `empyrean-server` process on loopback, with soak bots
//! over real UDP sockets, in real time. It checks the transport under load on a loss-free link.
//!
//! The server is started with a generated `empyrean.toml` (host `127.0.0.1`, a free high port, the
//! default session limits, the test inputs' dat directory and `world.pack`, the console off), fresh
//! shard and auth files in a scratch directory, and `--run-for`, so
//! it shuts itself down (`ServerManager.DoShutdownNow`, as on Ctrl-C) once the bots are done; if it
//! has not exited a minute after that, the smoke kills it. It never binds anything but loopback.
//!
//! The bots are the soak's ([`super::bot`]), with no fights (no admin elevation over the wire) and
//! no relogs (their invariant needs the world); accounts are auto-created (`AllowAutoAccountCreation`).
//!
//! Nothing here is an ACE port; there are no ACE anchors.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dereth_primitives::IncomingMessage;

use super::bot::{Bot, BotIo, State};
use super::metrics;
use crate::{ClientStatus, TestClient};

/// The smoke's shape.
#[derive(Debug, Clone)]
pub struct UdpConfig {
    pub bots: usize,
    /// Real seconds of play once every bot is in the world (or the login window has passed).
    pub play_secs: f64,
    /// The `empyrean-server` binary.
    pub server_bin: PathBuf,
    /// A scratch directory for the configuration, the databases and the server's log.
    pub dir: PathBuf,
    pub seed: u64,
}

/// What the smoke saw.
#[derive(Debug, Clone, Default)]
pub struct UdpReport {
    pub port: u16,
    pub bots: usize,
    pub connected: usize,
    pub entered: usize,
    pub logged_off: usize,
    pub wall_secs: f64,
    pub boot_secs: f64,
    pub handshake_ms_max: f64,
    pub enter_secs_max: f64,
    pub messages_received: u64,
    pub messages_sent: u64,
    pub datagrams_received: u64,
    pub datagrams_sent: u64,
    pub completed: BTreeMap<String, u64>,
    pub abandoned: BTreeMap<String, u64>,
    pub server_exit: Option<i32>,
    pub server_killed: bool,
    pub server_rss_peak: Option<u64>,
    pub server_log_lines: usize,
    pub server_errors: Vec<String>,
    pub server_panics: usize,
    pub violations: Vec<String>,
    pub markdown: String,
}

/// A client socket read by its own thread, blocked with no timeout (see empyrean-net's socket tests:
/// a Windows `recv_from` whose `SO_RCVTIMEO` fires can drop a datagram).
struct ClientSocket {
    socket: Arc<UdpSocket>,
    rx: Receiver<(SocketAddr, Vec<u8>)>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ClientSocket {
    fn bind() -> Self {
        let socket = Arc::new(
            UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
                .expect("a loopback client socket"),
        );
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let (s, st) = (Arc::clone(&socket), Arc::clone(&stop));
        let thread = std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            loop {
                let r = s.recv_from(&mut buf);
                if st.load(Ordering::SeqCst) {
                    return;
                }
                if let Ok((n, from)) = r {
                    if tx.send((from, buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
            }
        });
        Self {
            socket,
            rx,
            stop,
            thread: Some(thread),
        }
    }

    fn addr(&self) -> SocketAddr {
        self.socket.local_addr().expect("a local address")
    }
}

impl Drop for ClientSocket {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.socket.send_to(&[], self.addr());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A free port `P` on loopback with `P + 1` free too, in the high range.
fn free_port_pair(seed: u64) -> u16 {
    let mut p = 40_000 + u16::try_from(seed % 20_000).unwrap_or(0);
    for _ in 0..2000 {
        let lo = UdpSocket::bind((Ipv4Addr::LOCALHOST, p));
        let hi = UdpSocket::bind((Ipv4Addr::LOCALHOST, p + 1));
        if lo.is_ok() && hi.is_ok() {
            return p;
        }
        p = if p >= 64_000 { 40_000 } else { p + 2 };
    }
    panic!("no free loopback port pair");
}

/// The default binary: `empyrean-server` beside the test executable's `deps` directory.
#[must_use]
pub fn default_server_bin() -> PathBuf {
    if let Some(p) = std::env::var_os("EMPYREAN_SERVER_BIN") {
        return PathBuf::from(p);
    }
    let exe = std::env::current_exe().expect("the test executable");
    let profile_dir = exe
        .parent()
        .and_then(Path::parent)
        .expect("target/<profile>/deps");
    profile_dir.join(format!("empyrean-server{}", std::env::consts::EXE_SUFFIX))
}

/// The server process's resident memory (`tasklist` on Windows, `/proc` on Linux).
fn child_rss(pid: u32) -> Option<u64> {
    #[cfg(windows)]
    {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let field = text.trim().rsplit("\",\"").next()?.trim_end_matches('"');
        let kb: u64 = field
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()?;
        Some(kb * 1024)
    }
    #[cfg(target_os = "linux")]
    {
        let s = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        let kb: u64 = s
            .lines()
            .find(|l| l.starts_with("VmRSS:"))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()?;
        Some(kb * 1024)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = pid;
        None
    }
}

/// Starts the server; its stderr (the log) is copied to `server.log` and scanned.
fn start_server(
    cfg: &UdpConfig,
    port: u16,
    run_for: f64,
    log: Arc<Mutex<Vec<String>>>,
    listening: Arc<AtomicU64>,
) -> Child {
    std::fs::create_dir_all(&cfg.dir).expect("the scratch directory");
    for f in [
        "shard.db",
        "auth.db",
        "shard.db-wal",
        "shard.db-shm",
        "auth.db-wal",
        "auth.db-shm",
    ] {
        let _ = std::fs::remove_file(cfg.dir.join(f));
    }
    // The server reads no test variables: everything it needs is in its configuration.
    let mut config = empyrean_common::master_configuration::MasterConfiguration::default();
    config.server.world_name = "Soak".to_owned();
    config.server.network.host = "127.0.0.1".to_owned();
    config.server.network.port = u32::from(port);
    config.server.network.maximum_allowed_sessions = 128;
    config.server.dat_files_directory = dereth_dat::testing::dat_dir()
        .to_string_lossy()
        .into_owned();
    config.server.world_pack_path = empyrean_common::test_paths::world_pack()
        .to_string_lossy()
        .into_owned();
    config.server.interactive_console = false;
    config.database.shard_db_path = cfg.dir.join("shard.db").to_string_lossy().into_owned();
    config.database.auth_db_path = cfg.dir.join("auth.db").to_string_lossy().into_owned();
    let config_path = cfg.dir.join("empyrean.toml");
    std::fs::write(
        &config_path,
        empyrean_common::toml_config::to_toml_string(&config),
    )
    .expect("empyrean.toml");
    let mut child = Command::new(&cfg.server_bin)
        .args([
            "--config",
            &config_path.to_string_lossy(),
            "--run-for",
            &format!("{run_for:.0}"),
        ])
        .current_dir(&cfg.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("start {}: {e}", cfg.server_bin.display()));
    let stderr = child.stderr.take().expect("piped stderr");
    let log_path = cfg.dir.join("server.log");
    std::thread::spawn(move || {
        let mut file = std::fs::File::create(log_path).ok();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Some(f) = file.as_mut() {
                use std::io::Write as _;
                let _ = writeln!(f, "{line}");
            }
            if line.contains("Listening on") {
                listening.fetch_add(1, Ordering::SeqCst);
            }
            if let Ok(mut l) = log.lock() {
                l.push(line);
            }
        }
    });
    child
}

/// Runs the smoke. See the module docs.
///
/// # Panics
/// When the server binary cannot be started or never listens.
#[allow(clippy::too_many_lines)]
pub fn run(cfg: &UdpConfig) -> UdpReport {
    let wall0 = Instant::now();
    let port = free_port_pair(cfg.seed);
    let mut r = UdpReport {
        port,
        bots: cfg.bots,
        ..UdpReport::default()
    };
    let log = Arc::new(Mutex::new(Vec::new()));
    let listening = Arc::new(AtomicU64::new(0));
    // the bots' logins, the play, the log-offs, and slack; the smoke stops it itself before that
    let run_for = 60.0 + cfg.play_secs + 60.0;
    let mut child = start_server(cfg, port, run_for, Arc::clone(&log), Arc::clone(&listening));
    let pid = child.id();

    // wait for both sockets
    while listening.load(Ordering::SeqCst) < 2 {
        assert!(
            wall0.elapsed() < Duration::from_secs(60),
            "the server never listened (see {})",
            cfg.dir.join("server.log").display()
        );
        if let Ok(Some(status)) = child.try_wait() {
            panic!(
                "the server exited during boot: {status} (see {})",
                cfg.dir.join("server.log").display()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    r.boot_secs = wall0.elapsed().as_secs_f64();
    let server = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

    struct Udp {
        bot: Bot,
        io: BotIo,
        socket: ClientSocket,
        client: TestClient,
        inbox: Vec<IncomingMessage>,
        connected_at: Option<f64>,
        entered_at: Option<f64>,
    }
    let start = Instant::now();
    let now = || start.elapsed().as_secs_f64();
    let mut bots: Vec<Udp> = (0..cfg.bots)
        .map(|i| {
            let socket = ClientSocket::bind();
            let account = format!("udpsoak{i:03}");
            let client = TestClient::new(socket.addr(), server, &account, "pw");
            let mut bot = Bot::new(
                i,
                account,
                super::character_name(i),
                0,
                cfg.seed.wrapping_add(i as u64),
            );
            bot.allow_fight = false;
            bot.allow_relog = false;
            Udp {
                bot,
                io: BotIo::default(),
                socket,
                client,
                inbox: Vec::new(),
                connected_at: None,
                entered_at: None,
            }
        })
        .collect();

    let login_window = 60.0;
    let mut play_until: Option<f64> = None;
    let mut next_bot_step = 0.0;
    let mut next_rss = 0.0;
    let mut finishing = false;
    let mut finish_deadline = f64::MAX;
    loop {
        let t = now();
        for b in &mut bots {
            b.client.tick(t);
            for (to, bytes) in b.client.take_outgoing() {
                r.datagrams_sent += 1;
                let _ = b.socket.socket.send_to(&bytes, to);
            }
            while let Ok((from, bytes)) = b.socket.rx.try_recv() {
                r.datagrams_received += 1;
                b.client.handle_datagram(from, &bytes, t);
            }
            while let Some(m) = b.client.poll() {
                b.inbox.push(m);
            }
        }
        if t >= next_bot_step {
            next_bot_step = t + 0.1;
            for b in &mut bots {
                if b.bot.state == State::Connecting && b.client.status() == ClientStatus::Connected
                {
                    b.connected_at = Some(t);
                    b.bot.connected(&mut b.io);
                }
                let inbox = std::mem::take(&mut b.inbox);
                b.bot.receive(t, &inbox);
                b.bot.update(t, &mut b.io);
                if b.bot.state == State::InWorld && b.entered_at.is_none() {
                    b.entered_at = Some(t);
                }
                for (queue, bytes) in b.io.out.drain(..) {
                    b.client.send(queue, &bytes);
                }
                b.bot.events.clear();
            }
            if play_until.is_none()
                && (bots.iter().all(|b| b.entered_at.is_some()) || t > login_window)
            {
                play_until = Some(t + cfg.play_secs);
            }
            if !finishing && play_until.is_some_and(|p| t >= p) {
                finishing = true;
                finish_deadline = t + 30.0;
                for b in &mut bots {
                    b.bot.finish = true;
                }
            }
            if finishing && (bots.iter().all(|b| b.bot.state == State::Done) || t > finish_deadline)
            {
                break;
            }
        }
        if t >= next_rss {
            next_rss = t + 5.0;
            r.server_rss_peak = r.server_rss_peak.max(child_rss(pid));
        }
        if let Ok(Some(status)) = child.try_wait() {
            r.violations
                .push(format!("the server exited while the bots played: {status}"));
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }

    // disconnect every client, then let the server finish its --run-for and shut down
    for b in &mut bots {
        b.client.log_off();
        for (to, bytes) in b.client.take_outgoing() {
            let _ = b.socket.socket.send_to(&bytes, to);
        }
    }
    let give_up = start + Duration::from_secs_f64(run_for + 60.0);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < give_up => std::thread::sleep(Duration::from_millis(100)),
            _ => break None,
        }
    };
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        r.server_killed = true;
        r.violations
            .push("the server did not shut down by itself; killed".to_owned());
    }
    r.server_exit = status.and_then(|s| s.code());
    if r.server_exit.is_some_and(|c| c != 0) {
        r.violations
            .push(format!("the server exited with {:?}", r.server_exit));
    }
    std::thread::sleep(Duration::from_millis(200));

    // the numbers
    r.wall_secs = wall0.elapsed().as_secs_f64();
    for b in &bots {
        r.connected += usize::from(b.connected_at.is_some());
        r.entered += usize::from(b.entered_at.is_some());
        r.logged_off += usize::from(b.bot.state == State::Done && b.bot.stats.logins > 0);
        r.handshake_ms_max = r
            .handshake_ms_max
            .max(b.connected_at.unwrap_or(0.0) * 1000.0);
        if let (Some(c), Some(e)) = (b.connected_at, b.entered_at) {
            r.enter_secs_max = r.enter_secs_max.max(e - c);
        }
        r.messages_received += b.bot.stats.received_total;
        r.messages_sent += b.bot.stats.sent_total;
        for (k, v) in &b.bot.stats.completed {
            *r.completed.entry((*k).to_owned()).or_insert(0) += v;
        }
        for (k, v) in &b.bot.stats.abandoned {
            *r.abandoned.entry(k.clone()).or_insert(0) += v;
        }
    }
    let lines = log.lock().map(|l| l.clone()).unwrap_or_default();
    r.server_log_lines = lines.len();
    r.server_panics = lines
        .iter()
        .filter(|l| l.contains("panicked") || l.contains("threw an exception"))
        .count();
    r.server_errors = lines
        .iter()
        .filter(|l| l.contains("ERROR") || l.contains("panicked"))
        .take(20)
        .cloned()
        .collect();
    if r.connected < cfg.bots {
        r.violations.push(format!(
            "only {} of {} bots connected",
            r.connected, cfg.bots
        ));
    }
    if r.entered < cfg.bots {
        r.violations.push(format!(
            "only {} of {} bots entered the world",
            r.entered, cfg.bots
        ));
    }
    if r.logged_off < r.entered {
        r.violations.push(format!(
            "only {} of {} bots logged off cleanly",
            r.logged_off, r.entered
        ));
    }
    if r.server_panics > 0 {
        r.violations
            .push(format!("{} panics in the server log", r.server_panics));
    }

    let mut m = String::new();
    let _ = writeln!(
        m,
        "# UDP smoke: {} bots over loopback 127.0.0.1:{port}\n",
        cfg.bots
    );
    let _ = writeln!(
        m,
        "- result: **{}**",
        if r.violations.is_empty() {
            "passed"
        } else {
            "FAILED"
        }
    );
    for v in &r.violations {
        let _ = writeln!(m, "  - {v}");
    }
    let _ = writeln!(
        m,
        "- server boot {:.1} s; exit {:?}; killed {}; resident peak {} MB",
        r.boot_secs,
        r.server_exit,
        r.server_killed,
        metrics::mb(r.server_rss_peak)
    );
    let _ = writeln!(m, "- connected {}/{}, entered {}/{}, logged off {}; slowest handshake {:.0} ms after start, slowest enter {:.2} s", r.connected, cfg.bots, r.entered, cfg.bots, r.logged_off, r.handshake_ms_max, r.enter_secs_max);
    let _ = writeln!(
        m,
        "- messages received {}, sent {}; datagrams received {}, sent {}",
        r.messages_received, r.messages_sent, r.datagrams_received, r.datagrams_sent
    );
    let _ = writeln!(m, "- behaviours completed: {:?}", r.completed);
    let _ = writeln!(m, "- behaviours abandoned: {:?}", r.abandoned);
    let _ = writeln!(
        m,
        "- server log: {} lines, {} panics; errors: {:?}",
        r.server_log_lines, r.server_panics, r.server_errors
    );
    let _ = writeln!(m, "- wall {:.1} s", r.wall_secs);
    r.markdown = m;
    let _ = std::fs::write(cfg.dir.join("udp-smoke.md"), &r.markdown);
    r
}
